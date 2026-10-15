# 0002. Tauri 2 with React and TypeScript for the desktop app

Status: Accepted

Date: 2026-10-15

## Context

The app targets Windows 10 22H2+ (x64) and macOS 13+ (Apple Silicon). It needs a polished native-feeling UI: a queue sidebar, a drop area, a side-by-side review view with synced scrolling, and a settings screen. It will also supervise other processes, run long downloads, probe hardware and persist a job queue.

The app competes for memory with the OCR models. On a machine with 8 to 16 GB of RAM, every few hundred megabytes used by the shell is memory the model cannot use. Installer size also matters, because the models are already large and we want the installer itself to be small (ADR 0007).

The maintainer works on one Windows machine and one Apple Silicon Mac, so the toolchain has to build and test on both.

## Decision

The desktop app uses Tauri 2. The core is Rust. The UI is React and TypeScript, built with Vite, and rendered in the OS webview: WebView2 on Windows and WKWebView on macOS. UI components use Radix unstyled primitives with our own design tokens.

The Rust core owns the job queue, process supervision, downloads, hardware probing and settings. The UI talks to the core through Tauri commands and events.

## Consequences

Good:

- Small installer (an estimate of 20 to 40 MB for the app alone) and lower idle RAM than a bundled Chromium.
- Rust is a good fit for process supervision, resumable downloads and checksum verification.
- The React and TypeScript ecosystem gives us accessible primitives (Radix) and plenty of contributors who already know it.
- Tauri 2 has first-class bundling for .msi, .exe and .dmg, which is what the release workflow needs (ADR 0011).

Bad:

- Contributors need both a Rust and a Node toolchain to build the app. This raises the barrier for casual contributions to the shell.
- The UI renders differently in WebView2 and WKWebView. We must test both and avoid features one of them lacks.
- Rust compile times are slow, especially on CI.
- Tauri's sidecar and permission model has to be configured carefully for the Python engine and `llama-server`.
- The Python engine is a separate process, so the project has three runtimes to maintain (Rust, TypeScript, Python). ADR 0003 explains why that cost is accepted.

## Alternatives considered

**Electron.** Mature, consistent rendering across platforms, and the largest ecosystem. Rejected because it bundles Chromium, which adds about 100 MB or more to the installer and uses significantly more RAM at idle. That RAM is better left to the models.

**Native per-OS apps (WinUI/WPF and SwiftUI).** Best platform feel and smallest footprint. Rejected because it means two separate codebases and two sets of skills for a project that is currently maintained by one person. Review, queue and settings screens would have to be written twice.

**Python GUI such as Qt (PySide).** Would let the UI and engine share a language and avoid the process boundary. Rejected because getting a Qt app to look like a native utility and follow OS light and dark mode takes more work than a webview UI. Packaging a Python app with Qt as a small, signed installer is also harder than packaging a Tauri app, and it would tie the UI process to the heavy Python environment that we want to create lazily on first run.
