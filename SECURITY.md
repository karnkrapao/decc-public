# Security Policy

## Reporting a security issue

Please do **not** open a public GitHub issue for vulnerabilities, exposed credentials, signing problems, or reports that include sensitive local-project data.

Use GitHub's private security reporting flow at `Security` → `Advisories` → `Report a vulnerability`, or open the private report form at <https://github.com/karnkrapao/decc-public/security/advisories/new>.

A useful report includes:

- affected DECC version or commit,
- operating system,
- clear reproduction steps,
- expected vs actual behavior,
- security impact,
- relevant logs with secrets and personal paths removed.

## Sensitive data

DECC works with local developer projects, processes, logs, environment metadata, ports, and Git state. Security reports should avoid attaching real source code, secret values, private keys, access tokens, or full environment files unless a private maintainer channel explicitly requires them.

If a credential has been exposed, revoke or rotate it immediately. Removing a value from Git history is not a substitute for credential rotation.

## Release trust

Production distributions are expected to pass platform-specific signing checks:

- macOS: Developer ID signing and Gatekeeper/notarization validation,
- Windows: valid Authenticode signatures,
- future in-app updater: Tauri updater signature verification in addition to platform signing.

Unsigned or ad-hoc release-candidate artifacts are for validation only and are not production distribution artifacts.

## Supported versions

Until the first production release is published, security fixes target the current pre-release development snapshot.

After public releases begin, the latest stable release will be the primary supported version unless the release notes state otherwise.
