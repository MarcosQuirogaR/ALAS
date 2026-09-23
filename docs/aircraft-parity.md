# Real-aircraft parity audit

This audit compares the ALAS preset export with independent, source-backed anchors for seven real aircraft: A220-300, A320-200, A340-300, A380-800, ATR72-600, B787-9 and DC-10. AVE is a synthetic ALAS design and is intentionally excluded. The contract is [golden/aircraft/real_aircraft_parity.json](../golden/aircraft/real_aircraft_parity.json); it is an evidence record, not a production input or a certification claim.

The harness keeps five kinds of missing or weak evidence explicit:

- `within_tolerance` and `out_of_tolerance` are eligible only for a numeric source anchor marked `primary` with `condition_match: true`.
- `diagnostic` means both values exist, but the source is secondary/estimated, the metric is a proxy, or conditions/variant/reference area do not match. It is excluded from the score.
- `unsupported` means a source target exists but the model export does not contain the requested quantity.
- `evidence_gap` means no trustworthy public numeric target was found for the requested quantity.
- `source_conflict` means the optional local evidence file disagrees with the immutable contract anchor. The harness does not select either value for scoring.

No expected value is copied from `MODEL.json`, and no failed or missing result is replaced with zero. The harness writes its working evidence files to a local, git-ignored output directory; the result records their SHA-256 hashes so a rerun can be audited.

## Reproduce the audit

From the repository root, generate the model export and run the parity harness:

```text
cargo run -p alas-pipeline --example model_reference_dump
node tools/aircraft_parity.cjs
```

The default outputs (see the `outputDir`/`reportPath` defaults in [tools/aircraft_parity.cjs](../tools/aircraft_parity.cjs), a git-ignored local directory) are:

- `AIRCRAFT_PARITY.json` — machine-readable rows, source metadata, hashes and status summary.
- `AIRCRAFT_PARITY.csv` — flat review table.
- a self-contained aircraft-parity HTML report with links to sources and regulatory implications.
- an A220 five-state CG/gear replay HTML report with phase applicability and source/model findings.

Paths can be overridden for a clean fixture or a release job:

```text
node tools/aircraft_parity.cjs `
  --model path/to/MODEL.json `
  --contract golden/aircraft/real_aircraft_parity.json `
  --out path/to/results `
  --report path/to/report.html
```

`--fail-on-out-of-tolerance` returns exit code 2 when a matched primary comparison exceeds its declared tolerance. Evidence gaps and diagnostics remain reportable without making a release job fail by accident. The harness itself is covered by `node --test tools/aircraft_parity.test.cjs`.

The optional [all_preset_cg_closeout example](../crates/alas-acceptance/examples/all_preset_cg_closeout.rs) can export model/public MAC-frame diagnostics for the seven presets. It is useful for investigation, but the parity harness intentionally leaves CG rows unsupported until the model export carries a normalized CG envelope, cruise CG, datum, LEMAC, MAC, weight case and fuel/cabin condition. A report that only has an aggregate mass or a percent-MAC number without its frame is not a CG parity result.

## Current run and interpretation

### 2026-09-23 regenerated run (release 1.2 candidate)

`model_reference_dump` was regenerated from the integrated release branch (release profile) and
the harness was rerun against the unchanged contract:

- `MODEL.json` SHA-256 `E57AF6D11AE5C430C82BAF9387454033956D8B71A80F68CF24BBE9EE76941252`
- contract SHA-256 `DD043B3324978AAE18ACF58BF2E44CC40FEFA7325A4E3D87E24728C4074B68ED`
- result JSON SHA-256 `39D5F29A188F5B4BF379A75C22DDCC085ABB656EFCBD9EE8E206CBDBFEF82672`
- harness self-test `node --test tools/aircraft_parity.test.cjs`: 18/18 pass

| Status | Rows |
| --- | ---: |
| within_tolerance | 68 |
| out_of_tolerance | 6 |
| diagnostic | 70 |
| unsupported | 21 |
| evidence_gap | 76 |
| source_conflict | 0 |

**Reference inputs are now separated from model outputs.** The harness marks a scored row with
`model_provenance_note` when the model value is a registered input rather than an analysis result:
source-scaled gear stations, published fuel capacity, and (new in this run) declared preset inputs
exported straight from the design vector, requirements or gear-layout definition (span, fuselage
length, engine spanwise position, MTOW, cruise Mach, gear topology counts). The summary reports
`by_provenance` for the 74 scored rows:

| Scored rows | within_tolerance | out_of_tolerance |
| --- | ---: | ---: |
| Registered reference inputs (data retention, not prediction) | 64 | 2 |
| Independent model outputs | 4 | 4 |

The 68 `within_tolerance` rows therefore do **not** mean 68 validated predictions. Only eight
scored rows are model outputs:

