# 0011. Monorepo layout and tag-triggered draft releases

Status: Accepted

Date: 2026-10-15

## Context

The project has several parts that must change together: the Tauri app (Rust and React), the Python engine, the manifest that pins models and runtime builds, and the documentation. A change to the engine interface or the document IR touches the engine, the Rust core that calls it, and the docs. The manifest format is read by the Rust core and by the suggester.

The maintainer works alone for now and tests releases by hand on one Windows PC and one Apple Silicon Mac. Releases must be reviewable before they go public, especially while builds are unsigned (ADR 0009).

## Decision

One repository, laid out as:

```
apps/desktop/        Tauri app (src-tauri/ Rust, src/ React UI)
engine/              Python sidecar (uv project, pytest)
models/manifest.json models, runtime builds, checksums, suggester rules
docs/                user guide, architecture, ADRs, contributing a new engine
.github/             CI, release builds, issue/PR templates
```

The project follows SemVer starting at 0.1.0. Pushing a tag matching `v*` runs a GitHub Actions workflow that builds the Windows .msi and .exe installers and the macOS .dmg and attaches them to a draft release. The maintainer checks the draft, tests the builds, and publishes it by hand. Release notes use GitHub's auto-generated notes.

Contribution uses the normal GitHub flow: fork, branch, pull request. There is no DCO and no CLA. There is no Conventional Commits requirement and no release-please.

## Consequences

Good:

- A single pull request can change the engine, the app and the docs together, and CI sees a consistent state.
- One place for issues, one release stream, and one version number.
- Draft releases give a review point before anything is public, which matters for unsigned builds.
- Tag-triggered builds are reproducible from the repository and need no special release machine.
- Few process requirements keep the barrier to contribution low.

Bad:

- Contributors who touch only the engine still have a repository that includes a Rust and Node project. CI must be configured to run only relevant jobs to keep feedback quick.
- Issues about the app, the engine and the models all land in one tracker and need labels to stay manageable.
- Auto-generated release notes depend on good pull request titles. Without Conventional Commits, there is no enforced structure, and notes can be uneven.
- Draft release publishing is manual, so a release can be delayed if the maintainer is unavailable.
- CI needs both macOS and Windows runners for release builds, which are slower and use more minutes than Linux runners.
- Without a CLA, relicensing in the future would require agreement from every contributor.

## Alternatives considered

**Separate repositories for app and engine.** Cleaner boundaries and independent versioning. Rejected because the interface between them changes often in early development, and two repositories would force coordinated changes and two release streams.

**Fully automated releases (publish on tag).** Faster. Rejected because the maintainer wants to test the installers before they are public.

**release-please and Conventional Commits.** Automates versioning and changelogs. Rejected because it adds a contribution rule for a project that wants a low barrier to entry, and the project is small enough to handle versioning by hand.

**DCO or CLA.** Rejected to keep contribution simple. The MIT license is enough for the project's needs.
