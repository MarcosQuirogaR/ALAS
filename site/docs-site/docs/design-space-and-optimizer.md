# Design space & optimizer

Requirements say what you want; the design space says how far ALAS may search
and the optimizer settings say how. This is the machinery behind
[Optimization results](optimization-results.md).

## Sixteen degrees of freedom

Every candidate airframe is a point in a 16-dimensional space. Each variable
is named, with one source for its default, bounds and unit:

| Variable | Generic default | Lower | Upper | Unit |
|---|---|---|---|---|
| `span_m` | 71.75 | 60.0 | 80.0 | m |
| `root_chord_m` | 16.50 | 12.0 | 19.0 | m |
| `break_chord_m` | 7.80 | 6.0 | 10.0 | m |
| `tip_chord_m` | 1.60 | 1.0 | 3.0 | m |
| `sweep_deg` | 34.00 | 25.0 | 45.0 | deg |
| `tip_twist_deg` | 0.00 | -5.0 | 1.0 | deg |
| `wing_x_shift_m` | 0.00 | -5.0 | 8.0 | m |
| `tail_scale` | 1.00 | 0.75 | 1.25 | – |
| `fuselage_length_m` | 76.72 | 65.0 | 85.0 | m |
| `tail_x_shift_m` | 0.00 | -2.0 | 3.0 | m |
| `airfoil_thickness_scale` | 1.00 | 0.80 | 1.30 | – |
| `airfoil_camber_scale` | 1.00 | 0.70 | 1.40 | – |
| `bump_upper_front` | 0.00 | -0.005 | 0.002 | – |
| `bump_upper_rear` | 0.00 | -0.005 | 0.002 | – |
| `bump_lower_mid` | 0.00 | -0.005 | 0.003 | – |
| `bump_lower_rear` | 0.00 | -0.005 | 0.003 | – |

Three groups:

- **Planform** (`span_m` through `sweep_deg`, `tip_twist_deg`): the wing shape,
  where most of the leverage is. Span and sweep trade induced drag against wave
  drag and structural mass.
- **Placement** (`wing_x_shift_m`, `tail_scale`, `fuselage_length_m`,
  `tail_x_shift_m`): `wing_x_shift_m` mainly keeps the loaded CG inside the
  envelope. In a reference adaptation `tail_scale` is derived from the wing so the
  tails keep the registered tail volume coefficients, and is not searched.
