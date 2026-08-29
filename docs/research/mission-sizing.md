# Mission analysis and preliminary sizing research for ALAS

Status: completed research deliverable

Date: 2026-08-26

Scope: requirements-first conceptual aircraft design, mission definition and reserves, payload-range, fuel fraction, Class-I/Class-II mass estimation, wing-loading/thrust-loading constraints, coupled sizing loops, architecture seed generation, and uncertainty-aware screening.

This report turns the evidence in the locally curated PDFs into an implementable contract for ALAS. It is deliberately a conceptual-design specification, not a certification method. Internal equations should use SI units and explicit load cases; the user-facing units in the existing requirements document can remain NM, ft, KCAS, kg, and similar aviation units.

No existing source file was modified for this deliverable. The PDFs listed in the source ledger were downloaded before this report was written and remain under `bib/mission-sizing/`.

## Executive findings

1. The design point must be explicit. A practical default for a transport-like ALAS concept is the requested payload at the requested trip range, with a separately named reserve policy and no reserve range credit. The payload-range envelope then adds the maximum-fuel and zero-payload cases.
2. Use a two-level mass method. A fast Class-I closure provides a feasible seed; a component-level Class-II buildup replaces it as soon as geometry and architecture are available. No single regression should be treated as truth: public comparison work shows similar total-weight performance can hide much larger component errors.
3. Size wing area and installed thrust from a constraint matrix, not from one historical ratio. The matrix must contain landing/stall, cruise, climb, OEI climb, takeoff field length, approach, fuel-volume, span, and any role-specific constraints, each tied to a load case and atmosphere.
4. Mission, propulsion, aerodynamics, and mass are a coupled solve. The inner loop closes aircraft mass and fuel; an outer loop changes continuous design variables and enumerates discrete architecture. Off-design conditions must be checked after the nominal design point closes, and failures must remain typed residuals.
5. Reserve fuel is a policy object, not a hidden percentage. Operational rules distinguish taxi, trip, contingency, alternate, final reserve, additional, extra, and discretionary fuel/energy. A conceptual study may use a named 5% convention, but ALAS must record it as a study assumption rather than imply certification compliance.
6. Robustness belongs in the screening contract. Sample payload, Mach, atmosphere, SFC, aerodynamic coefficients, weight-model factors, and technology assumptions; require a quantile or probability of satisfying hard requirements. Separate input uncertainty from model-form uncertainty.

## Evidence at a glance

| Design question | Evidence and implication for ALAS |
| --- | --- |
| What is a transport design point? | Simpson's transport notes separate takeoff gross weight, initial/final cruise weight, trip fuel, and reserve fuel, then show that payload fraction decreases with range and that the full-payload design-range point should be selected deliberately [S02, pp. 6-25]. |
| How should a fast mass loop work? | Global Sentry estimates dimensions and power/wing loading from constraints, repeats the spreadsheet until estimated and resulting weights agree, and uses a fuel-fraction/Breguet range calculation [S03, pp. 19-24, 55]. NASA's FLOPS documentation makes the fixed-range loop explicit: estimate gross weight, build maximum zero-fuel weight, compute available fuel, and repeat until available and required fuel agree [S09, pp. 10-17]. |
| What is the Class-I/Class-II split? | Horvath and Wells summarize Raymer's three levels and Roskam's first-process/Class-I/Class-II workflow, including the benefit of averaging applicable weight equations and the risk of ambiguity and inconsistent component groupings [S12, pp. 2-5, 18]. |
| What changes for novel configurations? | The BWB methodology uses architecture-specific cabin and pressurized-structure models instead of a tube-fuselage correlation [S05, pp. 5-23]. N+3 likewise couples first-principles and empirical models and warns that its tube-wing models do not transfer directly to non-traditional configurations [S06, pp. 114-118]. |
| How should the mission be solved? | LEAPS uses a modular mission model and an energy-based low-order approach; N+3 integrates trajectory states, sizes the engine at cruise, checks off-design conditions, and iterates weight and mission fuel to a stable design [S11, pp. 2-8; S06, pp. 112-116, 126-132]. |
| How should the framework expose stages? | LAMBDA separates requirements, sizing, geometry, aerodynamics, engine, performance, weight, and optimization modules, with a convergence loop and replaceable external interfaces [S01, pp. 7-9, 17-18, 26]. LEAPS and OpenMDAO support a typed, modular, multi-fidelity workflow [S13, pp. 5-9; S07, pp. 1-9]. |
| How should uncertainty affect design? | NASA's design-under-uncertainty study treats input and model-form uncertainty inside optimization, calculates confidence bounds, and reports a heavier robust design with a substantial computational cost increase [S14, pp. 1-6, 13-15]. |

## 1. Requirements, mission definition, and reserves

### 1.1 Requirements-first interpretation

The existing ALAS requirements architecture already makes the right distinction between a value/unit, a load case, a policy, and evidence/status. The sizing contract should preserve that distinction all the way into the solver:

```text
Requirement = {
    name,
    value,
    unit,
    load_case,
    direction,       # minimum, maximum, equality, or target
    policy,          # hard, soft, objective, diagnostic
    evidence_status,
    source_or_assumption
}
```

The mission brief should state at least:

- trip route distance and the definition of design range;
- design payload and maximum payload, with passenger and baggage mass basis;
- departure and destination conditions, route/weather assumptions, and airports;
- cruise Mach/altitude and operating limits;
- climb, OEI, takeoff, landing, and approach requirements;
- reserve policy and whether any segment is allowed to receive range credit;
- propulsion/energy architecture and fuel or battery limits;
- load cases used for mass, CG, stability, and airport checks.

Recommended ALAS convention: `design_range` is the distance from takeoff to the intended destination in the trip mission. Alternate, final-reserve, and additional-fuel segments occur after the destination and have `range_credit = false`. If a user imports a legacy range that includes a diversion or hold, the adapter must retain the original definition in `range_definition` rather than silently reinterpret it.

This follows the transport evidence: the 1973 notes treat climb, cruise burn, and reserve as distinct weight components, and the short- and long-range payload diagrams make the full-payload design point a design decision rather than a consequence of a single maximum-range number [S02, pp. 6-25]. The NASA Short-Haul study similarly derives a 600 NM design mission, payload, cruise speed, balanced field length, and reserve rule as one requirements set [S10, pp. 36-38].

### 1.2 Reserve policy as an explicit object

For a fuel-powered aircraft, the generic conceptual requirement is:

```text
m_usable_required =
    m_taxi
  + m_trip
  + m_contingency
  + m_destination_alternate
  + m_final_reserve
  + m_additional
  + m_extra
```

For an energy-limited electric or hybrid aircraft the same structure applies to usable electrical or chemical energy, but the battery mass, state of charge, depth of discharge, thermal limits, and reserve state must be solved together with the aircraft mass. NASA's hybrid-electric regional study is a concrete example: the reserve includes both fuel and battery, the energy-storage weight is updated from integrated mission energy, and the outer wing/thrust sizing is constrained by range, field length, OEI climb, approach speed, and fuel volume [S08, pp. 1-3, 8-11]. Discretionary fuel can be represented as an optional operator choice, but should not be silently included in a certification-like design point.

The current EASA Air Operations rules are a useful policy model, not a universal ALAS requirement. They require an operator fuel/energy planning policy based on aircraft-specific or manufacturer data and operating conditions including anticipated masses, weather, routing, and delays. The pre-flight calculation distinguishes taxi, trip, contingency, destination alternate, final reserve, additional, extra, and discretionary fuel/energy. The basic Class-A guidance includes climb, cruise, descent, approach, alternate, and a contingency calculation, with final reserve based on holding conditions [W01]. The current US eCFR examples are also role-specific: 14 CFR 121.639 lists destination, the most distant required alternate, and 45 minutes at normal cruise consumption for domestic operations [W02]; 14 CFR 121.645 gives a different turbine flag/supplemental rule outside the contiguous United States, including 10% of destination-flight time, required alternate, and 30 minutes at holding speed [W03].

