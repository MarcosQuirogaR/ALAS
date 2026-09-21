# FLOPS-based mass robustness and OEW verification

The production architecture retains the serialized name
`pure_flops_transport_v1` for compatibility. Its numerical core is NASA
FLOPS transport, with explicitly selected LTH cabin and pylon relations and
a GASP/TM-83458 shaft-power branch. Equation provenance must be read from the
completed buildup and ledger, not inferred from the architecture name.

## Engineering contracts

- Internal masses, lengths, force, torque and power use SI. The empirical
  equations convert at their boundaries. `reduction_ratio` is engine rpm /
  propeller rpm; the TM-83458 gearbox relation uses the reciprocal.
- Passenger cabin configuration is resolved before the first mass pass.
  Installed count-mode seats remain distinct from a layout occupancy
  shortfall. Legacy Premium seats use the same canonical treatment throughout.
  Explicit count-mode cabins also bound the payload layout: a named preset
  must not replace them with a larger capacity, and a row-packing shortfall
  remains visible to the feasibility check instead of shrinking the request.
- Public configuration validation and the runtime equation adapter both
  check their contracts. Impossible values must fail by name rather than
  propagating NaN or producing a plausible clamped result.
- Design landing mass cannot exceed its design gross mass. A declared design
  gross override and its blank landing-mass behavior have one sizing policy.
- Systems, unusable fuel and itemized payload close to their component
  allocations within `1e-6` relative tolerance. A broken mass ledger is an
  infeasibility, not an optional omitted figure.
- APU presence and engine equipment scope are installed-architecture inputs.
  The ATR hotel-mode case omits a separate APU. Unknown engine equipment
  inclusion is explicitly retained as uncertainty.

The full source descriptions remain in [the model documentation](flops-mass-model.md).
The primary FLOPS/Aviary reference files and hashes are in
[`flops-mass-sources.json`](flops-mass-sources.json).

## What an OEW comparison establishes

Equation verification, accounting closure, calibration and physical validation
are different checks. The pinned NASA cases verify the equation implementation;
the preset suite verifies complete inputs and accounting. A published OEW only
validates an aircraft prediction when model, engine, weight variant, cabin and
included operating equipment match.

The source registry `alas_config::oew_reference` distinguishes matched values,
conditional comparisons, source gaps and context-only estimates. In particular:

- AVE is evaluated against a **user-assumed Boeing 777-9 benchmark**. This does
  not turn the AVE geometry or an early projected empty mass into a weighed
  777-9. The current Boeing planning source does not provide numeric OEW.
- ATR PW127M/N and PW127XT-M factsheets refer to different aircraft equipment
  generations; their published OEWs must not be exchanged silently.
- Maintenance empty weights, jacking limits and manufacturer empty masses
  cannot be substituted for operating empty weight.
- LTH population extrapolation and unresolved engine installation scope remain
  limitations even when numerical regression tests pass.

No aircraft-specific multiplier or unexplained residual is applied to force
agreement with a published total. Reference-case input changes are retained
alongside each comparison so improvements can be reproduced and reviewed.

## Checks

### Component assumptions refined on 2026-09-21

The ATR72/PW127M preset declares an approximate 568F-1 propeller assembly
mass of `360.9 lb × 0.45359237 = 163.701486 kg` per propeller and `46.220 kg`
of full engine oil for the aircraft. These come from the
[JCAB TCDS No.75, Revision 3](https://asims.cab.mlit.go.jp/fsdb/a_katashikisyoumei.nsf/6b3e09b1290c32b7492574fd0029b17c/34d28828b5d6793249257c0f0033a32a/%24FILE/JCAB%20TCDS-ATR42%2672%20Revision%203.pdf),
PDF pages 8 and 10; page 12 states the full-oil empty-weight convention.
The propeller entry is approximate and does not enumerate accessory inclusions.
No extra accessory mass is invented. A declared assembly mass represents that
installed assembly; it must be reconsidered when changing the propeller design.
The generic regression remains available when no assembly mass is declared.

The turboprop adapter uses cruise Mach as its available approximation to the
propeller regression's maximum-power design condition. Its export records the
resolved Mach, nacelle wetted area, area basis, effective area coefficient and
propeller mass basis. Nacelle reference mass and area are calibration inputs,
not measured material density. The ATR42-derived installation and nacelle
anchors remain a source-transfer limitation for ATR72 prediction.
When engine bodies are omitted, the adapter reconstructs the configured nacelle
profile for the same wetted-area calculation. Malformed present geometry is
rejected. A cylindrical fallback is reserved for legacy configurations without
a profile and is identified in the exported basis.

Every registered preset retains `FCOMP = 0`, the published FLOPS wing-equation
endpoint with no composite technology credit. This does not assert metallic
construction. No evidence-based mapping from aircraft composite material
percentage to FCOMP has been established. Exports include wing-only sensitivity
evaluations at `FCOMP = 0, 0.5, 1`, holding the other resolved wing inputs fixed.
These scenarios neither alter production OEW nor define confidence intervals.

### Reproduction

```powershell
cargo test -p alas-config --lib
cargo test -p alas-mass --locked --no-fail-fast
cargo test -p alas-pipeline --lib
cargo test -p alas-opt --locked --lib
cargo check -p alas-report -p alas-gui --locked
cargo run -p alas-pipeline --example flops_robustness --locked -- .agent/data/flops-robustness-20260920/reference-conditioned-cases.json
```

The matrix includes all eight registered product presets and eight
reference-conditioned cases, with engine geometry retained. Every row checks
the item ledger, finite takeoff mass/CG/inertia and a physical inertia tensor.
Its cabin fields distinguish installed counts used for OEW from seated payload
occupancy. A configuration that misses these contracts causes a failing exit
status after the diagnostic artifact is written.

The 2026-09-20 implementation and evidence artifacts are retained in the ignored
`.agent/data/flops-robustness-20260920/` directory and the corresponding HTML
report under `.agent/reports/`. Historical comparison outputs from earlier
revisions are not current validation results.
