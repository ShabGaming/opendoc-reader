# 0004. Three model tiers behind a common engine interface

Status: Accepted

Date: 2026-10-15

## Context

Users range from a person with an 8 GB laptop and no GPU to a person with an RTX 4090 or an Apple Silicon Mac with 32 GB. One model cannot serve both. A 3B vision-language model is too slow on a CPU for batch work, while a classic OCR pipeline is wasteful on a strong GPU and weaker on tables and formulas.

Non-technical users should not have to choose between model names and quantizations. At the same time, contributors and curious users want to see which model is actually running and under what license.

New document OCR models appear frequently. We expect to add GLM-OCR and MinerU, and we do not want each addition to require changes across the app.

## Decision

The app offers three tiers. The UI shows friendly names. A details panel shows the real model names and licenses.

- **Fast:** PP-OCRv6 + PP-StructureV3 (PaddleOCR 3.7, Apache-2.0). Runs on any CPU. Aimed at plain text and simple layouts.
- **Accurate:** PaddleOCR-VL 1.6 (0.9B, Apache-2.0) with PP-DocLayoutV3 for layout. Slow on CPU, fast on GPU or Apple Silicon. Aimed at tables, formulas and complex layouts. It scores higher than DeepSeek-OCR 2 on OmniDocBench.
- **Max:** DeepSeek-OCR 2 (3B, Apache-2.0) through llama.cpp, with DeepSeek-OCR v1 (3B, MIT) as the fallback. Recommended for a GPU or Apple Silicon with 16 GB or more. "Max" means the biggest model, not necessarily the most accurate.

All engines implement a common interface covering capabilities, requirements for the suggester, and `run(page)` returning blocks in the shared document IR (ADR 0005). The suggester (ADR 0008) recommends a tier from the hardware.

**Spike caveat.** llama.cpp has supported DeepSeek-OCR 2 since 2026-05-29. However, a PR that fixed multi-tile handling for it was closed unmerged, and multi-tile support for v1 is more mature. A technical spike on the dev PC decides which version ships as Max. Documentation and UI code should refer to "DeepSeek-OCR" generically until then.

## Consequences

Good:

- A clear mental model for users, and a sensible default for every hardware class.
- Tiers map onto real hardware differences, so the suggester can give plain-language advice.
- Adding an engine means implementing one interface and adding manifest entries, not changing the pipeline or exporters.

Bad:

- Three tiers means three sets of normalization code, three sets of golden-file expectations in principle, and more to test. Only Fast is tested in CI (ADR 0012).
- The tier name "Max" invites the assumption that it is the best. In our own expectations Accurate may beat it on tables and formulas, so the details panel and docs must be honest about this.
- Shipping either DeepSeek-OCR version carries risk. If v1 ships, we ship an older model than the newest available. If OCR 2 ships, we depend on llama.cpp behavior that has known multi-tile issues.
- The interface has to be designed before we have written the second engine, so it may need revision. It is a draft until v0.1.

## Alternatives considered

**A single model for everyone.** Simplest to build and test. Rejected because no single model is both fast on a CPU laptop and best on a GPU workstation.

**Expose raw model names and quantizations.** Honest and flexible. Rejected for the primary UI because the audience is non-technical. The details panel keeps this information available.

**Two tiers (Fast and Accurate), no Max.** Fewer moving parts. Rejected because DeepSeek-OCR is one of the two named model families the project is built around, and high-end hardware can run it. This is open to review if the spike shows that Max gives no measurable benefit over Accurate.