The conceptual `ReservePolicy` should therefore contain:

```text
ReservePolicy {
    name: String,
    applicability: String,          # study, operating rule, certification basis
    contingency: SegmentRule,
    destination_alternate: SegmentRule,
    final_reserve: SegmentRule,
    additional: SegmentRule,
    extra: SegmentRule,
    discretionary: SegmentRule,
    range_credit: bool,             # false for reserve segments by default
    no_range_credit_reason: String,
    source: EvidenceRef
}

SegmentRule {
    enabled: bool,
    mode: percentage | time | distance | mission_segment | statistical,
    value: number,
    reference: trip_fuel | holding_speed | route | arrival_mass | custom,
    load_case: LoadCase,
    atmosphere: AtmosphereCase,
    notes: String
}
```

The NASA examples illustrate why the segment list matters. The HSCT concept included missed approach, climb to reserve cruise, a 250 nmi subsonic reserve cruise, a 30-minute hold, descent, and 5% trip-fuel reserve; the reserve did not receive range credit [S04, pp. 15-18]. The Short-Haul study used 5% trip fuel, an 87 NM diversion, and 45 minutes of continued cruise in its reserve convention [S10, pp. 37-38]. N+3 reports a 5% reserve convention for its D8.5 mission and a separate 5% reserve in the H3.2 parameters [S06, pp. 58, 128-129]. Those are traceable study policies, not interchangeable regulatory defaults.

### 1.3 Mission graph and load cases

The mission should be a directed graph of segments rather than a single range scalar:

```text
taxi -> takeoff -> climb -> cruise/step-climb -> descent -> approach -> landing
                                      |
                                      +-> missed approach -> alternate -> hold -> landing
```

Each segment carries start/end conditions or an integration rule, a distance/time objective, an atmosphere case, a propulsion mode, and a load case. Fuel/energy consumption is integrated on the segment. A reserve segment is linked to the design mission for accounting but cannot increase the design trip range unless the brief explicitly chooses a different range convention.

Use separate mass snapshots at:

1. brake release / takeoff gross mass;
2. top of climb and start of cruise;
3. end of cruise / top of descent;
4. destination landing mass;
5. alternate or final-reserve landing mass;
6. maximum payload and maximum zero-fuel cases.

This is important for thrust lapse, wing loading, CG, stability, landing weight, and reserve calculations. The N+3 process performs trajectory integration and repeats the initial takeoff-fuel value until the specified range is achieved [S06, pp. 112-113].

## 2. Payload-range and fuel fraction

### 2.1 Breguet equations as a Class-I check

For a jet in steady cruise with constant speed and lift-to-drag ratio, define (c_T) as thrust-specific fuel consumption in inverse time and (W) as weight. The classical range relation is:

```text
R_jet = (V / c_T) * (L/D) * ln(W_i / W_f)
```

For a propeller aircraft with propulsive efficiency `eta_p` and power-specific fuel consumption `c_P` in the compatible inverse-distance units:

```text
R_prop = (eta_p / c_P) * (L/D) * ln(W_i / W_f)
```

The derivation starts from the specific range and integrates `dR = r * (-dW/W)` between initial and final cruise weight. Simpson's notes show both the short-range approximation and the exact logarithmic form, then connect the result to the payload-range diagram [S02, pp. 13-25]. The equation is a seed and cross-check; the ALAS mission solver should integrate the actual segment models when climb, descent, altitude, throttling, temperature, or off-design propulsion matter.

For segment fuel fractions define:

```text
M_i = W_out_i / W_in_i
M_trip = product(M_i) over trip segments
m_trip_fuel = m_at_trip_start * (1 - M_trip)
```

Reserve segments have their own mass ratio and are added after the trip mission. Do not combine `M_trip` and `M_reserve` if the output needs to report trip burn and reserve burn separately.

### 2.2 Payload-range corner points

At minimum, calculate these cases using the same geometry and mass model:

| Point | Payload | Fuel/energy | Meaning |
| --- | --- | --- | --- |
| A | design payload | enough for design trip plus policy reserves | primary requirements point |
| B | reduced payload as necessary | usable tank/energy capacity | maximum-fuel or structural/fuel-volume trade |
| C | zero or minimum operational payload | usable tank/energy capacity | maximum range check |
| D | maximum payload | fuel selected by the maximum-payload mission | accommodation and MZFW check |

For a fixed takeoff mass and a valid design, an algebraic payload upper bound is:

```text
m_payload,max(R) = min(
    m_structural_payload_limit,
    m0 - m_oew - m_other_fixed - m_fuel_required(R) - m_energy_storage
)
```

The result must then be intersected with cabin/cargo capacity and MZFW limits. `m_payload,max` is not the same as geometry-derived capacity: the former is a mass/range limit, the latter is a physical accommodation limit. The existing ALAS requirements document already calls out this distinction; the BWB study shows why it is especially important for non-circular cabins [S05, pp. 5-23].

### 2.3 Design-range selection

The design range should be chosen at a named payload point, normally the full design payload. Range-only optimization creates a misleading result because reducing payload can improve maximum range while violating the commercial or operational intent. The 1973 transport notes show that the payload fraction becomes strongly range-dependent and that the payload-range envelope has fuel-volume and cabin-volume boundaries [S02, pp. 20-25].

## 3. Weight estimation: Class I and Class II

### 3.1 Mass taxonomy

ALAS should select one mass taxonomy at the brief boundary and preserve it in every report. A workable transport taxonomy is:

```text
operating_empty_mass = structure + propulsion + systems + interiors + operating_items
zero_fuel_mass       = operating_empty_mass + payload
takeoff_mass         = operating_empty_mass + payload + loaded_fuel + stored_energy
landing_mass         = takeoff_mass - consumed_fuel - consumed_energy
```

Crew and operational items may be included in operating-empty mass or represented as separate line items, but not both. The taxonomy and inclusion flags must be visible in the mass statement. The 1973 notes place reserve fuel in operating empty weight for that lecture's simplified accounting [S02, p. 7]; ALAS should not adopt that non-standard convention silently. Keep all reserve fuel in the mission/fuel statement and only put unusable fuel or tank-system hardware in the appropriate empty-mass item.

### 3.2 Class-I initial closure

The Class-I estimate is a low-cost initializer. For a candidate takeoff mass `m0`, compute a first estimate from payload and fraction assumptions:

```text
m0_guess = (m_payload + m_crew + m_fixed_operating)
           / (1 - f_empty - f_trip_fuel - f_reserve_fuel - f_storage)
```

The fractions must be tagged with aircraft class, architecture, propulsion, source, and calibration status. A better Class-I version replaces the fuel fraction with the mission segment product:

```text
m_fuel_required(m0) = m0 * (1 - M_trip(m0)) + m_reserve(m0)
m0_new = m_oew_fraction(m0) * m0
         + m_payload + m_crew + m_fixed_operating
         + m_fuel_required(m0) + m_storage
```

Iterate `m0` until the relative change and the normalized closure residual are below the selected tolerance. Use relaxation when the fuel or empty-mass model is steep:

```text
m0_next = (1 - alpha) * m0_old + alpha * m0_new
```

The public Global Sentry report is a useful concrete precedent: it builds a parametric constraint chart, chooses a plausible wing/power loading, estimates fuel fraction and component weights, and repeats the spreadsheet until estimated weight equals resulting weight [S03, pp. 19-24].

### 3.3 Class-II component buildup

Once geometry and architecture exist, calculate component masses with a source-aware buildup:

```text
m_oew = sum(m_i)

m_i = K_i * product(X_ij ** b_ij) * correction_i
```

`X_ij` may include area, length, span, thickness, load, design mass, gear length, engine thrust/power, cabin volume, or other physically meaningful variables. Every component output should contain:

```text
MassItem {
    name,
    mass_kg,
    cg_m,
    inertia_kg_m2,
    method,                 # fraction, WER, geometry, FEA, external tool
    fidelity,
    applicability,
    calibration_id,
    uncertainty,
    evidence
}
```

