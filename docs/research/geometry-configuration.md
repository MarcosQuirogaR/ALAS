# Geometry and configuration synthesis for requirements-first transport concepts

**Status:** additive research memo for ALAS
**Date:** 2026-08-26
**Scope:** conceptual transport-aircraft configuration selection, parametric
geometry, cabin and cargo accommodation, balance, fuel volume, high-lift
topology, structural/mesh proxies, and analysis-grade geometry validation.

This memo is a design recommendation, not a certification method. Numerical
ranges are starting bounds for an ALAS design space and must be calibrated
against the aircraft family, current ALAS physics, and golden/parity evidence.
A value useful for screening is not evidence that a design is flightworthy.

## Executive recommendation

Use a two-level mixed-variable search:

1. Select a small, named architecture seed in an outer stage. The seed owns
   topology and genuinely discrete choices: wing position, deck count, tail
   arrangement, propulsion arrangement, landing-gear architecture, high-lift
   concept, tank strategy, and cabin/container architecture.
2. Optimise bounded continuous variables within each surviving seed. The
   continuous vector owns planform, placement, section shape, cabin envelope,
   tail scale, gear geometry, tank boundaries, and structural/mesh controls.

Materialise every seed into one deterministic source geometry. From that source,
produce at least three linked products:

- a watertight master geometry for section, intersection, volume, and CAD-like
  operations;
- a solver representation, such as panels, lifting-line sections, wingbox
  regions, or external-tool geometry, with explicit fidelity and provenance;
- a display or preview representation.

The display mesh is not an analysis proof. A low-resolution triangulation or an
OpenVSP DegenGeom-like abstraction can be excellent for preview and rapid
screening while being insufficient for panel quality, viscous aerodynamics,
structural stress, or manufacturing evidence. OpenVSP's work on degenerate
geometry forms makes this separation explicit: surface, plate, stick, and
point forms carry aggregate quantities for different disciplines but remain
derived from linked master geometry [R3].

For the current ALAS design:

- keep the canonical requirements object and physical configuration separate;
- keep the existing GeometryConfig scaffold and DesignVector ownership
  boundary;
- add this schema as a synthesis and traceability layer around them;
- reject geometry with typed residuals before expensive aero, mission, or
  structures stages;
- make cabin, landing gear, fuel volume, and high-lift topology early
  feasibility consumers rather than late report decorations.

## Evidence summary

| Topic | Evidence | Consequence for ALAS |
| --- | --- | --- |
| Parametric conceptual geometry | OpenVSP was developed to balance productivity and fidelity, with watertight 3-D geometry suitable for downstream analysis, metamodels, wind-tunnel work, and display [R1]. | Use a compact, editable master geometry with explicit joins and a deterministic build. |
| Linked geometry and multiple fidelities | OpenVSP degenerate geometry forms retain geometry and aggregate properties for surface, plate, stick, and point abstractions [R3]. TiGL creates 3-D aircraft geometry from CPACS and exports CAD, visualization, and analysis-oriented representations [R6]. | Treat the master geometry as source of truth; tag every derived solver or preview mesh with source hash and fidelity. |
| Interoperability and coordinate discipline | CPACS is a hierarchical aircraft data schema with parent transformations and an x-aft, y-right, z-up coordinate convention in its documentation [W1]. CPACS has been used as a central exchange for geometry, masses, performance requirements, aero polars, structures, engines, and process data [R5]. | Make frame, units, parent UID, and datum transformations first-class fields. |
| Architecture synthesis | The DLR collaborative configuration study varies aircraft and subsystem architecture, including wing area/aspect ratio, landing-gear type, and tank/fuel-system choices, through CPACS-based system-driven optimisation [R5]. | Enumerate topology before continuous optimisation; retain seed provenance and rejection reasons. |
| Wing planform controls | OpenAeroStruct exposes sweep, taper, dihedral, twist, chord, x-shear, z-shear, thickness, and radius distributions; B-spline control points decouple design-variable count from analysis mesh density [W3, R14]. | Use piecewise planform stations plus low-order spline controls for smooth distributions, not one variable per mesh point. |
| Wingbox and internal fuel | OpenAeroStruct exposes a wingbox fuel-volume residual and requires non-negative usable volume after accounting for required fuel mass [W4, R14, R15]. | Include spar/skin boundaries, fuel exclusions, density, reserve, and usable-volume efficiency before selecting a wing. |
| Cabin-aware geometry | The CPACS cabin extension supports passenger, VIP, cargo, and other decks, reusable components, meshes/CAD references, luggage, panels, exits, monuments, and detailed layouts [R7]. | Seat count, cargo/container positions, floor/deck elevations, monuments, doors, and clearances belong in the geometry contract. |
| Tail and control sizing | A NASA conceptual module sized horizontal control surfaces first for rotation and then for stability over a mission, including downwash, upwash, ground proximity, and centre-of-gravity effects [R10]. | Tail area and arm must be coupled to CG, gear, rotation, trim, and stability; tail volume is only an initial scale law. |
| Landing gear integration | Chai and Mason describe geometry, kinematics, flotation, weight, structure, runway, and economic integration for conceptual transport gear; they identify gear as an early mass and layout driver [R9]. | Choose gear architecture early and check retraction, ground loads, clearance, bay interference, CG load share, and pavement proxies. |
| High-lift topology | Rudolph compares mechanism and layout families, including single-slot Fowler concepts, slats, gap progression, fairing size/count, complexity, reliability, maintainability, and weight [R11]. | High-lift is a discrete configuration choice with geometry and mechanism checks before detailed field performance. |
| Airfoil parameterisation | Kulfan's CST represents smooth sections with a class function and Bernstein shape functions, extending to wings, bodies, ducts, and nacelles with relatively few variables [R17, R18]. | Make CST the default optimizer-native section family; bound physical measures, not just raw coefficients. |
| Aerodynamic-performance parameterisation | The open PAERO study combines thin-airfoil/Fourier camber controls with CST/B-spline thickness and uses a learned filter to screen unreasonable shapes [R20]. | Offer a low-dimensional Fourier/PAERO mode for controlled camber, but retain curvature, thickness, and leading/trailing-edge gates. |
| Low-fidelity aero and structures | OpenAeroStruct couples a vortex-lattice model with a simplified beam/wingbox model and adjoint derivatives; AVL provides extended vortex-lattice lifting surfaces, slender bodies, controls, trim, stability, and mass properties [R14–R16]. | Use VLM/AVL/OAS in the funnel, not as a substitute for stall, viscous, junction, manufacturing, or certification evidence. |
| Analysis mesh robustness | NASA work on automatic conceptual-design FE meshes emphasises parametric structural layouts, connected meshes, element quality, and avoiding poor aspect-ratio triangles; a related report targets robust meshing during large geometry changes [R12, R13]. | Mesh quality and topology are explicit residuals. A render mesh must never be reported as an FE/CFD-quality mesh. |

## Mixed discrete/continuous architecture

### Architecture seed

Use a typed record with stable names rather than integer flags hidden inside a
floating-point vector. The following is conceptual and can be adapted to Rust:

~~~
ArchitectureSeed
  id: stable name
  parent_brief_id: requirements provenance
  family:
    role: passenger | freighter | combi | special
    layout: tube_and_wing | blended | strut | box | other_supported_family
    wing_position: low | mid | high
    fuselage_architecture: single_deck | twin_deck | cylindrical | noncircular
    tail: conventional | t_tail | h_tail | v_tail | canard | tailless
    propulsion: turbofan | turboprop | electric | hybrid | other_supported
    engine_count: supported integer catalogue
    gear: tricycle | bicycle | taildragger | quadricycle | other_supported
    high_lift: none | plain_te | slotted_te | fowler | slat_fowler | multi_element
    tank: wingbox | centre_body | fuselage | distributed
    cabin: seat_rows | LD3_45 | bulk_cargo | mixed_container
  continuous:
    wing, fuselage, cabin, tail, gear, tank, section, high_lift, structure, mesh
  derived:
    geometry_metrics, accommodation_summary, mass_cg_metrics, fuel_metrics,
    gear_metrics, mesh_metrics
  bounds_profile: conservative | balanced | exploratory
  fidelity_policy: screening | native_vlm | external_finalist
  provenance:
    source_geometry_hash
    parent_seed_id
    generator_version
    source_references
  rejection_residuals: typed list
~~~

The important properties are:

- topology is visible and validated against a supported catalogue;
- continuous variables have physical units and named bounds;
- derived values are not editable;
- every rejection has a machine-readable residual;
- every generated geometry can be traced to a requirements brief and a source
  geometry hash.

### Seed-generation procedure

1. Convert the DesignBrief into a small morphological matrix. Remove
   combinations that are impossible for the role before geometry generation.
2. Generate named seeds from supported combinations. Start with a sparse
   catalogue of conventional tube-and-wing variants and add blended, strut, or
   unusual layouts only when the downstream geometry and physics stages can
   evaluate them.
3. Apply cheap checks: units, station order, approximate payload volume, wing
   loading, span, fuel volume, tail arm, gear envelope, and propulsion
   clearance.
4. Sample continuous variables inside a profile-specific box. Latin-hypercube,
   Sobol, or differential-evolution sampling is appropriate after a seed
   exists; it should not be asked to invent a topology from a continuous
   integer surrogate.
5. Keep each seed's source, bounds, random/DOE index, generated geometry hash,
   and rejection residuals. A family-level diagnosis must distinguish an
   impossible brief, a wrong architecture, and a bad continuous region.
6. Promote only feasible or near-feasible seeds into geometry, aero, mission,
   and structures stages. Keep the requirements-first order: valid geometry,
   hard feasibility, soft targets, then objective.

## Recommended parameter schema

This synthesis contract is intentionally more explicit than the wizard-facing
vector. It can be materialised into the existing ALAS physical configuration
and design vector without making the wizard maintain a second copy of aircraft
dimensions.

### Frame, units, and provenance

| Group | Recommended fields | Required invariants |
| --- | --- | --- |
| Units | length_unit, mass_unit, angle_unit, force_unit; canonical internal SI | Every incoming value is converted once; no mixed-degree/radian or inch/metre geometry. |
| Aircraft frame | origin, x/y/z directions, reference point, reference area/chord/span | Coordinate convention is written into every export. |
| Parent transforms | parent_uid, local translation, local rotation, scale if supported | A component is evaluated in its parent frame and transformed exactly once. |
| Provenance | brief ID, seed ID, generator version, vector, bounds profile, source hash, fidelity | A solver result is traceable to exact geometry and requirements. |
| Tolerances | merge, intersection, closure, angle, curvature, mesh tolerances | Tolerances are named and reported, not hidden in a renderer. |

Use the CPACS convention as the interoperability default: x positive toward
the aft direction, y positive to starboard/right, z positive upward, with
component-local frames connected by parent transforms [W1]. If ALAS keeps a
different internal convention, the adapter must state and test the transform.

### Discrete architecture fields

Validate these fields against an implemented catalogue:

- role and payload type: passenger, freighter, combi, special;
- primary layout: conventional tube-and-wing, blended, strut-braced, box-wing,
  or another implemented family;
- wing position and dihedral family;
- deck count and cabin/container architecture;
- tail topology and control-surface family;
- propulsion family, engine count, nacelle/pylon arrangement, and installation
  zone;
- landing-gear architecture and number of wheels/legs;
- tank strategy and fuel-system topology;
- high-lift mechanism and spanwise segmentation;
- airfoil representation family: known section, CST, or bounded PAERO/Fourier.

Do not encode an unsupported family merely because an enum is easy to add. A
family is supported only when the geometry builder, accommodation resolver,
relevant physics, and rejection reporting can evaluate it.

### Wing planform and placement

Represent each lifting surface as a sequence of full-span or semispan
stations. The representation must state which convention it uses; a field
named span without that convention is a recurring factor-of-two error.

Station fields:

- span fraction and absolute spanwise coordinate;
- leading-edge x and z;
- local chord;
- local sweep convention (leading edge, quarter chord, or reference chord);
- local dihedral;
- twist/incidence;
- thickness ratio and airfoil ID;
- optional side-of-body/root station, kink station, and tip closure;
- structural boundaries such as front/rear spar fractions.

High-leverage continuous variables:

- reference area or wing-loading-derived area;
- projected span and aspect ratio;
- root, side-of-body, kink, and tip chords;
- taper ratios and piecewise leading-edge sweep;
- kink span fraction;
- wing x/z position;
- dihedral and twist control points;
- section thickness/camber controls;
- optional x-shear/z-shear controls for non-planar or strut-supported families.

Use a small number of geometric cranks or B-spline control points. Sample the
surface at higher mesh resolution without increasing design dimension.
OpenAeroStruct uses this separation for chord, twist, sweep, taper, dihedral,
shear, and thickness distributions [W3].

Derived quantities must include:

- total and projected area, span, aspect ratio, mean aerodynamic chord, and
  spanwise centroid;
- root/kink/tip station locations and chord continuity;
- quarter-chord sweep and aerodynamic-centre estimate;
- wetted/planform area and exposed area after fuselage/nacelle intersection;
- structural box area and usable internal volume;
- minimum local thickness and control-surface room.

### Fuselage and cabin-aware sizing

Separate the external fuselage loft from the internal accommodation envelope.
The internal envelope is derived from external sections after shell, insulation,
structure, floor, ceiling, and systems keep-outs. A plausible outer tube
without a capacity calculation is not a cabin model.

Outer fuselage fields:

- longitudinal stations with x, width, height, centre z, and section family;
- nose, constant-cabin, tail-cone, and pressure-shell break stations;
- external cross-section parameters, such as ellipse or superellipse exponent;
- floor and pressure-shell reference levels;
- doors, emergency exits, cargo doors, radome, fairings, and tail junctions;
- local frame or bulkhead stations.

Cabin and hold fields:

- deck ID, floor z, clear height, usable width, aisle count, and aisle width;
- seat abreast, seat pitch, seat/row integer allocation, and seat orientation;
- galley, lavatory, closet, crew-rest, monument, and exit envelopes;
- passenger, freight, or mixed zones;
- cargo deck/hold volume, container type, orientation, and position list;
- requested, placed, and maximum-capacity load cases;
- unfilled cabin/hold capacity and owner of capacity shortfall.

