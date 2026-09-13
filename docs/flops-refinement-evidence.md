# FLOPS refinement evidence

**Review date:** 2026-09-11  
**Status:** READY for production and validation review  
**Scope:** source-backed refinements and falsifiable diagnostics for the pinned pure transport FLOPS model. Production code and presets were not edited in this subtask.

## Result

The retained primary sources support exact engine dry-mass candidates for the A220-300, A320-214, A340-312, A380-841 and 787-9 engine families. They do not all support a literal replacement of the current FLOPS engine line because FLOPS adds starters and thrust reversers as separate terms, while some certification dry-weight definitions include engine equipment. The machine-readable candidates therefore report both a literal engine-line delta and conditional deltas after removing a separately estimated overlapping starter/reverser term.

Only the A220 and A320 source values increase the current pure FLOPS operating-empty-weight analogue in the literal line scenario (+730.485 kg and +456.221 kg total respectively). The A340, A380 and 787 source values are slightly lighter than the current Eq. 76 fallback and therefore do not reduce their positive comparison residuals. Their source-backed replacements remain useful for provenance and a later installation-scope audit; they are not residual-fitting knobs.

The A340-300 has a stronger comparison anchor than previously recorded: the current Airbus A340 ACAP Rev. 33 contains a primary planning figure labelled `OEW 131 215 kg` (PDF p. 143, printed section 2-14-0 p. 9). It is a maintenance/jacking configuration label, not a serial-number or operator load-sheet value, so the residual remains a configuration diagnostic.

A current Boeing 777-9 case can be specified exactly for the public planning fields in D6-86073 Rev. G, but there is no numeric 777-9 OEW in that document or in the retained FAA experimental-aircraft record. The current Boeing payload/range chart says `DATA TO BE PROVIDED AT A LATER DATE`. AVE is a notional benchmark with geometric resemblance to the 777-9; it is not a 777-9 mass ground truth.

## Definition and calculation boundary

The production snapshot is `outputs/pure-flops-production/raw.json`. Its pure FLOPS value is the model's `WOWE` analogue from NASA Eq. 141, assembled as `WWE + WOPIT`. NASA Eq. 139 defines `WWE = WSTRCT + WPRO + WSYS + WMARG`; Eq. 140 defines operating items as flight crew/baggage, cabin attendants/galley crew/baggage, unusable fuel, oil, passenger service, and cargo containers.

For an engine candidate, the **literal line delta** is:

```text
(candidate dry-engine mass - current baseline_engine_mass_kg)
  * installed engine count
```

That arithmetic assumes the source boundary matches `WENGB` on the no-separate-inlet/nozzle Eq. 80 branch and leaves NASA Eq. 86 reversers and Eq. 89 starters unchanged. It is deliberately labelled literal rather than an approved production correction. The conditional values in [refinement_candidates.json](../.agent/data/flops-refinement-evidence/refinement_candidates.json) remove the current FLOPS starter estimate where the engine TCDS makes the starter part of engine type design. The A340 record also shows a named scenario removing the CFM56-5C adapter/reverser term. Exact installation/EBU part weights are still needed before promotion.

A dry engine is not an installed propulsion system. Nacelle, pylon/strut, inlet, nozzle, reverser, starter, controls, fuel-system plumbing and fluids are separate unless the source explicitly places them inside the quoted value. In particular, the A220 ARP `engine & nacelle assembly` value is a whole-assembly diagnostic and must not be added to a dry-engine delta.

## Engine candidates

