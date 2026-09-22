# ALAS downloads and standalone packaging

The supported downloads are a portable Windows ZIP and a portable Linux
tar.gz, both produced by `cargo xtask dist`. Each contains the native
standalone application, its release metadata, the corresponding AGPL source
snapshot, configuration templates, and the notices needed to audit the
package. No installer or machine-wide runtime registration is required on
either platform.

A "package" can mean three different things, and this page is only ever about
the third one:

1. a **local package** a developer produced on their own machine with `cargo
   xtask dist` (see below) — never distributed automatically;
2. a **CI workflow artifact** produced by the manual `release-preflight.yml`
   (Windows only) or `release-package.yml` (Windows and Linux) GitHub Actions
   workflows on a clean runner and uploaded as a workflow-run artifact for
   reviewers with repository access — reproducibility evidence, not a
   download, and not published; see
   [`release-packaging.md`](release-packaging.md#ci-release-preflight);
3. an **externally published release** — a maintainer-reviewed, tagged
   package a human deliberately publishes for public download. That is the
   only kind of package this downloads page describes below.

## Linux package requirements

The Linux package is built on `ubuntu-22.04` (glibc 2.35) deliberately, not a
newer or rolling image, so it keeps running on any distribution with glibc
2.35 or newer already installed — most currently supported Linux desktop and
server distributions qualify. The desktop application needs the ordinary
desktop graphics stack an X11 or Wayland session already provides (an X11 or
Wayland client library, fontconfig, and a working OpenGL/Vulkan driver — the
same libraries a modern browser or any other `egui`/`wgpu` desktop
application needs); nothing beyond that is required to be installed
separately for a normal desktop session. ALAS also runs fully headless from
the command line (`ALAS --config <file> --output <dir> ...`, `ALAS --help`)
for scripted or server-side use, with no display required at all. The exact
system packages this project installs to *build* the Linux package in CI are
listed in [`release-packaging.md`](release-packaging.md#ci-release-package-windows-and-linux);
they are build-time dependencies, not something an end user extracting the
package needs to install.

## What a standalone package contains

After extraction, start `ALAS.exe` (Windows) or `./ALAS` (Linux) from the
package directory. Keep the directory intact: the executable resolves its
adjacent `configs/`, source manifest, notices, and any separately distributed
solver directories relative to the package. The exact archive name is
version- and target-specific, for example `alas-v1.2.0-windows-x86_64.zip` or
`alas-v1.2.0-linux-x86_64.tar.gz`; the release manifest records the actual
version, target, source revision, dirty-worktree flag, and hashes.

Every accepted package includes:

- `ALAS.exe` or `ALAS`, the native desktop application;
- `RELEASE-MANIFEST.json`, including byte counts, SHA-256 hashes, and the
  status of every adjacent external tool;
- `SOURCE-MANIFEST.json` and the matching `source/` AGPL snapshot;
- `configs/ave.yaml`, a generated configuration template;
- `README.md`, `LICENSE`, `NOTICE`, and `THIRD-PARTY-NOTICES.md`.
- `assets/mses/osmapDP.dat` plus its exact XFOIL source archive, GPL text,
  provenance README, and acquisition script. This is transition data for an
  installed MSES process; it is not the MSES executable bundle.

The archive also has a sibling `.zip.sha256` (Windows) or `.tar.gz.sha256`
(Linux) file. Verify that checksum before extracting a download. The release
task always validates `--help`, configuration round-tripping, and a headless
run through a path containing spaces before it writes the archive; on the
Windows package it additionally requires the bundled AVL executable to
produce its total-force evidence (see "External tools and redistribution"
below for why the Linux package does not carry that same requirement).
Packaging metadata is provenance, not a claim that every optional discipline
has converged or that the aircraft is certified.

## External tools and redistribution

External programs remain process-boundary dependencies. A download must not
imply that an executable is included merely because ALAS has a configuration
field for it. The manifest distinguishes a packaged tool from a deliberately
user-supplied or unavailable one.

- AVL 3.52 may be aggregated when the package carries its unchanged
  executable, corresponding source archive, and GPL-2.0 notice. Today that is
  the Windows package alone, where this project has actually reviewed and
  tested the win32 build; its distribution validation requires this child
  executable for the independent aerodynamic cross-check. The Linux package
  ships the same GPL source archive and licence text plus
  `external tools/AVL-LINUX-BUILD.txt`, a written note pointing at the
  archive's own `gfortran` build targets, and is recorded `not_bundled` with
  that reason; ALAS runs its own analytical vortex-lattice stage there and
  the AVL cross-check is simply absent from Model Comparison until a built or
  acquired executable is configured under Tools. This project does not build
  AVL from that source unattended in CI, and never ships the Windows
  executable inside a package for a platform it cannot run on.
- NASTRAN-95 is optional. It may be included only when the complete reviewed
  NOSA 1.3 staging tree is present: executable, rigid formats, runtime DLL
  inventory, licence, modification record, source archive, and source/build
  association. Incomplete staging is omitted and recorded as `not_bundled`;
  strict opt-in mode fails closed. See [the NASTRAN-95 bundle
  policy](NASTRAN95-BUNDLE.md).
- MSES (`mset`, `mses`, and `mplot`) is not redistributed when the governing
  MIT per-seat licence does not authorize that use. It remains a user-supplied
  installation. The compatible GPL double-precision `osmapDP.dat` transition
  map is bundled separately under `assets/mses/` and is passed to the
  installed solver by absolute path, so free-transition runs do not depend on
  the user's working directory. Screening still reports the missing/
  unconfigured executable installation rather than fabricating a result.
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

The command writes `dist/<package>/` and then creates the archive (ZIP on
Windows, tar.gz elsewhere) only after the standalone validation succeeds.
This command builds for whatever platform it runs on: there is no Linux
cross-build from a Windows checkout, so a Linux package can only be produced
by running this command on Linux, locally or in CI. To require a fully
reviewed NASTRAN-95 bundle instead of accepting an omitted optional tool, set
`ALAS_STRICT_BUNDLED_NASTRAN95=1` before running the command. Review
[`release-packaging.md`](release-packaging.md) for the source allowlist,
manifest checks, and the boundary between packaging verification and physical
solver qualification.

A locally produced package reflects whatever is on that machine, including an
uncommitted change (recorded via the manifest's dirty-worktree flag). To
reproduce packaging from a clean checkout and the committed `Cargo.lock`
instead, run the manual `release-package.yml` workflow (Windows and Linux) or
the Windows-only `release-preflight.yml` workflow from the Actions tab and
download its workflow-run artifact; see [CI release
package](release-packaging.md#ci-release-package-windows-and-linux). That
artifact is still not a published release.

## Accepting a package locally

The packaging task validates the package it just wrote. Two opt-in suites
re-check an assembled package as an external artifact, reading only what the
package directory contains. Point them at the directory, not the archive:

```powershell
$env:ALAS_W55_PACKAGE_DIR = "dist/alas-v1.2.0-windows-x86_64"
cargo test -p alas-acceptance --test distribution_license_boundary
cargo test -p alas-acceptance --test distribution_acceptance
```

(On Linux, `export ALAS_W55_PACKAGE_DIR=dist/alas-v1.2.0-linux-x86_64` and the
same two `cargo test` commands; both suites read whatever package directory
the variable names, on either platform.)

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
