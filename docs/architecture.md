# ALAS — Architecture

This document describes the workflow, the pipeline, and how the modules connect
and communicate. For the underlying equations see
[`methods.md`](methods.md).

> **On the front-end sections.** `desktop/` (Wails + React + a Python
> sidecar) is the only front-end in this repo. §2a gives the current
> picture; §12 covers the mechanism in depth (process model, how figures are
> served, what's genuinely client-rendered). The "GUI integration"
> paragraphs inside §9e/§10c/§11e predate the React port and are trimmed to
> the physics/data facts that are still true, with a pointer to §12 for how
> the current front-end actually renders each one.

## 1. The workflow at a glance

```
┌──────────────┐ ┌───────────────┐ ┌───────────────┐ ┌──────────────┐ ┌──────────────┐
│ REQUIREMENTS │►│  OPTIMIZATION │►│ FULL ANALYSIS │►│  REPORTING   │►│   MISSION    │
│ (user input) │ │  (search)     │ │ (evaluate)    │ │  (export)    │ │ (SUAVE, opt.)│
└──────────────┘ └───────────────┘ └───────────────┘ └──────────────┘ └──────────────┘
   YAML / code      DesignVector      AnalysisReport     JSON/.dat/PNG   Route + MissionResult
```

1. **Requirements** — the user provides targets/limits as an `ALASConfig`
   (from a YAML file or constructed in code).
2. **Optimization** — `DesignOptimizer` searches the named design space with
   SciPy's differential evolution, minimising `DesignObjective`. The initial
   population is seeded as a tight cluster around the initial/preset design
   by default (`SolverSettings.seed_near_initial_design`, see
   `methods.md` §13a) rather than SciPy's default full-space
   sampling, guaranteeing a known-valid starting point instead of relying on
   chance to find one. Each candidate is built, balanced, and aerodynamically
   scored with a *fast* 2-point estimate.
3. **Full analysis** — the winning `DesignVector` is rebuilt with engines,
   balanced, and run through a *fine* alpha sweep. `FullAnalysis` extracts the
   cruise design point, fits a clean drag polar, and measures static margin.
4. **Reporting** — results are printed, exported to a JSON design database and an
   optimized airfoil `.dat`, and optionally plotted.
5. **Mission analysis** (Stage 5, `config.mission.enabled`, default on) — a SUAVE
   climb/cruise/descent mission for the chosen design over
   `config.departure_airport`/`arrival_airport`, run automatically as part of
   the same `run()` call, not a separate step. See §9.

**Parallel execution.** `DesignPipeline.run(..., parallel=True)` (the default;
`--no-parallel` on the CLI, an unchecked "Run stages in parallel" box in the
GUI force the old behaviour) runs independent stages on background threads
instead of strictly sequentially: Stage 2's two full analyses (the optimized
design, and — if `compare_baseline` — the nominal design comparison) run
concurrently on two threads; once Stage 2 finishes, export (Stage 3) and the
SUAVE mission analysis (Stage 5, a subprocess that can take minutes) run on
background threads while plotting (Stage 4) proceeds on the calling thread
(matplotlib figure creation is never itself spread across threads). This is
a pure scheduling change — every stage's inputs/outputs and exception
behaviour are unchanged. `AirfoilLibrary._init_zip()` (`geometry/airfoils.py`)
is lock-guarded specifically so its first-population cache check-then-set
can't race when the two Stage-2 analyses build geometry concurrently.

**Stage 0 — Baseline W&B + stability.** Before optimization, `run()` always
computes a `BaselineReport` for the *initial* design: build → mass analysis (to find physical CG) → neutral point calculation (to find static margin) →
detailed payload layout → mass analysis → two-point static margin (no full
sweep). It surfaces as the **first Results tab** so the user can validate that a
preset (e.g. the A320) sits at a sensible CG and static margin — i.e. the mass
model and payload distribution are correct — before spending time on the
optimizer. `DesignPipeline.analyze_baseline()` runs *only* this stage (backing
the GUI "Analyze baseline" button).

**Detailed payload model, everywhere.** The optimizer loop, the baseline pass,
and the final pass all build the *detailed* cargo/passenger interior — ULD
pallets or a class-by-class seat map with galleys, lavatories and CS-25 exits
— via `physics.payload.build_payload_layout` (dispatches to `cargo_loader` or
`cabin_layout`, returns a `PayloadLayout`). Each still does a cheap first pass
with the fast lumped payload (`cabin_payload_density_kg_m`) to get an OEW/x_oew
estimate, then rebuilds with the detailed layout, which overrides the lumped
Payload mass/CG (the optional `payload_layout=` argument of
`run_mass_analysis`). The optimizer used to skip this second pass and check its
CG-envelope constraint against the lumped estimate alone — a real bug (not a
speed optimization), since the lumped model's payload CG (geometric centre of
the occupied cabin length) can differ from the detailed model's true
mass-weighted CG by several percent MAC, letting a candidate the optimizer
scored as compliant land outside the envelope once the final analysis
recomputed it with real geometry. Fixed by running the same two-pass sequence
in the optimizer (`optimization/objective.py`); see `methods.md` §17a
for the measured magnitude and cost. That single `PayloadLayout` object feeds
the mass analysis, the Cabin/Payload preview, and the report.

## 2. Module map

```
alas/
├── config/                 ── all user-tunable inputs (typed dataclasses)
│   ├── requirements.py        DesignRequirements      (mission/targets)
│   ├── design_variables.py    DesignVector + specs    (the search space)
│   ├── geometry_config.py     GeometryConfig          (fixed scaffold)
│   ├── physics_config.py      DragModelConfig         (drag-model coefficients)
│   ├── optimizer_config.py    OptimizerConfig         (solver + weights)
│   ├── analysis_config.py     AnalysisConfig          (sweep fidelity)
│   ├── performance_config.py  PerformanceConfig       (high-lift & field perf constants)
│   ├── cabin_config.py        CabinConfig             (passenger classes + cargo deck loading)
│   ├── engines.py             Engine database         (immutable presets; seeds EngineConfig, see §10)
│   ├── propulsion_config.py   PropulsionCycleConfig   (on-design cycle component efficiencies, §10)
│   ├── presets.py             Presets database        (A320, B787, A380 presets)
│   ├── airports.py            Airport database        (20 airports: major + challenging)
│   ├── mission_config.py      MissionConfig + MissionProfileConfig (SUAVE mission settings;
│   │                                 every climb/cruise/descent speed/rate is here, not
│   │                                 hardcoded in the SUAVE runner -- see §9)
│   ├── control_surfaces_config.py  ControlSurfacesConfig  (slat/flap/aileron/spoiler/
│   │                                 elevator/rudder chord- & span-fraction bounds --
│   │                                 a representation input for the control-surface
│   │                                 sizing diagram only, not fed into aero/mass models)
│   ├── materials.py            MaterialSpec + MATERIAL_DATABASE (wingbox materials, §11)
│   ├── structures_config.py    StructuresConfig        (wingbox + NASTRAN + Patran settings, §11)
│   └── settings.py            ALASConfig         (aggregator + YAML I/O)
│
├── data/
│   └── airfoil_data.py        reference section coordinates
│
├── geometry/               ── shape generation
│   ├── airfoils.py            AirfoilLibrary, bumps, morphing, build_section
│   ├── aircraft_builder.py    AircraftBuilder: (DesignVector, config) → Airplane
│   ├── wing_structure.py      WingStructureGeometry: generic N-spar rib/spar FEM geometry (§11)
│   └── wing_mesh_bdf.py       build_wing_mesh_bdf(): pyNastran BDF mesh (skin/spars/ribs), §11
│
├── physics/                ── the solvers
│   ├── aerodynamics.py        AeroAnalysis: VLM + Raymer + Korn drag
│   ├── stability.py           neutral_point, fuselage_cm_alpha, stability_and_trim (§9d trim solve)
│   ├── mass.py                Torenbeek component mass & physical CG solver
│   ├── payload.py             PayloadLayout/DeckItem, CabinGeometry, build_payload_layout
│   ├── cargo_loader.py        ULD database + cargo CG-targeting load solver (run-once)
│   ├── cabin_layout.py        passenger seat/galley/lav/exit engine (run-once)
│   ├── performance.py         Matching chart, V speeds, field performance, V-n diagram,
│   │                                 Breguet range/payload-range, wing fuel-volume check
│   ├── dynamics.py            Dynamic-mode analysis (phugoid/short-period/dutch roll/
│   │                                 roll subsidence/spiral) via AeroSandbox's own
│   │                                 get_modes(); inertia estimate (radius-of-gyration)
│   ├── propulsion.py          On-design turbofan cycle: compute_turbofan_cycle,
│   │                                 carpet plot / BPR / efficiency / altitude sweeps (§10)
│   ├── structural_loads.py    Shared wingbox load model: load_cases, elliptic distribution,
│   │                                 cantilever shear/moment integration, engine point loads (§11)
│   ├── structural_sizing.py   size_wingbox(): direct strength-based cap/web/skin/rib sizing (§11)
│   └── structural_analysis.py analyze_structure(): Castigliano/Euler-Bernoulli deflection,
│                                     Rayleigh natural frequencies, stress margins -- no NASTRAN (§11)
│
├── optimization/           ── the search
│   ├── objective.py           DesignObjective (cost), OptimizationHistory
│   └── optimizer.py           DesignOptimizer → OptimizationResult  (polish=False)
│
├── analysis/               ── final evaluation
│   └── full_analysis.py       FullAnalysis → AnalysisReport
│
├── integration/            ── SUAVE mission-analysis subprocess bridge + NASTRAN
│   ├── suave_bridge.py        run_mission(): shells out to the isolated .suave-venv
│   ├── suave_vehicle.py       AnalysisReport + config → vehicle request dict
│   ├── suave_mission.py       Airport pair + route distance + MissionProfileConfig
│   │                                 → mission request dict
│   ├── assets.py              download/status helpers for navdata + Earth texture,
│   │                                 shared by scripts/*.py and the GUI's download buttons
│   ├── _nastran_compat.py     numpy 2.x shim for pyNastran (same idea as SUAVE's _compat.py, §11)
│   ├── nastran_runner.py      SOL 101/103/111 BDF case-control builders + opt-in subprocess
│   │                                 runner + OP2/F06 result reading (§11)
│   └── patran_runner.py       opt-in headless Patran deformation-plot PNG export (§11g)
│
├── routing/                ── route generation between airports
│   ├── route.py                Waypoint, Route, great_circle(), for_airports()
│   ├── navdata_graph.py        open-navdata airway graph + Dijkstra (GPLv3, not bundled)
│   └── kml_import.py           SimBrief KML → Route (manual, highest fidelity)
│
├── reporting/              ── outputs
│   ├── design_report.py       JSON/.dat export, console summary
│   ├── visualization.py       matplotlib figures (lazy import)
│   └── route_globe.py         PyVista textured 3D globe + mass/altitude sync onto a Route
│
├── pipeline.py             ── DesignPipeline: orchestrates the 5 stages (incl. mission analysis)
├── cli.py                  ── argparse front-end → builds config, runs pipeline
└── __init__.py             ── public API: ALASConfig, DesignPipeline
```

