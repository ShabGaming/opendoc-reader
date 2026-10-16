"""Generate THIRD_PARTY_NOTICES.md from the project's dependencies and the model manifest.

    python scripts/gen-notices.py [--engine-env engine/.venv] [--out THIRD_PARTY_NOTICES.md]

Needs, in the repository: `cargo`, `pnpm` with the workspace installed (`pnpm install`), and the
engine environment (`cd engine && uv sync --locked --no-dev` or a normal `uv sync`). Only the
standard library of Python 3.11 or newer is used.

What goes in:

- Rust crates: the normal (not build or dev) dependency graph of the desktop app, from
  `cargo metadata`. The graph is not limited to one platform, so Windows-only and Unix-only
  crates are both listed.
- npm packages: production dependencies of the desktop app (`pnpm licenses list --prod`).
- Python packages: the runtime dependencies of the engine, found through `engine/uv.lock` (the
  dev group is left out) and read from the `METADATA` of the installed packages.
- llama.cpp, the model weights and the other downloads, from `models/manifest.json`.

The output has no dates, so running the script twice on the same inputs gives the same file.
Licenses that are not on the short permissive list are collected in a section of their own.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tomllib
from collections import Counter
from email.parser import HeaderParser
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
TAURI = REPO / "apps" / "desktop" / "src-tauri"
ENGINE = REPO / "engine"
MANIFEST = REPO / "models" / "manifest.json"

# Licenses that ask only for the copyright and license text to travel with copies (or nothing
# at all). Anything else is listed in the "look closer" section.
PERMISSIVE = {
    "0BSD",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "BlueOak-1.0.0",
    "CC0-1.0",
    "HPND",
    "ISC",
    "MIT",
    "MIT-0",
    "NCSA",
    "Python-2.0",
    "PSF-2.0",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Unlicense",
    "Zlib",
    "Public domain",
    # Short permissive licenses that some packages name by their own identifiers.
    "MIT-CMU",
    "TCL",
    "bzip2-1.0.6",
    "CDLA-Permissive-2.0",
}

# Names that packages use instead of the SPDX identifier.
ALIASES = {
    "Apache License 2.0": "Apache-2.0",
    "Apache 2.0": "Apache-2.0",
    "Apache License, Version 2.0": "Apache-2.0",
    "3-Clause BSD License": "BSD-3-Clause",
    "BSD 3-Clause": "BSD-3-Clause",
    "MIT License": "MIT",
}

# Trove classifiers that carry a license, mapped to SPDX names.
CLASSIFIERS = {
    "MIT License": "MIT",
    "BSD License": "BSD-3-Clause",
    "Apache Software License": "Apache-2.0",
    "ISC License (ISCL)": "ISC",
    "Python Software Foundation License": "PSF-2.0",
    "Mozilla Public License 2.0 (MPL 2.0)": "MPL-2.0",
    "GNU Lesser General Public License v3 (LGPLv3)": "LGPL-3.0-only",
    "GNU Lesser General Public License v2 or later (LGPLv2+)": "LGPL-2.1-or-later",
    "GNU Lesser General Public License v3 or later (LGPLv3+)": "LGPL-3.0-or-later",
    "GNU General Public License v3 (GPLv3)": "GPL-3.0-only",
    "GNU General Public License v2 (GPLv2)": "GPL-2.0-only",
    "Historical Permission Notice and Disclaimer (HPND)": "HPND",
    "The Unlicense (Unlicense)": "Unlicense",
    "CC0 1.0 Universal (CC0 1.0) Public Domain Dedication": "CC0-1.0",
    "Public Domain": "Public domain",
}

# Phrases that identify a license text when a package gives no short name.
SNIFF = [
    ("Apache License", "Apache-2.0"),
    ("Permission is hereby granted, free of charge", "MIT"),
    ("Redistribution and use in source and binary forms", "BSD-3-Clause"),
    ("Permission to use, copy, modify, and/or distribute", "ISC"),
    ("Mozilla Public License", "MPL-2.0"),
    ("GNU LESSER GENERAL PUBLIC LICENSE", "LGPL"),
    ("GNU GENERAL PUBLIC LICENSE", "GPL"),
    ("This is free and unencumbered software", "Unlicense"),
]


# --- license expressions --------------------------------------------------------------------


def normalize_expression(text: str) -> str:
    """Old crates.io style `MIT/Apache-2.0` becomes `MIT OR Apache-2.0`."""
    text = ALIASES.get(text.strip(), text.strip())
    text = re.sub(r"\s+/\s+", " OR ", text)
    if re.fullmatch(r"[\w.+\-]+(/[\w.+\-]+)+", text):
        return " OR ".join(text.split("/"))
    return text


def is_permissive(expression: str) -> bool:
    """True when the SPDX expression can be met with permissive licenses only."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expression)
    position = 0

    def parse_or() -> bool:
        nonlocal position
        result = parse_and()
        while position < len(tokens) and tokens[position].upper() == "OR":
            position += 1
            result = parse_and() or result
        return result

    def parse_and() -> bool:
        nonlocal position
        result = parse_atom()
        while position < len(tokens) and tokens[position].upper() == "AND":
            position += 1
            result = parse_atom() and result
        return result

    def parse_atom() -> bool:
        nonlocal position
        if position >= len(tokens):
            return False
        token = tokens[position]
        position += 1
        if token == "(":
            result = parse_or()
            if position < len(tokens) and tokens[position] == ")":
                position += 1
            return result
        if position < len(tokens) and tokens[position].upper() == "WITH":
            position += 2  # the exception only widens what the license allows
        return token.rstrip("+") in PERMISSIVE

    try:
        return parse_or()
    except IndexError:
        return False


