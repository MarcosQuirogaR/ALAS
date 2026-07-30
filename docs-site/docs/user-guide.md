# User guide

This is the complete tour of the application: every page in the navigation,
every control that does something, and every action available to you. It is
organised the way the interface is, so you can read it front to back or jump
to whichever page you are looking at.

If you have not installed ALAS yet, start with
[Installation](installation.md). If you want to understand *why* the
application is arranged this way, [How ALAS works
inside](architecture.md) explains the machinery underneath.

## The window

The application is one window with four parts:

| Region | What it is |
|---|---|
| **Menu bar** | File, View and Help menus: configuration files, theme, zoom, and the walkthrough |
| **Navigation sidebar** | Three groups (Setup, Advanced Settings, Results), containing every page below |
| **Main panel** | Whichever page is selected |
| **Control bar** | The **Run** button, run progress and the live log |

Many pages also show a **live preview** beside the form: a small chart that
redraws as you edit, so you can see the effect of a change without starting a
run.

!!! tip "Field labels are the real reference"
    Hover (or focus) almost any field's label in the application for its own
    explanation, written against the code that consumes it -- no separate icon,
    the label itself is the hover target. Pages that need more than a sentence
    also have a collapsible **How this works** panel. This guide is the map;
    those labels are the territory.

---

## Setup

The four pages you need before a first run.

### Inputs

The mission you want flown, and the aircraft family that will fly it.

- **Aircraft preset**: seven airliners ship with the application: **AVE**
  (the long-range reference twin), **A340-300**, **A380-800**, **B787-9**,
  **A320-200**, **A220-300** and **DC-10**. Choosing one swaps the geometry
  scaffold, engine, cabin defaults and design-space bounds together, as a
  consistent set. It is the fastest way to start from something sane.
- **Engine**: selecting a different engine re-scales nacelle geometry,
  mounting position and the wetted-area drag contribution automatically.
- **Cruise Mach and altitude**: the design point everything is sized
  around.
- **Maximum take-off weight**: the budget the entire mass build-up must
  close against.
- **Aircraft type**: `passenger` or `cargo`, which switches the whole
  payload model and the cabin preset list along with it.
- **Cabin preset and passenger count**: named layouts for passenger
  aircraft, or a payload target for freighters. The passenger count becomes
  editable only when the cabin preset is set to `Custom`, because otherwise
  the preset is what determines it.
- **Departure and arrival airports**: the route flown by
  [mission analysis](mission-and-route.md).

Fields marked *advanced* (structural load factors, sizing constraints, the
stability target, the CG envelope width) are on this page too, below the
primary ones. [Mission requirements](mission-requirements.md) explains what
each one constrains.

### Design Space

The sixteen geometric degrees of freedom the optimizer is allowed to move,
as a table. Each row gives you:

- an **initial value**: the starting design, and the one analysed when you
  skip optimization;
- a **lower and upper bound**: the box the search may explore.

Both are editable. **Reset to defaults** restores the shipped values for the
current preset. Widening a bound gives the optimizer more freedom and a
larger space to get lost in; narrowing one is how you say "I have already
decided this". [Design space & optimizer](design-space-and-optimizer.md)
lists all sixteen with their defaults.

### Analyses

One page of switches deciding which optional stages run: mission analysis,
the MSES transonic solve, structural analysis. Turning a stage off makes runs
faster; turning one on costs time but fills in a results tab. Nothing here
changes the aircraft, only how much is computed about it.

### External Tools

Where the external programs live and whether they were found: **AeroSandbox**,
**SUAVE**, **MSES**, **Nastran**, **Patran** and **SimBrief**. Each shows a
status, so you can tell at a glance what is available and therefore what a run
will be able to compute.

AeroSandbox is a hard dependency and always present. MSES ships with the
application. SUAVE needs its one-time isolated environment. Nastran and Patran
are yours to license and install; the page links out to Hexagon for both, and
to SimBrief for a flight-plan account.

!!! note "Asset downloads are on the Mission Analysis page"
    The two optional data downloads (the open airway navdata and the Earth
    texture for the route map) sit on **Advanced Settings → Mission
    Analysis**, beside the routing settings that use them, rather than here.
    The navdata download asks for confirmation first, because it is
    third-party GPLv3 data rather than something bundled with the
    application.

