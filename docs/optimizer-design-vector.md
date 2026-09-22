# The optimizer design vector: units, frames, signs and validity

What the search algorithm is allowed to change, in what units, measured from
where, and with which sign. The search kernels treat the vector as opaque
coordinates inside declared bounds and never interpret a component, so this
document is the only place the meaning lives. It is normative for
`alas-opt`: a kernel swap must not change anything below.

Source of truth for the table: `crates/alas-config/src/design_variables.rs`,
the `design_space!` invocation. Anything here that disagrees with that macro
is a defect in this document.

## Reference frame

Aircraft axes, as declared in `crates/alas-geom/src/aircraft/section_outline.rs`:

| Axis | Direction | Origin |
| --- | --- | --- |
| `x` | aft, along the fuselage | the configured fuselage nose reference |
| `y` | toward the right wing tip | the plane of symmetry |
| `z` | up | the configured vertical datum |

The frame is right-handed. Geometry is symmetric about `y = 0`; only the
right half is generated and mirrored, so no design variable can produce an
asymmetric aircraft.

## Units

Lengths, areas, masses and forces are SI throughout the optimizer and the
physics behind it: metres, square metres, kilograms, newtons.

**Angles are the one deliberate exception.** They are degrees at the
configuration boundary, which is what the design vector is, and radians
everywhere inside the models. A component named `_deg` is degrees; anything
reaching a trigonometric function has been converted. Never pass a `_deg`
value to `sin`, `cos` or `tan` without converting it first.

Dimensionless components carry the unit `-`. They are multipliers on a
configured shape, not physical quantities, and a value of `1.0` means "as
configured".

## The sixteen design variables

`default` is the shipped nominal. `lower`/`upper` are the clean-sheet search
bounds. `preset lower`/`preset upper` are the wider guardrails a registered
preset may be adapted within (`DesignVariableSpec::preset_lower`,
`preset_upper`); they bound the *envelope*, never the search directly.

| # | Component | Unit | Default | Lower | Upper | Preset lower | Preset upper | Meaning and sign |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `span_m` | m | 71.75 | 60.0 | 80.0 | 20.0 | 90.0 | Full projected wingspan, tip to tip, so twice the semi-span. Always positive. |
| 2 | `root_chord_m` | m | 16.50 | 12.0 | 19.0 | 3.0 | 26.0 | Streamwise chord at the wing root. |
| 3 | `break_chord_m` | m | 7.80 | 6.0 | 10.0 | 2.0 | 14.0 | Chord at the trailing-edge break (yehudi). |
| 4 | `tip_chord_m` | m | 1.60 | 1.0 | 3.0 | 0.5 | 4.0 | Chord at the wing tip. |
| 5 | `sweep_deg` | deg | 34.00 | 25.0 | 45.0 | 0.0 | 45.0 | Inboard **leading-edge** sweep. **Positive is aft**, matching `WingConfig`. |
| 6 | `tip_twist_deg` | deg | 0.00 | -5.0 | 1.0 | -5.0 | 2.0 | Geometric twist at the tip. Incidence is **positive leading-edge-up** (washin), so a **negative value is washout**, the usual design intent. |
| 7 | `wing_x_shift_m` | m | 0.00 | -5.0 | 8.0 | -10.0 | 5.0 | Longitudinal shift of the wing root along `x`, for CG balance. **Positive moves the wing aft.** |
| 8 | `tail_scale` | - | 1.00 | 0.75 | 1.25 | 0.5 | 1.5 | Uniform scale on the empennage. Areas scale as the square. |
| 9 | `fuselage_length_m` | m | 76.72 | 65.0 | 85.0 | 20.0 | 90.0 | Overall fuselage length. Fixed, not searched, whenever the cabin sizes the fuselage. |
| 10 | `tail_x_shift_m` | m | 0.00 | -2.0 | 3.0 | -5.0 | 5.0 | Longitudinal shift of the empennage along `x`. **Positive moves it aft**, lengthening the tail arm. |
| 11 | `airfoil_thickness_scale` | - | 1.00 | 0.80 | 1.30 | 0.5 | 1.5 | Multiplier on root and break airfoil thickness. |
| 12 | `airfoil_camber_scale` | - | 1.00 | 0.7 | 1.4 | 0.5 | 1.5 | Multiplier on root and break airfoil camber. |
| 13 | `bump_upper_front` | - | 0.00 | -0.005 | 0.002 | -0.01 | 0.005 | Hicks-Henne bump, upper surface near 25 % chord (suction peak). Positive adds material. |
| 14 | `bump_upper_rear` | - | 0.00 | -0.005 | 0.002 | -0.01 | 0.005 | Hicks-Henne bump, upper surface near 75 % chord (shock and recovery). |
| 15 | `bump_lower_mid` | - | 0.00 | -0.005 | 0.003 | -0.01 | 0.006 | Hicks-Henne bump, lower surface near 40 % chord (belly volume). |
| 16 | `bump_lower_rear` | - | 0.00 | -0.005 | 0.003 | -0.01 | 0.006 | Hicks-Henne bump, lower surface near 85 % chord (rear loading). |

