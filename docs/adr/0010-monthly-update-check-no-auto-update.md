# 0010. Monthly update check instead of auto-update

Status: Accepted

Date: 2026-10-15

## Context

Users need to learn that a new version exists, especially while the project is pre-1.0 and changing quickly. A fully automatic updater needs signed update payloads, a hosting scheme for update metadata, and a trusted signing key. We do not have code signing yet (ADR 0009). An updater that installs unsigned code is a security risk and would also conflict with the project's privacy stance (ADR 0001) if it contacted a server we run.

At the same time, an app that never mentions new versions leaves users on old builds with known bugs.

## Decision

The app checks for a new release once a month. The check is on by default and can be turned off in Settings. When a newer version exists, the app only opens the GitHub releases page in the user's browser. It downloads and installs nothing.

The check sends nothing about the user: no identifier, no version beyond what a plain HTTPS request to GitHub's public release information reveals, and no telemetry. It is the only automatic network request the app makes besides downloads the user starts.

There is no auto-update for now. In-app auto-update is listed under "Later" on the roadmap. The monthly check is scheduled for v0.2.

## Consequences

Good:

- Simple to build and easy to audit.
- No update server to run and no update-signing key to protect.
- Users stay in control of when and what they install.
- Consistent with the no-telemetry rule. A request to GitHub's public API reveals an IP address to GitHub, as any download would, and nothing else we control.

Bad:

- Updating is manual: the user downloads and runs the new installer, and passes through the same security warnings described in ADR 0009 each time while builds are unsigned.
- A monthly check means a user may not hear about a fix for up to a month. Critical fixes could be missed.
- Some users will ignore the notice, so old versions will persist in the wild.
- The check does reveal the user's IP address to GitHub, which a user on a restricted or sensitive network might not want. The toggle in Settings is the mitigation, and the behavior should be described in the README.
- v0.1 ships without the check, so users of the first release get no notification about v0.2.

## Alternatives considered

**Auto-update (for example Tauri's updater).** Best user experience. Rejected for now because it needs signed update bundles and hosted metadata, and we have no signing set up. It will be revisited after code signing is in place.

**No update check.** Simplest and most private. Rejected because users would have no way to learn about fixes inside the app.

**Check on every launch.** Rejected as unnecessary traffic, and it makes the app feel like it phones home. A monthly cadence is enough for a project with infrequent releases.

**Opt-in check, off by default.** Fewer users would be surprised, but most would never turn it on. Since the request carries nothing about the user, on by default with a visible toggle was chosen.
