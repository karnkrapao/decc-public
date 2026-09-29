<div align="center">

<img src="src-tauri/icons/icon.png" width="96" alt="DECC app icon" />

# DECC

**A local-first desktop control center for the projects you already run.**

Add a workspace once. Let DECC detect the runnable parts inside it, then run, inspect, troubleshoot, and open them from one calm desktop app.

**macOS · Windows · Tauri 2 · Rust · React · TypeScript**

[Downloads](#download) · [What DECC does](#what-decc-does) · [Build from source](#build-from-source) · [Release process](RELEASE.md)

</div>

---

> **Preview status:** DECC `0.1.0` is pre-release. Signed public installers are not published yet. When the first production build is ready, it will appear in [GitHub Releases](https://github.com/karnkrapao/decc/releases).

## Source availability and licensing

DECC may be published with its source visible while the product is still in preview. The repository currently does not include an open-source license. Distribution and commercial-use terms will be published separately when they are finalized.

## Why DECC

Modern development workspaces are rarely one process in one folder. A single repository can contain a frontend, API, worker, CLI, desktop app, and supporting services, each with different commands, ports, runtimes, and tooling.

DECC sits above that complexity without trying to replace your IDE or terminal.

```text
Workspace
├── Frontend       Next.js · pnpm
├── API            Go
├── Worker         Rust
├── Infrastructure project-defined orchestration
└── Git · Runtime · Environment · Ports · Logs
```

The goal is simple: **understand a local workspace quickly and control the useful parts without memorizing everything.**

## What DECC does

| Capability | What it means |
| --- | --- |
| **Detection-first workspaces** | Add a folder and DECC scans it for runnable components instead of forcing a long setup form. |
| **Technology-neutral components** | JavaScript/TypeScript, Go, .NET, Rust, and project-defined orchestration are handled as components rather than hard-coded “frontend/backend” types. |
| **Run control** | Start, stop, restart, and inspect DECC-owned processes while keeping external processes clearly separate. |
| **Port awareness** | See which process owns a port and understand conflicts instead of decoding an opaque bind error. |
| **Runtime insight** | Detect missing or incompatible local runtimes and explain what needs attention without silently installing toolchains. |
| **Environment insight** | Inspect environment-file presence and variable names without exposing secret values by default. |
| **Git context** | See repository/branch context from the same workspace view. |
| **Editor launching** | Open a workspace or component in the configured editor, with global defaults and per-workspace overrides. |
| **Local-first state** | Workspace metadata, logs, and preferences stay local by default. |

## Product principles

- **Workspace first.** One folder may contain many useful components.
- **Detect before asking.** Good defaults first, manual correction second.
- **Respect the repository.** Existing scripts and orchestration are inputs; DECC should not invent commands when the project already defines them.
- **Do not silently own external processes.**
- **Do not become another IDE.**
- **Keep user repositories clean.** DECC-specific state lives outside the user's project by default.
- **Local-first by default.** Source code, logs, and environment values are not uploaded automatically.

## Download

### Windows

The production Windows download will be a signed NSIS installer:

```text
DECC_<version>_x64-setup.exe
```

Open the installer and follow the normal Windows installation flow.

### macOS

The production macOS download will be a signed and notarized DMG:

```text
DECC_<version>_<architecture>.dmg
```

Open the DMG and drag **DECC** into **Applications**.

Tauri documents DMG as the standard outside-the-App-Store installation experience for macOS applications, which is why DECC's release pipeline produces one in addition to the app bundle.

### Releases

Production builds will be published from the [GitHub Releases page](https://github.com/karnkrapao/decc/releases) after cross-platform verification, signing/notarization checks, and final release review pass.

Release tags prepare a **draft** GitHub Release first so the final assets and notes can be reviewed before publishing.

## Updates

DECC does not claim in-app automatic updates yet.

The intended update experience is user-controlled:

1. DECC checks whether a newer signed version exists.
2. The app shows the version and release notes.
3. The user chooses **Download & install**.
4. DECC downloads the signed update bundle and verifies its updater signature.
5. Windows launches the updater installer and exits the current app; macOS installs the update and then relaunches the app.
6. A manual **Check for updates** action remains available in Settings.

Until that updater path is enabled and verified, users should install newer versions from GitHub Releases.

See [docs/UPDATES.md](docs/UPDATES.md) for the technical update strategy.

## Security and privacy

DECC treats local developer projects as trusted-but-sensitive data.

- No generic arbitrary-shell primitive is exposed to the React UI.
- Privileged operations go through typed Tauri commands and validated Rust logic.
- External processes are detected for context but are not silently killed or adopted.
- Environment values are not shown by default.
- Release artifacts have platform-specific integrity/signing gates.
- Release-critical source checks reject common private/local files from Git tracking.

If a signing key, credential, or token is ever exposed, removing it from Git history is not enough—the credential must also be rotated.

## Build from source

### Requirements

- Node.js 24
- pnpm 12.5.1
- Rust toolchain
- Tauri platform prerequisites for your OS

### Setup

```bash
git clone https://github.com/karnkrapao/decc.git
cd decc
pnpm install --frozen-lockfile
pnpm tauri dev
```

### Useful commands

```bash
pnpm dev
pnpm test
pnpm build
pnpm verify
pnpm tauri dev
cd src-tauri && cargo test
```

`pnpm verify` is the canonical local gate. GitHub Actions runs the same verification on macOS and Windows before the integrated Tauri build.

## Release engineering

| Command | Purpose |
| --- | --- |
| `pnpm verify` | Metadata, source-integrity, frontend, Rust tests, formatting, and native build |
| `pnpm release:preflight:macos` | Build and validate the macOS app + DMG |
| `pnpm release:verify-signed:macos` | Require Developer ID signing and Gatekeeper acceptance |
| `pnpm release:preflight:windows` | Build and validate the Windows release executable + NSIS installer |
| `pnpm release:smoke:windows` | Install → launch → uninstall lifecycle test |
| `pnpm release:verify-signed:windows` | Require valid Authenticode signatures |

The GitHub Actions **Release Candidate** workflow can build unsigned RC artifacts for validation. Distribution/tag runs are fail-closed: strict platform signing checks must pass before a draft release can be prepared.

See [RELEASE.md](RELEASE.md) and [CHANGELOG.md](CHANGELOG.md).

## Current scope

DECC is focused on the local development control loop: **add workspace → detect components → understand state → run and inspect**.

Cloud sync, accounts, updater channels, analytics, and other online product infrastructure are intentionally not treated as prerequisites for the core local experience.

---

<div align="center">

**DECC — understand the workspace, control the useful parts, stay in your flow.**

</div>
