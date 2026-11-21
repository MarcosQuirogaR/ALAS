# 0002. Workspace and crate structure

Status: Accepted (2026-10-22)

## Context

ALAS is one program with several physics disciplines, a pipeline, a report
layer and a desktop interface. Compile time, test selection and ownership all
depend on where the boundaries sit.

## Decision

1. One Cargo workspace (`crates/*` and `xtask`), one lockfile, shared
   `[workspace.dependencies]`, one pinned toolchain (`rust-toolchain.toml`).
2. One crate per discipline or layer, acyclic by construction. The layers are
   foundations, description (`alas-config`, `alas-geom`), disciplines
   (aero, prop, mass, struct, stab, perf, mission, payload), composition
   (`alas-opt`, `alas-pipeline`), output (`alas-report`, `alas-viz`) and
   application (`alas-gui`, `alas-app`). `docs/architecture/crates.md` lists
   each crate's layer and responsibilities.
3. Nothing below `alas-gui` knows a user interface exists; the headless CLI
   and the GUI call the same pipeline.
4. `alas-config` is the single configuration type every model reads. Models
   take their tunable numbers from it; constants inside a model are sourced
   correlation coefficients, not user-adjustable assumptions.
5. Adapters to external programs follow the discipline that owns the file formats.
   Pure codecs (deck writers, force parsers) live with the discipline
   (`alas-aero`, `alas-struct`). MSES and Nastran/NASTRAN-95 are also launched
   inside their discipline crates (`alas-aero/src/mses/exec.rs`,
   `alas-struct/src/nastran/run.rs`) through the `alas-exec` supervision
   primitives; AVL, VSPAERO, OpenVSP and FLOWUnsteady are orchestrated from
   `alas-pipeline`. Process supervision itself is always `alas-exec`.
6. `xtask` is not a dependency of any crate.
7. A new crate needs an ADR and a reason it cannot live in an existing crate
   (a different dependency set, build-cost profile or independent consumer).

## Consequences

- Cargo enforces the acyclic rule; the boundary review in
  `docs/architecture/crates.md` records places where the layering is
  uncomfortable (a 51k-line `alas-config`, a 49k-line `alas-opt`, a GUI with 19
  direct edges). Those are candidates for later ADRs, not defects.
- A configuration change rebuilds most of the workspace; that cost is accepted
  for the single-source-of-numbers property.

## Evidence

`Cargo.toml`, `docs/ARCHITECTURE.md`, `docs/architecture/crates.md`.
