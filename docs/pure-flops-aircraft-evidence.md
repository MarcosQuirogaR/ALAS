# Pure FLOPS aircraft evidence

This document records the aircraft evidence used to map the eight registered
baseline names to the pinned NASA FLOPS transport model. It is an evidence
and review artifact, not a claim that every preset has a complete certified
design mission. The machine-readable records described below are retained in
the maintainers' local audit trail; they are not part of the repository
checkout and are not published, and should be regenerated from the primary
sources cited in this document before relying on them:

- `aircraft_inputs.json`, with all 15 scalar FLOPS inputs, source locators,
  units, variant applicability, status, candidates, and uncertainty.
- `mass_reference_anchors.json`, with revision-locked mass anchors, the
  WWE/WOPIT/WOWE definition audit, engine mass coverage, and the
  no-calibration rule.
- `source_manifest.json`, with source authority, URL, local file, hash,
  revision, and coverage.

The bounded collection uses manufacturer, certification-authority, and NASA
documents first. Operator manuals and derived validation records are retained
when a primary source does not publish the quantity, and are labelled as such.
The local Airbus, Boeing, ATR, EASA, and NASA PDFs are preserved in the source
intake directories with SHA-256 values in the manifest. A source-backed number
can still be inapplicable to the selected weight, engine, or cabin variant; the
variant statement and uncertainty field are part of the evidence.

## Coverage at a glance

| Preset | Exact baseline used in the evidence | Mission | Cabin | Installed architecture | Engine dry mass | Geometry |
|---|---|---|---|---|---|---|
| A220-300 | BD-500-1A11, legacy Issue 12 weights, PW1521G-3, S/N 55001–59999 ACP applicability | Source gap; range candidate is declared | WBM 140Y source, registered 130-seat case needs resizing | Mixed: engines/tank topology/fixed wing sourced; hydraulics and WCARGO unresolved | **Verified** PW1521G-3 2177 kg bare engine | Primary span/length/gear; area/MAC secondary; detailed planform gap |
| A320-200 | A320-214 WV017, CFM56-5B4/3 SAC, Sharklet | Source gap; no exact WV017 payload/profile/reserve point | Airbus typical 150-seat two-class source | Mixed: MMO, hydraulics, engines, tanks and fixed wing sourced; WCARGO load case unresolved | **Verified** CFM56-5B4/3 SAC 2454.8 kg bare engine | Primary dimensions/gear; some planform values secondary |
| A340-300 | A340-312 WV029, CFM56-5C3/F | Source gap; chart is partial | Airbus typical 335-seat source, registered case is 290 seats | Mixed; MMO, hydraulics, tank interpretation and WCARGO unresolved | Exact -5C3/F mass **source gap** | Primary length/gear/nacelle; area and engine station limited |
| A380-800 | A380-841 WV000, Trent 970-84 | Source gap; partial chart has reserve context | Airbus typical three-class 555 source, registered case is 525 seats | Mixed; engine/tanks/fixed wing sourced, hydraulic/WCARGO unresolved | **Verified** Trent 970-84 6246 kg bare engine | Primary span/area/sweep/length/gear; MAC and detailed planform gap |
| B787-9 | Boeing 787-9 legacy 561,500 lb MTOW, GEnx-1B74/75/P2 family | Source gap; no exact legacy mission/reserves | Boeing typical two-class 290 source | Mixed; engines/tanks/fixed wing sourced, hydraulic/WCARGO unresolved | Exact -1B74/75 mass **source gap**; old family proxy rejected | Primary span/MAC/length; several planform/gear values secondary |
| DC-10 | DC-10-30 passenger, registered 572,000 lb option, CF6-50C family | Source gap; source standard row does not match option | Total seating source, exact class split gap | Mixed; engine mounting/fixed wing sourced, MMO/hydraulics/tank cells/WCARGO unresolved | Exact CF6-50C mass **source gap**; family proxy rejected | NASA primary wing geometry; length primary, other values limited |
| ATR72-600 | ATR 72-212A/600, PW127M, 23,000 kg option | Unsupported by pinned transport FLOPS | Factsheet 72-seat reference only | Unsupported propeller/shaft-power technology | Unsupported; no exact verified PW127M mass | Source-backed reference only; no FLOPS result |
| AVE | AVE-v1 notional reference twin | N/A | N/A | N/A | N/A | N/A |

