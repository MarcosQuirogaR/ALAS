# Structures and aeroelasticity research

Research note for ALAS, prepared 2026-08-26. This note is
implementation-oriented: it translates the regulatory baseline and the
conceptual-design literature into stage contracts, evidence requirements, and
fidelity gates for the existing Rust pipeline.

This is not certification data. FAR-25 means 14 CFR Part 25 in this note;
CS-25 is the EASA certification specification. The live rule and the
applicable amendment, means of compliance, aircraft category, and
certification authority must always take precedence over this research note.

## Executive conclusion

ALAS already has a useful first-fidelity structural screen: an elliptic
semi-wing load, cantilever shear/moment integration, direct cap/web sizing,
Euler panel-buckling rib spacing, a mass-relieved Euler-Bernoulli beam report,
Rayleigh modal estimates, mesh generation, and optional NASTRAN execution.
That is a reasonable concept-ranking model.

It is not yet a general transport-aircraft structural or aeroelastic
substantiation model. In particular, the current three scalar cases do not
cover a regulatory load-case catalogue; the engine model is a point mass rather
than a pylon/engine load path; skin buckling, torsion, joints, fatigue, damage
tolerance, and material-environment allowables are not represented; and the
current modal and SOL111 paths do not constitute gust, flutter, divergence, or
control-reversal evaluation.

The recommended architecture is:

1. Make load cases, load paths, material allowables, and evidence provenance
   first-class data.
2. Keep the fast beam/wingbox model as the deterministic F1 screen, but attach
   an explicit capability matrix so an unsupported requirement is
   not_evaluated, never an implicit pass.
3. Add a shell/static/modal/buckling F2 model before using a solver-backed
   aeroelastic result to rank finalists.
4. Add a reduced-order aeroelastic F3 path using structural modes plus
   unsteady aerodynamic derivatives for gust, flutter, divergence, and control
   reversal.
5. Treat NASTRAN/FEA as an evidence-producing F4 stage only after input,
   mesh, equilibrium, solver, parser, convergence, and correlation gates pass.
6. Iterate structural mass, inertia, stiffness, loads, and mission performance
   to closure. A structural mass that is merely reported beside the original
   MTOW is not coupled sizing.

This fits the requirements-first flow in
docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md: architecture and load-case
definition precede sizing, wingbox screening precedes optional NASTRAN/AVL/MSES,
and final reports retain residuals and diagnostic reasons.

## 1. ALAS baseline and the gap it creates

The current implementation was inspected without changing source files.

| Existing owner | Current behavior | Safe interpretation | Main gap |
| --- | --- | --- | --- |
| crates/alas-struct/src/loads.rs | LoadCase has a static name, signed load factor, and total semi-wing force. load_cases returns pull-up, push-down, and level. The span load is elliptic and integrated tip-to-root. | Deterministic global bending screen. | No discrete/continuous gust, roll/yaw, flaps, landing, engine torque/failure/gyro, pressure, payload/fuel distribution, or load-spectrum identity. |
| crates/alas-struct/src/loads.rs | Positive-y wing engines become point masses using thrust/T/W and installation factor. | Preliminary inertial relief and modal-mass approximation. | No pylon stiffness, thrust/drag/torque, engine-out transient, side load, gyroscopic load, or engine-to-wing attachment path. |
| crates/alas-struct/src/sizing.rs | Spar caps are sized from bending strength, webs from root shear, skin is at configured minimum, and rib pitch comes from an Euler panel-buckling criterion. | F1 global wingbox sizing and mass estimate. | No distributed torsion, local/global skin-stringer buckling, web shear buckling, crippling, joints, load introduction, strength interaction, or fatigue/damage checks. |
| crates/alas-struct/src/analytical.rs | Euler-Bernoulli deflection, inertial relief, cap stress/margins, and Rayleigh modes. | Global stiffness and first modal trend. | No unsteady aerodynamics, modal damping identification, flutter determinant, divergence, control reversal, aeroservoelastic controls, or modal correlation. |
| crates/alas-config/src/structures.rs | Configures gauges, materials, stations, mesh density, MSC/NASA NASTRAN paths, SOL101/SOL103/SOL111, frequency sweep, damping, PSD, and Patran. | Good external-solver seam. | No typed fidelity policy, load-case catalogue, requirement coverage, allowable provenance, mass-coupling policy, convergence tolerances, or aeroelastic-specific input. |
| crates/alas-pipeline/src/structural.rs | StructuralAnalysisResult uses string status and optional sizing, analytical, mesh, NASTRAN, NASTRAN-95, and Patran results. It rejects negative/non-finite strength margins and over-wide ribs before meshing. | A physical sizing failure is already treated as meaningful. | Solver absence, solver failure, and unsupported physics need distinct typed evidence states; a successful stage execution must not be confused with a passed requirement. |

Two details deserve special care:

- The current negative case derives limit_load_factor_neg * 1.5, while
  additional_safety_factor is another configurable multiplier. The 1.5
  regulatory default and any engineering/company margin must remain separate
  fields, with an audit trail, or a concept can accidentally double-count (or
  hide) safety factor.
- The current SOL111 random-vibration result is a force-PSD/receptance
  calculation. It is useful vibration evidence, but it must not be labelled
  continuous-turbulence or CS/FAR 25.341 evidence without an atmospheric
  turbulence input, aircraft-level dynamic model, and the required load
  combinations.

## 2. Regulatory baseline translated to model obligations

### 2.1 FAR-25 / 14 CFR Part 25

Part 25 is a useful requirements catalogue even when ALAS is designing a
smaller or non-certified aircraft. The sections below are the minimum
categories that should be visible in the structural evidence model.

| Rule or guidance | Engineering obligation | Minimum conceptual evidence |
| --- | --- | --- |
| 14 CFR 25.301, 25.303, 25.305 | Loads must be in equilibrium with inertia; limit and ultimate conditions are distinct; the default safety factor is 1.5 unless another basis applies; deformation and load redistribution matter. | A load-case record with limit/ultimate semantics, an equilibrium residual, the applied factor provenance, and a statement of whether deformation redistribution is included. |
| 25.321, 25.331, 25.333, 25.335, 25.337 | Cover the flight envelope, critical weights, CG/payload/fuel distributions, speeds, symmetric maneuvers, and positive/negative V-n conditions. | A generated envelope matrix, not only the single largest root moment. Retain the governing case and near-governing cases. |
| 25.341 and AC 25.341-1 | Discrete gust uses a 1-cosine time history; continuous turbulence is a spectral/dynamic problem. The model should include the significant elastic, inertial, aerodynamic, and control-system characteristics of the coupled aircraft and examine mass, CG, configuration, thrust, altitude, and speed combinations. | Discrete gust response with a time history and dynamic increment; continuous turbulence response with PSD/transfer-function provenance; configuration and mass coverage. |
| 25.345, 25.349 | High-lift cases and asymmetric rolling/gust conditions can govern. Torsional flexibility and unequal left/right gust increments must be retained. | Flap/landing configuration cases, roll/yaw force and moment components, and a torsion-capability flag. |
| 25.361, 25.362, 25.363, 25.367, 25.371 | Engine/pylon structure sees torque, engine failure transients, side loads, asymmetric thrust/drag build-up, and gyroscopic loads. | Engine attachment load path, thrust/drag/torque/gyro components, engine-out transient assumptions, and adjacent-airframe load transfer. |
| 25.473, 25.479, 25.491 | Landing and ground cases include vertical sink speed, gear dynamics, spin-up/springback, side/drag components, and rough-ground taxi/roll. | Gear/attachment load case records and a declaration when landing dynamics are not modelled. A wing-only static gravity case is not landing evidence. |
| 25.571 and AC 25.571-1D | The structure must avoid catastrophic fatigue, corrosion, manufacturing, accidental-damage, and widespread-fatigue failure over life; damage tolerance, residual strength, inspection, PSE, WFD, and LOV evidence are part of the certification story. | At concept stage: mission spectrum, hot-spot stress-range proxy, material/environment basis, initial flaw/inspection assumptions, residual-strength proxy, and explicit not_certification status. |
| 25.629 and AC 25.629-1C | Evaluate flutter, divergence, control reversal, undue loss of stability/control from deformation, significant propeller/rotor whirl modes, normal envelope and failure conditions. The current AC stresses analysis/test combinations, model uncertainty, stiffness/mass/aero sensitivity, controls, and flight-test substantiation for a new type. | Structural modes, unsteady-aero model, damping/uncertainty assumptions, flutter/divergence/control-reversal margins, control-system model, engine/rotating-device modes, and a fidelity gate. |

