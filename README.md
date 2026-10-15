# OpenDoc Reader

OpenDoc Reader is a free, open source desktop app that turns scanned documents and photos of documents into editable Markdown and Word files.

> **Status: pre-alpha.** Planning is complete and the code has not been written yet. There is nothing to download. The first release will be v0.1.

## What it does

You give OpenDoc Reader a PDF or an image of a page. It reads the text, finds the headings, lists, tables, formulas and pictures, and saves the result as a file you can edit.

Everything runs on your computer. There is no account and no cloud service, and your documents are never uploaded. After the one-time setup, the app works offline.

Version 0.1 will include:

- First-run setup that checks your computer and recommends a model.
- Three model tiers (Fast, Accurate, Max) that you can install and remove.
- Input from PDF, PNG, JPG, TIFF (multi-page), BMP, WebP and HEIC files.
- Automatic page rotation and deskewing.
- A batch queue that handles many files or folders in the background, with progress, pause, cancel, and resume after a restart.
- Export to Markdown and Word (.docx).
- A side-by-side review screen (original page on the left, result on the right). In v0.1 it is read-only.

The goal is to preserve the document's structure: headings, paragraphs, reading order, lists, tables (including merged cells), formulas, figures and page breaks. Fonts and exact positions are approximated so that the output stays easy to edit. A searchable PDF export, planned for v0.2, covers cases where you need the exact look of the original.

## How it works

The first time you open the app, it looks at your computer and suggests a model that fits. You confirm the choice and the app downloads that model once. After that, you drop files into the window, pick an output format, and press Convert. When a file is done you can check it page by page next to the original, then export it. The result is saved next to the original file by default, for example `invoice.pdf` becomes `invoice.docx`. Existing files are never overwritten.

## Models

The app shows friendly tier names. A details panel shows the underlying model and its license. Hardware guidance below is an estimate until we have benchmark results.

| Tier | Underlying model | License | Best for | Hardware guidance (estimate) |
| --- | --- | --- | --- | --- |
| Fast | PP-OCRv6 + PP-StructureV3 (PaddleOCR 3.7) | Apache-2.0 | Plain text and simple layouts | Runs on any CPU. A few seconds per page. |
| Accurate | PaddleOCR-VL 1.6 (0.9B) with PP-DocLayoutV3 | Apache-2.0 | Tables, formulas, complex layouts | Slow on a CPU-only machine. Fast on a GPU or Apple Silicon. |
| Max | DeepSeek-OCR 2 (3B), or DeepSeek-OCR v1 (3B) as a fallback | Apache-2.0 (OCR 2), MIT (v1) | Difficult scans on stronger hardware | GPU or Apple Silicon with 16 GB or more of memory. |

"Max" means the largest model. It is not necessarily the most accurate one. In the results each team has published on OmniDocBench, PaddleOCR-VL scores higher than DeepSeek-OCR 2. We will publish our own comparison on the test set.

Which DeepSeek-OCR version ships in the Max tier is still undecided. A technical spike will compare the two before v0.1.

## System requirements

- Windows 10 version 22H2 or later, 64-bit.
- macOS 13 Ventura or later on Apple Silicon (M1 or newer). Intel Macs are not supported.
- 8 GB of RAM is the lowest we are designing for. 16 GB or more is recommended for the Max tier. These are estimates.
- Free disk space for models. As a rough figure, the Max tier model is about 2.5 GB (4-bit quantization) to 3 GB or more (8-bit), plus a few more components that setup downloads. Other sizes have not been measured yet. All of this is an estimate.
- A dedicated GPU is optional. The app is meant to be useful on a laptop without one.

Windows on ARM and Linux are on the roadmap but are not supported at launch.

## Install

Downloads will appear on the [Releases page](https://github.com/ShabGaming/opendoc-reader/releases). The installer will contain only the app itself (roughly 20 to 40 MB). Models and runtime components are downloaded during setup.

### Opening an unsigned build

Early releases will not be code signed, so your operating system will warn you the first time you open the app. Signed builds are planned before 1.0.

**Windows.** If SmartScreen says "Windows protected your PC", click "More info", then "Run anyway".

**macOS.** Try to open the app once. When macOS blocks it, open System Settings, go to Privacy & Security, scroll down, and click "Open Anyway" next to the OpenDoc Reader message.

## Privacy

OpenDoc Reader has no telemetry. It does not collect usage data, and your documents and results stay on your computer. If something goes wrong, the app keeps a local log that you can copy into a GitHub issue yourself.

Network access is limited to:

- Downloads during setup and when you install a model. Model files come from Hugging Face, with ModelScope as a fallback. The inference runtime comes from the llama.cpp releases on GitHub, and the engine's Python environment from the Python package index. Every file is checked against a known checksum. The app uses your system proxy settings.
- From v0.2, a check of GitHub Releases once a month to see whether a newer version exists. It is on by default and can be turned off in Settings. The request carries no personal data. If a newer version exists, the app opens the release page. It does not update itself.

On networks where downloads are blocked, Settings will let you import a model from a folder. The files are checked against known checksums.

## Roadmap

Plans can change as the project moves forward.

- [ ] **v0.1:** first-run setup, hardware check, model suggestion and downloads; all three tiers; Markdown and Word export; batch queue; read-only side-by-side review.
- [ ] **v0.2:** searchable PDF, HTML and plain text export; editing text in the review screen; PDF page ranges; monthly update check.
- [ ] **v0.3:** Excel (tables) and JSON export; more OCR engines (for example GLM-OCR and MinerU).
- [ ] **1.0:** code signing (Apple notarization, and SignPath Foundation or Azure Trusted Signing on Windows); polish.
- [ ] **Later:** translations (Simplified Chinese first), Linux builds, Windows on ARM, folder watching, in-app auto-update.

## Contributing

Contributions are welcome, including feedback on the plan. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to get involved, and [docs/architecture.md](docs/architecture.md) for how the app is designed. Design decisions are recorded in [docs/adr/](docs/adr/).

## Acknowledgements

OpenDoc Reader builds on the work of others:

- [DeepSeek-OCR](https://github.com/deepseek-ai/DeepSeek-OCR) by deepseek-ai.
- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) by PaddlePaddle.
- [llama.cpp](https://github.com/ggml-org/llama.cpp) by ggml-org.
- [Tauri](https://tauri.app).

Model weights are not part of this repository. They are downloaded separately and keep their own licenses, which the app shows before you download them.

## License

The OpenDoc Reader source code is released under the MIT License. See [LICENSE](LICENSE).