`Source gap` means the corresponding value is intentionally null in the JSON.
Where a model run needs a number, a separate `candidate` record identifies the
scenario choice and its uncertainty. Candidate values do not upgrade the
source status and must not be used as validation data without resolving the
gap.

## The 15 FLOPS inputs

The field order is fixed in `aircraft_inputs.json` and mirrors the transport
configuration interface:

1. `maximum_mach` (`VMAX`): maximum operating Mach/MMO. Cruise Mach is not
   substituted. Verified values are A220 0.82, A320 0.82, A380 0.89, and
   B787-9 0.90. A340 and DC-10 remain source gaps; their cruise or secondary
   proxies are shown only as unverified candidates. ATR's 0.55 is reference
   data under the unsupported domain.
2. `design_range_nmi` (`DESRNG`): no exact complete mission was found for any
   real baseline. The study candidates are A220 3400, A320 3400, A340 7200,
   A380 8000, B787-9 7635, and DC-10 5700 nmi. These are user-declared
   scenario values, not a conversion of cruise Mach or a selected route.
3. `flight_crew_count` (`NFLCR`): NASA's published transport equation gives
   two through 150 passengers and three at 151 or more. That default is
   labelled `published_flops_default`; A380 two and DC-10 three also have
   primary type evidence. The A220, A320, and B787 operational choices remain
   separate candidates where needed.
4. `flight_attendant_count` (`NSTU`): NASA's equation gives zero at zero
   passengers, one through 50, and `1 + ceil(NPASS/40)` thereafter. This is a
   FLOPS default, not a universal airline staffing rule. Airbus' A320 typical
   150-seat layout provides four attendant positions, and the A340 typical
   335-seat layout provides nine; these are tied to those layouts and not to
   arbitrary resized cabins. The other installed counts are unresolved or
   retained as explicit scenario candidates.
5. `galley_crew_count` (`NGALC`): NASA's published default is zero below 151
   passengers and `1 + ceil(NPASS/250)` at 151 or more. It is recorded as a
   model default; no retained aircraft source establishes dedicated galley
   crew for the baseline installations.
6–8. `first_class_passenger_count`, `business_class_passenger_count`, and
   `tourist_class_passenger_count`: these are installed class counts, not the
   planning passenger total. Source layouts are A320 12/0/138, A340 30/0/305
   (typical 335-seat layout), A380 22/96/437 (typical 555-seat layout), and
   B787-9 0/28/262. The A220 operator WBM is all-economy 140Y, while the
   registered scenario is 130 seats. The DC-10 source gives totals but no
   exact split. Resized all-economy candidate cabins are marked declared;
   they are not presented as actual installed airline cabins.
9. `hydraulic_pressure_pa` (`HYDPR`): the only retained aircraft-specific
   pressure is the A320 nominal 3000 psi, converted to
   20,684,271.879504 Pa using 6894.757293168 Pa/psi. A220, A340, A380, B787,
   DC-10, and ATR values are gaps or scenario conversions. The 3000/5000 psi
   choices in candidates are not universal defaults.
10. `variable_sweep_penalty` (`VARSWP`): the real aircraft have fixed swept
    wings, so zero is source-backed as the fixed-geometry endpoint. The NASA
    coefficient is not a wing sweep angle. `FCOMP` is separate and is not a
    literal percentage of composite structural mass.
11–12. `wing_mounted_engine_count` (`FNEW`) and
    `fuselage_mounted_engine_count` (`FNEF`): A220, A320, A340, A380 and B787
    map to 2/0, 2/0, 4/0, 4/0 and 2/0 respectively. DC-10 maps to 2/1 because
    its third CF6 is mounted in the fin root. ATR is 2/0 but unsupported by
    the transport equations. The counts must equal the actual installed
    engine count.
