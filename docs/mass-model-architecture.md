# Mass-model architecture: definitions, ownership and sizing basis

This is the durable description of how ALAS asks and answers the three
different mass questions, which item owns every kilogram of the ledger, and
where each design weight comes from. The equation-level description of the
NASA FLOPS transport port is [`flops-mass-model.md`](flops-mass-model.md);
the evidence behind the registered aircraft inputs is
[`pure-flops-aircraft-evidence.md`](pure-flops-aircraft-evidence.md). The
reproducible experiment matrix behind the numbers quoted here is

```sh
cargo run -p alas-pipeline --release --example mass_experiment_matrix -- \
    outputs/mass-model-consolidation/<label> [--presets A320-200,A220-300] [--no-missions]
```

and its results, before and after the corrections of 2026-09-12, are under
`outputs/mass-model-consolidation/`. The self-contained report is kept in a
local, git-ignored reports directory.

## One production architecture

The architecture's serialized name is retained for compatibility. Its current
equation set includes declared LTH cabin/pylon relations and, for shaft-power
aircraft, GASP/TM-83458 installation relations. The completed buildup and item
ledger identify those sources; the name alone does not establish NASA-only
equations or physical validation. The current robustness contracts and OEW
comparison limits are in [`flops-robustness.md`](flops-robustness.md).

`MassModelConfig::mass_architecture = pure_flops_transport_v1` owns every
production mass group end to end: screening, optimization, MDA closure, the
final analysis, the payload layout, the item ledger, CG and inertia, the
performance stages, reports, the GUI and the exports all consume the one
`FlopsMassBuildup` that `run_product_mass_analysis_with_groups` returns. The
legacy Torenbeek/fraction buildup is reachable only by selecting
`legacy_reference_compatible_comparison` by name through the explicit
`*_reference_compatibility` constructors, and nothing falls back to it. A
missing FLOPS input is a typed `FlopsUnverified` blocker. The strength-sized
wing box is a feasibility diagnostic beside the ledger; it never replaces
the FLOPS wing (see "Structural diagnostic" below).

## The three questions and the sizing basis

The FLOPS equations size the wing, tails, fuselage, surface controls and
pod relief at a design gross mass `DG`, and the landing gear at a design
landing mass `WLDG`. Which mass those are is the whole difference between
the three questions, so it is an explicit state variable:
`alas_config::MassSizingBasis`, resolved by `AlasConfig::mass_sizing_basis()`
from the design mode.

| Question | `DesignMode` | `MassSizingBasis` | `DG` | `WLDG` | What a mission changes |
|---|---|---|---|---|---|
| A. Fixed-aircraft mass estimation | `BaselineSandbox`, `ReferenceAdaptation` | `FixedAircraft` | declared `requirements.mtow_kg` | the preset's declared MLW, else `mlw_fraction_mtow x DG` | nothing: OEW is a property of the aircraft |
| B. Fixed-aircraft mission evaluation | same | `FixedAircraft` | same | same | fuel, dispatch mass, landing mass, CG and trim only |
| C. Coupled new-aircraft sizing | `CleanSheet` | `Coupled` | the closed takeoff mass of each pass | `mlw_fraction_mtow x DG` | every component follows the closure |

A declared `flops_structure.design_gross_mass_kg` pins `DG` in any mode; it
is the user's statement that the structure was designed for a heavier weight
variant than the MTOW in use. `flops_structure.design_landing_mass_kg` pins
`WLDG` the same way.

`AlasConfig::at_closure_mass(closure_mass_kg)` is the one seam that
evaluates the ledger at a takeoff mass other than the requirement: it writes
the closure mass into `requirements.mtow_kg` (what the fuel remainder, the
payload layout and the trim read) and, under a fixed-aircraft basis, writes
the declared `DG`/`WLDG` into the structure overrides first. The
mission-sized MDA loop (`alas_opt::mdo::mda`), the sized final report
(`FullAnalysis::run_at_sized_takeoff_mass`) and the structural load cards of
the pipeline all go through it. `SizedCandidate` reports `sizing_basis`,
`design_gross_mass_kg` and `design_landing_mass_kg` next to the takeoff mass
so a result always says which aircraft its ledger belongs to.

### What was wrong before