Vector order is the table order and is load-bearing: it is the order the
kernels see, the order bounds are supplied in, and the order a saved run
replays in. Adding, removing or reordering a component changes the meaning of
every stored design vector.

## What is fixed, and why that matters

Wing vertical position (`WingConfig::root_z_m`, default `-2.1` m) is **not** a
design variable. The optimizer cannot raise or lower the wing on the
fuselage, so high-, mid- and low-wing layouts are a configuration choice, not
a search outcome. Engine architecture is fixed the same way.

## The objective

The scalar the search minimizes is assembled in
`crates/alas-opt/src/mdo/cost.rs`. It is dimensionless by construction:

| Objective kind | Raw unit | Normalized by |
| --- | --- | --- |
| `BlockFuel` | kg | 0.3 x the MTOW ceiling, in kg |
| `TakeoffMass` | kg | the MTOW ceiling, in kg |
| `OperatingEmptyMass` | kg | the MTOW ceiling, in kg |
| `FuelPerSeatKilometre` | kg/(seat.km) | 1.0e-3 kg/(seat.km) |

Constraint violations are added as a weighted penalty. A candidate that
violates a hard limit and is not admitted by the relaxation policy costs
`1.0 + total hard violation`, which places it above any normalized feasible
objective, so feasibility always outranks quality. Ordering is by the triple
`(infeasible, violation, cost)` (`ScoredPoint::feasibility_key`).

## Bounds and repair

Every kernel receives `(lower, upper)` per component and must keep every
evaluated candidate inside them. Two repairs are used, deliberately
different:

- **Reflection** for a mutant a difference vector pushed out of the box
  (`constrained_de::reflect_into_bounds`). A component that overshoots by a
  little lands a little inside the bound it crossed. The fold is a pure
  function of the mutant, so repair does not consume the random generator and
  a seeded run replays exactly.
- **Clamping** for a caller-supplied starting point
  (`constrained_de::clamp_into_bounds`). The nearest admissible design is the
  honest repair for a point that was chosen deliberately.

Non-finite values fail closed to the lower bound rather than propagating.

## Reproducibility

`optimizer.solver.seed` fixes the search. Left unset, a fresh seed is taken
from the clock and the run is **not** reproducible; the resolved seed is
written to the progress log so a run can be replayed after the fact. The
Differential Evolution driver evaluates candidates one at a time in a fixed
order, so its result does not depend on `optimizer.solver.workers`.

## Known gap: no geometric interference constraint

Nothing in the optimizer checks that the wing does not intersect the
fuselage. `docs/methods.md` states plainly that the exposed-area correction
"is not a mesh intersection" and does not resolve dihedral crossing the body
surface; it is an aerodynamic correction, not a feasibility test. There is
also no residual relating `wing_x_shift_m` or `tail_x_shift_m` to
`fuselage_length_m`, so longitudinal placement is bounded only by the
component ranges above, which are not derived from the fuselage the wing is
being placed on.

The consequence is directional: a search that is better at finding the edges
of the declared box is also better at finding geometry the box does not
forbid. Bounds are not a substitute for an interference constraint, and no
result from this optimizer should be read as evidence that a candidate is
manufacturable.