| Preset | Quantity | Model | Source | Result |
| --- | --- | ---: | ---: | --- |
| A320-200 | `geometry.mac_m` | 4.1934 m | 4.1935 m | within |
| A380-800 | `geometry.wing_area_m2` | 845.0 m2 | 845 m2 | within |
| ATR72-600 | `geometry.wing_area_m2` | 61.0 m2 | 61 m2 | within |
| DC-10 | `geometry.wing_area_m2` | 338.84 m2 | 338.8 m2 | within |
| A340-300 | `geometry.mac_m` | 7.3466 m | 7.27 m | out, +1.05 % |
| B787-9 | `geometry.mac_m` | 7.5478 m | 6.2713 m | out, +20.4 % |
| ATR72-600 | `mass.oew_kg` | 15,244 kg | 13,450 kg | out, +13.3 % |
| ATR72-600 | `mass.max_payload_kg` | 5,679 kg | 7,550 kg | out, -24.8 % |

Wing area is a computed projected planform area, but each preset planform was built from the same
published geometry, so these three matches are close to calibration rather than prediction. MAC is
integrated from that planform; the A340-300 and B787-9 misses remain reference-geometry questions
(see below), not aerodynamic or mass tuning targets. The ATR72-600 OEW moved from -10.7 % (2026-09-09)
to +13.3 % after the integrated mass and fuel corrections; with the MTOW fixed, the payload
shortfall follows from it. It is a genuine model gap.

The two out-of-tolerance reference inputs are data-entry discrepancies, not model errors: the A320-200
usable fuel literal (19,334 kg against 19,004 kg, the circularity case described below) and the DC-10
declared fuselage length (55.55 m against the EASA IM.A.210 DC-10-30 value of 55.35 m).

None of these rows is certification, weighed-aircraft or flight-test evidence.

### 2026-09-09 run

The 2026-09-09 release-profile run against the freshly regenerated `MODEL.json` contains 241 rows. The fresh model export SHA-256 is `F40A20F265A4654C8D1EADC53A47CE101D8EB87F9442D6023D52F5501D6071B4` (corrected 2026-09-22; the previously published string here was missing its trailing hex digit); the parity result hash is `1DA8F0F84AB3EDE2D31989431469C57D1B471685EAFF1EF268C59887F41E26DF`. The pre-A220-station export is retained locally (outside this checkout) as `MODEL-pre-a220-gear-20260909.json` with SHA-256 `1AF549DC73408497224314DA65FBA75CD3652D1E5720182A5AD4555E80CCE996`, and the pre-station-anchor export remains `MODEL-pre-gear-20260909.json` with SHA-256 `08CF196BBE3CC381C7A7805ED8B7E0CA1F06E22B70D5C8F9A31DE0442AA68760`.

| Status | Rows | Interpretation |
| --- | ---: | --- |
| within_tolerance | 72 | Matched primary anchors inside the declared tolerance |
| out_of_tolerance | 2 | Matched primary anchors outside the declared tolerance |
| diagnostic | 70 | Numeric or descriptive comparison retained for review but excluded from score |
| unsupported | 21 | Public target exists; model quantity is absent or not normalized |
| evidence_gap | 76 | No trustworthy public numeric target was found |
| source_conflict | 0 | Local source bundle agrees with all static contract anchors |

The 74 eligible rows are not a certification score.

### Tolerance-evaluation fix (2026-09-09)

`compareNumeric` in `tools/aircraft_parity.cjs` combined the declared absolute and relative
tolerances with **OR**: a row passed if it satisfied *either* bound. Because every tolerance
policy in the contract declares both an absolute and a relative bound sized for the largest
aircraft in its category, this let a generous absolute bound silently swallow a large relative
error on a smaller one. Concretely, `mass_oew` declares `absolute: 2000 kg, relative: 0.03`; on
the ATR72-600 (source OEW 13,450 kg) that let a 1,435.4 kg (**10.67%**) miss score
`within_tolerance` because `1435.4 ≤ 2000`, even though `10.67% ≫ 3%`. The harness now requires
**both** declared bounds to hold (AND); a bound that is not declared for a given tolerance policy
is simply not checked. `tools/aircraft_parity.test.cjs` adds direct `compareNumeric` unit tests
for this (including the zero-source-value and single-bound-declared fallback cases) plus a
full-harness regression test reproducing the ATR-OEW-shaped failure mode.

Re-running the harness with the fixed code against the *same, unregenerated* `MODEL.json`
(SHA-256 unchanged, `F40A20F265A4654C8D1EADC53A47CE101D8EB87F9442D6023D52F5501D6071B4`) and contract
exposes three previously-hidden misses, written to an isolated local fixture (not the canonical
harness output, which this audit intentionally leaves untouched so it is not silently changed for
other in-flight work; the next ordinary reproduction run will pick up the fix automatically):