Cabin capacity must be resolved in the geometry that feeds preview and
reports. The CPACS cabin work demonstrates why the schema must carry decks,
LOPA/layout objects, components, luggage, exits, and 3-D geometry/CAD
references rather than only passenger count [R7]. The ALAS requirements
already call for main/upper-deck visibility, cargo, and LD3-45 accommodation;
the synthesis layer should make deck and hold allocation explicit.

A useful conceptual relation is:

    L_cabin = L_seat_rows + L_monuments + L_crew + L_doors_and_clearance

where seat rows and seats-abreast are discrete, and each term comes from the
actual layout. Do not infer cabin length from a fineness ratio after entering
the passenger requirement.

### Empennage, tail placement, and control surfaces

Tail placement references current mass/CG and the wing aerodynamic reference,
not only fuselage length. Recommended fields:

- tail topology and number of lifting/control surfaces;
- horizontal and vertical tail area, aspect ratio, taper, sweep, dihedral,
  incidence, and thickness/airfoil;
- tail x and z datum, moment arm to the current CG, and hinge/control-surface
  fractions;
- elevator/rudder/aileron span and chord fractions, deflection limits, and
  clearance;
- downwash/upwash and ground-effect assumptions for the selected fidelity;
- tail structural attachment and fairing/empennage junction constraints.

Use tail-volume coefficients as initial scaling variables:

    V_H = S_H l_H / (S c_bar)
    V_V = S_V l_V / (S b)

Here S is wing area, c_bar mean aerodynamic chord, b wing span, and l_H/l_V
the appropriate moment arms. These coefficients are not acceptance criteria.
The detailed gate evaluates trim, static margin, rotation, control authority,
elevator/rudder effectiveness, and the complete CG envelope. The NASA
control-surface sizing report explicitly couples rotation and mission
stability rather than sizing the tail from volume alone [R10].

### Landing gear and ground integration

Gear is a discrete/continuous subsystem, not a final drawing overlay.

Discrete fields:

- tricycle, bicycle, taildragger, quadricycle, or supported special layout;
- number and location of main/nose legs and wheels;
- wing, fuselage, nacelle, or sponson installation;
- steering/braking and retraction-direction family.

Continuous fields:

- main and nose contact points;
- wheel radius, width, axle spacing, and track;
- strut attachment points, lengths, rake, and rotation axes;
- bay boundaries, doors, fairings, and local structure;
- ground plane, runway slope, tyre/flotation parameters, and CG range;
- extension/retraction sweep and minimum-clearance margins.

Minimum screening checks:

1. all wheels contact the ground plane in the landing attitude;
2. gear has positive static load share for every required CG case;
3. tipback, nose-over, turnover, tail-strike, propulsor, and wing-tip
   clearances pass the selected screening model;
4. retracted gear and doors fit the declared bay without intersecting pressure
   shell, wingbox, tank, engine, or payload volume;
5. runway/pavement and flotation proxies are reported where required;
6. gear weight is fed into mass and CG, not added after geometry.

Chai and Mason report landing gear as an early integrated discipline and give a
historical conceptual range of roughly 3–6 percent of MTOW for landing gear.
That range is an empirical historical cue, not an ALAS universal bound; use
the active mass model and calibration evidence for final sizing [R9].

### Fuel tanks, volume, and internal packaging

Expose both tank topology and a volume integral:

- tank strategy and tank IDs;
- wingbox front/rear spar fractions by span station;
- root/centre-body/fuselage/nacelle tank boundaries;
- systems, landing-gear, high-lift, and structural exclusions;
- fuel density, reserve, trapped/unusable fraction, and usable-volume
  efficiency;
- fill sequence and centre-of-gravity limits;
- tank vent/inspection access proxies where supported.

For fuel mass m_f and density rho_f:

    V_required = m_f / rho_f
    V_usable = eta_tank integral(A_box(y) dy) - V_exclusions
    residual_fuel_volume = V_usable - V_required

Require residual_fuel_volume >= 0 at the selected load case, with named reserve
and explicit efficiency. OpenAeroStruct's wingbox fuel-volume constraint is a
useful precedent: volume is a first-class residual rather than an annotation
[W4, R14, R15].

Fuel placement changes CG and trim. A volume pass that does not send fuel mass
and fill sequence into the mass-balance stage is incomplete.

### Airfoil and section parameterisation

#### CST as the default family

For chord-normalised coordinate xi in [0, 1], the CST form can be written:

    y(xi) = C(xi) S(xi) + xi * Delta_y_TE
    C(xi) = xi^N1 (1 - xi)^N2
    S(xi) = sum(A_i B_i^n(xi))

Use separate upper/lower coefficient vectors or a camber/thickness
decomposition, with explicit trailing-edge thickness. CST is attractive for
ALAS because it is smooth, compact, differentiable, and extensible to 3-D
surfaces [R17, R18].

Do not bound only coefficients. Also check:

- maximum thickness and chordwise position;
- leading-edge-radius proxy;
- maximum camber and position;
- upper/lower curvature and inflection count;
- trailing-edge thickness and wedge angle;
- monotone chordwise traversal and closure;
- local thickness after flap/slat/structural cuts.

#### Fourier/PAERO as an optional family

Fourier or PAERO-style camber modes are useful when design variables should
have an aerodynamic-performance interpretation. The open PAERO study combines
thin-airfoil/Fourier camber with CST/B-spline thickness and demonstrates a
learned filter for unreasonable shapes [R20]. Use this family only with
geometry gates. Unconstrained high-order Fourier coefficients can create
oscillatory curvature, leading-edge artefacts, or unusable trailing edges.

#### Known sections

Allow a discrete, named airfoil library for validated baseline sections. A
known section must still be checked at every loft station for coordinate
orientation, thickness, scaling, closure, and compatibility with planform and
high-lift layout. A library identifier is provenance, not a geometry waiver.

### High-lift concept

Represent high-lift in two layers:

- discrete mechanism/topology: none, plain trailing edge, slotted, Fowler,
  slat/Fowler, multi-element, or another implemented family;
- continuous controls: spanwise start/end, chord fraction, deflection, gap,
  overlap, hinge axis, fairing envelope, and segmentation.

Rudolph's survey shows why the mechanism is not just a lift-coefficient scalar:
Fowler motion, slot/gap progression, fairing count, complexity, reliability,
maintainability, and weight change with the layout [R11].

At screening fidelity, high-lift geometry provides a declared configuration and
a calibrated CLmax/drag-increment model. At higher fidelity, send it to the
applicable viscous or high-lift analysis. A clean-wing VLM result is not
deployed high-lift performance.

### Structural layout and manufacturability proxies

Optional structural intent should include:

- front/rear spar fractions and allowable spanwise variation;
- rib stations and spacing;
- fuselage frame and bulkhead stations;
- minimum skin/beam thickness and local thickness-to-chord limits;
- gear and engine attachment zones;
- control-surface hinge/load-path zones;
- allowable unsupported lengths and access/assembly zones.