| Case | Current baseline kg/engine | Primary value kg/engine | Count | Literal total delta kg | Scope result |
|---|---:|---:|---:|---:|---|
| A220-300 / PW1521G-3 | 1,811.758 | 2,177.0 | 2 | **+730.485** | Verified EASA basic-engine value; thrust reverser is aircraft type design; starter inclusion is unstated, so promotion is conditional. |
| A320-214 / CFM56-5B4/3 SAC | 2,226.689 | 2,454.8 | 2 | **+456.221** | Verified EASA family value; starter is engine type design; 5B reversers are aircraft-TC systems. Reconcile starter before promotion. |
| A340-312 / CFM56-5C3/F | 2,680.185 | 2,644.4 | 4 | **-143.140** | Verified EASA family value and matching 14,457 daN CFM56-5C3/F rating. The 5C description includes an adapter kit with mixer, exhaust plug and thrust reverser; starter is engine type design. Scope is conditional. |
| A380-841 / Trent 970-84 | 6,279.598 | 6,246.0 | 4 | **-134.391** | Verified EASA model value; explicitly excludes fluids and Nacelle EBU; TRU/FFD are aircraft-type-design items. Starter scope is unstated. |
| 787-9 / GEnx-1B74/75/P2 | 6,285.160 | 6,147.1 | 2 | **-276.120** | Verified EASA family value and approved 787-9 BOM family. Starter is engine type design; 787 fan-reverser installation is listed separately. Reconcile starter/installation before promotion. |
| DC-10-30 / CF6-50C | 4,156.734 | **unknown** | 3 | — | No exact primary DC-10 installation mass found. NASA CR-3119 gives 4,321 kg bare and 6,174 kg complete pod for a 242.8 kN reference installation, not this DC-10 variant; proxy is prohibited. |
| AVE-v1 / conceptual GE9X | 8,658.318 | ~9,525.44 | 2 | **+1,734.245** | GE publishes `~21,000 lb` for generic GE9X. This is an estimated notional scenario, not an exact GE9X-105B1A dry or installed mass and not an aircraft anchor. |

The conditional total deltas are recorded numerically in the JSON artifact. The important scope checks are:

- EASA PW1500G Issue 10, PDF p. 7, gives 2,177 kg for all PW1500G models and says the value applies to the basic engine with standard equipment. PDF p. 14 Note 4 says the thrust reverser is not engine type design and is certified as part of the aircraft. The TCDS does not state whether the starter is inside the dry value.
- EASA CFM56 Issue 06, PDF p. 11, gives 2,454.8 kg for CFM56-5B SAC and 2,644.4 kg for all CFM56-5C models, including basic engine, accessories, optional accessories and engine-condition-monitoring equipment. The same page says the starter is part of engine type design. PDF p. 17 distinguishes 5B aircraft-TC reverser systems from 5C reverser systems in the engine parts list; the 5C description places a thrust reverser in the adapter kit.
- EASA Trent 900 Issue 12, PDF p. 9, gives 6,246 kg for Trent 970-84 and explicitly excludes fluids and Nacelle EBU. It states that the TRU and FFD do not form part of engine type design and must be certified as aircraft type design.
- EASA GEnx Issue 12, PDF p. 8, gives 6,147.1 kg for GEnx-1B/P2 and says dry weight includes basic engine, basic accessories and optional equipment. The starter is part of engine type design; PDF p. 19 lists the 787 fan-reverser installation part numbers.

## Existing-aircraft mass anchors and guards

