# Aerodynamic exposed-area correction

The hybrid parasite buildup uses gross projected wing area for coefficient
normalization and exposed wing area for skin friction. With
`drag_model.exclude_buried_main_wing_area = true` (the product default), the
main-wing center section inside the body is subtracted before applying the
configured wing wetted-area factor. Setting it to `false` retains the gross
wing-area convention; `AeroAnalysis::new_reference_compatibility` also retains
the frozen convention independently of this flag.

The basis is the exposed-area relation in [NASA NDARC Theory, v1.6, wing drag
model](https://rotorcraft.arc.nasa.gov/Publications/files/NDARCTheory_v1_6_938.pdf),
`S_wet = 2(S - c*w_fus)`. ALAS integrates the piecewise-linear chord over the
buried span, interpolates the local fuselage cross-section at the root quarter
chord, and evaluates its superellipse width at wing-root height. Thus a detached
high wing is not shielded merely because it projects onto the body. The
aircraft reference area is unchanged.

This is a local extruded-body approximation, not a mesh intersection: it does
not resolve fairings, thickness, changing body contour over the chord, twist
in the buried-area metric, or dihedral crossing the body surface. It is limited
to symmetric main wings and centered fuselages. Tail wetted-area conventions
are unchanged. OpenVSP's [Comp Geom](https://www.nasa.gov/reference/openvsp-comp-geom/)
offers an intersected-surface calculation for higher-fidelity checks.

Reproduce the numerical change with
`cargo run -p alas-aero --example parasite_area_correction`. Analytic tests cover
a rectangular center section, a tapered center section, vertical and horizontal
detachment, and an oversized body; the analysis test verifies flag selection,
legacy replay, and unchanged induced and wave terms. These establish numerical
verification, not aircraft drag calibration or physical validation.

For the current preset geometry, the isolated flag comparison at each preset's
cruise Mach and altitude gives the following dimensionless parasite coefficients
(these are not fitted total-polar intercepts):

| Preset | Gross convention | Exposed convention | Change |
| --- | ---: | ---: | ---: |
| AVE | 0.01764529 | 0.01666136 | -5.576% |
| A340-300 | 0.01851220 | 0.01750726 | -5.429% |
| A380-800 | 0.01468718 | 0.01368473 | -6.825% |
| B787-9 | 0.01899801 | 0.01780423 | -6.284% |
| A320-200 | 0.02196036 | 0.02052914 | -6.517% |
| A220-300 | 0.02281666 | 0.02157961 | -5.422% |
| ATR72-600 | 0.02460001 | 0.02460001 | 0.000% |
| DC-10 | 0.01844081 | 0.01715870 | -6.953% |

The ATR main-wing root is above the modeled fuselage; its projected overlap is
therefore not subtracted. These modest changes do not remove the reported
25-63% differences against published drag estimates. Further agreement requires
equivalent observables and flight conditions, geometry checks, and independent
calibration evidence; no multiplier is fitted to those reported differences.

## Interpreting drag and lift comparisons

Sun, Hoekstra and Ellerbroek's [2020 drag-polar
paper](https://pure.tudelft.nl/ws/portalfiles/portal/71038050/published_OpenAP_drag_polar.pdf)
estimates polar coefficients from flight surveillance with a stochastic total
energy model. Its coefficients are model-based estimates, not directly
measured component drag. A fitted total-polar intercept is not necessarily the
parasite buildup alone, and effective Oswald efficiency from a total polar is
not the same observable as pointwise inviscid span efficiency from a solver.

`PolarSweep` exposes both the geometric solve angle and the Prandtl-Glauert
relabelled reporting angle. A slope on the latter axis is an analytical
compressibility approximation, not a compressible VLM solution. Low-speed
wing-alone estimates must not be scored against a whole-aircraft cruise slope.

There is no universal swept-wing efficiency ceiling below 0.968. The planar
elliptic-loading bound is conditional; nonplanar lifting systems can exceed
unity for a fixed projected reference span. See [NASA's nonplanar lifting-line
discussion](https://ntrs.nasa.gov/api/citations/19920016018/downloads/19920016018.pdf).

# Mass statement, tanks, fuel policy and dispatch closure

The item ledger (`alas-mass::ledger`) is the representation every mass
property is computed from. Each item carries a mass, a role (fixed,
operating item, unusable fuel, payload, usable fuel), a reference point in
the geometry frame (x aft, y starboard, z up) and a centroidal inertia
tensor; the aircraft tensor about any state's centre of gravity follows from
the parallel-axis theorem, with products of inertia stored as the positive
integrals and negated on the matrix off-diagonal (JSBSim structural frame
convention). Component tensors are closed-form solids: thin plates for
lifting surfaces, a thin-walled cylinder for the fuselage, solid cylinders
for engines, prisms for tanks and payload items. Component stations follow
Raymer (*Aircraft Design*, 6th ed., ch. 15-16): the integrated wingbox
centroid, tails at 42 percent of their mean chord, fuselage at 45-50 percent
of its length by engine placement, gear at the configured nose and main
stations, engines at nacelle mid-length. The radius-of-gyration cross-check
uses Raymer's definition against half-span, half-length and half their mean
(`I_xx = m (R_x b/2)^2`), reproducing the measured B747-100 tensor of NASA
CR-2144 to one percent; the frozen `alas-stab` estimate keeps its own
full-length convention for parity.

Tanks (`alas-mass::tanks`) are spar-box volumes integrated on the built
wing between the configured front and rear spars over declared semispan
intervals, a carry-through centre tank between the body sides, a stabiliser
trim tank and a declared-volume auxiliary tank. Manufacturer-published
per-tank volumes are authoritative where a preset declares them; the
geometric estimate supplies the centroid and a calibration factor that keeps
a redesigned wing's capacity consistent: on a design other than the preset's
own, each published cell becomes a per-cell factor between its published
volume and the geometric estimate on the preset's geometry, applied to the
candidate's own spar box (`FuelTankLayout::resolve_scaled`), so the optimizer
sees fuel volume grow and shrink with the wing. Fuel is loaded in the reverse of the
burn order and burned centre first, outer wing last; unusable fuel (CS
25.959) is part of the empty mass and expansion space (CS 25.969, at least
two percent) is excluded from the usable volume.

The fuel policy (`alas-config::fuel_policy`, `alas-mass::fuel_policy`)
decomposes the load into taxi, trip, contingency, destination alternate,
final reserve, additional and extra fuel. Under the EASA basic scheme
(CAT.OP.MPA.181(c) and AMC1) contingency is the larger of five percent of the
trip fuel and five minutes holding at 1,500 ft above the destination at the
estimated landing mass, the final reserve is thirty minutes holding at 1,500
ft at the estimated mass on arrival at the alternate, and a missing
destination alternate is replaced by a fifteen-minute hold. Under 14 CFR
121.639 the reserve is forty-five minutes at normal cruise consumption after
the alternate with no contingency; under 121.645(b) it is ten percent of the
flight time to the destination priced at cruise consumption plus a thirty-
minute hold, and two hours of cruise replace a missing alternate
(121.645(c)). Holding fuel flow is the cruise TSFC applied at the minimum-
drag lift coefficient of the parabolic polar, which for a high-bypass
narrowbody lands within about ten percent of the cruise flow at the same
mass; this is an analytic estimate, not an engine-deck value.

The dispatch closure (`alas-mass::dispatch`, `alas-pipeline::mission_stage::
dispatch`) solves the fixed point `takeoff mass = zero-fuel mass + required
takeoff fuel(takeoff mass)`, first with the analytic Breguet model built from
the report's drag polar and engine binding, then by re-flying the native
segment mission at the estimate and re-pricing the policy on the flown trip
until the mass changes by less than the larger of five kilograms and one
part in ten thousand of the takeoff mass (the fixed point contracts by about
a quarter per flight, so the residual error is under a third of the last
change). The mass is bounded by
the takeoff-mass limit and by the usable tanks less the taxi fuel; the
shortfall beyond either bound is a reported finding, and the mission is
flown at the admissible mass.

Every product search minimises a mission quantity; the frozen weighted
lift-to-drag objective of the Python reference is replayed only by the
parity fixtures and is not selectable. The mission-sized objective
(`optimizer.objective`) ranks candidates feasibility first: a candidate that violates a hard requirement family
costs more than any feasible one and infeasible candidates order by their
normalised violation; soft families rank behind feasibility and ahead of the
objective; diagnostic families are reported only. Tail-volume windows are
plausibility bands (tier S of the research note) and rank soft even under a
hard geometry family, and a violation below ten parts per million of its
limit is numerical noise (the builder's own rounding of the reference area)
and is not counted. The objective is a
mission quantity closed by an inner takeoff-mass fixed point, which the
2026-09-05 MDO research note argues is the right architecture for a
derivative-free search (an equality closure constraint leaves a
population-based method a measure-zero feasible set).

# FLOPS transport mass method

The NASA Flight Optimization System weight equations (Wells, Horvath and
McCullers, *The Flight Optimization System Weights Estimation Method*,
NASA/TM-2017-219627 Vol. I, 2017) are implemented as three separately
selectable groups in `alas-mass::flops_transport`, each replacing the
corresponding frozen group of the Torenbeek/fraction buildup when selected
in `mass_model`:

- `systems_mass_method = flops_transport_v1`: surface controls, APU,
  instruments, hydraulics, electrical, avionics, furnishings, air
  conditioning and anti-icing (equations 97-115) and the operating items
  (crew and baggage, unusable fuel, engine oil, passenger service, cargo
  containers; equations 119-126).
- `structural_mass_method = flops_transport_v1`: the wing (equations 10-17
  and 33-45, with the simplified bending factor of equation 10 or the
  detailed load-path integration of equations 18-32 and the engine
  inertia-relief factor of equations 27-30 and 39-41), horizontal tail (46),
  vertical tail (50), fuselage (56-57), main and nose landing gear (63-67),
  paint (68) and nacelles (69, 74).
- `propulsion_mass_method = flops_transport_v1`: the scaled engine
  (75-76, 80), the distributed-propulsion scaling of counts, thrust and
  nacelle diameter beyond four engines (81-85), thrust reversers (86),
  engine controls and starters (87, 89, 91) and the fuel system (92).

Every equation is evaluated in its published US customary units with the
exact conversions in `alas-units` and returned in kilograms. Geometry
inputs are read from the built airplane (reference area and span projected
on the aircraft plane, tip-to-root taper, quarter-chord sweep, the
area-weighted lofted thickness ratio, tail areas and taper, fuselage length
and maximum width and depth, nacelle diameter and length, engine stations);
architecture inputs that a geometry cannot express (engine mounting,
maximum Mach, maximum fuel capacity, crew and cabin classes, hydraulic
pressure) are the declared `mass_model.flops_transport` fields, and the
technology factors (`FCOMP`, `FAERT`, `FSTRT`, `PCTL`, `CARGF`, `WPAINT`,
`WENGB`, `THRSO`, `EEXP`, `WPMISC`, `WMARG`) are `mass_model.flops_structure`
with the FLOPS defaults. A missing datum makes the selected method
unverified and the buildup fails with the list of blockers; no fraction is
substituted.

Where FLOPS estimates an input the user leaves blank, the same estimate is
used and named in the evaluation record: the design landing mass defaults
to the mass model's landing-mass fraction of the design gross mass rather
than FLOPS' range-based equation 65, and the gear oleo lengths follow
equations 66-67. The fin, canard and hybrid-wing-body branches, alternate
engines and energy storage, and the general-aviation and fighter equations
are not transport equations and are not translated. Nacelles belong to the
structural group in the FLOPS statement but are carried in the propulsion
group of the ten-group breakdown so the mass stations place them on the
engines.

Verification uses the two FLOPS-run validation cases NASA's Aviary
distributes (`LargeSingleAisle1FLOPS`, detailed wing, and
`LargeSingleAisle2FLOPS`, simple wing; inputs and FLOPS outputs recorded in
an internal validation-data note, test
`crates/alas-mass/tests/flops_validation_cases.rs`). Every structural,
propulsion, systems and operating-item output is reproduced within the
data file's quoted precision (one part in a thousand; the simple bending
factor 8.8294 and the detailed factor 11.5918 to four figures, the pod
inertia-relief factor 0.967333 to 1e-4). Two conventions the memorandum
typesets ambiguously were settled by that cross-check: the sweep-bucket
bracket of equation 31 divides the unadjusted factor, and the
distance-weighted sweep of equation 18 is a plain weighted sum that FLOPS
does not renormalise when the stations stop short of the tip. This is
implementation verification against the published equations as FLOPS
evaluates them, not physical validation against weighed aircraft.

# Multidisciplinary sizing loop and gradient-based driver

`alas-opt::mdo::mda` closes each candidate as a converged multidisciplinary
analysis rather than a single pass: mass and centre of gravity at the
current takeoff mass, trim and drag polar at that mass and centre of
gravity, mission fuel at that polar, and the takeoff mass those imply,
repeated until the takeoff mass changes by less than
`optimizer.objective.sizing_tolerance_kg`. The fixed point is accelerated
with Aitken's delta-squared extrapolation every third pass; the
vortex-lattice trim is re-run only when the centre of gravity has moved by
more than `retrim_cg_tolerance_pct_mac` of the mean aerodynamic chord since
the last trim, so a converged candidate is trimmed at its own weight and
its cruise lift coefficient follows the sized mass (a zero tolerance keeps
the single trim at the takeoff-mass ceiling). The number of passes and
re-trims and the residual centre-of-gravity inconsistency are reported with
the sized candidate.

`optimizer.solver.method = sqp` runs a gradient-based driver over that loop
in the multidisciplinary-feasible architecture: only the design variables
are unknowns and every evaluation is a converged aircraft. The driver is
line-search sequential quadratic programming with an l1 merit function and
a damped BFGS Hessian in variables normalised to the unit box (Nocedal and
Wright, *Numerical Optimization*, Algorithm 18.3, Procedure 18.2). Each
major iteration linearises the objective and every hard requirement
residual by forward differences evaluated as one parallel batch
(`finite_difference_step` of the bound range per variable), solves the
elastic quadratic subproblem (slack variables with an l1 penalty keep the
linearised constraints consistent, as in SNOPT's elastic mode) with a
dense Mehrotra predictor-corrector interior-point method, and backtracks
on the merit function. Soft residuals enter the objective through the
configured penalty weight; hard residuals are the constraints, signed so
that zero is the limit. The run stops when the step falls below 1e-4 of
the box with every constraint within `constraint_tolerance`, when the
feasible objective has stopped changing to `tolerance` over two
iterations, or on the iteration budget or a failed line search, and the
termination reason is reported in place of the population strategy. The
best feasible point seen, or the least-infeasible one when none was
feasible, is returned. Under a delegated evaluator the driver has no
constraint vector and reduces to a bound-constrained search.
