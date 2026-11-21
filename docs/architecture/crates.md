# Crates: responsibilities, layers and boundary review

The workspace has 31 crates under `crates/` plus `xtask`. `docs/ARCHITECTURE.md`
gives the picture and the data flow; this page states what each crate owns, what
it may depend on, and which boundaries are under review. The authoritative
dependency graph is each crate's `Cargo.toml`; the layer numbers below were
computed from it (a crate's layer is one more than the highest layer it
depends on) and Cargo guarantees there is no cycle.

Sizes are production plus test lines of `src/` (rounded) and are a guide to
where maintenance effort and rebuild cost sit, not a quality measure.

## Layers

| Layer | Crates | Rule |
|---|---|---|
| 0 | `alas-units`, `alas-math`, `alas-i18n`, `alas-fonts`, `alas-config-derive`, `alas-exec`, `alas-testkit`, `alas-uav-db` | No workspace dependencies. No aircraft concepts, except `alas-uav-db` (data schema) and `alas-testkit` (test support only). |
| 1 | `alas-atmo`, `alas-addons` | Environment model; add-on runtime. Depend only on layer 0. |
| 2 | `alas-config` | The one configuration type every model reads. |
| 3 | `alas-geom`, `alas-route`, `alas-prop`, `alas-perf` | Geometry builder, routes, engine cycle, field performance. Read configuration only. |
| 4 | `alas-aero`, `alas-cfd` | Aerodynamics and file-format codecs for AVL, VSPAERO, MSES and FLOWUnsteady; OpenFOAM airfoil studies. |
| 5 | `alas-stab`, `alas-struct`, `alas-mission`, `alas-uav` | Stability, structures, mission solver, UAV selection. |
| 6 | `alas-mass` | Component masses, tanks, fuel policy, dispatch closure. |
| 7 | `alas-payload` | Cabin, cargo and payload layout; reads mass and geometry. |
| 8 | `alas-opt` | Objective evaluation, multidisciplinary loop, differential evolution. |
| 9 | `alas-screen`, `alas-pipeline` | Airfoil screening; stage sequencing, solver orchestration. |
| 10 | `alas-report` | Figure scenes, export, design report. |
| 11 | `alas-viz`, `alas-acceptance` | Scene drawing and rasterisation; acceptance matrices, plausibility tier, benchmark binaries. |
| 12 | `alas-gui` | Desktop application. |
| 13 | `alas-app` | The `ALAS` binary and the headless command line. |

`xtask` is outside the layering: it reads the repository, not the crates'
APIs, and must not be depended on by any crate.

## Responsibilities

| Crate | Owns | Must not own | Size |
|---|---|---|---|
| `alas-units` | Base-SI conversion constants. | Anything dimensional beyond constants. | 0.2k |
| `alas-math` | Splines, Chebyshev operators, small dense linear algebra, root finding. | Aircraft concepts. | 6k |
| `alas-i18n` | English source strings, Spanish catalogue lookup. | Translations of domain text outside the catalogue. | 0.6k |
| `alas-fonts` | Embedded typefaces. | Rendering. | 0.04k |
| `alas-config-derive` | The `ConfigNode` derive macro. | Any runtime behaviour. | 0.5k |
| `alas-exec` | Child-process supervision, tool discovery (`ToolLocator`), the one-click tool installer. | Solver-specific physics or file formats. | 11k |
| `alas-testkit` | Fixture loading, tolerance tiers, the shared test-result store. | Production code paths. | 0.9k |
| `alas-uav-db` | Provenance-tracked UAV component database schema. | UAV physics. | 4k |
| `alas-atmo` | ISA, US 1976 and the reference-model atmosphere. | Per-aircraft values. | 2k |
| `alas-addons` | Add-on manifests, the Rhai sandbox, the process protocol. | Writing into the design (the GUI applies results). | 1.6k |
| `alas-config` | `AlasConfig`, field metadata, defaults, registered presets, validation, retired keys, airport data. | Calculations. | 51k |
| `alas-geom` | Airfoils, wings, fuselages, the aircraft builder. | Masses, aerodynamics. | 18k |
| `alas-route` | Great-circle, airway and flight-plan routes. | Mission physics. | 2k |
| `alas-prop` | On-design turbofan cycle and engine models. | Installation drag (aero). | 13k |
| `alas-perf` | Point and field performance, V-speeds, landing-gear sizing. | Mission integration. | 7k |
| `alas-aero` | VLM, lifting line, airfoil surrogates, drag build-up, native codecs for AVL, VSPAERO, MSES, FLOWUnsteady files; launches MSES through `alas-exec`. | Orchestration of AVL, VSPAERO, OpenVSP, FLOWUnsteady (that is `alas-pipeline`). | 26k |
| `alas-cfd` | OpenFOAM airfoil study contract. | The transport pipeline. | 15k |
| `alas-stab` | Static margin, neutral point, trim, dynamic modes. | Mass ledger. | 5k |
| `alas-struct` | Wingbox loads and sizing, analytical deflection, Nastran and NASTRAN-95 decks, OP2 reader. | Mass-model accounting. | 26k |
| `alas-mission` | Segment-based mission solver. | Fuel policy and dispatch (in `alas-mass`). | 5k |
| `alas-uav` | Electric UAV catalogue and feasibility physics. | Transport-aircraft stages. | 33k |
| `alas-mass` | Component mass methods, ledger, tanks, fuel policy, takeoff-mass closure. | Geometry construction. | 29k |
| `alas-payload` | Cabin, cargo and payload layout and CG. | Cabin drawing (report). | 13k |
| `alas-opt` | Evaluator, envelope checks, MDO loop, L-SHADE. | Pipeline stage sequencing. | 49k |
| `alas-screen` | Airfoil-database batch screening. | Aircraft sizing. | 2k |
| `alas-pipeline` | Stage sequencing, feasibility, CPACS, orchestration of AVL, VSPAERO, OpenVSP, Patran, FLOWUnsteady, run manifests. | Plot rendering. | 47k |
| `alas-report` | Backend-neutral figure scenes, SVG/PNG/PDF export, the design report. | Interactive drawing. | 43k |
| `alas-viz` | Drawing a scene into egui, pan/zoom, rasterising SVG. | Figure content. | 2k |
| `alas-acceptance` | Acceptance matrix, plausibility tier, preset audits, `alas-bench`. | Production logic. | 13k |
| `alas-gui` | The desktop application. | Physics. | 61k |
| `alas-app` | `ALAS` binary, CLI, path resolution. | Anything the GUI and CLI do not share. | 1.4k |