Useful conceptual manufacturability proxies include:

- smoothness and continuity of lofts;
- bounded curvature and no sudden thickness/section jumps;
- minimum radii and minimum material thickness;
- structural access and connected load paths;
- repeated or standardised sections where the architecture calls for them;
- number and size of fairings, mechanism breaks, and non-manufacturable
  intersections;
- mesh element quality after tessellation.

These are screening proxies. They do not replace tooling, joints, fatigue,
damage tolerance, or manufacturing-process analysis.

### Mesh and representation contract

Every candidate should carry a representation record:

~~~
GeometryRepresentation
  source_geometry_hash
  representation_kind: master | solver | preview
  fidelity: algebraic | surface | panel | VLM | FE | CFD
  units_and_frame
  tolerance_set
  element_count
  quality_metrics
  generation_tool_and_version
  validation_status
~~~

The master representation must be sufficient for the operation being claimed:

- a preview can tolerate a coarse triangulation;
- a panel/VLM model needs consistent normals, no duplicate or inverted panels,
  and appropriate leading/trailing-edge discretisation;
- an FE shell mesh needs connected topology, acceptable elements,
  material/section assignments, and structural boundaries;
- a CFD surface mesh needs watertightness, non-overlap, valid normals,
  curvature/refinement controls, and a volume-mesh strategy.

OpenVSP CompGeom, planar slice, mesh, and DegenGeom tooling illustrates this
range of derived representations [R2]. NASA conceptual FE-mesh work adds the
important warning that automatic generation must actively manage element
quality, connectivity, and large geometry changes [R12, R13]. Report preview
mesh separately from these analysis claims.

## Suggested design-variable bounds

### Normalisation

Use a normalised search coordinate u in [0, 1] and an explicit physical map:

    x = x_min + u (x_max - x_min)

For centred or signed controls, use a symmetric map and store both physical
and normalised values. Scale residuals by a named characteristic:

    r_norm = (actual - target) / scale

The bounds profile is versioned and stored with every run. The following are
initial heuristics for subsonic transport-like tube-and-wing seeds, not
universal requirements:

| Quantity | Conservative seed | Balanced seed | Exploratory seed | Notes |
| --- | ---: | ---: | ---: | --- |
| Wing aspect ratio | 7–10 | 8–12 | 6–14 | Derive area/span consistently; impose the brief's span limit. |
| Quarter-chord/LE sweep | 15–25 deg | 20–35 deg | 0–40 deg | Couple to Mach, thickness, aero model, and engine installation. |
| Trapezoid taper ratio | 0.35–0.60 | 0.25–0.55 | 0.15–0.75 | Bound local chord and tip thickness too. |
| Wing twist | -2 to +1 deg | -4 to +2 deg | -6 to +4 deg | Sign convention explicit; check stall/trim proxies. |
| Wing dihedral | 2–5 deg | 2–7 deg | 0–10 deg | High-wing/strut families need different priors. |
| Kink span fraction, semispan | 0.25–0.40 | 0.25–0.50 | 0.15–0.65 | Enforce ordered stations and positive segment area. |
| Root thickness ratio | 0.12–0.16 | 0.10–0.16 | 0.08–0.18 | Mach, structure, tank, and flap packaging control this. |
| Tip thickness ratio | 0.09–0.13 | 0.08–0.13 | 0.06–0.16 | Check minimum absolute thickness and tip closure. |
| Fuselage fineness L/D | 9–12 | 8–14 | 7–16 | Cabin clearances and pressure-shell geometry dominate. |
| Horizontal/vertical tail volume | family-calibrated | family-calibrated | family-calibrated | Use only as seed; trim, stability, and rotation accept. |
| PAERO camber controls | narrow around zero | narrow around zero | source-calibrated | Small example ranges are not transferable universal bounds. |

Start a new aircraft family with the balanced profile, then widen only
variables that have a named physical model and useful residual. A bound is
invalid if it routinely produces geometry that cannot be lofted, accommodated,
balanced, fuelled, or meshed.

### Coupled bound rules

- If wing area is variable, derive span from area and aspect ratio, or
  vice versa; do not independently vary all three without a residual.
- If root and tip chords are variables, derive area from stations. If area is
  requirement-facing, project chords onto requested area.
- Keep wing x-position, cabin length, and tail arm coupled to current CG.
- Keep tank boundaries inside wingbox and out of gear, high-lift, and
  structural exclusion zones.
- Map airfoil coefficients through a physical-feasibility projection or reject
  them with a section residual; never clip coefficients silently.
- Use family-specific bounds for canards, blended bodies, box wings, and
  non-circular cabins instead of stretching tube-and-wing priors.

## Scaling laws and derived quantities

These relations are suitable for deterministic preliminary sizing and
screening, not replacements for detailed discipline models.

### Wing

From wing loading W/S:

    S = W / (W/S)

From aspect ratio:

    b = sqrt(AR S)
    c_bar = S / b

For a full-span trapezoid with root chord c_r and tip chord c_t:

    lambda = c_t / c_r
    S = b c_r (1 + lambda) / 2
    c_t = lambda c_r
    MAC = (2/3) c_r (1 + lambda + lambda^2) / (1 + lambda)

For a piecewise planform, integrate each trapezoidal segment and verify that
the sum equals declared area within tolerance. Leading-edge station positions
can use:

    x_LE(y) = x_LE,r + y tan(Lambda_LE)
    x_TE(y) = x_LE(y) + c(y)

Check spanwise station order, positive chords, positive segment area, and
non-crossing leading/trailing edges.

### Cabin and fuselage

At screening fidelity:

    rows = ceil(passengers / seats_abreast)
    L_seat_rows = rows * seat_pitch

Add actual monument, crew, exit, and clearance lengths from the layout rather
than a single fixed allowance. Usable cabin cross-sectional area is outer
section area minus shell, insulation, structural, floor, ceiling, and systems
keep-outs. The capacity model returns integer seats/containers and clearances,
not just continuous volume.

### Tail

Use tail-volume coefficients to generate an initial area/arm pair, then
recompute arm from the current CG and tail reference point. The physical stages
evaluate static margin, trim moment, control-surface authority, rotation,
tail-strike, engine-out yaw, and high-lift configuration.

### Gear

Solve ground-contact geometry from wheel locations, gear attitudes, and current
CG. Report main/nose load shares for the full CG/load envelope and pass the
result into mass and pavement screening. Do not infer gear position solely from
wing quarter-chord; cabin, tank, bay, rotation, and load-path constraints move
it.

### Fuel and wingbox

Use V_required = m_f/rho_f and integrate wingbox/tank regions. Subtract
gear bays, spars/ribs, systems, unusable volume, and trapped-fuel allowance.
Evaluate the residual for every fuel/load case affecting CG or mission closure.

### Field-length and high-lift screening

The conceptual stall relation is:

    V_stall = sqrt(2 W / (rho S CL_max))

An approach proxy may use V_app = k_app V_stall with a declared factor. Takeoff
and landing distances couple wing loading, thrust/weight, density/temperature,
runway state, and high-lift coefficients. Use these relations only to reject
obviously implausible candidates; ALAS field-performance models remain
authoritative at their selected fidelity.

