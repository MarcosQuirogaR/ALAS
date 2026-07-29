# Security policy

## Reporting a vulnerability

Please report security issues privately, through
[GitHub's private vulnerability reporting](https://github.com/MarcosQuirogaR/ALAS/security/advisories/new)
on this repository. Do not open a public issue for anything exploitable.

Include what you did, what happened, and what you expected. A proof of concept
helps. Expect an acknowledgement within a week; this is a single-maintainer
project, so please allow reasonable time for a fix before disclosing.

## Supported versions

The latest release is supported. Fixes are not backported.

## What is in scope

ALAS is a desktop and command-line engineering application, not a hosted
service. The parts worth scrutinising:

- **The local HTTP sidecar.** The desktop shell talks to a FastAPI server bound
  to `127.0.0.1` on an ephemeral port, with an origin allowlist. Anything that
  lets a remote or cross-origin caller reach it is in scope.
- **Path handling in request bodies.** Config load/save and export endpoints
  accept paths. Traversal outside the intended directory is in scope.
- **Subprocess invocation.** ALAS launches external solvers (MSES, Nastran,
  Patran) and an isolated Python environment. Argument or path injection into
  any of those is in scope.
- **Archive extraction.** Packaged builds unpack embedded archives to a cache
  directory. Path escape during extraction ("zip slip") is in scope.
- **Downloaded assets.** The navdata downloader writes to a per-user data
  directory over HTTPS. Anything allowing a write outside it, or acceptance of
  corrupted data as valid, is in scope.

## What is not in scope

- **External solvers themselves.** MSES, Nastran and Patran are third-party
  programs supplied by the user. Report issues in them to their vendors.
- **Vendored SUAVE.** Report upstream, at the SUAVE project.
- **Numerical accuracy.** A wrong or misleading result is a serious bug and we
  very much want to hear about it — but it is a normal issue, not a security
  one. Open it publicly.
- **Denial of service through deliberately expensive input.** Requesting a huge
  optimizer run is a supported use, not an attack.

## A note on binaries

Released binaries are currently unsigned, so Windows SmartScreen will warn on
first download. Builds are produced by CI from a tagged public commit and can
be reproduced from source. Code signing is planned; until then, verify what you
download against the release page.