13. `fuel_tank_count` (`NTANK`): the count is a declared FLOPS topology, not
    an arbitrary count of symmetric cells. The A320 has three functional tanks
    even though a retained cell representation has five symmetric cells. The
    A340 record exposes six modelled cells (two inner, two outer, centre and
    trim) and the alternate three functional groups; this definition is still
    unresolved. The A380 has seven (inner, mid, outer pairs plus trim), and the
    B787 has three (inner-wing and centre). A220 has three in the operator WBM;
    DC-10 four is an explicitly uncertain topology because the ACAP does not
    publish a complete cell count.
14. `maximum_fuel_capacity_kg` (`FMXTOT`): use a source mass where available.
    Anchors are A220 17,400 kg (operator WBM), A320 19,004 kg (24,209 L at
    0.785 kg/L), A340 113,200 kg, A380 253,983 kg (323,546 L at 0.785 kg/L),
    and B787-9 101,522 kg. The A380 EASA 324,339 L/259,471 kg system-inventory
    quantity is recorded as an alternate definition. DC-10 110,007 kg is a
    transparent 137,509 L × 0.8 conversion and remains uncertain. ATR 5000 kg
    is reference only.
15. `containerized_cargo_kg` (`WCARGO`): this is mass carried in standardized
    containers. It is not the requirements' payload-cargo mass and not the
    volume or ULD-position count. A320, A340, A380, B787-9, and DC-10 publish
    some ULD or compartment information but not the exact containerized mass
    for the registered mission, so the source value is null and the zero
    candidates are explicit scenario choices. A220 and ATR are bulk-only in
    the retained references, so zero standardized-container cargo is factual
    for those references (ATR still remains unsupported).

The full field records include the source ID, revision, page/section/table or
data-module locator, units, applicability, and uncertainty. `source_backed`
in the production provenance enum corresponds to the document-backed
`verified_source` label used in this artifact. The other labels are kept
separate from that enum so a default, declared scenario, estimate, unsupported
technology, or notional record cannot be mistaken for an aircraft measurement.

## Geometry fidelity and propulsion coverage

The outer-geometry evidence is deliberately graded. A short source value does
not imply that the FLOPS geometry is a faithful aircraft model.

- **A220-300:** 35.10 m span and 38.70 m length are primary; 15.232380 m
  wheelbase and 6.731 m track come from ACP Issue 013 for S/N 55001–59999.
  The 112.30 m² area and 3.781 m MAC are operator-WBM values. PW1521G-3
  fan-case diameter is 2.006 m and bare engine length is 3.045 m from EASA
  IM.E.090; certified nacelle/pylon dimensions are unavailable.
- **A320-214 WV017:** 35.80 m Sharklet span, 37.57 m fuselage length, 3.95 m
  body width, 4.14 m body height, 4.1935 m MAC, 12.64 m wheelbase, 7.59 m
  track, and 5.755 m engine offset are retained from Airbus/EASA records.
  Area, sweep, taper, and fan diameter are secondary or equivalent-shape
  values. CFM56-5B4/3 SAC bare dry mass is 2454.8 kg from EASA E.003;
  installed nacelle, reverser, pylon, and installation mass are unavailable.
- **A340-312 WV029:** primary span 60.30 m, length 63.66 m, body diameter
  5.64 m, wheelbase 25.375 m, track 10.684 m, and CFM56-5C nacelle
  5.69 m × 2.43 m are retained. Area is a secondary 361.60 m² value and the
  engine station is an estimate. Exact CFM56-5C3/F dry mass is a source gap;
  the A320 engine mass must not be substituted.