### Compressibility and mesh

For an initial subsonic heuristic, normal Mach is often related to
M cos(Lambda). Treat this as a bound-setting cue only: thickness, sweep, shock
control, and drag rise require the selected aero model.

Choose a nominal surface size from characteristic length L_c and a declared
resolution count N_c:

    h <= L_c / N_c

Refine around leading/trailing edges, high curvature, control gaps, junctions,
gear/nacelle intersections, and wakes as required by the analysis. Acceptance
is mesh quality plus convergence/sensitivity evidence, not a universal element
size.

## Feasibility gates and residuals

Run cheap, local checks before expensive solvers.

### G0 — Scalar and frame validity

- all values finite;
- positive lengths, areas, masses, densities, and frequencies where required;
- angles and units converted exactly once;
- valid parent transforms and no singular scale;
- no NaN or overflow in derived quantities;
- requirements and architecture choices cross-field consistent.

### G1 — Topology and connectivity

- every required component has a supported builder;
- expected joins exist and attach to intended parent;
- no duplicate UID or orphan component;
- intentional gaps are declared; accidental gaps are residuals;
- discrete configuration is legal for role and fidelity.

### G2 — Planform and loft geometry

- station order and span bounds;
- positive chords, thickness, segment area, and volume;
- no self-intersections or wing/fuselage/nacelle overlap outside declared
  junctions;
- continuous position at joins and appropriate tangent/curvature continuity;
- section orientation and trailing-edge closure;
- bounded camber, thickness, leading-edge radius, and curvature;
- declared area, span, aspect ratio, and derived MAC agree within tolerance.

### G3 — Accommodation and payload envelope

- required decks, floors, clear heights, aisles, exits, monuments, and
  pressure-shell zones exist;
- requested seats/containers are placed as integers and fit clearances;
- cargo/LD3-45 positions are compliant and accessible;
- design and maximum cases return capacity, payload, and unfilled residuals;
- cabin/hold volume does not intersect tanks, gear, frames, or systems.

### G4 — Balance, tail, and gear

- CG is finite and within loading envelope;
- horizontal/vertical tail arms are positive and measured from current CG;
- static-margin/trim/control residuals are available at selected stage;
- gear contact, load share, track, tipback/nose-over/turnover, tail-strike,
  wing-tip, propulsor, and ground clearances pass;
- retraction and bay envelopes do not intersect aircraft or payload.

### G5 — Fuel, tanks, and structural intent

- usable fuel volume covers required fuel and reserve;
- tanks remain inside declared surfaces and exclusions;
- spar/rib/skin layout is connected and has positive thickness;
- gear, engine, tail, and high-lift attachments have declared load paths;
- tank fill sequence and mass/CG effect are available to mass stage.

### G6 — High-lift and controls

- selected mechanism has hinge/gap/overlap/fairing parameters;
- deployed surfaces do not self-intersect or leave parent surface;
- spanwise segmentation and junctions are valid;
- control-surface area and deflection limits are within calibrated model;
- clean-wing result is not labelled deployed high-lift result.

### G7 — Analysis-grade representation

- master surfaces are closed or intentionally open with declared boundaries;
- analysis representation has consistent normals and no zero-area/inverted
  elements;
- no duplicated panels, slivers, or forbidden non-manifold edges;
- aspect ratio, skewness, minimum angle, curvature resolution, and
  intersection metrics meet fidelity policy;
- structural/CFD material, section, and boundary assignments are complete;
- preview mesh is reported separately and cannot satisfy this gate by itself.

### G8 — External or finalist fidelity

- AVL/VLM/OAS model converges and returns requested residuals;
- higher-order aero, mesh, or structures tools receive same source geometry and
  frame;
- solver failure is a typed stage failure, not a successful candidate with a
  missing result;
- finalist results retain source geometry hash, tool/version, settings, mesh
  hash, and requirements snapshot.

Use a residual record with at least:

~~~
ConstraintResidual
  name
  stage
  actual
  target
  direction
  normalized_violation
  scale
  policy: hard | soft | objective | diagnostic
  source_geometry_hash
  fidelity
  explanation
~~~

## Traceability to REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md

The following maps this memo into the existing requirements-first flow. It is a
translation table, not a replacement for the requirements document.

| Requirements-first location | Geometry/configuration action | Output or residual |
| --- | --- | --- |
| Step 1 — Start with intent | Select role, architecture family, wing/tail/gear/propulsion topology, deck/container strategy, and technology assumptions; create ArchitectureSeed. | Seed ID, discrete choices, supported-builder status, topology residuals. |
| Step 2 — Mission and route | Use range, reserve, Mach, altitude, climb, and airport data to choose initial wing loading, area/span bounds, propulsion installation, tank strategy, high-lift seed, and field envelope. | Preliminary sizing values; span, wing-loading, fuel-volume, Mach/thickness, and field-screening residuals. |
| Step 3 — Payload and cabin | Build external fuselage sections and internal decks/holds from passengers, passenger-mass basis, cargo, LD3-45, exits, monuments, and deck requirements. | Requested/placed/capacity/unfilled summary by deck/hold; cabin-intersection and clear-height residuals. |
| Step 4 — Shape and balance | Materialise wing stations, chords, sweep, twist, x/z position, fuselage loft, tail size/arm, section family, high-lift layout, and initial gear/tank envelopes. | Geometry preview plus master-geometry hash; planform, section, tail-volume, gear-envelope, and layout residuals. |
| Step 5 — Feasibility and refinement | Run G0–G7 before expensive analysis; feed survivors into mass, payload, CG, fuel, aero, trim, stability, field, and structures stages. | Typed hard/soft/diagnostic residuals; rejection stage/reason; no silent pass when translator unavailable. |
| Step 6 — Review and launch | Freeze brief, seed, initial geometry, vector, bounds, fidelity policy, and output options. | Immutable run snapshot, source geometry hash, bounds profile, solver settings, provenance. |
| Candidate materialisation | Decode continuous vector into named seed and project/validate coupled variables. | DesignVector-to-geometry audit, scaling, and bounds residuals. |
| Preliminary sizing | Apply wing-loading/AR, cabin length, tail-volume, gear, and fuel-volume relations. | Derived area/span/chord/volume/arm quantities and preliminary feasibility. |
| Geometry and accommodation | Build exterior and resolve interior from the same source geometry. | Accommodation summary and geometry-validity result. |
| Physical analysis | Pass source/fidelity-specific representation to native aero, VLM/AVL/OAS, mission, field, and structural stages. | Stage output with geometry/tool/frame provenance. |
| Hard feasibility and ranking | Rank valid geometry and zero hard residuals before soft targets and objective score. | Feasibility-first candidate record; near-feasible candidates retain grouped residuals. |
| Fidelity strategy | Use algebraic/geometry gates, payload/mass/CG, native VLM/stability, mission/field, wingbox, then optional external finalist tools. | Fidelity level, model/mesh hashes, and explicit not-yet-evaluated status where translator is absent. |

### Requirement-to-geometry translators

