# 0003. Hybrid runtime: llama-server plus a CPU-only Python engine

Status: Accepted

Date: 2026-10-15

## Context

We need to run two kinds of models. The vision-language models (PaddleOCR-VL, DeepSeek-OCR) are available as GGUF files and are supported by llama.cpp. The classic PaddleOCR pieces (PP-DocLayoutV3 layout detection, PP-OCRv6, PP-StructureV3) are Python and PaddlePaddle based. We also need a place for PDF rendering, page preprocessing, normalization into the document IR and all exporters, and the best libraries for those (pypdfium2, python-docx) are Python.

The app must work on NVIDIA GPUs, AMD and Intel GPUs on Windows, Apple Silicon and plain CPUs. Installing CUDA builds of PyTorch is about 2.5 GB and breaks easily across driver versions, which is a poor experience for non-technical users.

PaddleOCR 3.x officially supports llama-server as a VLM backend, so the Paddle pipeline can send element recognition to llama.cpp.

## Decision

The app runs three processes:

1. The Tauri app (UI and Rust core).
2. `llama-server` from llama.cpp, using a pinned upstream release build per backend: CUDA, Vulkan (AMD and Intel on Windows), Metal, and CPU. It runs the VLMs and listens on 127.0.0.1 on a random port.
3. A Python engine sidecar with a CPU-only environment created by `uv` on first run (python-build-standalone). It runs the PaddleOCR pipeline pieces, preprocessing, PDF rendering, normalization and exporters. The Rust core talks to it with JSON-RPC over stdio, so it opens no port.

The Accurate tier uses Paddle layout detection in the engine and PaddleOCR-VL element recognition through `llama-server`. There is no CUDA PyTorch anywhere in the project.

## Consequences

Good:

- GPU acceleration comes from llama.cpp's backends, which we do not build or patch. We get CUDA, Vulkan and Metal from upstream releases.
- The Python environment stays small and CPU-only, with no driver coupling.
- Models can be swapped in `llama-server` without changing the engine's Python dependencies.
- Python keeps the document-processing code (rendering, DOCX, PDF) in the language with the best libraries for it.

Bad:

- Three processes are more complex to supervise: startup order, crashes, port selection, shutdown, and cleanup of orphans on exit.
- First run has more to download and set up (a `llama-server` build, a Python environment, models), and more places to fail. The `uv` binary and the engine source ship inside the installer; `uv` then fetches the interpreter and locked dependencies.
- We depend on upstream llama.cpp for model support. Pinning a release protects us from regressions but delays fixes. For DeepSeek-OCR 2, an upstream multi-tile fix PR was closed unmerged (see ADR 0004).
- The Fast tier and layout detection still need PaddlePaddle on CPU, which is a sizeable dependency. Whether PP-DocLayoutV3 can run through ONNX Runtime instead is an open question for the spike.

## Alternatives considered

**llama.cpp only.** Simplest runtime. Rejected because it does not cover the Fast tier or PP-DocLayoutV3, and we would have to re-implement PDF rendering, preprocessing and normalization in Rust or C++ without the Python libraries we want.

**Python for everything with CUDA PyTorch.** One runtime and one language for inference. Rejected because of the roughly 2.5 GB, driver-sensitive install, no support for AMD or Intel GPUs on Windows without further work, and weaker Apple Silicon performance than Metal through llama.cpp.

**Python engine hosting the VLMs through a Python binding of llama.cpp.** Would reduce process count. Rejected because separate builds per backend are easier to ship as pinned upstream binaries than as wheels compiled for each combination of backend and platform.
