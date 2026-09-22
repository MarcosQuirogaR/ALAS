# Real-aircraft source audit, 2026-09-22

This note publishes, into the tracked checkout, the durable findings of two
2026-09-22 read-only research passes that were produced as dispatch
deliverables outside this repository (`REAL_AIRCRAFT_PRIMARY_SOURCES.md`, five
independent web/document research agents; `REAL_AIRCRAFT_EVIDENCE_MATRIX.md`,
a read-only re-compilation of the tracked contract). Those two files are not
part of this checkout. This note extracts only the parts that materially
change how a reviewer should read [aircraft-parity.md](../aircraft-parity.md):
confirmed genuine model gaps, a confirmed circularity risk, certification
document currency updates, and explicitly unresolved conflicts. It does not
modify [golden/aircraft/real_aircraft_parity.json](../../golden/aircraft/real_aircraft_parity.json),
[tools/aircraft_parity.cjs](../../tools/aircraft_parity.cjs), or any tolerance.

## 1. A320-200 `mass.usable_fuel_kg`, confirmed source/model circularity for the default preset

The parity contract's registered anchor (Airbus AC, 24,209 L at 0.785 kg/L =
19,004 kg) misses the ALAS model value (19,334 kg) by +1.74%, outside the
0.5% relative fuel-mass bound (`docs/aircraft-parity.md` "Tolerance-evaluation
fix"). The research pass found a second, newer certification source that
matches the model value digit-for-digit: EASA TCDS EASA.A.064, **Issue 62,
26 June 2026** (<https://www.easa.europa.eu/en/downloads/16507/en>), III.9
"Fluid Capacities" p. 43, "CFM, with MOD 37331 AND MOD 160001": 24,167 L at
0.800 kg/L = 19,333.6 kg -> 19,334 kg.

An independent code trace (this session, read-only) resolves why the match is
exact rather than approximate. For the default, unmodified A320-200 preset:

- `crates/alas-config/src/presets/narrowbody.rs:107-109` sets three literal
  constants directly on the preset's reference data: `usable_fuel_volume_l:
  Some(24_167.0)`, `usable_fuel_mass_kg: Some(19_334.0)`,
  `fuel_density_kg_l: Some(0.8)`. The mass field is not computed as
  `24_167.0 * 0.8` in code; all three are independent literals. The preset's
  own `identity.tank_configuration` at line 97 reads `"three tanks; MOD37331 +
  MOD160001"` and its `reference.sources` at line 138 already cites `"EASA.A.064
  Issue 62, pp.37-48"`, the ALAS source predates and already names the same
  certification issue and MOD state the research pass located independently.
- `crates/alas-pipeline/src/feasibility/fuel.rs:341-349`
  (`assess_fuel_capacity`) returns this literal verbatim, tagged
  `FuelCapacityEvidence::PublishedPreset`, whenever the evaluated design
  vector equals the preset's own default design vector, which is the case
  that produces the reported 19,334 kg. The alternative geometric path
  (tagged `GeometryEstimate`, `wing_fuel_volume_m3(...) * fuel_density`) only
  executes once the design vector has been changed away from the preset
  default.
- The same guard pattern recurs in `crates/alas-mass/src/product_stations.rs:38-51`
  (`tank_reference`), and `crates/alas-mass/src/tanks/resolve.rs:381-430`
  confirms the per-tank calibration factor is the identity (1.0) whenever
  every cell already carries a `published_usable_volume_l`, which is the
  case here, so there is zero geometric contribution to the reported figure.

**Classification: source/model circularity risk, not independent validation,
for the default-preset case.** The "model output" of 19,334 kg is a
hand-entered, explicitly-cited transcription of a certification fluid-capacity
table, not a result of an independent tank-geometry-times-density
computation. Registering the TCDS Issue 62 cell as the contract's primary
source anchor (a change this note does not make, since the contract is not
owned here) would make that row compare a literal against itself. This is a
release-relevant caveat: it does not, on its own, justify closing the
existing +1.74% miss against the currently registered Airbus AC anchor, and
it does not establish that ALAS independently reproduces A320-200 usable
fuel capacity for the default preset. An independent geometric estimate would
only exist for a design vector that has been perturbed away from the
preset's stock values, and that path's own inputs (the per-tank AMM literals
in `crates/alas-config/src/preset_fuel_tanks.rs:91-96`, which sum to 24,209 L
- a value matching the *other* registered source, the Airbus AC) would need
a separate circularity check before being treated as independent evidence.

No threshold, tolerance, or contract file was changed to record this finding.

## 2. ATR72-600 `mass.oew_kg`, confirmed genuine model gap across every published variant

Cross-checking OEW across every public ATR 72-600 document vintage and engine
variant found in this pass (2020 PW127M/N factsheet, ~2012 family brochure,
2026 PW127XT-M brochure, 2022 PW127XT-M freighter factsheet, Aircraft
Commerce Dec 2006/Jan 2007) gives a published passenger-variant OEW envelope
of **12,950-13,750 kg** across the whole family and every vintage/engine
combination. ALAS's independently modeled OEW, **12,014.6 kg**, sits 935 kg
*below the lowest published passenger figure in the entire family* and lands
within +1.8% of the windowless PW127XT-M *freighter* OEW (11,800 kg, no cabin
fit), a 72-seat passenger airliner cannot legitimately match a stripped
cargo airframe's empty weight. This corroborates the existing internal
diagnosis in `docs/flops-mass-model.md`/`docs/flops-robustness.md` (ALAS has
no turboprop/shaft-power mass branch calibrated for this class): the
10.67% miss already recorded in `docs/aircraft-parity.md` is a genuine,
structural model gap, not a source-selection or tolerance problem. Even the
most favorable legitimate comparator (13,010 kg Tech.-Spec.) still misses by
-7.65%, roughly 2.5x the existing +/-3% relative band. No tolerance
relaxation and no data substitution is justified.

EASA TCDS EASA.A.084 "ATR 42 and ATR 72", Issue 14, 23 February 2026
(<https://www.easa.europa.eu/en/downloads/7358/en>), III.13 pp. 36-37,
certifies only MRW/MTOW/MLW/MZFW for the ATR 72-212A (Mod 6219 / WV50); it
carries **no OEW, BEW, or standard-items table at all** and defers empty
weight to the Weight and Balance Manual. This metric can never be promoted to
certification tier from public EASA TCDS text alone.

## 3. DC-10 `geometry.fuselage_length_m`, confirmed genuine model discrepancy

The registered anchor (EASA IM.A.210 Issue 2, 55.35 m) is now independently
corroborated by a second source: a DHL Aviation Cargo dimension sheet states
"181 ft 7 in / 55.35 m"; converting the imperial figure directly,
181 ft 7 in x 0.3048 m/ft = 55.3542 m, matching to within ~0.01 m rounding -
an order of magnitude below the ALAS model's 0.20 m miss recorded in
`docs/aircraft-parity.md`. The discrepancy is a model-side geometry issue,
not source imprecision. FAA TCDS A22WE (the airspeed-limit authority EASA
IM.A.210 itself names) could not be retrieved this pass: its historical host
(`rgl.faa.gov`) is decommissioned and its replacement (`drs.faa.gov`) serves a
JavaScript-only interface with no fetchable document text; this closes off
both the DC-10 MMO question and an FAA-side length cross-check for now
without a browser-based FAA DRS search or a FOIA request.

## 4. A340-300 / B787-9 `geometry.mac_m`, certification is a documented dead end for LEMAC; a testable MAC-frame hypothesis for the B787

EASA TCDS A.015 Issue 28 (now dated **15 January 2026**,
<https://www.easa.europa.eu/en/downloads/19823/en>) states only "Datum:
Station 0.0, located 6.382 m forward of aeroplane nose. MAC: 7.270 m" and
defers CG range to the AFM; no LEMAC is published. EASA TCDS IM.A.115 **Issue 30, 17
December 2025** (<https://www.easa.europa.eu/en/downloads/7302/en>) gives MAC
6.27126 m identically for the 787-8/-9/-10, datum "1.41732 m forward of
airplane nose," and likewise defers CG range to the AFM. Neither publishes
LEMAC in any certification, ACAP, or accident-report source checked. Genuine
A340-300 operator WBM/AHM560 documents exist but every located copy was
paywalled or blocked this pass (Scribd `ECONNRESET`, dokumen.tips/mirrors
`403`).

A new, explicitly-labeled *hypothesis* (not a sourced fact) for the B787-9:
the certified 246.9 in / 6.27126 m MAC may be a **trapezoidal-reference-wing**
MAC, while ALAS's 7.54779 m is a **gross/Wimpress full-planform integrated**
MAC, a reference-wing-definition mismatch, not a modeling error. Supporting
numerical evidence (a tier-3 Lissys/Piano 787-8 dataset): a trapezoidal
reference MAC of 6.437 m against a trapezoidal reference area of 325.3 m^2
and span 58.67 m, distinct from that same tool's own "Wimpress reference"
area of 359.5 m^2 (close to ALAS's S=360.464 m^2, b=60.12 m). The
certified-MAC/(gross S/b) ratio of 1.046 is mathematically too low for a
gross-planform MAC at any physically reasonable taper ratio, while
6.437/(325.3/58.67)=1.161 is consistent with a taper ratio near 0.14,
supporting (not proving) the trapezoidal-reference reading. Do not tune
aero/mass coefficients around either MAC row while this frame question is
open, per the existing instruction in `docs/aircraft-parity.md`.

Separately: Boeing 787 ACAP Revision Q (Oct 2025) gives 787-9 weight tables
but **no OEW figure and no max structural payload**, the currently
registered ACAP citation for B787-9 OEW should be re-checked by the mass/OEW
owner, since OEW there appears only implicitly as a payload-range chart's
lower bound. A 23 March 2026 Boeing announcement states an FAA-cleared MTOW
increase of ~10,000 lb for the 787-9; ACAP Rev Q's higher weight option
(571,500 lb) already anticipates this. Neither of these is actioned here.

## 5. A220-300 / A340-300 `geometry.wing_area_m2`, remains internal-only; conflict documented, not resolved

Both `geometry.wing_area_m2` rows remain `alas_preset_input`
(INTERNAL-ONLY), with no independent manufacturer or certification anchor.
EASA TCDS IM.A.570 Issue 25 (A220) and A.015 Issue 28 (A340-300) both state
overall dimensions but never wing area, matching the pattern already
documented for the A320. The two Airbus documents that might carry an A220
wing-area table (ACP Issue 013 and the A220-300 APP) exceeded this pass's
10 MB web-fetch limit and were not read; this is an explicit tool-capability gap,
not a negative result. For the A340-300, the commonly-cited tertiary figure
(363.1 m^2, Wikipedia/aggregators) conflicts with ALAS's internal 361.6 m^2;
neither has a manufacturer or certification source, and this conflict is
recorded here explicitly rather than silently favoring either number.

## 6. A380-800 fuel condition, confirmed current, no action needed

EASA's type-certificate index confirms TCDS Issue 17 is still the current
A380 issue (no Issue 18 exists), and Airbus's aircraft-characteristics index
confirms the December 2025 AC is still the latest revision. The existing
registered fuel-condition split documented in `docs/aircraft-parity.md`
(Airbus AC tank-only 253,983 kg/323,546 L at 0.785 kg/L vs. EASA TCDS total
259,471 kg/324,339 L at 0.800 kg/L) needs no revisit.

## 7. Certification-document currency updates found this pass

| Document | Previously registered | Now current | URL |
| --- | --- | --- | --- |
| EASA A.064 (A320 family) | Issue 12 | **Issue 62, 26 Jun 2026** | <https://www.easa.europa.eu/en/downloads/16507/en> |
| EASA IM.A.570 (A220) | earlier issue | **Issue 25, 21 Aug 2026** | <https://www.easa.europa.eu/en/downloads/20964/en> |
| EASA A.015 (A340) | Issue 28 | **Issue 28, republished 15 Jan 2026** | <https://www.easa.europa.eu/en/downloads/19823/en> |
| EASA IM.A.115 (B787) | earlier issue | **Issue 30, 17 Dec 2025** | <https://www.easa.europa.eu/en/downloads/7302/en> |
| Airbus A320 Aircraft Characteristics | Rev 01 Jun 2024 | **Rev 46, 01 Jul 2026 (no fuel-table change)** | <https://mediaassets.airbus.com/pm_38_916_916266-iujedqawwy.pdf?fileName=aca32001-jul-2026-2.pdf> |

None of these currency updates were used to change a registered contract
anchor; the finding in section 1 above explains why the A320 case in
particular needs a circularity check before any such update.

## Sources not usable this pass (explicit negative results, not silent gaps)

- Boeing DC/MD-10 ACAP (`DAC-67803A Rev A`, May 2011, 5.9 MB) and the two
  Airbus A220 ACP/APP PDFs, exceeded the research tooling's 10 MB fetch cap;
  a direct download and local parse would close the A220 wing-area and
  DC-10 length/gear/payload-range gaps further.
- FAA TCDS A22WE (DC-10), historical host decommissioned, replacement is a
  JavaScript-only search UI with no fetchable document text.
- Turkish Airlines AHM560 A330-203/A340-300 documents, and a
  theairlinepilots.com Airbus weight-and-balance mirror, paywalled or
  blocked (`403`) at every located copy.
- Current A380 Aircraft Characteristics payload-range section, fetched but
  the PDF's text layer is compressed/image-based and not extractable.

## Provenance note carried over, not fixed here

`crates/alas-config/src/oew_reference/sources.rs` embeds `.agent/evidence/...`
and `.agent/data/...` local paths in several `OewSource.local_path` fields
(A320 ACAP, A220 ACP, A340 ACAP, A380 ACAP, DC-10 ACAP, ATR factsheet).
These paths are not usable outside the authoring checkout. This is source
code, outside this note's ownership; it is recorded here so a release
reviewer sees it, per the existing `LUNA_TEXT_AUDIT.md` LT-03 finding. The
public `title`/`publisher`/`url`/`quote`/`tier` fields on the same records are
unaffected.

## What this note deliberately does not do

It does not change `golden/aircraft/real_aircraft_parity.json`, any
tolerance, `tools/aircraft_parity.cjs`, or any preset/config/pipeline source
file. It does not convert a diagnostic or evidence-gap row into a pass. It
does not fabricate a model output for a quantity the model does not export.
