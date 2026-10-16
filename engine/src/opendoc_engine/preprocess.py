"""Page preprocessing for `page.render`: rotate, deskew, dewarp.

- Rotation by 90 degree steps is either explicit or detected with Paddle's page orientation
  model (`PP-LCNet_x1_0_doc_ori`). The model is optional: when no installed model folder holds
  it, `rotate: "auto"` leaves the page as it is.
- Deskew is classical (no model): the skew angle is the one that makes the rows of a binarized
  page most uneven, i.e. text lines horizontal. Pages without a clear peak are left alone.
- Dewarp needs Paddle's `UVDoc` model and is only run when the caller asks for it. Without the
  model the request fails with MODEL_NOT_READY instead of being ignored.

The Paddle models are loaded on first use and kept for the process.
"""

from __future__ import annotations

import logging
import threading
from pathlib import Path
from typing import Any

import cv2
import numpy as np
from PIL import Image

from opendoc_engine.errors import model_not_ready
from opendoc_engine.paddle_env import find_shared_model, prepare_paddle_env

log = logging.getLogger(__name__)

ORIENTATION_MODEL = "PP-LCNet_x1_0_doc_ori"
UNWARP_MODEL = "UVDoc"

# Skew search: whole degrees first, then tenths around the best one.
MAX_SKEW_DEGREES = 15.0
_SKEW_ANALYSIS_SIDE = 1000
# Below this the page counts as straight; the rotation would only blur it.
MIN_SKEW_DEGREES = 0.2
# The best angle must beat "no rotation" by this factor, else the page has no clear text lines.
MIN_SKEW_GAIN = 1.08
# Ink coverage outside this range means a blank page or a photograph: nothing to measure.
_INK_RANGE = (0.002, 0.45)

_models: dict[tuple[str, str], Any] = {}
_models_lock = threading.Lock()


def rotate_clockwise(image: Image.Image, degrees: int) -> Image.Image:
    degrees %= 360
    if degrees == 0:
        return image
    method = {
        90: Image.Transpose.ROTATE_270,
        180: Image.Transpose.ROTATE_180,
        270: Image.Transpose.ROTATE_90,
    }[degrees]
    return image.transpose(method)


def _paddle_model(name: str, directory: Path, scratch_dir: Path) -> Any:
    key = (name, str(directory))
    with _models_lock:
        model = _models.get(key)
        if model is None:
            prepare_paddle_env(scratch_dir)
            from paddlex import create_model

            model = create_model(model_name=name, model_dir=str(directory), device="cpu")
            _models[key] = model
        return model


def _bgr(image: Image.Image) -> np.ndarray:
    return np.ascontiguousarray(np.asarray(image.convert("RGB"))[:, :, ::-1])


def detect_rotation(image: Image.Image, models_dir: Path, scratch_dir: Path) -> int:
    """Clockwise degrees (0, 90, 180 or 270) that make the page upright, or 0 when the
    orientation model is not installed."""
    directory = find_shared_model(models_dir, ORIENTATION_MODEL)
    if directory is None:
        log.info("no %s model installed; page orientation is left as is", ORIENTATION_MODEL)
        return 0
    model = _paddle_model(ORIENTATION_MODEL, directory, scratch_dir)
    result = next(iter(model.predict(_bgr(image), batch_size=1)))
    # The label is how far the page is turned counterclockwise; undo it clockwise.
    return (360 - int(result["label_names"][0])) % 360


# --- deskew ---------------------------------------------------------------------------------


def _row_unevenness(binary: np.ndarray, angle: float) -> float:
    height, width = binary.shape
    matrix = cv2.getRotationMatrix2D((width / 2, height / 2), angle, 1.0)
    rotated = cv2.warpAffine(binary, matrix, (width, height), flags=cv2.INTER_NEAREST)
    rows = rotated.sum(axis=1, dtype=np.float64)
    return float(np.sum(np.diff(rows) ** 2))


def estimate_skew(image: Image.Image) -> float:
    """Degrees to rotate the page counterclockwise to straighten its text lines (0 when the
    page is straight or has no measurable lines)."""
    gray = np.asarray(image.convert("L"))
    scale = _SKEW_ANALYSIS_SIDE / max(gray.shape)
    if scale < 1:
        gray = cv2.resize(gray, None, fx=scale, fy=scale, interpolation=cv2.INTER_AREA)
    _, binary = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY_INV + cv2.THRESH_OTSU)
    margin = max(2, int(0.02 * min(binary.shape)))  # scanner borders are not text
    binary[:margin, :] = 0
    binary[-margin:, :] = 0
    binary[:, :margin] = 0
    binary[:, -margin:] = 0
    ink = float(np.count_nonzero(binary)) / binary.size
    if not _INK_RANGE[0] <= ink <= _INK_RANGE[1]:
        return 0.0

    def score(angle: float) -> float:
        return _row_unevenness(binary, angle)

    coarse = np.arange(-MAX_SKEW_DEGREES, MAX_SKEW_DEGREES + 0.5, 1.0)
    best = max(coarse, key=score)
    fine = np.arange(best - 1.0, best + 1.01, 0.1)
    best = max(fine, key=score)
    straight = score(0.0)
    if abs(best) < MIN_SKEW_DEGREES or straight <= 0 or score(best) < MIN_SKEW_GAIN * straight:
        return 0.0
    return round(float(best), 2)


def deskew(image: Image.Image) -> tuple[Image.Image, float]:
    """Straighten the page. Returns the image and the counterclockwise degrees applied."""
    angle = estimate_skew(image)
    if angle == 0.0:
        return image, 0.0
    array = np.asarray(image)
    height, width = array.shape[:2]
    matrix = cv2.getRotationMatrix2D((width / 2, height / 2), angle, 1.0)
    rotated = cv2.warpAffine(
        array,
        matrix,
        (width, height),
        flags=cv2.INTER_CUBIC,
        borderMode=cv2.BORDER_REPLICATE,
    )
    return Image.fromarray(rotated), angle


# --- dewarp ---------------------------------------------------------------------------------


def dewarp(image: Image.Image, models_dir: Path, scratch_dir: Path) -> Image.Image:
    directory = find_shared_model(models_dir, UNWARP_MODEL)
    if directory is None:
        raise model_not_ready("files_missing", f"{UNWARP_MODEL} (no installed model folder)")
    model = _paddle_model(UNWARP_MODEL, directory, scratch_dir)
    result = next(iter(model.predict(_bgr(image), batch_size=1)))
    return Image.fromarray(np.asarray(result["doctr_img"])[:, :, ::-1])


def release_models() -> None:
    with _models_lock:
        _models.clear()