The FLOPS weight documentation describes a bottoms-up component buildup, maximum zero-fuel weight, and a fixed-range loop that repeats mission fuel and gross weight until fuel available equals fuel required [S09, pp. 10-17]. It also shows separate wing bending, shear/control-surface, miscellaneous, and inertia-relief contributions rather than a single opaque wing fraction [S09, pp. 22-25].

Horvath and Wells compare FLOPS with Raymer and Roskam methods. Their summary is directly useful for ALAS: Raymer's process progresses from gross-weight/fraction estimates to statistical component weights and then detailed component regressions; Roskam uses an initial gross/fuel/empty estimate followed by Class-I historical fractions and Class-II detailed regression [S12, pp. 2-5]. They also show that methods which agree reasonably at total empty weight can differ materially by component; in one 737-200 example, mission-fuel error was about 6.9%, while wing and fuselage errors for the comparison set were much larger than total-weight error [S12, pp. 12-13]. Treat regressions as calibrated estimates with uncertainty, not as exact physics.

### 3.4 Closure equations and limits

Define a consistent takeoff mass statement:

```text
m0 = m_oew + m_payload + m_loaded_fuel + m_stored_energy

m_zfw = m_oew + m_payload

r_mass = (m0 - m_oew - m_payload - m_loaded_fuel - m_stored_energy) / m0
r_fuel = m_usable_capacity - m_usable_required
r_mzfw = m_mzfw_limit - m_zfw
```

Use `m_loaded_fuel = m_usable_required + m_unusable_fuel` for a fuel system, and represent battery reserve/state-of-charge limits separately from usable energy capacity. The fixed-range root is:

```text
F(m0) = m0
       - m_oew(m0, geometry, architecture)
       - m_payload
       - m_loaded_fuel(m0, mission, reserve_policy)
       - m_stored_energy(m0, mission, architecture)
       = 0
```

Also check:

```text
m_zfw <= m_mzfw_limit
m0    <= m_mtow_limit
m_loaded_fuel <= m_fuel_tank_usable_capacity
m_payload <= m_structural_payload_limit
m_payload <= m_cabin_or_cargo_capacity
```

A converged numerical loop is not a feasible design if any of these limits are violated. Record each violation separately so that the user can distinguish an oversized mission from inadequate tank volume, cabin capacity, wing, engine, or structural margin.

## 4. Wing loading and thrust-to-weight constraint analysis

### 4.1 Common relations

For a reference weight `W_ref` and wing area `S`:

```text
C_L = 2 * W / (rho * V**2 * S)
W/S = q * C_L
q   = 0.5 * rho * V**2
```

With a parabolic polar, a low-order drag estimate is:

```text
C_D = C_D0 + k * C_L**2 + C_D_config + C_D_trim + C_D_wave
D/W = q * C_D / (W/S)
```

Landing/stall gives a maximum wing loading for a chosen configuration:

```text
V_stall = sqrt(2 * W_landing / (rho * S * C_Lmax_landing))
W_landing/S <= 0.5 * rho * V_stall_limit**2 * C_Lmax_landing
V_app = k_app * V_stall
```

For steady climb at rate `ROC`:

```text
T/W >= D/W + ROC/V
```

For cruise:

```text
T/W >= D/W
```

For a propeller or electric aircraft, use excess power as the primary relation where appropriate:

```text
P_s = V * (T - D) / W
P_s >= ROC + V * dh/dx       # with the selected flight-path convention
```

Takeoff field length, balanced field, OEI second segment, go-around, and landing distance should come from the available ALAS model or a named calibrated correlation. They should not be replaced by a hidden universal coefficient. Each screen must state whether thrust means sea-level static thrust, thrust at the relevant altitude/speed, shaft power, or installed net thrust.

### 4.2 Matching diagram and load-case matrix

The matching diagram should evaluate a grid or adaptive set of `(W/S, T/W)` pairs. For each point, calculate:

| Constraint | Weight/condition | Residual convention |
| --- | --- | --- |
| landing stall/approach | MLW, landing configuration, destination atmosphere | `target_Vapp - actual_Vapp` |
| cruise drag | start/end cruise masses and altitudes | `T_available - D_required` |
| initial climb/TTC | MTOW, ISA+10, required rate or time | `ROC_actual - ROC_target` or `TTC_target - TTC_actual` |
| OEI climb/ceiling | prescribed mass, engine-out, ISA | `ROC_actual - ROC_target` |
| takeoff/balanced field | MTOW, runway and atmosphere | `TOFL_target - TOFL_actual` |
| go-around/missed approach | MLW or specified approach mass, OEI/AEO | excess-thrust residual |
| fuel volume | design payload/range mission | `capacity - required` |
| span/airport/ACN | geometry and pavement case | limit residual |

The candidate's initial geometry and propulsion are then obtained from:

```text
S_seed = W_ref / (W/S)_chosen
T_installed_seed = W_ref * (T/W)_chosen
```

`(W/S)_chosen` should be the smallest area that satisfies hard landing/approach and low-speed constraints subject to a stated objective or trade penalty. `(T/W)_chosen` should cover the governing takeoff/climb/OEI constraint with a visible margin. Global Sentry explicitly used takeoff, landing, cruise, and climb constraint analysis to select wing and power loading; its reported values are an aircraft-specific example, not ALAS defaults [S03, pp. 19-24]. LAMBDA likewise begins with a similar-aircraft guess and then switches to a Roskam-style matching diagram once geometry and aerodynamic data exist [S01, pp. 8-9].

### 4.3 Fuel-volume and geometric coupling

Wing loading and thrust loading cannot be screened in isolation from fuel volume, cabin volume, and structural mass. Increasing `S` may reduce stall and approach speed but increases wetted area, structural mass, and potentially drag. Increasing engine thrust increases propulsion mass and may change nacelle drag, fuel burn, and CG. The HSCT report shows this coupled trade among field length, usable fuel volume, approach speed, wing area, thrust, and gross weight [S04, pp. 15-20].

## 5. Coupled sizing loops

### 5.1 Recommended loop hierarchy

Use a small outer architecture enumeration around an inner continuous sizing loop:

```text
for architecture_seed in supported_architecture_catalogue:
    state = initialise_from_requirements_and_seed()

    for iteration in 1..max_sizing_iterations:
        brief = normalize_load_cases_and_units()
        class_i = estimate_class_i_mass_and_fuel(state, brief)
        constraints = evaluate_matching_diagram(class_i, state, brief)
        state = update_wing_area_thrust_and_energy_capacity(constraints)

        geometry = materialise_geometry(state, brief)
        accommodation = resolve_cabin_or_cargo(geometry, brief)
        class_ii = build_component_mass_and_cg(geometry, state, brief)
        mission = solve_trip_and_reserves(class_ii, geometry, state, brief)
        performance = evaluate_cruise_climb_descent_airport(class_ii, mission, brief)
        structure = evaluate_structural_screen_if_enabled(geometry, state, brief)

        residuals = collect_typed_residuals(...)
        state = relaxed_update_from_mission_and_mass_closure(...)

        if converged_mass_fuel_geometry_and_constraints(state):
            break

    if converged:
        evaluate_off_design_and_robust_scenarios(state, brief)
rank_by_feasibility_then_preferences_then_objective()
```

The loop is supported by several public frameworks. LAMBDA describes sizing, geometry, aero, engine, and weight feedback in a user-selected convergence loop [S01, pp. 7-9]. LEAPS uses a modular aircraft object and layered fidelity with user feedback on errors [S13, pp. 5-9]. N+3 separates an inner weight iteration from an outer optimization over airframe, propulsion, and operating variables [S06, pp. 114-116].

### 5.2 Inner mass/fuel closure

At a fixed architecture and design vector, solve the coupled residual system:

```text
R_mass(m0, S, T, ...) = 0
R_fuel(m0, S, T, ...) = m_usable_capacity - m_usable_required
R_mzfw              = m_mzfw_limit - m_zfw
R_mtow              = m_mtow_limit - m0
```

