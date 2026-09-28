# DECC Update Strategy

## Recommended user experience

DECC should eventually **check automatically but install only with user consent**.

Default flow:

1. Check for a newer stable version after startup and from a manual **Check for updates** action.
2. If an update exists, show the new version, release notes, approximate download size when available, and a **Download & install** button.
3. Download only after the user accepts the update.
4. Show progress and keep the current version usable until installation begins.
5. Verify the updater signature before installation.
6. Install and restart/relaunch with a clear success/failure path.
7. Keep GitHub Releases as a manual fallback.

A future preference may allow background downloading, but silent forced installation should not be the v1 default.

## Tauri updater model

DECC is built on Tauri 2, whose updater plugin supports Windows and macOS and can use either a dynamic update service or static JSON hosted with release assets.

The updater security model is separate from normal platform code signing:

- The app contains an **updater public key**.
- Release/update artifacts are signed with the matching **updater private key**.
- Tauri verifies the signature before installation.
- Updater signature verification cannot be disabled.
- Losing the updater private key can prevent shipping trusted updates to already-installed clients, so the private key must be backed up securely and never committed to Git.

DECC should use a dedicated updater signing key stored only in the release secret system.

## Suggested release architecture

For the first in-app updater implementation:

- Enable the Tauri updater plugin.
- Set `bundle.createUpdaterArtifacts` to `true`.
- Generate and store the updater key pair.
- Put only the public key in Tauri configuration.
- Keep the private key/password in release secrets.
- Publish signed updater artifacts with each production GitHub Release.
- Publish a static `latest.json` manifest that points to the signed platform artifacts.
- Configure the stable updater endpoint to that manifest.
- Add a Settings action for manual checks.
- Add a non-blocking update notification in the app shell.

GitHub Releases is sufficient for a simple stable channel. A dedicated service is only necessary if DECC later needs staged rollouts, richer targeting, private channels, rollback policy, or analytics.

## Platform behavior

### Windows

Recommended mode: **passive** installer UI.

After the user accepts an update:

- DECC downloads and verifies the signed updater package.
- The updater installer is launched.
- The running DECC process exits so files can be replaced.
- The installer shows lightweight progress and completes the update.
- DECC can then be started again/restarted by the updater flow.

The normal signed NSIS installer remains the manual fallback.

### macOS

After the user accepts an update:

- DECC downloads and verifies the signed macOS updater archive.
- The application bundle is replaced by the updater.
- DECC relaunches into the new version.

The signed/notarized DMG remains the manual install/fallback path.

## v0.1.0 boundary

Until updater keys, updater artifacts, endpoint configuration, UI states, and cross-platform update tests are all in place:

- Do not show an in-app update button that implies the updater is operational.
- Publish normal signed installers through Releases.
- Users update by downloading the new release and installing it over the existing version.

This manual fallback is acceptable for the first production release and keeps updater/signing complexity out of the critical path.

## Required updater tests

Before enabling in-app updates publicly, verify at least:

- no-update response,
- update available,
- interrupted download,
- invalid updater signature,
- unreachable endpoint,
- install failure,
- upgrade from the oldest supported installed version,
- Windows install/restart,
- macOS install/relaunch,
- application state/persisted workspaces survive the update,
- release channel/version comparison does not offer downgrades accidentally.

## Key-management rule

Platform signing credentials and the Tauri updater private key are different trust anchors. Treat both as production credentials.

Never commit:

- updater private keys,
- updater private-key passwords,
- Apple signing certificates/private keys,
- Windows code-signing private keys,
- release-service access tokens.

If any credential is exposed, rotate/revoke it rather than relying on Git history cleanup alone.