---

## Advanced Settings: Modeling

Eight pages describing the aircraft and its physics. All have sensible
defaults; you can run without touching any of them.

### Drag model

The coefficients behind the parasite and wave-drag build-up: the minimum
parasite-drag floor, and the drag-divergence behaviour that decides how
sharply drag rises toward cruise Mach. Raise the technology factor for a more
modern supercritical wing. Induced drag is not set here; it comes from the
vortex-lattice solution.

*Preview: drag against Mach.*

### Geometry scaffold

The fixed geometry the design vector morphs against: fuselage shape and
dimensions, empennage layout, nacelle placement. This is the part of the
aircraft the optimizer may **not** change. Editing it is how you define a
different aircraft family rather than resize the current one.

*Preview: three-view schematic.*

### Mass model

Assumptions behind the Torenbeek component-mass build-up and the
centre-of-gravity solver: structural fractions, margins and reference values.

*Preview: CG envelope.*

### Landing gear

Main- and nose-gear placement, strut sizing, and the tip-over and strength
assumptions. These feed the nose-gear steering and tip-over boundaries drawn
on the [CG envelope](weight-balance-and-stability.md#the-cg-envelope-across-the-whole-flight).

*Preview: landing-gear planform.*

### Engine Designer

On-design turbofan cycle assumptions: component efficiencies and pressure
ratios. The engine *model* is chosen on the Inputs page; this page tunes the
thermodynamics behind it, feeding
[Propulsion analysis](propulsion-analysis.md).

*Preview: engine cycle.*

### Cabin & Payload

The passenger class mix (first, business, premium, economy, each with its
own seat count, pitch, width and mass) or, for a freighter, the cargo deck
configuration, container types and loading strategy. See
[Cabin & payload](cabin-and-payload.md).

### Control Surfaces

Chord and span fractions for slats, flaps, ailerons, spoilers, elevator and
rudder.

!!! note "These are representational"
    Control-surface settings drive the sizing diagram only. They are not fed
    into the aerodynamic or mass models, so changing them will not move your
    drag or weight numbers.

*Preview: control-surface layout.*

### Structures

Wingbox sizing: materials, spar positions, rib spacing and the load cases.
The analytical solver always runs and needs nothing installed. Set the
Nastran path and enable the solver here to additionally run a real
finite-element solve. See
[Structural analysis](structural-analysis.md).

*Preview: wingbox planform.*

---

## Advanced Settings: Analysis

Six pages controlling how hard the program works, rather than what it is
working on.

### Optimizer & weights

Solver settings (strategy, population size, iteration count, tolerance,
random seed, worker count) and the objective weights described in
[Design space & optimizer](design-space-and-optimizer.md#the-objective-function).

Two settings deserve attention. **Seeding near the initial design** starts the
population close to a known-valid aircraft instead of scattering it across the
whole box, which is usually what you want. **Seed** fixes the random sequence:
set it if you want two runs to be comparable.

A **solver preset** picker fills these fields with a coherent set (a quick
exploratory search versus a thorough one) instead of making you tune eight
numbers individually.

### Analysis fidelity

How finely the winning design is evaluated: the angle-of-attack sweep range
and point count, and the vortex-lattice panel densities for the coarse
(in-loop) and fine (final) passes. A **fidelity preset** picker offers
matched sets.

### Performance

High-lift and field-performance constants (maximum lift coefficients, thrust
lapse, engine-out climb gradient, landing constant) behind the matching
chart and the take-off and landing results.

### Mission Analysis

Everything about the flown mission: whether it runs, where the isolated SUAVE
environment lives, the route and asset directories, the subprocess timeout,
and the complete speed profile: every climb rate, cruise-leg speed and
descent step. See [Mission & route analysis](mission-and-route.md).

Routing is attempted in order of fidelity: an imported flight plan, then the
airway graph if the navdata has been downloaded, then a great-circle track.
Each falls through to the next, so a route always renders.

### MSES Analysis

The optional transonic section solve: whether it runs, and the sweep it runs
over. The section sees the sweep-corrected Mach number, not the freestream
value. If MSES reports the target lift coefficient outside its converged
range, widen the sweep half-width. See
[Transonic section analysis](transonic-analysis.md).

### Airfoil Screening

The one page in Advanced Settings that is a tool rather than a settings form.
It searches the section catalogue against your current design and returns a
ranked shortlist. The process behind it is described in
[Airfoil screening](airfoil-screening.md).

Controls: the scoring weights (lift-to-drag, fuel volume, robustness),
thickness bounds, a name filter, a static-margin floor, how many candidates
proceed to the three-dimensional pass, and how many finalists get an MSES
check. A screening run reports progress and **can be cancelled**, the
one long-running operation with a cancel button, because the full sweep is
worth interrupting when you realise a filter was wrong.

---

## Running

The **Run** button in the control bar starts the pipeline. Progress streams
into the log as each stage completes, and the button becomes **Running…**
until the run finishes.

There is also **Analyze baseline**, which skips the optimizer entirely and
analyses the initial design. Use it constantly: it is fast, and it is the
right way to check that a preset or a hand-edited design space is sane before
committing to a full search.

If the log reports a stage as unavailable (mission analysis without its
environment, Nastran without a licence), the run has not failed. Optional
stages degrade individually and everything else completes.

---

## Results

Results are grouped by discipline. Which tabs appear depends on what ran.

| Tab | Contents |
|---|---|
| **Summary** | The design point as text, with the key numbers |
| **Optimization** | Convergence history, design evolution, planform and polar comparison against the baseline |
| **Aerodynamics** | Lift/drag/moment sweep, drag breakdown, span loading, VLM streamlines, airfoil comparison and spanwise evolution, section behaviour against Reynolds number, wireframes |
| **Weight & Balance** | Mass breakdown, mass distribution, CG envelope, static margin, fuel-volume check, landing-gear planform, cabin and payload layout |
| **Propulsion** | Cycle summary, carpet plot, efficiency decomposition, bypass-ratio sensitivity, altitude and Mach sweep |
| **Structures** | Wingbox sizing, static loads, stress margins, normal modes, vibration, Patran renders |
| **Mission & Route** | Ground track, mission profile, airspeeds, flight path, aerodynamic coefficients and forces, drag components |
| **Model Comparison** | AeroSandbox, SUAVE and MSES on shared axes |
| **MSES** | Surface pressure distribution and Mach contours |
| **Field Performance** | Take-off and landing at the departure and arrival airports |

A tab that could not be produced says **Not available for this run** and
explains why, rather than disappearing.

### Getting results out

Three exports, all from the Results view:

- **Generate PDF report**: a bound document of the whole run.
- **Figure archive**: every chart as a ZIP, at publication resolution.
- **Design database**: the machine-readable JSON written automatically to
  the output folder, along with the winning section coordinates and, if a
  mission ran, the flight-data CSV.

[Reporting & export](reporting-and-export.md) describes each format and what
is in it.

---

## Menus and view

**File**: *Load configuration…* and *Save configuration…*. Configurations
are YAML, so a setup is reproducible, diffable and shareable. Saving writes
everything currently set; loading applies a file over the defaults.

**View**: *Light*, *Grey* and *Dark* themes, and *Zoom in* / *Zoom out*.
Figures follow the theme, so charts stay legible in whichever you pick and
exported images match what you saw.

**Help**: *Replay Walkthrough* restarts the guided introduction. There is
also a **How it works** toggle that reveals the deeper explanation panels
throughout the application; turn it on when you want the reasoning and off
when you want a clean form.

---

## A suggested first session

1. Open **Inputs**, choose a preset close to what you have in mind, and set
   your cruise point, weight and payload.
2. Press **Analyze baseline**. Read the Weight & Balance tab. Does the CG
   envelope look sane? Is the static margin plausible? If not, the problem is
   in your inputs, and no amount of optimizing will fix it.
3. Open **Design Space** and narrow anything you have already decided.
4. Press **Run**. Watch the log.
5. Read **Optimization** first. Did the search actually improve anything, or
   did it converge immediately? Then work through the discipline tabs.
6. **Save configuration** so you can get back here, and **Generate PDF
   report** so you have the run written down.

The mistake worth avoiding is going straight to a full run with unexamined
inputs. The baseline pass exists precisely so you can catch a bad assumption
in twenty seconds instead of twenty minutes.