The implementation can use a bracketed scalar root for `m0` at low fidelity and a coupled nonlinear solver when battery mass, engine sizing, or geometry have strong feedback. Keep `m0`, `S`, thrust, energy storage, and any discrete architecture choice distinct; a continuous optimizer should not encode an engine count or cabin layout as an arbitrary floating-point variable.

Convergence should require all relevant changes and residuals, for example:

```text
abs(m0[k] - m0[k-1]) / max(m0[k], 1 kg) < tol_mass
abs(m_fuel[k] - m_fuel[k-1]) / max(m_fuel[k], 1 kg) < tol_fuel
abs(S[k] - S[k-1]) / max(S[k], 1 m2) < tol_area
max(abs(normalized_residuals)) < tol_residual
```

The solver must also detect a cycle, a non-finite result, a bracket failure, a geometry failure, and an external-tool failure. A failed stage returns a typed result and its evidence, not a fabricated zero fuel burn or an objective penalty with no reason.

### 5.3 Mission integration and energy height

For a point-mass low-order mission model, define energy height:

```text
h_e = h + V**2 / (2 * g)
P_s = d(h_e)/dt = V * (T - D) / W
dt  = d(h_e) / P_s
```

For a jet with fuel flow `m_dot_f`, fuel burn over a discretized segment is:

```text
dm_fuel = m_dot_f(state, throttle, altitude, Mach, ...) * dt
```

The equations are most useful as a low-order integration scaffold; the exact flight-path and sign convention must be kept with the implementation. LEAPS uses energy height and excess specific power to calculate time and fuel along climb/descent/mission segments, then iterates range and energy-storage mass to closure [S11, pp. 2-8].

Important coupling rules:

- size cruise thrust from the start-of-cruise condition, then check takeoff, climb, descent, go-around, and OEI performance with off-design engine data;
- allow continuous cruise climb when the mission/policy permits it, because weight reduction changes the optimum altitude and drag;
- integrate descent and approach fuel if the reserve or landing mass matters;
- keep throttle, propulsor count, shaft power, battery state of charge, and thermal limits architecture-specific;
- preserve the time, distance, altitude, mass, fuel/energy, thrust/power, and constraint state at each segment node.

N+3 explicitly sizes an engine for cruise, uses off-design analysis for takeoff, climb, and descent, integrates a continuous cruise climb, and rejects a design that cannot trim or close the engine cycle at off-design conditions [S06, pp. 112-113, 126-132].

## 6. Architecture seed generation

### 6.1 Discrete outer choices

The requirements-first workflow should generate a small, auditable catalogue of architecture seeds before continuous geometry optimization. Candidate seed families can include only configurations supported by the active ALAS model, for example:

- conventional tube-and-wing;
- blended or hybrid wing body;
- double-bubble or lifting-fuselage variant;
- truss-braced wing;
- hybrid-electric or turbo-electric variant of a supported airframe.

For each seed, enumerate discrete choices such as cabin type, deck count, engine count, propulsion family, energy source, tail arrangement, and cargo/container system. Continuous variables then include span, area, sweep, chord, fuselage/cabin dimensions, tail scale, engine size, tank/energy volume, and technology factors.

```text
ArchitectureSeed {
    id,
    family,
    discrete_choices,
    continuous_nominal,
    continuous_bounds,
    cabin_model,
    mass_model,
    aero_model,
    propulsion_model,
    supported_fidelities,
    calibration_range,
    assumptions,
    evidence
}
```

Architecture generation should derive nominal ranges from the TLAR rather than copy a real aircraft. Use payload, cabin/deck requirements, route range, field limits, span, and propulsion assumptions to set the first size. Keep a named baseline comparison when one exists, but do not let a baseline overwrite a ground-up seed.

### 6.2 Architecture-specific accommodation and mass

The BWB sizing method demonstrates the correct boundary: passenger bays, aisles, galleys, lavatories, pressure-wall layout, cargo, and pressurized centerbody structure are solved from the centerbody geometry; a conventional fuselage model is not reused without calibration [S05, pp. 5-23]. Its regression form,

```text
W_cabin = K_s * 0.316422 * TOGW**0.166552 * S_cabin**1.061158
```

is a calibrated example for the generated BWB family, not a universal ALAS weight equation [S05, pp. 18-20].

N+3 also links architecture and mission: the HWB concept uses centerbody lift, cabin/cargo volume, boundary-layer ingestion, distributed propulsion, and a specific payload/range mission; the report notes that propulsion configuration, core count, transmission, fuel, noise, and efficiency trade together [S06, pp. 118-125]. Therefore each architecture seed must declare which models are transferable and which require a dedicated model or an uncertainty penalty.

### 6.3 Fidelity funnel

Use a funnel consistent with the existing ALAS document:

```text
requirements validation
    -> algebraic Class-I sizing and matching diagram
    -> geometry/accommodation and payload/mass/CG screening
    -> low-order aero, propulsion, mission, airport checks
    -> structures/wingbox and high-fidelity external tools for finalists
    -> robust multi-scenario and final comparison
```

LAMBDA reports a modular framework that can exchange data with external high-fidelity tools and lets different modules use different methods/fidelities [S01, pp. 7-9, 17-18, 26-27]. OpenMDAO shows the same architectural benefit for DOE, surrogates, optimization, and reconfigurable workflows [S07, pp. 1-9].

## 7. Uncertainty and robust margins

### 7.1 Uncertainty taxonomy

Every uncertain quantity should be tagged as:

```text
aleatory_input       # operational or environmental variability
epistemic_input      # incomplete technology or calibration knowledge
model_form           # limitation of the chosen analysis method
numerical             # solver/discretization/tolerance effect
```

Candidate inputs include payload/baggage, Mach, winds, temperature, runway state, CLmax, CD0, induced-drag factor, SFC/efficiency, engine lapse, empty-weight regression factors, structural technology factors, battery specific energy, usable fuel fraction, and reserve policy parameters. Model-form uncertainty should not be hidden inside a random measurement error; it requires a calibration factor, bounded discrepancy, or an explicit fidelity transition.

NASA's UQ paper makes this distinction explicitly and integrates UQ into optimization rather than treating it only as a post-optimality plot [S14, pp. 1-6].

### 7.2 Probability and quantile constraints

Define every hard requirement as a signed margin that is positive when passing:

```text
r_min = actual - minimum_required
r_max = maximum_allowed - actual
r_eq  = tolerance - abs(actual - target)
```

For an estimated pass probability `p_pass`, require:

```text
P(r >= 0) >= p_target
```

or use a conservative quantile:

```text
Q_alpha(r) >= 0
```

where `alpha = 1 - p_target` for a lower-tail pass margin. For example, a 95% robust minimum requires the estimated 5th percentile of `r_min` to be non-negative. Record sample count, seed, distributions, correlations, confidence interval, and whether the result is a Monte Carlo, Latin-hypercube, polynomial-chaos, or interval bound.

For a scalar objective `J`, a robust ranking can be:

```text
J_robust = E[J] + lambda_risk * Risk(J)
```

with `Risk` chosen as a high percentile, standard deviation, CVaR-like tail metric, or an explicit mass/energy margin penalty. The feasibility-first ordering remains primary: a low expected fuel burn does not outrank a candidate whose 95% takeoff or reserve margin is negative.

### 7.3 Practical ALAS policy

At low fidelity:

1. use deterministic nominal screening first;
2. run a small Latin-hypercube or Monte Carlo scenario set on all nominally feasible candidates;
3. reject or flag candidates with negative hard-margin quantiles;
4. estimate sensitivity so that the next design search invests in the dominant uncertain inputs;
5. run polynomial-chaos or analytic-gradient UQ only for finalists when the installed tooling supports it.

At higher fidelity, compare the low-order prediction with the new solver and record the discrepancy as model-form evidence. NASA's case study reports that an uncertainty-optimized design was heavier than the deterministic design and that a 95% confidence-bound optimization cost about 13.6 times more computation [S14, pp. 13-15]. ALAS should expose that cost and not make robust mode appear free.

