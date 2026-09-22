# A320 FLOPS audit evidence

This evidence package reconstructs the exact study identity and maps it to NASA FLOPS definitions without promoting unsupported values to aircraft truth. The selected case is **A320-214 / WV017 / MOD160500 Sharklet / MOD37147 CFM56-5B4/3 Tech Insertion / MOD37331 + MOD160001 / three tanks / no ACT**.

The package is `READY_WITH_GAPS`. Identity, certified limits, selected fuel table, headline dimensions, engine rating, rule-level loads and FLOPS equations are source-backed. Selected-aircraft OEW, WBM/load sheet, component statement, complete engine installation split, full wing geometry, oleo lengths, actual optional interior/service inventories, selected hydraulic pressure and A320-specific tested loads remain unavailable. No production or test code was changed.

## Machine-readable artifacts

This document is the published record of the audit. The underlying machine-readable working files are retained in the maintainers' local audit trail; they are not part of the repository checkout and are not published, and should be regenerated from the primary sources cited below before relying on them:

- `source_manifest.json`: URLs, authority, revision/date, local file, SHA256, page/section locators, applicability and uncertainty.
- `reconstructed_input_deck.json`: source-conditioned candidate inputs, per-field status, FLOPS variable mapping, current/prior snapshots, OEW verdict and holdout freeze recommendation.
- `equation_map.json`: NASA transport equations 1–17, 46, 50, 56, 63–69, 73–92, 97, 101–126 and 136–145, variable definitions, ALAS mapping and coverage gaps.
- `raw_source_anchors.json`: short raw text anchors with source IDs, local files, PDF pages, claims and uncertainty.
- `quantity_comparison.csv`: 97 quantity-by-quantity rows comparing pure baseline, prior cabin refinement and the reconstructed candidate.
- A detailed reconstruction report with the evidence interpretation and bounded gaps.

## Values that are safe to carry as selected-case inputs

EASA A.064 Issue 62 (26 June 2026) and Airbus ACAP Revision 46 (July 2026) agree on WV017 limits: MRW 78,400 kg, MTOW 78,000 kg, MLW 66,000 kg and MZFW 62,500 kg. EASA gives MMO 0.82, two flight crew, and the selected MOD37147 CFM56-5B4/3 applicability. Airbus/EASA dimensions give 37.57 m length, 3.95 m outside fuselage width, 35.80 m Sharklet span, 7.59 m track and 12.64 m wheelbase. Airbus public data give 27.51 m cabin length and 3.70 m maximum cabin width.

For MOD37331 + MOD160001, EASA reports 15,919 L inner-wing fuel, 8,248 L center fuel, 24,167 L usable total, 0.8 kg/L table density, and 82.1 L / 65.7 kg unusable fuel. The current configuration's tank-layout values sum to the conditional/common 24,209 L table, while its maximum fuel mass is 19,334 kg from the selected table. This 42 L configuration mismatch is recorded and left untouched in production.

The Airbus typical 150-seat diagram is 12 first class + 138 economy, four attendants, three lavatories and two galleys. It is a planning scenario, not proof of the selected operator's cabin, optional interior, catering/water load or OEW definition. EASA lists a separate 150-seat certified option with three cabin crew under MOD150364. The prior 12/138/4 refinement is retained as a declared candidate sensitivity.

## OEW and FLOPS interpretation

NASA defines `WOPIT = WFLCRB + WSTUAB + WUF + WOIL + WSRV + WCON` and `WOWE = WWE + WOPIT`. The terms represent crew/baggage, attendants/galley crew/baggage, unusable fuel, engine oil, passenger service and cargo-container tare. FLOPS correlation output is not automatically a weighed installed inventory.

There is no public matched OEW for this A320-214 WV017 configuration in the frozen source set. The 41,052 kg F-HDRF operator value is secondary and conditional: it is a different 77,000 kg MTOW aircraft with unspecified empty-weight contents. The current 41,244 kg preset declaration is unsupported by current Airbus ACAP Rev46. ACAP's current ground-clearance figure instead labels 45,000 kg “EMPTY WEIGHT FOR MAINTENANCE,” which is not OEW. The package therefore marks the matched numeric reference as `not_matched`; no residual was assigned.

NASA Eq. 76 says the baseline engine weight includes inlet/nozzle only when those are not specified separately. Current ALAS has no declared baseline inlet/nozzle masses, so its 2,226.689 kg/engine `THRSO/5.5` result is a conditional fallback. EASA CFM56 E.003 gives 2,454.8 kg dry/basic engine mass and identifies starter/reverser type-design ownership, but no numerical A320 installation breakdown. Inlets, nozzles, starter, reverser, nacelle, pylon and EBU coverage must remain separate or unknown in any review.

CS-25.337 supplies a +2.5 g positive lower bound and −1.0 g negative bound; CS-25.303/25.305 supplies a 1.5 safety factor. The current +3.75 g ultimate input is only `1.5 × 2.5` screening arithmetic. FLOPS gear inputs are extended oleo lengths, not wheelbase or track; the current 2.41263/1.68884 m values remain Eq. 66/67 estimates.

## Current versus prior model result

The pure production record has model OEW 39,119.153 kg, structural group 21,356.745 kg, propulsion group 5,810.975 kg, systems group 10,193.458 kg, operating items 1,757.975 kg and closure fuel 23,880.847 kg. The prior 12/138/4 cabin refinement changes the model OEW to 39,653.046 kg (+533.893 kg), systems group to 10,563.589 kg, operating items to 1,921.737 kg and closure fuel to 23,346.954 kg. The systems difference is the cabin-dependent furnishings correlation; no independent installed-systems source correction was made. It does not establish physical validation. Both closure-fuel values exceed the selected 19,334 kg capacity, so the difference must be treated as a FLOPS closure/capacity issue rather than a fuel-capacity correction.

## Recommended holdout

Freeze **A220-300 BD-500-1A11, S/N 55001–59999** before any fit results. Airbus A220 ACP Issue 013 provides geometry/gear planning data, Airbus A220 ARP Issue 111 provides planning OWE and component data including 3,220.56 kg engine+nacelle per engine, and EASA IM.A.570 Issue 25 provides current certification identity. Freeze exact serial/configuration applicability, source revision, URL, hash, units and component/OEW definition, then apply only demonstrated general corrections unchanged. Do not add aircraft-specific offsets or fit-derived geometry.
