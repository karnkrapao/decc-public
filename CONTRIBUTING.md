# Contributing to DECC

Thanks for helping improve DECC.

## Preview contribution policy

DECC is currently in preview while its long-term product, licensing, and contributor terms are being finalized.

Bug reports, product feedback, and reproducible issue reports are welcome. Unsolicited code contributions are not accepted by default at this stage. Please do not open a pull request unless a maintainer has explicitly asked for that specific change.

This keeps source ownership and future commercial/licensing options clear while DECC is still early. A broader contribution policy can be published later if external code contributions are opened.

## Reporting bugs

Open a GitHub issue and include, where relevant:

- DECC version or commit,
- operating system,
- workspace/project type,
- clear reproduction steps,
- expected and actual behavior,
- sanitized screenshots or logs.

Do not include sensitive project data or private source code in public reports.

## Feature requests and product feedback

Feature requests are welcome. Describe the workflow or problem you want DECC to improve rather than only proposing a specific implementation.

## Security issues

Do not report vulnerabilities in a public issue. Follow [SECURITY.md](SECURITY.md) and use GitHub private vulnerability reporting.

## Pull requests

If a maintainer explicitly invites a pull request:

1. Keep the change limited to the agreed scope.
2. Do not add dependencies, telemetry, cloud services, licensing changes, or release infrastructure unless the requested change requires them.
3. Run `pnpm verify` before submitting.
4. Do not include generated build output or local configuration.
5. Expect review for product fit, cross-platform behavior, security boundaries, and repository hygiene.

Submitting a pull request does not change the repository's current licensing status or create an open-source license for DECC.
