# Architecture Decision Records

An Architecture Decision Record (ADR) is a short document that captures one significant decision: the context that forced it, what was decided, what it costs, and which alternatives were rejected. ADRs in this repository are numbered in the order they were written and are not renumbered. If a decision changes, the old ADR stays in place with its status changed to "Superseded by NNNN", and a new ADR explains the change. To propose a new decision, open a pull request that adds the next-numbered file using the same format as the existing ones. The overall design these records support is described in [docs/architecture.md](../architecture.md). The project is pre-alpha, so every record below describes a planned design that has not been implemented yet.

| Number | Title | Status |
| --- | --- | --- |
| [0001](0001-local-only-no-telemetry.md) | Local-only processing with no telemetry | Accepted |
| [0002](0002-tauri-react-typescript-desktop-app.md) | Tauri 2 with React and TypeScript for the desktop app | Accepted |
| [0003](0003-hybrid-llama-server-and-python-engine.md) | Hybrid runtime: llama-server plus a CPU-only Python engine | Accepted |
| [0004](0004-three-model-tiers-common-engine-interface.md) | Three model tiers behind a common engine interface | Accepted |
| [0005](0005-single-document-model-no-pandoc.md) | One document model for all exporters, no pandoc | Accepted |
| [0006](0006-structural-fidelity-and-searchable-pdf.md) | Structural fidelity for Markdown and Word, searchable PDF for visual fidelity | Accepted |
| [0007](0007-small-installer-first-run-downloads.md) | Small installer with first-run downloads from a checksummed manifest | Accepted |
| [0008](0008-hardware-based-model-suggester.md) | Hardware-based model suggester with rules stored as data | Accepted |
| [0009](0009-unsigned-builds-before-1.0.md) | Ship unsigned builds before 1.0 | Accepted |
| [0010](0010-monthly-update-check-no-auto-update.md) | Monthly update check instead of auto-update | Accepted |
| [0011](0011-monorepo-and-tag-triggered-draft-releases.md) | Monorepo layout and tag-triggered draft releases | Accepted |
| [0012](0012-golden-file-tests-on-fast-tier.md) | Golden-file tests on the Fast tier in CI | Accepted |