Dependency direction is strictly downward: `desktop`/`sidecar`/`cli` →
`pipeline` → `optimization`/`analysis` → `physics`/`geometry` →
`config`/`data`. Nothing in the core imports a front-end; only `reporting`
and `sidecar` import matplotlib (both kept optional/lazy so headless use
needs neither a display nor the GUI stack). `integration` and `routing` sit
beside `reporting`/`sidecar` in that same optional tier: `integration` never
imports SUAVE directly (it shells out to the isolated `.suave-venv` as a
subprocess — see §9), and `reporting/route_globe.py` imports `pyvista`
lazily, the same pattern matplotlib already uses (kept for a possible future
static-report export, though the live GUI route globe is now a client-side
WebGL view, §12b, not PyVista).

## 2a. The GUI layer (front-end)

The desktop app (`desktop/`) is a thin shell over `DesignPipeline` and adds no
physics; §12 covers its process model and figure pipeline in depth. Three
ideas make it generic and maintainable, carried over from an earlier PySide6
prototype but implemented natively in React against a FastAPI sidecar rather
than ported line-for-line:

- **Auto-generated forms.** `DynamicForm` (`desktop/frontend/src/components/
  DynamicForm.tsx`) walks a config dataclass's JSON-schema-like field
  metadata and emits a labelled, unit-annotated control per field (recursing
  into nested dataclasses), with hover-help sourced from the same metadata
  via `HelpHover`/`InfoTip.tsx` — no per-field UI code. The Inputs and
  Advanced Settings pages are built directly from the config dataclasses;
  adding a config field gives it a control automatically.
- **User-controlled design space.** `DesignSpaceTable` exposes the same three
  columns per DOF (Initial Value / Lower / Upper) the PySide6 prototype
  established, editable, with a *Reset to defaults* control. Loading a preset
  recentres Lower/Upper around its vector — every preset must still be
  calibrated so `CL_required` at the nominal and the all-variables-at-minimum
  corner both stay below `requirements.max_cruise_cl` (see `methods.md` §2,
  Preset calibration note).
- **Threaded run, server-rendered figures.** A Run is dispatched to the
  FastAPI sidecar and tracked over WebSocket/polling (`sidecar/runs.py`,
  `lib/sidecarClient.ts`) so the UI stays responsive and streams progress;
  results are organised into the same discipline-grouped tabs the prototype
  used (Baseline W&B & Stability, Summary, Optimization, Aerodynamics,
  Weight & Balance, Propulsion Analysis, Structural Analysis, Mission &
  Route, Model Comparison), each a lazy tab built from the shared
  `RESULT_FIGURES` registry (§12b) rather than a bespoke widget per
  discipline. `ResultsScreen.tsx` keeps a fetched figure mounted (hidden via
  CSS) once its tab has been opened, so revisiting one never re-fetches. A
  separate **Analyze baseline** action computes only the first tab (no
  optimizer), and a **Cabin / Payload** live-preview panel (beside the
  exterior 3D preview on Inputs) shows the seat/ULD layout across all decks
  as the config changes, with no run needed.

## 3. The central data objects (how stages communicate)

| Object | Produced by | Consumed by | Carries |
|--------|-------------|-------------|---------|
| `ALASConfig` | user / YAML / CLI | every stage | all inputs |
| `DesignVector` | optimizer, or initial_design from UI | geometry, analysis | the 16 named DOFs |
| `asb.Airplane` | `AircraftBuilder` | physics, reporting | the built geometry |
| `OptimizationHistory` | `DesignObjective` | reporting | per-evaluation log |
| `OptimizationResult` | `DesignOptimizer` | pipeline, reporting | best design + history |
| `AnalysisReport` | `FullAnalysis` | reporting | polar, design point, fit, SM, payload layout, `cg_envelope_ok` (same check `objective._check_cg_envelope` runs, computed here purely for reporting — lets `figure_polar_comparison` explain a baseline that beats the optimized design on raw L/D but fails this check) |
| `BaselineReport` | `DesignPipeline._baseline_analysis` | results view (first tab) | initial-design W&B + static margin + payload layout |
| `PayloadLayout` | `build_payload_layout` | mass, visualization, report | deck items (seats/ULDs/monuments/exits), payload mass & CG |
| `Route` | `DesignPipeline._run_mission_analysis` (Stage 5, §9) | results view, route globe | lateral waypoints + source tier (KML/navdata/great-circle) |
| `MissionResult` | `suave_bridge.run_mission` (Stage 5, §9) | results view, route globe | status + every SUAVE mission CSV column |
| `PipelineResult` | `DesignPipeline` | caller (CLI/GUI) | everything above |

Because these are plain dataclasses, any front-end or test can inspect them
without re-running physics.

## 4. The two fidelity levels

ALAS deliberately uses **two** aerodynamic evaluations:

- **Fast (`AeroAnalysis.quick_performance`)** — two VLM points, linearised lift
  slope, scaled induced drag. Used *inside* the optimization loop, where it may
  be called hundreds of times.
- **Fine (`AeroAnalysis.run_sweep`)** — a full alpha sweep producing the corrected
  drag polar and stability curve. Used *once*, on the winning design.

Both share the same parasite/wave-drag buildup, so the fast estimate is a
consistent reduction of the fine one, not a different model.

## 5. The optimization loop (data flow)

```
differential_evolution
        │  x  (flat array, bounds from DESIGN_VARIABLE_SPECS)
        ▼
DesignObjective.__call__(x)
        │ DesignVector.from_array(x)
        ▼
AircraftBuilder.build(dv, engines=False) ──► asb.Airplane
        │
        ├─ sizing penalties (area, wing loading)               ← requirements
        ├─ run_mass_analysis (lumped) → build_payload_layout   ← physics.mass / physics.payload (two-pass:
        │    → run_mass_analysis(payload_layout=...)              lumped for OEW/x_oew, then the detailed
        │                                                          cabin/cargo layout overrides the payload
        │                                                          CG — same model the final report uses,
        │                                                          §17a; falls back to the lumped result
        │                                                          non-fatally on any exception)
        ├─ required_cruise_cl(q, S)  (+ stall guard, HARD)     ← requirements
        ├─ stability_and_trim(plane, cl_target, mach, alt)     ← physics.stability (cruise-condition 3-point
        │    → x_np, static_margin, trimmed alpha & i_h           probe: NP/SM + closed-form trim solve, §9d)
        ├─ check static_margin < min_physical_static_margin      (flag only, not an early return — see below)
        ├─ check CG envelope (OEW/MZFW/MTOW vs [fwd, aft] limits) (flag + exceedance, not an early return)
        ├─ AeroAnalysis.trimmed_performance(trim, mach, alt)   ← physics.aerodynamics (1 more VLM solve at
        │    → genuinely trimmed CL/CD/L-over-D                  the closed-form trim point — real trim drag,
        │                                                          run for EVERY candidate, see below)
        ├─ tail volume coefficient penalties (Vh, Vv)          ← optimizer weights §9c
        ├─ static margin TARGET deviation penalty (soft)       ← objective weights
        ├─ static-margin-floor / CG-envelope penalties          ← large-but-continuous, dominate −L/D but don't
        │    (added here, not returned early)                     erase it (see below)
        └─ assemble scalar cost (−L/D + penalties)             ← optimizer weights
        ▼
   cost  +  history.record(..., reason=...)
```