## 8. Implementable ALAS stage contract

### 8.1 Canonical input contract

The existing `DesignBrief` is the correct source of truth. The sizing adapter should consume it without copying passenger counts, dimensions, or engine settings into a second wizard state.

```text
SizingInput {
    brief_id: String,
    design_load_case: LoadCase,
    mission: MissionDefinition,
    reserve_policy: ReservePolicy,
    accommodation: AccommodationRequirements,
    performance_limits: PerformanceRequirements,
    airport_limits: AirportRequirements,
    architecture_mode: baseline | ground_up | comparison,
    architecture_seeds: Vec<ArchitectureSeed>,
    initial_bounds: DesignBounds,
    fidelity: FidelityPlan,
    uncertainty_plan: Option<UncertaintyPlan>,
    evidence: Vec<EvidenceRef>
}
```

`MissionDefinition` must include a segment graph, route distance, start/end conditions, load cases, atmosphere assumptions, and the range convention. `AccommodationRequirements` must retain design and maximum passenger/cargo cases, mass basis, deck/LD3-45 requirements, and cabin policy. `PerformanceRequirements` must retain Mach/MMO/VMO, ICA/TTC, OEI ceiling, cruise ceiling, TOFL, landing, and approach speed. Every field inherits the existing hard/soft/objective/diagnostic policy.

### 8.2 Stage outputs

```text
SizingState {
    architecture_id,
    design_vector,
    wing_area_m2,
    wing_loading_N_m2,
    installed_thrust_N,
    thrust_loading,
    energy_or_fuel_capacity,
    geometry_seed,
    accommodation_summary,
    class_i_mass,
    class_ii_mass_statement,
    mission_result,
    performance_result,
    structure_result,
    residuals,
    convergence,
    uncertainty_result,
    evidence
}

MassStatement {
    takeoff_mass_kg,
    operating_empty_mass_kg,
    zero_fuel_mass_kg,
    payload_mass_kg,
    fuel_or_storage_mass_kg,
    unusable_fuel_mass_kg,
    cg_m,
    inertia_kg_m2,
    items: Vec<MassItem>,
    taxonomy_id,
    closure_residual,
    model_uncertainty
}

MissionResult {
    trip_range_nm,
    trip_time_s,
    segment_results,
    trip_fuel_kg,
    reserve_fuel_kg,
    required_usable_fuel_kg,
    arrival_mass_kg,
    reserve_arrival_mass_kg,
    energy_state_trace,
    converged,
    failure
}

ConstraintResidual {
    name,
    actual,
    target,
    unit,
    direction,
    load_case,
    atmosphere_case,
    normalized_violation,
    signed_margin,
    policy,
    fidelity,
    source_or_assumption
}
```

The residual shape is aligned with the existing ALAS requirements document. A failed geometry, accommodation, mission, or structural stage should generate one or more named residuals and a typed stage failure; it must not be converted to a plausible aircraft.

### 8.3 Staged evaluation sequence

| Stage | Inputs | Required outputs | Failure semantics |
| --- | --- | --- | --- |
| 0. Normalize brief | `DesignBrief`, unit adapters, reserve policy | SI values, load cases, range convention, validated policies | reject invalid units, contradictory ranges, missing reserve, or unowned requirements |
| 1. Generate architecture seeds | TLAR, accommodation, supported catalogue | named seeds, discrete choices, bounds, calibration provenance | retain an architecture residual if no supported family can translate the brief |
| 2. Class-I sizing | payload, mission fractions/Breguet seed, constraints | initial `m0`, `S`, thrust/power, fuel/energy capacity | keep mass/constraint residuals; no high-fidelity call |
| 3. Constraint matrix | `W/S`, `T/W`, load cases, atmosphere | matching diagram, governing constraint, selected initial point | hard residual if no feasible region exists |
| 4. Materialize geometry and accommodation | state, architecture, cabin/cargo policy | geometry, placed/requested/capacity/unfilled summary, volume | typed geometry or capacity failure |
| 5. Class-II mass and CG | geometry, propulsion, payload | component mass statement, CG, inertia, MZFW/MTOW checks | typed model applicability or closure failure |
| 6. Mission closure | mass, aero, engine, route, reserves | integrated segment trace, trip/reserve fuel, range, arrival masses | typed non-convergence, infeasible reserve, tank/energy limit, or solver failure |
| 7. Performance and airport | mission state, engines, runway, landing case | cruise/climb/descent/OEI/TOFL/LDN/Vapp residuals | retain each failing load case; do not collapse to one score |
| 8. Structure/wingbox | geometry, loads, materials, technology factors | structural mass, margins, loads, fidelity evidence | typed structural failure or diagnostic if not available |
| 9. Robust screen | nominal candidate plus uncertainty plan | probability/quantile margins, sensitivities, cost | flag insufficient samples or negative robust hard margins |
| 10. Rank/report | all stage outputs | hard-feasible ordering, objective/preferences, evidence ledger | near-feasible candidates remain inspectable with grouped rejection reasons |

### 8.4 Ranking and diagnostics

Use the existing hard-first policy:

1. valid geometry and required stages completed;
2. zero hard requirement violations;
3. lower normalized hard residual severity for near-feasible diagnostics;
4. lower soft-target penalty;
5. objective value such as fuel burn, mass, cost, or emissions.

Retain the best near-feasible candidate for every major rejection group. This lets ALAS tell the user whether the issue is payload, range/reserves, wing area, thrust, tank volume, cabin capacity, airport performance, stability, or structure. OpenMDAO's documented handling of failed analyses supports this approach: failed or non-converged cases need explicit treatment instead of being fed into a surrogate as arbitrary high objective values [S07, pp. 5-9].

## 9. Mapping to `REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md`

The mapping below is intentionally to the existing document's concepts, not to a new parallel architecture.

| Existing requirements-first concept | Mission-sizing implementation |
| --- | --- |
| TLAR value/unit/load-case/policy/evidence | `Requirement` metadata is carried into every sizing input and residual; no sizing assumption becomes a hard requirement without an explicit policy. |
| Design range | `MissionDefinition.trip_range` plus explicit `range_definition`; reserve segments are separate and default to no range credit. |
| Design and maximum payload | `A` and `D` payload-range cases; mass closure, MZFW, cabin/cargo capacity, and fuel-volume checks are all retained. |
| Initial cruise Mach, ICA/TTC, OEI ceiling, maximum cruise altitude | start/end cruise, climb, and OEI segments with separate atmosphere/load cases; mission and performance residuals point back to the owning TLAR. |
| TOFL, landing distance, approach speed | takeoff/landing rows in the matching matrix; `W/S` from stall/approach and `T/W` from field/climb/go-around analyses. |
| Wingspan and ACN | geometry/airport stages produce signed limit residuals before expensive finalist analyses. |
| Passenger mass basis, decks, LD3-45, cabin policy | accommodation stage returns requested, placed, capacity, unfilled, and deck/hold summaries; payload mass is not confused with physical capacity. |
| `DesignBrief` as one source of truth | `SizingInput` is an adapter view of the existing brief; no wizard-side duplicate of geometry, passengers, or engine settings. |
| Ground-up architecture phase | `ArchitectureSeed` enumerates discrete supported families and records the continuous design bounds and model applicability. |
| Candidate materialisation | continuous vector is applied only after architecture selection; geometry and accommodation failures are typed. |
| Preliminary sizing | Class-I mass/fuel closure plus constraint matrix produces wing, thrust/power, cabin, and fuel-volume seeds. |
| Physical analysis stages | Class-II mass/CG, aero/trim/stability, mission/performance, airport, and structures consume the same candidate state. |
| Hard/soft/objective/diagnostic ranking | residuals carry policy and signed margin; hard feasibility precedes preferences and objectives. |
| Multi-fidelity funnel | algebraic sizing -> geometry/mass/CG -> low-order mission/airport -> structures/external tools -> robust finalist analysis. |
| Final report and comparison | publish mass statement, payload-range points, mission trace, constraint matrix, reserve breakdown, uncertainty summary, and baseline comparison with evidence. |