- **Airfoil shape** (`airfoil_thickness_scale`, `airfoil_camber_scale`, four
  `bump_*`): the four bumps are
  [Hicks-Henne](https://doi.org/10.2514/3.44235)-style local perturbations with
  small bounds (0.002 to 0.005 of chord) because they refine a shape.

## What the optimizer cannot change

Fuselage diameter, cabin cross-section, engine model, tail airfoil and the
configuration family are the **geometry scaffold**, fixed for the run. The
optimizer resizes an aircraft; it does not invent a different one. Changing the
scaffold (another preset, engine or fuselage family) is a separate action.

## The search: differential evolution

ALAS uses L-SHADE differential evolution under epsilon constraints, a
population-based, gradient-free method. The objective (VLM aerodynamics, drag
polar, weight closure, CG check, chained) is not smooth enough to trust a
gradient method. Default solver settings (stages stop on their time limit; the
evaluation ceiling is rarely reached):

| Setting | Value | Meaning |
|---|---|---|
| `screening` | 30 s, ceiling 20,000 evaluations | Space-filling sample plus the baseline; its diverse elite seeds the refinement |
| `refinement` | 120 s (at most 300 s), ceiling 20,000 evaluations | Differential evolution at full fidelity; the budget also sets the population |
| `tolerance` | 0.02 | Normalised spread below which a stagnated run counts as converged |
| `seed` | `null` | Random by default; set an integer (42 for the runs in this guide) to fix the random draws |
| `workers` | 0 | Every thread; with evaluation-budget stops the result does not depend on it |
| `stop_on_evaluations_only` | `false` | `true` ignores the time limits; a seeded run is then bit-identical on any machine and worker count |

A seed fixes the random draws, not where a time-limited stage stops. Under the
default time limits, how many candidates each stage analyses depends on machine
speed and load, and the refinement plans its population schedule from the
measured throughput, so two runs with the same seed can deliver different
designs. To reproduce a run, either stop on evaluation budgets only or replay
the stage counts the run recorded (`replay_evaluations`,
`replay_planned_evaluations`).

Each evaluation is a full coupled analysis (VLM, mass, mission sizing), so the
default 2.5 minutes buys a few hundred to about a thousand of them: a local
refinement around the preset rather than a global search.

## Screening, then refinement

The search does not scatter its population over the whole box. Screening first
samples the box (space-filling) together with the baseline design and keeps a
diverse elite. Refinement then starts from that elite, so early generations
are spent near feasible airframes instead of rejecting nonsensical ones (a 60 m
span with a 19 m root chord). The convergence history in
[Optimization results](optimization-results.md) shows the effect.

## The objective function

The search minimises one mission quantity, set by `optimizer.objective.kind`:
block fuel (default), takeoff mass, operating empty mass, or fuel per
seat-kilometre. Takeoff mass is closed by the sizing mission, so the value comes
from a flown design mission. The Results summary shows it as an **objective
tile** labelled with the objective name and unit.

Intrinsic study preferences (tail-volume windows, clean-sheet body-angle
windows) add a small scaled term, `preference_weight` (default 10). They rank
candidates against each other and never act as a constraint allowance.

## Constraints are hard

Every constraint is hard: a candidate that violates one is invalid. There is
no constraint policy to choose and no violation allowance; saved
configurations that name an older policy or allowance load with those fields
discarded, and the saved `soft_penalty_weight` is carried over as
`preference_weight`. Invalid candidates stay in the evaluation history for
inspection, and ranking always puts a feasible candidate ahead of any invalid
one. If no feasible design is found, the run reports `NoFeasibleDesign` with
the least-violating candidate as diagnostics.

## Takeoff-mass modes

`optimizer.objective.mtow_sizing` states what `mtow_kg` means:

| Mode | Meaning |
|---|---|
| `fixed_requirement` (**Hard MTOW**) | `mtow_kg` is the takeoff mass the design is checked against and a ceiling it may not exceed. Default for registered presets. |
| `sized_by_mission` | The sizing mission closes takeoff mass; `mtow_kg` is the ceiling. Default for custom configurations. |
| `mtow_band` | Required takeoff mass must stay within `T(1 + p)` and the closed mass above `T(1 - p)` for a target `T` and fraction `p` (default 0.05). |
| `payload_adjusted`, `unconstrained` | Mission closure without a mass ceiling. |

In every mode the takeoff fuel is capped at the usable tank capacity of the
candidate, so a mass budget can never be met by carrying fuel the tanks
cannot hold. An explicit saved mode is always honoured.

## Clean-sheet design

A brief with no preset is a clean-sheet design. ALAS derives the starting
design, the search bounds and the dependent geometry from the brief (seats,
range, cruise Mach, engine) instead of using a registered aircraft's values;
explicit `initial_design` and `bounds` entries override the derivation
variable by variable.

- **New aircraft.** In the desktop application, choosing *New aircraft* as the
  design mode on the Design Space page clears the preset identity and keeps the
  current geometry as the starting shape. The derivation runs when the brief is
  committed, not on every keystroke. To edit the geometry itself, use
  [Sandbox mode](sandbox.md) and promote the result: a promoted design is a
  clean-sheet design with a custom baseline.
- **Seat-count cabin.** `requirements.num_passengers` is the seat target. The
  cabin style supplies pitch, seat width, seats abreast and the class mix, and
  the exits are the smallest arrangement whose per-exit seat allowance
  satisfies CS-25.807(g).
- **Auto aerodrome code.** When the brief states no ICAO aerodrome reference
  code, `auto` picks the Annex 14 letter whose wingspan band holds the class
  span the brief implies, and that band limits the wingspan. An explicit
  letter, F included, is never replaced.

Details and the derived-box rules are in the repository file
`docs/optimizer-design-vector.md`.