- **A380-841 WV000:** primary 79.75 m span, 845 m² area, 33.5° quarter-chord
  sweep, 72.72 m length, 7.14 m body width, 8.41 m body height, 28.61/31.88 m
  nose-to-body/wing-gear wheelbases, and 14.34 m track. MAC and detailed
  planform are not retained. Trent 970-84 bare dry mass is 6246 kg from EASA
  E.012. The catalogue installation envelope is not a certified nacelle mass
  or a substitute for the bare-engine dimensions.
- **B787-9:** primary 60.1218 m span, 6.27126 m MAC, and Boeing 62.81 m
  overall length are retained. Area 377 m², sweep, taper, body dimensions,
  wheelbase, and track are secondary/derived values. The exact GEnx-1B74/75/P2
  dry mass and nacelle/pylon geometry are gaps. A 6126 kg value for an older
  GEnx family is retained only as a rejected proxy in the JSON.
- **DC-10-30:** NASA CR-3119 supplies primary 50.39 m span, 338.80 m² area,
  aspect ratio 7.5, and 35° quarter-chord sweep; EASA supplies the type length
  context. MAC, body width, wheelbase, and other installation dimensions are
  secondary or estimated. The 2/1 engine mounting is source-backed. Exact
  CF6-50C dry mass and nacelle/pylon geometry are gaps.
- **ATR72-600:** the factsheet supplies a useful 27.05 m span, 61 m² area,
  27.166 m length, and 3.93 m propeller diameter, but this is reference data
  only. The pinned NASA memorandum has no propeller/shaft-power transport
  branch, so these values cannot make the ATR a supported pure-FLOPS case.
- **AVE:** no geometry, engine, mass, cabin, or FLOPS value is promoted. It is
  a notional reference and remains N/A.

All bare engine masses explicitly exclude nacelle, pylon, thrust reverser,
fluids, and aircraft installation equipment unless a future source states
otherwise. The exact-engine mass coverage therefore is:

| Engine identity | Bare dry mass | Evidence | FLOPS `WENGB` (since 2026-09-12) |
|---|---:|---|---|
| PW1521G-3 | 2177 kg | EASA IM.E.090, Issue 10, III.5 (basic engine with standard equipment; reverser aircraft-side, Note 4) — verified | declared |
| CFM56-5B4/3 SAC | 2454.8 kg | EASA E.003, Issue 06, III.5 (basic engine, accessories, ECM; starter in type design; the -5B reverser is not in the engine parts list) — verified | declared |
| CFM56-5C3/F | 2644.4 kg | EASA E.003, Issue 06, III.5; the -5C dry weight contains the adapter kit with mixer, exhaust plug and thrust reverser — verified, not separable | equation 76 retained |
| Trent 970-84 | 6246 kg | EASA E.012, Issue 12, III.5 ("Not including fluids and Nacelle EBU") — verified | declared |
| GEnx-1B74/75/P2 | 6147.1 kg | EASA GEnx TCDS Issue 12, III.5 (basic engine, accessories, optional equipment); the 787 fan reversers are listed under the engine type design without a split — verified, scope unresolved | equation 76 retained |
| CF6-50C | null | FAA TCDS E23EA not retrieved; exact variant mass gap | equation 76 retained |
| PW127M | null | Reference family proxy only; ATR unsupported |
| AVE engine | N/A | Notional aircraft |

## Mass anchors and operating-item definitions

The mass artifact keeps the FLOPS comparison at the right level. NASA
equation 139's `WWE` is the structural, propulsion, systems, and margin
empty-weight build-up. NASA equation 140's `WOPIT` carries operating items,
including flight-deck crew, cabin attendants, baggage, unusable fuel, oil,
passenger-service/catering items, and cargo containers. Equation 141 forms
`WOWE = WWE + WOPIT`, which is the FLOPS analogue closer to an operator OEW or
DOW. A published manufacturer OEW is not interchangeable with either number
until its definition is known.

