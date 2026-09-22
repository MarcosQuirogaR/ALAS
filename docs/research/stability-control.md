# Stability and control research for conceptual aircraft design

Status: research and architecture note for ALAS
Date: 2026-08-26
Scope: static and dynamic stability, trim, primary-control sizing, engine-out controllability, handling qualities, gust response, flight-envelope and CG coverage, and optimization coupling.

This note is an engineering research input, not a certification compliance statement. Civil certification material is used to define the shape of demanding load cases and evidence, while military handling-quality documents and research papers are used to define analysis methods and metrics. A value quoted from a regulation, military specification, or example aircraft is not a universal ALAS design requirement.

## 1. Decision summary

The useful conceptual-design object is not a single static-margin number. It is a typed set of aircraft states, configurations, failures, controls, and evidence, with separate records for:

- static stability and neutral point;
- trimmed force and moment equilibrium;
- control authority and control-use margins;
- OEI performance and OEI controllability;
- dynamic modes and derivative quality;
- task-based handling qualities;
- discrete-gust, continuous-turbulence, and control-system response; and
- uncertainty, model domain, and residual status.

The recommended ALAS policy is a staged funnel:

1. Make geometry, references, signs, mass properties, and units valid.
2. Require converged trim in a small nominal case set.
3. Sweep forward and aft CG, weight, configuration, speed, altitude, and thrust states for static margin, trim, and control authority.
4. Add OEI controllability, dynamic modes, actuator limits, and nonlinear time histories as the concept becomes credible.
5. Add gust, flexible/aeroservoelastic effects, pilot-in-loop handling qualities, and flight-data identification only when the fidelity and design intent justify them.

This avoids two common errors: treating a positive static margin as proof that an aircraft is controllable, and making uncertain early-fidelity dynamic or gust calculations hard optimization gates. The preferred static margin can be a soft target; a physical minimum is a policy-controlled hard constraint only after reference conventions and model validity have been checked.

## 2. Evidence scope and interpretation

### 2.1 Civil requirements and guidance

FAA AC 25-7D provides an acceptable flight-test interpretation of transport-category requirements. It calls for adequate trim over the reasonably maintained steady-flight conditions and consideration of weights from minimum in-flight weight through maximum takeoff weight. It treats aft CG as generally critical for static longitudinal stability and gives procedures for stabilized static-stability and return-to-trim demonstrations. Its material is guidance, not a regulation. See [S1] and the [FAA AC 25-7D page](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/1033309).

FAA AC 25.341-1 treats discrete gust and continuous turbulence as dynamic analyses. The modeling can include rigid-body, elastic, inertial, unsteady-aerodynamic, propulsion, and control-system effects; the required sophistication depends on the aircraft. Its discrete-gust discussion uses a 1-cosine profile and a range of gust-gradient distances, while continuous turbulence excites lightly damped modes. It is also guidance, not a regulation. See [S2] and the [FAA AC 25.341-1 page](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/25.341-1).

EASA CS-25 Amendment 28 is the civil regulatory context archived here. The relevant families are CS 25.143/25.147 for general and lateral-directional controllability, CS 25.149 for minimum control speeds, CS 25.161/25.171/25.173/25.181/25.255 for trim and stability, CS 25.675/25.677 for control stops and trim systems, and CS 25.341 for gust and turbulence. These clauses define demanding cases such as critical engine failure, unfavorable CG, takeoff and approach configurations, rudder-force limits, heading/attitude behavior, dynamic stability, and gust-response modeling. See [S3] and the [official EASA CS-25 page](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28). Confirm the latest amendment and correction before any compliance use; the official page has indicated file replacement/correction activity after the archived edition.

### 2.2 Military handling qualities and research

