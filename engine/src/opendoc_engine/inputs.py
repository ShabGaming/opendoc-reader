"""Reading input files: PDFs through PDFium and images through Pillow.

Everything that can go wrong with a user's file is turned into a BAD_INPUT error with the
protocol's reason, so callers only deal with `RpcError`. Page images are returned as RGB
Pillow images; writing them (and the preprocessing in `preprocess.py`) is the caller's job.
"""

from __future__ import annotations

import threading
import unicodedata
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import Any

import numpy as np
import pypdfium2 as pdfium
import pypdfium2.raw as pdfium_raw
from PIL import Image, ImageOps, UnidentifiedImageError

from opendoc_engine.errors import bad_input, out_of_memory

# A page has a usable text layer from this many non-whitespace characters, and when at least
# this share of them are real characters (not replacement, private-use or control codes).
MIN_TEXT_CHARS = 20
MIN_PRINTABLE_SHARE = 0.9

_heif_lock = threading.Lock()
_heif_registered = False


def _register_heif() -> None:
    """Teach Pillow to open HEIC/HEIF files. Done on first use so that `engine.ping` and PDF
    work do not pay for the import."""
    global _heif_registered
    with _heif_lock:
        if not _heif_registered:
            import pi_heif

            pi_heif.register_heif_opener()
            _heif_registered = True


# --- PDF ------------------------------------------------------------------------------------


@contextmanager
def open_pdf(path: str) -> Iterator[pdfium.PdfDocument]:
    if not Path(path).is_file():
        raise bad_input("file_not_found", path)
    try:
        document = pdfium.PdfDocument(path)
    except pdfium.PdfiumError as exc:
        code = getattr(exc, "err_code", None)
        if code == pdfium_raw.FPDF_ERR_PASSWORD:
            raise bad_input("password_protected", path) from exc
        if code == pdfium_raw.FPDF_ERR_FILE:
            raise bad_input("unreadable", f"{path}: {exc}") from exc
        raise bad_input("corrupt", f"{path}: {exc}") from exc
    except OSError as exc:
        raise bad_input("unreadable", f"{path}: {exc}") from exc
    try:
        yield document
    finally:
        document.close()


def _page(document: pdfium.PdfDocument, page_index: int, path: str) -> pdfium.PdfPage:
    if not 0 <= page_index < len(document):
        raise bad_input("page_out_of_range", f"{path}: page {page_index} of {len(document)}")
    try:
        return document[page_index]
    except pdfium.PdfiumError as exc:
        raise bad_input("corrupt", f"{path}: page {page_index}: {exc}") from exc


def text_layer_stats(text: str) -> tuple[int, int]:
    """(non-whitespace characters, how many of those are real text) of a page's text."""
    total = good = 0
    for char in text:
        if char.isspace():
            continue
        total += 1
        if char != "�" and unicodedata.category(char) not in ("Co", "Cn", "Cc", "Cs"):
            good += 1
    return total, good


def has_text_layer(total: int, good: int) -> bool:
    return total >= MIN_TEXT_CHARS and good >= MIN_PRINTABLE_SHARE * total


def inspect_pdf(path: str) -> dict[str, Any]:
    with open_pdf(path) as document:
        pages = []
        for index in range(len(document)):
            page = _page(document, index, path)
            width, height = page.get_size()
            textpage = page.get_textpage()
            try:
                total, good = text_layer_stats(textpage.get_text_range())
            finally:
                textpage.close()
            page.close()
            pages.append(
                {
                    "index": index,
                    "width": round(width, 2),
                    "height": round(height, 2),
                    "hasTextLayer": has_text_layer(total, good),
                    "charCount": total,
                }
            )
    return {"pageCount": len(pages), "pages": pages}


def render_pdf_page(path: str, page_index: int, dpi: int) -> Image.Image:
    with open_pdf(path) as document:
        page = _page(document, page_index, path)
        try:
            bitmap = page.render(scale=dpi / 72.0, may_draw_forms=False)
            return bitmap.to_pil().convert("RGB")
        except MemoryError as exc:
            raise out_of_memory("ram", f"{path}: page {page_index} at {dpi} dpi") from exc
        except pdfium.PdfiumError as exc:
            raise bad_input("corrupt", f"{path}: page {page_index}: {exc}") from exc
        finally:
            page.close()


# --- images ---------------------------------------------------------------------------------


@contextmanager
def open_image(path: str) -> Iterator[Image.Image]:
    if not Path(path).is_file():
        raise bad_input("file_not_found", path)
    _register_heif()
    try:
        image = Image.open(path)
    except UnidentifiedImageError as exc:
        raise bad_input("unsupported_format", path) from exc
    except Image.DecompressionBombError as exc:
        raise bad_input("unreadable", f"{path}: {exc}") from exc
    except OSError as exc:
        raise bad_input("unreadable", f"{path}: {exc}") from exc
    try:
        yield image
    finally:
        image.close()


def _frame_count(image: Image.Image) -> int:
    return int(getattr(image, "n_frames", 1))


def inspect_image(path: str) -> dict[str, Any]:
    with open_image(path) as image:
        pages = []
        try:
            for index in range(_frame_count(image)):
                image.seek(index)
                width, height = image.size
                pages.append({"index": index, "width": width, "height": height})
        except (OSError, SyntaxError, EOFError, ValueError) as exc:
            raise bad_input("corrupt", f"{path}: {exc}") from exc
    return {"pageCount": len(pages), "pages": pages}


def _to_rgb(image: Image.Image) -> Image.Image:
    if image.mode in ("I;16", "I;16L", "I;16B", "I", "F"):
        data = np.asarray(image, dtype=np.float64)
        peak = data.max() if data.size else 0
        scale = 1.0 / 257.0 if peak > 255 else 1.0
        return Image.fromarray(np.clip(data * scale, 0, 255).astype(np.uint8), "L").convert("RGB")
    if image.mode == "P" and "transparency" in image.info:
        image = image.convert("RGBA")
    if image.mode in ("RGBA", "LA"):
        background = Image.new("RGB", image.size, (255, 255, 255))
        rgba = image.convert("RGBA")
        background.paste(rgba, mask=rgba.getchannel("A"))
        return background
    return image.convert("RGB")


def load_image_page(path: str, page_index: int) -> Image.Image:
    """One frame of an image file as RGB, with its EXIF orientation applied."""
    with open_image(path) as image:
        count = _frame_count(image)
        if not 0 <= page_index < count:
            raise bad_input("page_out_of_range", f"{path}: page {page_index} of {count}")
        try:
            image.seek(page_index)
            image.load()
            return _to_rgb(ImageOps.exif_transpose(image))
        except MemoryError as exc:
            raise out_of_memory("ram", path) from exc
        except (OSError, SyntaxError, EOFError, ValueError) as exc:
            raise bad_input("corrupt", f"{path}: {exc}") from exc


def is_pdf(path: str) -> bool:
    """Whether `path` is a PDF, judged by the file's first bytes (the extension can lie)."""
    try:
        with open(path, "rb") as handle:
            return b"%PDF-" in handle.read(1024)
    except OSError:
        return False


def render_source_page(path: str, page_index: int, dpi: int) -> tuple[Image.Image, int | None]:
    """A page of a PDF or image file as an RGB image, and the DPI used (None for images)."""
    if not Path(path).is_file():
        raise bad_input("file_not_found", path)
    if is_pdf(path):
        return render_pdf_page(path, page_index, dpi), dpi
    return load_image_page(path, page_index), None
