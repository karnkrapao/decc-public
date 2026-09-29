# Changelog

Notable DECC changes are recorded here. A release date should be added only when the corresponding version is actually published.

## 0.1.0 — Unreleased

### Added

- Local-first Workspace → Components desktop control flow for macOS and Windows.
- Cross-platform component discovery, runtime/environment/Git inspection, process control, ports, logs, editor launching, and persisted workspace preferences.
- Canonical release metadata and tracked-source integrity verification.
- macOS app + DMG release preflight with a strict Developer ID/Gatekeeper distribution gate.
- Windows NSIS release preflight with metadata, SHA-256, Authenticode inspection, and isolated install → launch → uninstall smoke coverage.
- GitHub cross-platform verification and fail-closed Release Candidate workflow.
- Tag-gated draft GitHub Release preparation for validated DMG and NSIS install assets.
- Public product README and documented future in-app update strategy.

### Hardened

- Windows native process-tree handling and platform-specific test behavior.
- Recovery from stale workspace/preferences state and corrupted persistence candidates.
- Test fixture cleanup and protection against development/test workspace data leaking into production state.
- Release version consistency across package, Tauri, Cargo, workflow input, and release tag metadata.
- Windows release validation now tolerates an unavailable Authenticode inspection module only for non-distribution RC validation; strict distribution verification remains fail-closed.
- Repository hygiene now ignores and rejects common local credentials, machine state, local tool state, and maintainer-only files from the tracked public tree.
- GitHub Actions use Node 24-compatible action generations.

### Distribution status

v0.1.0 remains a release candidate until production macOS signing/notarization, Windows Authenticode signing, release terms/licensing, final clean-machine QA, and final artifact review are complete.

In-app updating is intentionally not enabled yet; signed installer downloads remain the fallback until the Tauri updater path is implemented and verified.