Until 2026-09-12 every MDA pass rewrote `requirements.mtow_kg` to the
dispatch iterate and re-ran the whole FLOPS buildup at it, in every design
mode. A registered aircraft flown on a short route therefore became a lighter
aircraft with the same name: on the baseline tree the B787-9 operating empty
mass in `BaselineSandbox` was 103,933 kg on a 500 nmi mission, 105,462 kg at
2,000 nmi and 112,048 kg at its 7,635 nmi design range, against 112,867 kg
at the declared MTOW. The `MtowSizing::Unconstrained` probes of 2026-09-11
(A380-800 370,695 kg, A340-300 199,883 kg, B787-9 201,629 kg, DC-10
210,050 kg) were this coupled closure, run in `CleanSheet` mode on the
presets' operational airport pairs (2,900 to 4,200 nmi), with a re-derived
cabin and no cargo; they are not fixed-aircraft results and they are not a
prediction of the published MTOW.

After the correction the same B787-9 sweep gives 112,867.41 kg at every
range, identical to the single-pass declared-MTOW ledger, and the coupled
clean-sheet mode still lets the wing follow the closed mass (regression tests
`a_fixed_aircraft_keeps_its_component_ledger_under_mission_only_changes` and
`a_sized_report_of_a_fixed_aircraft_keeps_the_declared_design_weights`).

Case C on a registered design vector is a *new* aircraft: the clean-sheet
fuselage is re-solved from the cabin, the landing mass is the fraction of the
closed mass and the wing inventory is the enumerated clean-sheet list. Its
operating empty mass must not be compared with the registered aircraft's
published one.

## One cabin per case

The registered aircraft carry two cabins: a planning-cabin seed
(`planning_cabin_config`, length shares and seat pitch) that the product
layout fills to the exit-limited floor capacity, and a FLOPS class split in
`preset_flops` chosen earlier from published typical layouts. On the baseline
tree the report path priced the FLOPS furnishings, passenger service, crew
and air conditioning for the second cabin while the zero-fuel mass carried
the first (A320-200: a 150-seat 12F/138Y operating empty mass under a
180-seat payload; A380-800: 525 seats under 663).

Now the cabin the layout seats is the cabin the ledger prices:

* `FullAnalysis::run` re-derives the FLOPS class counts and passenger total
  from the seated layout before the second mass pass
  (`full_analysis/cabin_sync.rs`); the optimizer path already did this in
  `apply_candidate_payload_load_case`.
* A cabin declared by count (`cabin.passenger.class_mix_mode = "count"` with
  per-class seats and seat geometry) is an input in the product path: the
  layout seats exactly those classes and reports what does not fit, and the
  first mass pass prices the declared split. This is how a reconstructed
  reference case (a 12F/138Y A320, a 140Y A220) is run without editing the
  registry.
* Percent-share cabins keep the product contract: capacity is dynamic and
  exit-limited, and the registered `num_passengers` is a seed.

The consequence for the published source-matched splits is honest: the
A320-200's 12F/138Y and the B787-9's 28C/262Y are declared study cabins that
the product's percent-share layouts do not reproduce (they seat 180Y and
42C/260Y respectively); the numbers under those splits are available as
declared-count cases, not as the preset's production result.

## Passenger and baggage mass

`requirements.passenger_mass_kg` (100 kg, the shipped project load-case
default) is the single product load-case authority: every seated passenger,
of any class, costs the zero-fuel mass that combined figure. FAA AC 120-27F
is operator weight-and-balance guidance and does not establish a universal
passenger mass; an operational value must carry its operator, population,
baggage method and date. `cabin.passenger.checked_bag_mass_kg`
(16 kg) is the baggage share the layout charges per seated passenger, and the
per-class occupant slot is the derived remainder (84 kg). The one seam that
applies it is `PassengerCabinConfig::apply_passenger_mass_authority`, called
by the product entry point `build_payload_layout` after a cabin preset or a
declared cabin has written its seat geometry, and by the optimizer's
`apply_candidate_payload_load_case`; the report, GUI preview, pipeline,
export, acceptance and optimizer paths therefore price the same seat the
same way. Per-class `mass_per_pax_kg` values are geometry seeds, not
inputs, on the product path; the reference-compatibility paths never call
the resolver, so the frozen Python parity fixtures keep their historical
per-class masses. Explicit user inputs are preserved: the combined mass, the
bag share, `belly_cargo_kg` (revenue freight, exactly as requested and capped
by the hold), declared class counts and geometry, and FLOPS
`containerized_cargo_kg` (container tare, an operating item inside OEW,
never payload). Before this decision the report path carried a class premium
(90/96 kg occupants in business/first) worth about 250 kg on a 787-9 with 42
business seats and 590 kg on a 663-seat A380; the optimizer path had earlier
charged checked bags twice (116 kg per seat).

## The OEW reference registry

