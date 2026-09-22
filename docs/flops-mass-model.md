# FLOPS transport mass model

The NASA FLOPS conventional-transport weight equations, implemented in
`crates/alas-mass/src/flops_transport/`. This is the durable description:
what is implemented, what is deliberately not, and which conventions were
chosen where the source is ambiguous.

The serialized architecture name `pure_flops_transport_v1` is retained for
saved-file compatibility. The current production model is **FLOPS-based**, not
a claim that every contribution is a NASA FLOPS equation: registered aircraft
can use LTH cabin equipment and pylons, and turboprops use a declared
shaft-power installation model. These resolved methods are retained in the
mass ledger and JSON export. An evaluated input contract is not physical OEW
validation. See [mass robustness and OEW verification](flops-robustness.md).

## Sources

| Role | Source |
|---|---|
| Equations | **NASA/TM-2017-219627 Vol. I**, *FLOPS Weight Equations* (Wells, Horvath, McCullers, 2017), **including the April 2018 errata printed as PDF page 4**. |
| Numeric reference | **NASA OpenMDAO Aviary**, Apache-2.0, pinned at commit `c7affbbe54dcbeded7373eae05f771882e2bb28a`. |

Both are pinned with retrieval URLs and SHA-256 digests in
[`docs/flops-mass-sources.json`](flops-mass-sources.json); the files
themselves are kept in a local, git-ignored evidence directory.

Reference parity establishes that this port evaluates the published
equations the way FLOPS itself does. **It is not physical validation against
a weighed aircraft**, and nothing in this model has been calibrated against
one.

## Sizing basis, cabin and payload ownership

Which design gross mass `DG` and design landing mass `WLDG` the equations
are evaluated at is an explicit state, `alas_config::MassSizingBasis`,
resolved from the design mode: a registered aircraft (`BaselineSandbox`,
`ReferenceAdaptation`) keeps its declared MTOW and certified MLW under any
mission; a clean-sheet design couples to its closed takeoff mass.
`FlopsStructureConfig::design_gross_mass_kg` declares a structural design
weight that pins `DG` in every mode. The FLOPS cabin terms are priced for
the cabin the payload layout seats (or for a cabin declared by count), and
the passenger-mass authority is split into occupant and checked bag. The
definitions, the accounting crosswalk and the validation status are in
[`mass-model-architecture.md`](mass-model-architecture.md).

## Selecting it

`MassModelConfig::mass_architecture` is the one method selector. The product
default is `pure_flops_transport_v1`; it owns the wing, tails, fuselage, gear,
nacelles, installed propulsion, systems, furnishings, operating items,
payload closure and fuel remainder as one checked buildup. The former
`systems_mass_method`, `structural_mass_method`, and `propulsion_mass_method`
fields are derived mirrors retained for loading old files and are not
independent production controls.

`legacy_reference_compatible_comparison` is an explicit comparison and
regression control. It is never selected as a missing-input fallback, and no
FLOPS-based group may be paired with its Torenbeek/fraction groups. Declared
LTH cabin/pylon methods and the shaft-power branch are explicit, separately
attributed components within the production architecture. A pre-version-2
file that selected the old all-legacy default or a hybrid is migrated to pure
FLOPS with a visible migration record; a user who needs the old numbers must
choose the comparison architecture by name.

Declared FLOPS architecture inputs live in
`MassModelConfig::flops_transport` (`FlopsTransportConfig`); technology
factors and declared airframe/propulsion overrides live in
`MassModelConfig::flops_structure` (`FlopsStructureConfig`). Missing physical
inputs return named blockers instead of invoking the comparison buildup.

**There is no fallback.** When a required datum is missing the evaluation
returns `FlopsTransportEvaluation::Unverified` with a stable, named list of
blockers, and the mass buildup fails with
`ComponentMassError::FlopsUnverified`. A missing input never becomes a
fraction.

## Group mapping into the ALAS ledger

FLOPS groups do not line up one-to-one with the eight ALAS OEW slots:

| FLOPS | ALAS slot | Note |
|---|---|---|
| Nacelles (structure, eq. 136) | `propulsion` | They sit on the engines, where the mass stations put them. |
| Paint (eq. 68) | `fuselage` | |
| Furnishings `WFURN` (inside eq. 138's systems group) | `furnishings` | Subtracted from the systems slot so `WFURN` is counted **once**. |
| Operating items `WOPIT` (eq. 140) | `furnishings` | Above empty mass, below OEW, as eq. 141 has it. |
| Empty-mass margin `WMARG` (eq. 139) | `systems` | Placed as an explicit ledger row, never implicit. |

The item-level mass statement (`crates/alas-mass/src/statement/`) enforces
three closures rather than assuming them:

1. The eight named systems rows plus an explicit
   `systems-empty_mass_margin_and_residual` row sum to `MassBreakdown::systems`.
   A residual below `-1e-6` relative is rejected as an invalid mass.
2. Caller-supplied unusable-fuel rows must agree with the FLOPS
   unusable-fuel allocation (eq. 121) within the same tolerance; the
   allocation is then relieved from the lumped furnishings remainder so the
   fuel is placed once, at its tank stations. A disagreement returns
   `LedgerError::UnusableFuelAllocationMismatch`.
3. Ledger `MassMethod` labels come from `LedgerMethods::from_mass_model`,
or, for a completed production evaluation, `LedgerMethods::from_buildup`.
Labels retain the actual LTH, FLOPS or shaft-power source. Exports preserve
each row's method and the resolved cabin/propulsion selectors.
4. A nonempty itemized payload list must sum to the component payload mass
within `1e-6` relative tolerance. A mismatch returns
`LedgerError::PayloadAllocationMismatch`; it cannot silently change zero-fuel
mass or CG. A failed physical ledger is an error feasibility finding.

## Installed equipment and cabin resolution

The first mass pass resolves passenger counts from the canonical cabin before
evaluating FLOPS. Count mode declares installed seats; percent mode resolves
a complete seed allocation and then follows the seated layout. Legacy Premium
seats are handled consistently by the canonical three-class representation.
Missing or inconsistent inputs at the raw equation adapter still produce a
named blocker. Public preflight checks the mass-specific configuration and
the design gross/landing mass relationship.

`apu_installed` is explicit. The default and jet presets retain equation 101;
the ATR hotel-mode preset has no separate APU. Engine starter and nozzle scope
are explicit metadata and configuration choices, so a certified dry-engine
mass is not silently assumed to include a complete installation. Unresolved
equipment scope remains an uncertainty, not a manufactured mass correction.

## Conventions chosen where the source is ambiguous

### `SFLAP` is wing-only

NASA/TM-2017-219627 prints `SFLAP` as the "total movable **wing** surface
area including flaps, elevators, spoilers, etc." — the enumeration
**explicitly names elevators**, so a plain reading includes the tail
movables. This implementation excludes them.

That is a deliberate **compatibility choice with NASA Aviary**, whose
`flops_based/surface_controls.py` computes `flap_ratio * wing_area` and adds
no tail term. Matching Aviary is what makes the port checkable against the
only published numeric reference. It is *not* a demonstration that
wing-only is physically correct: eq. 97's `WSC` is an all-aircraft
flight-controls group where tail movables plainly belong, and FLOPS feeds it
from the same `SFLAP` as the wing structural term of eq. 35. The source is
not self-consistent here.

Relative to a literal reading this lowers `SFLAP` by roughly a quarter on
the registered presets, `WSC` by about 16 % and `W2` by about 9 %.

Within that convention, each movable's area is its chord fraction times the
**laterally projected** planform band its declared span stations cut from
the wing, integrated over the real chord distribution. Projected `y` is the
measure `alas-opt`'s `transport_planform` uses for the same configuration
fields, and it makes a near-vertical winglet contribute nothing rather than
shifting every station inboard.

### Equation 10 grouping

The main body prints the whole fraction in the exponent; the **April 2018
errata** corrects it to `BT = [0.215 (0.37 + 0.7 TR) (SPAN^2/SW)^EMS] /
(CAYL TCA)`, which is what is implemented and what Aviary evaluates. The
errata's base is `SPAN^2/SW`, which differs from the aspect ratio only
through a glove (`AR = SPAN^2/(SW - GLOV)`); ALAS builds no glove.

### Equation 31 bracket

The memorandum prints the sweep-bucket bracket as a *multiplier*; Aviary
uses it as a *divisor*, and the divisor is implemented. The bracket is
literally `CAYL`'s second factor and eq. 10 divides by `CAYL`, so a
multiplier would invert every `FAERT`/`FSTRT` sensitivity. Logged as a
suspected second typo.

### Equations 77-80: inlet and nozzle

A branch, not a fold. `WENGB` "includes inlet and nozzle weight if they are
not specified separately". With `baseline_inlet_mass_kg` and
`baseline_nozzle_mass_kg` absent — the default — the result is eq. 80,
`WENG = WENGP`, the catalogue dry mass. Declaring either selects eq. 79,
`WENG = WENGP + WINL + WNOZ`, each scaled by its own exponent on the same
thrust ratio.

`FlopsStructureConfig::validate` **requires an explicit
`baseline_engine_mass_kg`** before either may be declared, because the eq. 76
fallback `THRSO/5.5` is an all-in baseline for the FLOPS engine term and adding
a separate inlet to it would double-count. That term includes inlet/nozzle on
the default branch, but it is not a whole installed pod: nacelle, pylon,
mounts, starters, reversers, controls, fluids and other installation scope
remain separate or unresolved. The complete FLOPS engine term is what reaches
eq. 137's group total and eq. 41's pod relief.

## Not implemented, and why

| Item | Status |
|---|---|
| **Turboprop / propeller / gearbox** | No NASA FLOPS thrust-based propulsion branch is used. The production model explicitly uses certificated dry-engine mass or GASP specific weight, source-declared propeller mass (with explicit accessory scope) or the Hamilton Standard regression, TM-83458 separate gearbox mass where needed, declared installation items and FLOPS fuel-system mass. Shaft power is never converted to equivalent thrust. A gearbox included in engine mass is not added again. The input reduction ratio is engine/propeller speed and is converted to TM-83458's reciprocal convention. The ATR preset uses the approximate JCAB 568F-1 mass and full-oil quantity described in [the robustness record](flops-robustness.md). Nacelle area follows the built or reconstructed configured profile, with an explicit component mass/area calibration anchor. Detailed FLOPS wing pod relief remains unsupported for shaft-power aircraft. |
| Fighter/attack, general-aviation, hybrid-wing-body branches | Not translated. Transport branch only. |
| `WARM` (armament), `AEWT`, `POWWT`, `WTBAT` | Zero for a transport; not represented. |
| Alternate mass equations (TM §7.1) | The shaft-power branch uses the thrust-free unusable-fuel equation 161. This is not a complete implementation of the alternate equation set. |
| Per-component mass scalers | Aviary has them; ALAS carries none, so validation cases divide the FLOPS output by the case's scaler. |
| Unusable-fuel density ratio | Aviary multiplies eq. 121 by `fuel_density / 6.7 lbm/galUS`. **This factor is not in the memorandum** and is inert for Jet A; not implemented. |
| `sqrt(Engine.SCALE_FACTOR)` nacelle rescale | An Aviary deviation with no TM basis. ALAS reads the *installed* nacelle from the built geometry, so applying it would double-count. |
| `FLAPR` ratio fallback for `SFLAP` | Not implemented; `SFLAP` is always integrated from the declared control surfaces. |
| `WLDG` from eq. 65 | ALAS uses the declared/mode-aware landing-mass limit instead, recorded in the evaluation's `sources.landing_mass`; `sources.design_gross_mass` records whether `DG` was the takeoff-mass requirement or a declared override. |

## Verification

`crates/alas-mass/tests/flops_validation_cases.rs` reproduces the
FLOPS-produced outputs of Aviary's `LargeSingleAisle1FLOPS` (detailed wing)
and `LargeSingleAisle2FLOPS` (simple wing, scalers at one) to the precision
the pinned data files quote. Unit tests cover the distributed-propulsion
branch (`NENG > 4`), which neither validation case exercises, against the
pinned `distributed_prop.py`.

## Reproducing the preset comparison

```sh
cargo run -p alas-mass --example flops_preset_comparison -- outputs/pure-flops-production/raw.json \
    <path-to-local-evidence-copy>/mass_reference_anchors.json
uv run --with matplotlib python tools/report_flops_comparison.py \
    outputs/pure-flops-production/raw.json outputs/pure-flops-production
```

The comparison example evaluates the registered presets with their
declared inputs and resolved methods. Jet transports and the ATR shaft-power
scenario produce separately attributed FLOPS-based rows. The secondary legacy
column is selected by name inside the example and never supplies a missing
FLOPS input. User-declared values in the default conventional scenario remain
labelled as assumptions, **not** preset or manufacturer facts.