| Case | Pure FLOPS kg | Anchor or source result | Residual / interpretation |
|---|---:|---:|---|
| A220-300 | 33,793.295 | Airbus ACP OWE 37,081 kg (PDF p. 30, module p. 2, Table 2); Airbus ARP OWE 37,149 kg (PDF p. 101, module p. 3, Table 2) | +3,287.705 / +3,355.705 kg. Both are primary planning values; ARP says to use WBM/load-sheet/fleet OEW where available. The 68 kg difference is a revision/configuration difference, not a calibration target. |
| A320-200 | 39,119.153 | Current Airbus ACAP Rev. 46 PDF p. 38 gives WV017 MTOW 78,000 kg and MZFW 62,500 kg. Ground-clearance PDFs pp. 52-53 label 45,000 kg `empty weight for maintenance`. | The 45,000 kg maintenance light-weight condition is not OEW and must not replace the existing secondary 41,052 kg comparison anchor. |
| A340-300 | 111,822.035 | Airbus ACAP Rev. 33 PDF p. 143, printed 2-14-0 p. 9, labels `OEW 131 215 kg` for the A340-300 standard-tire maintenance/jacking figure. | +19,392.965 kg. Primary planning label, but serial/configuration and operating-item inclusion are not published; do not fit groups to it. |
| A380-800 | 228,750.596 | Current Airbus ACAP Rev. 20 has no numeric OEW in the retained text. | Existing 277,000 kg value remains an estimated/typical secondary comparison anchor; residual +48,249.404 kg is not source-resolved. |
| B787-9 | 113,898.476 | Current Boeing ACAP Rev. Q PDF p. 17 defines OEW but supplies no numeric 787-9 OEW. | Existing 128,850 kg value remains an estimated/typical secondary anchor; residual +14,951.524 kg is not source-resolved. |
| DC-10-30 | 107,184.428 | No exact retained OEW anchor or exact CF6-50C installation mass. | Leave source gaps explicit. |
| AVE-v1 | 160,827.179 | No aircraft anchor; AVE is notional. | Do not call the AVE value actual or use it to calibrate 777-9. |

The full residuals, source IDs, and literal post-correction values are in [mass_gap_diagnostics.json](../.agent/data/flops-refinement-evidence/mass_gap_diagnostics.json). A scalar OEW residual cannot identify whether the mismatch is structural, installed propulsion, systems, cabin, water, catering, containers, unusable fuel, or a definition boundary.

## A220 component diagnostics

The current Airbus ACP/ARP material is useful for falsification but not for adding an arbitrary mass offset.

- ACP Issue 013 PDF p. 30 Table 2 gives A220-300 planning OWE 37,081 kg, maximum tank capacity 21,918 L and unusable fuel 109 kg. Its Table 3 gives total engine oil 56.7 kg. Current pure FLOPS values are 49.394 kg oil and 157.245 kg unusable fuel. Naive deltas would be +7.306 kg and -48.245 kg respectively, but the source and Eq. 140 boundaries are not proven identical. The latter moves the model downward and cannot repair a positive underprediction.
- ARP Issue 111 PDF p. 101 Table 2 gives OWE 37,149 kg and explicitly instructs users to use WBM/load-sheet/fleet OEW when available. Its Table 3 lists generic crew/service items; the full listed aggregate is 1,579.32 kg versus the current FLOPS operating total 1,593.377 kg, but the item boundaries differ. This is a boundary audit, not a correction.
- ARP Issue 111 PDF p. 102 Table 4 gives `Engine & nacelle assembly (each) 7,100 lb / 3,220.56 kg`. The current core plus structural nacelle terms are about 2,139.518 kg per assembly before separate reverser, starter, controls and fuel-system terms. The apparent +2,162.085 kg total gap is a whole-assembly overlap diagnostic; it must not be added to the FLOPS engine/nacelle lines.
- ARP Table 3 lists five flight-attendant positions and a third crew member as generic planning positions. The registered production case uses three flight attendants and two flight-deck crew as a declared study configuration. Extra positions may be evaluated as a scenario, but the table does not prove the installed operator complement.

## Boeing 777-9 matched planning case

[matched_7779_case.json](../.agent/data/flops-refinement-evidence/matched_7779_case.json) is a source-locked planning case for `777-9 / GE9X-105B1A / D6-86073 Rev. G (September 2025)`. It contains no fabricated OEW. Verified public planning fields include:

