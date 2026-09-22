# Preset correlation and model corrections

The audit generator is `tools/preset_correlation.cjs`. Its local inputs are the
evidence-graded aircraft datasets and solver records in a local, git-ignored
validation directory (defaulted in the script; pass a directory argument to
use a different one). It writes CSV, JSON, and an internal correlation report
(2026-09-04). Those research artifacts are ignored; the generator and its
tests are tracked.

```powershell
cargo run --profile test -p alas-pipeline --example model_reference_dump -- out/validation/MODEL.json
node tools/preset_correlation.cjs out/validation
node --test tools/preset_correlation.test.cjs
```

The optimized test profile avoids the unoptimized dense-solver cost without
changing mesh settings. The dump runs every preset through the native analysis
and mission pipeline. Errors and partial mission status remain explicit.
Route source and effective airport codes distinguish airway paths, imported
plans, and great-circle approximations.

## Implemented corrections

- [Cargo capacity](cargo-capacity.md) distinguishes generated positions from
  loaded cargo, keeps bulk separate from containers, and prevents overlapping
  physical bulk/ULD footprints. A220 passenger baggage respects its bulk-only
  loading system. Passenger baggage consumes the configured cargo settings.
- [Exposed wing area](methods.md) removes buried main-wing skin from parasite
  drag, with an explicit legacy switch and analytical verification.
- [Routing](route-model.md) reads coordinate-bearing airway endpoints,
  including navaids absent from the fix catalog. The automatic detour guard
  preserves the source label of a fallback and exempts dispatched plans.
- The ATR active planform closes to the manufacturer's 61 m² wing area.
  Estimated chord ratios are preserved; no fit is made to the weakly sourced
  reference MAC or unknown longitudinal datum. Source: [ATR 72-600
  factsheet](https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf).
- A320 external height is 4.14 m and its engine axes are ±5.755 m from the
  centerline; A340 tail span is 19.4 m. Sources: Airbus aircraft-characteristics
  general-arrangement drawings and EASA.A.064 Issue 12, as cited in the local
  datasets. [A320 aircraft characteristics](https://aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-01/AC_A320_0624.pdf).
- A380 and DC-10 quarter-chord sweeps are 33.5° and 35°. The leading-edge
  parameters are converted accordingly, preserving all active station chords,
  span, and gross area (845 and 338.84 m²). Sources: Airbus A380 Facts and
  Figures and NASA CR-3119; the geometry regression states the conventions.
- A mission with zero horizontal distance cannot connect airports at different
  elevations under the model's finite climb/descent-rate constraints. The
  scheduling boundary rejects that case before altitude scaling drops legs.

## Audit semantics

Every numeric reference is retained, even when the model has no corresponding
output. `unsupported` means absent model capability or an unmatched route;
it does not mean zero. `diagnostic` retains a numerical difference but excludes
it from summary error statistics because conditions or definitions differ.
`comparison` still does not establish independent physical validation: many
geometry and engine quantities are input consistency checks.

The generator does not compare container internal volume against total usable
hold volume, geometric cruise-aircraft lift slope against a low-speed wing-only
estimate, or fitted total-polar Oswald efficiency against inviscid point span
efficiency as if these were the same measurement. Engine takeoff fields are
separate from cruise-cycle inputs. Historical VSPAERO records are diagnostic
until rerun with the modified geometry and matching conditions.

The dump integrates all active planform panels analytically, including the
side-of-body station. It preserves fuel and payload capacities for turboprops
without inventing a turbofan-TSFC Breguet range. The report uses true median
absolute differences, handles zero references explicitly, and escapes source
text in HTML and CSV.

## Remaining limitations

ATR main-deck baggage compartments and certified usable hold volumes are not
represented. Their total published volume alone supplies neither compartment
stations nor structural loading constraints. Generic available ULD positions
are not evidence of installed certified loading arrangements. Several mass,
MAC, area, engine-envelope and modification-state comparisons still need
equivalent reference definitions. Residual drag differences require matched
conditions and independent calibration evidence. External structural validation
and renewed VSPAERO comparisons are not supplied by the numerical tests.

Focused regression tests and library Clippy checks pass. Broader suites still
have failures in existing preset/configuration parity, cabin fixtures,
operational-speed expectations, a tank-limited fixture whose load is not
tank-limited, and an optimizer fixture requesting 350 seats where the current
layout fits 349. These are not counted as passing checks. See the local report
for the verification record; do not treat this work as a green workspace gate.
