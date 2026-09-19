# ALAS downloads and standalone packaging

The supported Windows download is a portable ZIP produced by `cargo xtask
dist`. It contains the native standalone application, its release metadata,
the corresponding AGPL source snapshot, configuration templates, and the
notices needed to audit the package. No installer or machine-wide runtime
registration is required.

## What a standalone package contains

After extraction, start `ALAS.exe` from the package directory. Keep the
directory intact: the executable resolves its adjacent `configs/`, source
manifest, notices, and any separately distributed solver directories relative
to the package. The exact archive name is version- and target-specific, for
example `alas-v1.1.0-windows-x86_64.zip`; the release manifest records the
actual version, target, source revision, dirty-worktree flag, and hashes.

Every accepted package includes:

- `ALAS.exe`, the native desktop application;
- `RELEASE-MANIFEST.json`, including byte counts, SHA-256 hashes, and the
  status of every adjacent external tool;
- `SOURCE-MANIFEST.json` and the matching `source/` AGPL snapshot;
- `configs/ave.yaml`, a generated configuration template;
- `README.md`, `LICENSE`, `NOTICE`, and `THIRD-PARTY-NOTICES.md`.

The archive also has a sibling `.zip.sha256` file. Verify that checksum before
extracting a download. The release task validates `--help`, configuration
round-tripping, a headless run through a path containing spaces, and the
required AVL evidence before it writes the archive. Packaging metadata is
provenance, not a claim that every optional discipline has converged or that
the aircraft is certified.

## External tools and redistribution

External programs remain process-boundary dependencies. A download must not
imply that an executable is included merely because ALAS has a configuration
field for it. The manifest distinguishes a packaged tool from a deliberately
user-supplied or unavailable one.

- AVL 3.52 may be aggregated when the package carries its unchanged
  executable, corresponding source archive, and GPL-2.0 notice. The current
  distribution validation requires this child executable for the independent
  aerodynamic cross-check.
- NASTRAN-95 is optional. It may be included only when the complete reviewed
  NOSA 1.3 staging tree is present: executable, rigid formats, runtime DLL
  inventory, licence, modification record, source archive, and source/build
  association. Incomplete staging is omitted and recorded as `not_bundled`;
  strict opt-in mode fails closed. See [the NASTRAN-95 bundle
  policy](NASTRAN95-BUNDLE.md).
- MSES (`mset`, `mses`, and `mplot`) is not redistributed when the governing
  MIT per-seat licence does not authorize that use. It remains a user-supplied
  installation, and Screening reports the missing/unconfigured Stage 3 state
  rather than fabricating a result.
- OpenVSP/VSPAERO, MSC Nastran, MSC Patran, and FLOWUnsteady/Julia remain
  user-supplied unless a specific release has been reviewed for redistribution
  and its notices are carried with the exact archive. Do not copy local
  `external tools/` contents into a public package by default.

The complete inventory and licence notes are in
[`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md). A website download page
should link to the archive checksum and retain the release manifest alongside
the file; it should not advertise an optional solver as bundled unless the
manifest says `bundled`.

## Building a release package locally

From the ALAS checkout:

```powershell
cargo xtask dist
```

The command writes `dist/<package>/` and then creates the ZIP only after the
standalone validation succeeds. To require a fully reviewed NASTRAN-95 bundle
instead of accepting an omitted optional tool, set
`ALAS_STRICT_BUNDLED_NASTRAN95=1` before running the command. Review
[`release-packaging.md`](release-packaging.md) for the source allowlist,
manifest checks, and the boundary between packaging verification and physical
solver qualification.

## Accepting a package locally

The packaging task validates the package it just wrote. Two opt-in suites
re-check an assembled package as an external artifact, reading only what the
package directory contains. Point them at the directory, not the archive:

```powershell
$env:ALAS_W55_PACKAGE_DIR = "dist/alas-v1.1.0-windows-x86_64"
cargo test -p alas-acceptance --test distribution_license_boundary
cargo test -p alas-acceptance --test distribution_acceptance
```

`distribution_license_boundary` is the redistribution check: no packaged file
is one of the programs this project has no licence to redistribute (MSES's
`mset`/`mses`/`mplot`, OpenVSP/VSPAERO, the MSC tools), every one of those is
recorded `user_supplied` in `RELEASE-MANIFEST.json` with a reason pointing at
the notices, a bundled tool ships the source and licence its terms require, an
omitted NASTRAN-95 ships no staging at all, and the package carries its own
AGPL source snapshot with no excluded path smuggled back into it.
`distribution_acceptance` runs the packaged executable across the preset
matrix from a copy of the package with no repository checkout in reach.

Both are packaging acceptance. They establish what the download contains and
whether its manifest describes it honestly. They are not a desktop review of
the application's windows, and they say nothing about whether any discipline's
numerical result has been physically validated.