`alas_config::oew_reference` is the one place a published operating empty
mass lives. Every registered preset has exactly one record; every report,
harness and example that prints a reference OEW reads it (the presets'
`reference.oew_kg`, the experiment matrix, `flops_preset_comparison`, the
acceptance and parity tests). A record separates the aircraft the number
describes (model, weight variant, MTOW, engine, modification state, cabin)
from the registered preset, states what the number includes (crew, baggage,
unusable fuel, oil, catering, water, containers, manuals: included, excluded
or unknown), and carries the document, revision, locator, URL or local copy,
retrieval date and quote. Its `applicability` decides use:

| Applicability | Meaning | Enters a validation metric |
|---|---|---|
| `configuration_matched` | same aircraft and cabin, inclusion list stated | yes (none today) |
| `conditional_mismatch` | same model and weight variant, a stated cabin or definition difference | no; visible conditional comparison |
| `source_gap` | nothing published for the exact configuration; a case anchor for another configuration may exist and is compared only through the named reconstructed case | no |
| `unsupported_model` | a value exists but the model cannot evaluate the aircraft | no |
| `not_applicable` | notional design | no |

Current records (2026-09-12): A220-300 37,149 kg (Airbus recovery
publication planning OEW with a stated inclusion list; the ACP gives
37,081 kg for the same 140-seat cabin; conditional on the 145-seat product
cabin); A340-300 131,215 kg (the "OEW" printed on the ACAP jacking figure,
weight variant and cabin unstated, printed pound value inconsistent);
DC-10 120,914 kg (ACAP Series 30 passenger column with the 572,000 lb
footnote applied, exactly the preset's weights); ATR72-600 13,450 kg
(factsheet, model unsupported); A320-200, A380-800 and B787-9 source gaps
whose anchors (41,052 kg operator sheet for a 77 t / 180Y / wingtip-fence
aircraft; 277,000 kg aggregator typical three-class; 128,850 kg attributed
to a superseded Boeing revision) are compared only through their
reconstructed cases; AVE not applicable. The former A320 41,244 kg is absent
from the cited Airbus document and is recorded as an unsourced value that
only documents the declared structural payload. No record is configuration
matched, so no validation metric exists; every OEW comparison in this
repository is conditional.

## Accounting crosswalk and ownership

Every kilogram has exactly one owner. Payload and fuel are ledger closure
terms, never part of OEW.

| Item | FLOPS term | ALAS owner | Ledger slot / row | Basis |
|---|---|---|---|---|
| Wing (bending, shear and controls, miscellaneous) | eqs. 10-45 | `flops_transport::structure` | `wing` | `DG`, `ULF`, planform, `SFLAP` |
| Horizontal and vertical tails | eqs. 46, 50 | structure | `h_stab`, `v_stab` | `DG`, tail areas |
| Fuselage | eq. 56 | structure | `fuselage` | length, width, depth |
| Paint | eq. 68 | structure | `fuselage` | wetted area, `WPAINT` (0 by default) |
| Nose and main gear | eqs. 63-67 | structure | `gear` (`nose_gear`, `main_gear` rows at 15/85) | `WLDG`, oleo lengths |
| Nacelles | eq. 69 | structure group, charged once to propulsion | `propulsion` | thrust, nacelle geometry |
| Engine term `WENGP`/`WENG` | eqs. 75-80 | propulsion | `propulsion` | declared `WENGB` (the certified dry engine mass of the type-certificate data sheet, where its stated scope is the basic engine with accessories: A320 CFM56-5B4/3 2,454.8 kg, A220 PW1521G-3 2,177 kg, A380 Trent 970-84 6,246 kg) or the `THRSO/5.5` correlation where no such scope is stated (A340: the CFM56-5C dry weight contains its adapter/reverser; B787: the GEnx data sheet lists the fan reversers under the engine type design without a split; DC-10: no retained CF6-50C value). Certified A320/A220/A380 aircraft-side exhaust/EBU scope is recorded as `OutsideUnmodelled`; no Eq. 78 mass is invented without a separable source. Starter overlap is explicit per preset and remains conservative when unresolved. See `preset_flops/structure.rs` |
| Inlet, nozzle | eqs. 77-78 | propulsion | `propulsion` | declared baselines only; otherwise inside `WENG` |
| Thrust reversers, engine controls, starters, misc. | eqs. 86, 87, 89 | propulsion | `propulsion` | thrust, `VMAX`, nacelle diameter |
| Fuel system | eq. 92 | propulsion | `propulsion` | `FMXTOT`, engines, `VMAX` |
| Pylons, mounts, EBU, fluids | none | **unresolved installation scope** | not represented | source gap; not invented |
| Surface controls, APU, instruments, hydraulics, electrical, avionics, air conditioning, anti-ice | eqs. 97, 101-113 | `flops_transport::equations` | `systems` (nine rows) | geometry, `DG`, crew, passengers, `VMAX`, range |
| Furnishings | eq. 110 | systems group | `furnishings` (once) | crew, class counts, compartment |
| Empty-mass margin | eq. 139 | `flops_methods` | `systems-empty_mass_margin_and_residual` | fraction of structure + propulsion + systems |
| Flight crew and baggage | eq. 115 | operating items | `operating-flight_crew` | `NFLCR` |
| Cabin crew, galley crew and baggage | eq. 116 | operating items | `operating-cabin_crew` | `NSTU`, `NGALC` |
| Unusable fuel | eq. 121 | operating items, placed per tank | tank rows | `FMXTOT`, tanks, engines, `SW` (a correlation, 165 kg on the A320; EASA lists 65.7 kg) |
| Engine oil | eq. 122 | operating items | `operating-engine_oil` | engines, thrust |
| Passenger service (catering, water, supplies) | eq. 123 | operating items | `operating-passenger_service` | class counts, range, `VMAX` |
| Cargo containers | eq. 126 | operating items | `operating-cargo_containers` | `WCARGO` (0 declared on every preset) |
| **Operating empty mass** | eq. 141 | `OEW_KEYS` sum | structure + propulsion + systems + furnishings (incl. operating items) | |
| Passengers and checked bags | | payload layout | `payload` rows | seated count, 84 + 16 kg per seat (100 kg combined) |
| Belly / revenue cargo | | payload layout | `payload` rows | `belly_cargo_kg`, capped by `max_structural_payload_kg` |
| Containerised cargo mass | `WCARGO` | `flops_transport` | container tare only | never substituted from `cargo_payload_kg` |
| **Actual ZFW** | | ledger | OEW + payload | compared with the reference `mzfw_kg`, never clamped to it |
| Required mission fuel, reserves | | `alas_mass::dispatch` + fuel policy | dispatch solution | EASA basic scheme by default |
| Usable tank capacity | | preset reference / tank layout | `usable_capacity_kg` | published capacity at the source density |
| MTOW fuel headroom | | ledger closure | `fuel` = `mtow_kg` - OEW - payload | a budget, not a burn |

`WENGB` is an engine-term boundary: on the generic default branch it includes
the inlet and nozzle because they are not declared. Certified preset scope
controls can instead mark known aircraft-side exhaust/EBU hardware as
`OutsideUnmodelled`; the corresponding Eq. 78 split is still absent and no
mass is guessed. Pylons, mounts, installation hardware and fluids are outside
every FLOPS term and are recorded as unresolved scope, not as an all-in pod.

### Sources of the design quantities

| Quantity | Source in the product | Note |
|---|---|---|
| `DG` | `MassSizingBasis` (above) | declared MTOW for a fixed aircraft, closure mass for a clean sheet, or a declared override |
| `WLDG` | preset `reference.mlw_kg` in fixed modes; `mlw_fraction_mtow` (0.92) x `DG` otherwise; declared override wins | on the baseline tree the report path used 0.92 x MTOW for every preset because presets load in `CleanSheet`; the A320 gear was 272 kg heavier than at its certified 66,000 kg |
| Ultimate load factor | `requirements.ultimate_load_factor` = 3.75 | Screening value (`1.5 x 2.5`); verify certification basis, amendment, category and load case; no aircraft-specific design load case is retained |
| Dive speed | `requirements.dive_speed_m_s` | Screening/Torenbeek strength input and V-n value; speed type, altitude/Mach envelope and certification basis must be recorded |
| Fuel density | preset `reference.fuel_density_kg_l` (source table) | A320: 0.800 kg/L (EASA table); the Airbus common convention 0.785 kg/L is a different bookkeeping |
| Usable fuel | preset `reference.usable_fuel_mass_kg` | `FMXTOT`; the A320 tank layout sums to the 24,209 L common table while the selected MOD table is 24,167 L (42 L recorded mismatch) |
| Unusable fuel | eq. 121 correlation | physical inventory (EASA 65.7 kg on the A320) is not what the equation returns |

## Structural diagnostic

The strength-sized primary wing box (`alas_struct::sizing`) is reconciled
with the FLOPS wing in `wing_reconciliation`. In the reference modes the
frozen empirical wing is the FLOPS wing and the box is subtracted from it.
On four presets (A320-200, A340-300, A380-800, DC-10) the doubled semi-wing
box exceeds the whole FLOPS wing (A320: 12,533 kg box against 7,913 kg wing)
and the reconciliation used to abort the candidate with an opaque
`structural_sizing` error, which made the fixed-aircraft mode unusable on
them. It now publishes the empirical wing unchanged, carries the box beside
it, and marks the inventory `ReferenceExceededBySizedBox` so the
`structural_inventory_unverified` residual carries the finding. The
structural model's loads, materials and gauges on those aircraft are a
separate open item; nothing in the mass ledger depends on them.

## Fixed-aircraft mission checks

`wing_loading` is a design wing loading (the requirement help text says
MTOW over area) and is evaluated on `design_gross_mass_kg`, so a fixed
aircraft dispatched light on a short route is not penalised for it. The
landing-mass limit of a fixed aircraft is its sizing basis's
`design_landing_mass_kg` in every `MtowSizing` mode; only a coupled
clean-sheet closure recomputes it from the takeoff-mass iterate. For a fixed
aircraft whose mission closes below the declared MTOW, `Unconstrained` and
`SizedByMission` are the same evaluation (takeoff mass, ledger, fuel and
landing limit; regression tests in `alas-opt/tests/mission_sized.rs`); a
preset without a declared MLW (AVE) keeps `mlw_fraction_mtow x DG` rather
than following the dispatch mass. The report's field-performance stage
reads the same basis. The
`mission_profile_range` residual on short operational routes (A220-300
LEMD-LEPA) is intentional: the configured climb and descent profile does not
fit in the route.

## Validation status (2026-09-12)

* Implementation correctness: the independent NASA/Aviary equation replay,
  run on a deck and an ALAS record generated from the same tree
  (`outputs/oew-evidence-closeout/aviary-parity/`), closes all 34 A320-200
  rows to 1.1e-13 kg and all 34 A220-300 rows to 0.0 kg with the declared
  engine masses. The 0.23 kg A220 wing difference reported on 2026-09-12
  morning was a frozen record compared with a later deck, not a drift.
* Numerical consistency: fixed-aircraft ledgers are invariant under mission
  changes (sweep spread 0 kg on every preset); `Unconstrained` and
  `SizedByMission` evaluate the same fixed aircraft; the sized final report
  and the MDA loop use one seam; ledger closure OEW + payload + fuel = MTOW
  holds to 1e-6 kg.
* Physical predictive accuracy: not established, and every comparison is
  conditional (registry section above). After the certified engine-mass
  input (A320 +456 kg, A220 +730 kg, A380 -134 kg): the A320 reconstructed
  77 t / 180Y / wingtip-fence case gives 40,149 kg against the 41,052 kg
  operator sheet (-903 kg, -2.2 %, inclusion list unknown, fence span not
  applied); the A220-300 holdout gives 34,820 kg at the product 145Y cabin
  and 34,665 kg at the declared 140Y cabin against the Airbus planning OEW
  37,149 kg (-6.3 % and -6.7 %); the A340-300 gives 111,952 kg against the
  ACAP jacking-figure 131,215 kg (-14.7 %; the declared typical 335-seat
  cabin -16.0 %); the DC-10 gives 105,294 kg against the ACAP 572,000 lb
  value 120,914 kg (-12.9 %; -13.6 % at the standard 555,000 lb row); the
  B787-9 typical 290-seat case gives 112,166 kg against the unverified
  128,850 kg attribution (-12.9 %); the A380 typical 555-seat case gives
  224,820 kg against the 277,000 kg aggregator value (-18.8 %). The
  widebody residuals are far larger than any input or scope item identified
  here and are not attributable to a component with the retained evidence;
  no calibration was fitted. Metrics: `outputs/oew-evidence-closeout/validation-metrics.json`
  (validated set empty; conditional statistics descriptive only).

## Explicit boundaries

* ATR72-600 is outside the pinned transport FLOPS domain (no propeller or
  shaft-power equation) and stays an explicit unsupported row.
* AVE is notional; the Boeing 777-9 is its benchmark, not its truth.
* The A320-200 fixed-aircraft mission does not close on the generic default
  climb schedule (250 m/s true-airspeed step climbs, M0.78-0.85 at 5-9 km):
  the deficit persists at zero payload and at 1.2 x rated thrust, so it is a
  mission-schedule input, not a mass result. The preset's operational
  defaults carry no narrowbody schedule; see the climb-diagnosis rows of the
  experiment matrix.
