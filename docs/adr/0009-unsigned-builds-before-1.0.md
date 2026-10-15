# 0009. Ship unsigned builds before 1.0 (ad-hoc signing on macOS)

Status: Accepted

Date: 2026-10-15

## Context

Windows shows a SmartScreen warning for executables without an established signature and reputation. macOS Gatekeeper blocks apps that are not notarized, though a user can override this. Apple Silicon additionally refuses to run unsigned arm64 code, so some form of signature is needed there.

Proper signing costs money or requires an application process: an Apple Developer Program membership for notarization on macOS, and either SignPath Foundation (free for qualifying open source projects, with an approval process) or Azure Trusted Signing on Windows. The project is pre-alpha, has one maintainer, and has no users yet. Signing infrastructure would take time away from getting a working app, and the first builds are for testers who can follow instructions.

## Decision

Builds are not signed with a purchased or issued certificate before 1.0. CI applies free ad-hoc signing on macOS, which is the minimum Apple Silicon requires to run the app.

The README includes short written instructions (no screenshots) for getting past each warning:

- Windows SmartScreen: click "More info", then "Run anyway".
- macOS Gatekeeper: System Settings, then Privacy & Security, then "Open Anyway".

Proper signing is a pre-1.0 roadmap item: Apple notarization on macOS, and SignPath Foundation or Azure Trusted Signing on Windows.

## Consequences

Good:

- No recurring cost and no dependency on a signing provider while the project is still changing quickly.
- The release pipeline can be built and tested without secrets for signing.
- Releases can start as soon as there is something to test.

Bad:

- Users see a security warning on first launch on both platforms. For non-technical users this is a real barrier, and some will give up. It also teaches users to click through warnings.
- Each new build on Windows may be flagged again, because SmartScreen reputation is tied to a signature.
- Antivirus products are more likely to flag unsigned installers, particularly because the app downloads and runs `llama-server`.
- Users cannot cryptographically verify that a download came from the maintainer. We rely on GitHub release hosting and, where we publish them, checksums.
- The README steps depend on the exact OS wording, which changes between versions and will need maintenance.
- Auto-update is not offered, which is consistent with ADR 0010 and avoids trusting unsigned update payloads.

## Alternatives considered

**Sign from the first release.** Better first-run experience. Rejected for now because it requires cost or approval processes that are not justified at pre-alpha, and signing setup would delay the first usable build. It remains a goal before 1.0.

**No signing at all, including no ad-hoc signing on macOS.** Rejected because Apple Silicon will not run completely unsigned arm64 binaries.

**Distribute only source and require users to build.** Rejected because the target users are non-technical.
