# How ALAS works inside

This chapter is the one that explains the machine rather than the
aircraft. If you are extending ALAS, debugging a run that behaved
oddly, or simply want to know what is actually happening between pressing
Run and getting figures, this is the map.

## The shape of the codebase

ALAS is layered, and the layering is strict in one direction:
everything below knows nothing about anything above it.

| Layer | Responsibility |
|---|---|
| `config/` | Typed dataclasses for every input: requirements, design space, geometry scaffold, engines, materials, cabin, mission, solver settings |
| `geometry/` | Turns a design vector into an actual aircraft: airfoil shaping, parametric assembly, structural meshing |
| `physics/` | The discipline models: aerodynamics, mass, stability, dynamics, propulsion, structures, payload, landing gear, performance |
| `optimization/` | Objective function, sampling, and the differential-evolution driver |
| `analysis/` | Full-fidelity evaluation of one finished design; airfoil screening |
| `integration/` | Boundaries to outside solvers (SUAVE, Nastran, Patran) and downloadable assets |
| `routing/` | Route construction between two airports |
| `reporting/` | Design-database export, console summaries, and every figure factory |
| `pipeline.py` | Orchestration: the only module that knows the full sequence |

The core is **interface-agnostic**. `pipeline.py` has no idea whether it
was called by the CLI, the desktop app, or a script, which is what allows
all three front ends to be genuinely equivalent rather than one being a
degraded copy of another.

## The central objects

Three data structures carry everything between stages.

**`DesignVector`**: one candidate aircraft, as sixteen named floats. It
converts to and from the flat NumPy array the optimizer works in, so the
solver can stay generic while the geometry builder writes `dv.span_m`
instead of `x[0]`. A consistency check runs at import time to guarantee the
dataclass fields and the specification list never drift apart.

**`AnalysisReport`**: everything known about one fully analysed design:
the built aircraft, the polar, the design point, the trimmed design point,
static margin and neutral point, component masses and their coordinates,
the physical centre of gravity, and the detailed payload layout.

**`PipelineResult`**: the whole run: optimized and baseline reports, the
optimizer history, the route and mission result, the MSES polar and
pressure field, and the structural result. Every optional stage is
`Optional[...]` and defaults to `None`, which is the type system encoding
the degradation contract described below.

## Two fidelity levels, on purpose

This is the single most important thing to understand about how ALAS
gets its speed.

Inside the optimizer loop, aerodynamics runs **coarse**: a low-resolution
vortex-lattice panelling and a fast cruise estimate. Roughly 1,400
evaluations in a run means anything expensive is multiplied by 1,400.

Once a winner is chosen, it is re-analysed **fine**: a denser panelling, a
full angle-of-attack sweep, a proper drag-polar fit, a real trim solve.

Nothing is carried over. The final numbers are not the optimizer's numbers
with more decimal places; they come from a different, more accurate
evaluation of the same geometry. This is why the lift-to-drag figure the
convergence plot converges to and the one the final report states are not
identical, and why that is correct rather than a bug.

The same principle governs the payload model: the optimizer uses a fast
lumped mass, while the baseline and final passes build the full seat-by-
seat, container-by-container layout from
[Cabin & payload](cabin-and-payload.md).

## What a run actually does

```
  build config  →  baseline pass  →  optimize  →  final analysis  →  post-analysis  →  export
                        │               │              │                  │
                   nominal design   DE search    fine-fidelity      mission / MSES /
                   W&B + stability  16 vars      re-evaluation      structures  (concurrent)
```

1. **Configuration is assembled.** Defaults, then any YAML overlay merged
   on top. The overlay is partial (you specify only what you change) and
   unknown keys raise rather than being silently ignored, so a typo is
   caught immediately instead of quietly doing nothing.

2. **A baseline pass runs first.** The nominal design gets a mass build-up,
   centre-of-gravity envelope, static margin and detailed payload layout
   before any optimization happens. This is deliberate: it is fast, and it
   catches a broken mass model or a badly placed wing before an optimizer
   spends minutes exploring a space built on it.

