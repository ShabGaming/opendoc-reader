# 0012. Golden-file tests on the Fast tier in CI

Status: Accepted

Date: 2026-10-15

Updated: 2026-10-16, when the golden set was built

## Context

OCR output is hard to test. Results depend on the model, its quantization, the hardware and the preprocessing. Small changes in rendering or deskewing can shift output. We still need to catch regressions in preprocessing, normalization to the IR, and the exporters, since those are our own code and change often.

GitHub Actions runners have no GPU. The VLM tiers need either a GPU for reasonable speed or minutes per page on CPU, and their models are multi-gigabyte downloads. Running them on every pull request is not practical.

The maintainer has an RTX 4090 PC and an Apple Silicon Mac, so heavier checks can be run by hand before a release.

## Decision

The repository includes a small set of public-domain sample pages: a book page, a statistical table, a formula page, a two-column dictionary page, a cookbook page with lists, and two US federal forms. Each has checked-in expected outputs (golden files). Sources and licenses are in `engine/tests/golden/SOURCES.md`.

On every pull request, CI runs these samples through the Fast tier, which runs on any CPU, and compares the result to the golden files. The comparison covers the IR and the exported Markdown; the DOCX exporter has its own structural unit tests. Tolerance rules for harmless differences (block order similarity, word similarity, bounding box overlap) are defined at the top of `engine/tests/test_golden.py` and kept as narrow as practical.

For the Accurate and Max tiers, a manual benchmark script is run on the maintainer's machines before each release. Its results are reviewed by hand and are not part of CI.

Golden files are updated deliberately, in the same pull request as the change that causes them to differ, and reviewers check the diff.

## Consequences

Good:

- Regressions in preprocessing, the IR and exporters are caught on every pull request without special hardware.
- The sample set doubles as documentation of what the pipeline is expected to do.
- Because the samples are public domain, they can live in the repository without license concerns.
- The manual benchmark focuses attention on the models that matter for users with strong hardware.

Bad:

- CI never exercises the VLM paths: `llama-server` integration, DeepSeek-OCR grounding parsing and PaddleOCR-VL element handling. Bugs there can reach a release. The manual benchmark is the only guard, and it relies on the maintainer remembering to run it.
- Fast tier output may change when PaddleOCR is upgraded, which means regenerating golden files and checking whether differences are improvements or regressions.
- Tests may be flaky across platforms if floating point or library versions produce slightly different results. The engine environment must be pinned in CI.
- Five samples are a thin sample. Edge cases such as skewed phone photos, HEIC input or very large PDFs need extra fixtures over time.
- Golden files can encode current mistakes as correct. Reviewers have to look at actual output and not only at whether the test passes.
- The Fast tier needs the engine environment and its models in CI, which adds download time and caching work.

## Alternatives considered

**Self-hosted GPU runner.** Would allow testing the VLM tiers on every pull request. Rejected because it needs hardware and ongoing maintenance, and a runner exposed to pull requests from forks is a security risk.

**Mock the models.** Fast and deterministic. Rejected as the only approach because mocks do not test recognition or normalization against real output. They can still be used for narrow unit tests.

**No automated output tests; manual checks only.** Rejected because regressions in exporters and normalization are likely during early development and are cheap to catch automatically.

**Quality metrics (edit distance) instead of exact golden files.** Useful for model comparison. Rejected for CI gating because a threshold hides small structural regressions. A metric-based comparison may be added to the manual benchmark.
