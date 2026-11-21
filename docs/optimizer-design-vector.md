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

The MTOW band mode appends a seventeenth coordinate to the flat vector, the
takeoff mass `m` in kg (`DesignVector::mtow_kg`, bounded by `[T (1 - p), T (1 + p)]`,
seeded at `T`). It is not one of the sixteen geometric variables below: it is
`None` and absent from the vector in every other mode.

`default` is the shipped nominal. `lower`/`upper` are the clean-sheet search
bounds. `preset lower`/`preset upper` are the wider guardrails a registered
preset may be adapted within (`DesignVariableSpec::preset_lower`,
`preset_upper`); they bound the *envelope*, never the search directly.

| # | Component | Unit | Default | Lower | Upper | Preset lower | Preset upper | Meaning and sign |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `span_m` | m | 71.75 | 60.0 | 80.0 | 20.0 | 90.0 | Full projected wingspan, tip to tip, so twice the semi-span. Always positive. |
| 2 | `root_chord_m` | m | 16.50 | 12.0 | 19.0 | 2.0 | 26.0 | Streamwise chord at the wing root. |
| 3 | `break_chord_m` | m | 7.80 | 6.0 | 10.0 | 2.0 | 14.0 | Chord at the trailing-edge break (yehudi). |
| 4 | `tip_chord_m` | m | 1.60 | 1.0 | 3.0 | 0.5 | 5.0 | Chord at the wing tip. |
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
a search outcome. Engine architecture (count, stations, cycle) is fixed the
same way; the engine *rating* is not a coordinate either, but in clean-sheet
mode it is a sized output of every evaluation (see "Engine sizing" below).

In reference adaptation, `design_space.reference_fixed_variables` also holds
variables at the registered aircraft's values; the tail scale is always
derived. The default is `fuselage_length_m, tail_x_shift_m, bump_lower_rear`.

`tip_twist_deg` is held at its nominal value in every mode: the trimmed drag table
takes the induced drag of the twist as the handbook allowance for a non-optimum twist
(Jenkinson, Civil Jet Aircraft Design, 1999, pp. 179-180), and no published form gives
that allowance as a function of the departure from the optimum distribution, so a
search over the twist has no sourced drag to work with.

The wing-section shape variables (`airfoil_thickness_scale`, `airfoil_camber_scale`
and the four Hicks-Henne bumps) are held at their nominal values in every mode. The
in-loop model carries thickness into the wing mass, tank volume, form factor and
compressibility drag, and camber into the lattice and trim, but neither into the
section maximum lift (CLmax scales the declared coefficients with planform terms
only) nor into a section drag polar, so a thinner or more cambered section would be
credited its drag and lift without the loss of maximum lift with leading-edge
sharpness (USAF DATCOM Sec. 4.1.3.4) or the profile drag away from the design lift
(Abbott and von Doenhoff, Theory of Wing Sections, Ch. 7). The ATR 72-600 search of
2026-11-18 put camber +10 % and thickness -10 % on the window edges.

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

- **Midpoint repair** for a trial component a difference vector pushed out of
  the box (`search_methods::lshade_de_ops::repair_midpoint`). A component
  outside a bound is placed halfway between the crossed bound and the
  parent's value. The repair is a pure function of the mutant and the parent,
  so it does not consume the random generator and a seeded run replays
  exactly.
- **Clamping** for a caller-supplied starting point or seed
  (`search_methods::clamp_to_bounds`). The nearest admissible design is the
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