The existing wizard flow maps directly to the contract: intent and architecture seed generation precede mission/accommodation entry; validation freezes a readable `DesignBrief`; live preview uses the same geometry/cabin/route concepts; launch freezes run options; candidate evaluation then follows materialization, accommodation, mass/fuel, aero/performance, mission/airport, structures, residuals, and ranking.

## 10. Recommended implementation assumptions and guardrails

1. Use SI internally. Convert NM, ft, KCAS, lb, and horsepower at the adapter boundary; store the original user value and unit for traceability.
2. Keep force and mass distinct. Weight-loading equations use `N/m2`; legacy reports may display `kg/m2` or `lb/ft2`, but conversions must be explicit.
3. Keep reserve fuel separate from trip fuel and from operating-empty mass. A legacy source that uses another convention must name it in the source/assumption field.
4. Use Class-I fractions only to initialize or screen. Upgrade to Class-II when geometry, engine, cabin, or load information exists; include a calibration and uncertainty record for each component model.
5. Treat takeoff, landing, OEI, climb, cruise, descent, and reserve as different load cases. A single design mass and a single cruise drag estimate cannot represent all of them.
6. Do not use a tube-wing WER for BWB, truss-braced, hydrogen, distributed-electric, or other novel architectures without a declared applicability range and uncertainty/discrepancy model.
7. Detect non-convergence and cycle behavior. A failed analysis is evidence about the candidate and should be retained as a typed residual.
8. Keep discrete architecture outside the continuous optimizer. Enumerate a small supported catalogue or use a mixed-integer driver that preserves architecture identity.
9. Default conceptual reserve segments to `range_credit = false`. Allow a different convention only through an explicit brief field and report it in every payload-range plot.
10. Use nominal plus robust screening. Report the sample distribution, confidence/quantile, and computational cost so the user can distinguish deterministic feasibility from robust feasibility.

## 11. Downloaded PDF source ledger

The local copies were checked as PDFs with `pdfinfo`; page counts below are file-page counts. SHA-256 values were computed with `Get-FileHash -Algorithm SHA256` on 2026-08-26. NASA rights notes reproduce the NTRS metadata fields: public distribution, `PUBLIC_USE_PERMITTED` or `GOV_PUBLIC_USE_PERMITTED`, and no third-party material indicated. This is a provenance record, not a legal opinion. The MDPI article is explicitly published under CC BY 4.0.

