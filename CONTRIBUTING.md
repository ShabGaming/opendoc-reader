# Contributing to OpenDoc Reader

## Welcome

Thanks for your interest in OpenDoc Reader. This guide explains how to take part. If something here is unclear or out of date, open an issue and say so.

## Project status

Version 0.1 is in testing. Every planned feature is built (setup, model downloads, the three OCR tiers, the queue, review and export), and the remaining work is end-to-end testing, a macOS pass and the release dry run. See [docs/plans/v0.1.md](docs/plans/v0.1.md).

The most useful contributions right now are:

- Feedback on the plan. Read [docs/architecture.md](docs/architecture.md) and the records in [docs/adr/](docs/adr/), then tell us what looks wrong or missing.
- Sample scans for the test set (see [Test data rules](#test-data-rules)).
- Testing, especially on macOS and on computers without an NVIDIA graphics card.

## Ways to contribute

- **Bug reports.** Open an issue with your OS version, the model tier you used, and the steps to reproduce. Settings, Privacy and logs has a "Copy diagnostics" button. Paste its output into the issue.
- **Feature ideas.** Start a thread in GitHub Discussions, or open an issue if Discussions is not enabled yet. Check the roadmap in the [README](README.md) first.
- **Sample scans.** Public-domain or your own documents for the test set.
- **Code.** Rust, TypeScript and Python are all used. Smaller, focused changes are easier to review.
- **Documentation.** Fixes to the README, the architecture notes and the user guide.

## Development setup

You need:

- Rust (stable, 1.90 or newer) and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS. On Windows this includes the Microsoft C++ Build Tools and WebView2. On macOS it includes the Xcode command line tools.
- Node.js 22 or newer and [pnpm](https://pnpm.io/) 10 (the exact version is pinned in `package.json`; `corepack enable` picks it up).
- [uv](https://docs.astral.sh/uv/) for the engine. It installs Python 3.12 for you.

Layout:

```
apps/desktop/        Tauri app (src-tauri/ Rust, src/ React UI)
engine/              Python engine (uv project, pytest)
models/              manifest.json (downloads, checksums, suggester rules) and its schema
docs/                architecture, ADRs, design reference, spike report
.github/             CI, release builds, issue and PR templates
```

Desktop app, from the repository root:

```bash
pnpm install
node scripts/fetch-uv.mjs
pnpm --filter desktop tauri dev
```

`scripts/fetch-uv.mjs` downloads the pinned `uv` release, checks its SHA-256 and puts it in `apps/desktop/src-tauri/binaries/`, where Tauri expects the bundled sidecar. The app uses it to build the engine's Python environment on first run. Run it once, and again whenever the pinned version changes. Pass `--target <triple>` to fetch for another platform.

`pnpm --filter desktop dev` runs only the web UI in a browser at http://localhost:1420, which is quicker for layout work.

Checks that CI runs for the desktop app (from the repository root):

```bash
pnpm --filter desktop lint
pnpm --filter desktop typecheck
pnpm --filter desktop test
pnpm --filter desktop build
```

and in `apps/desktop/src-tauri` (build the frontend first, since the Rust crate embeds it):

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`lint` also fails when `apps/desktop/src/types/ir.ts` is out of date with `engine/schema/ir.schema.json`. After changing the schema, run `pnpm --filter desktop gen:types` and commit the result. The engine, Rust and UI contracts are described in [docs/protocol.md](docs/protocol.md).

Engine, in `engine/`:

```bash
uv sync
uv run ruff check .
uv run ruff format --check .
uv run pytest
```

The engine speaks JSON-RPC over stdin and stdout. A quick check that it starts:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"engine.ping"}' | uv run python -m opendoc_engine
```

Tests that need real models are skipped unless `OPENDOC_INTEGRATION=1` is set, so a plain `pytest` and CI stay green without them.

- Engine, in `engine/`: `OPENDOC_INTEGRATION=1 uv run pytest tests/test_integration.py -s`. The tests read models in the manifest layout from `OPENDOC_MODELS_DIR` and the sample scans from `OPENDOC_SAMPLES_DIR`, and need a running llama-server for the Accurate and Max engines (`OPENDOC_LLAMA_URL`). `OPENDOC_INTEGRATION_ENGINES` picks the engines and `OPENDOC_MODEL_ID` the model; the header of the file lists every setting. The defaults point at the maintainer's folders, so set them on your machine.
- Rust, in `apps/desktop/src-tauri`: `OPENDOC_INTEGRATION=1 cargo test --test integration -- --nocapture`. This runs the real engine environment (it needs `uv` and network access for the managed Python) and, for the llama-server tests, a llama-server build and model files. `OPENDOC_UV` names the `uv` to use (the default is the sidecar in `binaries/`, then `uv` on the PATH), `OPENDOC_LLAMA_DIR` a llama-server build folder, and `OPENDOC_MODELS_DIR` a models folder laid out as `<modelsDir>/<modelId>/<file>`.

`engine/scripts/link_dev_models.py` builds such a models folder from a local copy of the spike files, and `scripts/fetch-models.py` fills one from the URLs in the manifest (`python scripts/fetch-models.py --tier fast --out DIR`, checked against the SHA-256 values).

### Golden tests

`engine/tests/test_golden.py` runs the Fast tier on seven public-domain pages and compares the result with the IR and Markdown checked in under `engine/tests/golden/expected/`. It compares structure with tolerances (block types and order, text similarity, box overlap), not bytes. It is opt-in locally and runs in CI as the `golden` job, which fetches the Fast tier models and caches them by the hash of the manifest:

```bash
cd engine
OPENDOC_GOLDEN=1 OPENDOC_MODELS_DIR=/path/to/models uv run pytest tests/test_golden.py
```

Where each page comes from, and what its expected output looks like, is in `engine/tests/golden/SOURCES.md`. After a change that should alter the output, regenerate the files with `OPENDOC_GOLDEN_UPDATE=1` added to the command above, then read the changed files and commit them in the same pull request. Do not edit them by hand. The tolerance checks themselves are tested by tests that always run.

### Third-party notices

`THIRD_PARTY_NOTICES.md` is generated. After changing dependencies or the model manifest, run, from the repository root:

```bash
python scripts/gen-notices.py
```

It reads `cargo metadata`, `pnpm licenses list --prod`, the engine's installed packages (run `uv sync` in `engine/` first) and `models/manifest.json`, and lists licenses that need a closer look in a section of their own. Commit the result.

The engine tests also validate `models/manifest.json` against `models/manifest.schema.json`, so run them after editing the manifest.

## Workflow

We use the normal GitHub flow.

1. Fork the repository and create a branch from `main`.
2. Make your change. Keep the pull request focused on one thing.
3. Open a pull request against `main`. Describe what changed and why, and say how you tested it. For OCR changes, say which sample scans and which model tier you used.
4. Respond to review comments. A maintainer merges when the change is ready.

There is no CLA and no DCO sign-off. You keep the copyright on your contribution, and it is released under the project's [MIT license](LICENSE).

## Code style

CI enforces these:

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
- [ ] Add golden-file tests. Run the engine on the golden pages (see [Golden tests](#golden-tests)) and check in the expected IR output. Document how you produced it.
- [ ] Document the license of the model weights, and of any code the engine pulls in, in the manifest and in your pull request. The app shows model licenses to users before download.
- [ ] State the hardware you tested on and the speed and memory you measured. Mark numbers you did not measure as estimates.

CI has no GPU, so golden-file tests that run on every pull request use the Fast tier. Tests for larger models are run by hand before each release.

## Test data rules

The test set holds a small number of scans with expected outputs checked in alongside them. To keep it safe to publish:

- Use public-domain documents, or documents you created yourself and are willing to release under a permissive license. Say which in the pull request.
- Remove personal data. No names, addresses, account numbers, signatures or photos of real people unless they are fictional or clearly public-domain material.
- Do not submit pages from copyrighted books, papers or forms.
- Keep files small. A single page at a reasonable resolution is enough.

The set in `engine/tests/golden/` covers a book page with an illustration, a table, a formula page, a two-column dictionary page, a recipe page with lists, and two US federal forms. New pages that add a layout the set does not have are welcome; keep the whole set small.

## Reporting security issues

Do not report security problems in a public issue. See [SECURITY.md](SECURITY.md) for how to report them privately.

## Code of conduct

This project follows the Contributor Covenant. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