The official live rule pages used for the synthesis are
[25.301](https://ecfr.io/Title-14/Section-25.301),
[25.303](https://ecfr.io/Title-14/Section-25.303),
[25.305](https://ecfr.io/Title-14/Section-25.305),
[25.321](https://ecfr.io/Title-14/Section-25.321),
[25.331](https://ecfr.io/Title-14/Section-25.331),
[25.333](https://ecfr.io/Title-14/Section-25.333),
[25.335](https://ecfr.io/Title-14/Section-25.335),
[25.337](https://ecfr.io/Title-14/Section-25.337),
[25.341](https://ecfr.io/Title-14/Section-25.341),
[25.345](https://ecfr.io/Title-14/Section-25.345),
[25.349](https://ecfr.io/Title-14/Section-25.349),
[25.361](https://ecfr.io/Title-14/Section-25.361),
[25.362](https://ecfr.io/Title-14/Section-25.362),
[25.363](https://ecfr.io/Title-14/Section-25.363),
[25.367](https://ecfr.io/Title-14/Section-25.367),
[25.371](https://ecfr.io/Title-14/Section-25.371),
[25.473](https://ecfr.io/Title-14/Section-25.473),
[25.479](https://ecfr.io/Title-14/Section-25.479),
[25.491](https://ecfr.io/Title-14/Section-25.491),
[25.571](https://ecfr.io/Title-14/Section-25.571), and
[25.629](https://ecfr.io/Title-14/Section-25.629). The downloaded official
package is [CFR-2025-title14-vol1.pdf](../../bib/structures-aeroelasticity/CFR-2025-title14-vol1.pdf);
the live online rule must be checked for amendments.

FAA advisory circulars are acceptable means of compliance guidance, not
regulations. The most relevant are
[AC 25.341-1](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/25.341-1),
[AC 25.571-1D](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/865446),
[AC 25.629-1C](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1043057),
and the composite-structure
[AC 20-107B](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/20-107B).

### 2.2 EASA CS-25

EASA CS-25 has the same important structural and aeroelastic families:
flight and ground loads, gust and turbulence, engine loads, fatigue/damage
tolerance, and aeroelastic stability. Use the applicable amendment rather than
copying FAR-25 factors into a generic model. The [CS-25 document
family](https://www.easa.europa.eu/en/document-library/certification-specifications/group/cs-25-large-aeroplanes)
and [CS-25 Amendment
28](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28)
were used here. The consolidated EASA Easy Access Rules are convenient for
cross-reference, but EASA labels them as a non-official consolidated
publication.

The EASA design implication is especially important for ALAS: each applicable
requirement must be considered at the appropriate mass, CG, configuration,
altitude, speed, and load distribution, with tests or calculations of
appropriate accuracy and a systematic investigation of probable combinations.
Therefore a structural stage should output a coverage matrix and not only the
single case that sized the caps.

For composites, [AMC-20 Amendment
19](https://www.easa.europa.eu/sites/default/files/dfu/AMC-20%20Amendment%2019.pdf)
and its AMC 20-29 material provide a useful allowables and environmental
qualification baseline. It is guidance/reference material, not a replacement
for the current certification programme or approved material database.

## 3. State of practice: a defensible fidelity ladder

The literature converges on a staged workflow. DLR's ELWIS chain generates
parameterized beam, shell, and aerodynamic models, computes aerodynamic,
fuel, landing-gear, and engine loads, couples them to structure, sizes, and
returns thickness/mass to a CPACS-level design loop. NASA's conceptual
structural work similarly emphasizes parametric geometry, automatic FE
generation, static/aeroelastic analysis, and rapid ranking across large
geometry changes. These are good precedents for ALAS.

| Tier | Model | Good for | Must not claim |
| --- | --- | --- | --- |
| F0 | Requirements and architecture contract | Load-case definition, mass/CG envelope, load-path existence, unsupported-physics visibility | Structural pass |
| F1 | Closed-form/1D beam and algebraic wingbox | Global bending/shear, preliminary cap/web sizing, rib-pitch proxy, deflection and first-frequency trends, early mass feedback | Local buckling certification, joint strength, fatigue/damage tolerance, flutter/gust compliance |
| F2 | Parameterized shell wingbox or equivalent beam/shell FE | Static equilibrium, load introduction, local stress/strain, skin/web/stringer buckling proxy, deflection/twist, modes, component mass | Robust aeroelastic certification without aero coupling and correlation |
| F3 | Reduced-order coupled aeroelastic model: structural modes plus VLM/DLM/unsteady derivatives and controls | Static aeroelastic deformation, discrete gust, continuous turbulence, flutter, divergence, control reversal, sensitivity studies | Final certification substantiation unless the authority accepts the method and the model is validated |
| F4 | Solver-backed NASTRAN/FEA with independent checks and convergence | Detailed static/modal/buckling/aeroelastic evidence for finalists, solver cross-check, mesh and load-path audit | Automatically valid evidence merely because an F06 or OP2 exists |
| F5 | Test/correlation/certification programme | Ground-vibration, wind-tunnel, flight-flutter, material/element/full-scale and fatigue/damage substantiation | In scope for a conceptual ALAS run; ALAS can prepare evidence packages and identify the missing test |

A lower tier can inform a higher tier, but it cannot silently inherit the
higher tier's requirement status. A candidate may be screen_pass at F1 and
flutter_not_evaluated at F1 at the same time.

The 2022 open multi-fidelity study by Thelen, Bryson, Stanford, and Beran is a
useful warning: low-fidelity/high-fidelity transition can reduce expensive
evaluations only when gradients, constraints, and the trust region are
credible; adding structural sizing can erase the expected saving if the
coupling is poorly conditioned. ALAS should retain residuals and evidence
quality instead of training or ranking on an undifferentiated Boolean.

## 4. Load cases and load paths

### 4.1 Load-case catalogue

Every structural case should be a named, immutable record with a requirement
owner, a mass/CG state, a configuration, a limit/ultimate state, and a fidelity
capability. The minimum catalogue is:

| Family | Cases to represent | Principal structural effects |
| --- | --- | --- |
| Mass, payload, fuel, and CG | Design minimum/maximum mass, MTOW/MLW, MZFW, payload distributions, fuel states, forward/aft CG, tank pressure and liquid inertia when relevant | Inertial loads, reactions, modal mass, load redistribution, trim and stability coupling |
| Symmetric manoeuvre | Positive and negative V-n points, pull-up/push-down, checked manoeuvre, pitch/rotation conditions, critical speed/altitude combinations | Wing/fuselage bending, tail loads, inertial relief, control-system loads |
| Discrete gust | Positive/negative vertical and lateral 1-cos gust, multiple gust gradients, speeds and configurations | Dynamic load increment, torsion, modal excitation, control response |
| Continuous turbulence | Atmospheric PSD, transfer function, aircraft modes, damping, integration bandwidth and response statistics | RMS/peak statistical loads, lightly damped-mode excitation, fatigue spectrum |
| Unsymmetric flight | Roll/yaw manoeuvre, one-side/other-side gust fractions, asymmetric thrust, engine-out corrective action | Torsion, differential spar/rib loads, fuselage/empennage and pylon reactions |
| High lift | Take-off, approach, landing flap settings, 2g/gust combinations where applicable | Flap tracks, rear spar, torsion, control-surface and high-lift attachment loads |
| Engine and propulsion | Thrust/drag, torque, acceleration/deceleration, engine failure transient, side load, gyroscopic load, propeller/rotor whirl, pylon flexibility | Pylon/wing attachment, local load introduction, aeroelastic modes, adjacent airframe |
| Landing and ground | Sink-speed cases, level landing, side-load/drift, spin-up/springback, taxi/rough ground, gear retraction and braking | Gear/fuselage attachment, local bending, dynamic shock, floor and keel load paths |
| Pressure and other sources | Cabin/centrebody pressure, doors/cut-outs, bird/foreign-object/discrete source damage, thermal and environmental gradients | Membrane/bending interaction, residual strength, local damage and load redistribution |
| Durability spectrum | Mission segments, cycles, gust/landing counts, pressurization cycles, temperature/moisture, inspection intervals | Stress range, crack/delamination growth, residual strength, WFD/LOV proxy |

limit and ultimate must be explicit, not inferred from the case name. Typically,
a limit response is used for deformation and serviceability while ultimate
strength includes the approved safety factor. Dynamic gust loads are not
obtained by multiplying an elliptic 1g load by a single n.

### 4.2 Load-path model

The minimum load path is a graph, not only a root resultant:

~~~text
aerodynamic surface / gust field
             + inertial mass distribution
             + fuel, payload, equipment, gear, engine, controls
                               |
                               v
        pressure panels -> skin/stringer -> ribs/webs -> spars
                               |             |
                               v             v
                     pylon/attachments -> center section/fuselage
                               |
                               v
                     mounts, joints, reactions, ground
~~~

At each spanwise or structural station, retain the six-component section
resultant: axial force, two shears, bending moments about both axes, and
torsion. Also retain the local applied loads, reaction loads, attachment IDs,
and equilibrium residual. A root bending moment alone cannot tell whether an
engine pylon, rear spar, rib, or joint is carrying a physically plausible
combination.

Load-path checks should answer:

- Does every external and inertial load have a connected structural path to a
  reaction?
- Are fuel/payload/engine masses represented at their actual attachment
  locations, not smeared into wing lift?
- Are lift-center offset, thrust/drag, control-surface, landing, and
  asymmetric loads converted to torsion and in-plane forces?
- Does the discretization preserve total force and moment to a configurable
  tolerance?
- Does a partial-span spar or pylon stop carrying load beyond its geometric
  break?
- Are the same section loads used by sizing, analytical response, FE FORCE/
  pressure cards, and aeroelastic mapping?

The last point is already a strength of ALAS: the shared load integration in
alas-struct::loads is intended to keep sizing, analytical results, and NASTRAN
FORCE cards aligned. The next step is to make the richer load-case and
load-path records the shared primitive.

## 5. Structural sizing and failure proxies

### 5.1 Global strength and stiffness

For early sizing, use a section model with axial force, two shears, bending
about both axes, and torsion. A useful first pass calculates cap/skin stress
from axial plus bending, web shear from vertical and in-plane shear, and
closed-box shear flow from torsion. Report demand, allowable, margin, and
governing component for every station and load case.

The existing cap sizing is appropriate as an F1 lower-complexity model, but a
future wingbox contract should distinguish:

- material allowable versus design allowable;
- limit stress versus ultimate stress;
- global section stress versus local hot-spot stress;
- elastic deflection/twist versus permanent deformation;
- mass-relieved versus conservative load response;
- analytical estimate versus FE result.

Use an explicit interaction policy for combined axial, bending, shear, and
torsion. If no validated interaction equation is available, report
unsupported_interaction rather than summing unrelated margins and calling the
result a pass.

### 5.2 Buckling and crippling

Skin and web buckling often governs before simple yield/ultimate strength. DLR
reports that a single load case cannot produce a useful thickness/mass
distribution and that skin buckling can govern below strength. NASA's HWB
centerbody work also uses minimum gauges and displacement constraints in a
NASTRAN optimization loop.

The F1/F2 proxy set should include:

- local plate buckling between ribs/stringers in compression and shear;
- web shear buckling and post-buckling policy;
- stiffener crippling or column buckling;
- global Euler or beam-column buckling;
- skin-stringer/beam-column interaction;
- rib crippling and load-introduction bearing/bypass;
- panel edge restraint and boundary-condition sensitivity;
- minimum manufacturing gauge and damage/imperfection knockdowns.

For an elastic eigenvalue proxy, expose the result as a factor:

    buckling_margin = lambda_cr / lambda_required - 1

where lambda_required includes the selected design state and knockdown policy.
Do not use the raw eigenvalue as an ultimate allowable: initial imperfections,
residual stress, stiffener eccentricity, joints, boundary conditions, and
post-buckling reserve can change the result substantially.

### 5.3 Fatigue and damage tolerance proxies

At concept stage, fatigue is best represented by a declared proxy rather than
omitted:

1. Build a mission/load spectrum from flight segments, gust occurrence,
   manoeuvre counts, landing cycles, pressurization cycles, and temperature/
   humidity exposure.
2. Convert section/hot-spot stresses to ranges with mean-stress and local
   concentration assumptions.
3. Use a material/process-specific S-N, strain-life, or composite fatigue
   degradation curve for a safe-life indicator.
4. For damage-tolerance candidates, seed an initial flaw or delamination,
   calculate residual strength and crack/delamination growth with a declared
   law and inspection interval, and report the scatter/knockdown basis.
5. Identify principal structural elements and locations where a single
   failure could be catastrophic; map them to inspection and fail-safe/load
   path assumptions.

The proxy output should say fatigue_screened, damage_tolerance_screened, or
not_evaluated; none is a certification finding. It should expose the missing
data: spectrum, material curve, initial flaw, environment, joint detail,
inspection, or full-scale correlation.

For composites, include fibre-direction failure, matrix/inter-fibre failure,
delamination/interlaminar stress, impact damage, compression-after-impact,
fatigue degradation, and environmental conditioning as separate mechanisms.
A Tsai-Wu or similar first-ply-failure result is a useful screening indicator,
but it is not equivalent to laminate ultimate, damage tolerance, or
certification allowables.

### 5.4 Material allowables

Material data must carry provenance and conditions. A material record should
include at least:

- material, product form, fibre/ply/lay-up or metal temper;
- process, cure/heat treatment, batch or qualification source;
- tension/compression/shear/bearing and fracture properties;
- orientation, laminate stacking sequence, thickness and joint detail;
- temperature, moisture, ageing, corrosion, fluid, and impact condition;
- A-, B-, mean, minimum, or other statistical basis;
- confidence/reliability, sample size, test method, and environment;
- knockdowns and the reason for each knockdown.

NASA's Allowables for Structural Composites explains the statistical
distinction directly: A-basis is the value below which only 1 in 100 specimens
is expected to fail at 95% confidence; B-basis is the corresponding 10 in 100
value at 95% confidence. These are not generic material constants. The NASA
and EASA material guidance both reinforce representative process and
environment testing. A single f_allow_pa field is adequate for a toy metal
screen, but it must not be presented as a certification material database.

## 6. Aeroelasticity

### 6.1 Required physics

A useful reduced-order equation is:

    M q_ddot + C q_dot + K q = Q_aero(q, q_dot, Mach, speed) + Q_control + f_external

where the structural mass and stiffness are generated from the same design
that supplies the aerodynamic model. The minimum coupled state includes rigid
body motion when it matters, structural modes, unsteady aerodynamic
derivatives, controls/actuators, engine/pylon masses and stiffness, fuel
states, and damping assumptions.

The aeroelastic check set is:

- static aeroelastic deformation and twist, including trim and load
  redistribution;
- dynamic gust response and continuous turbulence response;
- flutter speed and damping trend with velocity, Mach, density, mass, fuel,
  controls, and engine/store configuration;
- divergence or static aeroelastic instability;
- control reversal and control-effectiveness loss;
- control-system aeroservoelastic stability and gain/phase margin where
  applicable;
- propeller/rotor/engine whirl and gyroscopic coupling;
- failure conditions and damage/stiffness/mass sensitivity.

The DLR engine-wing integration study is directly relevant: engine position,
engine mass, and pylon stiffness alter aeroelastic stability, and a beam model
that omits engine-wing coupling can miss the controlling behavior. The
implication for ALAS is to preserve an explicit engine/pylon attachment and
mass/stiffness model even when the early external load is a point load.

### 6.2 Flutter, divergence, and control reversal gates

For an F3/F4 result, report a curve or table, not just one scalar:

- modal frequencies and damping versus speed;
- the mode identity and correlation used at each speed;
- flutter candidate speed, frequency, damping crossing, and uncertainty band;
- divergence determinant/eigenvalue or static-twist stability margin;
- control-effectiveness derivative and reversal speed/margin;
- configuration, fuel, engine, payload, CG, and failure state;
- numerical settings: modes retained, aerodynamic panels, reduced frequency,
  interpolation/spline, damping, and convergence.

A conservative conceptual default is to require the demonstrated instability
boundary to remain beyond the declared operating envelope plus a configurable
engineering margin, then to label any result lacking validation as
screening_only. The exact certification factor and test plan come from the
authority and certification basis, not from an ALAS default.

### 6.3 Current NASTRAN boundary

The existing configuration supports SOL101, SOL103, and SOL111. That is a
valuable foundation, but these are separate evidence types:

- SOL101 can support static stress/displacement and load-path checks;
- SOL103 can support free-vibration modes if mass, constraints, and model
  quality are credible;
- SOL111 can support frequency response or random/harmonic vibration for the
  specified input;
- none of these alone demonstrates gust, flutter, divergence, or control
  reversal;
- an aeroelastic solution needs aerodynamic influence/derivative data,
  structural modes, coupling/interpolation, controls, and a declared
  stability/response method.

Keep vibration PSD and atmospheric turbulence as distinct input schemas. A
force PSD used for engine excitation is not a gust PSD.

## 7. Structural mass feedback and uncertainty

### 7.1 Mass closure

Structural mass changes MTOW, wing loading, inertia, lift distribution,
mission fuel, trim, landing weight, and engine sizing. The coupling loop should
be explicit:

~~~text
requirements/configuration
        -> mass, CG, inertia, fuel/payload states
        -> aero/trim and manoeuvre/gust/ground loads
        -> structural sizing and stiffness/modes
        -> updated structural mass and attachment masses
        -> repeat until mass/load/stiffness residuals close
~~~

Use under-relaxation and a maximum iteration count. Store both the empirical
Torenbeek estimate and the sized structural mass, with labels such as
calibration_reference, sized_mass, and mass_closure_residual. Never silently
replace the mass model after a structural run.

The convergence record should include total mass, component masses, CG,
inertia, governing load, root moment/shear/torsion, maximum stress/buckling
margin, first modes, and mission performance. If it does not close, return a
typed mass_coupling_not_converged diagnostic and keep the last iterate for
inspection.

### 7.2 Uncertainty and robust sizing

Relevant uncertainties include:

- aerodynamic coefficients, gust gradient, turbulence PSD, and load
  combination;
- material allowable, thickness, density, joint efficiency, and
  manufacturing variation;
- stiffness, damping, mass distribution, fuel level, CG, and engine/pylon
  location;
- geometry, mesh, spline/interpolation, boundary condition, and solver
  numerical error;
- model-form error between beam, shell, and high-fidelity aeroelastic models.

Separate aleatory variability from epistemic model uncertainty. Early
implementation can use interval/knockdown bounds or Latin-hypercube samples;
later, use polynomial chaos or surrogate models for repeated robust
evaluations. The NASA aeroelastic-wingbox uncertainty study is a precedent for
propagating uncertain aeroelastic safety factors through sampling-driven
polynomial chaos and a nested topology/sizing optimization.

Robust outputs should include a quantile or probability target, sample count,
random seed, confidence interval, and the deterministic nominal result. A
candidate that passes only at the nominal point should not be called robust.

## 8. Recommended stage contracts

The following are proposed contracts, not source changes in this research
task. Names are illustrative Rust-like types.

~~~rust
enum LoadCategory {
    Maneuver, DiscreteGust, ContinuousTurbulence, RollYaw,
    HighLift, Engine, LandingGround, Pressure, DurabilitySpectrum,
    DamageSource,
}

enum DesignState {
    Limit,
    Ultimate,
    Serviceability,
    FatigueSpectrum,
    ResidualStrength,
}

struct StructuralLoadCase {
    id: LoadCaseId,
    category: LoadCategory,
    design_state: DesignState,
    mass_case: MassCaseId,
    cg_case: CgCaseId,
    configuration: ConfigurationId,
    atmosphere: AtmosphereState,
    speed: SpeedState,
    thrust: ThrustState,
    payload: PayloadState,
    fuel: FuelState,
    input: LoadInput,
    required_fidelity: FidelityTier,
    requirement_ids: Vec<RequirementId>,
    provenance: EvidenceRef,
}

struct SectionLoad {
    station: StationId,
    fx_n: f64,
    fy_n: f64,
    fz_n: f64,
    mx_nm: f64,
    my_nm: f64,
    mz_nm: f64,
    equilibrium_residual: EquilibriumResidual,
    source_nodes: Vec<LoadSourceId>,
}

struct LoadPathGraph {
    nodes: Vec<LoadPathNode>,
    edges: Vec<LoadPathEdge>,
    reactions: Vec<Reaction>,
}

struct StructuralAssessment {
    requirement_id: RequirementId,
    case_id: LoadCaseId,
    metric: MetricId,
    actual: Quantity,
    allowable: Option<Quantity>,
    margin: Option<f64>,
    status: AssessmentStatus,
    fidelity: FidelityTier,
    evidence: EvidenceRef,
}
~~~

The stage sequence should be:

| Stage | Inputs | Outputs | Gate |
| --- | --- | --- | --- |
| Requirements/load-case normalization | TLAR, aircraft configuration, applicable rule basis | Immutable load-case catalogue and coverage map | Every required category has an owner, or a typed unsupported diagnostic |
| Mass/CG/inertia | Payload, fuel, equipment, engines, structure | Mass states, CG envelope, inertia, discrete load sources | Mass closure and physically valid CG/inertia |
| Load generation | Atmosphere, aero/trim, controls, engine, ground model | Surface, nodal, and section loads for each case | Force/moment equilibrium and provenance |
| Load-path assembly | Geometry, attachments, structure graph | Connected load path and station resultants | No disconnected force, missing reaction, or illegal outboard load |
| F1 beam/wingbox screen | Section loads, material screen, gauges | Sizing, mass, stress, rib/buckling proxy, deflection, modes | Finite values, positive physical margins, declared capability |
| F2 shell/static/buckling | Parameterized shell mesh, load mapping, properties | Displacement, stress/strain, buckling, component mass | Mesh, BC, balance, convergence, local/global checks |
| F3 aeroelastic reduction | F2 mass/stiffness/modes, aero derivatives, controls | Gust, turbulence, flutter, divergence, reversal | Mode/aero mapping, speed sweep, damping/margin, sensitivity |
| Durability | Spectrum, hot spots, material/environment/inspection | Fatigue/damage proxy and missing-data ledger | Spectrum and allowable provenance; never silently default |
| F4 solver evidence | Approved deck, executable, output artifacts | Solver results, parser diagnostics, comparison/correlation | Tool, deck, output, parser, convergence, and independent-check gates |
| Candidate/report assembly | All assessments and diagnostics | Requirement trace, residuals, evidence package | Unsupported or failed requirements remain visible |

### 8.1 Typed diagnostics

Replace free-form structural status strings over time with an enum or a
structured diagnostic. At minimum:

~~~rust
enum StructuralDiagnosticCode {
    InputInvalid,
    RequirementUnsupported,
    LoadCaseMissing,
    LoadEquilibrium,
    LoadPathDisconnected,
    MassClosure,
    GeometryInvalid,
    MaterialAllowableMissing,
    StaticStrength,
    Buckling,
    FatigueProxy,
    DamageToleranceProxy,
    MeshInvalid,
    BoundaryConditionInvalid,
    ModalInvalid,
    Flutter,
    Divergence,
    ControlReversal,
    SolverUnavailable,
    SolverLaunchFailed,
    SolverFailed,
    ResultUnreadable,
    VerificationMismatch,
    MeshNotConverged,
    FidelityInsufficient,
    Uncertainty,
}

struct StructuralDiagnostic {
    code: StructuralDiagnosticCode,
    severity: Severity,
    stage: StageId,
    case_id: Option<LoadCaseId>,
    requirement_ids: Vec<RequirementId>,
    message: String,
    actual: Option<Quantity>,
    target: Option<Quantity>,
    evidence: Option<EvidenceRef>,
}
~~~

SolverUnavailable means the requested evidence was not produced; it is not the
same as SolverFailed, and neither is the same as a physical StaticStrength
failure. A solver that is optional for a screening run can leave the candidate
usable while marking the affected requirements as unassessed.

## 9. High-fidelity NASTRAN/FEA promotion gate

High fidelity should be a promotion decision for finalists, not a magic
boolean attached to an executable path.

| Gate | Required checks | Failure state |
| --- | --- | --- |
| Entry | Candidate passed F0/F1 requirements; geometry, material, mass, CG, load catalogue, and load-path IDs are frozen and versioned | FidelityInsufficient or InputInvalid |
| Tool provenance | Executable path/version, dialect, deck generator version, environment, timeout, and license/availability recorded | SolverUnavailable |
| Mesh | No duplicate/zero-area elements, invalid connectivity, disconnected islands, bad normals, uncontrolled aspect/skew, or missing properties; mesh statistics stored | MeshInvalid |
| Boundary conditions | Constraints remove rigid-body modes only as intended; attachments, RBE/MPC, pylon, gear, and symmetry assumptions are explicit | BoundaryConditionInvalid |
| Load mapping | Applied force/pressure/temperature/PSD cards reproduce the section resultant and all six global reactions within configured tolerance | LoadEquilibrium |
| Static | Solver completes without fatal errors; displacements/stresses are finite; reaction balance, constraint forces, and energy are plausible; compare F1 global response | SolverFailed, ResultUnreadable, or VerificationMismatch |
| Buckling | Eigenvalue/linear buckling assumptions, modes, boundary conditions, minimum gauges, and knockdowns are recorded; compare shell/beam trends | Buckling or MeshNotConverged |
| Modal | Positive physical frequencies, sensible effective mass, mode count/frequency convergence, and mode identity/correlation to F1 | ModalInvalid or VerificationMismatch |
| Aeroelastic | Structural modes are mapped to the aerodynamic model; Mach/reduced-frequency/panel convergence, damping, flutter, divergence, reversal, controls, and engine cases are covered | Flutter, Divergence, ControlReversal, or FidelityInsufficient |
| Convergence | Repeat with mesh, modes, frequency step, timestep, and relevant solver tolerances; store response changes and governing-case stability | MeshNotConverged or VerificationMismatch |
| Cross-check | Compare at least one result against an independent formulation, second solver/dialect, analytical bound, benchmark, test datum, or trusted reference case | VerificationMismatch |
| Artifact | Deck, input data, solver stdout/stderr, F06/OP2/plot/result hashes, parser version, and diagnostic summary are retained | ResultUnreadable |

Suggested starting defaults should be configuration, not hard-coded policy:
global force/moment balance below 0.1% of the applied resultant; static
global response change below 5% under the selected refinement; buckling/mode
quantities below 10% under refinement; and flutter/reversal boundary
convergence below the project-defined engineering tolerance. These are
screening defaults only; an authority-approved analysis plan can require
different thresholds.

Promotion rules:

- A successful executable return is not a pass.
- An absent optional executable leaves the candidate at the previous
  validated tier and sets the higher-tier requirements to not_evaluated.
- A high-fidelity physical failure is a physical failure, even if F1 passed.
- A parser warning that changes a result is a failed evidence gate, not a
  cosmetic warning.
- A high-fidelity result cannot upgrade fatigue/damage tolerance or
  certification status without allowables, spectra, inspection, and
  correlation evidence.

## 10. Mapping to the ALAS crates and requirements document

### alas-config

Future structural configuration should add typed equivalents of:

- fidelity policy and required tier per requirement/candidate stage;
- load-case catalogue and rule-basis metadata;
- mass-coupling iteration count, relaxation, and convergence tolerances;
- material/allowable provenance, environmental condition, statistical basis,
  and knockdowns;
- buckling, fatigue, damage, aeroelastic, and uncertainty policy flags;
- mesh quality, balance, modal, aeroelastic, and solver-artifact gate
  tolerances;
- engine/pylon/gear/attachment properties, not only engine spanwise position;
- separate gust/turbulence inputs from harmonic/random engine-vibration PSD.

### alas-struct

The current modules can evolve around shared typed primitives:

~~~text
load_cases       catalogue, envelope, gust, landing, engine, spectra
load_paths       graph, attachments, section resultants, balance
beam             current analytical/sizing F1 path
shell            parameterized wingbox and component FE model
buckling         local/global/crippling proxies and FE extraction
durability       spectra, fatigue, damage, inspection proxies
aeroelastic      modes, DLM/VLM coupling, gust, flutter, divergence, reversal
evidence         fidelity gates, hashes, parser, comparison, diagnostics
~~~

Keep the shared load integration primitive, but extend it so the analytical,
mesh, NASTRAN, and aeroelastic consumers consume the same StructuralLoadCase
and SectionLoad records. This prevents the current same-elliptic-load benefit
from being lost when richer cases are added.

### alas-pipeline

The current StructuralAnalysisResult should eventually carry:

- typed overall state: screen_pass, screen_fail, high_fidelity_pass,
  high_fidelity_fail, or not_evaluated;
- per-requirement and per-load-case assessments;
- diagnostics with stage and case IDs;
- load-path and equilibrium summaries;
- mass-coupling iterations and residuals;
- fidelity gates and evidence references;
- separate solver availability, execution, parsing, verification, and
  physical-result states.

The pipeline should preserve the existing early rejection of infeasible rib
spacing and non-finite/negative strength margins. It should add analogous
early gates for missing load cases, disconnected paths, invalid mass/CG,
unresolved material basis, and unsupported high-fidelity claims.

### REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md

The existing document already states the key policy: load-case definition
comes before preliminary sizing; structural loads and wingbox sizing are a
stage; optional NASTRAN/AVL/MSES are later evidence; failed external solvers
must return typed reasons; and a requirement not supported at a selected
fidelity remains visible as not yet evaluated or diagnostic.

The structural extension should add a table linking every requirement to:
load-case family, mass/CG/configuration matrix, minimum fidelity, output
metric, allowable/provenance, and promotion gate. This makes wingbox /
NASTRAN feasibility more informative than a single status string and lets the
optimiser rank screened concepts without pretending that unassessed flutter or
damage tolerance passed.

## 11. Literature synthesis

### NASA and AGARD

- Eldred, Padula, and Li, NASA/TM-2015-218687, demonstrates a rapid
  conceptual structural subprocess with parameterized geometry, automatic FE
  mesh generation, static/aeroelastic analysis, and structural sizing. The
  main ALAS lesson is to make geometry and model generation robust to large
  design changes and to rank structural feasibility rather than require a
  perfect detailed model for every concept.
- Bradley, NASA/CR-2004-213016, uses FE-based centerbody sizing for a
  blended-wing-body. It supports a separate centrebody/load-path model when
  the aircraft architecture cannot be represented by a conventional wing
  root.
- Jutte, Stanford, and Wieseman, NASA/TM-2015-218697, shows the
  mass/flutter/stress interaction of parametric spars, ribs, and stringers.
  A topology/layout choice changed weight, flutter speed, and stress in
  opposite directions. Layout variables therefore belong outside a simple
  scalar cap-thickness optimizer.
- Stanford, Jutte, and Coker, NASA 2019, nests outer topology/layout choices
  with inner sizing. This is a strong pattern for ALAS optimisation:
  discrete load-path choices should be screened outside a continuous sizing
  loop, and each candidate must retain feasibility residuals.
- Stanford and Roy, IFASD 2019/NASA 2020 record, propagates uncertain
  aeroelastic safety factors through sampling-driven polynomial chaos. It
  supports an uncertainty tier after deterministic closure rather than
  pretending a single nominal flutter margin is robust.
- Gern, NASA 2012, demonstrates a scalable NASTRAN FE model for HWB
  centrebody structural optimization, including load-case reduction,
  minimum gauges, displacement constraints, and load mapping from aerodynamic
  tools. It is a direct precedent for a finalist-only shell/NASTRAN gate.
- Yates, NASA/TM-100492, documents the AGARD 445.6 standard aeroelastic
  configuration. ALAS should use this type of benchmark for modal/aeroelastic
  regression before trusting a new DLM/FE coupling path. The original AGARD
  report is not needed when the public NASA standard-configuration copy is
  available.
- Nettles, NASA 2004, is the material-allowables reference for distinguishing
  A/B statistical bases and for refusing a generic, environment-free
  composite allowable.

### DLR

- Scherer et al., ELWIS, is the closest process precedent: parameterized
  geometry, beam/shell/VLM model generators, aerodynamic/fuel/landing/
  engine loads, coupling, sizing, calibration, and mass return to the
  aircraft definition.
- Voss and Klimmek's UCAV study uses hundreds of manoeuvre/gust/landing cases
  and compares a Pratt-style quasi-static gust approximation with dynamic
  modal/unsteady-aero response. Pratt is reasonable for a preliminary screen,
  but it can be higher or lower than the dynamic result and must not be
  promoted silently.
- Schulze, Neumann, and Klimmek show that engine mass, location, and pylon
  stiffness alter aeroelastic stability. The engine must be represented as a
  coupled attachment model as soon as flutter or dynamic gust is assessed.
- Voss and Klimmek's fighter study demonstrates the practical scale of a
  flight-envelope loads analysis: 688 manoeuvre cases and a structural
  optimization from section/nodal loads. ALAS should reduce cases only with a
  declared envelope-clustering or governing-case algorithm that preserves
  traceability.

### Open academic work

- Thelen et al., Algorithms 2022, uses multi-fidelity gradient-based
  aeroelastic optimization and is useful for transition/trust-region policy.
- Hosseini et al., Aerospace 2024, presents a modular conceptual-design/MDO
  architecture and a beam-to-shell/FE reasoning that supports a staged ALAS
  implementation.
- Hoghøj et al., arXiv/published 2023, couples a source-doublet panel method
  one-way to linear FE topology/shape optimization. Its explicit one-way
  coupling limitation is a useful warning: the approximation is most
  defensible for small deformation and should be labelled accordingly.
- Kilimtzidis and Kostopoulos, Aerospace 2023, is a useful online open
  reference for high-aspect-ratio composite wing optimization with strength,
  panel buckling, and flutter constraints. It illustrates that buckling and
  flutter constraints can become active together and that a lower-fidelity
  optimum should trigger a higher-fidelity investigation.

## 12. Sources not downloaded

The following were cited or considered but intentionally not copied:

- Krengel and Hübner, A Physics-Based Approach for Aeroservoelastic Wing
  Sizing in Conceptual Aircraft Design, DLR eLib 138258: the repository
  identifies the PDF as DLR-internal/restricted.
- Original AGARD-R-677/AGARD report mirrors where rights were not clear:
  the public NASA TM-100492 AGARD 445.6 standard-configuration report is used
  instead.
- Paywalled AIAA/Springer final papers where an author or NASA/DLR open copy
  was not available. The open NASA, DLR, MDPI, and arXiv versions are the
  reference copies.
- MDPI Aerospace 10(3), 251, Static Aeroelastic Optimization of
  High-Aspect-Ratio Composite Aircraft Wings via Surrogate Modeling, is
  cited online because the publisher's direct PDF endpoint was not
  consistently retrievable in this environment; no unclear-rights mirror was
  downloaded. See the [publisher article](https://www.mdpi.com/2226-4310/10/3/251).

## 13. Local PDF ledger

All files below were downloaded on 2026-08-26 into
bib/structures-aeroelasticity/. Every file was checked for the %PDF- header.
SHA-256 values are lowercase hexadecimal and should be recomputed before a
release or citation archive is made.

Rights notes describe why a local research copy was considered acceptable;
they do not grant redistribution rights or replace the publisher/authority's
terms.

| Local PDF | Metadata and source | Year | Size (bytes) | Rights note | SHA-256 |
| --- | --- | ---: | ---: | --- | --- |
| [CFR-2025-title14-vol1.pdf](../../bib/structures-aeroelasticity/CFR-2025-title14-vol1.pdf) | U.S. Government, Code of Federal Regulations, Title 14, Volume 1; [GovInfo record](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1.pdf) | 2025 package | 13,910,044 | Official U.S. Government publication; reference copy. Check current eCFR/amendments and third-party material before redistribution. | 40114aad73b1af4dabc2458bcc1ea644adbacb33d5a4ef3c7c8ca0bb507e5a27 |
| [FAA-AC-25-341-1.pdf](../../bib/structures-aeroelasticity/FAA-AC-25-341-1.pdf) | FAA, Dynamic Gust Loads, AC 25.341-1; [official landing page](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/25.341-1) | 2014 | 663,765 | Official FAA/U.S. Government advisory circular; non-regulatory guidance/reference copy. | eb390b7a63aa12f1b648863ee14a26a3a9e886c53b070249a27a1e36fe26db6d |
| [FAA-AC-25-571-1D.pdf](../../bib/structures-aeroelasticity/FAA-AC-25-571-1D.pdf) | FAA, Damage Tolerance and Fatigue Evaluation of Structure, AC 25.571-1D; [official landing page](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/865446) | 2011 | 693,367 | Official FAA/U.S. Government advisory circular; non-regulatory guidance/reference copy. | 7e0bbae7a71d174cbdfb3f45bb3902a1d606053e688844c2eaebc9a4a8517926 |
| [FAA-AC-25-629-1C.pdf](../../bib/structures-aeroelasticity/FAA-AC-25-629-1C.pdf) | FAA, Means of Compliance to 14 CFR 25.629, Aeroelastic Stability Requirements, AC 25.629-1C; [official landing page](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1043057) | 2024 | 608,734 | Official FAA/U.S. Government advisory circular; non-regulatory guidance/reference copy. | 8b40586b5167f131057ba33a2d49a56b204b681e9f8999f3c7533e1d6398c7fc |
| [FAA-AC-20-107B-change1.pdf](../../bib/structures-aeroelasticity/FAA-AC-20-107B-change1.pdf) | FAA, Composite Aircraft Structure, AC 20-107B with Change 1; [official landing page](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/20-107B) | 2009 | 644,300 | Official FAA/U.S. Government advisory circular; non-regulatory guidance/reference copy. | b41e2bd8b5d94a8f0530b7fe803ed8f319209b234e9feb67f990dd18dd7a1588 |
| [EASA-CS-25-Amendment-28.pdf](../../bib/structures-aeroelasticity/EASA-CS-25-Amendment-28.pdf) | EASA, Certification Specifications and Acceptable Means of Compliance for Large Aeroplanes CS-25 Amendment 28; [official page](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28) | 2023, corrected 2025 | 23,584,619 | Official EASA publication; local reference copy retaining EASA attribution/disclaimer, not a normative replacement. | 32f1a9acf26e8ceceb291d206bf07a7071f7f59f40f10c096bb17f34c6a58007 |
| [EASA-AMC-20-Amendment-19.pdf](../../bib/structures-aeroelasticity/EASA-AMC-20-Amendment-19.pdf) | EASA, General Acceptable Means of Compliance for Airworthiness of Products, Parts and Appliances (AMC-20) Amendment 19, including AMC 20-29; [official PDF](https://www.easa.europa.eu/sites/default/files/dfu/AMC-20%20Amendment%2019.pdf) | 2020 | 9,196,226 | Official EASA AMC publication; local research/reference copy retaining EASA terms and disclaimer. | 43b8dfc8a3259cf21c618d31b93c00af6d765caf451031a400a819f6c8ebb211 |
| [NASA-TM-2015-218687-structural-analysis.pdf](../../bib/structures-aeroelasticity/NASA-TM-2015-218687-structural-analysis.pdf) | Lloyd B. Eldred, Sharon L. Padula, Wu Li, Enabling Rapid and Robust Structural Analysis During Conceptual Design, NASA/TM-2015-218687; [NTRS record](https://ntrs.nasa.gov/citations/20150002820) | 2015 | 1,152,507 | NASA NTRS metadata: PUBLIC, GOV_PUBLIC_USE_PERMITTED; no third-party content indicated. | 9f85856bc7f4fca594d3bedb3dd6703ed610aa2485b58afdd1240731f5dcbdc6 |
| [NASA-CR-2004-213016-BWB-sizing.pdf](../../bib/structures-aeroelasticity/NASA-CR-2004-213016-BWB-sizing.pdf) | Kevin R. Bradley, A Sizing Methodology for the Conceptual Design of Blended-Wing-Body Transports, NASA/CR-2004-213016; [NTRS record](https://ntrs.nasa.gov/citations/20040110949) | 2004 | 1,197,883 | NASA NTRS metadata: PUBLIC, PUBLIC_USE_PERMITTED. | cc00d924cfd66937894b12fa4c5361c9f67b0b5197621fe345ec3cf13182833e |
| [NASA-TM-2015-218697-CRM-wingbox.pdf](../../bib/structures-aeroelasticity/NASA-TM-2015-218697-CRM-wingbox.pdf) | Christine V. Jutte, Bret K. Stanford, Carol D. Wieseman, Internal Structural Design of the Common Research Model Wing Box for Aeroelastic Tailoring, NASA/TM-2015-218697; [NTRS record](https://ntrs.nasa.gov/citations/20150003788) | 2015 | 1,583,771 | NASA NTRS metadata: PUBLIC, PUBLIC_USE_PERMITTED. | 6f7186cb004a18916c3cacad4c102c5cef1c75cd9bf5755788ffd9e048c21c96 |
| [NASA-2019-aeroelastic-wingbox-nested-optimization.pdf](../../bib/structures-aeroelasticity/NASA-2019-aeroelastic-wingbox-nested-optimization.pdf) | Bret K. Stanford, Christine V. Jutte, Christian A. Coker, Sizing and Layout Design of an Aeroelastic Wingbox through Nested Optimization; [NTRS record](https://ntrs.nasa.gov/citations/20190000445) | 2019 | 8,347,477 | NASA NTRS metadata: PUBLIC, PUBLIC_USE_PERMITTED. | c22273fa119334ee7eafb0549f23dc90d18d58f1f92be11634dac8a6b4e326b3 |
| [NASA-2020-aeroelastic-wingbox-uncertainty.pdf](../../bib/structures-aeroelasticity/NASA-2020-aeroelastic-wingbox-uncertainty.pdf) | Bret K. Stanford, Satadru Roy, Sizing and Topology Design of an Aeroelastic Wingbox Under Uncertainty; [NTRS record](https://ntrs.nasa.gov/citations/20200002634) | 2019/2020 record | 7,962,640 | NASA NTRS metadata: PUBLIC, GOV_PUBLIC_USE_PERMITTED; no third-party content indicated. | dfa486345be6aa5597a871a705a28e04ff77f1a4d433db3b6a9c555ccb0350d0 |
| [NASA-TM-100492-AGARD-445.6.pdf](../../bib/structures-aeroelasticity/NASA-TM-100492-AGARD-445.6.pdf) | E. Carson Yates Jr., AGARD Standard Aeroelastic Configurations for Dynamic Response: Candidate Configuration I - Wing 445.6, NASA/TM-100492; [NTRS record](https://ntrs.nasa.gov/citations/19880001820) | 1987 | 3,496,860 | NASA NTRS metadata: PUBLIC, GOV_PUBLIC_USE_PERMITTED; public NASA copy used instead of unclear-rights mirrors. | 7145d3283d6859f7a685a5546515d9f2bdc93cbdefb4a8bcd65002ad54c2dd75 |
| [NASA-2012-BWB-FEA-centerbody.pdf](../../bib/structures-aeroelasticity/NASA-2012-BWB-FEA-centerbody.pdf) | Frank H. Gern, Finite Element Based HWB Centerbody Structural Optimization and Weight Prediction; [NTRS record](https://ntrs.nasa.gov/citations/20120008184) | 2012 | 1,457,767 | NASA NTRS metadata: PUBLIC, GOV_PUBLIC_USE_PERMITTED; no third-party content indicated. | 881b84cbf9a66dd759a94258f54e870de7c765bc4031d1eb9a62df905663c381 |
| [NASA-2004-composite-allowables.pdf](../../bib/structures-aeroelasticity/NASA-2004-composite-allowables.pdf) | Alan T. Nettles, Allowables for Structural Composites; [NTRS record](https://ntrs.nasa.gov/citations/20040111395) | 2004 | 193,334 | NASA NTRS metadata: PUBLIC, GOV_PUBLIC_USE_PERMITTED. | f2ac685bbba8d90fa018c28715826673e576d753d34d74e9b19f5a948073d9c7 |
| [DLR-2013-ELWIS-structural-sizing.pdf](../../bib/structures-aeroelasticity/DLR-2013-ELWIS-structural-sizing.pdf) | J. Scherer, D. Kohlgrüber, F. Dorbath, M. Sorour, A Finite Element Based Tool Chain for Structural Sizing of Transport Aircraft in Preliminary Aircraft Design; [DLR eLib record](https://elib.dlr.de/84917/) | 2013 | 2,179,285 | DLR eLib marks the record Open Access; local reference copy with attribution. | cc1b303d247f3916df64a324977f094b7b24a3e9b2d4384413cfb18044662563 |
| [DLR-2017-UCAV-aeroelastic-sizing.pdf](../../bib/structures-aeroelasticity/DLR-2017-UCAV-aeroelastic-sizing.pdf) | Arne Voss, Thomas Klimmek, Design and Sizing on a Parametric Structural Model for a UCAV Configuration for Loads and Aeroelastic Analysis; [DLR eLib record](https://elib.dlr.de/107257/) | 2017 | 2,600,621 | DLR eLib marks the record Open Access; local reference copy with attribution. | 2b5a21534aa07223ce4fd0027ecc29b7331e73ad2a23afbdc30e0c30a78b2bb4 |
| [DLR-2020-engine-wing-integration-aeroelasticity.pdf](../../bib/structures-aeroelasticity/DLR-2020-engine-wing-integration-aeroelasticity.pdf) | Matthias Schulze, Jens Neumann, Thomas Klimmek, Parametric Modeling of a Long-Range Aircraft under Consideration of Engine-Wing Integration, Aerospace 2021, 8, 2; [DLR eLib record](https://elib.dlr.de/140161/) | 2020/2021 online publication | 9,193,709 | DLR repository marks Open Access Gold; PDF identifies the MDPI CC BY 4.0 license. | 1f552611daaaf0dc51624659e77d7af00832a9c8befcb90164df6d907337da38 |
| [DLR-2022-fighter-aeroelastic-loads.pdf](../../bib/structures-aeroelasticity/DLR-2022-fighter-aeroelastic-loads.pdf) | Arne Voss, Thomas Klimmek, Aeroelastic Modeling, Loads Analysis and Structural Design of a Fighter Aircraft; [DLR eLib record](https://elib.dlr.de/188328/) | 2022 | 6,416,439 | DLR eLib marks the record Open Access; local reference copy with attribution. | 2a2881725cd8d37159239ac2ff4cdb942b1fd06b926df5fbca789f16a66cd475 |
| [MDPI-2022-multifidelity-aeroelastic-optimization.pdf](../../bib/structures-aeroelasticity/MDPI-2022-multifidelity-aeroelastic-optimization.pdf) | Andrew S. Thelen, Dean E. Bryson, Bret K. Stanford, Philip S. Beran, Multi-Fidelity Gradient-Based Optimization for High-Dimensional Aeroelastic Configurations, Algorithms 2022, 15, 131; [article](https://www.mdpi.com/1999-4893/15/4/131) | 2022 | 6,479,698 | PDF states CC BY 4.0 open-access license. | d4521fc19f8fc28270052357ea4ed14891f7cf4cc5587be76d25911d24c53973 |
| [MDPI-2024-conceptual-design-MDO.pdf](../../bib/structures-aeroelasticity/MDPI-2024-conceptual-design-MDO.pdf) | Saeed Hosseini, Mohammad Ali Vaziry-Zanjany, Hamid Reza Ovesy, A Framework for Aircraft Conceptual Design and Multidisciplinary Optimization, Aerospace 2024, 11, 273; [article](https://www.mdpi.com/2226-4310/11/4/273) | 2024 | 11,650,905 | PDF states CC BY 4.0 open-access license. | 02a0bcd8b2e38040c0766f1a802828bff25c10fbbf77a4dae131bd3d6e0659cf |
| [arXiv-2209.09330-wing-shape-topology.pdf](../../bib/structures-aeroelasticity/arXiv-2209.09330-wing-shape-topology.pdf) | Lukas C. Hoghøj, Cian Conlan-Smith, Ole Sigmund, Casper Schousboe Andreasen, Simultaneous Shape and Topology Optimization of Wings, arXiv:2209.09330v2; [arXiv record/license](https://arxiv.org/abs/2209.09330) | 2023 published / 2022 preprint | 11,238,457 | arXiv record grants arXiv a non-exclusive license to distribute and states the submitter has the right to grant it; local author-version research copy, retain attribution and do not treat as a blanket redistribution license. | 20588e19b1bcefb1eec316da815998c8badff743e4394356b392d4ddade66115 |

## 14. Recommended implementation order

1. Add immutable load-case, mass/CG, section-load, load-path, assessment, and
   diagnostic types. Keep the existing F1 path as an adapter.
2. Expand maneuver cases and add a coverage matrix for gust, high-lift,
   asymmetric, engine, landing, pressure, and durability families.
3. Separate limit/ultimate factors and add equilibrium checks for force,
   moment, and load introduction.
4. Add structural mass feedback with relaxation and explicit convergence
   evidence.
5. Add local buckling, web/crippling, torsion, joint/load-introduction, and
   material-allowable provenance proxies.
6. Build a parameterized shell F2 model and regression it against F1,
   analytical bounds, and the NASA/DLR benchmark evidence.
7. Add mode extraction/correlation and F3 DLM/VLM aeroelastic coupling,
   beginning with static aeroelasticity and discrete gust before flutter and
   control reversal.
8. Add fatigue/damage spectra and uncertainty as separate declared
   capabilities, not hidden factors in strength margins.
9. Upgrade NASTRAN results to the promotion gate above, retaining decks,
   solver outputs, hashes, parser diagnostics, convergence studies, and
   independent comparisons.
10. Update the requirements document's structural mapping so every result says
    which requirement, load case, fidelity, allowable, and evidence supports
    it.

The immediate high-value change is not a more elaborate formula. It is making
the existing physical results honest and composable: a fast wingbox screen,
clear missing physics, traceable load cases, and a hard boundary before a
solver-backed aeroelastic claim.
