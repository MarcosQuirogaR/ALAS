# How ALAS works inside

The internal structure of ALAS: crate layers, the stages of a run and the main design mechanisms.

---

## Shape of the program

ALAS is compiled in Rust as a single native executable (`ALAS.exe` on Windows,
`ALAS` on Linux). There is no interpreter and no server process.

```
        alas-app  (ALAS.exe)
             |
        alas-gui  ------------- alas-viz
             |                      |
        alas-pipeline  ---------- alas-report
             |
  +----------+----------+----------+----------+
  |          |          |          |          |
alas-opt  alas-mission alas-struct alas-screen ...
  |          |          |          |
  +----------+----------+----------+
             |
     the physics crates
             |
   alas-geom, alas-atmo, alas-math, alas-units
```

The core is strictly **interface-agnostic**: crates below `alas-gui` have no
knowledge that a user interface exists. Both the headless CLI (`alas-app`) and
the desktop GUI (`alas-gui`) are callers of `alas-pipeline`.

---

## Crate layering

The workspace is organized into shallow, acyclic crates:

| Tier | Crates | Responsibilities |
|---|---|---|
| **L0 Foundations** | `alas-units`, `alas-math`, `alas-i18n`, `alas-config-derive` | Physical unit conversions, root finders, splines, localization |
| **L1 Environment** | `alas-config`, `alas-atmo` | Authoritative configuration schemas (typed settings with per-field metadata), the aircraft preset registry, atmosphere models |
| **L2 Geometry & Execution** | `alas-geom`, `alas-route`, `alas-exec` | Airfoil coordinates, wing/fuselage generators, structural mesh, airway routing, external process execution |
| **L3 Disciplinary Analyses** | `alas-aero`, `alas-prop`, `alas-mass`, `alas-stab`, `alas-perf`, `alas-payload` | Vortex-lattice aerodynamics, turbofan thermodynamic cycle, CG envelopes, longitudinal stability |
| **L4 Composed Stages** | `alas-mission`, `alas-struct`, `alas-opt`, `alas-screen` | Native mission segment solver, wingbox structural sizing, optimization algorithms, airfoil screening |
| **L5 Orchestration & Output** | `alas-pipeline`, `alas-report` | Multidisciplinary stage sequencing, concurrent execution, figure scene definitions |
| **L6 Presentation** | `alas-viz`, `alas-gui`, `alas-app` | Hardware-accelerated GUI rendering (egui), desktop window (guided workspace and [Sandbox mode](sandbox.md)), headless CLI binary |
| **Side workflows** | `alas-cfd`, `alas-uav` | OpenFOAM airfoil studies and electric-UAV component selection, launched from the GUI; not stages of the transport-aircraft pipeline |

---

## The stages of a design run

After the configuration and geometry are resolved, `alas-pipeline` runs seven
numbered stages (the stage names are the identifiers in the run log):

| Stage | What it does |
|---|---|
| 1. `baseline` | Fast weight-and-balance and stability estimate of the starting design |
| 2. `optimization` | Differential-evolution search; each candidate is a closed multidisciplinary analysis (geometry, mass, aerodynamics, propulsion, mission sizing and fuel closure, trim and CG envelope, payload layout, structural feasibility). Can be skipped |
| 3. `full_analysis` | Fine vortex-lattice polars, trim, neutral point, CG envelope, mass and CG for the baseline and the optimized design |
| 4. `geometry_export` | CPACS export, design database and airfoil files |
| 5. `downstream` | Runs concurrently: the flown mission with lateral routing (native, in-process), MSES section analysis, wingbox sizing and finite-element solves, optional VSPAERO, AVL and FLOWUnsteady comparisons |
| 6. `feasibility` | Physical feasibility assessment over all results, as typed findings rather than one pass or fail |
| 7. `finalization` | Artifacts and the run manifest |

---

## Key architectural mechanisms

### 1. Typed analysis status

Every disciplinary analysis that can fail or be omitted reports a typed status
of its own that distinguishes a result, an error and `NotRun`.

This enforces **honest degradation**:
- An absent external solver reports `NotRun` and the pipeline continues with
  analytical fallbacks.
- A solver that diverges or exceeds iteration limits reports `Error` with diagnostic
  transcripts preserved.
- Missing optional stages render as stated absences rather than empty axes or
  fabricated default values.
- Partial numerical results identify the run and analysis stage that produced them.
  Check the status before using a value from an unfinished analysis.
- In external solvers such as MSES, finite contour files alone are not convergence;
  accepted pressure data require recorded native convergence, and failed attempts
  are retained as diagnostics.

### 2. Compile-time configuration metadata

Every tunable design setting is declared once in `alas-config` with
`#[derive(ConfigNode)]` and metadata attributes (label, unit, bounds, tooltip).

From this single source of truth, ALAS generates:
- Graphical settings controls in the desktop GUI.
- The `--save-config` output.
- Serialization and deserialization for YAML and JSON configuration files.


### 3. Figures as vector scenes

A figure produces a backend-neutral scene description (polylines, polygons,
text, coordinates, and axes).

- `alas-viz` renders the scene interactively into an `egui` GPU surface.
- `alas-report` renders the identical scene to vector SVG and raster PNG files
  for export.
- Because figure generation is a pure function from analysis results to vector
  primitives, figures are completely deterministic and parity-testable.

### 4. External solver isolation

External tools (MSES, MSC Nastran, NASTRAN-95, AVL) are invoked as isolated child
processes by `alas-exec`.

- `alas-exec` prepares input decks, manages execution timeouts, and terminates child
  processes when a run ends or is cancelled.
- Solvers report structured outputs or error logs; if an external solver is not
  installed, ALAS reports the stage unavailable without crashing.

### 5. The sandbox is a second case in the same state

The desktop application keeps one authoritative configuration that every view
reads. [Sandbox mode](sandbox.md) does not fork the views: entering it moves the
whole guided case (configuration, design vector, run log, results) aside and
installs the sandbox aircraft in the same state fields, so the same forms,
validation and Full Analysis apply. Leaving restores the guided case
(*discard*) or keeps the drawn aircraft as the guided workspace's custom
baseline (*promote*). The quick estimates come from an in-process
`alas-pipeline::quick_analysis` stage that closes the fixed aircraft's mass and
fuel, then runs the same baseline analysis the Full Analysis runs, so each
published value can be tagged as the Full Analysis' own or as an estimate.
Every result carries the configuration revision it was computed for, and a
late result for older geometry is dropped.

---

## Concurrency & execution

Independent analysis stages execute concurrently in parallel worker threads:
- Native mission analysis, 2D section diagnostics, and wingbox structural sizing
  run concurrently during post-optimization passes.
- Airfoil screening parallelizes candidate evaluation across all available CPU cores.
- Figure rendering contains no global drawing locks, ensuring high responsiveness
  during interactive visualization.
