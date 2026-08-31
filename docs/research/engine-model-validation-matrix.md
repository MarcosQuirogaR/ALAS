# Transport engine model validation matrix

Prepared 2026-08-30 for the unified ALAS propulsion architecture.

## Evidence policy

An engine record must identify an exact variant and operating condition. Rated
thrust or power, bypass ratio, pressure ratio, ICAO LTO fuel flow, cruise fuel
consumption and installation dimensions are not interchangeable design-point
properties.

ICAO LTO fuel-flow rows are certification test anchors at 7%, 30%, 85% and
100% rated thrust. They do not identify component maps, corrected-flow
schedules, altitude lapse, installation losses or cruise TSFC. A model may be
calibrated to some anchors and validated against different evidence; it must
not call the calibration anchors independent validation.

## Catalogue findings

| Current name | Evidence identity | Principal correction required |
| --- | --- | --- |
| CFM56-5B4/3 | ICAO UID `01P08CM105` | UID reports 120.10 kN, BPR 5.70 and OPR 27.30. Current values mix another family/design condition. |
| CFM56-5C | CFM56-5C4 proxy, UID `2CM015` | Rename or mark proxy. Anchor: 151.25 kN, BPR 6.60, OPR 31.15. |
| CFM56-5C3/F | CFM56-5C3 family match, UID `1CM011` | `/F` is not established as a separate emissions-engine variant. Anchor OPR is 29.90, not 38. |
| LEAP-1A | LEAP-1A26/26E1, UID `08P28CM155` | Use exact variant and condition. Anchor: 120.636 kN, BPR 10.706, OPR 33.177. |
| PW1500G compatibility selector | PW1521G / PW1521G-3, UID `20PW131` | The A220 preset is explicitly PW1521G-3. Use the 97.72 kN PW1521G LTO record; do not substitute the higher-rated PW1525G. |
| Trent 970-84 | UID `18RR081` | Anchor: 338.7 kN, BPR 8.45, OPR 38.0. Preserve any different certified rating only with its definition. |
| Trent 900 | Unspecified family member | Do not normalize a Trent 970 fuel schedule onto an unidentified 374 kN engine. Bind an exact variant or remove the duplicate. |
| GEnx-1B | GEnx-1B74/75/P2, UID `07P27GE235` | Store takeoff and top-of-climb parameters separately; BPR/OPR are condition-dependent. |
| GE9X | Generic 105,000 lbf class | No March-2026 ICAO fuel row. The current GEnx schedule is an explicit, unvalidated family proxy. |
| CF6-50 | CF6-50C, UID `3GE070` | Rename or make the family proxy explicit. Anchor: 224.2 kN, BPR 4.30, OPR 27.76. |
| PW127M | EASA PW100-series certified variant / ATR 72-600 installation | Normal AEO takeoff 1.846 MW; automatic-reserve/max takeoff 2.051 MW; MCT 1.864 MW; maximum climb 1.635 MW; maximum cruise 1.590 MW. Reserve power is not ordinary AEO takeoff. ATR publishes 762 kg/h for the complete aircraft at maximum cruise; this is a single calibration anchor, not an engine deck. |
| 568F-1 | Hamilton Sundstrand six-blade propeller | Diameter 3.93 m, nominal 1,200 rpm. Public CT/CP/pitch maps are unavailable; use a declared surrogate until independent data exist. |

The runtime PW127M/568F instance is constructed from the typed takeoff,
reserve, continuous, climb and cruise ratings, governed RPM, propeller
diameter and cruise fuel anchor. These values are not duplicated as runtime
constants: a valid derivative edit therefore changes both mission physics and
the technology-specific report figures. The enum rating values remain only as
the certified defaults used to initialize the built-in model.

## Required record structure

Each future catalogue entry should separate:

- certified identity and rating;
- operating condition for every parameter;
- uninstalled engine dimensions from installed nacelle geometry;
- measured/calibrated values from estimated or proxy values;
- calibration data from independent validation data;
- model validity domain and uncertainty;
- source URL, document revision, table/page and retrieval date.

## Implemented transport mission deck

All ten turbofan catalogue rows now dispatch through the same typed
Battel--Young/OpenAP thrust-lapse model in total-energy missions. The
top-of-climb reference is no longer embedded in mission code.

For identity-qualified ICAO rows, the typed empirical payload uses the ICAO
sea-level-static rated thrust. The ICAO takeoff bypass ratio is stored
separately from the conceptual cycle/design bypass ratio and is used only by
the Battel--Young takeoff lapse correlation. This prevents parameters from
different operating conditions being silently treated as one number. Older
typed configuration files that predate this split fall back explicitly to the
cycle BPR and remain readable.

