# How ALAS works inside

This chapter explains the internal architecture of ALAS. If you are inspecting
the codebase, debugging a run, or want to understand what happens between
launching an analysis and generating figures, this is the map.

---

## Shape of the program

ALAS is compiled in Rust as a single native executable (`alas.exe` on Windows,
`alas` on Linux). There is no interpreter, no loopback HTTP sidecar, and no
separate server process.

```
        alas-app  (alas.exe)
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
| **L0 Foundations** | `alas-units`, `alas-math`, `alas-types`, `alas-i18n`, `alas-config-derive` | Physical unit conversions, root finders, splines, stage status contracts, localization |
| **L1 Environment** | `alas-config`, `alas-atmo` | Authoritative configuration schemas (~5,600 lines of typed settings), atmosphere models |
| **L2 Geometry & Execution** | `alas-geom`, `alas-route`, `alas-exec` | Airfoil coordinates, wing/fuselage generators, structural mesh, airway routing, external process execution |
| **L3 Disciplinary Analyses** | `alas-aero`, `alas-prop`, `alas-mass`, `alas-stab`, `alas-perf`, `alas-payload` | Vortex-lattice aerodynamics, turbofan thermodynamic cycle, CG envelopes, longitudinal stability |
| **L4 Composed Stages** | `alas-mission`, `alas-struct`, `alas-opt`, `alas-screen` | Native mission segment solver, wingbox structural sizing, optimization algorithms, airfoil screening |
| **L5 Orchestration & Output** | `alas-pipeline`, `alas-report` | Multidisciplinary stage sequencing, concurrent execution, figure scene definitions |
| **L6 Presentation** | `alas-viz`, `alas-gui`, `alas-app` | Hardware-accelerated GUI rendering (egui), desktop window, headless CLI binary |

---

## Key architectural mechanisms

### 1. The stage status contract

Every disciplinary analysis that can fail or be omitted returns a `Stage<T>`:
`Ok(T)`, `Error(String)`, or `NotRun`.

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
- CLI argument parsing and overrides.
- Serialization and deserialization for YAML and JSON configuration files.

Configuration metadata controls supported inputs, while numerical and model
assumptions remain documented in code.

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

---

## Concurrency & execution

Independent analysis stages execute concurrently in parallel worker threads:
- Native mission analysis, 2D section diagnostics, and wingbox structural sizing
  run concurrently during post-optimization passes.
- Airfoil screening parallelizes candidate evaluation across all available CPU cores.
- Figure rendering contains no global drawing locks, ensuring high responsiveness
  during interactive visualization.