A candidate that fails to *build or evaluate* (geometry/mass/VLM crash, or
the stall guard `CL_required > max_cruise_cl`, which leaves no sensible
operating point to evaluate at all) returns a large `failure_cost` instead of
raising, so the global solver keeps exploring — these are true hard,
early-return rejects.

A candidate that builds and evaluates successfully but is *physically
invalid* (static margin below `min_physical_static_margin`, or a CG-envelope
violation at any loading state) is different: it is **not** an early return.
`trimmed_performance` still runs and contributes a real L/D to the cost, and
the violation adds a large-but-continuous penalty (`instability_failure_cost`
plus a graduated term, §12/§17 of `methods.md`) on top — dominant
enough that a compliant candidate always beats a non-compliant one, but not
so absolute that SciPy's `differential_evolution` loses all L/D signal for
the population while it's still outside the envelope. An earlier version of
this function *did* early-return a flat/graduated cost with no L/D computed
at all for these cases; a direct DOF sweep confirms that a CG-compliant
region genuinely exists in the search space,
but the solver couldn't find it once L/D was blanked out for every
non-compliant candidate — it had no way to simultaneously feel toward
"compliant" and "aerodynamically good," and converged on an infeasible "best
of a bad lot" design instead. `OptimizationHistory.reject_reason_counts`
still distinguishes "crashed" from "physically unstable" (and payload
shortfall) for diagnostics, independent of this cost-continuity fix.

## 6. Configuration & overlay mechanism

`ALASConfig.from_dict` / `from_yaml` overlay a **partial** user dictionary
onto the default dataclass tree (`settings._overlay_dataclass`, recursive).
Consequences:

- A user's YAML only needs the keys they care about.
- Unknown keys raise `KeyError` (typos are caught early).
- Lists in YAML are re-tupled where the dataclass declares a tuple
  (e.g. `EngineConfig.spanwise_positions_m`).

`--save-config` serialises the *effective* config back to YAML for editing.

## 7. Extension points

- **New design DOF**: add a spec to `DESIGN_VARIABLE_SPECS` *and* the matching
  field on `DesignVector` (a startup assertion enforces they stay in sync), then
  consume it in `AircraftBuilder` / `airfoils`. The Design Space table and all
  auto-generated bounds will pick it up automatically.
- **New analysis output / plot**: add a field to `AnalysisReport`, populate it in
  `FullAnalysis.run`, add a factory function to `reporting/visualization.py`, and
  call it from `gui/widgets/results_view.py`.
- **New fidelity**: SUAVE mission analysis (§9) is the existing example of this
  pattern — a module under a new package (`integration/`) plus a pipeline
  stage, fed by the JSON-equivalent design data `reporting` already produces.
- **Different solver**: `DesignOptimizer.run` is the only place bound to SciPy;
  swap it there while keeping `DesignObjective` unchanged.
- **New aircraft type / configuration module**: create a new `GeometryConfig`
  subclass or additional config fields; the auto-generated forms will surface them
  in the GUI without any UI code.

## 8. Packaging (standalone `.exe` / Linux binary)

`wails build` (in `desktop/`) produces a fully standalone executable: no repo
checkout, no `uv`, no Python installed on the target machine. This is what
`sidecar.go`'s own comments had anticipated as "Phase 6" since the sidecar was
first split out of the old PySide6 GUI (§ note at the top of this document).

