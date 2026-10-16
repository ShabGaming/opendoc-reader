# 0004. Three model tiers behind a common engine interface

Status: Accepted

Date: 2026-10-15

Updated: 2026-10-15 after the model spike

## Context

Users range from a person with an 8 GB laptop and no GPU to a person with an RTX 4090 or an Apple Silicon Mac with 32 GB. One model cannot serve both. A 3B vision-language model is too slow on a CPU for batch work, while a classic OCR pipeline is wasteful on a strong GPU and weaker on tables and formulas.

Non-technical users should not have to choose between model names and quantizations. At the same time, contributors and curious users want to see which model is actually running and under what license.

New document OCR models appear frequently. We expect to add GLM-OCR and MinerU, and we do not want each addition to require changes across the app.

## Decision

The app offers three tiers. The UI shows friendly names. A details panel shows the real model names and licenses.

- **Fast:** PP-OCRv6 + PP-StructureV3 (PaddleOCR 3.7, Apache-2.0), text and layout only. Tables and formulas are kept as images in the output. Runs on any CPU. Aimed at plain text and simple layouts. Configured with layout detection and text OCR only, PP-OCRv6 tiny det/rec, a detection limit of 960 and oneDNN off.
- **Accurate:** PaddleOCR-VL 1.6 (0.9B, Apache-2.0) with PP-DocLayoutV3 for layout. Slow on CPU, fast on GPU or Apple Silicon. Aimed at tables, formulas and complex layouts. It scores higher than DeepSeek-OCR 2 on OmniDocBench. This is the default recommendation for most hardware.
- **Max:** DeepSeek-OCR v1 (3B, MIT, Q8_0) from the official ggml-org GGUF repo, through llama.cpp. Recommended for a GPU with at least 8 GB of VRAM or an Apple Silicon Mac with at least 16 GB of memory. "Max" means the biggest model, not necessarily the most accurate.
  - DeepSeek-OCR 2 (3B, Apache-2.0) is available as an optional, experimental model under Max for advanced users. It is not a tier default. It uses a community GGUF conversion (Q8_0) pinned by revision and SHA-256 in the manifest, needs about 7.3 GB of VRAM, runs with DRY sampling, and is labelled experimental in the UI.

All engines implement a common interface covering capabilities, requirements for the suggester, and `run(page)` returning blocks in the shared document IR (ADR 0005). The suggester (ADR 0008) recommends a tier from the hardware. Its rules must not assume Accurate is faster than Max on a CPU.

**Spike outcome.** The model spike ([report](../spike/2026-10-model-spike.md)) compared the two DeepSeek versions on one machine and five pages. Multi-tile worked by default for both on the pinned llama.cpp build, so maturity did not separate them. DeepSeek-OCR 2 showed no quality advantage over v1, needs DRY sampling to avoid looping on the Q4_K_M quant, needs more VRAM (7.3 GB against 5.7 GB for Q8_0), and has no official GGUF. So v1 ships as Max, and OCR 2 stays available as an experimental option. The spike also found that the Fast tier cannot do tables and formulas at a usable speed on CPU, which is why it is redefined above. Verifying the community OCR 2 conversion, or publishing our own from the official weights, is a follow-up before v0.1.

## Consequences

Good:

- A clear mental model for users, and a sensible default for every hardware class.
- Tiers map onto real hardware differences, so the suggester can give plain-language advice.
- Adding an engine means implementing one interface and adding manifest entries, not changing the pipeline or exporters.

Bad:

- Three tiers means three sets of normalization code, three sets of golden-file expectations in principle, and more to test. Only Fast is tested in CI (ADR 0012).
- The tier name "Max" invites the assumption that it is the best. In our own expectations Accurate may beat it on tables and formulas, so the details panel and docs must be honest about this.
- Max ships an older model than the newest available. In our five-page test v1 was at least as good as OCR 2, but that sample is small.
- Two DeepSeek variants to maintain: separate manifest entries, sampling settings (DRY for OCR 2) and normalization quirks, plus one more thing to explain in the UI.
- DeepSeek-OCR 2 depends on a community GGUF conversion by an individual account. Pinning the revision and SHA-256 stops it changing underneath us, but we have not verified how it was made. Until we verify it or publish our own, it stays experimental.
- Fast gives up tables and formulas as text. They come out as images, so Fast is only suitable for plain text and simple layouts, and users with tables need Accurate or Max.
- Fast needs oneDNN off because it crashes on paddlepaddle 3.3.1 CPU, and it is still slow (4 to 11 s/page on a fast desktop CPU, slower on a laptop). Faster CPU engines (ONNX Runtime, OpenVINO) are deferred until after v0.1.
- The interface has to be designed before we have written the second engine, so it may need revision. It is a draft until v0.1.

## Alternatives considered

**A single model for everyone.** Simplest to build and test. Rejected because no single model is both fast on a CPU laptop and best on a GPU workstation.

**Expose raw model names and quantizations.** Honest and flexible. Rejected for the primary UI because the audience is non-technical. The details panel keeps this information available.

**Two tiers (Fast and Accurate), no Max.** Fewer moving parts. Rejected because DeepSeek-OCR is one of the two named model families the project is built around, and high-end hardware can run it. The spike did not show that Max gives no benefit over Accurate (DeepSeek v1 was best on the math page and faster on CPU), but the sample was five pages, so this is still open to review once we have a larger test set.