| Requirement group | Geometry-side translator | Boundary |
| --- | --- | --- |
| Range and reserve | Preliminary fuel mass, tank topology, usable volume, and CG/fill sequence. | Mission solver determines achieved range and reserve. |
| Design payload | Cabin/hold capacity, payload loading, floor/volume, and mass/CG location. | Payload and mass stages determine closure and MTOW/MZFW margins. |
| Maximum passengers | Integer seat/deck/exit/monument layout and clearances. | Capacity model and selected fidelity own final capacity. |
| Maximum cargo and LD3-45 | Hold/container type, orientation, position list, and access envelope. | Cargo manager owns fill/mass capacity and compliance. |
| Cruise Mach, MMO/VMO, altitude | Sweep, thickness, section family, nacelle/pylon placement, and structural thickness bounds. | Atmosphere/aero/operating-envelope checks own margins. |
| ICA, TTC, OEI ceiling, maximum cruise altitude | Engine installation clearance, wing area/span, high-lift and tail/control geometry. | Propulsion and climb/performance solvers own achieved performance. |
| TOFL, landing distance, approach speed | Wing area/loading, high-lift topology, control geometry, gear, and ground stance. | Field-performance model owns distances and V_app. |
| Wingspan | Projected span and dihedral/non-planar geometry. | Airport constraint owns final limit residual. |
| ACN/pavement | Gear architecture, wheel count, track, load share, and flotation proxy. | Pavement model owns ACN/PCN compatibility. |
| CG and static margin | Payload/tank positions, fuel fill, gear, wing/tail placement, propulsion mass. | Mass-balance and stability stages own envelope and trim. |
| Wingbox/Nastran feasibility | Spar/rib/skin intent, thickness, structural attachments, mesh quality. | Structures stage owns stress, buckling, and solver evidence. |

If a selected fidelity cannot evaluate a translator, show the requirement as
not yet evaluated or diagnostic. Never report it as passed merely because the
geometry builder succeeded.

## Implementation notes for current ALAS boundaries

The requirements-first design already separates:

- the canonical DesignBrief from the physical AlasConfig;
- architecture and preliminary sizing from continuous candidate generation;
- GeometryConfig's scaffold from DesignVector-owned variables;
- geometry/accommodation from mass, aero, mission, and structures stages;
- typed candidate failure from objective ranking.

Recommended adapter flow:

1. Decode DesignBrief and ArchitectureSeed.
2. Materialise the supported scaffold into GeometryConfig.
3. Decode the continuous design vector into existing DesignVector fields.
4. Build source/master geometry and run G0–G7.
5. Resolve cabin, payload, fuel, mass, and CG from that same source.
6. Derive solver representations and run the existing physics funnel.
7. Attach source hash, representation/fidelity metadata, and residuals to
   candidate records and report/export payloads.

This memo does not change source code. New fields should be introduced only
where they preserve the one-source-of-truth contract and existing
golden/parity evidence. The preview adapter should consume the same materialised
geometry as the solver adapter; it should not invent another set of dimensions.

## Fidelity limits and cautions

- OpenVSP and TiGL are strong references for parametric, connected geometry and
  interoperability. They do not make every generated mesh analysis-grade.
- CPACS is a data exchange/schema contract, not by itself a guarantee of valid
  geometry, cabin compliance, or solver convergence.
- AVL and native VLM are valuable linear low-fidelity aerodynamic tools. They
  do not resolve viscous separation, stall, transonic shock structure, detailed
  high-lift performance, or certification loads.
- OpenAeroStruct is useful for coupled low-fidelity aero/structural optimisation
  and internal volume constraints. Its simplified beam/wingbox model is not
  complete airframe structural substantiation.
- NASA historical conceptual-design reports provide methods and integration
  lessons, not current certification rules or universal bounds.
- CST/Fourier coefficients are not meaningful without section metrics,
  coordinate conventions, and bounds.
- A visual preview, STL, or coarse DegenGeom output must not be described as a
  watertight CAD master, CFD surface, FE mesh, or manufacturing evidence unless
  it passed the corresponding gate.

## Unresolved and citation-only sources

These sources were identified as useful context but were not downloaded into
the local corpus. They remain citation-only so the corpus stays focused on the
already verified PDFs and rights metadata:

- DLR CPACS current HTML schema documentation and CPACS website, including
  current 3.5.x fuel-tank/deck elements [W1, W2, W8].
- “Aircraft Geometry and Meshing with Common Language Schema CPACS for
  Variable-Fidelity MDO Applications,” Aerospace 5(2):47 [W7]. This is useful
  for CPACS-to-mesh fidelity context; use the publisher page for license and
  version details.
- OpenAeroStruct live geometry-manipulation and wingbox-fuel-volume
  documentation [W3, W4]. The cited local preprints cover the method, while
  the live pages document current API semantics.
- OpenVSP GitHub project and NASA Open Source Agreement notices [W5]. The
  local NASA papers are the research evidence; the repository is the current
  software/licence reference.
- AVL project landing page [W6]. The local User Primer is the archived manual;
  the project page is the current software distribution reference.
- Feng Deng, Cheng Xue, Ning Qin et al., “Parameterizing Airfoil Shape Using
  Aerodynamic Performance Parameters,” AIAA Journal 60(7), 2022,
  DOI 10.2514/1.J061464 [W9]. The downloaded 2023 open PAERO paper is the
  accessible source used here; the 2022 journal item remains a citation-only
  predecessor.
- NASA “Automated Tetrahedral Mesh Generation for CFD Analysis of Aircraft in
  Conceptual Design,” NTRS 20140008833. It is relevant to CFD mesh quality and
  automation but was not needed for the minimum corpus.
- NASA ACSYNT internal-layout work, including the internal layout module for
  concurrent cabin/external-shape design, NTRS 19960028158. It is a useful
  historical comparator for cabin-aware sizing but was not downloaded.
- NASA historical high-lift and field-performance reports beyond Rudolph's
  survey, including NTRS 19820007144 and 19790065319. They may support
  calibration later, but do not alter the topology recommendation here.
- Additional DLR CPACS-MONA and system-definition papers. They are relevant to
  process orchestration and schema evolution; this memo uses the downloaded
  collaborative configuration and cabin papers as the core evidence.

Citation-only does not mean “accepted without review.” Before adding any of
these to a redistributable bibliography, verify the exact version, metadata,
license, and checksum.

## Source inventory, metadata, and rights

The following PDFs were downloaded from legally accessible NASA, DLR,
university-author, publisher, arXiv, or MIT-hosted locations for this memo.
SHA-256 values are for the exact local files in
bib/geometry-configuration. NASA NTRS records identify these documents as
PUBLIC and generally mark US-government use as permitted; preserve the NASA
record, report number, and attribution. That status is not a blanket CC-BY
grant for every third-party figure.

For author-hosted preprints and AIAA papers, the source is legally accessible
for reading/research, but no blanket redistribution right is inferred. The
MDPI paper and the Walther cabin paper are identified as CC BY 4.0 by their
publishers. The DLR proceedings page identifies the 2016 paper as open access
with author permission for proceedings distribution. Check the source record
before redistributing any PDF or third-party figure.

