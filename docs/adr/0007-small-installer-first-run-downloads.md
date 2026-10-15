# 0007. Small installer with first-run downloads from a checksummed manifest

Status: Accepted

Date: 2026-10-15

## Context

The pieces the app needs are large. Models range from a few hundred megabytes to over 3 GB (DeepSeek-OCR 2 at Q4_K_M is about 2.5 GB including the multimodal projector). The right `llama-server` build depends on the user's GPU, and the Python environment is created for the user's platform. Most users need only some of these.

Bundling everything would produce an installer of many gigabytes, most of it unused on any given machine, and GitHub release assets have size limits. Users also differ in where they want to store models: a laptop with a small system drive may prefer an external or secondary drive.

Downloaded files are executed (`llama-server`) or loaded as model weights, so integrity matters. Users on restricted networks may not reach Hugging Face at all.

## Decision

The installer contains only the app, an estimated 20 to 40 MB. On first run the Setup screen downloads:

- the `llama-server` build for the detected backend,
- the Python engine environment (created by `uv`),
- the model or models the user chooses.

The list of files, their sources and their checksums lives in `models/manifest.json`. For each file it records the primary URL, mirror URLs (official Hugging Face repositories first, ModelScope as fallback), SHA-256, size and license. Every file is verified against its SHA-256 before use.

Downloads are resumable and pausable, and they honor the system proxy. The user picks the storage folder for models. Licenses are shown before download. Models can be removed in Settings. Settings also offers "Import model from folder", which verifies files against the same manifest checksums, for restricted networks.

## Consequences

Good:

- A small installer that downloads quickly and gives users an early look at the app.
- Users download only what their hardware needs.
- Pinned checksums mean a tampered or corrupted file is detected before it is run or loaded.
- Model updates and new engines can be added by editing the manifest, subject to review.
- Offline installs work through folder import.

Bad:

- The app is not usable until the first-run download finishes, which can take a long time on a slow connection.
- We depend on Hugging Face, ModelScope and GitHub being reachable, and on upstream files not moving or changing. A changed file fails verification and requires a manifest update.
- Maintainers must update checksums by hand when pinning new files. A mistake in the manifest breaks setup for everyone, so changes to it need careful review.
- Setup has several failure modes (disk space, proxy, partial downloads, antivirus quarantine of `llama-server`) that need clear error messages.
- The installer has to carry the `uv` binary and the engine source so the Python environment can be built from a lock file. This adds roughly 15 to 20 MB (estimate) but avoids a separate bootstrap download.

## Alternatives considered

**Bundle everything in the installer.** Simplest for the user once downloaded. Rejected because of the multi-gigabyte size, wasted bandwidth on unused backends, and release asset limits.

**Download models without checksums.** Less maintenance. Rejected because we run downloaded binaries and want to detect corruption.

**Use a project-hosted download server.** Better control and speed. Rejected because it would add infrastructure and cost for a free project, and it conflicts with the goal of relying on official sources.
