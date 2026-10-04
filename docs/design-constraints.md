# Design-constraint audit

The audit covers the configuration fields that are
presented as design constraints, their defaults and preset overrides, and the
code that consumes them. It also records the boundary between an aircraft
requirement, a physical-model input, a numerical setting, a preference, and a
validity-domain limit. References below identify the current consumers by
path rather than by a commit hash.

The current implementation has a useful policy split in
`crates/alas-config/src/optimizer.rs`: objective settings, solver settings,
design-space bounds and plausibility limits are separate
groups. `DesignRequirements` still mixes those categories for historical
wire-format compatibility. The audit therefore treats the existing layout as
the contract and records the migration that is needed before moving serialized
fields.

## Classification used by the audit

* **Requirement/TLAR** is a value the selected aircraft or mission is asked to
  meet. It may be a ceiling, floor, target, or discrete choice, and must say
  whether it is fixed, derived, or disabled.
* **Physical/model input** is an assumption used by a discipline model. It
  needs a basis, validity range, units, and a traceable source; changing it can
  change the meaning of several requirements.
* **Numerical setting** controls convergence, tolerances, or ranking. It is not
  evidence that an aircraft is feasible.
* **Preference** ranks a design inside the feasible set. It should not be
  described as a certification limit.
* **Validity-domain limit** protects a correlation or solver from being used
  outside the geometry or operating domain for which it was checked.
* **Legacy/derived** exists for saved-file compatibility or is computed from
  another field. It should not introduce a second authority.

The consumer names below refer to the actual code paths found by search. A
field is considered traceable only when its default/override rule and at least
one physical or numerical consumer are both identifiable.

## `DesignRequirements` ledger

Source: `crates/alas-config/src/requirements.rs`. The shipped defaults are the
`Default` implementation at lines 256–277. Named presets may overwrite a
subset; custom cabin values are user overrides where the schema says so.

