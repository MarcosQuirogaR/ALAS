# 0009. Release process and package kinds

Status: Accepted (2026-10-22)

## Context

A downloadable package can be mistaken for a release, a build for a
reproducible artifact, and a bundled tool for a licensed one. Several external
tools have licences that decide what may travel with the program.

## Decision

1. `cargo xtask dist` builds the release binary from the committed lockfile
   (`--locked`) and assembles a portable package: `.zip` on Windows,
   `.tar.gz` elsewhere. It validates the package by running it (`--help`,
   config export and read, a headless run from a path containing spaces)
   before the archive is created.
2. Every package carries `RELEASE-MANIFEST.json` (version, target, channel,
   source commit, dirty flag, tool statuses, hashes) and
   `SOURCE-MANIFEST.json` (the source snapshot, assembled from a positive
   allowlist). A dirty tree is recorded so an unreleased build cannot pass as
   a tagged release.
3. Three things are never conflated: a local package, a CI workflow artifact
   (reproducibility evidence with limited retention) and an externally
   published release. CI never publishes; publication is a deliberate,
   human-gated action by the maintainer.
4. Linux packages are built on `ubuntu-22.04` (glibc 2.35) to keep the glibc
   floor stable; the Windows package is built on `windows-latest`.
5. Tool bundling follows licence: AVL (GPL) travels as an adjacent child
   process on Windows only, where the executable was reviewed; Linux ships the
   same source and licence with a build note. MSES, MSC Nastran, MSC Patran,
   VSPAERO and FLOWUnsteady are user-supplied. NASTRAN-95 is bundled only when
   its source, notices and runtime inventory pass the fail-closed check
   (`ALAS_STRICT_BUNDLED_NASTRAN95=1`). Each outcome is recorded in the
   manifest with its reason.
6. Internal planning material must not reach a release. The source snapshot
   admits `docs/**` by file extension and has no rejection for internal
   folders, so this is not yet enforced by `xtask dist`; closing that is an open
   defect (reject the folder in `source_path_allowed`, with a test).
7. Dependency policy (`cargo deny`, licences, advisories, sources) runs in CI
   beside the gate; exceptions name advisory ids and fail when unused.

## Consequences

- Every release needs a human decision to publish; there is no automatic
  channel.
- Adding a bundled tool means a licence review and an entry in the allowlist,
  the manifest and `THIRD-PARTY-NOTICES.md`.

## Evidence

`docs/release-packaging.md`, `xtask/src/dist.rs`, `.github/workflows/release-package.yml`,
`docs/dependency-policy.md`, `docs/downloads.md`.
