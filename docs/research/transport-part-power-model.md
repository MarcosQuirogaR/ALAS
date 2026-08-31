# Transport turbofan part-power model

## Implemented fidelity and semantics

The product mission command is a **normalized requested net-thrust fraction**,
not a physical power-lever angle. At each altitude and Mach number the existing
station network provides the maximum-rating thrust and fuel flow. Requested
thrust remains `F = lambda F_max` because `lambda` is the solver's thrust
request. Fuel is no longer assumed proportional to thrust:

`m_f(lambda) = m_f,max g(lambda)`

where `g` is a monotone piecewise-cubic interpolant through the engine's
normalized ICAO LTO fuel-flow anchors at thrust fractions 0.07, 0.30, 0.85,
and 1.00. The mission solver represents operating engines, so commands below
7% saturate at the idle thrust and fuel anchors. Engine shutdown/windmilling
requires a future explicit engine-state variable and is not inferred from a
zero thrust request. Consequently TSFC is
derived from the interpolated fuel flow and requested thrust rather than held
constant. Frozen predecessor parity uses an explicit `LegacyLinear` policy and
is not product physics.

This is a Level-1 empirical schedule. ICAO LTO points are sea-level-static
certification data; multiplying their normalized shape by the local
maximum-rating fuel flow is a separability assumption, not altitude/Mach
validation. It does not model a power lever, corrected spool speed, compressor
maps, surge margin, bleed/extraction, variable geometry, thermal limits,
transients, reverse thrust, windmilling, or relight. A real engine deck or
component-matched off-design model remains the required Level-2 replacement.

## Data provenance

Source workbook: EASA-hosted **ICAO Aircraft Engine Emissions Databank,
March 2026**, downloaded from the official EASA page on 2026-08-30. Fuel-flow
columns are take-off, climb-out, approach, and idle in kg/s; stored ratios are
idle/take-off, approach/take-off, climb-out/take-off, and 1.0.

| ALAS engine | EEDB UID / identification | Status |
|---|---|---|
| CFM56-5B4/3 | 01P08CM105, CFM56-5B4/3 | direct variant |
| CFM56-5C | 2CM015, CFM56-5C4 | rating proxy for generic family entry |
| CFM56-5C3/F | 1CM011, CFM56-5C3 | direct base variant |
| LEAP-1A | 08P28CM155, LEAP-1A26/26E1 | rating match |
| PW1500G | 04P20PW195, PW1525G | family/rating match |
| Trent 970-84 | 18RR081, Trent 970-84 | direct variant |
| Trent 900 | 18RR081, Trent 970-84 | family match |
| GEnx-1B | 07P27GE235, GEnx-1B74/75/P2 | rating match |
| CF6-50 | 3GE070, CF6-50C | family/rating match |
| GE9X | 07P27GE235, GEnx-1B74/75/P2 | explicit family proxy; GE9X absent from the release |

The source UID/proxy statement travels with every live `EngineConfig`, so a
saved or edited design does not silently consult the database again.

## Numerical method

The interpolation uses the Fritsch-Carlson monotonicity limiter. It reproduces
the anchors, remains bounded between adjacent nodes, and prevents negative or
overshooting fuel flow. Configuration ingress requires four finite,
nondecreasing ratios and a final ratio of 1.0.

## Bibliography

1. European Union Aviation Safety Agency, “ICAO Aircraft Engine Emissions
   Databank,” March 2026. <https://www.easa.europa.eu/en/domains/environment/icao-aircraft-engine-emissions-databank>
2. J. A. DeCastro, J. S. Litt, and D. K. Frederick, “A Modular Aero-Propulsion
   System Simulation of a Large Commercial Aircraft Engine,” NASA/TM-2008-215303,
   2008, DOI `10.2514/6.2008-4579`.
   <https://ntrs.nasa.gov/search.jsp?R=20080043619>
3. F. J. Lallman, “Simplified Off-Design Performance Model of a Dry Turbofan
   Engine Cycle,” NASA TM-83204, September 1981.
   <https://ntrs.nasa.gov/api/citations/19810024663/downloads/19810024663.pdf>
4. J. D. Mattingly, *Elements of Gas Turbine Propulsion*. AIAA, 2005,
   ISBN 1-56347-778-5.
5. F. N. Fritsch and R. E. Carlson, “Monotone Piecewise Cubic Interpolation,”
   *SIAM Journal on Numerical Analysis*, 17(2), pp. 238–246, 1980,
   DOI `10.1137/0717021`.

NASA C-MAPSS and Mattingly support the conclusion that genuine off-design
performance requires component maps and matched shaft/nozzle/control states.
NASA TM-83204 supports low-order off-design methodology but its low-bypass
example is not used to calibrate modern high-bypass engines. The ICAO/EASA
databank supplies the implemented empirical anchors; it does not validate them
aloft.
