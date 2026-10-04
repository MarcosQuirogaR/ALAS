# Architecture

ALAS sizes and analyses a conceptual transport aircraft. A design vector goes
in; a trimmed, mass-balanced aircraft with a flown mission, a drag build-up, a
sized wingbox and a set of figures comes out.

This document describes how the Rust workspace is arranged. The models are
described in `docs/methods.md`, the numerical-parity and licence record against
the original Python implementation is `docs/PORTING.md`, and what works today
is `docs/STATUS.md`.

---

## Shape of the program

One executable (`ALAS`, from `alas-app`). No server, no interpreter, no
sidecar process. The desktop interface calls the library directly; the headless
command line is a second thin caller of the same pipeline. External solvers are
optional child processes.

```
                     alas-app  (ALAS binary: GUI or headless CLI)
                         |
        alas-gui --------+-------- alas-report --- alas-viz
   (desktop application)               (figures, SVG/PDF)
      |   |   |   \                        |
      |   |   |    alas-cfd, alas-uav      |
      |   |   |    (OpenFOAM airfoil       |
      |   |   |     studies; electric UAV  |
      |   |   |     selection)             |
      |   |   |                            |
      +---+---+------ alas-pipeline -------+
                       |
        alas-opt   alas-screen   alas-struct   alas-mission
            \          |             |            /
             alas-mass  alas-payload  alas-stab  alas-perf
                       alas-aero   alas-prop
                            |
      alas-geom   alas-route   alas-exec
                            |
             alas-config   alas-atmo
                            |
   alas-units  alas-math  alas-i18n  alas-config-derive
```

Arrows in the sketch are indicative; the authoritative graph is each crate's
`Cargo.toml`, which Cargo keeps acyclic. Nothing below `alas-gui` knows a user
interface exists.

---

## Crate map

Crates are organised by discipline. Dependencies listed are the workspace
crates each one uses in `[dependencies]`.

**Foundations (no ALAS domain concepts)**

| Crate | Role |
|---|---|
| `alas-units` | Conversion factors to base SI. |
| `alas-math` | Splines, Chebyshev operators, small dense linear algebra, root finding. |
| `alas-i18n` | English source strings and the Spanish catalogue lookup. |
| `alas-fonts` | Bundled typefaces shared by the GUI and the SVG rasteriser. |
| `alas-config-derive` | `#[derive(ConfigNode)]`, the settings-metadata macro. |
| `alas-exec` | Child-process supervision for external solvers. |
| `alas-testkit` | Fixture loading and tolerance assertions for parity tests. |

**Description of an aircraft and its environment**

| Crate | Role | Depends on |
|---|---|---|
| `alas-atmo` | ISA, US 1976 and the fitted atmosphere used by the reference model. | math |
| `alas-config` | Every tunable value, the registered aircraft presets, fuel policy, airport data, and the settings metadata. | atmo, config-derive, i18n |
| `alas-geom` | Airfoils, wings, fuselages and the aircraft builder. | config, math |
| `alas-route` | Great-circle, airway and flight-plan routes; navigation data assets. | config |

**Disciplinary analyses**

| Crate | Role | Depends on |
|---|---|---|
| `alas-aero` | In-process and mission vortex-lattice methods, airfoil and lift surrogates, drag build-up. | atmo, config, geom, math, exec |
| `alas-prop` | On-design turbofan cycle. | atmo, config |
| `alas-mass` | Component mass methods (Torenbeek, FLOPS transport), item-level mass ledger, tanks, fuel policy, dispatch closure. | geom, config, struct, units |
| `alas-payload` | Cabin, cargo and baggage layout; payload centre of gravity. | geom, config, mass, math |
| `alas-stab` | Static margin, neutral point, trim, dynamic modes. | aero, config, geom, atmo, math |
| `alas-perf` | Point and field performance, V-speeds, landing-gear sizing. | config, atmo |
| `alas-mission` | Segment-based mission solver. | aero, atmo, config, math, prop |
| `alas-struct` | Wingbox loads and sizing, analytical deflection, Nastran and NASTRAN-95 decks, OP2 reader. | config, exec, geom |
| `alas-uav` | Fixed-wing electric UAV component catalogue and feasibility physics. | geom, aero, atmo |
| `alas-cfd` | Two-dimensional OpenFOAM airfoil study contract (Gmsh extrusion, case lifecycle, parsers). | exec, geom |

**Composed analyses, orchestration and output**

| Crate | Role | Depends on |
|---|---|---|
| `alas-opt` | Objective evaluation, envelope checks, multidisciplinary sizing loop, L-SHADE differential evolution. | types, config, geom, atmo, mass, struct, payload, perf, aero, prop, stab, math, units, mission |
| `alas-screen` | Airfoil-database batch screening. | config, geom, atmo, mass, aero, stab, opt |
| `alas-pipeline` | Stage sequencing, feasibility, CPACS and OpenVSP/AVL/VSPAERO adapters, run manifests. | config, geom, atmo, mass, payload, perf, aero, stab, opt, struct, mission, prop, route, math, exec, units |
| `alas-report` | Figure scenes, SVG/PNG/PDF export, design report. | most analysis crates, screen, opt, pipeline |
| `alas-viz` | Draws a scene into egui, with pan, zoom and fit. | fonts, report |
| `alas-gui` | The desktop application (egui/wgpu), including the CFD and UAV pages. | pipeline, report, viz, opt, screen, cfd, uav and others |
| `alas-app` | The `ALAS` binary and the headless command line. | config, gui, report, pipeline, exec, route |

**Verification**