| Status | Rows (pre-fix) | Rows (post-fix) |
| --- | ---: | ---: |
| within_tolerance | 72 | 69 |
| out_of_tolerance | 2 | 5 |

The three newly-exposed misses:

- **ATR72-600 `mass.oew_kg`**: model 12,014.6 kg vs. source 13,450 kg, **−10.67%** (absolute error
  1,435.4 kg, inside the 2,000 kg absolute bound but far outside the 3% relative bound). This row
  was suspected of scoring `within_tolerance` only because of the OR-tolerance defect — confirmed
  here by an actual run, not a static read.
- **A320-200 `mass.usable_fuel_kg`**: model 19,334 kg vs. source 19,004 kg, **+1.74%** (absolute
  error 330 kg, inside the 500 kg absolute bound but outside the 0.5% relative bound).
- **DC-10 `geometry.fuselage_length_m`**: model 55.55 m vs. the EASA IM.A.210 (Issue 2) DC-10-30
  anchor of 55.35 m, a **0.20 m** absolute miss — outside the 0.05 m absolute bound declared for
  rounding, though inside the 0.5% relative bound. This is the mirror case: here the *relative*
  bound was the loose one masking a real absolute discrepancy. No production geometry was changed
  to investigate or close this; it is reported as found.

None of these were re-tuned, loosened, or removed to make them pass; they are reported as new
`out_of_tolerance` findings for the geometry/mass owners to investigate.

### 2026-09-22 reproduction and outstanding model-dump staleness

The AND-tolerance fix above is no longer a hypothetical rerun: it is the checked-out
`tools/aircraft_parity.cjs` (unchanged since 2026-09-15) and it was reproduced today with
`node --test tools/aircraft_parity.test.cjs` (17/17 pass, exit 0) and

```text
node tools/aircraft_parity.cjs --out out/parity-rerun-20260922 --report out/2026-09-22-aircraft-parity.html
```

against the *same, still-unregenerated* `MODEL.json` (SHA-256
`F40A20F265A4654C8D1EADC53A47CE101D8EB87F9442D6023D52F5501D6071B4`, dated 2026-09-09) and the
unchanged contract (SHA-256 `6CFA76B71D091BEDEE3F9FBE483EEA482EEFA77934A2EFD96DDCA2E84F7FF557`).
Exit 0; result JSON SHA-256 `87E9255BB18E1FCC3DF52F57D7274AEB401AF2FE2E2BA089518F4845A5F4FAD1`. The
summary is exactly the post-fix row from the table above (69 `within_tolerance`, 5
`out_of_tolerance`, 70 `diagnostic`, 21 `unsupported`, 76 `evidence_gap`, 0 `source_conflict`),
confirming the fix is now simply the harness's ordinary, unmodified behavior rather than a special
case. A per-preset status figure generated directly from this run's JSON was saved to an internal
report path (SHA-256 `7201405CABB26E47022188AC4406DCDDB4F4EC798C3425113E30C1436FB036C9`); it is a
comparison-status count chart, not a certification pass/fail figure, and its own caption says so.

This rerun deliberately used the isolated `--out`/`--report` paths rather than overwriting the
tool's canonical default output locations (`AIRCRAFT_PARITY.json` and
`2026-09-09-aircraft-parity.html`), for the same reason given above: those canonical outputs are
left alone so other in-flight work is not silently changed underneath it.

**The underlying `MODEL.json` was not regenerated and is now materially stale.** It is 13 days old
relative to this rerun, and the working tree has an active, uncommitted A320 cabin/mass regression
in flight (`crates/alas-pipeline/tests/fixed_aircraft_mass_basis.rs` currently fails because the
cabin/mass path yields 138 seats where the pipeline expects 132) whose owner holds exclusive
compiler access for the duration of that fix. `cargo run -p alas-pipeline --example
model_reference_dump` was therefore not run this session; every A320-200 row in this rerun reflects
the pre-regression-fix model state, not the current source tree. Once that lane's Cargo access is
released and the regression is resolved, the correct sequence to refresh this section is: `cargo run
-p alas-pipeline --example model_reference_dump` then `node tools/aircraft_parity.cjs` (writing over
the canonical default output once no other lane depends on the old copy), and updating
the SHA-256 values and status table above from that fresh run, not from this one.

