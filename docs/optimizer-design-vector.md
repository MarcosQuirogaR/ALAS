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
| 6 | `tip_twist_deg` | deg | 0.00 | -5.0 | 1.0 | -5.0 | 2.0 | Incidence of the tip section, **absolute** and **positive nose-up**, not a washout relative to the root: the built washout is this value minus the root (and break) incidence of the geometry. Every spanwise run must be non-increasing toward the tip, so the tip may not sit above the break incidence. |
| 7 | `wing_x_shift_m` | m | 0.00 | -5.0 | 8.0 | -10.0 | 5.0 | Longitudinal shift of the wing root along `x`, for CG balance. **Positive moves the wing aft.** |
| 8 | `tail_scale` | - | 1.00 | 0.75 | 1.25 | 0.5 | 1.5 | Uniform scale on the empennage. Areas scale as the square. In reference adaptation it is derived, not searched: the tails are resized to the registered aircraft's tail volume coefficients, with the fin carrying its own derived ratio. |
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

In reference adaptation, `design_space.reference_fixed_variables` also holds
variables at the registered aircraft's values; the tail scale is always
derived. The default is `fuselage_length_m, tail_x_shift_m, bump_lower_rear`.
The rule: hold a variable only if its block-fuel sensitivity at the
registered design is below 0.1 % of block fuel per full window width on all
three aircraft below. Sensitivities are central differences at +-5 % of the
window, on the same model the search uses. Only `bump_lower_rear` qualifies.

| Variable | A320-200 | B787-9 | A380-800 |
| --- | ---: | ---: | ---: |
| `span_m` | +1.80 | -7.46 | -1.94 |
| `root_chord_m` | +4.61 | +7.71 | +6.93 |
| `break_chord_m` | +2.21 | +0.12 | +1.51 |
| `tip_chord_m` | +0.73 | -0.33 | +0.29 |
| `sweep_deg` | +0.08 | -8.71 | -1.69 |
| `tip_twist_deg` | -6.69 | -11.11 | -9.04 |
| `wing_x_shift_m` | +0.75 | +0.34 | +0.44 |
| `airfoil_thickness_scale` | +0.21 | +6.42 | +2.67 |
| `airfoil_camber_scale` | +0.92 | +1.77 | +1.18 |
| `bump_upper_front` | +0.03 | +0.74 | +0.25 |
| `bump_upper_rear` | +0.08 | +0.36 | +0.19 |
| `bump_lower_mid` | -0.01 | -0.83 | -0.29 |
| `bump_lower_rear` | +0.06 | +0.03 | +0.09 |

A first-order sensitivity at the start point does not measure flatness at the
optimum, where every interior gradient vanishes. Holding all four bumps was
measured and rejected: on the B787-9 it left the three-seed block-fuel spread
unchanged (0.69 % to 0.72 %) and raised the mean by 0.6 %. Removing a name
from the list frees that variable again.

## The objective

The scalar the search minimizes is assembled in
`crates/alas-opt/src/mdo/cost.rs`. It is dimensionless by construction:

| Objective kind | Raw unit | Normalized by |
| --- | --- | --- |
| `BlockFuel` | kg | 0.3 x the MTOW ceiling, in kg |
| `TakeoffMass` | kg | the MTOW ceiling, in kg |
| `OperatingEmptyMass` | kg | the MTOW ceiling, in kg |
| `FuelPerSeatKilometre` | kg/(seat.km) | 1.0e-3 kg/(seat.km) |

The total cost is the normalized objective plus
`preference_weight × total preference violation`. An infeasible candidate
also receives the surcharge `1.0 + total hard violation`; the objective and
preference terms remain in its total. The surcharge alone does not guarantee
feasibility ordering, because feasible objectives need not be below one.
Selection enforces that ordering through
`(feasibility tier, total normalized violation, total cost)`
(`ScoredPoint::feasibility_key`): every feasible candidate outranks an
infeasible candidate, irrespective of its scalar cost.

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

## A brief no preset describes

