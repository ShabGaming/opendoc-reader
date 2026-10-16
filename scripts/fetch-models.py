"""Download model files listed in models/manifest.json into a folder, verifying SHA-256.

    python scripts/fetch-models.py --out DIR [--tier fast] [--model ID ...]

Uses only the standard library, so it runs before the engine environment exists. Each file
lands at `<out>/<model id>/[<dir>/]<name>`, the layout the app's downloader writes and the
engine reads. Files that are already there with the right checksum are kept, so the folder can
be cached between runs. URLs are tried in the manifest's order. The default tier is `fast`, the
only one CI can run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MANIFEST = REPO / "models" / "manifest.json"
ATTEMPTS = 3


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, target: Path, expected: str) -> None:
    part = target.with_name(target.name + ".part")
    request = urllib.request.Request(url, headers={"User-Agent": "opendoc-reader-ci"})
    digest = hashlib.sha256()
    with urllib.request.urlopen(request, timeout=60) as response, part.open("wb") as out:
        for chunk in iter(lambda: response.read(1 << 20), b""):
            out.write(chunk)
            digest.update(chunk)
    if digest.hexdigest() != expected:
        part.unlink(missing_ok=True)
        raise ValueError(f"checksum mismatch for {url}: got {digest.hexdigest()}")
    part.replace(target)


def fetch_file(label: str, entry: dict, target: Path) -> None:
    if target.is_file() and sha256_of(target) == entry["sha256"]:
        print(f"ok       {label}")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    errors: list[str] = []
    for url in entry["urls"]:
        for attempt in range(1, ATTEMPTS + 1):
            try:
                download(url, target, entry["sha256"])
                print(f"fetched  {label}")
                return
            except (urllib.error.URLError, OSError, ValueError) as exc:
                errors.append(f"{url} (attempt {attempt}): {exc}")
                time.sleep(2 * attempt)
    raise SystemExit(f"could not fetch {label}:\n  " + "\n  ".join(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True, help="models folder to fill")
    parser.add_argument("--tier", default="fast", help="model tier to fetch (default: fast)")
    parser.add_argument("--model", action="append", help="fetch this model id instead of a tier")
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    args = parser.parse_args()

    models = json.loads(args.manifest.read_text(encoding="utf-8"))["models"]
    if args.model:
        unknown = [m for m in args.model if m not in models]
        if unknown:
            raise SystemExit(f"not in the manifest: {', '.join(unknown)}")
        selected = {m: models[m] for m in args.model}
    else:
        selected = {m: v for m, v in models.items() if v["tier"] == args.tier}
    if not selected:
        raise SystemExit(f"no models for tier {args.tier!r}")

    for model_id, model in selected.items():
        for entry in model["files"]:
            parts = [model_id, *([entry["dir"]] if "dir" in entry else []), entry["name"]]
            fetch_file("/".join(parts), entry, args.out.joinpath(*parts))
    return 0


if __name__ == "__main__":
    sys.exit(main())
