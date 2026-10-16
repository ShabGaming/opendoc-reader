"""Build a models directory in the manifest layout from the spike machine's local files.

    uv run python scripts/link_dev_models.py [--spike DIR] [--out DIR]

The defaults are G:/opendoc-spike and G:/opendoc-dev/models. Each manifest file
`models.<id>.files[]` ends up at `<out>/<id>/[<dir>/]<name>`, the layout the downloader writes
and the engine reads (docs/protocol.md 1.3). Files are hardlinked from the spike folder when it
is on the same volume, copied otherwise, and always verified against the SHA-256 in
`models/manifest.json`. The spike folder is only read.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MANIFEST = REPO / "models" / "manifest.json"

# Where each manifest model's GGUF files sit in the spike folder (under `models/`).
SPIKE_FOLDERS = {
    "accurate-paddleocr-vl-1.6": "paddleocr-vl",
    "max-deepseek-ocr-1-q8": "deepseek-ocr-1",
    "experimental-deepseek-ocr-2-q8": "deepseek-ocr-2",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 22), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_source(spike: Path, model_id: str, entry: dict) -> Path | None:
    if "dir" in entry:
        candidate = spike / "paddle-cache" / "official_models" / entry["dir"] / entry["name"]
    else:
        folder = SPIKE_FOLDERS.get(model_id)
        if folder is None:
            return None
        candidate = spike / "models" / folder / entry["name"]
    return candidate if candidate.is_file() else None


def place(source: Path, target: Path) -> str:
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        os.link(source, target)
        return "linked"
    except OSError:
        shutil.copy2(source, target)
        return "copied"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spike", type=Path, default=Path("G:/opendoc-spike"))
    parser.add_argument("--out", type=Path, default=Path("G:/opendoc-dev/models"))
    args = parser.parse_args()

    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    failures = 0
    for model_id, model in manifest["models"].items():
        for entry in model["files"]:
            target = args.out / model_id / entry.get("dir", "") / entry["name"]
            label = f"{model_id}/{entry.get('dir', '')}/{entry['name']}".replace("//", "/")
            if target.is_file() and target.stat().st_size == entry["sizeBytes"]:
                if sha256(target) == entry["sha256"]:
                    print(f"ok      {label}")
                    continue
                target.unlink()
            source = find_source(args.spike, model_id, entry)
            if source is None:
                print(f"MISSING {label}: no copy under {args.spike}")
                failures += 1
                continue
            if sha256(source) != entry["sha256"]:
                print(f"BAD SHA {label}: {source}")
                failures += 1
                continue
            print(f"{place(source, target):7} {label}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