Only CFM56-5C4 and CF6-50C have exact-identity OpenAP cruise anchors in the
current ALAS catalogue. CFM56-5C3 is a family proxy because the ALAS selector
adds an unverified `/F` suffix; CFM56-5B4/3 uses the related -5B4 anchor. The
remaining engines use OpenAP's disclosed Svoboda fallback
`T_cr = 0.2 T_SLS + 890 N` per engine and remain explicitly **Extrapolated**.
This is a coherent preliminary-design model, not OEM-deck validation.

The empirical fuel path now reproduces each record's absolute ICAO
sea-level-static takeoff and idle fuel-flow anchors and uses shape-preserving
interpolation across the four LTO thrust fractions. Applying that schedule at
altitude is still an extrapolation and is not independently validated at
cruise. Maximum-continuous thrust currently shares the maximum-climb
envelope because no public engine-specific MCT decks were found. The OpenAP
7% descent-idle approximation is a model assumption, not a certified rating.

The normalized-force control contract is bounded by that same declared
maximum-climb envelope. Commands below the modeled flight-idle force saturate
at idle, return the achieved (not requested) normalized demand, and expose an
active `flight-idle-thrust` limit. This keeps solver capability, evaluated
force and fuel flow mutually consistent. The separate takeoff rating remains
available only through the named takeoff/go-around demand.

OpenAP's pinned fuel implementation likewise evaluates fuel as a function of
total aircraft thrust, using an engine/aircraft polynomial fitted from ICAO
points; its en-route method first computes required aircraft thrust and then
calls that thrust-based fuel model. It does not provide an independent
altitude correction to the engine fuel law. ALAS therefore does not claim that
substituting OpenAP's curve for the present shape-preserving interpolation
would constitute altitude validation.

Battel and Young report takeoff-thrust agreement within ±1% only for their
reference two-shaft turbofans and only through Mach 0.4. Applying the
correlation to three-spool Trent engines, the geared PW1500G, or newer UHBR
LEAP/GEnx/GE9X engines is an extrapolation and is labelled accordingly.

## Primary sources

- [EASA-hosted ICAO Engine Emissions Databank, March 2026](https://www.easa.europa.eu/en/downloads/131424/en)
- [EASA CFM56 type-certificate data](https://www.easa.europa.eu/sites/default/files/dfu/TCDS%20EASA%20E.003%20issue%2006.pdf)
- [EASA LEAP type-certificate data](https://www.easa.europa.eu/en/downloads/20086/en)
- [EASA PW100-series type-certificate data](https://www.easa.europa.eu/sites/default/files/dfu/EASA.IM_.E.041_TCDS_Issue_7.pdf)
- [EASA Trent 900 type-certificate data](https://www.easa.europa.eu/en/downloads/7779/en)
- [GE GEnx data sheet](https://www.geaerospace.com/sites/default/files/datasheet-genx.pdf)
- [GE9X product data](https://www.geaerospace.com/commercial/aircraft-engines/ge9x)
- [GE CF6 product data](https://www.geaerospace.com/commercial/aircraft-engines/cf6)
- [Pratt & Whitney PW1500G product card](https://prd-sc102-cdn.rtx.com/-/media/pw/products/commercial-jet-engines/pratt-and-whitney-gtf/family/files/pw_gtf_pc_pw1500g_2021_web.pdf)
- [NASA PW127E-like NPSS/FLOPS study](https://ntrs.nasa.gov/api/citations/20160007763/downloads/20160007763.pdf?attachment=true)
- [ATR 72-600 manufacturer factsheet](https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf)
- [Battel & Young (2008), simplified two-shaft turbofan models](https://doi.org/10.2514/1.35589)
- [OpenAP model paper](https://doi.org/10.3390/aerospace7080104)
- [Pinned OpenAP engine dataset](https://github.com/TUDelft-CNS-ATM/openap/blob/46753acb988c1e8c9d47ec5ae72d6fb75b9cf260/openap/data/engine/engines.csv)
- [Pinned OpenAP thrust implementation and fallback](https://github.com/TUDelft-CNS-ATM/openap/blob/46753acb988c1e8c9d47ec5ae72d6fb75b9cf260/openap/thrust.py)
- [Pinned OpenAP thrust-based fuel implementation](https://github.com/TUDelft-CNS-ATM/openap/blob/46753acb988c1e8c9d47ec5ae72d6fb75b9cf260/openap/fuel.py)
