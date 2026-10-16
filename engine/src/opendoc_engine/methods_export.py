"""RPC methods that read and write documents: `document.assemble`, `export.docx` and
`export.markdown` (see docs/protocol.md).

Every output is a file at a path the caller chose. The exports never overwrite: they fail with
`IO_ERROR` (reason `already_exists`) when a target exists, because the caller already picked a
free name. `document.assemble` replaces its output atomically, since Rust may assemble the same
job again after a resume.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

from opendoc_engine import rpc
from opendoc_engine.errors import bad_input, invalid_params
from opendoc_engine.exporters import files
from opendoc_engine.exporters.docx import docx_bytes
from opendoc_engine.exporters.markdown import figure_files, to_markdown
from opendoc_engine.ir import Document, EngineInfo, Page, Source
from opendoc_engine.params import Field, parse_params
from opendoc_engine.rpc import MethodRegistry, RpcError

log = logging.getLogger(__name__)


def _io_error(exc: files.OutputError) -> RpcError:
    return RpcError(rpc.IO_ERROR, "I/O error", {"reason": exc.reason, "detail": exc.detail})


def _path_param(params: dict[str, Any], name: str) -> Path:
    value = params[name]
    path = Path(value)
    if not value or not path.is_absolute():
        raise invalid_params("bad_type", name)
    return path


def _read_json(path: Path, what: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise bad_input("file_not_found", str(path)) from None
    except json.JSONDecodeError as exc:
        raise bad_input("corrupt", f"{what} {path}: {exc}") from exc
    except (OSError, UnicodeDecodeError) as exc:
        raise bad_input("unreadable", f"{what} {path}: {exc}") from exc


def _load_page(path: Path) -> Page:
    data = _read_json(path, "page file")
    try:
        return Page.from_dict(data)
    except (ValueError, TypeError, AttributeError, KeyError) as exc:
        raise bad_input("corrupt", f"page file {path}: {exc}") from exc


def _load_document(value: Any) -> Document:
    if isinstance(value, dict):
        data, where = value, "document"
    else:
        path = Path(value)
        if not path.is_absolute():
            raise invalid_params("bad_type", "document")
        data, where = _read_json(path, "document"), f"document {path}"
    try:
        return Document.from_dict(data)
    except (ValueError, TypeError, AttributeError, KeyError) as exc:
        raise bad_input("corrupt", f"{where}: {exc}") from exc


def register_export_methods(methods: MethodRegistry) -> None:
    @methods.method("document.assemble")
    def assemble(raw: Any) -> dict[str, Any]:
        params = parse_params(
            raw,
            {
                "source": Field("object"),
                "engine": Field("object"),
                "pages": Field("list"),
                "outPath": Field("path"),
            },
        )
        out_path = _path_param(params, "outPath")
        if not params["pages"]:
            raise invalid_params("bad_type", "pages")
        if not all(isinstance(p, str) and p for p in params["pages"]):
            raise invalid_params("bad_type", "pages")
        try:
            source = Source.from_dict(params["source"])
        except (ValueError, TypeError, AttributeError) as exc:
            raise invalid_params("bad_type", "source") from exc
        try:
            engine = EngineInfo.from_dict(params["engine"])
        except (ValueError, TypeError, AttributeError) as exc:
            raise invalid_params("bad_type", "engine") from exc

        pages = []
        for name in params["pages"]:
            path = Path(name)
            if not path.is_absolute():
                raise invalid_params("bad_type", "pages")
            pages.append(_load_page(path))
        indexes = [p.index for p in pages]
        if indexes != sorted(set(indexes)):
            raise bad_input("corrupt", f"pages are not in order: {indexes}")

        document = Document(source=source, engine=engine, pages=pages)
        payload = json.dumps(document.to_dict(), ensure_ascii=False).encode("utf-8")
        try:
            files.write_replacing(out_path, payload)
        except files.OutputError as exc:
            raise _io_error(exc) from exc
        return {"path": str(out_path), "pageCount": len(pages)}

    @methods.method("export.docx")
    def export_docx(raw: Any) -> dict[str, Any]:
        params = parse_params(
            raw, {"document": Field("path_or_object"), "outputPath": Field("path")}
        )
        out_path = _path_param(params, "outputPath")
        try:
            files.ensure_free(out_path)
            document = _load_document(params["document"])
            data, warnings = docx_bytes(document)
            files.write_new(out_path, data)
        except files.OutputError as exc:
            raise _io_error(exc) from exc
        return {"files": [str(out_path)], "warnings": warnings}

    @methods.method("export.markdown")
    def export_markdown(raw: Any) -> dict[str, Any]:
        params = parse_params(
            raw,
            {
                "document": Field("path_or_object"),
                "outputPath": Field("path"),
                "assetsDir": Field("path"),
            },
        )
        out_path = _path_param(params, "outputPath")
        assets_dir = _path_param(params, "assetsDir")
        try:
            files.ensure_free(out_path)
            document = _load_document(params["document"])
            return _write_markdown(document, out_path, assets_dir)
        except files.OutputError as exc:
            raise _io_error(exc) from exc


def _write_markdown(document: Document, out_path: Path, assets_dir: Path) -> dict[str, Any]:
    warnings: list[str] = []
    copies: list[tuple[Path, Path]] = []
    for block_id, (name, crop) in figure_files(document).items():
        if crop is None:
            warnings.append(
                f"figure {block_id} has no crop image; the link points to a missing file"
            )
        elif not Path(crop).is_file():
            warnings.append(f"figure {block_id}: crop image not found at {crop}")
        else:
            copies.append((Path(crop), assets_dir / name))
    for _, target in copies:
        files.ensure_free(target)

    text = to_markdown(document, assets_dir=assets_dir.name)
    created_dir = bool(copies) and not assets_dir.exists()
    written: list[Path] = []
    try:
        for source, target in copies:
            files.copy_new(source, target)
            written.append(target)
        files.write_new(out_path, text.encode("utf-8"))
    except BaseException:
        for target in written:
            target.unlink(missing_ok=True)
        if created_dir:
            try:
                assets_dir.rmdir()
            except OSError:
                log.warning("could not remove %s after a failed export", assets_dir)
        raise
    return {"files": [str(out_path), *map(str, written)], "warnings": warnings}