| Field (unit) | Shipped default and override | Actual consumers | Classification and audit result |
|---|---|---|---|
| `cruise_mach` (-) | `0.84`; editable top-level requirement; aircraft presets may set it | Atmosphere and cruise trim in `alas-opt/src/mdo/mission_model.rs`, thrust requirement in `alas-opt/src/mdo/residuals_performance.rs`, mission/pipeline/report paths | Requirement/TLAR. Retain, but qualify it with the selected aircraft/route operating point and record the source case. |
| `cruise_altitude_m` (m) | `11887.2`; editable; preset/aircraft case may override | Atmosphere, mission profile, cruise thrust and reports | Requirement/TLAR plus atmosphere input. Retain in the brief, with ISA deviation and source case recorded alongside it. |
| `mtow_kg` (kg) | `358670`; editable; reference presets may provide a value; `MtowSizing` chooses fixed, mission-sized, seed-only, band-centre (when `mtow_target_kg` is zero) or seed use | `AlasConfig::mtow_plan()`, `mdo::sizing`, `mdo::mda`, `mdo::residuals`, feasibility, mass/mission/report paths | Requirement/weight limit. Retain as a declared limit or sizing seed, but always expose the selected sizing policy. It is not a structural-payload value. |
| `aircraft_type` (-) | `passenger`; user choice; `normalized()` selects valid cabin-preset family | Payload build, objective model, geometry residuals, report and GUI paths | Discrete requirement. Retain. The normalized value is the single authority. |
| `cabin_preset` (-) | `Ryanair`; preset or `Custom`; passenger/cargo type controls legal values | Payload geometry/build, objective model, GUI custom-input authority | Discrete load-case requirement. Retain. Named preset values need provenance; `Custom` is the explicit override. |
| `optimize_passenger_capacity` (-) | `true`; serde compatibility only, hidden and skipped on serialization | No product behavior; `resolves_payload_from_candidate_geometry()` is unconditional | Legacy. Keep only for old files and keep it out of the consumer ledger. Do not expose as an active constraint. |
| `num_passengers` (passengers) | `350`; geometry/preset-derived unless `Custom`; hidden | Payload load case, `objective_model`, legacy objective-cost path and reports | Derived load-case value. Keep as a snapshot of the resolved cabin, but do not use it as a second capacity authority. The residual uses `min_passenger_capacity` for an explicit floor. |
| `cargo_payload_kg` (kg) | `102100`; preset-derived unless `Custom`; user override only for custom cabin | Cargo load case, payload/mass/mission paths, objective fallback | Requirement/load case. Retain as resolved capacity and document that it is not a structural cap. |
| `cargo_objective_kg` (kg) | `0` disables; explicit positive request overrides objective target only | `cargo_target_kg()`, geometry residuals and soft cost | Objective target, not feasibility. Move conceptually to the objective group in a future schema; retain wire compatibility now. The implementation correctly keeps it separate from carried payload and capacity. |
| `max_structural_payload_kg` (kg) | `0` disables; reference presets may derive from an MZFW/OEW budget; explicit config override | Payload layout, `full_analysis`, quick analysis, structural-mass feasibility and payload-range reports | Physical feasibility requirement. Retain, but require provenance and a declared basis (`MZFW`, `OEW`, structure/volume case). It must never be inferred from MTOW alone. |
| `min_passenger_capacity` (passengers) | `0` disables; explicit user floor | `mdo::residuals_geometry` as `passenger_shortfall` | Requirement floor. Retain. State whether the resolved cabin count includes the configured passenger/baggage mass authority. |
| `ultimate_load_factor` (-g) | `3.75`; presets/cases may override | Structural mass and load/V-n paths, structural reports and experiment matrices | Certification-basis/model input, not a general mission requirement. Move to a structures/load-case group with authority, amendment/category, and load-case provenance. The default is a screening value (`1.5 × 2.5`) and is not universal. |
| `dive_speed_m_s` (m/s) | `220`; aircraft case may override | Structural mass, mission V-n/flight envelope and validation | Certification-basis/model input. Move to structures/performance basis with speed type (EAS/TAS/Mach), altitude and source. `V_C=V_D/1.25` is a screening relationship, not a universal certification rule. |
| `limit_load_factor_neg` (-g) | `-1.0`; explicit case override | V-n/structural paths and report/experiment code | Certification-basis/model input. Move with `ultimate_load_factor`; record speed range and applicability. |
| `max_wing_area_m2` (m²) | `535`; editable/preset case | Geometry residual, objective cost, trim, pipeline feasibility and tests | Aircraft design requirement/cap. Retain. Specify projected reference-area convention and whether the cap is a TLAR or a study bound. |
| `min_wing_loading_kg_m2` (kg/m²) | `485`; editable/preset case | Geometry residual and objective cost; related feasibility/report paths | Study/design bound. Retain if it is a deliberate aircraft requirement; otherwise move to geometry preferences. The consumer uses closed design gross mass, so the unit and mass basis must remain explicit. |
| `max_cruise_cl` (-) | `0.95`; editable/preset case | Trim, objective evaluation and feasibility gates | Aerodynamic operating guard. Retain as an explicit cruise-point requirement, but attach the airfoil/configuration and margin basis; it is not a universal stall limit. |
| `target_static_margin` (fraction MAC) | `0.10`; editable/preset case | Envelope assessment, objective cost, landing-gear/report figures | Balance preference/target. Keep separate from the physical floor; it should be labelled a target or preference, not a hard certification value. |
| `cg_range_pct_mac` (% MAC) | `30`; editable/preset case | Model CG envelope, balance residual and reports | Loading-envelope requirement. Retain, with forward/aft load cases and mass-property evidence. |
| `min_physical_static_margin` (fraction MAC) | `0.05`; editable/preset case | Hard physical envelope residual, objective evaluation, feasibility and reports | Physical stability floor. Retain as a hard floor only when the CG/neutral-point model is valid for the candidate; preserve a distinct `NotEvaluated`/evidence-gap status in future work. |
| `passenger_mass_kg` (kg/person) | `100`; user/model setting; applies to every seated passenger in the product load case | `DesignRequirements::payload_kg`, objective model, mass/balance, pipeline/mission/report/export | Weight-and-balance model input, not an aircraft TLAR. Move to a load-case/operations basis with passenger/baggage method, operator, population and date. FAA AC 120-27F is operator guidance and does not make 100 kg a universal certification default. |
| `gravity_m_s2` (m/s²) | `9.81`; explicit physics assumption | Every mass-to-weight, atmosphere, mission, thrust, fuel and feasibility path | Physics setting. Move to an atmosphere/physics group or make it an immutable SI standard assumption for terrestrial runs; retain an explicit override only for a documented non-standard scenario. |