# --- Rust -----------------------------------------------------------------------------------


def rust_crates() -> list[dict]:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=TAURI,
        capture_output=True,
        text=True,
        check=True,
        encoding="utf-8",
    )
    metadata = json.loads(result.stdout)
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    seen: set[str] = set()
    pending = [root]
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    crates = []
    for package_id in seen - {root}:
        package = packages[package_id]
        license_text = package.get("license")
        if license_text:
            license_text = normalize_expression(license_text)
        elif package.get("license_file"):
            license_text = "See the crate's license file"
        else:
            license_text = "Unknown"
        name, version = package["name"], package["version"]
        crates.append(
            {
                "name": name,
                "version": version,
                "license": license_text,
                "url": package.get("repository")
                or package.get("homepage")
                or f"https://crates.io/crates/{name}/{version}",
            }
        )
    return crates


# --- npm ------------------------------------------------------------------------------------


def npm_packages() -> list[dict]:
    pnpm = shutil.which("pnpm")
    if pnpm is None:
        raise SystemExit("pnpm was not found on PATH")
    result = subprocess.run(
        [pnpm, "--filter", "desktop", "licenses", "list", "--prod", "--json"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
        encoding="utf-8",
    )
    packages = []
    for entries in json.loads(result.stdout).values():
        for entry in entries:
            for version in entry["versions"]:
                packages.append(
                    {
                        "name": entry["name"],
                        "version": version,
                        "license": normalize_expression(entry["license"]),
                        "url": entry.get("homepage")
                        or f"https://www.npmjs.com/package/{entry['name']}/v/{version}",
                    }
                )
    return packages


# --- Python ---------------------------------------------------------------------------------


def canonical(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def runtime_closure() -> dict[str, str]:
    """name -> version of everything the engine needs at run time, from engine/uv.lock."""
    lock = tomllib.loads((ENGINE / "uv.lock").read_text(encoding="utf-8"))
    by_name = {canonical(p["name"]): p for p in lock["package"]}
    root = next(p for p in lock["package"] if p["name"] == "opendoc-engine")
    found: dict[str, str] = {}
    pending = [canonical(d["name"]) for d in root.get("dependencies", [])]
    while pending:
        name = pending.pop()
        if name in found:
            continue
        package = by_name[name]
        found[name] = package["version"]
        pending.extend(canonical(d["name"]) for d in package.get("dependencies", []))
    return found


def site_packages(env: Path) -> Path:
    windows = env / "Lib" / "site-packages"
    if windows.is_dir():
        return windows
    matches = sorted(env.glob("lib/python3*/site-packages"))
    if not matches:
        raise SystemExit(f"no site-packages found in {env}; run `uv sync` in engine/ first")
    return matches[0]


def sniff(text: str) -> str | None:
    for phrase, name in SNIFF:
        if phrase.lower() in text.lower():
            return name
    return None


def python_license(dist_info: Path) -> tuple[str, str | None]:
    """The license name and a home page URL of an installed package."""
    metadata = HeaderParser().parsestr((dist_info / "METADATA").read_text(encoding="utf-8"))
    url = metadata.get("Home-page")
    if not url or url == "UNKNOWN":
        for entry in metadata.get_all("Project-URL") or []:
            label, _, value = entry.partition(",")
            if label.strip().lower() in {"homepage", "home", "source", "repository", "github"}:
                url = value.strip()
                break
    expression = metadata.get("License-Expression")
    if expression:
        return normalize_expression(expression), url
    names = []
    for classifier in metadata.get_all("Classifier") or []:
        if classifier.startswith("License :: "):
            last = classifier.rsplit(" :: ", 1)[-1]
            if last in CLASSIFIERS:
                names.append(CLASSIFIERS[last])
    if names:
        return " OR ".join(dict.fromkeys(names)), url
    field = (metadata.get("License") or "").strip()
    if field and field != "UNKNOWN":
        if "\n" not in field and len(field) <= 60:
            return normalize_expression(field), url
        guess = sniff(field)
        if guess:
            return guess, url
    for candidate in sorted(dist_info.glob("licen[cs]e*")) + sorted(
        dist_info.glob("licenses/**/*")
    ):
        if candidate.is_file():
            guess = sniff(candidate.read_text(encoding="utf-8", errors="replace")[:4000])
            if guess:
                return guess, url
    return "Unknown", url


def python_packages(env: Path) -> list[dict]:
    installed = {}
    for dist_info in site_packages(env).glob("*.dist-info"):
        if (dist_info / "METADATA").is_file():
            stem = dist_info.name.removesuffix(".dist-info")
            installed[canonical(stem.rsplit("-", 1)[0])] = dist_info
    packages = []
    for name, version in sorted(runtime_closure().items()):
        dist_info = installed.get(name)
        if dist_info is None:
            license_text, url = "Unknown (not installed in this environment)", None
        else:
            license_text, url = python_license(dist_info)
        packages.append(
            {
                "name": name,
                "version": version,
                "license": license_text,
                "url": url or f"https://pypi.org/project/{name}/{version}/",
            }
        )
    return packages


# --- manifest -------------------------------------------------------------------------------


def uv_version() -> str | None:
    match = re.search(r'const VERSION = "([^"]+)"', (REPO / "scripts" / "fetch-uv.mjs").read_text())
    return match.group(1) if match else None


# --- output ---------------------------------------------------------------------------------


def cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def table(rows: list[dict]) -> list[str]:
    lines = ["| Name | Version | License | Link |", "| --- | --- | --- | --- |"]
    for row in sorted(rows, key=lambda r: (r["name"].lower(), r["version"])):
        lines.append(
            f"| {cell(row['name'])} | {cell(row['version'])} | {cell(row['license'])} "
            f"| [{cell(re.sub(r'^https?://', '', row['url']).rstrip('/'))}]({row['url']}) |"
        )
    return lines


def model_rows(manifest: dict) -> list[str]:
    lines = [
        "| Model | Tier | License | Source |",
        "| --- | --- | --- | --- |",
    ]
    for model in manifest["models"].values():
        sources = ", ".join(f"[{s['repo']}]({s['url']})" for s in model["sources"])
        name = model["displayName"]
        lines.append(
            f"| {cell(name)} | {model['tier']} | {cell(model['license'])} | {cell(sources)} |"
        )
    return lines


def build(env: Path) -> str:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    llama = manifest["runtimes"]["llamaServer"]
    groups = {
        "Python packages (the engine)": python_packages(env),
        "Rust crates (the app)": rust_crates(),
        "npm packages (the user interface)": npm_packages(),
    }

    out = [
        "# Third-party notices",
        "",
        "OpenDoc Reader is released under the [MIT License](LICENSE). It is built on the "
        "software and models listed here, each under its own license. This file is generated "
        "by `scripts/gen-notices.py`; do not edit it by hand.",
        "",
        "Most of these licenses (MIT, Apache-2.0, BSD and similar) ask that the copyright "
        "notice and license text stay with copies of the software. The full texts are in "
        "each project's repository, linked below, and in the packages themselves. Licenses "
        "with other conditions are collected under [Licenses to look at "
        "closely](#licenses-to-look-at-closely).",
        "",
        "## Models and runtime downloaded during setup",
        "",
        "None of these are part of the installer or this repository. The app downloads them "
        "when you choose a model, checks them against the checksums in `models/manifest.json`, "
        "and shows each model's license before the download.",
        "",
        f"- **llama.cpp** (`llama-server`, build {llama['build']}), license {llama['license']}: "
        f"[{llama['releaseUrl'].removeprefix('https://')}]({llama['releaseUrl']}). It runs "
        "the Accurate and Max models.",
        "- **NVIDIA CUDA runtime libraries** (`cudart`, from the same llama.cpp release), "
        "downloaded only on computers with an NVIDIA GPU. They are licensed by NVIDIA under the "
        "[CUDA Toolkit license](https://docs.nvidia.com/cuda/eula/).",
        "- **Model weights**, which keep their own licenses:",
        "",
        *model_rows(manifest),
        "",
        "The engine's Python environment is created at first run with "
        f"[uv](https://github.com/astral-sh/uv) {uv_version() or ''} (MIT OR Apache-2.0), "
        "which ships with the app, and installs the Python packages listed below from the "
        "Python package index. uv also downloads a Python 3.12 interpreter "
        "([PSF-2.0](https://docs.python.org/3/license.html)).",
        "",
    ]

    notable: list[tuple[str, dict]] = []
    for title, rows in groups.items():
        notable += [(title, r) for r in rows if not is_permissive(r["license"])]
    out += ["## Licenses to look at closely", ""]
    if notable:
        out += [
            "These packages are under licenses that are not in the plain permissive set "
            "(MIT, Apache-2.0, BSD, ISC, Zlib and similar), or the license could not be read "
            "from the package.",
            "",
            "| Package | Version | License | Part | Link |",
            "| --- | --- | --- | --- | --- |",
        ]
        for title, row in sorted(notable, key=lambda n: (n[1]["license"], n[1]["name"].lower())):
            out.append(
                f"| {cell(row['name'])} | {cell(row['version'])} | {cell(row['license'])} "
                f"| {title.split(' (')[0]} "
                f"| [{cell(re.sub(r'^https?://', '', row['url']).rstrip('/'))}]({row['url']}) |"
            )
    else:
        out.append("None.")
    out.append("")

    out += ["## License summary", "", "| License | Packages |", "| --- | --- |"]
    counts = Counter(r["license"] for rows in groups.values() for r in rows)
    for name, count in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
        out.append(f"| {cell(name)} | {count} |")
    out.append("")

    notes = {
        "Python packages (the engine)": (
            "The runtime dependencies of `engine/`, read from `engine/uv.lock` and the "
            "installed packages' metadata. PaddlePaddle and some other wheels contain compiled "
            "libraries with licenses of their own; see the license files inside those wheels."
        ),
        "Rust crates (the app)": (
            "The normal dependencies of the Tauri app on any platform, from `cargo metadata`. "
            "Build-time and test-only crates are not included because they are not shipped."
        ),
        "npm packages (the user interface)": (
            "The production dependencies of `apps/desktop`, from `pnpm licenses list --prod`."
        ),
    }
    for title, rows in groups.items():
        out += [f"## {title}", "", notes[title], "", *table(rows), ""]
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--engine-env", type=Path, default=ENGINE / ".venv")
    parser.add_argument("--out", type=Path, default=REPO / "THIRD_PARTY_NOTICES.md")
    args = parser.parse_args()
    text = build(args.engine_env)
    args.out.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