A brief whose `engine_name` is a turboprop takes the regional turboprop
class instead of the turbofan fleet and the jet defaults
(`alas_config::clean_sheet::turboprop_class`; values, pages and the
sources are in that module's documentation). The class is the median of the
four regional turboprop transports whose entries state the quantity in
Jane's 2008-2009 (ATR 42-500, ATR 72-500, Dash 8-300, Dash 8 Q400):

| Quantity | Value |
|---|---|
| Aspect ratio, wing loading | 12.40, 355 kg/m2 (area `S = MTOW / 355`), floor 331.7 kg/m2 |
| Planform | constant-chord centre section (break chord = root), tip/root 0.584, straight |
| Section | NACA 23018 / 23012, conventional class, centre-section incidence 2 deg |
| Engine station | 0.317 semispan |
| High lift | `CLmax` take-off 2.07, landing 2.71 (Sadraey Table 5.14, double-slotted flaps) |
| Ultimate load factor | `1.5 n`, `n = 2.1 + 24000/(W + 10000)` in [2.5, 3.8], `W` the MTOW in lb (14 CFR 25.337(b); 3.75 above 50,000 lb; evaluated at the brief's MTOW, while the envelope check reads the closed mass) |
| FLOPS block | `VMAX` 1.25 cruise Mach, attendants `ceil(seats/50)`, two tanks, no APU, bulk holds, short/medium haul, `FMXTOT` 0.242 MTOW, certified ceiling 7,620 m |
| Route | the curated pair of ordinary aerodromes whose longest generated route (great circle times `mission.max_airway_stretch`) is nearest below the design range, when no airport is named |

Explicit keys still win. The engine is the catalogue entry named by the
brief, which is the baseline cycle; its rating is not a design-vector
coordinate but is sized from the thrust requirements of every clean-sheet
evaluation (see "Engine sizing" below), so the class start only needs an
engine of the right class.

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

## Geometric interference constraints

The optimizer does not run a mesh intersection test: `docs/methods.md` states
plainly that the exposed-area correction "is not a mesh intersection" and does
not resolve dihedral crossing the body surface; it is an aerodynamic
correction, not a feasibility test. The built geometry is instead checked by
hard residuals, all Geometry-family constraints:

- `crates/alas-opt/src/mdo/residuals_layout.rs`: wing root leading and trailing
  edges on the fuselage (`wing_root_le_on_fuselage`,
  `wing_root_te_on_fuselage`), the wing apex fraction
  (`wing_apex_fraction`), the wingbox root depth fitting the fuselage
  (`wingbox_root_depth_fits_fuselage`) and the root within the body envelope.
- `crates/alas-opt/src/mdo/residuals_hold_clearance.rs`: on a redesign of a
  registered aircraft whose preset declares its lower holds by station, the
  centre box and gear bay (front spar to side-of-body trailing edge) reach no
  further into any declared hold than the registered aircraft's own do
  (`wing_box_clear_of_lower_holds`, m, feasible iff at most zero).
- `crates/alas-opt/src/mdo/residuals/interference.rs`, gaps in metres on the
  built airframe, feasible iff non-negative:
  `nacelle_clear_of_fuselage` and `nacelle_clear_of_nacelle` (no podded engine
  body overlaps the fuselage or another engine body: the exact minimum
  separation of the two superellipse section boundaries, 64-point polylines,
  at every common station, so offset and elongated sections are judged as
  they are and not along the line between their centres); `main_gear_clear_of_nacelle`
  (the extended main wheels, sized tyre diameter and width at the layout's
  wheel stations, against every engine body at the stations each wheel
  spans, on the static ground shape); `nacelle_clear_of_wing` (the top of a nacelle hung under the wing
  stays below the wing's lower surface); `wing_tip_ground_clearance_in_roll`,
  `nacelle_ground_clearance_in_roll` and `tail_ground_clearance_in_roll`
  (Raymer ch. 11, p. 342: no strike at the tail-down attitude in a 5 degree
  roll about the outboard main wheel, on the static ground shape; the
  nacelle and tail extension is this module's, judged at both level and
  tail-down attitude); `tail_root_attached` (the horizontal-tail root
  mid-chord point lies inside the fuselage or the fin).

The consequence is directional: a search that is better at finding the edges
of the declared box is also better at finding geometry the box does not
forbid. Bounds are not a substitute for an interference constraint, and no
result from this optimizer should be read as evidence that a candidate is
manufacturable.

All registered presets pass them. Eleven stand on a registered `fuselage_ground_clearance_m`:
E195-E2 1.65 m (Embraer APM 5824 Table 2.7, row 61,700 kg, CG 34 %: nose, nacelle and wing-tip
heights, which agree within 0.08 m; the table of the E190-E2 is Table 2.4), C919 1.46 m (COMAC
airport characteristics Table 1 item K), DC-10 2.26 m (Boeing ACAP sec. 2.3 item D), ATR72-600
0.64 m (ATR Aircraft Recovery Manual Figure 1-28: propeller hub, propeller tip and wing tip,
+-0.07 m; the wing-tip point alone gives 0.55-0.58 m) and A400M 1.21 m (brochure side view against
its 14.7 m height rule, +-0.05 m; the propeller tip reads 0.05-0.15 m higher than the model). The
notional AVE declares 2.44 m from the Boeing 777-9 airport-planning ground clearances (D6-86073
Rev G Table 2-2: each row's min-max pair gives an interval for the belly clearance; fuselage top
A, engine low point F and wing tip K intersect in 2.43-2.45 m; the tail rows lie above it and are
a geometry card). A user aircraft with no declared clearance is judged at both ends of the
documented sample of its class (`alas-mass` `GroundPlaneBasis::{Low, High}`; jets 0.298-0.546 D
from nine aircraft, turboprops 0.216-0.223 D from the A400M and ATR 72-600 (a turboprop of another layout,
low wing or nacelle gear, takes the full 0.216-0.546 D and the feasibility report flags it); 0.35 D and 0.22 D are
the nominal values of reports): the interference rows and the CG envelope are evaluated at both
ends and the worse result is kept, because no end is conservative for every row (nacelle, wing-tip
and propeller gaps are worse with a small clearance, the tail gap and the tip-back and turnover
limits with a large one, and the rotation boundary moves either way: E195-E2 and AVE probes). The
gear mass of a sponson-legged body stands on the large end. The FLOPS
eq. 66 gear-leg length was tried as a geometric derivation of the AVE plane and misses the
documented aircraft by -0.3 to +3.5 m. A turboprop also gets `propeller_ground_clearance`
(lowest blade tip at least 18 cm above ground, level static attitude, CS 25.925(a))
and `propeller_ground_clearance_tyre_deflated` (positive clearance with the critical tyre flat
and its strut bottomed: Raymer ch. 11 Eq. 11.12 stroke at 10 ft/s sink, load factor 2.7,
efficiency 0.75, 34 % of the stroke below static, flat-tyre drop from the rim ratio of the cited
46x17.0R20 tyre; the lower of a nose-gear and a main-gear case).

Evidence still missing, tracked as defects rather than described away: the
gear against the deployed flaps and the gear bay (no public flap deflection,
hinge and track geometry per type to validate a hard row against, so a row
would fail the registered presets on assumptions, not geometry), the pylon
and fairing (Obert, *Aerodynamic Design of Transport Aircraft*, Fig. 12.34
gives only a wind-tunnel boundary of nacelle position in H/C against X_F/C, an
interference-drag rule that CFD-based installations (Fig. 12.35) pass, not a
physical gap) and the horizontal tail against the exhaust plume (no sourced
turbofan exhaust spreading law; the NASA free-jet reports read are laminar or
low-Reynolds-number). None of the rows is a surface intersection test, and no
result from this optimizer is evidence that a
candidate is manufacturable.

## Engine sizing (clean-sheet mode)

A clean-sheet brief owns thrust requirements, not an engine. With
`optimizer.design_space.engine_sized_by_requirements` on (the default), every
evaluation scales the selected catalogue engine, the baseline cycle, to the
smallest rating that meets the thrust requirements of the closed airframe
(`crates/alas-opt/src/mdo/engine/sizing.rs`):

- Requirements: the CS 25.113 take-off field against the departure's take-off
  distance available, the engine-out second segment, final segment and
  approach climb, the landing climb, and level flight at the design Mach and
  altitude at the mid-cruise mass on the maximum-cruise rating. Each is met on
  its nominal value; the scatter bands of the thrust model are uncertainty,
  not margin to design into. The planner's cruise-level and top-of-climb rows
  are not sizing rows: the plan lowers the level until they hold, so they hold
  at any engine.
- Scaling laws (`alas_config::EngineConfig::scaled_by`): thrust, fuel flow and
  cruise reference thrust scale with the rating at constant TSFC (Jenkinson,
  *Civil Jet Aircraft Design*, Sec. 9.10); nacelle length with `x^0.4` and
  diameter (nacelle radius, fan) with `x^0.5` (Raymer, *Aircraft Design: A
  Conceptual Approach*, 6th ed., Eqs. 10.1-10.2, p. 283); the engine mass,
  reversers, controls, starters, pylon and nacelle through the FLOPS equations
  already in the mass model (engine mass `(T/T0)^1.15`, NASA TM-2017-219627
  Eq. 75). Turboprops scale shaft powers and fuel flow with the rating, the
  propeller diameter with `P^0.25` (Raymer Eq. 10.23) at constant tip speed.
- Closure: the thrust moves the engine, nacelle and pylon mass and the fuel
  flow the requirements are written on, so the evaluation closes the airframe,
  finds the requirement on it by bisection over the real residual table, and
  repeats with the engine at that rating until the installed rating exceeds the
  requirement by no more than 1 percent (typically two or three closures; the
  coupling contracts by about a factor of ten per closure).
- Hard rows: `engine_thrust_match` (installed against required rating, both
  directions, 1 percent) and `engine_scale_validity` (the sized rating within
  `engine_rating_window` of the catalogue rating, default 20 percent, the rule
  of thumb of Jenkinson Sec. 9.10, at most 0.6 (the end of the sizing search);
  a brief that needs an engine outside it is infeasible rather than
  extrapolated: select a nearer catalogue engine or widen the window).
- Switched off (`engine_sized_by_requirements` false) the engine keeps its
  catalogue rating, the row `engine_thrust_margin` (Diagnostic) reports the
  requirement and `engine_oversize` (a Diagnostic, warning only) reports a held engine whose requirement is more than the window
  below its rating (rating above `required / (1 - window)`: 25 percent above
  the requirement at the default 20 percent window). The design stays
  feasible; the pipeline emits the Warning finding `engine_oversize` with the
  numbers (report text and GUI), and the acceptance plausibility rule
  `ENGINE-OVERSIZE` reads it and flags it at MEDIUM severity. Too weak an
  engine stays hard (the thrust rows themselves). Reference adaptation and the
  baseline sandbox always keep the declared engine and are unchanged. An
  evaluation on an external aerodynamic polar keeps the engine too (the polar
  belongs to the nacelle it was computed for) and carries the same two
  Diagnostic rows in the optimizer's table; the pipeline finding and the
  plausibility rule judge the native assessment of the delivered design.

### Design take-off field length

`toda_override_m` (top-level, advanced, 600-5000 m, unset by default) states a
design take-off distance available for the case. The effective requirement is
the shorter of it and the departure aerodrome's take-off distance available
(`AlasConfig::departure_airport_record`, `settings_takeoff.rs`): it can only
make the requirement more demanding. A longer value is ignored, never used to
lengthen the runway, and says so (Diagnostic row `takeoff_field_override_ignored`
in the optimizer table, warning finding `takeoff_override_ignored` in the
feasibility findings). The same effective record is read by the take-off
constraint, the engine sizing, the field-performance findings, the engine-out
and VMC evidence, the matching chart and the departure LTO figures.
