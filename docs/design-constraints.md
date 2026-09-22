# Design-constraint audit

Audit date: 2026-09-19. The audit covers the configuration fields that are
presented as design constraints, their defaults and preset overrides, and the
code that consumes them. It also records the boundary between an aircraft
requirement, a physical-model input, a numerical setting, a preference, and a
validity-domain limit. The source tree had unrelated uncommitted work while
this audit was performed; references below identify the current consumers by
path rather than by a commit hash.

The current implementation has a useful policy split in
`crates/alas-config/src/optimizer.rs`: objective settings, solver settings,
design-space bounds, plausibility limits, and relaxation policies are separate
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
| `mtow_kg` (kg) | `358670`; editable; reference presets may provide a value; `MtowSizing` chooses fixed, mission-sized, or seed-only use | `mdo::sizing`, `mdo::mda`, `mdo::residuals`, feasibility, mass/mission/report paths | Requirement/weight limit. Retain as a declared limit or sizing seed, but always expose the selected sizing policy. It is not a structural-payload value. |
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
| `mtow_sizing` (-) | `SizedByMission` | MDA closure and mass residuals | Sizing policy. Keep in solver/objective settings; it determines whether `mtow_kg` is a ceiling, fixed value, or seed. |
| `sizing_max_iterations` (-) | `30` | Mission/MTOW fixed-point closure | Numerical setting. Move under solver settings. |
| `sizing_tolerance_kg` (kg) | `1` | Closure status and `sizing_not_closed` | Numerical setting. Move under solver settings. |
| `retrim_cg_tolerance_pct_mac` (% MAC) | `0.1` | MDA re-trim/re-evaluation | Numerical/model-update trigger. Move under solver settings and keep its unit explicit. |
| `mass_constraints`, `balance_constraints`, `performance_constraints`, `geometry_constraints` (-) | all `Hard`; user policy overrides | `mdo::residuals::build`, cost assembly and policy review | Evaluation policy, not a physical limit. Keep in optimizer policy settings; every residual must retain its family and policy in the run snapshot. |
| `max_span_m` (m) | `80`; `0` disables; explicit override | Geometry residual `span` | Route/aerodrome operating constraint. The help text describes ICAO reference code limits, but the current `Airport`/`CustomAirport` schema stores no reference code letter. Do not derive a code or 80 m from an ICAO identifier or runway length; keep this as an explicit study input until the airport schema carries a sourced code. |
| `max_approach_speed_kt` (kt) | `0` disables; explicit override | Performance residual `approach_speed` | Operating/aerodrome constraint. Keep in route/airport performance settings with the selected category/basis; it is not a universal aircraft requirement. |
| `soft_penalty_weight` (-) | `10` | Scalar cost assembly | Numerical ranking coefficient. Move to optimizer cost settings; never report it as a feasibility limit. |

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
| Mass | `structural_inventory_unverified`, `fuel_capacity`, `mtow_ceiling`, `landing_mass`, `dispatch_*`, `sizing_not_closed` | Structural inventory evidence, tank capacity, MTOW/landing limits and closure status. `Unconstrained` intentionally omits the MTOW ceiling after the seed pass. |
| Balance | `static_margin_floor`, `forward_cg_range`, `nose_gear_strength`, `main_gear_strength`, `min_nose_gear_load`, `cg_model_error` | Loading-envelope and gear reactions. Model errors must remain evidence gaps, not finite passes. |
| Performance | `oei_second_segment`, `cruise_thrust`, `takeoff_field`, `landing_field`, `approach_speed`, `mission_profile_range`, airport/mission evidence flags | Performance constraints and data availability. Unsupported engine counts are diagnostic/soft rather than silently treated as a Part 25 pass. |
| Geometry | `span`, `wing_area`, `wing_loading`, body-angle/tail-volume windows, plausibility windows, cargo target deviation and passenger shortfall | Aircraft bounds, preferences and model validity. Tail-volume under a hard family is intentionally demoted to soft because it is a preference. |

The objective-cost path in `mdo/objective_evaluate_cost.rs` still contains
legacy passenger/cargo shortfall terms in addition to the typed residual table.
The values should be reconciled in a later migration so the canonical residual
ledger cannot be charged twice. Until then, the run snapshot should identify
which residuals entered the scalar cost.

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
implementation and numerical checks are reported separately by the owning
agent. The remaining human decision is the certification basis and the
airport/route data contract: without those, moving the fields and assigning
universal defaults would create false traceability.
