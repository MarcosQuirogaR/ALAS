# Design space & optimizer

Once requirements say *what* you want, the design space says *how far
ALAS is allowed to search* to get it, and the optimizer settings say
*how* it searches. This is the machinery behind
[Optimization results](optimization-results.md). Read this chapter first
and that one will make sense as cause and effect rather than a chart to
take on faith.

## Sixteen degrees of freedom

Every candidate airframe ALAS considers is a point in a 16-dimensional
space. The original reference scripts this project grew from addressed
these by magic array index (`x[10]`, `x[4]`); ALAS names every one of
them, with a single source of truth for its default, bounds, and units:

| Variable | AVE default | Lower | Upper | Unit |
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

They fall into three groups:

**Planform** (`span_m` through `sweep_deg`, `tip_twist_deg`): the wing's
basic shape. This is where most of the optimizer's leverage lives: span
and sweep trade induced drag against wave drag and structural weight most
directly.

**Placement** (`wing_x_shift_m`, `tail_scale`, `fuselage_length_m`,
`tail_x_shift_m`): where the wing sits on the fuselage and how big the
tail is relative to it (in a reference adaptation `tail_scale` is derived
from the wing so the tails keep the registered tail volume coefficients, not
searched). `wing_x_shift_m` in particular is doing CG-balancing
work, not aerodynamic work: moving the wing fore/aft to keep the loaded
CG inside the stability envelope as everything else changes.

**Airfoil shape** (`airfoil_thickness_scale`, `airfoil_camber_scale`, the
four `bump_*` variables): fine control over the 2D section. The four bump
variables are [Hicks-Henne](https://doi.org/10.2514/3.44235)-style
localized perturbations to the airfoil's upper/lower surface: deliberately
tiny bounds (±0.002–0.005) because they're refining a shape, not
redesigning it. `bump_upper_rear`, for instance, nudges the region a
transonic shock tends to sit in.

## What the optimizer is *not* free to change

Fuselage diameter, cabin cross-section, engine model, tail airfoil, the
family of geometry (this-is-a-twin-jet-airliner): all of that is the
**geometry scaffold**, set once per run and held fixed while the optimizer
searches. It's the difference between "resize this aircraft" and "invent a
different aircraft." ALAS does the former. Swapping the scaffold
(different fuselage family, different engine, a different preset entirely)
is a deliberate, separate action, not something differential evolution
stumbles into mid-search.

## The search: differential evolution

ALAS uses differential evolution (L-SHADE under epsilon constraints): a
population-based, gradient-free global optimizer, which matters because the objective here
(VLM aerodynamics → drag polar → weight closure → CG check, chained
together) isn't smooth or convex enough to trust a gradient method not to
get stuck. AVE's own solver settings:

| Setting | Value | Meaning |
|---|---|---|
| `screening` | 30 s, 2000 evaluations | Space-filling sample plus the baseline; its diverse elite seeds the refinement |
| `refinement` | 120 s (at most 300 s), 600 evaluations | Differential evolution at full fidelity; the budget also sets the population |
| `tolerance` | 0.02 | Normalised spread below which a stagnated run counts as converged |
| `seed` | 42 | Fixed for reproducible runs (`null` = random) |
| `workers` | 0 | Every thread; the result does not depend on it |

Each evaluation is a full coupled analysis (VLM, mass, mission sizing), so
the default two-and-a-half minutes buys a few hundred of them: a local
refinement around the preset, not a global search. It's small by global-optimization standards on
purpose: ALAS's objective function is expensive enough (a real VLM
solve, not a surrogate) that the search has to be efficient about where it
spends evaluations, which is exactly what the next setting is for.

## Starting near home, not from scratch

```
seed_near_initial_design: true
seed_perturbation_fraction: 0.05
```

Rather than seed differential evolution's initial population uniformly at
random across the full 16-dimensional box, ALAS
by default clusters the starting population within ±5% of the *initial
design*, AVE's own baseline geometry. This is a meaningful choice: a
random population in a 16-D box this large wastes many early generations
on physically nonsensical airframes (a 60 m span paired with a 19 m root
chord, say) that the constraints have to reject before the search finds
its footing. Starting near a known-good design means generation 1 is
already in a sane part of the space, and the fifteen generations you *do*
spend go toward genuine improvement: visible directly in
[Optimization results](optimization-results.md)'s convergence history,
which moves fast in the first few generations precisely because it isn't
starting from noise.

## The objective function

The search minimises one mission quantity, chosen with
`optimizer.objective.kind`: block fuel (the default), takeoff mass, operating
empty mass, or fuel per seat-kilometre. The takeoff mass is closed by the
sizing mission, so the value is the result of a flown design mission, not a
proxy. The Results summary shows it as an **objective tile** whose label
carries the objective name and unit (for example block fuel in kg, or fuel per
seat-kilometre) and whose tooltip states how the value is defined.

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

- **New aircraft.** In the desktop application, selecting a preset and
  choosing *New aircraft* clears the preset identity and keeps its geometry
  editable. The derivation runs when the brief is committed, not on every
  keystroke.
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