The shipped defaults and the global box above (span 60-80 m, root chord
12-19 m) are the AVE reference twin's. A configuration that names no preset
and changes the brief (MTOW, passengers, cruise Mach or altitude, fuselage
diameter or height, freight payload, aircraft type or aerodrome code) is
sized from class relations instead (`alas_config::clean_sheet`):

| Quantity | Relation | Class input |
|---|---|---|
| Wing area | `S = m g / (q CL)`, `q = 0.7 p(h) M^2` (ISA) | fleet-median cruise `CL` at MTOW |
| Span | `b = sqrt(A S)`, under the aerodrome code's span limit | fleet-median aspect ratio |
| Inboard LE sweep | `cos L = M_n / M`, unswept for `M <= M_n` | fleet-median normal Mach `M cos L` |
| Chords | fleet taper ratios, scaled to close `S` on the built planform | fleet-median break/root, tip/root |
| Fuselage | nose and tail cone in proportion to the diameter; cabin-sized from the layout | fleet-median ratios to diameter |
| Wing position | quarter MAC at a fixed fraction of the fuselage length | fleet-median station |
| Engines (twin) | station a fraction of the class semispan, hung by the nacelle radius | fleet-median ratios |
| Empennage | the reference twin's at constant tail volume coefficients | `S c / L`, `S b / L` |

The fleet is every registered turbofan aircraft; each relation is the
median over it. The search box is a window about the derived start: cruise
`CL` 0.7-1.3 of the median (wing area the reciprocal), aspect ratio 0.6-1.3,
root and break chords 0.7-1.45, tip chord 0.4-1.9, sweep +-8 deg, the
longitudinal shifts the global box's scaled to the fuselage length, and the
global box for the rest. Every window contains every registered turbofan
(a unit test checks it). A cabin-sized fuselage is solved over
`[nose + tail cone + diameter, 90 m]` rather than the global 65-85 m. A
registered preset, or a brief equal to the shipped one, is unaffected.

Explicit keys override the derivation variable by variable in any
clean-sheet run, with or without a preset:

```json
"optimizer": {"design_space": {
  "initial_design": {"span_m": 35.8, "sweep_deg": 27.0},
  "bounds": {"span_m": [32.0, 35.99], "fuselage_length_m": [30.0, 45.0]}
}}
```

Loading a file marks a clean-sheet brief
(`optimizer.design_space.clean_sheet_brief: true`, saved with the file so a
reload keeps the decision; an explicit `false` is kept). Only clean-sheet mode
derives: a preset-less reference adaptation or baseline sandbox gets no
derived start, cabin, exits or geometry. A named `engine_name` binds its
catalogue entry only in a clean-sheet brief.

The aerodrome code of a clean-sheet brief that states none is `auto`
(`optimizer.objective.aerodrome_reference_code`): the ICAO Annex 14 letter
whose span band holds the class span the brief implies, re-derived whenever
the brief is committed (a 72.5 t, M 0.785 brief implies 33.2 m, code C, a
35.99 m limit). An explicit letter, F included, is never replaced. When the
class span is within 5 % of the band edge the derived span box runs to the
limit. Derived boxes are clipped to the design-variable guardrails, explicit
`bounds` outside them are rejected, and every box is widened to contain the
start.

In the desktop application the derivation runs when the brief is committed,
not on every keystroke. Choosing New aircraft, or starting a run after the
brief (or any input the derivation reads: cabin seat geometry, engine,
wing, fuselage or empennage configuration) changed, marks the brief, turns a
registered aircraft's code F into `auto`, re-derives the dependent geometry
through the same function the file path uses (when the brief itself
changed), and replaces the Design Space initial values and bounds with the
derived start and box. Loading a workspace, promoting a sandbox design and
editing the Design Space keep the design values and rebuild only the bounds,
widened to contain them.

A `fuselage_length_m` bound in a cabin-sized run is the interval the length
is solved in. Direct assessment of a clean-sheet vector still admits every
vector the global box admits.