- MTW 352,441 kg, MTOW 351,534 kg, MLW 266,258 kg, MZFW 254,918 kg;
- usable fuel 197,356 L / 157,477 kg using Boeing's 0.803 kg/L planning density;
- standard two-class 426 seats (42 business + 384 economy) and three-class 357 seats (8 first + 49 business + 300 economy);
- standard lower-deck volume 221.3 m³ and optional large-aft-door volume 230.2 m³, with 46/48 LD-3 positions in the two arrangements;
- length 76.73 m, extended span 71.76 m, and ground span 64.85 m;
- GE9X-105B1A, 134-inch fan and 105,000 lb Boeing equivalent thrust. The converted thrust is 467,063.27 N, but BET is not silently relabelled as an engine-TCDS static-thrust rating.

The current Boeing product page separates the 777-8 (MTOW 365,140 kg, range up to 9,500 nmi) from the 777-9 (MTOW 351,530 kg, range up to 8,000 nmi). Only the 777-9 values are retained in the matched case. The product range is marketing context, not FLOPS `DESRNG`; the current ACAP Rev. G payload/range page is marked `DATA TO BE PROVIDED AT A LATER DATE`.

Boeing's Rev. G OEW definition includes structure, powerplant, furnishings, systems, unusable fuel/other unusable propulsion agents, integral configuration equipment, certain standard personnel/equipment/supplies, and excludes usable fuel and payload. Rev. G Table 2-1 does not publish an OEW value. FAA's N779XZ record lists a Boeing 777-9 with GE9X SERIES, experimental classification and no TCDS; FAA warns that the record alone does not establish airworthiness or current configuration. Boeing's 16 July 2026 status article reports ongoing certification toward first delivery in 2027. These facts do not provide an operational OEW ground truth.

AVE's current notional values are close in length (76.72 m), extended span (71.75 m) and conceptual thrust (467,000 N), but differ in design gross mass (358,670 kg versus 351,534 kg MTOW), fuel capacity (200,000 kg versus 157,477 kg usable fuel), passenger case (350 all-tourist versus 357 three-class), and unknown external nacelle/wing/MMO/mission fields. That resemblance justifies a separate matched planning run after missing inputs are sourced; it does not justify replacing AVE OEW with a 777-9 value.

## FCOMP, MMO and mission guards

NASA Appendix D defines `FCOMP` as a composite utilization factor ranging from 0.0 for no composites to 1.0 for maximum use of composites. It is not a literal composite material fraction. The 777X ACAP identifies new composite wings, but no primary source maps the architecture or a quoted percentage to the FLOPS coefficient. At fixed inputs the wing terms multiply by `(1 - 0.4 FCOMP)`, `(1 - 0.17 FCOMP)` and `(1 - 0.3 FCOMP)`, so increasing FCOMP lowers the calculated wing mass. It cannot be used to repair a positive pure-OEW underprediction without an independent coefficient mapping.

`MMO`/`VMAX` is a certification speed input. It is not cruise Mach. The current 777-9 ACAP does not publish MMO; no value is entered in the matched case. FLOPS uses VMAX in the starter, fuel-system and passenger-service equations. The A340 ACAP's M=0.82 and Airbus A380 facts' M=0.85 are cruise/profile context and are not substitutes for MMO.

## Source register and reproduction

The complete 21-entry source register (20 primary/authority sources plus the existing secondary ALAS anchor registry), revisions, URLs, local paths and SHA-256 values is [source_manifest.json](../.agent/data/flops-refinement-evidence/source_manifest.json). Primary page text extracts are [primary-extracts.json](../.agent/data/flops-refinement-evidence/extracts/primary-extracts.json); the existing Boeing 777X and A220 extracts are retained separately in the same directory.

Key primary sources:

- [NASA/TM-2017-219627 Volume I](https://ntrs.nasa.gov/citations/20170005851), 2017: equations 75-80, 86-92, 116-126, 136-141 and FCOMP definition.
- [Airbus A220 ACP Issue 013](https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/A220-ACP-Issue013-00-27Nov2025.pdf), 27 November 2025: A220-300 OWE and fluid tables, PDF pp. 29-31.
- [Airbus A220 ARP Issue 111](https://mediaassets.airbus.com/pm_38_924_924594-csbojjclon.pdf?fileName=aircraft-recovery-publication-arp-for-a220-august-2026.pdf), 20 August 2026: planning OWE, operating items and assembly data, PDF pp. 99-102.
- [EASA PW1500G TCDS IM.E.090 Issue 10](https://www.easa.europa.eu/en/downloads/20863/en), 14 August 2025: PDF p. 7 dry weight and p. 14 reverser note.
- [EASA CFM56 TCDS E.003 Issue 06](https://www.easa.europa.eu/en/downloads/7797/en), 9 January 2023: PDF pp. 11-12 and 17 dry weight, starter and reverser scope.
- [EASA Trent 900 TCDS E.012 Issue 12](https://www.easa.europa.eu/en/downloads/7779/en), 16 March 2026: PDF pp. 9-10 dry weight, Nacelle EBU/TRU/FFD scope and rating. This corrected URL replaces the stale `7733/en` pointer in the earlier pure-evidence manifest.
- [EASA GEnx TCDS IM.E.102 Issue 12](https://www.easa.europa.eu/en/downloads/7641/en), 25 February 2026: PDF pp. 8 and 19 dry weight, starter and 787 fan-reverser data.
- [EASA Boeing 787 TCDS IM.A.115 Issue 30](https://www.easa.europa.eu/en/downloads/7302/en), 17 December 2025: PDF pp. 33-38 787-9 engine, fuel, MMO, weights and crew limits.
- [Airbus A340 AC Rev. 33](https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/AC_A340-200-300_20251201.pdf), 1 December 2025: PDF p. 35 WV029 and p. 143 A340-300 OEW label.
- [Airbus A320 AC Rev. 46](https://mediaassets.airbus.com/pm_38_916_916266-iujedqawwy.pdf?fileName=aca32001-jul-2026-2.pdf), 1 July 2026: PDF p. 38 WV017 and pp. 52-53 maintenance empty-weight guard.
- [Boeing 777-9 ACAP D6-86073 Rev. G](https://www.boeing.com/content/dam/boeing/v2/airports/acaps/777X_Rev_G.pdf), September 2025: PDF pp. 12-17, 20-23, 32-33.
- [Boeing 777X product page](https://www.boeing.com/commercial/777x), accessed 11 September 2026: current 777-8/777-9 comparison.
- [Boeing certification-status article](https://www.boeing.com/features/2026/07/certification-progress-reported-737-max-777-9), 16 July 2026; [FAA N779XZ record](https://registry.faa.gov/AircraftInquiry/Search/NNumberResult?nNumberTxt=779XZ); [GE9X product page](https://www.geaerospace.com/commercial/aircraft-engines/ge9x), accessed 11 September 2026.

The binary source hashes and exact local paths are machine-readable in the manifest. JSON files were parsed with the bundled Python 3 runtime and the retained PDFs were extracted with `pypdf`; no production files were modified. [refinement_candidates.json](../.agent/data/flops-refinement-evidence/refinement_candidates.json) is the handoff artifact for the lead, and [mass_gap_diagnostics.json](../.agent/data/flops-refinement-evidence/mass_gap_diagnostics.json) is the validation artifact.

## Remaining gaps

1. Exact operator/WBM or serial-number OEW is still missing for the current widebody comparison cases and for 777-9; published planning/maintenance labels cannot be promoted to ground truth.
2. Exact aircraft installation/EBU component breakdowns are needed for starter, reverser, nacelle, pylon and accessory overlap before engine candidates are promoted.
3. A complete mission/profile/reserve is missing for 777-9 and for several existing presets; marketing range and payload-range chart annotations cannot fill `DESRNG`.
4. No primary mapping exists from composite architecture to FLOPS `FCOMP`; it remains `unknown`.
5. ATR72 remains unsupported by the pinned pure transport thrust-based FLOPS model and receives no fabricated jet-thrust candidate.