| ID | Local PDF and pages | Authors; year; title; DOI/report | Source URL | Rights note | SHA-256 |
| --- | --- | --- | --- | --- | --- |
| S01 | [mdpi_2024_lambda-aircraft-design-framework.pdf](../../bib/mission-sizing/mdpi_2024_lambda-aircraft-design-framework.pdf), 33 pp. | Saeed Hosseini; Mohammad Ali Vaziry-Zanjany; Hamid Reza Ovesy (2024). *A Framework for Aircraft Conceptual Design and Multidisciplinary Optimization*. DOI: `10.3390/aerospace11040273`. | [MDPI article](https://www.mdpi.com/2226-4310/11/4/273) and [PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-11-00273/article_deploy/aerospace-11-00273-v2.pdf) | Open access, CC BY 4.0: [license](https://creativecommons.org/licenses/by/4.0/). | `02a0bcd8b2e38040c0766f1a802828bff25c10fbbf77a4dae131bd3d6e0659cf` |
| S02 | [nasa_19730024120_transport-design-lecture-notes.pdf](../../bib/mission-sizing/nasa_19730024120_transport-design-lecture-notes.pdf), 51 pp. | R. W. Simpson (1972). *Technology for design of transport aircraft. Lecture notes for MIT courses: Seminar 1.61 freshman seminar in air transportation and graduate course 1.201, transportation systems analysis*. NTRS 19730024120. | [NASA NTRS record](https://ntrs.nasa.gov/citations/19730024120) and [PDF](https://ntrs.nasa.gov/api/citations/19730024120/downloads/19730024120.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `35570b2b41bb3a07b2b0a7144367caa1c22f1cacb750d0c00bd7dbaf971472b5` |
| S03 | [nasa_19900016655_global-sentry-sizing.pdf](../../bib/mission-sizing/nasa_19900016655_global-sentry-sizing.pdf), 121 pp. | Mona-Lisa Alexandru; Frank Martinez; Jim Tsou; Henry Do; Ashish Peters; Tom Chatsworth; YE Yu; Jaskiran Dhillon (1990). *Global Sentry: NASA/USRA high altitude reconnaissance aircraft design, volume 2*. NASA-CR-186820-VOL-2. | [NASA NTRS record](https://ntrs.nasa.gov/citations/19900016655) and [PDF](https://ntrs.nasa.gov/api/citations/19900016655/downloads/19900016655.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `f67fd20766042e539024af38caf0e8eff17681d27b25cb16cb6fd2b6ce5b4951` |
| S04 | [nasa_20000013558_hsct-concept.pdf](../../bib/mission-sizing/nasa_20000013558_hsct-concept.pdf), 32 pp. | James W. Fenbert; Lori P. Ozoroski; Karl A. Geiselhart; Elwood W. Shields; Marcus O. McElroy (1999). *Concept Development of a Mach 2.4 High-Speed Civil Transport*. NASA/TP-1999-209694. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20000013558) and [PDF](https://ntrs.nasa.gov/api/citations/20000013558/downloads/20000013558.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `d03a6aa4778498744dcc52f763f5d3c6d039f665c8b9a0e1412ab60e8a288c51` |
| S05 | [nasa_20040110949_bwb-sizing.pdf](../../bib/mission-sizing/nasa_20040110949_bwb-sizing.pdf), 39 pp. | William M. Kimmel; Kevin R. Bradley (2004). *A Sizing Methodology for the Conceptual Design of Blended-Wing-Body Transports*. NASA/CR-2004-213016. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20040110949) and [PDF](https://ntrs.nasa.gov/api/citations/20040110949/downloads/20040110949.pdf) | NTRS: `PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `cc00d924cfd66937894b12fa4c5361c9f67b0b5197621fe345ec3cf13182833e` |
| S06 | [nasa_20100042401_n-plus-3-trade-studies.pdf](../../bib/mission-sizing/nasa_20100042401_n-plus-3-trade-studies.pdf), 189 pp. | E. M. Greitzer; P. A. Bonnefoy; E. DelaRosaBlanco; C. S. Dorbian; M. Drela; D. K. Hall; R. J. Hansman; J. I. Hileman; R. H. Liebeck; J. Levegren; P. Mody; J. A. Pertuze; S. Sato; Z. S. Spakovszky; C. S. Tan; J. S. Hollman; J. E. Duda; N. Fitzgerald; J. Houghton; J. L. Kerrebrock; G. F. Kiwada; D. Kordonowy; J. C. Parrish; J. Tylko; E. A. Wen (2010). *N+3 Aircraft Concept Designs and Trade Studies*. NASA/CR-2010-216794/VOL1. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20100042401) and [PDF](https://ntrs.nasa.gov/api/citations/20100042401/downloads/20100042401.pdf) | NTRS: `PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `3f2db87824637ecea952b33c7807bf48e99af71b91ed3202a32cbf7d62a6a599` |
| S07 | [nasa_20140016748_openmdao.pdf](../../bib/mission-sizing/nasa_20140016748_openmdao.pdf), 13 pp. | Christopher M. Heath; Justin S. Gray (2012; NTRS record 20140016748). *OpenMDAO: Framework for Flexible Multidisciplinary Design, Analysis and Optimization Methods*. GRC-E-DAA-TN14348. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20140016748) and [PDF](https://ntrs.nasa.gov/api/citations/20140016748/downloads/20140016748.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `1c0c01839fb1cd4a761d55bab99b6ceca963f68c0dac1bcfe1c08e2d76d756fa` |
| S08 | [nasa_20160007763_hybrid-electric-regional-sizing.pdf](../../bib/mission-sizing/nasa_20160007763_hybrid-electric-regional-sizing.pdf), 16 pp. | Kevin R. Antcliff; Mark D. Guynn; Ty V. Marien; Douglas P. Wells; Steven J. Schneider; Michael T. Tong (2016). *Mission Analysis and Aircraft Sizing of a Hybrid-Electric Regional Aircraft*. AIAA-2016-1028. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20160007763) and [PDF](https://ntrs.nasa.gov/api/citations/20160007763/downloads/20160007763.pdf?attachment=true) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `4a66d53a673201320c7cc6e55d106cc68245504c34697ec054d37f1f89061943` |
| S09 | [nasa_20170005851_flops-weights.pdf](../../bib/mission-sizing/nasa_20170005851_flops-weights.pdf), 91 pp. | Douglas P. Wells; Bryce L. Horvath; Linwood A. McCullers (2017). *The Flight Optimization System Weights Estimation Method*. NASA/TM-2017-219627/VOL1. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20170005851) and [PDF](https://ntrs.nasa.gov/api/citations/20170005851/downloads/20170005851.pdf) | NTRS: `PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `819a48fc9c8f34f14595d93f3e3d54dc8454298e83e64048c14ac7bda00bb51d` |
| S10 | [nasa_20180004393_short-haul-revitalization.pdf](../../bib/mission-sizing/nasa_20180004393_short-haul-revitalization.pdf), 75 pp. | Ty V. Marien; Kevin R. Antcliff; Mark D. Guynn; Douglas P. Wells; Steven J. Schneider; Michael T. Tong; Antonio A. Trani; Nicolas K. Hinze; Samuel M. Dollyhigh (2018). *Short-Haul Revitalization Study Final Report*. NASA/TM-2018-219833. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20180004393) and [PDF](https://ntrs.nasa.gov/api/citations/20180004393/downloads/20180004393.pdf) | NTRS: `PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `4c0611e2122299a848c1fec65678df2e348cfda07111afe4a0f19f96cc9e1224` |
| S11 | [nasa_20190000427_leaps-energy-mission.pdf](../../bib/mission-sizing/nasa_20190000427_leaps-energy-mission.pdf), 12 pp. | Francisco M. Capristan; Jason R. Welstead (2018; NTRS record 20190000427). *An Energy-Based Low-Order Approach for Mission Analysis of Air Vehicles in LEAPS*. NF1676L-27376. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20190000427) and [PDF](https://ntrs.nasa.gov/api/citations/20190000427/downloads/20190000427.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `0ea7235d03cc12ba38bd77a39a5f62191326a12e9b001e5b075efb5382ab1f3d` |
| S12 | [nasa_20190000431_weight-method-comparison.pdf](../../bib/mission-sizing/nasa_20190000431_weight-method-comparison.pdf), 19 pp. | Bryce L. Horvath; Douglas P. Wells (2018; NTRS record 20190000431). *Comparison of Aircraft Conceptual Design Weight Estimation Methods to the Flight Optimization System*. AIAA-2018-2032. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20190000431) and [PDF](https://ntrs.nasa.gov/api/citations/20190000431/downloads/20190000431.pdf) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `3690e6b138c3590a4f52a3c85299d7dedf3a7ae9df2aaaa516139b6b384541ab` |
| S13 | [nasa_20190000442_leaps-overview.pdf](../../bib/mission-sizing/nasa_20190000442_leaps-overview.pdf), 14 pp. | Jason R. Welstead; Darrell Caldwell; Ryan Condotta; Nerissa Monroe (2018; NTRS record 20190000442). *An Overview of the Layered and Extensible Aircraft Performance System (LEAPS) Development*. NF1676L-27420. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20190000442) and [PDF](https://ntrs.nasa.gov/api/citations/20190000442/downloads/20190000442.pdf) | NTRS: `PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. | `143b69749f0828bf9054f0a5e99c4bd47bcee98aaccb59bb60aefcfcef491255` |
| S14 | [nasa_20240014863_design-under-uncertainty.pdf](../../bib/mission-sizing/nasa_20240014863_design-under-uncertainty.pdf), 17 pp. | Ben D. Phillips; Joanna N. Schmidt; Eliot D. Aretskin-Hariton; Robert D. Falck (2024 record year). *Design Under Uncertainty for Conceptual Aircraft Design Leveraging Analytical Gradients*. NTRS 20240014863. | [NASA NTRS record](https://ntrs.nasa.gov/citations/20240014863) and [PDF](https://ntrs.nasa.gov/api/citations/20240014863/downloads/Phillips_SciTech_rev2.pdf?attachment=true) | NTRS: `GOV_PUBLIC_USE_PERMITTED`, public distribution, no third-party material indicated. The record did not expose a formal publication date; the year above is the NTRS record year. | `43cc62f314a886810af2a023aa313b3bdf1aa1889817c70ae315808bfb48dbb5` |

## 12. Citation-only and unresolved sources

These sources were recorded for future traceability but were not downloaded into the local bibliography. They are either copyrighted books, access-restricted DLR/Elsevier material, or sources whose redistribution rights were not verified. They must not be treated as local evidence until an authorized copy is obtained.

### Foundational books and methods

- AGARD, *Special Course on Engineering Methods in Aerodynamic Analysis and Design of Aircraft*, AGARD-R-783, ISBN 92-835-0652-9 (1992), [NASA NTRS record](https://ntrs.nasa.gov/citations/19920014707) and [AGARD external reference](https://www.sto.nato.int/publications/AGARD/AGARD-R-783/AGARD-R-783.pdf). The course is directly relevant to ALAS because its stated scope is proven engineering methods for conceptual and preliminary aircraft design, low-level analysis codes, and performance prediction. NTRS marks the record public but notes that portions may be copyright protected and provides no local download; it was therefore not downloaded.
- Raymer, Daniel P., *Aircraft Design: A Conceptual Approach*, AIAA Education Series. The public NASA comparison [S12, pp. 2-5] summarizes the Class-I/statistical/Class-II progression and the N+3 report cites a 2006 edition [S06, p. 129]. The book was not downloaded because it is copyrighted.
- Roskam, Jan, *Airplane Design*, Parts I-VIII, DARcorporation/Roskam Aviation and Engineering. N+3 cites *Airplane Design, Part V: Component Weight Estimation*, 2nd ed., 1989 [S06, p. 128]; LAMBDA and the weight comparison discuss its Class-I/Class-II workflow [S01, pp. 8-9; S12, pp. 2-5]. Not downloaded because it is copyrighted.
- Torenbeek, Egbert, *Synthesis of Subsonic Airplane Design*, Delft University Press, 1982, and later *Advanced Aircraft Design*. The LAMBDA paper cites Torenbeek in its weight-hierarchy discussion [S01, p. 18]. Not downloaded because redistribution rights were not verified.

### DLR, journal, and recent academic references

- DLR, *Conceptual Design Studies of Short Range Aircraft Configurations with Hybrid Electric Propulsion* (2019), [DLR eLib record](https://elib.dlr.de/128761/). The PDF endpoint was marked DLR-intern/restricted in the search result; not downloaded.
- Krengel and Hübner, *A Physics-Based Approach for Aeroservoelastic Wing Sizing in Conceptual Aircraft Design* (2020), [DLR eLib record](https://elib.dlr.de/138258/). Access was marked DLR-intern/restricted; not downloaded.
- Fioriti et al., *Multidisciplinary aircraft integration within a collaborative and distributed design framework using the AGILE paradigm*, *Progress in Aerospace Sciences* (2020), DOI `10.1016/j.paerosci.2020.100648`, [DLR record](https://elib.dlr.de/137038/). Journal redistribution rights were not verified; not downloaded.
- *Simultaneous aircraft sizing and multi-objective optimization considering off-design mission performance during early design*, *Aerospace Science and Technology* 126 (2022), 107662, DOI `10.1016/j.ast.2022.107662`. The publisher copy was not downloaded because access/redistribution was not verified.
- Di Bianchi, Sêcco, and Silvestre, *A framework for enhanced decision-making in aircraft conceptual design optimisation under uncertainty*, *The Aeronautical Journal* 125 (2021), 777-806, DOI `10.1017/aer.2020.134`. The publisher PDF was not downloaded because redistribution status was not verified.
- Borgia et al., *Uncertainty quantification framework for robust conceptual aircraft design*, SSRN preprint (2026), [SSRN record](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=6416145). The abstract is relevant to Monte Carlo/Latin-hypercube uncertainty in weight, aero, mission, and engine models, but the preprint was not downloaded because redistribution rights were not verified.
- Liu, Kim, Reyner, and Liem, *Data-driven multi-range mission-based overall aircraft conceptual design optimization*, ICAS 2024 author-uploaded record. The ResearchGate copy was not downloaded because the author-uploaded redistribution status was unclear.
- *Environment of Design Requirements Input and Preliminary Sizing for Aircraft Conceptual Design*, *Chinese Journal of Aeronautics* (2003), DOI `10.1016/S1000-9361(11)60165-9`. This is a useful requirements-to-automatic-TOGW-iteration citation, but it was not included in the local PDF set because the redistribution status was not verified during this pass.
- Chen, *Development and application of analytic equation of payload-range diagram for commercial aircraft* (2019), DOI `10.7527/S1000-6893.2018.22407`. Recorded for a future payload-range comparison; not downloaded because an authorized redistributable copy was not verified.

## 13. Implemented engineering decision: mission-sized dispatch closure

The normal ALAS pipeline now applies the inner closure described in Sections
3.2, 3.4, and 5.2 to the native segment mission. The immediate decision was
driven by the repeated-gross-weight/fuel closure used in FLOPS [S09] and the
N+3 practice of repeating initial fuel until the specified mission closes
[S06]: `MTOW - OEW - payload` remains an upper mass-budget load case and is no
longer reported as route-required fuel.

For the actual placed payload and component OEW, ALAS prepares the aerodynamic
surrogate, engine sizing, and segment schedule once, then re-evaluates the
mission at candidate brake-release masses until

```text
loaded fuel = native trip burn(loaded takeoff mass) + landing reserve
```

The landing reserve is the larger of the supported policy amount and the
declared minimum landing-fuel floor. `TripFraction` and `FixedMass` are
supported. `DiversionAndHold` is evaluated by a bounded outer replay of native
missed-approach, diversion, hold, and alternate-descent segments; the inner
dispatch closure continues to solve the mass-dependent trip burn. Diversion
distance and holding time therefore never become an unexplained fuel scalar.
The native trunk ends at a declared start-of-final-approach decision point.
The nominal branch then contains final approach, flare/landing roll, and
taxi-in; its final-approach distance is included in takeoff-to-intended-
destination range exactly once. A diversion-and-hold case branches from that
same decision state through missed approach, diversion, hold, alternate
descent, alternate terminal landing, and taxi-in; it never first lands at the
intended destination and none of those reserve segments receives design-range
credit. The configured approach fuel is partitioned once between final
approach and flare/landing roll. Taxi-out, landing roll, and taxi-in have no
range credit. Usable tank capacity and MTOW independently bound the solve.
Fuel exhaustion, invalid telemetry, non-converged segments, evaluation errors,
capacity/MTOW shortfalls, and iteration limits remain typed outcomes with the
available iterate history.

For each apparently complete native mission, ALAS independently evaluates the
retained fuel-flow history through the final row of every segment's
pseudospectral integration operator. It compares each segment integral with
that segment's mass loss, checks mass continuity at every segment boundary,
and finally compares summed flow with propagated mission endpoint loss. The
four resulting flow/state quantities are serialized on every complete
dispatch iterate. This is a numerical state/flow closure check, not a second
fuel-burn model. It exposes incomplete operators, canceling segment errors,
and state-propagation errors without claiming extra physical fidelity.

The engineering outputs now distinguish ZFM, ramp mass, take-off mass, landing
mass, reserve-arrival mass, usable/unusable fuel, usable capacity, ramp-loaded
and brake-release fuel, trip burn, named reserve components, MTOW/capacity
margins, closure residual, convergence tolerance, capacity evidence, and every
evaluated iterate. They are retained in the design database, canonical mission
solution, CLI run manifest, terminal summary, feasibility/CG state, and the
headless mission figures. When detailed tanks are declared, the selected burn
history is replayed through their feed topology and locations to retain
tank-local quantity, mass/CG, and feed-failure evidence. ALAS calls this a
detailed tank-feed/fuel-CG result only after that replay completes; an
aggregate capacity, component fuel centroid, or merely declared tank list is
not tank-feed or fuel-CG evidence. The component-ledger
state remains mass authority; the bounded Class-I closure supplies a
reconciled wing-area/thrust/tank seed and cannot overwrite it silently.

Capacity evidence in the design database and CLI manifest includes the method
identifier and the actual inputs used. Published-preset cases retain their
preset, exact model/weight-variant/engine/modification/tank identity, and
quantity-specific source revision plus source volume/density where declared;
geometry-estimated cases retain gross wing volume, usable fraction, usable
volume, configured density/reference condition, and the explicit conceptual-
design validity boundary.

### 13.1 CLI baseline/post-change experiment (AVE design mission)

The same unoptimized AVE configuration and route were evaluated through the
headless CLI before and after the inner dispatch closure. Geometry and the
configured-MTOW aerodynamic design point were held fixed. The old path used
the full `MTOW - ZFM` remainder: 358.67 t takeoff mass, 96.84 t carried fuel,
74.88 t trip burn, and 21.96 t destination fuel. The corrected path converged
in four native mission evaluations to 335.52 t takeoff mass, 73.69 t loaded
fuel, 70.19 t trip burn, and 3.51 t reserve/destination fuel. Its final fuel
closure residual was 0.043 kg against a 0.5 kg tolerance; MTOW and usable-tank
margins were 23.15 t and 106.29 t respectively.

The 6.45% lower brake-release mass reduced predicted trip burn by 6.27%, the
expected direction for the same aircraft and route. The change also moved the
analyzed takeoff CG to 19.32% MAC while retaining the explicit single-fuel-
centroid limitation. Total CLI wall time increased from 507 s to 568 s in this
external-tool-enabled debug campaign because the native mission was replayed
at each closure iterate. These timings are diagnostic rather than benchmark
quality.

This archived baseline predates the active conceptual field-performance
adapter. The current path passes solved brake-release and destination masses
to dry-runway all-engine, accelerate-stop/go, OEI second-segment, landing, and
approach-speed calculations using the frozen high-lift and airport assumptions.
Those are preliminary conceptual screens, not AFM or certification evidence.

## 14. Minimal acceptance checklist for the ALAS implementation

The remaining mission-sizing work should retain the following acceptance checks:

- a `DesignBrief` with design range, design/max payload, load cases, and named reserve policy can be serialized and reloaded without losing units or policy;
- the solver reports trip fuel and each reserve component separately;
- a Class-I seed can produce a matching-diagram point or a grouped infeasibility explanation;
- a Class-II mass statement has component provenance, CG, taxonomy, and closure residual;
- payload-range A/B/C/D cases use the same mass, geometry, and mission definitions;
- climb, cruise, descent, OEI, takeoff, landing, and reserve load cases are visible in the mission/performance output;
- tank/energy volume, MZFW, MTOW, cabin/cargo capacity, span, and airport constraints are hard residuals when configured as hard requirements;
- architecture choices are enumerated and remain identifiable in results;
- non-convergence and external-tool failures are typed and retained;
- nominal and robust results report fidelity, samples/distributions, quantiles, and computational cost;
- the existing requirements-first mapping remains the one source of truth and no wizard-specific duplicate state is introduced.
