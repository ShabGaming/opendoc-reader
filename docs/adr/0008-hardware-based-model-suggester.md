# 0008. Hardware-based model suggester with rules stored as data

Status: Accepted

Date: 2026-10-15

## Context

Which model runs well depends on the machine: GPU vendor, model and VRAM; unified memory on Apple Silicon; system RAM; CPU cores; and free disk space. Non-technical users cannot be expected to know whether a 3B model at Q4_K_M fits in their VRAM or how slow it will be on a CPU.

A wrong default is costly. Recommending a large model to a CPU-only laptop leads to a slow first experience and a multi-gigabyte download the user did not need. Under-recommending wastes a strong GPU.

Thresholds will change as we get real measurements, as llama.cpp backends improve, and as new engines are added. Contributors with different hardware are the best source of corrections, and many of them will not write Rust.

## Decision

The Rust core probes the hardware and a suggester applies a set of rules to the result. The rules live in `models/manifest.json`, next to the model entries they refer to, so a new model and the rules that recommend it land in the same pull request. Code only evaluates the rules, and tuning them does not need code changes.

The output is one recommended model with a plain-language reason, for example "Your RTX 3060 (12 GB) runs Max at about 4 s per page", plus a fit badge for each other option: "Runs well", "Slow on this PC" or "Won't fit". The Setup screen shows the recommendation first and the alternatives after it.

After a model is downloaded, a one-page calibration benchmark runs on the user's machine and refines the time estimates.

The example figure above is illustrative. Real numbers come from the technical spike and later from contributor reports.

## Consequences

Good:

- Non-technical users get a sensible default and an explanation in plain language.
- Contributors can improve the suggestions by editing JSON, and the changes are reviewable in a pull request.
- The calibration benchmark corrects for what static rules cannot know, such as thermal limits, driver quality or slow memory.
- The same rule format works for new engines.

Bad:

- The first rules will be based on a small number of machines (an RTX 4090 PC and an Apple Silicon Mac). Others are extrapolated until measured, and recommendations may be wrong for some hardware.
- Detecting hardware reliably is hard: VRAM on Windows with hybrid graphics, integrated GPUs sharing system memory, and Vulkan on older drivers all need handling.
- A rule language in JSON can grow into a poorly specified programming language. We should keep it small and add a schema and validation tests.
- The calibration step adds time at the end of setup, and a benchmark on one page may not represent real documents.
- Keeping rules in the manifest means a rule-only tweak still touches the file that pins downloads, so reviewers need to check that hashes and URLs were not changed by accident.

## Alternatives considered

**Ask the user to choose.** Simple to build. Rejected because the target users cannot judge the tradeoffs and often pick the largest option.

**Hard-coded recommendation logic in Rust.** Easier to write at first. Rejected because every tuning change would need a code change and a release, and it closes the door on contributors who do not write Rust.

**Always recommend the same tier.** Rejected because hardware ranges from CPU-only laptops to high-end GPUs and no single default works for both.

**Benchmark every model on first run instead of using rules.** Accurate but would require downloading everything first. Rejected. The one-page calibration after download is the compromise.
