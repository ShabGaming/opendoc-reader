# Security policy

## Supported versions

OpenDoc Reader is pre-1.0. Only the latest release receives security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately, using GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue for a security problem.

Useful details:

- What you found and where (file, setting, or part of the app).
- Steps to reproduce, or a proof of concept.
- The affected version and your operating system.
- What an attacker could do with it.

Please do not include real personal documents in a report.

## What to expect

OpenDoc Reader is a volunteer project with one maintainer. I will try to acknowledge a report within a week and to tell you what I plan to do about it, but I cannot promise fixed timelines. If you would like to be credited in the release notes, say so. Please give me reasonable time to fix a problem before you disclose it publicly.

## Scope

In scope:

- The desktop app, the Python engine, and the build and release workflows in this repository.
- The download manifest (`models/manifest.json`). A checksum mismatch, a wrong or missing hash, an unpinned or tampered URL, or any way to make the app run a file that was not verified against the manifest is a security problem, so please report it.
- Anything that sends document content or personal data off the machine. The app is designed to run locally with no telemetry.

Out of scope:

- Vulnerabilities in third-party software that the app downloads, such as `llama-server` builds from llama.cpp and the model files. Report those to their own projects. If a pinned version in the manifest is affected, tell me so the pin can be updated.
- Warnings from Windows SmartScreen, macOS Gatekeeper or antivirus software about the unsigned installers. This is known and documented in the README.
- Problems that need an attacker who already has full control of your machine or your user account.