| Ref | Local PDF | Pages | Bytes | SHA-256 | Source URL | Rights/access note |
| --- | --- | ---: | ---: | --- | --- | --- |
| R1 | hahn-2010-openvsp-parametric-geometry.pdf | 11 | 5707615 | 1ead1a613da9087cf249e86f26cc227b4f6bbceeb4b92ca6c485b7521f5d864b | https://ntrs.nasa.gov/citations/20100003046 | NASA NTRS: PUBLIC; GOV_PUBLIC_USE_PERMITTED; AIAA 2010-657. |
| R2 | litherland-2020-openvsp-ground-school.pdf | 12 | 599868 | 9ce8cbb1f95ca16ccfbd37aa38e68d7e55fd71085ab3d5212f0f4012e7421f4d | https://ntrs.nasa.gov/citations/20205007074 | NASA NTRS: PUBLIC; GOV_PUBLIC_USE_PERMITTED; workshop presentation. |
| R3 | mcdonald-2016-openvsp-degenerate-geometry.pdf | 14 | 1297307 | 7f97375051b1cb919a570e6f203786440a0517ef8268aa0f15489895ed336143 | https://ntrs.nasa.gov/citations/20160010160 | NASA NTRS: PUBLIC; GOV_PUBLIC_USE_PERMITTED; NF1676L-22875. |
| R4 | garcia-2019-viper-mdao-openvsp.pdf | 12 | 712195 | cdf78608f45abe2e21d77a340025a23a81d32a69f85c5be9c3e7b9fd3f3cc877 | https://ntrs.nasa.gov/citations/20190027157 | NASA NTRS: PUBLIC; PUBLIC_USE_PERMITTED; NASA/TM-2019-220239. |
| R5 | prakasha-ciampa-nagel-2016-collaborative-configuration.pdf | 14 | 1405905 | 31690f18dac69e86ce3c5696176ddd44e3ad45907416bd073b7a89ad18cdada0 | https://elib.dlr.de/110984/ | DLR eLib: refereed open-access ICAS paper; proceedings permission noted. |
| R6 | siggel-2018-tigl-parametric-geometry.pdf | 23 | 4908445 | d627a242b522fc809d8c7b99c30eafd097294d422a4618b00a88dba3e14a67cb | https://arxiv.org/abs/1810.10795 | Author-posted arXiv preprint; journal-version rights separate. |
| R7 | walther-2022-cpacs-cabin-description.pdf | 14 | 1973972 | 60cc010ede23e989ade6a867a71367bd901a52adf1e9ccd3a6a8df4a4b75db0c | https://doi.org/10.1007/s13272-022-00610-5 | CEAS Aeronautical Journal; publisher/source identifies CC BY 4.0. |
| R8 | fuselage-configuration-studies-1977.pdf | 277 | 11589492 | c8d89e3a7859444bb35a307948ddfa06bd2678e1f507b090666f51ca01744308 | https://ntrs.nasa.gov/citations/19770022198 | NASA NTRS: PUBLIC; GOV_PUBLIC_USE_PERMITTED. Actual title is “A study of commuter airplane design optimization”; local filename highlights the topical chapter. |
| R9 | chai-mason-1997-landing-gear-integration.pdf | 194 | 6838829 | f4a7f468e21d0d0fa88b1f383aa3a2f440e1068addecb90194ca3808a6c2bdce | https://ntrs.nasa.gov/citations/19970031272 | NASA-CR-205551; PUBLIC; GOV_PUBLIC_USE_PERMITTED. |
| R10 | sandlin-swanson-1990-horizontal-control-surface-sizing.pdf | 122 | 3502469 | 2a176a7a200ed009a44542e97b9d07be5602335ca000eb221134ca1245c05564 | https://ntrs.nasa.gov/citations/19900017199 | NASA-CR-186872; PUBLIC; GOV_PUBLIC_USE_PERMITTED. |
| R11 | rudolph-1998-high-lift-mechanical-design.pdf | 118 | 4204364 | 193dcb305ba9cc74104e1aa24d61a22b5c5ef0753867060ad7d628fc1d69d810 | https://ntrs.nasa.gov/citations/19980021287 | NASA/CR-1998-196709; PUBLIC; GOV_PUBLIC_USE_PERMITTED. |
| R12 | li-robinson-2016-automated-fe-meshes.pdf | 13 | 2278216 | 596996bcb28e2a32dc312e64c1ce8912af104f1363935e31bf1edae4219b84f5 | https://ntrs.nasa.gov/citations/20160010023 | NASA NTRS: PUBLIC; GOV_PUBLIC_USE_PERMITTED; NF1676L-22762. |
| R13 | nasa-2015-rapid-robust-structural-analysis.pdf | 26 | 1152507 | 9f85856bc7f4fca594d3bedb3dd6703ed610aa2485b58afdd1240731f5dcbdc6 | https://ntrs.nasa.gov/citations/20150002820 | NASA/TM-2015-218687; PUBLIC; GOV_PUBLIC_USE_PERMITTED. |
| R14 | jasa-2018-openaerostruct.pdf | 16 | 813943 | 4b7aa8ddfbbc94536a02614b23b2e434fedd19f69f4992920e511c133f472315 | https://websites.umich.edu/~mdolaboratory/pdf/Jasa2018a.pdf | Author/university-hosted preprint; published-version rights separate. |
| R15 | chauhan-2018-openaerostruct-wingbox.pdf | 12 | 407701 | 590b6feb552c13e56530436ed974a93508b97aac808f67624e14a1c2a9e9b416 | https://websites.umich.edu/~mdolaboratory/pdf/Chauhan2018b.pdf | Author/university-hosted preprint; published-version rights separate. |
| R16 | drela-youngren-avl-user-primer.pdf | 43 | 445276 | 63a5566c19282991559107ab3fa77e8eb1645b1ed03a4724a8a4d68563b72988 | https://web.mit.edu/drela/Public/web/avl/AVL_User_Primer.pdf | MIT-hosted AVL manual; software and manual notices are separate. |
| R17 | kulfan-2007-cst-universal-parametric-geometry.pdf | 36 | 3819362 | 1a1b5d093e24e80347ffb9a60f618bd3b3c765e18f8c2a9cb1cb14d87d9815f1 | https://www.brendakulfan.com/_files/ugd/169bff_a0cd2bb07def4b80881e8501cdc262f4.pdf | Author-hosted AIAA-2007-0062; no blanket redistribution right inferred. |
| R18 | kulfan-2009-supersonic-wing-cst.pdf | 19 | 1529639 | 5e1da2159f0d2784615648b407c7c86dad06bacb1ae18de0c8b65c142ab475db | https://www.brendakulfan.com/_files/ugd/169bff_0f087ef10aaf4b63852a2ddd287558c1.pdf | Author-hosted AIAA/J. Aircraft paper; first page states personal/internal-use conditions. |
| R19 | nasa-2015-subsonic-ultra-green-airfoil-parameterization.pdf | 378 | 23275740 | 631a11e0067254cadf63dfd3d2c1644c0ba9ba93a70875db5344fb693b8a2a99 | https://ntrs.nasa.gov/citations/20150017036 | NASA/CR-2015-218704/VOL1; PUBLIC; PUBLIC_USE_PERMITTED. |
| R20 | deng-xue-qin-2023-paero-cst.pdf | 18 | 8023346 | b807d432616e451bdc3fbbc92c15ed0b197407438b77cd8bb79f2aeb8e9a0347 | https://www.mdpi.com/2226-4310/10/7/650 | MDPI Aerospace paper; publisher identifies CC BY 4.0. |

