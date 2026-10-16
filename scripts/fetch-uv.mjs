#!/usr/bin/env node
// Downloads the pinned uv release for a target triple and places it where Tauri expects an
// external binary: apps/desktop/src-tauri/binaries/uv-<triple>[.exe]. The archive is checked
// against a SHA-256 pinned below, so a changed or tampered download fails the build.
//
//   node scripts/fetch-uv.mjs [--target <triple>] [--force]
//
// The target comes from --target, then TAURI_ENV_TARGET_TRIPLE (set by the Tauri CLI), then
// the host triple reported by rustc.
//
// To bump uv: change VERSION, then copy each archive's digest from the release page or from
// `gh api repos/astral-sh/uv/releases/tags/<version> --jq '.assets[] | [.name, .digest]'`.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "0.12.16";

const ARCHIVES = {
  "x86_64-pc-windows-msvc": {
    name: "uv-x86_64-pc-windows-msvc.zip",
    sha256: "f730454bf09019754e5e5abd71a8aa18683cb739cba0d9c720bac2e7c901160f",
  },
  "aarch64-pc-windows-msvc": {
    name: "uv-aarch64-pc-windows-msvc.zip",
    sha256: "9977129f89c4036edfcb200d2484755571e51fa74517f02e478c1d7bcc353b2e",
  },
  "aarch64-apple-darwin": {
    name: "uv-aarch64-apple-darwin.tar.gz",
    sha256: "b6e03fae61704b1aa622f12b792a69483e837b83068e44f4fd34f8a07a8f74a3",
  },
  "x86_64-apple-darwin": {
    name: "uv-x86_64-apple-darwin.tar.gz",
    sha256: "a42bcc9ce97eb8b364d7f162233a9c6b8c0ee25388e551d362809795127e0c31",
  },
};

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(repoRoot, "apps", "desktop", "src-tauri", "binaries");

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const match = /^host: (.+)$/m.exec(out);
  if (!match) throw new Error("could not read the host triple from `rustc -vV`");
  return match[1].trim();
}

function parseArgs(argv) {
  const args = { target: undefined, force: false };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--target") args.target = argv[++i];
    else if (argv[i] === "--force") args.force = true;
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return args;
}

function extract(archive, kind, into) {
  if (kind === "zip") {
    // Expand-Archive is always there on Windows; a bare `tar` may resolve to GNU tar, which
    // cannot read zip files.
    const quote = (p) => `'${p.replaceAll("'", "''")}'`;
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Expand-Archive -LiteralPath ${quote(archive)} -DestinationPath ${quote(into)} -Force`,
      ],
      { stdio: "inherit" },
    );
  } else {
    execFileSync("tar", ["-xzf", archive, "-C", into], { stdio: "inherit" });
  }
}

function findFile(dir, fileName) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      const found = findFile(full, fileName);
      if (found) return found;
    } else if (entry === fileName) {
      return full;
    }
  }
  return null;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const triple = args.target || process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
  const archive = ARCHIVES[triple];
  if (!archive) {
    throw new Error(
      `no pinned uv archive for ${triple}; supported: ${Object.keys(ARCHIVES).join(", ")}`,
    );
  }

  const isWindows = triple.includes("windows");
  const binaryName = isWindows ? "uv.exe" : "uv";
  const destination = join(outDir, `uv-${triple}${isWindows ? ".exe" : ""}`);
  const stampPath = join(outDir, `.uv-${triple}.stamp`);
  const stamp = `${VERSION} ${archive.sha256}`;

  if (
    !args.force &&
    existsSync(destination) &&
    statSync(destination).size > 0 &&
    existsSync(stampPath) &&
    readFileSync(stampPath, "utf8").trim() === stamp
  ) {
    console.log(`uv ${VERSION} for ${triple} is already in place`);
    return;
  }

  const url = `https://github.com/astral-sh/uv/releases/download/${VERSION}/${archive.name}`;
  console.log(`downloading ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) throw new Error(`download failed: HTTP ${response.status} for ${url}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== archive.sha256) {
    throw new Error(
      `checksum mismatch for ${archive.name}: expected ${archive.sha256}, got ${digest}`,
    );
  }

  const work = mkdtempSync(join(tmpdir(), "fetch-uv-"));
  try {
    const archivePath = join(work, archive.name);
    writeFileSync(archivePath, bytes);
    const unpacked = join(work, "unpacked");
    mkdirSync(unpacked);
    extract(archivePath, isWindows ? "zip" : "tar", unpacked);
    const binary = findFile(unpacked, binaryName);
    if (!binary) throw new Error(`${binaryName} not found in ${archive.name}`);
    mkdirSync(outDir, { recursive: true });
    copyFileSync(binary, destination);
    if (!isWindows) chmodSync(destination, 0o755);
    writeFileSync(stampPath, `${stamp}\n`);
    console.log(`uv ${VERSION} for ${triple} written to ${destination}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`fetch-uv: ${error.message}`);
  process.exit(1);
});