### Requirement consumers and override rules

The main payload authority is the geometry-resolved cabin. In passenger mode,
`num_passengers` is a result of the candidate shell and cabin preset; in cargo
mode, `cargo_payload_kg` is the configured capacity/load case. The optional
`cargo_objective_kg` is resolved by `cargo_target_kg()` and contributes a
two-sided soft target. It does not overwrite either the capacity or the
carried payload. This distinction is implemented in
`alas-opt/src/mdo/objective_model.rs` and `mdo/residuals_geometry.rs` and is
covered by the cargo objective tests.

Structural payload is a separate limit. A useful physical bookkeeping
relationship is

```text
maximum structural payload ≈ maximum-zero-fuel mass − operating empty mass
```

subject to the applicable structural, floor/volume, balance, and loading
cases. MTOW alone cannot provide that number: it also contains fuel and is
bounded by takeoff, landing, zero-fuel, CG, distribution, structural-loading,
and operating conditions. The project should retain an explicit override until
the mass model can compute and substantiate the relevant MZFW/OEW case.

## Objective and optimizer ledger

Source: `crates/alas-config/src/optimizer/objective.rs`, defaults at lines
242–253. These fields should not be shown as aircraft design constraints in a
requirements card.

| Field (unit) | Default and override | Consumer | Classification and disposition |
|---|---|---|---|
| `kind` (-) | `block_fuel`; explicit enum | Objective assembly and ranking | Numerical/objective policy. Keep in optimizer settings. |
| `design_range_nmi` (nmi) | `0` means selected-route great-circle distance; explicit override | Mission sizing and `mission_profile_range` residual | Mission brief. Keep in objective/mission group and record route-distance derivation. |
| `mtow_sizing` (-) | `fixed_requirement` for registered presets; `SizedByMission` for custom defaults; also `unconstrained`, `mtow_band`, `payload_adjusted`; an explicit saved mode is authoritative | `AlasConfig::mtow_plan()` (seed, dispatch and Aitken ceilings, residual bounds, design mission, structural basis, cost normalisation), MDA closure, mass residuals, structural design mass | Sizing policy. Keep in solver/objective settings; it determines whether `mtow_kg` is a ceiling, fixed value, seed or band centre. `mtow_band` and `payload_adjusted` are evaluated by the mission-sized closure under every optimizer method and design the structure at the closure; non-optimizing runs ignore the mode. |
| `mtow_target_kg` (kg) | `0` = `requirements.mtow_kg` | `MtowPlan::target_kg`: `mtow_band` seed, band centre and cost normalisation | Requirement/TLAR target. Finite and nonnegative (validation error otherwise). |
| `mtow_band_fraction` (-) | `0.05` (engineering estimate: about one weight-variant step) | `mtow_band` dispatch clamp `T (1 + p)` and the `mtow_band_upper`/`mtow_band_lower` residuals | Requirement tolerance. `0 < p < 1` (error), warning above 0.5. |
| `sizing_max_iterations` (-) | `30` | Mission/MTOW fixed-point closure | Numerical setting. Move under solver settings. |
| `sizing_tolerance_kg` (kg) | `1` | Closure status and `sizing_not_closed` | Numerical setting. Move under solver settings. |
| `retrim_cg_tolerance_pct_mac` (% MAC) | `0.1` | MDA re-trim/re-evaluation | Numerical/model-update trigger. Move under solver settings and keep its unit explicit. |
| `aerodrome_reference_code` (-) | `F` (80 m); `unrestricted` disables; a reference adaptation uses the preset's own sourced letter | Geometry residual `span`, and the upper bound of the span window (`AlasConfig::design_envelope`) | Aerodrome operating constraint from ICAO Annex 14 Vol. I Table 1-1 (wingspan bands A <15 m, B 15-<24, C 24-<36, D 36-<52, E 52-<65, F 65-<80; the band is open at the top, so a design keeps one centimetre below the edge). Do not derive a code from an ICAO identifier or runway length; the letter is a study input or a sourced preset value. |
| `max_approach_speed_kt` (kt) | `0` disables; explicit override | Performance residual `approach_speed` | Operating/aerodrome constraint. Keep in route/airport performance settings with the selected category/basis; it is not a universal aircraft requirement. |