The sidecar (`alas/sidecar/server.py`, the FastAPI app §12's frontend
talks to) is frozen with PyInstaller into a one-dir bundle —
`scripts/packaging/alas_sidecar.spec` is the spec (entry point is a thin
`scripts/packaging/entrypoint.py`, not `server.py` directly, since
`server.py`'s package-relative imports need a real package context PyInstaller
doesn't give a frozen entry script), `scripts/build_sidecar.py` is the driver.
One-dir, not one-file: one-file mode self-extracts to a fresh temp directory
on every launch and its self-extracting-archive shape is a well-known
antivirus false-positive trigger, so one-dir pays the extraction cost once, at
build time. `wails.json`'s `preBuildHooks["*/*"]` runs that driver script
automatically before every `wails build`, staging the result into
`desktop/sidecar_dist/<goos>-<goarch>/`, where `desktop/embed_sidecar.go`'s
`//go:embed all:sidecar_dist` picks it up at Go compile time.

At startup, `desktop/sidecar.go`'s `SidecarManager.Start()` checks whether a
frozen bundle was embedded for the current `GOOS`/`GOARCH`; if so, it extracts
it into a fingerprinted subdirectory of `sidecarCacheRoot()` (and
`suave_runtime.go`'s `suaveCacheRoot()` for the bundled SUAVE runtime, §9a) --
`os.UserCacheDir()/ALAS/...` (`%LocalAppData%\ALAS\...` on Windows),
**not** the raw OS temp directory. Extracting a large executable into Temp and
then running it from there is a classic behavioural-AV "dropper" pattern --
this was confirmed to trip Kaspersky's System Watcher directly on the
reference dev machine -- and Temp is documented as transient (any disk-cleanup
tool may clear it) whereas this cache is meant to persist across launches
anyway. The fingerprint means a rebuild is only ever extracted once; stale
fingerprints from a previous build are pruned on the next successful
extraction (`pruneStaleSidecarCaches`/`pruneStaleSuaveCaches`), and the cache
is intentionally left in place across `Stop()`/app restarts to avoid re-paying
the ~800MB extraction cost every launch. If not (a plain `go build`, or
`wails dev` — whose whole point is
live-editing Python with no separate freeze step), it falls back to the
original dev-mode `uv run python -m alas.sidecar.server` invocation. Both
paths announce their port and pass a `/healthz` check the same way (the
`ALAS_PORT=<n>` stdout handshake, §12), so nothing downstream of launch
needs to know which mode is active. `desktop/procattrs_windows.go` /
`procattrs_linux.go` / `procattrs_other.go` give each OS its own process
lifecycle guarantees (hidden console window + Job Object on Windows;
process-group `Setpgid`/`Pdeathsig` on Linux) so the sidecar — and anything it
itself spawns, like an MSES or SUAVE run — never outlives the parent app.

**PyInstaller cannot cross-compile.** A Windows `.exe` and a Linux binary each
need their own native build machine or CI runner; running
`wails build -platform linux/amd64` on a Windows host still freezes a
*Windows* sidecar (there is no way around this), so cross-platform releases
need one CI job per target OS.

**Known costs, not yet addressed**: the bundle is large (~600 MB one-dir —
PyVista/VTK alone is the ~100-200 MB this section used to flag before
casadi/scipy/aerosandbox/pyNastran were counted too); embedding that much data
via `go:embed` makes `wails build`'s own Go-compile step noticeably slower
(~70s, vs. near-instant for the placeholder-only dev case). A frozen build's
`outputs/` directory has no defined home yet (the core pipeline still writes
there via a path that resolved sensibly relative to a repo checkout in dev
mode; a packaged install needs a real user-data-directory strategy instead).
PyInstaller bundles compiled bytecode, not encrypted/obfuscated source, so it
is not real protection against someone reading the code back out of a
shipped binary — Nuitka (compiles to a real machine-code binary) or an added
PyArmor pass are the options if that's actually required.

**Finding tools once frozen — `alas/paths.py`.** Every external tool
integration (MSES, SUAVE, and previously NASTRAN) used to derive its
executable directory as `Path(__file__).resolve().parents[N]` — correct in a
dev checkout, silently wrong once frozen: `__file__` then points inside the
one-dir bundle's temp extraction cache above, nowhere near the real
`ALAS.exe` a user's `external tools/` folder sits beside. `paths.py` is
the single resolver now: `app_root()` (the real install dir — an
`ALAS_APP_DIR` env var the Go launcher sets from `os.Executable()`, or
`sys.executable`'s own directory as a fallback), `bundle_root()` (read-only
data frozen *into* the bundle, e.g. the airfoil coordinate database —
`sys._MEIPASS`, distinct from `app_root()`), `resolve_tool_dir()` (searches
app root / `app root/bin` / parent / repo root for a configured relative
tool path, always honouring an absolute override verbatim), and
`find_tool_dir()` (same search, but returns `None` instead of a best guess
when nothing actually exists — used to decide "is something already
provisioned here" before falling back to a bundled copy, see §9a's SUAVE
resolution order). `MSESConfig.mses_dir`/`MissionConfig.suave_venv_dir` etc.
still store the same repo-root-relative-looking defaults
(`"external tools/MSES"`) — only how they get resolved to an absolute path
changed.

## 9. Mission analysis, routing, and the 3D route globe

This layer answers: *given this optimized design and this departure/arrival
airport pair, what's the fuel burn and block time, and what does the route
look like on a globe?* It is entirely optional and additive — none of the
core pipeline (requirements → optimize → analyze → report) depends on it.

### 9a. Why SUAVE needs a subprocess, not an import

SUAVE 2.5.2 (`external tools/SUAVE-2.5.2/trunk/SUAVE`, a bundled source tree,
not pip-installed) was written against numpy 1.x/scipy/scikit-learn/matplotlib
3.x and is incompatible with ALAS's own numpy 2.x stack — both cannot
coexist in one Python process. `scripts/provision_suave_venv.py` provisions a
**second**, fully isolated Python 3.10 venv (`<repo root>/.suave-venv` in a dev
checkout) via `uv` (`uv venv --python 3.10` + a `--no-config` pinned install —
see that script's comments on why `--no-config` is load-bearing: ALAS's
own `pyproject.toml` sets a workspace-level `numpy>=2,<3` override that `uv`
would otherwise silently apply to this unrelated venv too, defeating the whole
point). ALAS's main process never imports SUAVE;
`alas/integration/suave_bridge.py` only ever invokes
`<venv>/Scripts/python.exe external tools/suave_runner/run_mission.py` as
a subprocess, passing a JSON request and reading back a CSV + summary JSON.

A packaged `ALAS.exe` bundles this same venv automatically instead of
requiring the manual step above: `scripts/build_suave_env.py` (run by
`wails build`'s pre-build hook, `scripts/prebuild.py`) provisions a fresh copy,
zips it together with the SUAVE source and `suave_runner`, and stages it for
`desktop/embed_suave.go`'s `//go:embed`. At launch, `desktop/suave_runtime.go`
extracts it once into a fingerprinted OS-temp cache dir and
`desktop/sidecar.go` passes its path to the sidecar via the
`ALAS_SUAVE_VENV_DIR`/`ALAS_SUAVE_RUNNER_DIR` env vars; `pipeline.py`
prefers a user-provisioned copy already on disk over this bundled fallback
(see `alas/paths.py`).

Two more wrinkles, both worked around in
`external tools/suave_runner/_compat.py` (imported first by every runner-side
module, before `import SUAVE`):
- SUAVE's vendored `pint` plugin does `from collections import MutableMapping`,
  a path Python removed in 3.10 (available via `collections.abc` since 3.3) —
  `_compat.py` re-aliases the needed names onto `collections` rather than
  patching SUAVE's vendored source.
- `_compat.py` also adds the SUAVE trunk directory to `sys.path`, since it
  isn't pip-installed.

### 9b. The vehicle/mission request, end to end

```
AnalysisReport + ALASConfig                  Airport × 2 + Route
        │ suave_vehicle.build_vehicle_request()           │ suave_mission.build_mission_request()
        ▼                                                  ▼
  vehicle request dict                          mission request dict (incl. "profile":
        │                                         dataclasses.asdict(config.mission.profile))
        └──────────────────┬───────────────────────────────┘
                            ▼
              suave_bridge.run_mission(venv_dir=, timeout_s=)  (writes request.json, spawns subprocess)
                            │
         ┌──────────────────┴───────────────────────────────────┐
         │   external tools/suave_runner/run_mission.py (Py 3.10, SUAVE venv)  │
         │     vehicle_builder.build_vehicle()   ── parameterized SUAVE.Vehicle│
         │     mission_builder.mission_setup()   ── every climb/cruise/descent │
         │                                           speed/rate read from      │
         │                                           request["profile"], not   │
         │                                           hardcoded here            │
         │     mission.evaluate()                ── SUAVE's own solver         │
         │     export_data.export_simulation_results()  ── writes the CSV      │
         └──────────────────┬───────────────────────────────────┘
                            ▼
              MissionResult (parsed CSV + summary.json)
                            ▼
              PipelineResult.route / .mission_result  (Stage 5, see §1/§3)
                            ▼
              ResultsView.populate(result, origin, dest)  -- same call as every
                                                               other tab, no separate step
```

`external tools/suave_runner/vehicle_builder.py` and `mission_builder.py` are
generalizations of `suave_example.py`'s original `vehicle_setup()` /
`mission_setup()` (which hardcoded one aircraft — "AVE", matching the `AVE`
preset in `config/presets.py` almost exactly — and one route, Madrid→Nairobi).
They now read every dimension from the request dict instead, so the same code
path works for any preset/optimized design and any airport pair, and every
climb/cruise/descent speed, rate, and altitude fraction comes from
`config.mission.profile` (`MissionProfileConfig` in `config/mission_config.py`,
auto-form-editable in Advanced Settings → Mission Analysis) rather than a
literal in the runner script. Where ALAS doesn't model a parameter SUAVE
needs (high-lift device span fractions, individual compressor pressure
ratios), the generic textbook-level defaults `suave_example.py` used are kept,
scaled by the one or two figures ALAS *does* know (e.g. each engine's
high-pressure-compressor ratio is backed out from its published overall
pressure ratio in `config/engines.py`). Engine cycle data
(`overall_pressure_ratio`, `turbine_inlet_temp_k`, `fan_pressure_ratio`) is
publicly known for OPR/BPR on most engines; where not (e.g. PW1500G's exact
OPR), the field comment in `engines.py` says so explicitly — appropriate for
conceptual-design-level mission analysis, not claimed as certified data.

Mission analysis **is** part of `DesignPipeline.run()` (Stage 5, see §1) —
not a separate worker or button. It runs automatically whenever
`config.mission.enabled` is true (default) and a Stage-2 `AnalysisReport`
exists, inside the same `PipelineWorker` thread the rest of the run already
uses. `DesignPipeline._run_mission_analysis()` resolves
`config.mission.{suave_venv_dir,routes_dir,navdata_dir}` (repo-root-relative,
editable) to absolute paths, builds the route and the two request dicts, and
calls `suave_bridge.run_mission()`; on any failure (SUAVE not configured,
unresolvable airport, subprocess error) it returns a non-"ok"
`MissionResult.status` rather than raising, so a normal Run never fails
because mission analysis couldn't complete.

### 9c. Routing: four fidelity tiers

`Route.for_airports(origin, dest, routes_dir=, navdata_dir=, great_circle_points=, simbrief_username=, simbrief_timeout_s=)`
(`alas/routing/route.py`) tries, in order:

1. **SimBrief API** (`routing/simbrief_route.py`) — if `config.mission.simbrief_username`
   (a SimBrief username or numeric Pilot ID, Advanced Settings → Mission
   Analysis) is set, fetches that user's most recently generated OFP via
   SimBrief's public `xml.fetcher.php` endpoint (no API key/approval needed
   for this specific read-only "fetch my last plan" call, unlike generating
   a *new* dispatch on demand, which does need a Navigraph-approved key —
   see §9c's SID/STAR note below). Only used if the fetched OFP's
   origin/destination actually match the requested airports (otherwise it's
   a different/stale plan and this tier is skipped); the practical workflow
   is to generate that exact city pair on SimBrief first, then run
   ALAS. Highest fidelity when it applies: real SID/STAR/airway routing
   against SimBrief's own current-AIRAC data, computed by the user's own
   account rather than ALAS needing to source/parse licensed data.
2. **Manual SimBrief KML** — `{config.mission.routes_dir}/{ORIGIN_ICAO}_{DEST_ICAO}.kml`,
   if present. Same fidelity idea as tier 1 but via a one-time manual
   export/drop-in instead of a live per-run fetch; the
   user generates this themselves on SimBrief's free website (no API key
   needed for personal use) and drops it in. `kml_import.py` is a direct
   Python port of the coordinate-extraction regex `route_globe.m` used.
3. **Open-navdata airway graph** — `navdata_graph.py` parses the X-Plane-format
   `earth_fix.dat`/`earth_awy.dat` (waypoints + jet airways) into a graph and
   Dijkstra-searches between the fixes nearest each airport. **Not bundled**:
   this data is GPLv3-licensed, and bundling it would impose copyleft
   obligations on anyone redistributing ALAS — download it once from
   Advanced Settings → Mission Analysis (or `scripts/download_navdata.py`) to
   opt in; the GUI button asks for confirmation first given the license.
   **Known simplifications**: real SID/STAR terminal procedures
   (ARINC424/CIFP, runway-specific) aren't modeled here (tiers 1/2 are the
   way to get real SID/STAR); the airport-to-nearest-fix transition is a
   straight line; and this specific mirror's 2012 AIRAC cycle is heavily
   fragmented (2,358 disconnected sub-graphs) — many intercontinental pairs
   fall through to tier 4 for lack of a connected chain of real airway edges.
4. **Great-circle** — always available, no setup. Spherical interpolation
   (`Route.great_circle`) between `Airport.latitude_deg`/`longitude_deg`
   (added to the `Airport` dataclass in `config/airports.py` for this); point
   count is `config.mission.great_circle_points`.

None of these four tiers ever raises out of `Route.for_airports` — each is
wrapped so a failure (network error, missing file, unparseable data, no
graph path) falls through to the next tier, ending at great-circle, which
always succeeds.

### 9d. The 2D route map (formerly a 3D globe)

`reporting/visualization.py`'s `figure_mission_route_2d()` draws the route
over an equirectangular Earth texture (public-domain NASA Blue Marble,
`scripts/download_earth_texture.py`) using plain matplotlib — the "Mission &
Route" tab's "Route map (2D)" figure. Once mission data exists, the route
line is colored by total aircraft mass (matplotlib `LineCollection` +
colorbar) with a cruise-point altitude/mass annotation and ICAO endpoint
labels. `reporting/route_globe.py`'s PyVista textured-3D-globe builder
(`build_globe_plotter`, a Python port of `route_globe.m`) is a *separate*,
no-longer-GUI-wired path (the old Qt `GlobeWidget` it fed is gone with the
rest of `gui/`) — the current desktop app's "3D Route Globe" is instead a
real client-side WebGL view fed by plain JSON, not this PyVista path at all
(§12b).

`route_globe.sync_mass_to_route()` (used by both the PyVista path above and
the desktop app's `route_geo_data`, §12b) mirrors `route_globe.m`'s distance-based
synchronisation (trapezoidal-integrate TAS over time, normalise to the
route's total length, drop non-increasing samples, `np.interp`) — but unlike
the MATLAB original, which only had the KML's static altitude, it also
projects the SUAVE-simulated **altitude** onto the route (ALAS actually
has a real climb/cruise/descent trace to use, not just a guessed ramp).
`MissionResult.columns` (`integration/suave_bridge.py`) carries every column
`export_data.py` writes (flight conditions, aerodynamic coefficients/forces,
full drag breakdown, weight/fuel — the same breadth `suave_example.py`'s own
`plot_mission()` covered), keyed by CSV header name, not just the
altitude/speed/mass subset the route sync needs; `reporting/visualization.py`'s
`figure_mission_aero_coefficients`/`figure_mission_aero_forces`/
`figure_mission_drag_components` (alongside `figure_mission_profile`) surface
the rest as sub-tabs.

### 9e. GUI integration

The "Mission & Route" results tab reads `result.route`/`result.mission_result`
straight off the `PipelineResult` (Stage 5 already computed them; see §9b).
If mission analysis was disabled or unavailable, the route still renders
(recomputed locally via `Route.for_airports` if `result.route` is `None`,
using the same `config.mission` routing settings incl. `simbrief_username`)
as a plain-colored line instead of the mass-colored one —
`figure_mission_route_2d`'s `mass_profile` parameter is optional for exactly
this degradation path. The reason mission data isn't showing (SUAVE not
configured, disabled, a run error) is in the run log via `pipeline.py`'s
`progress_callback`, not re-rendered as an in-scene message.

The tab offers both a 2-D map and the 3-D globe (§12b covers which is
server-rendered vs. genuinely client-side); Payload-Range is the one
sub-view not sourced from `mission_result` — it's built straight from
`result.optimized_report`/`baseline_report`
(`physics.performance.payload_range_diagram`), so it renders even when
mission analysis is disabled.

Advanced Settings → Mission Analysis shows live status for the SUAVE venv /
navdata / Earth texture and one-click download for the latter two, backed by
the shared `alas.integration.assets` module (the same functions
`scripts/download_navdata.py`/`download_earth_texture.py` call, so there's
one implementation, not two). The navdata control asks for confirmation
first given its GPLv3 license. Provisioning the isolated SUAVE venv itself in
a dev checkout (`scripts/provision_suave_venv.py`) stays a manual one-time
terminal step — a packaged build doesn't need this at all; see §9a for how
`scripts/build_suave_env.py` bundles the same venv automatically instead.

## 10. Propulsion Analysis and the Engine Designer tab

This layer answers: *what does this engine's on-design thermodynamic cycle
look like, and how sensitive is it to the parameters that describe it?* It
is entirely optional/additive (no other stage depends on it) and, like the
Matching Chart, works from an engine's top-level design numbers rather than
a full component map.

### 10a. `EngineConfig` is the one live copy of a design's engine

Before this feature, `config/engines.py`'s `ENGINE_DATABASE` was the *only*
place thrust/BPR/OPR/FPR/TIT/TSFC lived — every consumer (mass estimation,
SUAVE vehicle sizing, the Matching Chart's T/W lookup, the payload-range
diagram's TSFC) called `get_engine(config.geometry.engine.engine_name)`
fresh, each independently. That made those seven fields **read-only in
practice** (no GUI editor for them existed) and meant a hypothetical editor
would have had to update every consumer's lookup in lockstep to stay
consistent.

`config/geometry_config.py::EngineConfig` now carries those seven fields
directly (`thrust_kn`, `bypass_ratio`, `overall_pressure_ratio`,
`fan_pressure_ratio`, `turbine_inlet_temp_k`, `cruise_tsfc_kg_kgf_hr`,
`fan_diameter_m`), alongside the nacelle/placement fields it already had.
`apply_engine_spec()` is now the **only** place `ENGINE_DATABASE` is read at
runtime — selecting an engine (Inputs tab combo, a preset, or the Engine
Designer's "Reset" button) copies that entry's values into `EngineConfig`
once; every consumer listed above was rewired to read `config.geometry.
engine.<field>` directly instead of re-querying the registry. This is what
makes editing the Engine Designer tab's fields actually reach mass
estimation, SUAVE, the Matching Chart, and the Propulsion Analysis tab all
at once, with no second copy of the data to drift out of sync — the same
"single live source" principle applied to the preset registry.

`EngineConfig` is edited on its own dedicated "Engine Designer" Advanced
Settings tab (`ScrollableForm(config.geometry.engine)`), not inside the
Geometry Scaffold tab's auto-generated form any more — `GeometryConfig.
engine`'s field metadata carries `"hide_in_form": True`, a small opt-out
`DataclassForm.__init__` checks before recursing into a nested-dataclass
field. Without it, two independent `DataclassForm` instances would each hold
their own stale copy of `EngineConfig`'s live values, and whichever one's
overlay ran last on a given preview tick would silently clobber the other's
edits. `_gather_config()` (`main_window.py`) has to splice
`engine_designer_form.get_values()` back into the geometry dict it builds,
since `geometry_form.get_values()` no longer includes an `"engine"` key at
all once that field is hidden.

### 10b. On-design turbofan cycle (`physics/propulsion.py`)

A separate-flow (unmixed), two-spool turbofan parametric cycle model —
compressor/fan/turbine thermodynamics with polytropic component
efficiencies (`config/propulsion_config.py::PropulsionCycleConfig`) —
evaluated at a single flight condition per call (`compute_turbofan_cycle`),
or swept over a grid for the Results tab's carpet plot / BPR-sensitivity /
efficiency-decomposition / altitude-sweep figures. Every equation is
first-principles compressible-flow thermodynamics (isentropic + polytropic-
efficiency relations, energy/momentum conservation); there is no semi-
empirical curve-fit table (e.g. the classic Mattingly-style installed-
thrust-lapse table by engine type/throttle setting) — ALAS already has
a separate, simpler fixed-fraction thrust lapse for the Matching Chart
(`PerformanceConfig.thrust_lapse`), and re-deriving the Mach/altitude
sensitivity from the same closed-form cycle equations (the Results tab's
"Altitude & Mach" figure) avoids introducing a second, less certain physical
model just for that one figure.

`PropulsionCycleConfig`'s defaults reproduce the exact component
efficiencies/pressure-loss factors `external tools/suave_runner/
vehicle_builder.py` hardcodes when it builds a SUAVE turbofan network (inlet
recovery, the fixed LPC/HPC pressure-ratio split, fan/compressor/turbine
polytropic efficiencies, combustor/nozzle pressure ratios) — so this
on-design cycle and SUAVE's mission-simulated engine start from the same
component-level physics. They are still two different fidelity levels (a
fast closed-form conceptual estimate vs. a numerically-solved mission
simulation), the same relationship the Model Comparison tab already
documents between AeroSandbox/SUAVE/MSES — so the tab's computed TSFC is
not expected to exactly match `EngineConfig.cruise_tsfc_kg_kgf_hr` (the
real/published reference value the payload-range diagram uses instead); the
Propulsion Analysis Cycle Summary figure's caption says so explicitly.

Validated against all 7 registered engines at their published cruise design
point: every one produces a physically valid cycle (`0 ≤ η_th, η_p, η_o ≤
1`, and `η_th·η_p == η_o` exactly, a self-consistency identity the
"equivalent velocity" treatment of a choked/underexpanded nozzle is what
makes hold — see the long comment above the thrust/efficiency block in
`physics/propulsion.py`), with computed TSFC running systematically ~20-25%
above each engine's published reference value (generic assumed component
efficiencies vs. real, further-optimized hardware) and correctly ordered by
engine generation (newer/higher-OPR engines score better on every
efficiency metric) — a conceptual-design-level fidelity gap, not a
correctness bug.

### 10c. GUI integration

"Propulsion Analysis" is an ordinary lazy figure-factory results tab (§12b):
5 figures (cycle summary + station temperatures, carpet plot, efficiency
decomposition, BPR sensitivity, altitude/Mach sweep), the same pattern every
other discipline tab uses. It reads `result.config.geometry.engine` and
`result.config.requirements.cruise_mach/cruise_altitude_m`, so it always
reflects whichever engine (preset or hand-edited) actually produced the
displayed `AnalysisReport` — "all preset engines can be analyzed" falls out
of reading the live `EngineConfig` rather than anything engine-specific in
the tab itself.

The Engine Designer Advanced Settings page pairs the editable form (two
logically distinct dataclasses — engine parameters and cycle assumptions —
since both are edited together) with a live preview: a vertically-stacked,
narrow-panel-friendly layout distinct from the wide `figure_propulsion_cycle_
summary` the Results tab uses, since squeezed into this page's narrow column
that one crowded/overlapped. Its top panel draws `EngineConfig.
nacelle_profile`'s raw (x-station, radius-fraction) control points as an
actual labeled silhouette; its bottom panel is the same cycle-summary text
as the Results tab, sourced from a shared `_propulsion_cycle_summary_lines()`
helper so the two can never show different numbers for the same design even
though they're different figure functions. A single cheap cycle evaluation
per debounce tick, no parametric sweep. "Reset" reloads the current
`engine_name`'s values from `ENGINE_DATABASE` (`apply_engine_spec()`) and
resets `PropulsionCycleConfig` to its defaults — distinct from every other
page's flat "reset to defaults", since there is no single global default
engine, only "whichever preset is currently selected".

## 11. Structural Analysis (wingbox FEM)

This layer answers: *what does this design's main wing actually deform and
stress to under maneuver loads, and what does a real NASTRAN model of it
look like?* Like MSES/Propulsion Analysis, it is entirely optional/
additive — a new pipeline stage that runs after the optimizer has already
picked a design, never feeding back into `physics/mass.py`, the CG solve,
or the optimizer's cost function. It generalizes a prior-semester project's
six hardcoded, single-aircraft reference scripts (`Reference Scripts/
00_sizing.py`..`05_validation.py`) into something that works for any
ALAS design — any number of spars at any chord fraction, any preset or
optimizer-morphed geometry, any of the built-in structural materials.

### 11a. Why this generalizes cleanly

ALAS's actual main-wing planform (`geometry/aircraft_builder.py::
_build_main_wing`) is *already* a root → break(kink) → tip trapezoidal wing
with two sweep angles and per-station dihedral — structurally the same
shape the reference scripts hardcoded for one aircraft. `geometry/
wing_structure.py::WingStructureGeometry` computes planform/dihedral with
the *exact same formulas* `_build_main_wing` uses (copied, not reinvented),
and samples airfoil shape directly from the actual built `root_section`/
`tip_airfoil` `asb.Airfoil` objects (via their own `upper_coordinates()`/
`lower_coordinates()`) instead of a hardcoded blend — so a preset or an
optimizer-morphed section (bumps, thickness/camber scale) produces a
consistent, correct wingbox automatically. Each spar (`StructuresConfig.
spar_chord_fractions`, any count) gets its own 3-point (root/break/tip)
kinked reference line for its intersection with each rib's perpendicular
cut — a deliberate unification of the reference's own asymmetric treatment
(it kinked only its rear spar's reference line, keeping the front spar a
single straight root-to-tip line; here every spar gets the same, more
general treatment).

An optional *partial-span* center spar (`StructuresConfig.
center_spar_enabled`/`center_spar_chord_fraction`, default off/0.50 — the
widebody-style root-to-kink reinforcement spar, resolved into the actual
spar list by `config/structures_config.py::resolve_spar_geometry`) gets a
2-point reference line instead (root->break only, no tip point) via a
parallel `spar_full_span` flag per spar. A rib outboard of the break simply
has no intersection for that spar (`compute_spar_intersections` returns
`None`), reusing the exact same `j_spars[i] == -1` "this spar doesn't
exist at this rib" sentinel §11b's near-root truncated ribs already rely
on — the mesh builder needed no changes at all to support it.
`structural_sizing.py::size_wingbox` zeroes that spar's local section
height beyond the break *before* computing the per-spar moment-share
fractions, so its cap area/mass/margin-of-safety all zero out past its own
extent automatically. Verified this can *reduce* total structural mass
(the front/rear spars' full-span tapered profile shrinks in proportion to
their smaller moment share, while the center spar's own material only
spans the inboard fraction of the span) — the same reason real widebody
wings use this strategy.

### 11b. The mesh is not simple

Ribs are cosine-sampled chordwise and root-adjacent ("transition") ribs are
truncated by the root plane (Y=0), so adjacent ribs generally do **not**
have equal chordwise node counts. `geometry/wing_mesh_bdf.py` ports two
mechanisms from the reference project faithfully, not simplified, because a
naive rib-to-rib mesh on this geometry is exactly what caused extreme skin
warping in the original work:

- **Zipper-triangle skin bridging**: when two adjacent ribs have different
  node counts, the shorter rib's last node becomes a shared pivot and the
  extra panels on the longer rib's side close with `CTRIA3` instead of
  `CQUAD4` — no gaps regardless of node-count mismatch.
- **RBE3 rivets on transition ribs**: a transition rib's nodes are tied to
  the 3 nearest full-length skin nodes (within a spanwise search band) via
  `RBE3`, so a physically shorter rib deforms together with the skin around
  it instead of floating disconnected.

Five geometric health checks the reference project used to catch this bug
class are reproduced and returned as a `MeshHealthReport` (not just console
prints): rib-LE perpendicularity, `CQUAD4` warping coefficient, `CTRIA3`
degenerate-triangle detection, spar XY-straightness, and a "no node at
Y < 0" check. A degenerate triangle or a Y<0 node raises (hard mesh
corruption); the other three are non-fatal warnings. `report.ok` gates only
on the warping check — the one that directly measures the "skin warping"
failure mode this section is about. A nonzero spar-straightness deviation
right at the root is an *expected, documented* characteristic, not a
defect: the root rib's spar anchor is measured along the streamwise root
chord (it's a clean streamwise SPC'd cut), while every other station's
anchor follows the perpendicular-to-LE rib direction — the two conventions
aren't perfectly collinear at that one transition, inherited from the
reference scripts' own rear-spar reference-line convention.

### 11c. Sizing: direct strength, not a mass-target search

The reference's `00_sizing.py` iteratively bisected a cap-area scale factor
to hit one assignment's specific Torenbeek mass target. `physics/
structural_sizing.py::size_wingbox` sizes caps **directly from strength**
instead — margin of safety = 0 by construction at the root (the
bending-critical station) for the governing load case — since ALAS has
no equivalent external target for an arbitrary generated aircraft, and
direct sizing is simpler *and* more physically defensible (genuinely
minimum-mass-for-given-safety-factor). The cap taper law (full root section
up to `cap_taper_eta_lock`, then linear taper to `cap_taper_tip_fraction`
at the tip) and the geometric cap-width/height limits ARE kept from the
reference — legitimate structural/manufacturing conventions, not artifacts
of the mass-target search. Rib spacing/count comes from an Euler
panel-buckling criterion (`num_ribs_override` to force a specific count
instead). Moment/shear split across N spars is weighted by each spar's
local section depth (generalizes the reference's fixed 70/30 front/rear
split).

Loads (`physics/structural_loads.py`) are one consistent model shared by
sizing, the analytical solver, and the NASTRAN BDF FORCE cards — the
reference had these disconnected. An elliptic spanwise distribution scales
to `n · mtow_kg · g / 2` per semi-wing; load factors are **not** new
fields, reused directly from `DesignRequirements.ultimate_load_factor`/
`limit_load_factor_neg` with the exact derivation `physics/performance.py`'s
V-n diagram already uses, so the structural loads always match the V-n
diagram shown elsewhere in the app. Engine point mass reuses `physics/
mass.py`'s own per-engine dry-mass formula (thrust/TWR × installation
factor), placed at each wing-mounted (`y != 0`, and only the modeled
positive-Y semi-wing's own stations — a symmetric pair would otherwise
double-count one engine's mass onto a single semi-wing node) engine
station.

### 11d. Two fidelity levels: always-available analytical, opt-in NASTRAN

`physics/structural_analysis.py::analyze_structure` is always computed
(`StructuresConfig.enabled`, default on) and needs no NASTRAN install:
Castigliano/Euler-Bernoulli spanwise deflection (with inertial relief —
structural weight + engine point masses — unlike the sizing pass, matching
the reference's own validation-stage approach) for all three load cases,
spar-cap stress margins at every station, and Rayleigh-quotient natural
frequency/mode-shape estimates. This is the theoretical, no-NASTRAN
deformation/stress path this feature exists for.

`integration/nastran_runner.py` writes SOL 101 (static)/103 (normal
modes)/111 (sine sweep + random vibration) BDFs referencing the mesh via
`INCLUDE`, and — only if `StructuresConfig.run_nastran` (opt-in, like
MSES) and `nastran_exe_path` resolves to a real executable — invokes it as
a subprocess with a timeout, scans the `.f06` for `USER FATAL MESSAGE`, and
reads the `.op2` via `pyNastran`'s own `OP2` reader. Every failure mode
(exe not found, timeout, non-convergence, unreadable `.op2`) returns a
result with a non-"ok" `status` rather than raising — identical contract to
`MSESPolarResult`/`MissionResult`. Sine/random vibration (the Miles-
equation RMS check) has **no** analytical fallback — it inherently needs a
real frequency-response solve as input — so it only ever appears once a
real NASTRAN run succeeds; every other check degrades gracefully to the
analytical estimate when NASTRAN isn't configured. `pyNastran`'s own
package metadata pins `numpy<2`, but only two renamed/removed APIs it
actually calls break against numpy 2.x — `np.in1d` (renamed `np.isin`) and
`np.chararray` (moved to `np.char.chararray`, hit only when reading a real
SOL 111 vibration `.op2`) — rather than downgrade numpy for the whole app
(risking an AeroSandbox conflict) or stand up a second isolated venv
(SUAVE's much deeper incompatibility genuinely needed that; this doesn't),
`integration/_nastran_compat.py` patches just those two names, imported
first by both `wing_mesh_bdf.py` and `nastran_runner.py`.

Each enabled SOL gets its own `work_dir/<solution>/` subfolder (mesh stays
shared at `work_dir/wing_mesh.bdf`, `INCLUDE`d via a relative
`../wing_mesh.bdf`) rather than all four sharing one flat directory —
otherwise their `.f04`/`.f06`/`.log`/`.op2` (and, on a killed run, scratch
fragments) become impossible to tell apart. Each subfolder is wiped
(`shutil.rmtree` + recreate) immediately before writing that run's `.bdf`:
NASTRAN auto-versions its own output (`.f06` -> `.f06.1` -> ...) when a
same-named file already exists there, so a subfolder left alone across
re-runs would otherwise accumulate one more generation of files every run.
A timed-out solve is force-killed with `_kill_process_tree` (Windows:
`taskkill /F /T`; POSIX: a process-group kill), not a plain
`Popen.kill()` — `nastran.exe` is a launcher that forks a second-level
`nastran.exe`/`analysis.exe` solver process which survives its own
parent's death, so a plain kill leaves an orphaned solver still running
and holding a license seat.

Real NASTRAN SOL 103 modal results are matched against the analytical
Rayleigh trial modes (§20d) by **nearest frequency**, not raw list
position (`nastran_runner._read_modes` + `visualization._nearest_freq_
index`): a real solve with `cfg.n_modes` (default 30) returns many
torsion/local-panel modes interleaved with the handful of global-bending
modes the Rayleigh trial-shape table approximates, so pairing "Rayleigh
mode N" with NASTRAN's Nth raw-order frequency routinely compares
unrelated modes. `_read_modes` also extracts each surviving mode's own
front-spar T3 displacement shape, so the Normal Modes results tab can show
the real NASTRAN mode shape next to the Rayleigh trial shape, not just its
frequency.

### 11e. GUI integration and the FEM-vs-Torenbeek accuracy check

Advanced Settings → **Structural Analysis** pairs the auto-generated
`StructuresConfig` form (spar count/positions, materials, rib pattern,
safety factor, NASTRAN path — the user-controlled inputs) with a live
preview. The preview draws `figure_structures_designer_preview` (planform +
spar lines + a compact sizing summary) from a cheap `size_wingbox()` call
only — no FEM mesh, no NASTRAN — cheap enough for every debounce tick, the
same "narrow preview distinct from the wide Results figure" pattern the
Engine Designer page establishes (§10c). Results → **Structural Analysis**
is a lazy tab (Sizing / Static Loads / Stress Margins / Normal Modes /
Vibration / Patran Renders), each sub-view built from one
`figure_structures_*` factory that degrades independently to a status
message when its data isn't available (same convention as the Model
Comparison tab). The Sizing sub-view includes a FEM-vs-Torenbeek wing mass
comparison bar — the FEM wingbox's own computed mass, doubled for both
wings, next to `physics.mass`'s Torenbeek `component_masses["Wing"]`
estimate for this same design — a read-only accuracy check ("how close is
Torenbeek to a real sized structure for this design?"), explicitly not a
feedback loop into the mass model. Patran Renders (`figure_structures_
patran`) is the one sub-view that isn't a live matplotlib plot of this
process's own data — it displays the PNG files `integration/patran_
runner.py` (§11g) already wrote to disk, one per load case.

### 11g. Headless Patran deformation-plot export (opt-in, mirrors NASTRAN)

`integration/patran_runner.py` batch-replays a generated PCL session
(`patran.exe -b -graphics -sfp <session>.ses`) to render one deformation-
plot PNG per SOL 101 load case, entirely without a GUI window — opt-in via
`StructuresConfig.patran_exe_path`/`run_patran_export`, only attempted
after a successful NASTRAN SOL 101 solve, same non-fatal contract as
everything else in this section (a failure is a non-"ok"
`PatranExportResult.status`, never a raised exception). The session
template is a direct generalization of a *real recorded interactive
session's own PCL journal* (not documentation-derived guesswork), with two
batch-mode-specific fixes baked in: it opens its working database via
`uil_file_new.go(template_db, stem)` rather than the interactively-
recorded `uil_file_rebuild.start(...)`, which raises a "journal name
conflicts" confirmation dialog that batch mode auto-denies, silently
leaving the database unopened; and it locates its own output PNG by glob
rather than an exact path, since `gm_write_image(..., "Increment", ...)`
appends its own numeric suffix to the requested filename. Each load case
renders into its own `work_dir/patran/<case>/` subfolder (own database/
session/PNG), wiped before each run for the same reason NASTRAN's own
subfolders are (§11d) — Patran's own increment-naming would otherwise both
accumulate PNGs indefinitely across re-runs *and* let a stale image from
an unrelated earlier run be mistaken for the current run's (failed)
output.

### 11f. Known gaps (documented, not silent)

- Like the MSES figures before it (see §10's own acknowledged gap), the
  `figure_structures_*` figures (including Patran Renders) aren't wired
  into the CLI's `--plots` static PNG export — `pipeline.py::_plot()` runs
  on the calling thread concurrently with the structural-analysis
  background thread, so `structural_result` isn't ready yet at the point
  `_plot()` would need it without a more invasive scheduling change.
  `cli.py::_print_structural_summary` gives headless users the sizing/
  deflection/NASTRAN/Patran-status numbers on the console either way.
- Real NASTRAN execution (subprocess solve + OP2 parsing, process-tree
  cleanup, and the real Patran batch-render pipeline) is now verified
  end-to-end against a real licensed install and a real Patran 2026.1
  Student Edition install — no longer the "cannot be exercised here" gap
  earlier revisions of this section described.
- No shear/torsion-buckling-based skin sizing exists — `t_skin_min_m` is a
  single fixed constant regardless of aircraft size or actual load, which
  a real investigation found to be a genuine (if partial) contributor to
  the FEM wingbox running measurably stiffer/heavier than a real airframe
  for at least one shipped preset (A320-200): sweeping it from 6mm to an
  unrealistic 1mm dropped semi-wing mass 3,824->2,265 kg and raised
  ultimate-load tip deflection 3.13->4.18 m, still short of the
  reported real-world ~5.4 m figure. A bounded follow-on feature, not
  attempted here.
- Patran export covers SOL 101 static deformation only — SOL 103 mode-
  shape or SOL 111 vibration renders through Patran were never attempted
  (a deliberate scope limit).

## 12. Desktop app (Wails/Go + React + Python sidecar) — the only front-end

A **Wails** desktop app (`desktop/`) whose Go shell embeds a
**React/TypeScript** webview and spawns a **FastAPI sidecar**
(`alas/sidecar/`) that wraps `DesignPipeline` and renders figures. It
was originally a port of the PySide6 interface (see the note at the top of
this document) and has since diverged from it substantially.

**Dependency direction is unchanged and strictly one-way:** `desktop/` and
`alas/sidecar/` import the core (`DesignPipeline`,
`reporting.visualization`, `physics.performance`, `config.airports`, …) but
never modify it. The sidecar is Qt-free (imports cleanly with no PySide6
present, and none is installed any more).

### 12a. Process model & handshake
`desktop/sidecar.go` runs the sidecar as a subprocess — a frozen, PyInstaller-
built `alas-core(.exe)` embedded into the Go binary at compile time for a
`wails build`, or `python -m alas.sidecar.server` via `uv run` for
`wails dev` (see §8 for the full packaging story and why both paths exist).
Either way, the server binds an OS-assigned loopback socket, prints one
`ALAS_PORT=<n>` line on stdout, and serves on that same socket. Go reads
the port and exposes it to the frontend via `GetSidecarPort()`; the frontend
(`lib/sidecarClient.ts`) polls until ready and then talks HTTP/WS directly to
`127.0.0.1:<n>` (Go is not a proxy). The pipeline itself runs off the request
thread on a background `threading.Thread` + a queue the frontend polls/streams
over WebSocket (`sidecar/runs.py`).

### 12b. Figures as SVG, live previews without a run
`sidecar/figures.py` holds two registries: `RESULT_FIGURES` (one entry per
`reporting.visualization.figure_*`) and `PREVIEW_FIGURES` (live previews
built from a config + design vector with **no** pipeline run). Extra
hand-drawn figures (Matching Chart, Landing & Take-Off) live in
`sidecar/figures_extra.py`. Figures are served as **SVG** so charts scale
crisply to fill their container (`routes_figures.py`); the 3D matplotlib
previews (exterior/cabin wireframes, three-view) accept a `view`
(elev/azim/zoom) so the webview can rotate/zoom the camera by re-rendering
server-side (settle-to-render, one render per drag pause — the mplot3d
previews are still server-rendered, unlike the route globe below). Preview
requests also carry the client's measured `width_px`/`height_px`
(`FigureCard`'s opt-in `sizeAware` prop, a debounced `ResizeObserver`); the
route handler calls `fig.set_size_inches()` before serializing so a resized
dock/ultrawide window gets a figure genuinely re-shaped to fit, not just
CSS-letterboxed. A module-level **render lock** serializes Matplotlib, which
is not thread-safe under Starlette's threadpool. `reporting/visualization.py`
themes every figure through one `_theme_figure(fig, pal)` helper.

The 3D route globe is the one exception to "figures are server-rendered
matplotlib SVG": it's a real client-side WebGL view (`RouteGlobe.tsx`, three.js
+ three-globe), fed by a plain-JSON endpoint (`GET /pipeline/{id}/route-geo`,
`figures_extra.route_geo_data`) instead of a rendered image — genuinely
rotatable/zoomable, and fast since nothing round-trips to the backend per
frame. `reporting/route_globe.py`'s PyVista off-screen-screenshot builder
(`figure_route_globe`) is left in the codebase (e.g. for a possible future
static report export) but is no longer wired into `RESULT_FIGURES`.

Result figures are fetched once per (run, figure, theme) and kept mounted
(hidden via CSS, not unmounted) once a Results tab has been opened —
`ResultsScreen.tsx` — so revisiting a tab never re-fetches or re-renders.

### 12c. Parity notes
The React `DynamicForm`/`DesignSpaceTable` reproduce the auto-generated-form and
editable-bounds mechanics; the hover nav rail ports `hover_sidebar.py`; the
onboarding overlay ports and expands `onboarding.py`; the Field Performance tab
shows the V-speed/distance data as `StatTile`s, the same component the Summary
tab's headline metrics use. Known gap: the mplot3d previews (exterior/cabin/
three-view) are still settle-to-render, not free-spinning — the route globe
(§12b) is the only genuinely client-rendered 3D view so far.

## 13. Airfoil Screening (optional analysis tool)

*"Which airfoil in the ~1600-entry UIUC database best fits this design?"* —
entirely optional and additive: nothing in the normal Run/Analyze-baseline
pipeline calls into it, and it never mutates the config it's given (every
candidate is scored against an isolated `dataclasses.replace` copy). Lives at
`alas/analysis/airfoil_screening.py`, its own sidecar registry
(`sidecar/airfoil_sweep_runs.py`, structurally identical to but entirely
separate from the main `RunRegistry`) and routes (`sidecar/
routes_airfoil_sweep.py`), and its own frontend screen
(`AirfoilSweepScreen.tsx`, Advanced Settings ▸ Analysis ▸ Airfoil Screening).

**Three stages, each more expensive and more physically trustworthy than the
last** — the core design problem this tool has to solve is that a cheap
proxy can be gamed by a section that looks great in isolation but wouldn't
actually work on this aircraft, so each stage exists to catch what the
previous one's method is blind to:

1. **2-D NeuralFoil proxy** (`_score_candidate`) scores every database entry
   at this design's cruise condition (`_cruise_condition`: CL/Mach/Reynolds
   derived from live MTOW/wing-area/MAC — no prior Run needed) — a neural-net
   forward pass, seconds for the whole database vs. the tens of minutes a
   full 3-D VLM sweep would cost. Feeds NeuralFoil the **sweep-corrected**
   section Mach (`M_inf * cos(sweep)`), not the raw freestream value — a
   swept wing's section sees a lower Mach than the freestream, and using the
   freestream value was rejecting real transport sections on an
   over-severe drag-rise result.
   A plausibility gate (a hard 0.5-30% thickness sanity bound, CD ≤ 0.30)
   discards database-parsing garbage (mislabeled multi-element high-lift
   components, bad CST fits) — deliberately loose; it is not a per-design
   suitability filter (tightening it once caused survivors to collapse from
   ~1650 to 2). The user's own *thickness window* (`min_tc`/`max_tc`, §13a) is
   applied separately on top of it, so tightening that window never mislabels a
   legitimate thin section as "degenerate".
2. **3-D wing re-simulation** (`_refine_candidate_3d`, top `refine_top_n`
   survivors, default 20): rebuilds the actual aircraft with this candidate
   as root airfoil and evaluates it with the SAME rigor
   `analysis/full_analysis.py` uses for the app's trusted reported numbers —
   a real mass analysis anchoring the CG, then `physics/stability.py`'s
   `stability_and_trim` (closed-form trim solve) +
   `AeroAnalysis.trimmed_performance` (one real non-linear VLM point at the
   solved condition), not a cheap linear extrapolation. A candidate whose
   real trimmed CL can't actually reach the required cruise CL (L ≠ W) or
   whose solved trim alpha is unrealistically far outside the probe window
   is demoted outright, not just down-ranked.
3. **MSES verification** (`_verify_candidate_mses`, top `mses_top_n` Stage-2
   survivors, default 5, opt-in): a real MSES coupled viscous/inviscid solve
   — the only stage that models shocks and true wave drag, which is what can
   actually reward a genuinely supercritical section's transonic advantage.
   Reuses `physics/mses_analysis.run_mses_polar`/
   `run_mses_pressure_distribution` verbatim (the same functions
   `pipeline.py::_run_mses_analysis` calls for the main Run's Model
   Comparison tab), at the same sweep-corrected Mach/chord-based Reynolds, so
   MSES numbers are consistent app-wide. Brackets the alpha sweep around the
   candidate's own Stage-1 2-D alpha (not the Stage-2 3-D trimmed alpha — the
   two are different physical quantities and centering on the 3-D one missed
   the real MSES-converged CL range for some candidates), retrying once at
   double the bracket width before giving up. Non-convergence is expected,
   honest solver behaviour, not a bug — same tolerance the main Run already
   has for MSES.

**Real reference airfoils as physical ground truth.** `REFERENCE_AIRFOILS`
is a curated list of 16 real, wind-tunnel-validated transonic/supercritical
sections already in the UIUC database (the NASA SC(2) family — the same
family the AVE/A380/A220 presets use as their own root section via
SC2-0714 — plus the original Whitcomb airfoil and RAE 2822). These are
**forced through Stage 2 and Stage 3 regardless of their Stage-1 score** —
unioned onto the shortlist rather than left to compete for a top-N slot —
so the algorithmic picks always have a known-good physical anchor to be
compared against (`is_reference=True` on the result, a ★ marker across every
sweep figure and the results table), instead of only ever being judged
against each other.

**Every stage degrades gracefully and never aborts the sweep**: a bad
individual airfoil (any stage), a missing MSES install, or a non-convergent
geometry all report as a per-candidate error/status field, never an
exception that kills the whole run — same contract the rest of this
codebase's optional-analysis paths (SUAVE, MSES, structures) already follow.
Per-candidate MSES pressure/Mach-contour figures (same
`visualization.figure_mses_pressure_distribution`/`figure_mses_mach_contours`
factories the Model Comparison tab uses) are available for any MSES-verified
candidate via `GET /airfoil-sweep/{run_id}/candidates/{name}/figures/
{fig_name}` — clicking an "·MSES" row in the results table opens them inline.

### 13a. Run lifecycle and cancellation

- **Run lifecycle lives in `App.tsx`.** The screening run state (`runId`,
  `result`, `running`, `status`, the options object) lives in `App.tsx`
  (`runAirfoilSweep`/`cancelAirfoilSweep`, mirroring the pipeline's
  `doRun`/`streamRun`) rather than in `AirfoilSweepScreen`'s own state: the
  sidecar's sweep runs on a daemon thread regardless of which screen is
  mounted, so component-local state would let an unrelated tab switch
  unmount the screen and silently discard a run still in progress — and
  let a second click stack a concurrent sweep on top of it.
  `AirfoilSweepScreen` is a controlled component; tab switches preserve the
  run, and the Run button can't double-start it.
- **Cooperative cancellation.** `SweepState` carries a `threading.Event`
  (`cancel()`/`should_cancel()`); `run_airfoil_screening(..., should_cancel=)`
  polls it between candidates and between stages, stopping early and
  returning the partial ranking with `AirfoilScreeningResult.cancelled=True`.
  Endpoint: `POST /airfoil-sweep/{run_id}/cancel`. A "Cancel" button appears
  while a sweep runs.
- **Physically-grounded controls**: *thickness window* (`min_tc`/`max_tc`, a
  structural/fuel-volume band applied on top of the hard sanity gate),
  *static-margin floor* (`min_static_margin`, demotes a Stage-2 candidate
  whose real trim solve is too weakly stable rather than only displaying
  it), *name/family filter* (`name_filter`, comma-separated
  substrings/globs via `_filter_names`, e.g. `"sc2, naca23"`), *off-design
  robustness* (`robustness_weight`/`cl_band`, rewards a flat drag bucket —
  `_blend_scores` folds a third normalized term in when its weight > 0,
  reducing exactly to the L/D+fuel blend at weight 0), and *NeuralFoil model
  size* + *alpha-sweep range/step*. Surfaced in a regrouped Options card
  (Objective / Filters / Fidelity / Stages) with per-control ⓘ help and a
  "Recommended pick" callout.

### 13b. In-app help and results layout

- **`InfoTip`** (`components/InfoTip.tsx`) — a circled-ⓘ affordance that shows a
  themed popover on hover/focus, portal-rendered to `document.body` (the same
  escape-the-overflow trick `FigureCard`'s modal uses) so it's never clipped.
  Wired into `DynamicForm`'s field labels (reusing the schema's existing `help`
  text — so *every* auto-generated config field gains a proper explanation) and
  the screening options.
- **`HowItWorks`** (`components/HowItWorks.tsx`) — a collapsed-by-default "How
  this works" deep-dive matching the `Advanced (N)` disclosure idiom, the app's
  "detailed walkthrough" for advanced users. Driven per form page by an optional
  `pages.ts` `detail: string[]`, and used directly on the Airfoil Screening
  page, so always-visible descriptions can stay short.
- **`HelpContext`** (`lib/helpContext.tsx`) + a persisted **View ▸ Learn-more
  help** toggle: when off, every `InfoTip` and `HowItWorks` renders nothing — an
  in-app A/B of the denser-help presentation without a rebuild.
- **Results tab strip** is `position: sticky; top: 0` inside the padded scroll
  container `.af-content`. The top padding lives on the scroll container's
  direct child rather than on the container itself, with the Results root
  (`af-results-root`) zeroing it, so the strip pins flush against the top of
  the viewport with no gap for scrolled figures to bleed through and no
  negative-margin arithmetic for the browser's compositor to round apart.