A parallel, independent read-only research pass and a code-level circularity trace performed the
same day are published in
[real-aircraft-source-audit-2026-09-22.md](research/real-aircraft-source-audit-2026-09-22.md). Its
most release-relevant finding: the A320-200 `mass.usable_fuel_kg` model value (19,334 kg) is, for
the default preset, a hand-entered literal transcribed from a cited certification source
(`crates/alas-config/src/presets/narrowbody.rs:107-109`, tagged
`FuelCapacityEvidence::PublishedPreset` in `crates/alas-pipeline/src/feasibility/fuel.rs:341-349`),
not an independent tank-geometry computation, a confirmed **source/model circularity risk**, not
independent validation, for that row. It also confirms the ATR72-600 OEW and DC-10 fuselage-length
misses above are genuine model gaps across every published document vintage, not source-selection
artifacts, and records EASA TCDS issue-currency updates (A.064 Issue 62, IM.A.570 Issue 25, A.015
republished 15 Jan 2026, IM.A.115 Issue 30) found during that pass.

### Model-declared circularity markers (2026-09-09)

The model export already declares, about itself, when a station or capacity value is not an
independent prediction: `gear.stations_source_scaled` (`true` for A220/A320/A340/A380, `false`
for ATR72/B787/DC-10, which use an independently modeled nose-relative frame instead) and
`payload_range.fuel_capacity_limit === "published usable capacity"`. The harness now reads these
existing model fields (it invents nothing) and attaches a `model_provenance_note` to any row whose
model-side quantity is a station/wheelbase/track coordinate derived from a source-scaled drawing
frame, or a fuel-capacity figure that is the registered published capacity rather than a predicted
tank volume — explicitly excluding plain topology counts (`gear.n_nlg_wheels`,
`gear.n_mlg_struts`, wheels-per-strut, total wheels), which are transcribed integers, not scaled
coordinates. In the post-fix rerun, **33 of the 74 eligible rows (45%)** carry this note: 15 gear
station/wheelbase/track rows across the four Airbus presets and 18 fuel-capacity rows across all
seven presets. A `within_tolerance` result on one of these 33 rows confirms data-entry retention
through a unit/scaling conversion, not an independently predicted station or fuel volume. The full
independent/transcription/calibrated breakdown of all 74 eligible rows is retained in an internal
working audit that is not published with this checkout; the `model_provenance_note` field itself
is exposed in every harness run's output and can be inspected directly by reproducing the run.

The two prior out-of-tolerance rows identify further integration work:

- A340-300 `geometry.mac_m`: model 7.34658 m versus the certified 7.27 m anchor. Check whether the active planform integral and the certified balance MAC represent the same reference geometry before changing aero or mass coefficients.
- B787-9 `geometry.mac_m`: model 7.54779 m versus the certified 6.27126 m reference MAC. This is a frame/planform-definition issue until proven otherwise; do not tune drag or CG around it.
- The Airbus gear station checks now use source-drawing coordinates in the declared geometric nose-tip frame. A220 NLG/MLG `[3.401568, 18.633948, 18.633948]` m (ACP Issue 013, S/N 55001–59999), A320 NLG/MLG `[5.07, 17.71]` m, A340 NLG/MLG `[6.67, 32.05, 33.04]` m, and A380 NLG/WLG/BLG `[4.97, 33.58, 36.85]` m are registered with source-specific rounding tolerances. The A220 wheelbase is `15.232380 m` and track `6.731 m`; the A380 wing and body wheelbase checks are separate (`28.61 m` and `31.88 m`). These anchors are normalized geometry evidence and do not replace model loads, mass calibration or certified station/attachment data.

The A380 fuel row is now split by source condition. Airbus AC publishes 323,546 L and 253,983 kg for the wing and trim tank capacity at 0.785 kg/L. The current EASA A.110 TCDS Issue 17 adds 793 L of usable systems inventory and publishes a 324,339 L total at its 0.800 kg/L convention, or 259,471 kg. The A380 preset and current model output use the EASA total (324,339 L, 259,471 kg, 0.800 kg/L), so the Airbus AC rows are retained as condition-limited diagnostics and the TCDS mass row is within tolerance. The TCDS volume row remains `unsupported` because `MODEL.json` does not yet export `payload_range.fuel_capacity_l`.

The ATR 72-600 mission evidence has a material variant/condition boundary. The passenger PW127M/N sheet (2020) publishes 200/300 NM block fuel of 638/879 kg and 61/84 min, but does not print payload, reserve, alternate, taxi, route-profile or atmosphere conditions for those rows. The current response at the historical 2022 passenger URL is a 2026 PW127XT-M sheet with explicit 95 kg passenger, EASA reserve, 100 NM alternate and 10 min taxi conditions and 616/861/1106 kg at 200/300/400 NM; it is a qualified future PW127XT benchmark, not a PW127M comparison. The often-copied 624/869/1115 kg values belong to the separate ATR 72-600F freighter sheet. The current passenger preset route is LEMD-LEPA (348.2 NM, 1,069.5 kg trip fuel) and has no serialized payload-range corners or reserve contract, so no ATR mission row is eligible. The underlying ATR mission benchmark/condition analysis and the hashed manufacturer source manifest are retained in an internal audit trail; they are not published with this checkout, so the sheet dates, URLs and conditions cited above are the reproducible record for now.

