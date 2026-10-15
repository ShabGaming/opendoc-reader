# Contributing to OpenDoc Reader

## Welcome

Thanks for your interest in OpenDoc Reader. This guide explains how to take part. If something here is unclear or out of date, open an issue and say so.

## Project status

The project is pre-alpha. The plan is written, but there is no application code yet. The repository currently holds the license and documentation.

The most useful contributions right now are:

- Feedback on the plan. Read [docs/architecture.md](docs/architecture.md) and the records in [docs/adr/](docs/adr/), then tell us what looks wrong or missing.
- Sample scans for the test set (see [Test data rules](#test-data-rules)).
- Testing, once early builds exist.

## Ways to contribute

- **Bug reports.** Once there is something to run, open an issue with your OS version, the model tier you used, and the steps to reproduce. The app will have a "copy diagnostics" button in Settings. Paste its output into the issue.
- **Feature ideas.** Start a thread in GitHub Discussions, or open an issue if Discussions is not enabled yet. Check the roadmap in the [README](README.md) first.
- **Sample scans.** Public-domain or your own documents for the test set.
- **Code.** Rust, TypeScript and Python are all used. Smaller, focused changes are easier to review.
- **Documentation.** Fixes to the README, the architecture notes and the user guide.

## Development setup

The code does not exist yet, so there is nothing to build. Exact commands will be added here when the repository skeleton lands.

The planned toolchain is:

- Rust (stable) and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS. On Windows this includes the Microsoft C++ Build Tools and WebView2. On macOS it includes the Xcode command line tools.
- Node.js (LTS) and [pnpm](https://pnpm.io/) for the user interface.
- Python, managed through [uv](https://docs.astral.sh/uv/), for the engine.

The planned repository layout is:

```
apps/desktop/        Tauri app (src-tauri/ Rust, src/ React UI)
engine/              Python sidecar (uv project, pytest)
models/manifest.json models, runtime builds, checksums, suggester rules
docs/                user guide, architecture, ADRs, contributing a new engine
.github/             CI, release builds, issue/PR templates
```

## Workflow

We use the normal GitHub flow.

1. Fork the repository and create a branch from `main`.
2. Make your change. Keep the pull request focused on one thing.
3. Open a pull request against `main`. Describe what changed and why, and say how you tested it. For OCR changes, say which sample scans and which model tier you used.
4. Respond to review comments. A maintainer merges when the change is ready.

There is no CLA and no DCO sign-off. You keep the copyright on your contribution, and it is released under the project's [MIT license](LICENSE).

## Code style

These will be enforced in CI once the skeleton lands:

- Rust: `rustfmt` and `clippy`.
- TypeScript and React: ESLint and Prettier.
- Python: `ruff`.

Commit messages have no required format. Write a short summary line that says what changed.

## Adding an OCR engine

Every engine sits behind a common interface so that new ones can be added without touching the rest of the app. The queue, the exporters and the review screen know nothing about individual engines. This section describes the steps. The details will live in [docs/architecture.md](docs/architecture.md) and, later, in a dedicated guide under `docs/`.

Checklist:

- [ ] Open an issue first. Describe the engine, its license, its hardware needs, and what it does better than the existing tiers.
- [ ] Implement the engine interface described in [docs/architecture.md](docs/architecture.md). The engine takes a prepared page image and returns recognized content.
- [ ] Normalize the output into the shared document IR: pages, then blocks with a type (heading, paragraph, list, table, formula, figure, caption, header, footer and so on), a bounding box in page coordinates, a reading-order index, and the text. Tables carry cells with row and column spans. Formulas are stored as LaTeX. Figures are stored as crops from the page image. Do not write exporter code for your engine. All exporters render from the IR.
- [ ] Add the model entries to `models/manifest.json`: download sources, file names, sizes and a SHA-256 checksum for every file.
- [ ] Add suggester rules to the same data file so the setup screen can recommend the engine, or mark it as a poor fit, on suitable hardware. The rules are data, so no code change should be needed.
- [ ] Add golden-file tests. Run the engine on the sample scans and check in the expected IR output. Document how you produced it.
- [ ] Document the license of the model weights, and of any code the engine pulls in, in the manifest and in your pull request. The app shows model licenses to users before download.
- [ ] State the hardware you tested on and the speed and memory you measured. Mark numbers you did not measure as estimates.

CI has no GPU, so golden-file tests that run on every pull request use the Fast tier. Tests for larger models are run by hand before each release.

## Test data rules

The test set holds a small number of scans with expected outputs checked in alongside them. To keep it safe to publish:

- Use public-domain documents, or documents you created yourself and are willing to release under a permissive license. Say which in the pull request.
- Remove personal data. No names, addresses, account numbers, signatures or photos of real people unless they are fictional or clearly public-domain material.
- Do not submit pages from copyrighted books, papers or forms.
- Keep files small. A single page at a reasonable resolution is enough.

The set aims to cover a book page, a table-heavy invoice, a formula page, a two-column paper and a receipt.

## Reporting security issues

Do not report security problems in a public issue. A `SECURITY.md` file will be added with the repository templates. Until then, use GitHub's private vulnerability reporting on the Security tab of the repository.

## Code of conduct

We will follow the [Contributor Covenant](https://www.contributor-covenant.org/). A `CODE_OF_CONDUCT.md` file will be added with the repository templates. Until then, be considerate and keep discussion about the work.
