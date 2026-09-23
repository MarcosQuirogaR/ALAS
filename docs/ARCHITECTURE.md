# Architecture

ALAS sizes and analyses a conceptual transport aircraft. A design vector goes
in; a trimmed, mass-balanced aircraft with a flown mission, a drag build-up, a
sized wingbox and sixty-odd figures comes out.

This document describes how that is arranged in Rust. It does not describe the
models (those are in `docs/methods.md`) and it does not describe the
translation from Python, which is in `docs/PORTING.md`.

---

## Shape of the program

One executable. No server, no interpreter, no sidecar process, no bundled
runtime. The Python implementation ran a FastAPI process that the desktop shell
talked to over loopback HTTP, and rendered every figure as an SVG on that
process; all of that is gone.

What remains is a library, a user interface drawn on top of it, and a handful
of external solvers invoked as child processes when the user has them.

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

Nothing below `alas-gui` knows a user interface exists. That is the same
"UI-agnostic core" rule the Python implementation had, and it is what lets the
headless command line and the desktop application be the same program: both are
thin callers of `alas-pipeline`.

---

## Crate layering

Crates are organized by discipline, not by layer-cake convention. The
dependency graph is acyclic and shallow, and Cargo enforces it.

**L0: foundations, no ALAS concepts**
`alas-units` (unit conversion factors), `alas-math` (linear algebra wrappers,
splines, the Chebyshev differentiation matrices, the MINPACK root finder),
`alas-types` (the stage status contract), `alas-i18n` (English and Spanish
string tables), `alas-config-derive` (the settings metadata macro).

**L1: description of an aircraft and its environment**
`alas-config` (every tunable value, ~5,600 Python lines' worth),
`alas-atmo` (the two atmosphere models).

**L2: geometry and the outside world**
`alas-geom` (airfoils, wings, fuselages, the aircraft builder, the structural
mesh), `alas-route` (great-circle, airways, flight plans), `alas-exec`
(child-process orchestration for external solvers).

**L3: disciplinary analyses**
`alas-aero` (both vortex-lattice methods, the airfoil surrogate, drag
build-ups), `alas-prop` (the turbofan cycle), `alas-mass`, `alas-stab`,
`alas-perf`, `alas-payload`.

**L4: composed analyses**
`alas-mission` (the segment solver), `alas-struct` (loads, sizing, the
finite-element bridge), `alas-opt` (the design search), `alas-screen` (airfoil
screening).

**L5: orchestration and output**
`alas-pipeline` (stage sequencing, concurrency, persistence), `alas-report`
(figure scenes and their export).

**L6: presentation**
`alas-viz` (drawing a scene with egui), `alas-gui` (the application),
`alas-app` (the binary, and the headless command line).

Test-only members: `alas-testkit` (fixture loading and tolerance assertions),
`alas-acceptance` (end-to-end comparison against the Python implementation).
`xtask` holds the repository checks.

---

## Four mechanisms worth understanding

Most of the codebase is straightforward: equations in, numbers out. Four pieces
carry structural weight, and everything else is arranged around them.

### 1. The stage status contract

Every analysis that can fail to happen returns a `Stage<T>`, which is one of
`Ok`, `Error` or `NotRun`. It serializes to exactly the JSON shape the Python
implementation produced, which is what lets the two be compared field by field.

This is the type-level form of "degrade honestly". An analysis that could not
run says so, and says why; it never returns a plausible number instead. A
figure whose data is `NotRun` renders as a stated absence, not an empty axis.

The distinction between `Error` and `NotRun` matters: a missing MSES
installation is `NotRun` and expected, an MSES run that diverged is `Error` and
is not.

### 2. Configuration metadata drives the interface

Configuration structs carry per-field metadata (a label, a unit, a help text,
bounds) declared once with `#[derive(ConfigNode)]` and a `#[config(...)]`
attribute. The doc comment is the help text, so there is one place to write it.

From that single declaration come the settings forms, the YAML round trip, and
the translation keys. There is no hand-maintained table of fields anywhere, and
the macro refuses to compile a public field that has neither metadata nor an
explicit `skip`. In the Python implementation the same idea was implemented with
`dataclasses.field(metadata=...)` and read reflectively at runtime; here it is
checked at compile time, which is the one thing that arrangement could not do.

The rule this enforces is "no hardcoded design values": anything a user might
reasonably want to tune is a configuration field, and being a configuration
field automatically means being visible, labelled and documented in the
interface.

### 3. Figures are scenes, not drawings

A figure produces a backend-neutral description (polylines, polygons, text,
images, axes) and never touches a drawing API directly. `alas-viz` renders a
scene into an egui panel; the export path renders the same scene to SVG, and
from there to PNG and to a multi-page PDF.

This exists because the two consumers have irreconcilable requirements. The
interactive view needs to redraw at frame rate into a GPU surface; the export
needs 200 dpi raster and vector output with no window open. Writing each figure
twice would guarantee they drift. Writing them once against a scene means a
figure is a pure function from results to geometry, which is also what makes it
testable: the parity test checks the scene's numbers, not its pixels.

Three-dimensional views are projected on the CPU and emitted as ordinary scene
geometry. That is what Matplotlib's 3D axes do as well, so this is a faithful
reproduction rather than a compromise.

### 4. External solvers are a process boundary

MSES, MSC Nastran, NASTRAN-95 and AVL are separate executables. `alas-exec`
owns everything about running them: locating them, writing their input decks,
feeding them the keystrokes they expect, killing the whole process tree on
timeout, and parsing what they print.

The rest of the program sees a function returning a `Stage<T>`. No analysis
crate knows a subprocess exists.

Two details are not incidental. Killing the process tree rather than the child
matters because these solvers fork a second-level worker that survives its
parent and goes on holding a licence seat. And every one of them is optional:
the program computes an analytical answer where it has one, and reports the
absence where it does not.

---

## Concurrency

The pipeline runs its independent stages concurrently (the mission, the
two-dimensional airfoil analysis and the structural solve do not depend on each
other) and the airfoil screening evaluates candidates in parallel.

Two things the Python implementation needed are gone. There is no module-level
render lock, because figure construction is a pure function with no global
drawing state to serialize. And parallelism is real: the screening's thousand
candidates run on all cores rather than contending for one interpreter.

---

## What was dropped, and why

Recorded here so that the absence is a decision rather than an oversight. Each
also has a row in `docs/PORTING.md`.

- **The HTTP sidecar and its schema, route and run-management modules.** The
  interface calls the library directly. This removes the loopback attack
  surface the Python implementation had to defend against with an origin
  blocklist, and with it the AGPL section 13 question.
- **Patran.** It only ever produced PNG images of a deformed wingbox. The
  displacements it drew are already read from the results file, so the program
  draws them itself.
- **PyVista.** It backed a 3D globe that was no longer wired into the interface,
  and it accounted for a large fraction of the frozen bundle's size.
- **The lazy-import machinery.** Three separate mechanisms existed to defer
  AeroSandbox's ten-second import cost off the startup path. A native binary
  has no import cost to defer.