The certified widebody MAC values cannot be converted to a model `%MAC` frame from the public TCDS pages alone. A340-300 TCDS A.015 Issue 28 gives datum station 0 at 6.382 m forward of the nose and MAC 7.270 m, while B787 TCDS IM.A.115 Issue 30 gives datum station 0 at 1.41732 m forward of the nose and MAC 6.27126 m; neither publishes LEMAC. The source WBM/AFM LEMAC and the reference-wing boundary are therefore still unavailable. As a scale check, the current reference-area/span pairs give A340 `S/b = 361.6/60.30 = 5.996683 m` and B787 `S/b = 360.464/60.12 = 5.995742 m`; the certified B787 MAC is `1.045952 × S/b`, so it must not be relabeled as `S/b`, while the ALAS geometric MAC is 7.547785 m. The model export computes its own frame from the integrated active planform (`x_LEMAC = root_datum_x + wing_x_shift + ∫x c dy / ∫c dy`) and the CG closeout keeps this model frame separate from a source `PlanningMacReference`. Only the A220 currently has a source-grounded LEMAC. Do not add a guessed A340/B787 planning frame or alter their geometry until a WBM/AFM value or coordinate-level reference boundary is available.

A formula coverage register maps the current mission, fuel, mass/CG, aero, atmosphere and performance equations to their code paths, focused tests, independent evidence and remaining source gaps; it is retained internally and not published with this checkout. A companion NotebookLM query bundle was prepared for a future authenticated source audit; no NotebookLM result is represented in the current parity status.

The model-only `AVE` preset appears in the model hash and is reported as intentionally excluded. Most L/D, CD0, Oswald efficiency, critical-Mach and lift-curve rows are diagnostics because public values come from analytical, fleet-derived or wing-only sources. They are useful formula checks and trend diagnostics, not matched flight-test validation.

The landing-gear export keeps the two sides of this comparison explicit. `gear.n_nlg_wheels`, `gear.n_mlg_struts`, `gear.mlg_wheels_per_strut`, `gear.main_wheels_total`, `gear.x_nlg_m`, `gear.x_mlg_m`, `gear.main_gear_x_m`, `gear.wheelbase_m`, `gear.track_width_m` and `gear.positions` are the current model's sized layout in its aircraft nose-relative frame. `gear.main_gear_station_frame` declares when a registered source anchor was scaled from the geometric nose-tip drawing frame; it does not claim a certified WBM/AFM datum. `gear.source_reference` is independent metadata for the registered A220, A320, A340 and A380 references (including source wheelbase/track, longitudinal stations and per-strut counts); it never supplies the model loads or mass calibration. The A220 ACP source dimensions are nominal and vary with weight/CG; no 3-D attachment hardpoints are inferred. `gear.model_reference_geometry` exposes the configured source baseline carried by the typed landing-gear input, with the sizing/model distinction retained. The export preserves the heterogeneous A340 `[4,4,2]` and A380 `[4,4,6,6]` model bogie vectors and corresponding totals 10 and 20. The A380 `gear.wheelbase_m` comparison is explicitly NLG-to-wing-gear (28.61 m), while `gear.body_wheelbase_m` is the separate NLG-to-body-gear check (31.88 m).