To start from the nearest registered aircraft and edit it: in the desktop
application select the preset and switch the design mode to clean sheet
("New aircraft"), which clears the preset identity and keeps its geometry
editable; from a file, name the preset, set
`optimizer.design_space.mode` to `clean_sheet`, and state the box with
`bounds` (the global box applies otherwise, and a cabin-sized fuselage is
solved over the global 65-85 m unless a `fuselage_length_m` bound is given).

The cabin of a derived brief is sized to the brief, not the other way round:

- `requirements.num_passengers` is the seat target. A named cabin style
  (`Ryanair`, `Iberia`, `Emirates`) supplies each class's pitch, seat width
  and seats abreast and the class mix; the requested total is allocated over
  that mix and the cabin becomes a count cabin, which the layout reports
  short rather than replacing. A cabin that already declares class counts
  (`class_mix_mode = "count"`) keeps them.
- The exits are the smallest arrangement whose passenger-seat allowance per
  exit in each side (CS-25 Amendment 27, 25.807(g) lead-in; the same table as
  14 CFR 25.807(g) after Amendment 25-114: A 110, B 75, C 55, I 45, II 40,
  III 35) covers the seats: at least two floor-level door pairs (Type A from a
  5.0 m body, Type C from 3.6 m, Type I below) plus up to two Type III
  overwing pairs, none above 299 seats (25.807(g)(9)). Type III exits
  together allow at most 70 seats, and two in each side fewer than three rows
  apart at most 65 (25.807(g)(6)); with no exit stations known, two overwing
  pairs are counted at 65. Fewest pairs first, then fewer doors. A 3.96 m
  body with 168 seats gets C-III-III-C (175); 176 seats needs C-C-III-C.

A registered aircraft keeps its declared exits and its geometry-derived
cabin; one without a declared arrangement, and the shipped reference brief,
keep the generic proxy (one pair per 11 m of deck).

## Reproducibility

`optimizer.solver.seed` fixes the search. Left unset, a fresh seed is taken
from the clock; the resolved seed is recorded in the result, the summary
surfaces and the run manifest so the run can be replayed after the fact.
Candidates are generated and scored in a fixed order, so a run that stops on
its evaluation budgets does not depend on `optimizer.solver.workers`.
Time-limited: the stopping point depends on machine speed and worker count;
replay with the recorded evaluation counts for a bit-identical result at any
worker count.

Native screening combines nominal neighbourhoods with full-box Latin-hypercube
coverage. Three of four draws use a quarter of each nominal window and couple
chords to draw area between the nominal and the configured area/wing-loading
cap. A chord-squared volume proxy protects the nominal tank volume while the
gross-mass Korn estimate couples sweep and thickness to cruise Mach. The wing
translation compensates the planar quarter-chord station change (body x aft,
metres) to retain the anchor's longitudinal placement. These are sampling
guides; the full model resolves the actual tank, section thickness and balance.
The remaining draws cover the declared box.
The root chord is projected onto its constructible interval. Bounds and hard
constraints stay authoritative: clipping a coupled draw can undo its target
area, and every resulting candidate still passes the pre-gate and the coupled
physics evaluation.

The refinement uses the full model to evaluate its screening seeds, retains
every analysed seed, and fills the initial population with the same coupled
distribution around its best full-model pilot anchor. Separate screening scores
never enter the full-model history. Screening feasibility is provisional;
distinct full-model candidates with zero hard violation determine the audit's
valid count. The reporting mesh independently checks the delivered finalist.

Automatic optimizer workers use the available logical cores independently of
the switch for parallel downstream branches. Stage deadlines stop new candidate
dispatches; candidates already in flight finish, allowing at most one candidate's
remaining cost per lane beyond a deadline. The seeded sample and histories are
ordered so the completed prefix can be replayed from recorded stage counts.

The measured refinement throughput sets its population schedule, rather than
reducing the configured evaluation ceiling. A feasible search continues within
the remaining clock budget when later candidates cost less than its pilot;
the recorded schedule and evaluation prefix still reproduce its trajectory.

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