MIL-F-8785C is a historical US military specification, inactive for new design, but remains a useful source of mode, damping, control-force, and pilot-task language. MIL-STD-1797 and its revisions supersede or relate to it. The complete MIL-STD-1797B standard is not archived in this repository because the official DLA metadata identifies distribution restrictions; the public NASA review of contributions to MIL-STD-1797C is used instead. See [S4], [S7], and the [DLA MIL-STD-1797 metadata](https://quicksearch.dla.mil/qaDocDetails.aspx?ident_number=70037).

RTO-TR-029 emphasizes a complete flight-control development process: aerodynamic and system requirements, an unaugmented model, criteria and specification, control-law design, verification, piloted simulation, and aeroservoelasticity. It is particularly useful for the warning that actuator nonlinearities, rate limits, delays, saturation, and pilot-aircraft coupling can change flying qualities; an ideal linear control derivative is not enough. See [S5].

NASA RP-1168 and the NASA dynamic-modeling study provide the validation bridge from conceptual derivatives to measured behavior. They cover output-error parameter estimation, equations of motion, maneuver and envelope coverage, sensor effects, mass-property errors, geometry errors, and uncertainty in identified models. See [S8] and [S9]. Cooper-Harper pilot ratings are task-oriented evidence, not a substitute for trim or stability equations. See [S18].

### 2.3 Rights and local-use rule

The local archive records provenance, rights notes, page counts, and SHA-256 values. NASA NTRS entries used here are marked public or government-public-use-permitted in their metadata, but a third-party figure or embedded conference paper can still have separate rights. The Abbott Aerospace copies of MIL-F-8785C and RTO-TR-029 are publicly accessible mirrors; redistribution rights were not independently audited. The AGARD NTRS record has a copyright determination of `OTHER`, and the EUCASS paper is published with author permission. Treat those local copies as research inputs and do not redistribute them from ALAS. The JATM paper is marked CC BY 4.0 by the journal.

## 3. State of the art for conceptual sizing

### 3.1 Static margin and neutral point

For a consistent reference point, axis convention, and derivative sign convention:

```text
SM = (x_NP - x_CG) / c_bar = -C_m_alpha / C_L_alpha
x_NP = x_CG + SM * c_bar
```

The neutral point is the CG location at which the aircraft pitching-moment slope is zero. Static margin describes the slope at the selected CG; it does not describe available elevator authority, trim drag, stall recovery, actuator margin, or dynamic stability. A neutral-point estimate is only meaningful when the wing, tail, downwash, fuselage, propulsion, and reference definitions are consistent.

The conventional scissor-plot logic is still useful at low fidelity:

- the forward-CG boundary is often set by rotation, flare, approach trim, or nose-wheel lift-off control authority;
- the aft-CG boundary is often set by minimum static stability, nose-down recovery, or acceptable handling qualities;
- the feasible region changes with flap/gear state, speed, Mach, power, payload, and fuel; and
- a tail-volume coefficient is a starting correlation, not a topology-independent guarantee.

The EUCASS horizontal-tail study [S20] illustrates the scissor-plot combination of forward-CG takeoff rotation and aft-CG static-stability/nose-down-recovery constraints. The JATM tailplane study [S19] similarly combines static stability and controllability and validates a sizing method against several transport aircraft. The numerical examples in those papers, including typical positive static-margin rules of thumb and relaxed-static-stability values, should remain literature context rather than hard ALAS thresholds.

Relaxed static stability can reduce trim drag or improve performance, but the benefit is coupled to augmentation, control power, actuator bandwidth, failure behavior, sensor quality, and pilot-aircraft interaction. NASA's relaxed-static-stability report [S15] and the conceptual optimization studies [S13] and [S14] show why stability and control must enter MDO together. A negative or very small bare-airframe margin is therefore neither automatically invalid nor automatically acceptable.

Recommended evidence for each static-margin result is: derivative source and fidelity, reference point, CG and MAC definition, alpha/Mach/q condition, tail efficiency/downwash model, fuselage contribution, finite-difference step if applicable, convergence status, and uncertainty interval.

### 3.2 Trim and out-of-trim behavior

At a steady flight condition, the minimum longitudinal trim residual is:

```text
r_trim = [ C_L - W/(q*S), C_m ]
```

The unknowns may be angle of attack and horizontal-tail incidence, elevator deflection, or a coordinated combination. The same condition should report trim drag, thrust/power setting, control deflection, control force if modeled, and distance to stops or rate limits. A trim solution at one angle of attack is not enough to establish a stable or controllable aircraft.

The case grid should include minimum in-flight weight through MTOW where relevant, forward and aft CG, cruise and high-speed conditions, takeoff rotation, approach/landing with high lift, go-around or thrust changes, and plausible fuel/payload states. For out-of-trim cases, retain the initial trim state and explicitly model the trim-system authority and movement time. FAA AC 25-7D discusses speed, configuration, weight, force, and return-to-trim behavior; CS-25 has corresponding trim, stability, and out-of-trim clauses [S1, S3].

Trim is both a feasibility metric and an objective driver. Excessive tail download, elevator deflection, or stabilizer incidence can increase drag and tail/wing loads. The optimization should therefore preserve a separate `trim_drag` or `trim_power` objective instead of hiding it inside a static-margin gate.

### 3.3 Elevator and horizontal-tail sizing

The first sizing cases should be a pair of complementary controls problems, not one tail-volume correlation:

1. Forward-CG, takeoff configuration, rotation at the scheduled or design rotation speed. Require nose-wheel lift-off or target pitch response without exceeding elevator/stabilizer deflection, hinge moment, force, or actuator limits.
2. Aft-CG, low-speed and high-lift configuration, trimmed approach or stall-recovery/nose-down case. Require enough nose-down moment and recovery margin without unacceptable force or saturation.

Add cruise and high-speed trim, power changes, flap transitions, mistrim, and pitch-rate/normal-acceleration response when the design intent requires them. NASA CR-186872 [S10] is a direct conceptual horizontal-control-surface sizing reference; it models center-of-gravity limits, downwash/upwash, ground effect, stability, and takeoff rotation. The JATM and EUCASS papers [S19, S20] are useful independent tail-sizing checks.

The control record should distinguish geometric surface size from effective control power. At minimum, estimate `C_m_delta_e`, elevator effectiveness versus alpha and flap state, hinge-line or equivalent control-force assumptions, maximum deflection, rate, delay, and any aerodynamic separation or nonlinear effectiveness loss.

### 3.4 Rudder, vertical-tail, and lateral control sizing

The vertical tail and rudder must be checked against several distinct regimes:

- engine-out yaw moment and sideslip at takeoff and approach speeds;
- crosswind or crab/sideslip control at takeoff and landing;
- directional static stability (`C_n_beta`) and rudder effectiveness (`C_n_delta_r`);
- Dutch-roll damping and yaw/roll coupling;
- full-rudder pedal force and deflection limits; and
- one-engine-inoperative roll-away and heading-control behavior.

NASA CR-2014-218168 [S11] explicitly treats vertical-tail/rudder characteristics, engine-out controllability, crosswind capability, static and dynamic stability, and the possibility of using active flow control to increase rudder effectiveness. NASA's LCX conceptual study [S17] is a useful historical example of vertical-tail sizing being driven by critical engine-out and dynamic-stability considerations.

A rudder geometric fraction is not an authority result. The sizing evidence should include the available and required yawing moment, sideslip, bank angle, pedal force, rudder deflection, local dynamic pressure, propulsive/wing interference, and whether the operating engine thrust was reduced. For a multi-engine vehicle, use the actual engine locations and failure states rather than a generic vertical-tail multiplier.

For roll control, report `C_l_delta_a`, adverse yaw, roll acceleration, bank capture or roll-rate response, differential/symmetrical surface allocation, and remaining authority after a failed engine or spoiler/aileron failure. Aileron span and chord fractions in `ControlSurfacesConfig` are geometry inputs, not proof of roll control.

### 3.5 Engine-out performance versus engine-out controllability

These are separate requirements and must not share a single residual:

```text
OEI performance: excess thrust, climb gradient, ceiling, V2 or mission performance
OEI controllability: yaw/roll moment balance, sideslip, heading, bank, force, and authority
```

For an engine failure, the required yaw moment includes asymmetric thrust and all relevant aerodynamic and propulsion terms. In a simplified geometry model the asymmetric thrust contribution has the form:

```text
N_thrust = sum(T_i * y_i)
```

with signs and reference axes defined by the aircraft model. The total balance also depends on drag, sideslip, rudder, vertical-tail effectiveness, slipstream, propeller state, and possible thrust transients.

Civil minimum-control-speed cases use the critical failure mode, critical configuration, critical weight/CG, thrust on the operating engine(s), and pilot-force/heading/attitude restrictions. CS 25.149 and FAA AC 25-7D [S1, S3] provide the case structure; exact certification values must be taken from the applicable authority and aircraft category. Recommended ALAS cases are `VMCG`, `VMC_AIR`, `VMCL`, and `OEI_GO_AROUND` where the architecture supports them. `V2` and second-segment climb remain performance cases, not substitutes for the controllability solve.

### 3.6 Dynamic derivatives and modes

A conceptual derivative set should include at least the static derivatives and the rate derivatives needed for the selected equations of motion:

```text
Longitudinal: C_L_alpha, C_D_alpha, C_m_alpha, C_m_q, C_m_alpha_dot,
              C_L_delta_e, C_m_delta_e
Lateral-directional: C_y_beta, C_l_beta, C_n_beta,
                     C_l_p, C_l_r, C_n_p, C_n_r, C_y_p, C_y_r,
                     C_l_delta_a, C_n_delta_a, C_l_delta_r, C_n_delta_r
```

The exact set depends on the state vector and reference convention. Store the derivative units explicitly: per radian, per nondimensional rate, or dimensional derivative. Dimensionalization must include mass, inertia, dynamic pressure, reference area, span, and MAC consistently.

The usual early modes are short-period, phugoid, roll subsidence, Dutch roll, and spiral. Mode metrics should include eigenvalue, natural frequency, damping ratio, period or time constant, and stable/unstable status. Bare-airframe mode values do not establish closed-loop handling qualities; augmentation, control allocation, actuator dynamics, sensor filtering, delays, and pilot task can change the result.

MIL-F-8785C [S4], AGARD-531 [S6], NASA's MIL-STD-1797C review [S7], and RTO-TR-029 [S5] support using mode criteria as task- and vehicle-dependent evidence. Use published Level 1/2/3 or damping/time-constant boundaries only when the aircraft class, flight phase, control mode, and criterion source match. Otherwise report the mode and mark the criterion `diagnostic` or `unresolved`.

### 3.7 Handling qualities and pilot-aircraft coupling

Handling qualities are task-based. The Cooper-Harper scale [S18] exists because a pilot rating depends on the task, mission phase, workload, control inceptor, visual cueing, and aircraft response, not only on an eigenvalue. RTO-TR-029 [S5] adds the development-process and safety lessons for modern flight-control systems, including pilot-induced oscillation (PIO) and nonlinear control-system effects. NASA's 2020 report [S7] summarizes NASA contributions to MIL-STD-1797C, including response metrics, inceptor characteristics, PIO, high-alpha, and aeroelastic handling-quality topics.

For early ALAS concepts, use handling-quality metrics in three layers:

- `bare_airframe_diagnostic`: modes, control power, and open-loop response;
- `closed_loop_analytical`: commanded response, bandwidth/phase delay where a control law exists, actuator activity, and failure behavior; and
- `pilot_or_simulation_evidence`: task performance, workload, Cooper-Harper or related pilot rating, and PIO observations.

Do not claim a MIL-F or MIL-STD handling-quality level from the current closed-form modes alone.

### 3.8 Gust and turbulence response

The recommended conceptual gust ladder is:

1. steady 1g trim at the selected weight, CG, speed, altitude, and configuration;
2. discrete vertical and lateral 1-cosine gusts over a gradient-distance sweep;
3. continuous turbulence using a documented spectrum such as Dryden or von Karman;
4. control-law and actuator response if gust alleviation or active control exists; and
5. elastic/aeroservoelastic response for a flexible finalist.

FAA AC 25.341-1 [S2] describes the discrete and continuous approaches and emphasizes model validation, configuration/weight/CG/thrust/speed/altitude coverage, and nonlinear time-domain methods where linear methods are not adequate. NASA's flexible-aircraft method [S12] includes rigid and flexible modes, a 1-cosine gust, continuous von Karman turbulence, and control surfaces.

Early metrics should be typed as response evidence rather than silently treated as structural certification loads:

- peak and RMS normal/lateral acceleration and incremental load factor;
- vertical or lateral gust-to-output transfer response;
- pitch, roll, yaw rate, attitude, and sideslip excursions;
- peak control deflection, rate, acceleration, saturation, and duty cycle;
- structural load or bending response when a structural model exists; and
- ride-quality measures such as acceleration, jerk, or spectral exposure when mission relevant.

### 3.9 Flight-envelope and CG coverage

The envelope is a product of speed, Mach, altitude, configuration, weight, CG, thrust, atmosphere, and failure state. A V-n diagram at one CG and one clean configuration is only a projection. The coverage record should make the omitted dimensions explicit.

At minimum, the stability/control sweep should contain:

- forward and aft certified/design CG, plus any fuel-transfer intermediate CG;
- minimum in-flight, design payload, maximum payload, MTOW, and MLW states as applicable;
- clean, takeoff, approach, landing, and high-lift transition configurations;
- low-speed rotation/approach, climb, cruise, high-speed MMO/VMO, and maneuver points;
- sea-level hot/cold or density extremes as mission relevant, and altitude/Mach where derivative changes matter;
- all-engines-operating, critical engine-out, thrust-transient, and propulsion-failure cases; and
- positive/negative maneuver and gust cases when structural or operational intent requires them.

NASA RP-1168 [S8] treats envelope coverage and expansion as part of system identification planning. ALAS should use the same principle at conceptual fidelity: a result is only as strong as the state-space region it covers.

### 3.10 Coupling stability/control to optimization

NASA's conceptual optimization work [S13, S14] demonstrates the main design lesson: static trim constraints alone can shift the optimum substantially, and adding dynamic response and actuator constraints changes the feasible design space again. The recommendation for ALAS is therefore:

- keep geometric sizing variables and control-law variables separate until both are represented;
- use a small set of hard physical constraints only when the calculation converges and is within its model domain;
- use normalized soft residuals for preferred static margin, damping, trim drag, control size, and response quality;
- carry `not_evaluated`, `out_of_domain`, and `non_converged` as explicit statuses rather than converting them to zero violation;
- use robust or interval constraints for uncertain derivatives and CG/mass properties once a design is mature; and
- add gust, flexible modes, and pilot-task constraints in a later optimization stage or finalist screen.

A useful early objective vector is range/mission performance, mass, trim drag, control-surface area or weight, and a penalty for authority/robustness shortfall. The optimizer should not be allowed to make an aircraft “better” by exploiting an unmodeled elevator limit, an empirical VMC factor, a missing OEI yaw balance, or a non-converged trim solve.

## 4. Typed load cases and metrics recommended for ALAS

The following is a data-contract recommendation, not a source-code change in this task.

### 4.1 Case and policy types

```text
enum StabilityControlCaseKind {
    CruiseTrim,
    HighSpeedTrim,
    TakeoffRotation,
    ApproachTrim,
    LandingTrim,
    GoAroundTrim,
    StaticStability,
    Maneuver,
    VmcGround,
    VmcAir,
    VmcApproach,
    OeiSecondSegment,
    OeiGoAround,
    Crosswind,
    DynamicLongitudinal,
    DynamicLateralDirectional,
    ControlFailure,
    DiscreteGust,
    ContinuousTurbulence,
    VnEnvelope,
    UpsetRecovery,
}

enum RequirementPolicy { Hard, Soft, Objective, Diagnostic }

enum EvidenceStatus {
    Pass, Fail, NotEvaluated, NonConverged, NonFinite,
    OutOfDomain, InsufficientAuthority, MissingModel,
}

struct StabilityControlCase {
    id: String,
    kind: StabilityControlCaseKind,
    mass_kg: f64,
    cg_body_m: [f64; 3],
    inertia_kg_m2: [f64; 3],
    altitude_m: f64,
    mach: f64,
    calibrated_or_true_speed_m_s: f64,
    dynamic_pressure_pa: f64,
    atmosphere_id: String,
    configuration_id: String,
    payload_state: String,
    fuel_state: String,
    thrust_state: String,
    failure_state: String,
    control_limits_id: String,
    requirement_policy: RequirementPolicy,
    evidence_status: EvidenceStatus,
}
```

The `cg_body_m` field should be accompanied by the reference-frame and datum identifier. Do not store a static margin without the MAC, neutral-point reference, and sign convention that produced it.

### 4.2 Minimum case matrix

| Case family | Representative cases | Required outputs | Early policy |
|---|---|---|---|
| Trim | forward/aft CG at cruise, high speed, approach, landing, go-around | alpha, tail incidence/elevator, `C_L` and `C_m` residuals, trim drag, control/force margin | Hard after convergence; otherwise explicit non-convergence |
| Static longitudinal | forward/aft CG at cruise, approach, high-lift, and high speed | `C_L_alpha`, `C_m_alpha`, neutral point, static margin, CG-to-NP distance | Physical floor hard only by policy; preferred target soft |
| Rotation | forward CG, takeoff flaps, scheduled rotation speed, ground effect | pitch moment, rotation angle/rate, elevator/stabilizer margin, tail load | Hard for a takeoff-capable concept once model is valid |
| Nose-down/recovery | aft CG, high lift, low speed near stall warning or design recovery point | nose-down moment, remaining elevator, alpha/normal-acceleration response | Hard for certification-intent concepts; diagnostic earlier |
| Lateral/directional authority | roll capture, bank reversal, coordinated turn, crosswind | roll/yaw acceleration, bank/sideslip, `C_l_delta_a`, `C_n_delta_r`, control margin | Soft/diagnostic until control derivatives are validated |
| OEI controllability | `VMCG`, `VMC_AIR`, `VMCL`, critical engine failure, go-around | yaw moment balance, beta, heading, bank, rudder force/deflection, thrust state | Hard for multi-engine certification-intent concepts |
| OEI performance | second-segment climb, OEI ceiling, V2 | excess thrust, gradient, ceiling, speed margin | Separate hard performance residual |
| Dynamic modes | short-period, phugoid, roll subsidence, Dutch roll, spiral | eigenvalues, damping, frequency, time constant/period, derivative provenance | Diagnostic/soft until state-space and derivatives are validated |
| Actuator/control law | command steps/doublets, rate and position limits, failure/degraded control | response time, overshoot, rate, saturation, delay, PIO indicators | Soft/diagnostic early; hard for a closed-loop design |
| Gust | discrete 1-cosine, positive/negative, vertical/lateral; continuous turbulence | peak/RMS acceleration, load factor, rates, control activity, structural response | Diagnostic early; late hard load/response evidence |
| Envelope/CG | all weight/CG/configuration/speed/altitude/failure combinations | min/max of every metric plus coverage and uncertainty | Hard coverage completeness; metric thresholds policy-specific |

### 4.3 Metric records

```text
struct TrimMetric {
    alpha_deg: f64,
    stabilizer_or_elevator_deg: f64,
    cl_residual: f64,
    cm_residual: f64,
    trim_drag_coefficient: f64,
    control_force_N: Option<f64>,
    position_margin: f64,
    rate_margin: f64,
}

struct StaticStabilityMetric {
    cl_alpha_per_rad: f64,
    cm_alpha_per_rad: f64,
    cn_beta_per_rad: Option<f64>,
    neutral_point_x_m: f64,
    static_margin: f64,
    cg_to_neutral_point_m: f64,
    derivative_fidelity: String,
}

struct ControlAuthorityMetric {
    derivative: String,
    required_moment_Nm: f64,
    available_moment_Nm: f64,
    authority_margin: f64,
    deflection_deg: f64,
    rate_deg_s: f64,
    force_N: Option<f64>,
    saturated: bool,
}

struct OeiControllabilityMetric {
    failed_engine: String,
    asymmetric_thrust_moment_Nm: f64,
    residual_yaw_moment_Nm: f64,
    beta_deg: f64,
    heading_error_deg: f64,
    bank_deg: f64,
    rudder_deflection_deg: f64,
    rudder_force_N: Option<f64>,
    speed_margin_m_s: f64,
}

struct DynamicModeMetric {
    mode: String,
    eigenvalue_real_per_s: f64,
    eigenvalue_imag_per_s: f64,
    damping_ratio: Option<f64>,
    period_or_time_constant_s: Option<f64>,
    criterion_id: Option<String>,
}

struct GustResponseMetric {
    gust_model: String,
    gust_gradient_m: Option<f64>,
    peak_delta_n: f64,
    peak_acceleration_m_s2: f64,
    peak_rate: Option<f64>,
    peak_control_deflection_deg: f64,
    peak_control_rate_deg_s: f64,
    structural_load: Option<f64>,
}

struct ConstraintResidual {
    name: String,
    actual: f64,
    target: f64,
    units: String,
    direction: String,
    normalized_violation: f64,
    policy: RequirementPolicy,
    status: EvidenceStatus,
    load_case_id: String,
    source_id: String,
    fidelity: String,
    uncertainty: String,
}
```

For a lower-is-better constraint, a consistent normalized violation can be `max(0, (actual-target)/scale)`; for an upper-is-better constraint, reverse the sign. Store the scale and direction so that a residual remains auditable. A missing or invalid metric must not become `0.0` violation.

## 5. ALAS mapping and current gaps

The repository already has a useful low-fidelity foundation. The mapping below identifies what can be used now and what must not yet be inferred from it.

| ALAS owner/current path | Current capability | Research interpretation and gap |
|---|---|---|
| `crates/alas-stab/src/trim.rs`: `static_margin` | Two low-speed VLM alpha probes around the configured autobalance speed; returns `-dCm/dCL` and can be NaN for a degenerate probe | Good baseline slope estimate. It is one condition, not a CG/configuration envelope, and does not report control-surface authority, force, rate, or actuator limits. |
| `crates/alas-stab/src/trim.rs`: `neutral_point` | VLM wing/tail slope estimate with tail-efficiency and optional fuselage Munk/Multhopp contribution | Preserve the neutral-point provenance and add reference/datum/uncertainty. Validate against an independent AVL/DATCOM/external deck before making it a hard optimizer gate. |
| `crates/alas-stab/src/trim.rs`: `stability_and_trim` | Actual Mach/altitude two-probe solve for target `C_L` and `C_m=0`, with alpha and stabilizer-incidence unknowns; explicit statuses | This is the right trim abstraction. Extend it to named cases, elevator limits, trim drag, control force/rate, flaps/gear/power, forward/aft CG, and `not_evaluated` evidence. |
| `crates/alas-stab/src/static_stability.rs` | Closed-form DATCOM/downwash/tube-and-wing correlations for `C_L_alpha`, `C_m_alpha`, `C_n_beta`, static margin, and neutral point | Useful cross-check, not an authority result. The current reference-origin/CG convention deserves a dedicated regression test before using the correlation as a hard constraint. |
| `crates/alas-stab/src/dynamics.rs` and `modes.rs` | Six-solve VLM derivative sweep and closed-form approximations for phugoid, short period, roll subsidence, Dutch roll, and spiral | Report these as bare-airframe diagnostics. The current mode input is a subset of derivatives and does not include a full control-augmented state-space model, actuator nonlinearities, delays, or pilot task. |
| `crates/alas-perf/src/performance/constraints.rs` | `tw_oei_climb_constraint` and FAR-25-style OEI climb-gradient cases | Keep as OEI performance. It does not balance asymmetric yaw moments or establish VMC/VMCL. |
| `crates/alas-perf/src/performance/speeds.rs` | `VMC` is currently an empirical factor of takeoff stall speed; computes V1/VR/V2/Vapp/Vtd and stall speeds | Keep the empirical speed as a transparent sizing estimate. Add a separately named physical `Vmc` authority solve when rudder/engine geometry and failure states exist. |
| `crates/alas-perf/src/performance/envelope.rs` | `VnDiagramData` with speed axis, stall boundaries, limit/ultimate load factors, `Vs`, `Va`, `Vc`, `Vd`, and operating cruise | Good speed/load projection. Add configuration, CG, control-authority, gust, propulsion-failure, and evidence coverage dimensions rather than implying one diagram proves the full envelope. |
| `crates/alas-config/src/control_surfaces.rs` | Chord fractions and span start/end for flaps, aileron, elevator, rudder, spoiler, and slat | Geometry only. Add a separate limits/effectiveness model for deflection, rate, acceleration, hinge moment/force, delay, saturation, and failure/degraded modes. |
| `crates/alas-aero/src/avl.rs` | AVL derivative structure includes first-order, rate, control, design, neutral-point, and spiral-related fields | Strong candidate for independent derivative validation and control-authority metrics. Preserve the source deck, reference axes, control definitions, and convergence evidence. |
| `crates/alas-pipeline/src/dual_solver.rs` | Static margin can currently reject a candidate against `min_physical_static_margin` | Make the rejection policy-aware: physical floor versus preferred trim margin, condition/CG coverage, convergence, uncertainty, and explicit `NotEvaluated`/`OutOfDomain` statuses. |
| `crates/alas-pipeline/src/flowunsteady.rs` | Control-surface geometry is exported, while command/deflection is currently not applied to the geometry in the flow-unsteady request | Do not infer control authority from geometry export alone. Add a command/deflection contract when that solver is used for control cases. |

### 5.1 Recommended next implementation slices

Without changing source as part of this research task, the next slices should be:

1. Introduce a case descriptor and evidence/residual schema at the analysis boundary, reusing the requirement document's `value/unit/load case/policy/evidence` convention.
2. Refactor static-margin and trim outputs to include the complete case descriptor, reference convention, model fidelity, convergence, and uncertainty.
3. Add surface limits and control derivatives to the aero/control boundary, initially as explicit assumptions if no high-fidelity data exists.
4. Implement a physical OEI moment/authority solve separately from OEI climb performance and empirical VMC.
5. Add a full small-disturbance state-space path with control derivatives, actuator models, and failure/degraded control modes; retain the current closed-form modes as a cross-check.
6. Add independent AVL/DATCOM/external-deck regression cases and finite-difference step/convergence checks.
7. Add gust and flexible response only after the preceding evidence is stable; keep them as finalist screening metrics at first.

## 6. Mapping to `docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md`

The requirements document already provides the correct contract: every requirement carries a value, unit, load case, policy, and evidence/status; hard minimums, soft targets, objectives, and diagnostics are distinct; omitted/not-yet-evaluated stages remain explicit. Apply that structure to stability/control as follows.

| Requirements document concept | Stability/control mapping | Recommended evidence |
|---|---|---|
| Design payload, maximum payload, MTOW, MLW | Separate `Trim`, `StaticStability`, `ControlAuthority`, and `VnEnvelope` cases at each mass/CG state | Mass properties, CG calculation, configuration, solver fidelity, residuals |
| Cruise Mach, MMO/VMO, maximum operating altitude | `CruiseTrim`, `HighSpeedTrim`, static longitudinal, dynamic-mode, and out-of-trim cases | alpha/control setting, `C_m_alpha`, SM/NP, mode metrics, control margin |
| ICA/TTC and OEI net ceiling | `OeiSecondSegment` and OEI mission-performance cases | excess thrust, climb gradient, ceiling, speed, engine state; explicitly separate from yaw control |
| TOFL/LDN/Vapp | `TakeoffRotation`, `ApproachTrim`, `LandingTrim`, `VmcGround`, `VmcApproach`, and crosswind cases | rotation authority, elevator margin, rudder authority, field-performance link, configuration |
| Physical static-margin floor | A hard, policy-controlled `StaticStability` residual only after reference/model validity | NP/CG/MAC definition, derivative source, condition grid, uncertainty, convergence |
| Preferred trim margin | Soft `StaticStability`/`Trim` target, not a universal pass/fail rule | SM interval, trim drag/force, authority margin, preference source |
| CG envelope | Forward/aft and fuel-transfer cases across every relevant configuration and failure state | CG datum, mass breakdown, min/max metric envelope, coverage status |
| Ultimate load factor, dive speed, V-n diagram | `VnEnvelope`, maneuver, discrete gust, and later structural/aeroelastic cases | limit/ultimate factor, speed, gust model, structural model/fidelity |
| Engine-out feasibility | Two independent requirement records: OEI performance and OEI controllability | climb gradient/excess thrust versus yaw/roll/heading/force/deflection authority |
| Hard/soft/objective/diagnostic policy | Same policy on each residual; criteria source is part of evidence | `ConstraintResidual` with direction, scale, source, fidelity, uncertainty, status |
| Candidate failure and residual taxonomy | Preserve non-convergence, missing stage, out-of-domain, insufficient authority, and not evaluated | No silent omission; report the first failed or unverified stage and its case id |

This gives the pipeline a clean distinction between “the aircraft violates a requirement” and “the current fidelity cannot evaluate the requirement.” Both should be visible to optimization and to the final report, but they should not be collapsed into the same numeric score.

## 7. Validation hierarchy

Use the following hierarchy for stability/control evidence. Advancement to a higher level should not erase lower-level evidence; it should add an independent check.

| Level | Evidence | Typical ALAS use | Exit condition |
|---|---|---|---|
| V0 | Units, signs, reference axes, datum, MAC, CG, inertia positivity, finite values, analytic limiting cases | Every result | Deterministic checks and named reference convention pass |
| V1 | Closed-form/DATCOM/tail-volume correlations versus VLM; finite-difference step and mesh/solver convergence | Early static/trim screening | Trends agree and differences are bounded/documented |
| V2 | Independent AVL, Digital DATCOM, external aero deck, or another solver; independent control-surface definitions | Derivatives, NP, control power, modes | Independent result and residual explanation stored |
| V3 | Nonlinear six-degree-of-freedom time history with actuator position/rate/force limits, thrust transients, and failures | OEI, rotation, recovery, command response, saturation | Required task response is achieved without unmodeled saturation |
| V4 | Wind-tunnel, benchmark-aircraft, or flight-data comparison; output-error parameter estimation | Derivatives, modes, uncertainty, model bias | Identified model and data coverage are traceable; uncertainty reported |
| V5 | Pilot-in-loop simulation, handling-quality rating, flight test, or certification evidence | Handling qualities, PIO, operational acceptance | Task, pilot, control mode, environment, and rating protocol recorded |

NASA RP-1168 [S8] is the main reference for V4-style parameter estimation and maneuver/envelope planning. NASA TM-2013-218056 [S9] supports V4 uncertainty budgeting for sensors, mass properties, and geometry. RTO-TR-029 [S5] supports the V3-to-V5 development process. FAA/EASA documents [S1-S3] define the certification-shaped cases but do not make a low-fidelity ALAS result certification evidence.

Each validation record should contain: source model and version, input deck, geometry and mass hash, case id, solver settings, convergence, comparison metric, discrepancy, uncertainty, and disposition. A derivative comparison should distinguish sign error, reference-axis error, model-form difference, and numerical noise.

## 8. Uncertainty and residual taxonomy

### 8.1 Uncertainty sources

| Class | Examples | Recommended representation |
|---|---|---|
| Geometry/model-form epistemic | wing/tail interference, downwash, fuselage moment, stall/separation, propeller slipstream, control-surface nonlinear effectiveness | named model variant, calibration factor, interval or scenario set |
| Mass/CG/inertia epistemic | payload distribution, fuel CG, equipment growth, inertia correlation, reference datum | min/nominal/max or Monte Carlo samples; attach source and confidence |
| Propulsion epistemic | thrust lapse, windmilling/feathering, spool/transient, asymmetric installation drag | engine-map version, failure scenario, bounded thrust/drag model |
| Atmosphere/operational aleatory | density, wind, gust, turbulence, crosswind, flight-path variation | discrete design cases plus distributions for robust screening |
| Actuator/control-system epistemic | position/rate/acceleration limits, delay, backlash, saturation, sensor bias/filtering, control allocation | explicit limit set and uncertain delay/effectiveness interval |
| Structural/flexible epistemic | stiffness, damping, elastic mode shape, aeroelastic derivative, load path | rigid/flexible model tag; modal uncertainty and validation level |
| Numerical | VLM mesh, panel singularity, finite-difference step, nonlinear solver tolerance, ill-conditioned Jacobian | convergence sweep, condition number, repeated solve, status |
| Requirement/criterion | wrong aircraft class, obsolete criterion, missing policy, draft versus adopted rule | criterion id, issuing authority, edition/date, applicability status |

The NASA dynamic-modeling study [S9] is direct evidence that sensor measurements, mass properties, and geometry errors can materially change identified dynamics. Do not report more significant figures than the dominant uncertainty supports.

### 8.2 Residual and failure classes

- `SchemaInvalid`: missing case, unit, datum, configuration, or requirement policy.
- `NonFinite`: NaN or infinity in geometry, mass, derivative, solver output, or residual.
- `DegenerateProbe`: alpha/control perturbation did not identify a slope.
- `NonConverged`: trim, VLM, nonlinear time history, or eigenproblem did not meet its tolerance.
- `ReferenceMismatch`: inconsistent axes, moment reference, CG, MAC, sign, or dimensionalization.
- `OutOfDomain`: Mach, angle of attack, Reynolds number, flap state, control deflection, or failure state outside the model's evidence range.
- `InsufficientAuthority`: required moment/force/rate exceeds available control or actuator capability.
- `UnstableOrUnacceptable`: a valid result violates the selected criterion, such as a static, dynamic, OEI, gust, or task requirement.
- `CoverageGap`: no result for a required weight/CG/configuration/speed/altitude/failure combination.
- `MissingModel`: the requirement needs a model not yet present, such as physical VMC, actuator dynamics, flexible gust response, or pilot task.
- `CriterionUnresolved`: a result exists, but no applicable threshold has been selected or the source is citation-only.

Residual consumers should preserve both the numeric residual and its status. A `MissingModel` or `NonConverged` result can be assigned a conservative optimization penalty, but it must remain distinguishable from a physical design failure in reports.

## 9. Optimization and acceptance policy

The following tiering is recommended for the aircraft-design funnel:

| Tier | Content | Default policy |
|---|---|---|
| T0 | Geometry, axes, mass/CG/inertia, finite-state and unit validity | Hard |
| T1 | Nominal trim at design payload/CG/configuration | Hard after valid convergence |
| T2 | Static margin/NP and trim across forward/aft CG and key configurations | Physical floor hard by requirement; preferred margin soft |
| T3 | Elevator/aileron/rudder authority, rotation/recovery, OEI controllability | Hard for architecture/certification-intent cases; diagnostic while controls are provisional |
| T4 | Derivatives, modes, actuator-limited response, failures, handling-quality proxies | Soft/diagnostic until independently validated; hard only by explicit mission or certification policy |
| T5 | Discrete/continuous gust, flexible/aeroservoelastic response, pilot-in-loop evidence | Finalist screen or certification evidence; not an early universal gate |

For each optimization generation, report:

1. the set of cases evaluated and coverage gaps;
2. hard violations with normalized residual and uncertainty-adjusted value;
3. soft residuals and objective contributions;
4. evidence status and fidelity for every metric; and
5. the reason a candidate was rejected or retained.

When a static-margin floor is used, define whether it is a bare-airframe physical floor, a preferred handling-quality target, or an augmented-control-law requirement. These are different requirements. If a control law is co-designed, include actuator and sensor limits in the same evaluation; an unconstrained ideal controller can make an infeasible airframe appear optimal. This is the central result of the optimization references [S13, S14, S16].

## 10. Unresolved and citation-only sources

The following sources informed the scope but were not added to the local PDF archive, either because access or rights were not suitable or because the source is a current/draft metadata reference rather than a needed local artifact:

- **MIL-STD-1797B/1797 family, DLA metadata.** The full 1797B document is identified by DLA as distribution restricted. Use public NASA's 2020 review [S7] and the local MIL-F-8785C [S4] for research. Do not fetch or redistribute a restricted copy. [DLA metadata](https://quicksearch.dla.mil/qaDocDetails.aspx?ident_number=70037).
- **AGARD-279, Handling Qualities of Unstable Highly Augmented Aircraft.** Relevant to relaxed/static-augmented stability, but not archived here because an authoritative, clearly licensed PDF was not established. Treat it as a citation lead, not implementation evidence.
- **Cambridge, “Design criteria for conceptual sizing of primary flight controls,”** DOI [10.1017/S0001924000000464](https://doi.org/10.1017/S0001924000000464). The abstract is directly relevant to low-speed/emergency control sizing, double-engine-failure rudder power, Dutch-roll fin sizing, and pilot-in-loop validation, but the full text was not openly archived here.
- **Cambridge, “Generic stability and control for aerospace flight vehicle conceptual design,”** DOI [10.1017/S000192400000227X](https://doi.org/10.1017/S000192400000227X). Citation-only background for generic conceptual stability/control methods.
- **FAA AC 25-7D Change 2 draft.** A draft/open-comment document was not treated as adopted certification guidance. Use the archived AC 25-7D and current applicable authority material for requirements.

These sources should be promoted from citation-only only after checking applicability, edition, licensing, and whether the added detail changes an ALAS policy.

## 11. Reference ledger

All local PDFs were checked for a `%PDF-` signature and readable first-page text with the bundled `pypdf` runtime. Page counts are from the local files. SHA-256 is over the exact archived file. Paths are relative to this document.

| ID | Metadata and online provenance | Local archive | Pages | SHA-256 | Rights/provenance note |
|---|---|---|---:|---|---|
| S1 | FAA, *Flight Test Guide for Certification of Transport Category Airplanes*, AC 25-7D, issued 2018-05-04, Change 1; [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D.pdf) | [faa-ac-25-7d.pdf](../../bib/stability-control/faa-ac-25-7d.pdf) | 481 | `47dcb52c8ea7b74e1a2a58c5f9ebae38b5798f863fdfb4c93ca6a7fc74b9defe` | FAA advisory guidance; not itself mandatory/regulatory. |
| S2 | FAA, *Dynamic Gust Loads*, AC 25.341-1, 2014-12-12; [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25_341-1.pdf) | [faa-ac-25-341-1.pdf](../../bib/stability-control/faa-ac-25-341-1.pdf) | 24 | `eb390b7a63aa12f1b648863ee14a26a3a9e886c53b070249a27a1e36fe26db6d` | FAA advisory guidance; local research archive. |
| S3 | EASA, *Certification Specifications and Acceptable Means of Compliance for Large Aeroplanes CS-25 Amendment 28*, Annex to ED Decision 2023/021/R; [official page](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28) | [easa-cs-25-amendment-28.pdf](../../bib/stability-control/easa-cs-25-amendment-28.pdf) | 1515 | `32f1a9acf26e8ceceb291d206bf07a7071f7f59f40f10c096bb17f34c6a58007` | Official regulatory publication; verify latest amendment/correction before compliance use. |
| S4 | US DoD, *Flying Qualities of Piloted Airplanes*, MIL-F-8785C, 1980-11-05, inactive for new design; [DLA metadata](https://quicksearch.dla.mil/qsDocDetails.aspx?ident_number=7180); local copy from [Abbott Aerospace mirror](https://www.abbottaerospace.com/downloads/mil-f-8785c-flying-qualities-of-piloted-airplanes/) | [mil-f-8785c.pdf](../../bib/stability-control/mil-f-8785c.pdf) | 95 | `51c4e51c3650ab7a5df9ff81e98a075947fc6d2dce97561ce96ffeda7791af8d` | Historical specification, originally distribution A; mirror redistribution rights not independently audited; local research only. |
| S5 | NATO RTO, *Flight Control Design - Best Practices*, RTO-TR-029 / AC/323(SCI)TP/23, 2000; [source page](https://www.abbottaerospace.com/downloads/rto-tr-029-flight-control-design-best-practices/) | [rto-tr-029-flight-control-design-best-practices.pdf](../../bib/stability-control/rto-tr-029-flight-control-design-best-practices.pdf) | 217 | `83c7021a7ac49c79cf72bbd7106d259cef163be8a5fe9489a107b12cc12e3e31` | Publicly accessible report mirror; redistribution rights not independently audited; local research only. |
| S6 | L. W. Taylor Jr. and K. W. Iliff, *Recent Research Directed toward the Prediction of Lateral-Directional Handling Qualities*, AGARD-531 / NASA-TM-X-59621, 1966; [NASA NTRS record](https://ntrs.nasa.gov/citations/19670013913) | [agard-531-lateral-directional-handling-qualities.pdf](../../bib/stability-control/agard-531-lateral-directional-handling-qualities.pdf) | 27 | `313b89a085bb837eb073ae06f77f4663cc9e6f8299ee2a977401e564424c35c5` | NTRS distribution marked public, but copyright determination is `OTHER`; local research only. |
| S7 | NASA, *NASA's Flying Qualities Research Contributions to MIL-STD-1797C*, NASA/CR-2020-5002350, 2020-05-18; [NTRS record](https://ntrs.nasa.gov/citations/20205002350) | [nasa-cr-2020-5002350-flying-qualities-mil-std-1797c.pdf](../../bib/stability-control/nasa-cr-2020-5002350-flying-qualities-mil-std-1797c.pdf) | 81 | `d395085aec9a9e8da6e85aeeaa7636908ee793bc379bdd1cc090bbbf6ea95e18` | NTRS metadata: public, public-use-permitted; check third-party material before redistribution. |
| S8 | R. E. Maine and K. W. Iliff, *Application of Parameter Estimation to Aircraft Stability and Control: The Output-Error Approach*, NASA-RP-1168, 1986; [NTRS record](https://ntrs.nasa.gov/citations/19870020066) | [nasa-rp-1168-parameter-estimation.pdf](../../bib/stability-control/nasa-rp-1168-parameter-estimation.pdf) | 177 | `381cc16d9ec034adcd376d61e7a998ae291ec79287a75e99438907836b3f530e` | NTRS metadata: public, government-public-use-permitted. |
| S9 | J. A. Grauer and E. A. Morelli, *Dependence of Dynamic Modeling Accuracy on Sensor Measurements, Mass Properties, and Aircraft Geometry*, NASA/TM-2013-218056, 2013; [NTRS record](https://ntrs.nasa.gov/citations/20140003885) | [nasa-tm-2013-218056-dynamic-modeling-accuracy.pdf](../../bib/stability-control/nasa-tm-2013-218056-dynamic-modeling-accuracy.pdf) | 45 | `772e1bb650110a2091c5a09533467d88c1f72eacc2e21ca806c5176df522e5cb` | NTRS metadata: public, government-public-use-permitted. |
| S10 | S. M. Swanson, *A Computer Module Used to Calculate the Horizontal Control Surface Size of a Conceptual Aircraft Design*, NASA-CR-186872, 1990; [NTRS record](https://ntrs.nasa.gov/citations/19900017199) | [nasa-cr-186872-horizontal-control-surface-sizing.pdf](../../bib/stability-control/nasa-cr-186872-horizontal-control-surface-sizing.pdf) | 122 | `2a176a7a200ed009a44542e97b9d07be5602335ca000eb221134ca1245c05564` | NTRS metadata: public, government-public-use-permitted. |
| S11 | H. P. Mooney et al., *AFC-Enabled Vertical Tail System Integration Study*, NASA/CR-2014-218168, 2014; [NTRS record](https://ntrs.nasa.gov/citations/20140003900) | [nasa-cr-2014-218168-vertical-tail-integration.pdf](../../bib/stability-control/nasa-cr-2014-218168-vertical-tail-integration.pdf) | 66 | `7d6ba2e4bd607e9954440d7d3943878d7ff5a596bf947cf695d0b1cfcdf942d6` | NTRS metadata: public, public-use-permitted. |
| S12 | C. J. Funk, B. Perry III, W. A. Silva, and B. Newman, *A Summary of Revisions Applied to a Turbulence Response Analysis Method for Flexible Aircraft Configurations*, AIAA 2014-0600, 2014; [NTRS record](https://ntrs.nasa.gov/citations/20140011901) | [nasa-2014-0600-turbulence-response-method.pdf](../../bib/stability-control/nasa-2014-0600-turbulence-response-method.pdf) | 19 | `86795de50c92061e19e62356d3a55ab8e90611b91ab0fb17e8c09a51ef204ab9` | NTRS metadata: public; conference material may contain third-party rights. |
| S13 | J. Welstead and G. L. Crouse Jr., *Conceptual Design Optimization of an Augmented Stability Aircraft Incorporating Dynamic Response and Actuator Constraints*, AIAA 2014-0187, 2014; [NTRS record](https://ntrs.nasa.gov/citations/20140011926) | [nasa-2014-0187-augmented-stability-actuator-constraints.pdf](../../bib/stability-control/nasa-2014-0187-augmented-stability-actuator-constraints.pdf) | 22 | `e23bd95955206e2e42495210ce1a9c06845e693bb5d7968b7040cdd35bce8fd3` | NTRS metadata: public; conference material may contain third-party rights. |
| S14 | J. Welstead, *Conceptual Design Optimization of an Augmented Stability Aircraft Incorporating Dynamic Response Performance Constraints*, NF1676L-20607, 2014 dissertation record; [NTRS record](https://ntrs.nasa.gov/citations/20160009381) | [nasa-20160009381-dynamic-response-performance-constraints.pdf](../../bib/stability-control/nasa-20160009381-dynamic-response-performance-constraints.pdf) | 217 | `945697363c7dadc5137a908f981611d43fa62a3cb568a58163c4e369aa6492bd` | NTRS metadata: public; local research archive. |
| S15 | I. L. Ashkenas and D. H. Klyde, *Tailless Aircraft Performance Improvements with Relaxed Static Stability*, NASA-CR-181806, 1989; [NTRS record](https://ntrs.nasa.gov/citations/19890011628) | [nasa-cr-181806-relaxed-static-stability.pdf](../../bib/stability-control/nasa-cr-181806-relaxed-static-stability.pdf) | 140 | `6497546f3694083ec7dd1a68f06c75cc66fd09ba36f78cfa5f217cab80d5b892` | NTRS metadata: public; local research archive. |
| S16 | R. Bortins and J. A. Sorensen, *ACSYNT Inner Loop Flight Control Design Study*, NASA-CR-196316, 1993; [NTRS record](https://ntrs.nasa.gov/citations/19950004810) | [nasa-cr-196316-acsynt-inner-loop.pdf](../../bib/stability-control/nasa-cr-196316-acsynt-inner-loop.pdf) | 200 | `0cff1d6e40a5219136d82cc93bea0a60c00381fa41d1ee5fadd615a3662323b6` | NTRS metadata: public; local research archive. |
| S17 | FOWL Enterprises, *LCX: Proposal for a Low-Cost Commercial Transport*, NASA-CR-197186, 1994; [NTRS record](https://ntrs.nasa.gov/citations/19950006232) | [nasa-cr-197186-lcx-control-sizing.pdf](../../bib/stability-control/nasa-cr-197186-lcx-control-sizing.pdf) | 107 | `064fd9119d522684a51c77111cccabc2a2fa79baf35336369773e70fcd0109b2` | NTRS metadata: public; local research archive. |
| S18 | G. E. Cooper and R. P. Harper Jr., *The Use of Pilot Rating in the Evaluation of Aircraft Handling Qualities*, NASA-TN-D-5153, 1969; [NTRS record](https://ntrs.nasa.gov/citations/19690013177) | [nasa-tn-d-5153-cooper-harper.pdf](../../bib/stability-control/nasa-tn-d-5153-cooper-harper.pdf) | 60 | `6f7e9dd44f4d19d95c4c3d2052f148aedc766ad099401193ba4f27e74378a26a` | NTRS metadata: public, government-public-use-permitted. |
| S19 | B. S. de Mattos and N. R. Secco, *An Airplane Calculator Featuring a High-Fidelity Methodology for Tailplane Sizing*, JATM 5(4), 2013, DOI [10.5028/jatm.v5i4.254](https://doi.org/10.5028/jatm.v5i4.254); [article page](https://jatm.com.br/jatm/article/view/254) | [jatm-tailplane-sizing-2013.pdf](../../bib/stability-control/jatm-tailplane-sizing-2013.pdf) | 16 | `5e25d65e4c970058fc961840c1204a145ab10e3440ef6bd05fb1ad336de25c7e` | Journal marks article CC BY 4.0. |
| S20 | S. Karatoprak and S. Ozgen, *Sizing and Optimization of the Horizontal Tail of a Jet Trainer*, EUCASS 2019-0335, 2019; [PDF source](https://www.eucass.eu/doi/EUCASS2019-0335.pdf) | [eucass-2019-0335-horizontal-tail-sizing.pdf](../../bib/stability-control/eucass-2019-0335-horizontal-tail-sizing.pdf) | 15 | `3bf4f5a939a890c8f302ff1d584d5265b6afe7ac19140f2061b50ac5a415a374` | Copyright by authors, published with EUCASS permission; local research only. |

## 12. Practical acceptance checklist

Before treating a stability/control result as an optimization constraint or report conclusion, confirm:

- [ ] the load case has mass, CG, inertia, configuration, speed/Mach, altitude, atmosphere, thrust, fuel, payload, and failure state;
- [ ] the reference datum, axes, MAC, sign convention, and derivative units are recorded;
- [ ] trim residuals and solver convergence are finite and within tolerance;
- [ ] static margin and neutral point are tied to the same CG and derivative reference;
- [ ] control authority includes the required moment, available moment, position/rate/force limits, and saturation status;
- [ ] OEI climb/performance and OEI yaw/roll controllability are separate records;
- [ ] dynamic-mode results identify the derivative set, state vector, dimensionalization, and whether controls/actuators are included;
- [ ] gust results identify discrete/continuous model, gradient/spectrum, time-domain/frequency-domain method, and structural fidelity;
- [ ] uncertainty, model domain, validation level, and coverage gaps are present; and
- [ ] every hard/soft/objective/diagnostic decision names its requirement or criterion source.