For the A220 cabin, the source-scoped capacity boundary is EASA TCDS EASA.IM.A.570, Section 2 / III.19, page 23 in the official EASA-hosted Issue 24 text (20 February 2026; the local source record identifies current Issue 25, 21 August 2026). The baseline MPSC is 145 in configuration `C – III – C`, no installed option, with 3–5 cabin crew; 149 applies only with Option C25631002 and `C – III* – C`, and the TCDS note requires separate airworthiness approval for a customized 149-seat layout. Keep this 145 cap separate from the operator's 140-seat planning layout and the model's percent/floor-derived capacity. The exact legacy A220 preset variant is BD-500-1A11, 67,585 kg MTOW, S/N 55001–59999; it must not inherit the later 149-seat option implicitly. The source page is [EASA.IM.A.570](https://www.easa.europa.eu/en/downloads/20964/en).

The registered product dispatcher now carries the 145-seat source cap into the same per-row allocation pass that creates seat items and passenger mass. With the same `AlasConfig::from_value({"preset":"A220-300"})` and design vector used by the product payload path, the cabin input remains `num_passengers = 130` with a percent mix; the built fuselage is `3.50 m` in diameter and its passenger deck is `29.75 m` long. The generic diameter proxy still selects Type III and reports 70 seats per pair-rating rule. The registered source arrangement replaces that proxy on the product path with three complete pairs in the sequence `C – III – C`, whose ratings sum to `55 + 35 + 55 = 145`; the source cap is applied during row allocation rather than after it. The capacity probe therefore reports `generic_cap = 70`, `model_cap = geometric_capacity = 145`, `source_capacity_cap = 145`, `source_exit_layout = C-III-C`, `capacity_binding = source_exit_layout`, `exit_pairs = 3`, `exit_capacity = 145`, and `total_pax = seated_pax = 145` with `unseated_pax = 0`. The source pair stations are selected from the model's existing bay stations and the source-specific test checks adjacent spacing of at most `18.3 m`; this is a numerical layout check, not a full evacuation or certification demonstration. It does not establish that the generic cabin geometry reproduces the source A220 cabin configuration. The optional 149-seat `C – III* – C` arrangement remains out of scope for this preset.

The product capacity test keeps two other source records explicitly incomplete. ATR 72-600 publishes 72 seats (and a separate 78-seat approval), but no revision-locked source exit sequence is registered; the corrected generic Type-III pair proxy therefore exposes 70 seats and the shortfall remains visible. The DC-10 ACAP publishes `255/399` standard/maximum seating for the Series 30, but its extracted source package does not provide a matching LOPA, seat pitch or exit arrangement for this passenger variant. The current generic product row pass seats 243 against the 250-seat registered planning target; the test records this as a source/layout gap while requiring `unseated_pax = 0` and row mass/count closure. No planning count is copied into the model to turn either gap into a false pass.

The source station audit transcribes Airbus Aircraft Characteristics drawings for A220, A320, A340 and A380, records the local PDF hashes and page/figure identifiers, and keeps their rounded nose-tip dimensions separate from certified station data. The A220 ACP source identity is `A220-ACP-Issue013-00-27Nov2025.pdf`, SHA-256 `AC489084AC6FD0B68F7D8DF65BDE42F01644442DC6069B9C0562B42BC7AE678B`; pages 150–156 provide the applicability, stations, wheelbase, track and footprint. The active model applies the registered fractions to the current fuselage length, so shrink and optimization remain continuous; source dimensions do not replace model-derived mass or reaction quantities.

The A320 source maximum-payload load case is exercised separately from the ordinary zero-belly product baseline by [a320_source_max_payload_case_separates_net_tare_gross_and_usable_fuel](../crates/alas-acceptance/tests/acceptance_matrix.rs). The registered A320-214 WV017 references give MZFW 62,500 kg and configuration OEW 41,244 kg, hence 21,256 kg gross payload. With 180 seats, 2,880 kg checked bags and 2,682 kg explicitly requested net belly freight, the source-layout call carries seven ULDs and 574 kg tare, closing at 21,256 kg. The actual CG-targeted `FullAnalysis` call carries the same net load but selects six ULDs and 492 kg tare, producing 21,174 kg gross payload; the 82 kg residual remains visible as a load-plan/model condition difference. Its gross model fuel is 16,964.221 kg, resolved unusable fuel is 135.570 kg, and the corresponding usable closure is 16,828.651 kg. These are model outputs and identities, not a manufacturer fuel or certification claim. The exact frames, units and release action are reproducible from the cited acceptance test above and the `model_reference_dump` example; the full closure diagnosis narrative is retained in an internal audit report not published with this checkout.

The actual A220 five-state replay and its structured state artifact are retained internally and not published with this checkout; the figures below are reproducible from the release product path and `all_preset_cg_closeout`. The release product path emitted OEW, analyzed ZFW, operational mid-mission, operational reserve and analyzed TOW records. The current model applies its static-stability, configured-forward-CG and gear checks to all five; the report labels OEW as ground loading and leaves its generic forward-flight verdict open because no minimum-flight-mass/applicability field exists. The source ACP planning frame is compared only with the analyzed loaded TOW. A220 model CG/gear findings therefore remain separate: the forward model finding belongs to OEW (18.128606 %MAC versus 20.672602 %MAC), while the public loaded-TOW finding is 37.990705 %MAC versus 37.053233 %MAC aft. The latter remains an open source-frame result; no acceptance tolerance or envelope limit was changed.

## What is currently covered

The evidence bundle has primary or traceable anchors for many overall dimensions, selected certified or published weight limits, some fuel capacities, a few cruise Mach values, and selected landing-gear counts. It has limited mission telemetry and no exact independent block-fuel conditions for most routes. The following requested comparisons remain open or condition-limited:

| Requested quantity | Current status | What is needed for a release comparison |
| --- | --- | --- |
| CL versus AoA | `evidence_gap` for all presets | A public or test-backed curve with alpha convention, Mach/Reynolds, altitude, configuration, trim, reference area and uncertainty |
| Cruise L/D and cruise AoA | Mostly `diagnostic`; cruise AoA is not exported | A matched clean/trimmed polar and declared weight, altitude, Mach and atmosphere; export cruise AoA and its sign convention |
| Maximum Mach | Some certified MMO anchors; model does not export `aerodynamics.mmo` consistently | Normalized MMO/VMO record with altitude/configuration and source type; distinguish MMO from drag-rise onset or classical airfoil Mcrit |
| Ceiling | Public maximum operating altitude is not a service ceiling | Weight-, CG-, temperature- and engine-state-dependent residual-climb curve; export performance service ceiling separately from the operating limit |
| Payload-range/fuel | Mission route telemetry is not an independent payload-range point | Payload, zero-fuel weight, fuel/reserve policy, ISA/wind, route distance, climb/descent and block-fuel conditions for each corner |
| Subsystem masses | Model now exports component masses, centroids and first moments; no exact public real-aircraft subsystem breakdown | Exact variant definition, component list, unusable fuel/fluids, weighing condition and independent first moments |
| CG envelope/cruise CG | Model now exports loaded physical CG and its own geometry MAC frame; A220 planning vertices exist locally; others lack public numeric envelopes | Forward/aft limits by weight/configuration, datum/LEMAC/MAC, cabin/fuel case and a cruise CG point |
| Gear count/position | Model exports sized topology, wheelbase/track, normalized nose-tip stations and wheel coordinates; A220/A320/A340/A380 group anchors are source-backed, while ATR/B787/DC-10 remain source-limited | NLG/MLG topology, wheels per strut/bogie, axle coordinates, frame, track definition and exact variant |
| SOL101 deformation | `evidence_gap` for every preset (confirmed by a sourced 2026-09-09 negative search retained in an internal working note, not published with this checkout) | Matching load case, constraints, materials, mesh/element basis and an independent displacement/strain measurement or certified test result. Manufacturer press releases (e.g. Boeing 787 limit/ultimate wing-test deflections) state a number and whether it is limit or ultimate but no boundary conditions, load distribution, weight state or variant, so none is usable. The one open lead is Kirmse et al. (DLR ETTC 2021, elib 145368), an in-flight 2 g wing-deformation measurement on DLR's A320-232 "D-ATRA" — the only candidate on an ALAS-preset airframe, not yet retrieved, and in any case a flight-load case rather than a limit/ultimate value. |

## Formula and units audit

The formulas used by the ALAS aero and mission paths should be read with their domains and reference quantities attached:

```text
CL = L / (q S)
CD = D / (q S)
L/D = CL / CD
CD = CD0 + k CL²
k = 1 / (π e AR)
```

Here `q` is dynamic pressure in Pa, `S` is the declared reference area in m², `CL` and `CD` are dimensionless, `AR` is dimensionless, and `e` is a dimensionless effective span-efficiency factor. These relations describe a steady, trimmed operating point or a fitted polar only when the lift/drag definition, Mach, Reynolds number, configuration, trim and reference area match. The [Sun et al. drag-polar method](https://doi.org/10.1016/j.trc.2020.01.026), [OpenAP paper and data release](https://doi.org/10.3390/aerospace7080104), and the [Poll-Schumann Part 3 transport-aircraft analysis](https://doi.org/10.1017/aer.2024.141) are retained as named source diagnostics in the contract. None supplies a public, exact, seven-aircraft CL-versus-AoA validation set.

For range work, the Breguet jet form is a cruise approximation:

```text
R = (V / c) (L/D) ln(Wi / Wf)
```

`R` is distance, `V` is true airspeed, `c` is fuel mass flow per unit weight and time, and `Wi/Wf` is a dimensionless weight ratio. A mission block-fuel result also needs climb, descent, reserves, taxi, payload and atmosphere. Therefore a route result from `MODEL.json` is not compared with a marketing range number unless those conditions are captured in the source anchor.

For mass properties, the required relations are:

```text
xCG = Σ(mi xi) / Σmi
%MAC = 100 (xCG − LEMAC) / MAC
```

All longitudinal stations must use one declared datum and units; LEMAC and MAC must come from the same reference geometry. A component mass list without first moments cannot establish CG. Current certification text in [14 CFR 25.29](https://ecfr.io/Title-14/Section-25.29) requires a repeatable weighed OEW/CG condition including ballast, unusable fuel and operating fluids, which is why estimated component sums are not treated as measured OEW/CG evidence.

The current CG extraction path is explicit about this frame requirement: `model_reference_dump` exports the integrated geometry MAC and nose-relative LEMAC, and `all_preset_cg_closeout` computes model `%MAC` with those values. When a source planning frame exists, the closeout separately computes `%MAC = 100 (xCG − source_LEMAC) / source_MAC`; it does not substitute the geometry MAC. A datum station by itself only translates manufacturer stations and does not provide LEMAC, so the A340/B787 TCDS datum/MAC pair cannot support a numeric CG-envelope comparison.

No NotebookLM result is included in this audit. The curated NotebookLM connector had expired authentication, browser login could not complete in the available connected surface, and token refresh returned stale credentials. The formulas and source claims above come from the local evidence bibliography and the linked primary/peer-reviewed sources; they are not represented as NotebookLM inference.

The bounded MSES condition audit confirms the current first-order sweep-normal Mach and body/twist/downwash mapping, but retains the nominal r2 MSES run as `not_converged` (`0/7` points) and flags the unresolved `Re≈180e6` versus `12.875 m` root-chord provenance mismatch for lead-owned condition reconciliation.

## Regulatory and design-space requirements

The model design space should expose the conditions that the governing rules require the analyst to vary:

- [14 CFR 25.21](https://ecfr.io/Title-14/Section-25.21) requires proof across appropriate weight and CG combinations with tests or calculations of test-equivalent accuracy.
- [14 CFR 25.25](https://ecfr.io/Title-14/Section-25.25) makes maximum and minimum weights conditional on operating, environmental and loading conditions.
- [14 CFR 25.27](https://ecfr.io/Title-14/Section-25.27) requires practical forward/aft CG limits for separable operating conditions.
- [14 CFR 25.101](https://ecfr.io/Title-14/Section-25.101) requires declared ambient, still-air, humidity, configuration and procedures for performance results.
- [14 CFR 25.103](https://ecfr.io/Title-14/Section-25.103) ties reference stall speed to weight, area, configuration, CLMAX and the CG producing the highest VSR.
- [14 CFR 25.121](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-121.pdf) sets OEI climb gradients by engine count in the primary GovInfo 1-1-25 annual edition: gear extended positive/0.3/0.5%, gear retracted second segment 2.4/2.7/3.0%, final takeoff 1.2/1.5/1.7%, and approach 2.1/2.4/2.7% for two/three/four engines.
- [14 CFR 25.115](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-115.pdf) reduces the actual takeoff path by 0.8/0.9/1.0% to form the net path for two/three/four engines. [14 CFR 25.123](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-123.pdf) requires critical-engine and most-unfavorable-CG en-route paths at each applicable weight, altitude and temperature, with one-engine-inoperative net reductions of 1.1/1.4/1.6% and two-engine-inoperative reductions of 0.3% (tri) or 0.5% (quad).
- [14 CFR 25.119](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-119.pdf) requires a 3.2% all-engines landing climb at VREF using the thrust available eight seconds after selecting go-around thrust. [14 CFR 25.107](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-107.pdf) makes V2 a selected speed that must meet the 25.121(b) gradient and V2MIN/VMC/rotation/manoeuvring floors; a single configurable V2/VS multiplier is therefore only a screening input.
- [14 CFR 25.305](https://ecfr.io/Title-14/Section-25.305) addresses detrimental permanent deformation at limit load and strength/deformation treatment at ultimate load.
- [14 CFR 25.307](https://ecfr.io/Title-14/Section-25.307) permits analysis where the method is reliable for the structure and otherwise calls for substantiating tests.

The OEI API keeps the legacy closed-form `tw_oei_climb_constraint` available for existing golden tests, but documents its result as an in-flight requirement. `tw_oei_climb_constraint_at_v2` is the source-aware path: it derives CL from the selected V2/stall ratio, accepts gear-up high-lift, asymmetric trim/control and inoperative-engine drag as separate terms, and converts the in-flight requirement to SLS installed T/W using an explicit condition-to-SLS thrust ratio. It returns `None` for engine counts outside two/three/four or for missing drag/thrust evidence. The implementation does not invent an altitude/Mach thrust lapse or silently treat omitted trim or windmilling drag as zero.

The supporting flight-test guidance in [FAA AC 25-7D Change 1](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D_Chg_1.pdf) §4.9 (base [AC 25-7D](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D.pdf)) calls for drag polars and one-engine-inoperative yaw-drag data, and distinguishes the gear-extended takeoff trim condition from later configurations. Those data are not available for the generic ALAS presets, so their OEI status remains a conceptual screening result rather than a certification claim. The code tables intentionally return no value for five or more engines; the two-engine-inoperative net-path case is defined only for three- and four-engine airplanes.

The primary regulatory source used for the OEI table is the annual [CFR-2025 title 14 volume 1](https://www.govinfo.gov/app/details/CFR-2025-title14-vol1) edition, retrieved 2026-09-09. The linked eCFR mirror remains useful for navigation, but its daily edition is not treated as the authority in the evidence ledger. The detailed audit and helper-function signatures are retained in an internal working report, not published with this checkout.

The actionable integration sequence is to add normalized aero-condition metadata and polar arrays, add weight/CG/temperature/engine-state performance records, add component first moments and CG frames, and add gear topology/coordinates. Keep SOL101 external parity disabled until matching load and measurement evidence exists. This lets the harness turn each evidence gap into a reviewable row when a trustworthy source or normalized model export becomes available.