3. **The optimizer searches.** Differential evolution over the sixteen
   bounded variables, each candidate built, evaluated coarsely, and scored
   by the objective. Candidates violating a hard constraint are assigned a
   fixed large cost so the search moves away decisively.

4. **The winner is re-analysed at full fidelity**, and (when a baseline
   comparison was requested) the nominal design is analysed the same way.
   These two runs are independent, so they execute concurrently.

5. **Post-analysis stages fan out.** Export, mission, MSES and structures
   are mutually independent once the reports exist, so they are submitted
   to a thread pool and run at the same time. Figure rendering stays on the
   calling thread, because matplotlib is not thread-safe.

6. **Everything is written out**: see
   [Reporting & export](reporting-and-export.md).

`--no-parallel` forces the whole sequence to run strictly in order. It is
slower, and it exists because deterministic single-threaded execution is
enormously easier to debug.

## Process boundaries

Three external solvers are integrated, and all three sit behind a process
boundary rather than an import.

**SUAVE** needs an old NumPy/SciPy/scikit-learn stack that directly
conflicts with the versions ALAS's own aerodynamics require. It is
provisioned into an isolated Python 3.10 environment and invoked as a
subprocess: a JSON request goes in, a CSV and a summary come back. The main
process never imports it. This is the only way both stacks can coexist
without one being downgraded to suit the other.

**MSES** and **Nastran/Patran** are external executables, launched the same
way: arguments and input decks in, result files parsed back out.

The consistent contract across all three: **a failing optional stage never
fails the run.** A missing environment, an absent licence, a
non-convergence, a timeout: each is recorded as a non-`ok` status on that
stage's result object, and every other stage proceeds. You will see this
throughout the guide: the
[NASTRAN cross-check](structural-analysis.md#the-analytical-fallback-no-nastran-required)
that was unavailable while these pages were written, and the
[mission stage](mission-and-route.md) that reports `not_configured` until
its environment is provisioned.

## Configuration and the overlay mechanism

Every tunable value lives in a dataclass field with a label, a unit, and a
help string. That metadata is not documentation decoration; it is what the
desktop application reads to generate its forms. A new configuration field
appears in the interface, correctly labelled and unit-annotated, without
anyone writing interface code for it.

The design principle behind this is stated bluntly in the project's own
rules: **no hardcoded design values**. If a number could reasonably be
tuned, it belongs in `config/`, not in the solver that consumes it. The
mission profile is the clearest example: every climb rate, cruise-leg
speed and descent step in
[Mission & route analysis](mission-and-route.md) is a field, not a
constant buried in the runner.

## The desktop application

The packaged application is a **Wails** shell: a compiled Go binary
embedding a React interface, which starts a Python sidecar exposing the
same `DesignPipeline` over a loopback HTTP and WebSocket API.

- The sidecar binds an **operating-system-assigned port on the loopback
  interface** and announces it on standard output; nothing is reachable
  from outside the machine.
- Figures are rendered by the same factories the command line uses and
  served as SVG, so the desktop charts and the exported PNGs come from one
  implementation rather than two.
- Long runs stream progress over the WebSocket, which is what makes the
  live log possible without blocking the interface.
- For distribution, the sidecar is frozen with PyInstaller and embedded
  into the Go binary, then extracted and launched at startup. That is why
  the download needs no Python on the target machine, and why it is large.

Process lifetime is handled per platform (a Windows Job Object, and
process-group plus parent-death signalling on Linux), so the sidecar cannot
outlive the application that started it.

## Extending it

The layering makes the common extensions predictable:

| You want to… | Touch |
|---|---|
| Add a tunable input | The relevant `config/` dataclass: the interface follows automatically |
| Add a design variable | `config/design_variables.py`, then consume it in `geometry/` |
| Change a physical model | The single module in `physics/` that owns it |
| Add a figure | A factory in `reporting/visualization.py`, then wire it where it belongs |
| Add an external solver | A module in `integration/`, behind a process boundary, returning a status-carrying result |

The rule that keeps this workable is the one-way dependency: `physics/`
may not import from `analysis/`, `analysis/` may not import from
`pipeline.py`, and nothing in the core may import from a front end.
