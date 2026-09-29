# DECC Release Process

DECC treats release packaging as a verification pipeline, not merely a build command. A bundle is not distribution-ready just because it compiles.

## Release workflow

The GitHub Actions **Release Candidate** workflow has two modes.

### Manual RC validation

Run `workflow_dispatch`, provide the expected version, and leave `distribution` disabled.

The workflow:

- runs the canonical verification gates,
- builds the macOS app and DMG,
- validates macOS bundle/DMG metadata,
- builds the Windows NSIS installer,
- validates Windows release metadata and hashes,
- smoke-tests Windows install → launch → uninstall,
- uploads the validated RC artifacts.

Unsigned/ad-hoc RC artifacts are allowed only in this mode and are reported explicitly.

### Distribution/tag validation

A `v*` tag, or a manual run with `distribution` enabled, additionally requires:

- macOS Developer ID signing,
- Gatekeeper acceptance/notarization,
- valid Windows Authenticode signatures.

These checks are fail-closed. A distribution artifact is not prepared if strict signing verification fails or cannot be performed.

When a tag passes both platform jobs, the workflow downloads the validated install assets and prepares a **draft GitHub Release** containing the macOS DMG and Windows NSIS installer. The draft is reviewed manually before publication.

## Unsigned public preview

The separate GitHub Actions **Preview Release** workflow is for early public testing before production signing credentials are available.

It:

- requires the preview tag to use the `preview-*` namespace,
- runs the existing macOS release preflight and produces a DMG,
- runs the existing Windows release preflight and installer lifecycle smoke,
- prepares a **draft GitHub pre-release** before platform builds,
- uploads the user-installable DMG and NSIS installer directly from their platform runners,
- uploads an adjacent `.sha256` checksum file for each installer,
- keeps the draft unpublished until the assets and warnings are reviewed.

Preview tags use the `preview-*` namespace. Production `v*` tags remain reserved for the fail-closed signed distribution path.

The preview workflow never calls the strict production signing verification scripts. This is intentional for early testing and does not weaken the production/tag gates.

Because release immutability is enabled, every platform asset-upload step must explicitly preserve `draft: true` until both platforms and their checksum files are present. Publishing a partial preview would make that release immutable before the remaining assets can be attached.

## Version rule

`package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` must contain the same version.

Release automation also sets `DECC_EXPECTED_VERSION`. `scripts/check-release-metadata.mjs` accepts an optional leading `v` and fails when the requested/tagged version differs from repository metadata.

```text
tag: v0.1.0
repository metadata: 0.1.0
result: allowed

tag: v0.1.1
repository metadata: 0.1.0
result: rejected
```

## Required gates before a production tag

Before pushing a production tag:

1. The exact target commit is on `main` and the working tree is clean.
2. GitHub **Verify** is green on macOS and Windows.
3. `pnpm verify` passes in a normal development environment.
4. `pnpm release:preflight:macos` passes and produces a valid app + DMG.
5. `pnpm release:preflight:windows` passes.
6. `pnpm release:smoke:windows` passes on a real Windows machine.
7. `pnpm release:verify-signed:macos` passes with real distribution credentials.
8. `pnpm release:verify-signed:windows` passes with real distribution credentials.
9. Product/distribution terms have been reviewed for the intended release. If no open-source license is present, public release notes must not describe DECC as open source.
10. `CHANGELOG.md` and the draft release notes are reviewed.
11. Final installer QA is performed on clean macOS and Windows user profiles.

## Install assets

### macOS

The RC pipeline builds:

- `DECC.app` for bundle validation,
- a DMG for user installation.

The DMG is verified with `hdiutil verify` and its SHA-256 is reported.

For production distribution, the app must have a Developer ID Application signature and Gatekeeper must accept the resulting bundle before the DMG is published.

### Windows

The RC pipeline builds:

- `decc.exe`,
- `DECC_<version>_x64-setup.exe`.

The bundle checker verifies product/version metadata, non-empty artifacts, SHA-256 hashes, and Authenticode state.

Some CI environments can fail to load PowerShell's Authenticode inspection module. In non-distribution RC mode this is reported as `Unavailable` instead of failing the entire unsigned RC build. **Strict distribution mode still fails unless the signature status is verified as `Valid` and a signer certificate is present.**

## Update delivery

In-app updates are not enabled yet.

The recommended product behavior is:

- check for updates automatically,
- show version + release notes,
- download only after the user accepts,
- verify the Tauri updater signature,
- install/restart or relaunch,
- keep GitHub Releases as the manual fallback.

See [docs/UPDATES.md](docs/UPDATES.md).

Until the updater signing key, updater artifacts, endpoint manifest, UI states, and cross-platform update tests are complete, users update manually from GitHub Releases. Preview-channel installers may be unsigned and are clearly labeled as such; production releases remain signed/notarized.

## Credential policy

Do not commit or invent production credential values in source control.

Production secrets include:

- Apple signing/notarization credentials,
- Windows code-signing private keys,
- Tauri updater private keys/passwords,
- release-service access tokens.

The repository may contain public verification metadata such as a future updater public key, but never the corresponding private key.

## Repository hygiene

Release-critical source verification rejects common local/private files from Git tracking, including environment files, private-key containers, local databases, crash dumps, local tool state, and maintainer-only instruction files.

Public-facing repository documentation belongs in `README.md`, `SECURITY.md`, `CONTRIBUTING.md`, `RELEASE.md`, `CHANGELOG.md`, and `docs/`.

## Recovery rule

If a release workflow fails, repair the source/configuration and create a new verified snapshot.

Do not weaken or skip:

- release metadata validation,
- tracked-source integrity,
- signing checks,
- Gatekeeper/notarization,
- Authenticode verification,
- installer lifecycle smoke tests

just to make a release green.