The `optimizer/plausibility.rs` limits are already in the appropriate broad
group. They are validity-domain windows for aspect ratio, fuselage fineness,
tail-arm fraction, chord ratios, thickness ratio, washout and planform order.
They protect the correlations and geometry builder; they are not
certification requirements. Tail-volume windows and clean-sheet body-angle
windows in `mdo/residuals_geometry.rs` are preferences/study bounds and should
remain labelled as such.

## Residual consumer map

`crates/alas-opt/src/mdo/residuals.rs` is the canonical optimizer residual
assembly. `crates/alas-pipeline/src/feasibility.rs` repeats selected physical
checks after a run so a report can distinguish a candidate that was ranked from
one that is dispatch-feasible.

| Family | Residuals or checks | Physical/numerical meaning |
|---|---|---|
| Mass | `structural_inventory_unverified`, `fuel_capacity`, `fuel_capacity_declared` (reference adaptation only: modelled tank capacity against the nominal aircraft's modelled capacity, so the volume model's bias cancels; the published usable fuel is reported as diagnostic context), `mtow_ceiling`, `mtow_band_upper`, `mtow_band_lower`, `landing_mass`, `offdesign_fuel_capacity`, `offdesign_payload`, `offdesign_tow`, `dispatch_*`, `sizing_not_closed` | Structural inventory evidence, tank capacity, MTOW/landing limits and closure status. `mtow_ceiling` belongs to `fixed_requirement` and `sized_by_mission`; `unconstrained` and `payload_adjusted` declare no ceiling. `mtow_band` replaces it with the hard pair: required takeoff mass against `T (1 + p)` and closed mass against `T (1 - p)`, with no pull toward `T` inside the band. A design-mission closure flies the route off-design at the closed mass: reserve-inclusive takeoff fuel against usable capacity, route payload against the derived design structural payload (derived MZFW minus OEW), and route takeoff mass against the closed MTOW. |
| Balance | `static_margin_floor`, `forward_cg_range`, `nose_gear_strength`, `main_gear_strength`, `min_nose_gear_load`, `cg_model_error` | Loading-envelope and gear reactions, gated per loading state by `PhaseLimits`: rotation and the static-margin floor at takeoff, landing trim and ground mechanisms at landing, ground mechanisms at OEW. The rotation limit is the nose-wheel lift-off balance about the main-gear contact (Sadraey, *Aircraft Design: A Systems Engineering Approach*, Wiley 2012, sec. 9.6.2 eqs. 9.36-9.54a and sec. 12.6 eqs. 12.55-12.76) at the stall-branch VR (1.10 VS) with TOGA thrust and runway friction (mu 0.02 [E]). The pitch inertia is transferred to the contact, I_P = I_yy,cg + m[(x_P - x_cg)^2 + h_cg^2] (eq. 9.53), with I_yy,cg from the mass ledger's takeoff state (Raymer's jet-transport radius 0.38 L/2 when no ledger exists), so the balance is quadratic in the CG and is solved exactly (`alas_opt::envelope::rotation`). The required pitch acceleration is 7 deg/s^2 for every class, the midpoint of the Torenbeek/Roskam 6-8 deg/s^2 range [E] (Sadraey's class values were not used: his Table 9.6 could not be confirmed from an accessible source); `landing_gear.rotation_pitch_acceleration_deg_s2` and `landing_gear.pitch_radius_of_gyration_frac_mac` are optional overrides (unset = the class value and the mass-ledger radius). The tail lift at full up-elevator is derived from the geometry, CL_h = a_h [i_h - epsilon + tau_e (b_e/b_h) delta_e,max] with the fuselage level: DATCOM a_h of the built tail, its built incidence, the wing downwash 2 CL_g/(pi A) reduced by Wieselsberger's ground-effect factor at the wing height [E], the thin-airfoil effectiveness of the configured elevator chord times the empirical plain-flap large-deflection correction (USAF DATCOM section 6.1.1.1 / Raymer; 0.60 at 25 deg [E], anchors other than 25 deg not digitised from the figure) times its span fraction, and delta_e,max = -25 deg (Sadraey Table 12.3; one class-generic documented assumption for every aircraft [E]). The linear download is then limited to the tail section's stall, 0.9 c_l,max cos(Lambda_c/4) (Raymer) with c_l,max of the symmetric NACA tail section (Abbott and von Doenhoff, lower end of the scatter [E]); the cap does not bind on the registered presets. No take-off stabiliser trim beyond the built incidence is credited; the registered presets land at CL_h -0.85 to -0.97 (the earlier thin-airfoil upper bound gave -1.22 to -1.42, which made the gate effectively inactive). Against the published forward limits (checks, never calibration targets) the rotation limit is A320 9.2 vs 17, A220 -3.0 vs 18.4 (certified) and A340 28.3 vs 20.3 %MAC: the A340 remains a known residual (its published-limit test is ignored with that residual). Drag is omitted (about 1 % MAC or less; nose-up and conservative to omit when the drag line is above the CG). `min_nose_gear_load` uses the 6 % class steering minimum unless the registered preset carries its airport-planning document's nose share at the most-aft CG against weight (`AircraftReferenceData::aft_cg_nose_load`; Airbus 7-3-0 weight-variant rows, Boeing and A220 section 7.4 ground-envelope vertices). Each state reads the share at its own weight: linear between published points, the lightest point's share held below them (the share fixes the aft CG station and no source prints a further-aft limit at lower weight), and the heaviest point's main-gear load as a floor. The smallest published share lowers the steering minimum. Declared ranges: A220-300 5.1-6.9 % (36.3-68.0 t), A320 5.0-7.1 % (66.4-78.4 t), A340 5.3 % (254-261 t), A380 4.9 % (512-562 t), B787-9 4.3-7.8 % (111-255 t), DC-10-30 5.7-8.7 % (119-264 t); ATR and AVE keep the class default (no printed data). CG heights for tip-back are measured from a ground plane that hangs the belly clearance below the lowest point of the fuselage lower contour. The registered design vector keeps its published gear stations; every redesigned candidate (not in `baseline_sandbox`) has its main-gear group translated rigidly, nose gear fixed, to the station nearest the published one that meets tip-back, the nose-load window at every loading state and nose-wheel liftoff where the envelope gates it (maximum-fuel takeoff), with each wing leg under the wing chord at its own spanwise station and each body leg (the centre leg of a three-leg group, the inboard pair of a four-leg one) inside the fuselage, since a body gear reacts into the keel behind the wing box (the DC-10-30 centre gear stands behind the trailing edge of the centreline chord) (`alas_opt::envelope::place_main_gear`); the candidate is then re-sized with that station, starting from the previous closure's trim when its centre of gravity moved by no more than `retrim_cg_tolerance_pct_mac` (the closure's own re-trim rule), re-checked there and re-solved once from the re-sized closure if the secant missed, and the unchanged envelope gives the verdict. The registered nominal of every relative guard is resolved without the translation. Reports, figures, exports and the stage-6 verdict read the translation from the report (`main_gear_translation_m` in the geometry summary, `ResolvedProductState::main_gear_placement`), and a saved delivered configuration carries it. No station exists when the requirements conflict. On the ATR72-600 tip-back and nose-wheel lift-off conflicted while the rotation tail lift was a fixed -0.55; with the derived tail lift a redesigned ATR candidate gets a station that meets every mechanism. Model errors must remain evidence gaps, not finite passes. |
| Performance | `oei_second_segment`, `cruise_thrust`, `takeoff_field` (at the design gross mass, MTOW), `landing_field` and `approach_speed` (at the design landing mass, MLW), `mission_profile_range`, airport/mission evidence flags | Performance constraints and data availability. Unsupported engine counts are diagnostic/soft rather than silently treated as a Part 25 pass. |
| Geometry | `span` (below the aerodrome reference code letter's wingspan limit, ICAO Annex 14 Table 1-1), `panel_washout_max`, `root_to_kink_te_angle` (exposed edge), `wing_area`, `wing_loading`, body-angle window, `sweep_consistent_with_cruise_mach`, plausibility windows, cargo target deviation and passenger shortfall | Aircraft bounds, preferences and model validity. `sweep_consistent_with_cruise_mach` bounds the Korn/Lock wave drag at the drag-divergence value (dCD/dM = 0.1, Raymer) at the sizing cruise Mach and altitude and the lift of one mid-cruise mass (the start-of-cruise state, at the loaded takeoff mass with the climb burn not deducted, is computed beside it and shown in the residual detail with its CL and wave CD, but is not gated: mid-cruise is the design point the cruise checks and the fuel burn already use, and the start is its heavier upper bound), defined once for every MTOW mode (`alas_opt::mdo::cruise_mass`): the design mission flown from the candidate's takeoff loading, TOW - (F - R)/2, with F the fuel at brake release and R the fuel the closure's dispatch keeps on landing (reserves, extra and taxi-in; a trip-share contingency is rescaled to the longer trip, every other reserve keeps the dispatch value). Under `fixed_requirement` (Hard MTOW) the loading is the maximum the weight and volume budgets admit, so the state lies between ZFW + R and TOW of that one mission; under the mission-closed modes it is the closed mission's own mid-cruise mass. |

Preset wing, engine and fin heights come from the airport-planning ground
clearances at MRW and aft CG, so the configured wing heights are the static
**ground shape**. `geometry.wing.flight_tip_rise_semispan_fraction` adds the
static-to-1 g tip rise of the **flight shape**, spread as one uniform dihedral
increment from the centreline (`alas_config::WingShape`). The builder lofts
the flight shape by default: the lattice, the dihedral effect, the layout
dihedral check and the wingbox layout read it. Ground clearance, nacelle strike
and the FLOPS main-gear oleo dihedral term read the ground shape. Only the
A380 declares a rise (Airbus Facts and Figures: over 4 m at take-off); the
other presets carry one shape, so their in-flight dihedral and roll stability
stay understated where their heights are ground-fitted (A320, A340).

The fin root is attached to the body. `geometry.empennage.vstab_z_m` is the
line the drawn fin height and root chord are measured from (usually the
fuselage top line at the fin); the builder continues the fin trapezoid's
straight edges down, or trims them up, until the root meets the lowest point
of the top of the fuselage, or of a centreline nacelle, under its root chord.
No part of the root then floats above a tapering tail cone, and the tip stays
at its drawn height. The published fin areas are those of the trapezoid to its
stated reference line (A320 21.5 m^2 above the fuselage top line, A220
28.2 m^2 to the fuselage axis); the built fin, which every mass, stability
and lattice consumer reads, spans from the tail cone, between those two
reference lines.

## Engineering-source qualification

The applicable certification basis is a project choice and must be recorded;
the code currently has no authority/amendment/category/operating-condition
fields. The current EASA certification-specification index lists **CS-25
Amendment 28** (published 19 December 2023, file corrected 20 November 2025):
[EASA CS-25 Amendment 28](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28).
The EASA online Easy Access Rules page available to the code audit is a
consolidated January 2023 publication and carries an explicit disclaimer:
[EASA Easy Access Rules for Large Aeroplanes](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25).
Use the selected official amendment and certification basis for qualification,
not a bare paragraph number in a default help string.

The relevant source implications are:

* CS 25.25 establishes maximum weights for the applicable operating,
  environmental and loading conditions, including zero-fuel weight, CG and
  distribution. This supports keeping `max_structural_payload_kg` separate
  from MTOW and deriving it only from an evidenced MZFW/OEW/load case.
* CS 25.303's 1.5 factor and CS 25.337 load-factor limits are part of a
  specified certification basis and aircraft category. A shipped `3.75`
  (`1.5 × 2.5`) is a screening default, not a universal value for every
  aircraft or amendment.
* CS 25.335/AMC defines design-speed substantiation with multiple envelope and
  Mach/altitude considerations. `V_C = V_D / 1.25` is an explicit project
  screening assumption; it is not sufficient by itself to claim certification
  compliance.
* FAA [AC 120-27F](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1035868)
  is active operator weight-and-balance guidance issued 2019-05-06. It
  describes acceptable methods for W&B programs and average/estimated/actual
  weights; it does not establish a universal 100 kg passenger mass. The
  project's 100 kg value may remain a transparent load-case default only when
  the run records it as an assumption and does not label it as a certification
  constant.
* ICAO aerodrome reference-code span and approach-category limits require a
  selected aerodrome design/operating basis. The current `Airport` and
  `CustomAirport` values contain runway distances, elevation, temperature,
  position, name and ICAO identifier, but no sourced reference-code letter or
  approach category. A route-derived span/approach limit is therefore not
  implementable from the present schema without inventing data.

These sources qualify the model inputs; they do not constitute a complete
certification analysis. Numerical implementation checks and physical
validation remain separate deliverables.

## Reorganization plan

1. Keep the existing serialized `DesignRequirements` fields for compatibility
   during the next schema version. Add a canonical run snapshot containing
   `source_kind` (`default`, `preset`, `user`, `derived`), value, unit,
   authority/amendment/category where applicable, source URI/hash, and the
   consumer/residual IDs.
2. Move `passenger_mass_kg` into a passenger/baggage load-case basis and
   `gravity_m_s2` into atmosphere/physics settings. Move the structural speed
   and load-factor trio into a structures/performance certification-basis
   group. Keep aliases/read migration for old files.
3. Move objective targets and all iteration/tolerance/penalty fields into
   optimizer settings. Move span and approach limits into route/aerodrome
   constraints once the airport schema contains a sourced reference code,
   approach category, and validity date. Until then, require explicit values
   and preserve `0 = disabled` semantics.
4. Add field-level finite/domain validation for the currently unvalidated
   requirement scalars (mass, gravity, area, loading, CL, margins, and load
   factors). Reject invalid values at config normalization, before a model can
   convert them into NaN or an inverted residual.
5. Add consumer-contract tests: every retained requirement must appear in the
   canonical snapshot and at least one named evaluator; every moved setting
   must be absent from the design-constraint presentation; each default and
   preset override must have an assertion and a source/basis record.
6. Keep implementation, numerical verification, and physical validation
   separate in reports. A passing residual or parser test proves code behavior;
   it does not prove a design complies with a selected certification basis.

## Verification boundary

The audit used repository search and source inspection. It did not run a full
aircraft optimization or a certification substantiation. Figure-specific
implementation and numerical checks are recorded with the tests that own them.
The remaining human decision is the certification basis and the
airport/route data contract: without those, moving the fields and assigning
universal defaults would create false traceability.
