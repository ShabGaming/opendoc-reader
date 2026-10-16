# Architecture

> Status: pre-alpha. Planning is done and there is no code yet. This document describes the planned design and will change as v0.1 work settles the open questions listed at the end. The technical spike has already answered most of them. Decisions are recorded as ADRs in [docs/adr](adr/README.md).

## Overview

OpenDoc Reader is a desktop app for Windows and macOS that converts scanned documents (PDFs and images) into editable Markdown and Word files, with a searchable PDF export planned for v0.2. All recognition runs locally. The app is a Tauri 2 shell with a React UI. It supervises two helper processes: `llama-server` from llama.cpp, which runs the vision-language models, and a CPU-only Python engine, which handles preprocessing, the PaddleOCR pipeline pieces, normalization and export. Every engine's output is converted into one shared document model, and every exporter renders from that model. Models and runtime builds are not part of the installer. They are downloaded on first run from a manifest that pins every file by SHA-256. See [ADR 0001](adr/0001-local-only-no-telemetry.md) for the privacy constraints that shape all of this.

## Processes

Three processes cooperate at runtime.

```
+-------------------------------------------------------+
| 1. Tauri app                                          |
|    React UI (webview)  <->  Rust core                 |
|    Rust core owns: job queue, process supervision,    |
|    downloads, hardware probe, settings                |
+--------+----------------------------------+-----------+
         |                                  |
         | JSON-RPC over stdio              | HTTP, 127.0.0.1,
         | (no listening port)              | random port
         v                                  v
+--------------------+  HTTP, 127.0.0.1   +--------------------+
| 3. Python engine   | -----------------> | 2. llama-server    |
|    (sidecar, CPU)  |                    |    (llama.cpp)     |
|    uv-managed env  |                    |    GPU or CPU      |
+--------------------+                    +--------------------+
```

**Tauri app.** The UI is React and TypeScript, built with Vite, rendered in the OS webview (WebView2 on Windows, WKWebView on macOS). The Rust core owns the job queue, starts and stops the other two processes, runs downloads, probes the hardware, and stores settings. See [ADR 0002](adr/0002-tauri-react-typescript-desktop-app.md).

**llama-server.** An unmodified, pinned upstream llama.cpp release build, one per backend: CUDA for NVIDIA GPUs, Vulkan for AMD and Intel GPUs on Windows, Metal on Apple Silicon, and a CPU build. It loads one vision-language model at a time and listens on 127.0.0.1 on a random port. It is never reachable from other machines. See [ADR 0003](adr/0003-hybrid-llama-server-and-python-engine.md).

**Python engine.** A sidecar with a CPU-only environment created by `uv` on first run, using python-build-standalone. It renders PDFs, preprocesses pages, runs the PaddleOCR pipeline pieces (PP-DocLayoutV3 layout detection, PP-OCRv6, PP-StructureV3), normalizes results into the document IR, and runs all exporters. The Tauri core talks to it through JSON-RPC over stdio, so it opens no port of its own. For the Accurate tier, the engine uses PaddleOCR's llama-server VLM backend and sends element crops to `llama-server` over local HTTP. There is no CUDA PyTorch anywhere in the project.

The engine is the only client of `llama-server`, for both Accurate and Max. The Rust core starts and stops `llama-server` and passes its port to the engine, but never sends it requests. Keeping all model calls and output parsing in the engine means every engine presents the same interface to the Rust core.

## Conversion pipeline

Each file in the queue goes through the same steps. Pages are processed one at a time so progress can be reported and a job can resume from the last completed page.