For every real preset the audit therefore records `unknown` for crew, cabin
crew, baggage, unusable fuel, oil, passenger service/catering, and container
inclusion when the source does not define them. Usable fuel is normally
excluded from OEW, but that too is left as a source-definition statement rather
than assumed as a measured fact. Since 2026-09-12 the operating-empty-mass
anchors are owned by the one OEW reference registry
(`crates/alas-config/src/oew_reference.rs`; see
[`mass-model-architecture.md`](mass-model-architecture.md), "The OEW
reference registry"), which supersedes the `oew_kg` rows of
`mass_reference_anchors.json`. Its verified corrections to the earlier
statements: the A320 41,244 kg figure is absent from Airbus ACAP Rev 46
(45,000 kg is "empty weight for maintenance", 41,000 kg is a rescue-drawing
configuration) and the 41,052 kg operator sheet belongs to a 77 t / 180Y /
wingtip-fence aircraft; the A220 37,149 kg is the Airbus recovery-publication
planning OEW with a stated inclusion list (the ACP prints 37,081 kg for the
same 140-seat cabin); the A340 131,215 kg is the "OEW" on the ACAP jacking
figure (weight variant and cabin unstated, printed pound value inconsistent);
the DC-10 572,000 lb option does have a retained ACAP value, the Series 30
passenger column plus its footnote (266,191 + 379 lb = 120,914 kg); the A380
277,000 kg and B787-9 128,850 kg remain aggregator / superseded-attribution
anchors with no retained primary page.

The strongest reference masses are the same-variant certified or manufacturer
limits: A220 legacy Issue 12 68,039/67,585/58,740/55,792 kg
(MRW/MTOW/MLW/MZFW), A320 WV017 78,400/78,000/66,000/62,500 kg, A340 WV029
260,900/260,000/188,000/178,000 kg, A380 WV000 562,000/560,000/386,000/361,000
kg, and B787-9 legacy ACAP 255,372/254,692/192,776/181,436 kg. ATR limits are
retained only as unsupported reference data. DC-10 standard ACAP masses
(253,105/251,744/186,427/177,355 kg) are explicitly separated from the
registered 572,000 lb scenario. No standard-row DC-10 value is silently
relabelled as the option.

Derived maximum payload differences such as A320 62,500 − 41,052 = 21,448 kg
and A380 361,000 − 277,000 = 84,000 kg are marked estimates because their OEW
anchors are not exact registered installations. They are comparison anchors,
not achieved payload-range results.

## Explicit boundaries

- No complete exact-variant design mission was found. A source-defined range
  must include the selected payload, flight profile, reserves/diversion,
  weight variant, engine, and cabin assumptions. The retained payload-range
  charts are partial evidence only.
- Cruise Mach remains a requirement or performance datum. It is never used as
  MMO. The A340 and DC-10 MMO gaps remain open pending an applicable primary
  AFM/TCDS locator; secondary proxies are not promoted.
- A class count is not a passenger total. Operator cabin layouts are tied to
  their published capacity and are not silently resized into a verified
  installation.
- `WCARGO` is containerized cargo mass. ULD positions, bulk cargo volume, and
  the requirements' payload cargo do not supply this input.
- `FCOMP` is a FLOPS technology coefficient ranging from the metallic endpoint
  toward the composite benefit assumed by the FLOPS fits. It is not a literal
  percentage of composite structural mass. No manufacturer source in this
  collection maps material percentage to `FCOMP`; the study value remains a
  declared metallic baseline only.
- No component breakdown for exact wing, fuselage, landing gear, systems,
  furnishings, crew, oil, unusable fuel, catering, containers, nacelle, pylon,
  or installed engine exists for these eight variants. Older NASA component
  references do not close that gap. No fudge coefficient, component scaler, or
  OEW calibration is permitted.
- ATR remains unsupported because the pinned NASA transport equations do not
  contain a propeller/shaft-power branch. AVE remains N/A because it is
  notional. Neither status can be changed by copying transport inputs.

The resulting evidence supports an auditable FLOPS input selection and a
clear statement of data quality. It does not by itself establish physical
validation of the ALAS mass model; that claim is limited by the unresolved
mission definitions, cabin installations, component masses, and exact engine
installation data above.
