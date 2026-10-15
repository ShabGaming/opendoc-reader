# 0001. Local-only processing with no telemetry

Status: Accepted

Date: 2026-10-15

## Context

OpenDoc Reader converts scanned documents. Those documents are often invoices, contracts, medical letters and personal records. The target users are non-technical people who may not be comfortable uploading such files to a service. Cloud OCR is also unavailable in some workplaces and on restricted networks. At the same time, state-of-the-art OCR models (PaddleOCR-VL, DeepSeek-OCR) are small enough to run on a consumer laptop or desktop.

We also want the project to be maintainable by a small number of people. Running a cloud service, handling accounts and storing user data would add operational and legal work that this project does not want to take on.

## Decision

All document processing happens on the user's computer. There is no account, no cloud component and no telemetry. The app works offline once models are downloaded.

The app makes network requests only for these purposes:

- Downloading runtimes and models from the manifest (official Hugging Face repositories, with ModelScope as a fallback), when the user starts a download.
- The monthly update check, which only opens the GitHub release page in the user's browser (see ADR 0010).

Neither request includes document content or information about the user. The local `llama-server` listens on 127.0.0.1 only. The Python engine communicates with the app over stdio and opens no port. Logs stay on disk, and Settings has a "copy diagnostics" button so a user can choose to paste them into a GitHub issue.

## Consequences

Good:

- Documents never leave the machine, which is easy to explain and easy to verify by reading the code.
- No server costs, no accounts, no data-protection obligations for stored user data.
- Works on air-gapped machines after models are imported from a folder.

Bad:

- Quality and speed are bounded by the user's hardware. A laptop with 8 GB of RAM and no GPU will be slow on the larger models. This is why the Fast tier exists and why the hardware suggester exists (ADR 0004, ADR 0008).
- We get no usage data or crash reports. Bug reports depend on users copying diagnostics by hand, and we will not know which hardware configurations are common.
- First-run downloads are large (gigabytes), which is a barrier on slow connections.
- Contributors must keep this rule in mind. A dependency that phones home, or a well-meaning "anonymous stats" feature, breaks it and needs to be rejected in review.

## Alternatives considered

**Cloud OCR API.** Better quality is available with large hosted models, and no local hardware is needed. Rejected because it requires sending private documents to a third party, needs an account or API key, costs money per page, and does not work offline.

**Hybrid with an optional cloud mode.** Rejected for now. An optional cloud path creates pressure on the privacy guarantee and doubles the surface to test. It could be reconsidered later as a separate decision.

**Opt-in anonymous telemetry.** Would help prioritize hardware support and find crashes. Rejected because the project's main promise is that nothing leaves the machine, and a toggle makes that promise harder to state. The local log and "copy diagnostics" button cover the debugging need at lower cost.