## Boundary review (findings, 2026-10-22)

None of these is a defect that changes behaviour. Each is a candidate change that
needs its own decision record before anyone moves code; none is scheduled.

1. **`alas-config` is both schema and data (51k lines).** Registered presets,
   the aerodrome datasets and the settings schema live together. Every crate
   depends on `alas-config`, so an edit to preset data rebuilds the workspace.
   Candidate: split registered presets and airport datasets into a data crate
   (`alas-presets`) that depends on `alas-config`, leaving the types and
   metadata below it. Cost: preset tests and `at_closure_mass` helpers cross
   the boundary; needs an ADR.
2. **`alas-opt` bundles the search algorithm and the aircraft evaluator (49k).**
   `differential_evolution/` is not a generic leaf today: it imports the
   evaluator, history, cancellation, search-method and MDO modules of the same
   crate. A split would first need an evaluator trait that the algorithm
   depends on; do that refactor inside the crate before considering a new
   crate.
3. **`alas-viz` depends on `alas-report`** (it rasterises SVG and reads the
   `Scene` types). That is a correct direction, but the crate is 2k lines used
   only by `alas-gui`; folding it into `alas-gui` or into a feature of
   `alas-report` would remove a crate. Low priority.
4. **`alas-gui` depends directly on 19 workspace crates**, including
   `alas-aero`, `alas-struct`, `alas-mission` and `alas-perf`. Part of that is
   legitimate (editing configuration of those models); part is the GUI calling
   analysis code that `alas-pipeline` could expose. Reducing direct edges is
   the way to keep physics out of the GUI; review per call site, no mass move.
5. **`alas-mass` depends on `alas-struct`, which depends on `alas-aero`.** The
   mass model therefore sits above the structural and aerodynamic crates in the
   build graph. This follows the data (wingbox mass feeds the ledger) and is
   kept; the cost is that an aerodynamics edit recompiles mass.
6. **Adapters are split by solver, not uniformly.** Pure codecs live with the
   discipline. MSES (`alas-aero/src/mses/exec.rs`) and Nastran/NASTRAN-95
   (`alas-struct/src/nastran/run.rs`, `nastran95/run/process.rs`) are launched
   inside their discipline crates through `alas-exec`; AVL, VSPAERO, OpenVSP and
   FLOWUnsteady are orchestrated from `alas-pipeline`. The mixed pattern is
   deliberate where the solver result is part of the discipline model, but it
   means a reader cannot tell from the crate name where a launch lives; a
   later decision may centralise orchestration.
7. **`alas-acceptance` mixes a library, tests and a dozen binaries.** The
   plausibility tier is a library other crates' tests reach; the preset audits
   and benchmark are tools. They share fixtures, so they stay together; the
   binaries are listed in the capability map's CLI and tool sections.
8. **File layout.** Modules use the `foo.rs` plus `foo/` pattern.
   `docs/source-size-budgets.tsv` lists the files still above the 500-line
   target; it is the work queue for further module splits and may only shrink.

## Rules for a new crate

A new crate needs an ADR (`docs/adr/`), a row in this page, a layer, and a
reason the code cannot live in an existing crate: a different dependency set, a
different build cost profile, or an independent consumer. A crate that is only
the home of one other crate's helper does not qualify.