1. **Input.** Accepted formats are PDF, PNG, JPG, TIFF (multi-page), BMP, WebP and HEIC. If a PDF has a real text layer, the app offers to use that text directly (instant and exact), with a "Run OCR anyway" option.
2. **Render and preprocess.** PDF pages are rendered at 200 DPI with PDFium (pypdfium2). Every page is auto-rotated and deskewed. Dewarping for curled phone photos is opt-in.
3. **Engine-specific recognition.** The selected tier decides what happens here:
   - Fast: PP-StructureV3 runs in the engine process with only layout detection and text OCR enabled (PP-OCRv6 tiny det/rec, detection limit 960, oneDNN off). Table and formula recognition are off, so tables and formulas are cropped and kept as images.
   - Accurate: PP-DocLayoutV3 detects layout regions in the engine, then PaddleOCR-VL recognizes each element through `llama-server`.
   - Max: DeepSeek-OCR v1 (or DeepSeek-OCR 2, if the user picked it as an experimental option) runs through `llama-server` and returns text with grounding information for blocks.
4. **Normalize to the IR.** The engine converts whatever the model produced into [document IR](#the-document-ir) blocks: types, page-coordinate bounding boxes, reading order, table cells, LaTeX formulas, and cropped figures. Normalization code is per engine. Everything after this step is engine-agnostic.
5. **Export.** The selected exporters render the IR to files next to the original (or in the chosen output folder). Existing files are never overwritten; a numeric suffix such as " (2)" is added instead. See [Exporters](#exporters).

The Review screen reads the same IR to show the original page next to the result.

## Model tiers

The UI shows friendly tier names. A details panel shows the real model names and licenses. Speeds in the table are measured on one desktop (RTX 4090, Ryzen 7 7800X3D) in the [model spike](spike/2026-10-model-spike.md); laptops will be slower.

| Tier | Models | Runs on | Intended use | Notes |
| --- | --- | --- | --- | --- |
| Fast | PP-OCRv6 tiny + PP-StructureV3 (PaddleOCR 3.7, Apache-2.0), layout and text only | Any CPU | Plain text and simple layouts. Tables and formulas are kept as images. | 4 to 11 s/page measured on a Ryzen 7 7800X3D; a laptop will be slower. Language dropdown, defaulting to the multilingual model. |
| Accurate | PaddleOCR-VL 1.6 (0.9B, Apache-2.0, released 2026-05-28) + PP-DocLayoutV3 | CPU (slow) or GPU / Apple Silicon (fast) | Tables, formulas, complex layouts | The default recommendation for most hardware. About 2.3 GB of VRAM. Scores higher than DeepSeek-OCR 2 on OmniDocBench in each team's published results. |
| Max | DeepSeek-OCR v1 (3B, MIT), Q8_0 from the official ggml-org GGUF repo | GPU with 8 GB+ VRAM, or Apple Silicon with 16 GB+ memory | Hardest pages | Needs about 5.7 GB of VRAM. "Max" means the biggest model, not necessarily the most accurate. Optional experimental model: DeepSeek-OCR 2 (Apache-2.0), see below. |

The model spike chose DeepSeek-OCR v1 for Max. DeepSeek-OCR 2 showed no quality advantage over v1, needs DRY sampling on the Q4_K_M quant, uses more VRAM and has no official GGUF. It stays available as an optional, experimental model under Max for advanced users (for example in an "Advanced models" section of Settings), from a community conversion pinned by revision and SHA-256 in the manifest. It ships as the Q8_0 quant (about 7.3 GB of VRAM, so it is only offered on GPUs with 10 GB or more and Macs with 16 GB or more) and runs with DRY sampling as a guard against repetition loops. Verifying that conversion, or publishing our own from the official weights, is a follow-up before v0.1. See [ADR 0004](adr/0004-three-model-tiers-common-engine-interface.md) and the [spike report](spike/2026-10-model-spike.md).

There is no language setting for the VLM tiers, because those models detect the language themselves. Handwriting is best-effort and not advertised.

## The document IR

Every engine's output is normalized into one document model. This is a draft sketch. The schema will be finalized in v0.1 and then versioned. See [ADR 0005](adr/0005-single-document-model-no-pandoc.md).

```ts
// Draft. Field names and shapes may change before v0.1.

interface Document {
  schemaVersion: string;
  source: { path: string; kind: "pdf" | "image"; pageCount: number };
  engine: { id: string; model: string; version: string };
  pages: Page[];
}

interface Page {
  index: number;                 // 0-based
  width: number;                 // pixels of the rendered page image
  height: number;
  sourceImage: string;           // path to the rendered/preprocessed page image
  blocks: Block[];               // stored in reading order
}

type BlockType =
  | "heading" | "paragraph" | "list" | "table" | "formula"
  | "figure" | "caption" | "header" | "footer" | "footnote"
  | "page_number" | "other";

interface Block {
  id: string;
  type: BlockType;
  bbox: [x1: number, y1: number, x2: number, y2: number]; // page pixel coordinates
  order: number;                 // reading-order index within the page
  level?: number;                // heading level
  text?: string;                 // inline markdown (emphasis, links, inline math)
  items?: ListItem[];            // list blocks, with nesting and ordered/unordered flag
  table?: Table;
  latex?: string;                // formula blocks
  figure?: { crop: string };     // path to image cropped from the page
  caption?: { for: string };     // block id of the figure or table this caption describes
  confidence?: number;
}

interface Table {
  rows: number;
  cols: number;
  cells: {
    row: number;
    col: number;
    rowSpan: number;
    colSpan: number;
    header?: boolean;
    blocks: Block[];             // cell content; usually one paragraph
  }[];
}
```

Open details to settle in v0.1: how multi-column pages and cross-page tables are represented, whether table cells hold text only or nested blocks, and how line-level boxes (needed for the searchable PDF) are stored. If line boxes need to be stored, they would be an optional `lines` field on text blocks.

## Exporters

All exporters read the IR and nothing else. Pandoc is deliberately not used: it drops raw HTML (both DeepSeek-OCR and PaddleOCR-VL emit HTML tables), its merged-cell support is weak, and it adds about 100 MB. PaddleOCR's own DOCX export is not used either, because it only covers Paddle engines. See [ADR 0005](adr/0005-single-document-model-no-pandoc.md).

| Format | Approach | Planned release |
| --- | --- | --- |
| DOCX | `python-docx`. LaTeX is converted to MathML and then OMML, giving native Word equations. Real Word tables with merged cells. Figure crops embedded. Page breaks between source pages. | v0.1 |
| Markdown | A `.md` file plus an assets folder (`invoice.md` and `invoice_assets/`). Tables are GFM where possible and HTML tables when cells are merged. Math is `$...$` and `$$...$$`. | v0.1 |
| Searchable PDF | The original page image with invisible text placed at block or line boxes. | v0.2 |
| HTML, TXT | Rendered from the IR. | v0.2 |
| XLSX (tables) | One sheet per table, or similar. Layout to be decided. | v0.3 |
| JSON | The IR itself, for developers. | v0.3 |

"1:1" in this project means structural fidelity: headings, paragraphs, reading order, lists, tables including merged cells, formulas, figures, and page breaks. Fonts and exact positions are approximated so that the output stays editable. Visual fidelity is covered by the searchable PDF. See [ADR 0006](adr/0006-structural-fidelity-and-searchable-pdf.md).

## Engine interface

Engines implement one common interface so that more can be added later (GLM-OCR and MinerU are candidates for v0.3). The interface is described here conceptually. The real signatures will be written when the engine package exists, along with a "contributing a new engine" guide in `docs/`. See [ADR 0004](adr/0004-three-model-tiers-common-engine-interface.md).

A new engine provides:

- **Capabilities.** What it can recognize: tables, formulas, figures, reading order, language handling (auto-detected or selectable), and whether it returns bounding boxes.
- **Requirements for the suggester.** Which runtime backends it can use (CUDA, Vulkan, Metal, CPU), approximate memory needs per quantization, disk size, and license. These are declared as data in the manifest so the [hardware suggester](#hardware-probe-and-suggester) can rate it without engine-specific code.
- **Model and runtime references.** Which manifest entries it needs downloaded.
- **`run(page) -> IR blocks`.** Takes a preprocessed page image and returns blocks in the IR, including bounding boxes in page coordinates, reading order, table structure, LaTeX for formulas, and figure crops. All model-specific parsing (for example DeepSeek-OCR's grounding output) lives inside the engine.
- **Lifecycle hooks.** Prepare (load or start whatever it needs) and release (free memory), so the queue can run one GPU job at a time.

An engine does not render PDFs, rotate pages, write files, or know about exporters. Those belong to the shared pipeline.

## Downloads and the manifest

The installer contains the app, the `uv` binary and the engine's source code (about 20 to 40 MB in total, an estimate until we have a build). On first run the app downloads the `llama-server` build for the detected backend, runs `uv` to fetch a python-build-standalone interpreter and the engine's locked dependencies, and downloads the chosen model files. The engine's dependencies are pinned with hashes in `engine/uv.lock`. See [ADR 0007](adr/0007-small-installer-first-run-downloads.md).

`models/manifest.json` is the single source for what can be downloaded. It holds:

- **Runtimes per backend.** The pinned `llama-server` release build for CUDA, Vulkan, Metal and CPU, with URL, SHA-256 and size per platform.
- **Model files.** For each model: file names, source URLs, mirror URLs (official Hugging Face repositories first, ModelScope as fallback), SHA-256, size, and license.
- **Suggester rules.** The thresholds and scoring used by the [hardware suggester](#hardware-probe-and-suggester). They live in the same file so that a model entry and the rules that recommend it change together in one pull request.

Illustrative excerpt. Field names, URLs and hashes here are placeholders, not real values:

```json
{
  "schemaVersion": 1,
  "runtimes": {
    "llama-server": {
      "release": "<pinned llama.cpp release tag>",
      "builds": {
        "windows-x64-cuda": {
          "url": "https://github.com/ggml-org/llama.cpp/releases/download/<tag>/<archive>",
          "sha256": "<sha256>",
          "sizeBytes": 0
        }
      }
    }
  },
  "models": {
    "deepseek-ocr-1-q8_0": {
      "tier": "max",
      "license": "MIT",
      "files": [
        {
          "name": "<model>.gguf",
          "urls": [
            "https://huggingface.co/<org>/<repo>/resolve/<rev>/<model>.gguf",
            "https://modelscope.cn/models/<org>/<repo>/resolve/<rev>/<model>.gguf"
          ],
          "sha256": "<sha256>",
          "sizeBytes": 0
        }
      ]
    }
  },
  "suggesterRules": [
    { "if": { "backend": "metal", "memoryGB": { "min": 16 } }, "recommend": "max" }
  ]
}
```

Download behavior:

- Every file is verified against its SHA-256 before use. A mismatch deletes the file and reports an error.
- Downloads are resumable and pausable, and they honor the system proxy.
- Storage is a user-chosen folder, since models are gigabytes. The default is inside the app data directory.
- Licenses are shown before a download starts.
- Settings has "Import model from folder" for restricted networks. Imported files are verified against the same manifest checksums.
- Models can be removed in Settings.

## Hardware probe and suggester

The Rust core probes: GPU vendor, model and VRAM; Apple unified memory; system RAM; CPU core count; and free disk space. The suggester applies the rules from the manifest to that data and produces one recommended model with a plain-language reason, for example "Your RTX 3060 (12 GB) runs Max at about 4 s per page". Other tiers get a fit badge: "Runs well", "Slow on this PC" or "Won't fit". Because the rules are data, contributors can tune them without touching code. After a model is downloaded, a one-page calibration benchmark refines the time estimates. See [ADR 0008](adr/0008-hardware-based-model-suggester.md).

The speed figure in the example above is illustrative. Measured numbers are in the [spike report](spike/2026-10-model-spike.md).

Rules the spike settled:

- Accurate (PaddleOCR-VL) is the default recommendation for most hardware.
- Max is recommended for GPUs with at least 8 GB of VRAM and for Apple Silicon Macs with at least 16 GB of memory.
- Do not assume Accurate is faster than Max on CPU. On a CPU-only machine with enough RAM, DeepSeek-OCR v1 measured faster than PaddleOCR-VL on CPU (17 to 41 s/page against 20 to 109 s/page on a Ryzen 7 7800X3D). This is one CPU, so recheck on laptop hardware.
- Apple Silicon speeds are estimates only, not measurements. See the estimates in the spike report.

### llama-server settings

Settings the engines depend on (details in section 7 of the spike report):

- DeepSeek-OCR needs `--special`, otherwise the grounding tokens are stripped from the output.
- PaddleOCR-VL must run with `-np 1 -c 16384`. The defaults use 11.5 GB of VRAM instead of 2.5 GB.
- Pin llama.cpp by its `bNNNNN` build tag in the manifest. The "latest" GitHub release has no binaries.
- Always pass `--port`. The default port changed between builds.

## Job queue and resume

The queue accepts many files and folders and runs in the background with progress, pause and cancel. Only one GPU job runs at a time. Queue state is persisted so that jobs resume after an app restart, from the last completed page. The Rust core owns the queue. The engine is stateless between `run` calls, which keeps resume logic in one place. Queue state is stored in a SQLite database in the app data directory, which gives atomic updates if the app is killed mid-job.

## Where data lives on disk

| What | Where |
| --- | --- |
| Settings, logs, runtime (`llama-server` builds, Python environment), queue state | The per-OS Tauri app data directory |
| Models | A user-chosen folder; default is inside the app data directory |
| Output files | Next to the original by default (`invoice.pdf` becomes `invoice.docx`; Markdown becomes `invoice.md` and `invoice_assets/`), or in an optional fixed output folder |

Logs are local only. Settings has a button to copy diagnostics so a user can paste them into a GitHub issue. Nothing is sent anywhere automatically.

## Platform support

- Windows 10 22H2 or later, x64. Uses WebView2.
- macOS 13 Ventura or later, Apple Silicon (M1 and newer) only. No Intel Macs.
- Windows ARM64 and Linux are future roadmap items.
- The app must remain useful on a laptop with no dedicated GPU and 8 to 16 GB of RAM, which is why the Fast tier exists and why Accurate can run on CPU.

Builds are unsigned before 1.0, with free ad-hoc signing on macOS. See [ADR 0009](adr/0009-unsigned-builds-before-1.0.md). The monthly update check only opens the GitHub release page and never installs anything. See [ADR 0010](adr/0010-monthly-update-check-no-auto-update.md). Repository layout and releases are covered in [ADR 0011](adr/0011-monorepo-and-tag-triggered-draft-releases.md), and test strategy in [ADR 0012](adr/0012-golden-file-tests-on-fast-tier.md).

## Open questions

The technical spike (step 2 of the work order) answered most of these. See the [spike report](spike/2026-10-model-spike.md).

1. **DeepSeek-OCR 2 or v1 for Max.** Answered: v1 ships in Max, and OCR 2 is an experimental advanced option (spike sections 6 and 8). ([ADR 0004](adr/0004-three-model-tiers-common-engine-interface.md))
2. **Real speeds and memory.** Answered for one desktop (RTX 4090 and Ryzen 7 7800X3D); no laptop CPU or 8 GB GPU was measured (spike section 3). ([ADR 0008](adr/0008-hardware-based-model-suggester.md))
3. **PP-DocLayoutV3 runtime.** Partly answered: it runs under `paddlepaddle` CPU at about 2.8 s per page. ONNX Runtime is untested (spike section 6). ([ADR 0003](adr/0003-hybrid-llama-server-and-python-engine.md))
4. **Vulkan coverage.** Still open. Whether the Vulkan backend supports the operations DeepSeek-OCR needs on AMD and Intel GPUs. No such hardware was available for the spike.
5. **DeepSeek-OCR grounding syntax.** Answered: confirmed, with integer coordinates 0 to 999 and `--special` required (spike section 6).
6. **Apple Silicon: estimated only.** No Apple Silicon machine was benchmarked. The estimates in the spike report are derived from memory bandwidth and will be replaced by measurements.
7. **ONNX Runtime / OpenVINO for Fast: deferred past v0.1.** The Fast tier runs on `paddlepaddle` CPU for now.
8. **Fast tier languages.** The spike used PP-OCRv6 tiny on English pages only. Which languages the tiny models cover, and whether the language dropdown needs larger models for some scripts, is still to be checked.