| Crate | Role |
|---|---|
| `alas-acceptance` | End-to-end acceptance matrix over the registered presets, preset audits and the benchmark binary (`alas-bench`). |
| `xtask` | Repository tasks (`cargo xtask`): gate, checks, evidence audit, benchmarks, packaging. |

---

## Data flow of a design run

`alas-pipeline::pipeline` runs seven numbered stages (the stage names are the
identifiers in the run log), after configuration and geometry are resolved.

0. **Configuration.** `alas-config` resolves a preset or user YAML into one
   `AlasConfig`: design vector, geometry scaffold, requirements, engine, mass
   model, fuel policy and solver settings. Every computational crate reads its
   numbers from it.
   **Geometry.** `alas-geom` builds the aircraft (wings, fuselage, tails, gear
   scaffold) from the design vector. Nothing downstream re-derives a planform.
1. **`baseline`.** Fast weight-and-balance and stability estimate
   (`alas-mass`, `alas-payload`, `alas-perf`, `alas-stab`).
2. **`optimization`.** `alas-opt` searches the design vector with L-SHADE
   differential evolution under the epsilon-constrained method. Each
   candidate is closed as a multidisciplinary analysis: geometry and mass build,
   aerodynamics (`alas-aero`), propulsion (`alas-prop`), mission sizing and fuel
   closure (`alas-mission`, `alas-mass::dispatch`), trim and CG envelope
   (`alas-stab`), payload layout (`alas-payload`) and structural feasibility
   (`alas-struct`). Optimisation can be skipped.
3. **`full_analysis`.** Fine vortex-lattice polars, trim, neutral point, CG
   envelope, mass and CG anchor for the baseline and optimised designs.
4. **`geometry_export`.** CPACS export and geometry canonicalisation; design
   database and Selig airfoil files.
5. **`downstream`.** Runs concurrently: flown mission at the fuel-policy
   takeoff mass with lateral routing (`alas-mission`, `alas-route`), MSES
   two-dimensional transonic analysis, wingbox sizing, mesh and deck
   construction, analytical deflection and optional Nastran or NASTRAN-95
   solves (`alas-struct`), and the optional VSPAERO, AVL and FLOWUnsteady
   comparisons.
6. **`feasibility`.** Physical feasibility assessment over all results,
   producing typed findings rather than a single pass/fail.
7. **`finalization`.** Artifacts and the run manifest.

Reporting sits on top: `alas-report` turns results into backend-neutral scenes
and the design report; `alas-viz` draws them in the GUI and the export path
writes SVG, PNG and PDF.

`alas-screen` (airfoil screening), `alas-cfd` (OpenFOAM airfoil studies) and
`alas-uav` (electric UAV selection) are separate workflows launched from the
GUI; they are not stages of the transport-aircraft pipeline.

---

## Four mechanisms worth understanding

### 1. Typed analysis status

Every analysis that can fail to happen reports a typed status of its own (for
example `MsesStatus`, `VspaeroAnalysisStatus`, `ResultStatus`) that
distinguishes a result, an `Error` and `NotRun`. An analysis that could not
run says so and says why; it never returns a plausible number instead. A figure whose data is `NotRun`
renders as a stated absence, not an empty axis. A missing MSES installation is
`NotRun` and expected; an MSES run that diverged is `Error` and is not.

### 2. Configuration metadata drives the interface

Configuration structs carry per-field metadata (label, unit, help text, bounds)
declared once with `#[derive(ConfigNode)]` and `#[config(...)]`. The settings
forms, the YAML round trip and the translation keys all derive from that single
declaration. The macro refuses to compile a public field that has neither
metadata nor an explicit `skip`, which enforces the rule that anything a user
might reasonably tune is a visible, labelled configuration field.

### 3. Figures are scenes, not drawings

A figure produces a backend-neutral description (polylines, polygons, text,
images, axes) and never touches a drawing API. `alas-viz` renders a scene into
an egui panel; the export path renders the same scene to SVG, then PNG and PDF.
A figure is therefore a pure function from results to geometry, and tests check
the scene's numbers rather than its pixels. Three-dimensional views are
projected on the CPU into ordinary scene geometry.

### 4. External solvers are a process boundary

MSES, MSC Nastran, NASTRAN-95, AVL, VSPAERO, OpenVSP, OpenFOAM/Gmsh and
Patran are separate executables, all optional. `alas-exec` owns launching and
supervising them (on Windows every child is placed in a kill-on-close Job
Object, so ending ALAS ends them too). Adapters live beside the discipline they
serve (`alas-struct` for Nastran, `alas-pipeline` for AVL, VSPAERO, OpenVSP and
Patran, `alas-cfd` for OpenFOAM). The rest of the program sees each adapter's typed status;
where an analytical answer exists it is computed without the solver, and where
it does not, the absence is reported.

---

## Concurrency

The pipeline runs independent stages concurrently, the optimiser evaluates
candidates on a bounded worker pool, and airfoil screening evaluates candidates
in parallel. In the product profile search results do not depend on the worker count: a generation's
trial vectors are built in fixed order from the seeded stream and evaluated in
index order. Cancellation is a shared flag checked inside the search and at
stage boundaries.

---

## Deliberate departures from the Python implementation

Each has a row in `docs/PORTING.md`.

- **The HTTP sidecar and its schema, route and run-management modules.** The
  interface calls the library directly.
- **The lazy-import machinery.** A native binary has no import cost to defer.
- **PyVista.** The 3-D globe is drawn natively from scene geometry.
- **Patran rendering.** Deformed-wingbox images are drawn natively from the displacements read from the results file; the optional Patran adapter (`alas-pipeline::patran`) only exports images when Patran is installed.
