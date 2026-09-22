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
tail is relative to it. `wing_x_shift_m` in particular is doing CG-balancing
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

ALAS uses SciPy's `differential_evolution`: a population-based,
gradient-free global optimizer, which matters because the objective here
(VLM aerodynamics → drag polar → weight closure → CG check, chained
together) isn't smooth or convex enough to trust a gradient method not to
get stuck. AVE's own solver settings:

| Setting | Value | Meaning |
|---|---|---|
| `strategy` | `best1bin` | Mutation strategy: perturb the current best candidate |
| `max_iterations` | 15 | Generations to run |
| `population_size` | 6 | Multiplier: population = 6 × 16 variables = 96 candidates/generation |
| `tolerance` | 0.01–0.05 | Convergence tolerance on the population's spread |
| `seed` | 42 | Fixed for reproducible runs (`null` = random) |
| `workers` | 1 | Set >1 to parallelize (objective must be picklable) |

Fifteen generations at a population of 96 is roughly 1,400 full pipeline
evaluations for one optimization run: each one a VLM solve plus mass
model plus penalty checks. It's small by global-optimization standards on
purpose: ALAS's objective function is expensive enough (a real VLM
solve, not a surrogate) that the search has to be efficient about where it
spends evaluations, which is exactly what the next setting is for.

## Starting near home, not from scratch

```
seed_near_initial_design: true
seed_perturbation_fraction: 0.05
```

Rather than seed differential evolution's initial population uniformly at
random across the full 16-dimensional box (the SciPy default), ALAS
by default clusters the starting population within ±5% of the *initial
design*, AVE's own baseline geometry. This is a meaningful choice: a
random population in a 16-D box this large wastes many early generations
on physically nonsensical airframes (a 60 m span paired with a 19 m root
chord, say) that the penalty terms have to reject before the search finds
its footing. Starting near a known-good design means generation 1 is
already in a sane part of the space, and the fifteen generations you *do*
spend go toward genuine improvement: visible directly in
[Optimization results](optimization-results.md)'s convergence history,
which moves fast in the first few generations precisely because it isn't
starting from noise.

## The objective function

What "improvement" means is a weighted sum of a primary term and a long
list of penalties: `ObjectiveWeights` has over two dozen fields, because
a real airframe has that many ways to be infeasible. The shape is:

$$
\text{minimize} \quad -w_{L/D} \cdot \frac{L}{D} \;+\; \sum_i \; \text{penalty}_i(\text{design})
$$

The primary term rewards cruise efficiency (`ld_weight`, default 1.0); the
penalty terms cover everything from the obvious (exceed `max_wing_area_m2`,
undershoot `min_wing_loading_kg_m2`) to the structural and geometric
(taper realism, minimum wing position on the fuselage, tail-volume
coefficients within a sane band, fuselage fineness ratio) to the physical
(CG inside the envelope at every loading condition, static margin above
the hard floor, fuel physically fits in the wing). A design that fails a
hard check (instability, a CG envelope violation) is assigned a large
fixed `failure_cost` rather than a graded penalty, so the search steers
away from it decisively rather than merely disfavoring it. The full list
of weights lives in
[Reference → Configuration](reference/configuration.md#optimizer-solversettings-objectiveweights);
you won't need to touch most of them, but knowing they exist explains why
the optimizer sometimes trades a small L/D gain for a design that's
further from every constraint boundary.