## Reference details

- **R1.** Andrew S. Hahn, “Vehicle Sketch Pad: a Parametric Geometry Modeler
  for Conceptual Aircraft Design,” AIAA 2010-657, NASA, 2010.
- **R2.** Brandon Litherland, “OpenVSP Ground School Overview,” OpenVSP
  Workshop, NASA, 2020.
- **R3.** Erik D. Olson, “Multi-Disciplinary, Multi-Fidelity Discrete Data
  Transfer Using Degenerate Geometry Forms,” NASA, 2016.
- **R4.** Joseph A. Garcia, Jeffrey V. Bowles, David J. Kinney, John E.
  Melton, and Xun J. Jiang, “VIPER Integrated MDAO Analysis for Conceptual
  Design of Supersonic X-Plane Vehicles,” NASA/TM-2019-220239, 2019.
- **R5.** Prajwal Shiva Prakasha, Pier Davide Ciampa, and Björn Nagel,
  “Collaborative Systems Driven Aircraft Configuration Design Optimization,”
  ICAS, DLR, 2016.
- **R6.** Martin Siggel, Jan Kleinert, Tobias Stollenwerk, and Reinhold Maierl,
  “TiGL — An Open Source Computational Geometry Library for Parametric
  Aircraft Design,” arXiv:1810.10795; journal publication, 2018–2019.
- **R7.** Jan-N. Walther, Christian Hesse, Marko Alder, Jörn Y.-C.
  Biedermann, and Björn Nagel, “Expansion of the cabin description within the
  CPACS air vehicle data schema to support detailed analyses,” CEAS
  Aeronautical Journal 13, 1119–1132, 2022.
- **R8.** J. Roskam, R. D. Wyatt, D. A. Griswold, and J. L. Hammer, “A study of
  commuter airplane design optimization,” NASA-CR-154270, 1977.
- **R9.** Sonny T. Chai and William H. Mason, “Landing Gear Integration in
  Aircraft Conceptual Design,” NASA-CR-205551, 1997.
- **R10.** Doral R. Sandlin and Stephen Mark Swanson, “A computer module used
  to calculate the horizontal control surface size of a conceptual aircraft
  design,” NASA-CR-186872, 1990.
- **R11.** Peter K. C. Rudolph, “Mechanical Design of High Lift Systems for
  High Aspect Ratio Swept Wings,” NASA/CR-1998-196709, 1998.
- **R12.** Wu Li and Jay Robinson, “Automated Generation of Finite-Element
  Meshes for Aircraft Conceptual Design,” NASA, 2016.
- **R13.** Lloyd B. Eldred, Sharon L. Padula, and Wu Li, “Enabling Rapid and
  Robust Structural Analysis During Conceptual Design,” NASA/TM-2015-218687,
  2015.
- **R14.** John P. Jasa, John T. Hwang, and Joaquim R. R. A. Martins,
  “Open-source coupled aerostructural optimization using Python,” Structural
  and Multidisciplinary Optimization 57, 2018.
- **R15.** Shamsheer S. Chauhan and Joaquim R. R. A. Martins, “Low-Fidelity
  Aerostructural Optimization of Aircraft Wings with a Simplified Wingbox
  Model Using OpenAeroStruct,” EngOpt, 2018.
- **R16.** Mark Drela and Harold Youngren, AVL User Primer, MIT.
- **R17.** Brenda M. Kulfan, “A Universal Parametric Geometry Representation
  Method — ‘CST’,” AIAA-2007-0062, 2007.
- **R18.** Brenda M. Kulfan, “New Supersonic Wing Far-Field
  Composite-Element Wave-Drag Optimization Method,” Journal of Aircraft 46,
  2009.
- **R19.** Marty K. Bradley, Christopher K. Droney, and Timothy J. Allen,
  “Subsonic Ultra Green Aircraft Research: Truss Braced Wing Design
  Exploration — Phase II — Volume I,” NASA/CR-2015-218704/VOL1, 2015.
- **R20.** Jianmiao Yi and Feng Deng, “Cooperation of Thin-Airfoil Theory and
  Deep Learning for a Compact Airfoil Shape Parameterization,” Aerospace
  10(7):650, 2023, DOI 10.3390/aerospace10070650.

## Online technical references

- **W1.** DLR CPACS documentation and coordinate/transformation guidance:
  https://dlr-sl.github.io/CPACS/html/89b6a288-0944-bd56-a1ef-8d3c8e48ad95.htm
- **W2.** CPACS project website: https://dlr-sl.github.io/cpacs-website/
- **W3.** OpenAeroStruct geometry manipulation:
  https://mdolab-openaerostruct.readthedocs-hosted.com/en/latest/advanced_features/geometry_manipulation.html
- **W4.** OpenAeroStruct wingbox fuel-volume constraint:
  https://mdolab-openaerostruct.readthedocs-hosted.com/en/latest/wingbox_fuel_vol_delta.html
- **W5.** OpenVSP source/project repository and licence notices:
  https://github.com/OpenVSP/OpenVSP
- **W6.** AVL project landing page and current distribution:
  https://web.mit.edu/drela/Public/web/avl/
- **W7.** “Aircraft Geometry and Meshing with Common Language Schema CPACS for
  Variable-Fidelity MDO Applications,” Aerospace 5(2):47:
  https://www.mdpi.com/2226-4310/5/2/47
- **W8.** CPACS current documentation, including fuel-tank and accommodation
  elements: https://dlr-sl.github.io/CPACS/html/
- **W9.** Deng et al. PAERO DOI record:
  https://doi.org/10.2514/1.j061464

## Practical acceptance checklist

Before a generated concept enters native aero or mission stages, confirm:

- the architecture seed is named, supported, and traceable to the brief;
- all dimensions use a declared frame and unit system;
- source/master geometry builds deterministically from the frozen vector/seed;
- planform integrals, chord stations, section closures, and intersections agree
  with tolerances;
- cabin/hold geometry places requested seats/containers with clearances and
  returns capacity by deck/hold;
- tail arm, gear contact, gear bay, tank, high-lift, and structural
  attachments are evaluated against current geometry and CG;
- usable fuel volume covers the specified mass, reserve, and fill case;
- every downstream representation is labelled master, solver, or preview, with
  fidelity and source hash;
- analysis mesh passes its own quality/connectivity checks rather than
  inheriting preview status;
- a requirement unavailable at this fidelity is explicitly not yet evaluated
  or diagnostic rather than passed;
- failed and near-feasible candidates retain typed stages and residuals.

This is the minimum contract for keeping requirements-first design,
interactive preview, optimisation, physics, and exported geometry aligned.
