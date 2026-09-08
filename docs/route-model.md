# Generated airway route model

The mission pipeline reads coordinates directly from X-Plane 600/640 airway
endpoints. A named intersection catalog does not contain all VOR/NDB stations,
so resolving every airway endpoint against that catalog removes valid edges.
Each node is keyed by identifier and coordinates, retaining regional duplicates.
The [X-Plane airway format specification](https://developer.x-plane.com/article/airway-data-awy-dat-file-format-specification/)
defines those coordinate columns and explicitly permits navigation aids as endpoints.
Modern region/type records continue to use the existing parser.

`mission.use_airway_endpoint_coordinates` defaults to `true`.
`mission.max_airway_stretch` defaults to `1.20`; routes generated from the graph
above that distance/great-circle ratio fall back to the route source
`great_circle`. The guard is a configurable conceptual-model quality threshold,
not an operational routing restriction or a fit to observed city-pair flights.
Dispatched SimBrief and imported KML paths retain precedence and are not filtered.
Set the coordinate setting to `false` and the stretch setting to `0` to reproduce
legacy routing. The low-level `NavdataGraph::parse`, `load_navdata`, and
`plan_route` entry points retain their parity behavior.

For context, [Teoh et al. (2024), GAIA](https://doi.org/10.5194/acp-24-725-2024)
reports a global mean whole-flight extension of 5.2% in 2019. The 20% threshold
is an engineering choice, deliberately wider than that mean, and is not a
universal physical bound. Valid real flights can exceed it.

## Numerical verification

The installed navigation snapshot identifies itself as cycle 2012.08. Comparing
the same airport coordinates and unchanged shortest-path algorithm gives:

| Pair | Great circle (km) | Legacy ratio | Complete endpoint ratio |
|---|---:|---:|---:|
| LEMD–LEPA | 546.202 | 2.48236 | 1.18068 |
| EVRA–ESSA | 465.719 | 3.80142 | 1.01619 |

Both corrected paths pass the 1.20 threshold without a great-circle fallback.
These are numerical regression results on an old dataset, not physical
validation against measured tracks or evidence of a current permissible route.
The shortest-path graph still omits SID/STAR, altitude restrictions, directional
restrictions, airspace closures, and wind optimization. Distances are spherical
great-circle segment sums using an Earth radius of 6,371,000 m.

Snapshot SHA-256:

- `earth_awy.dat`: `EB2C2671C2E29618C8769677E00C05CCFCD976697632CB74DFB34BFB8BE96E24`
- `earth_fix.dat`: `AE60780B7BAB8F09F93C3396349D63B4C3B471C27206BD8BB87BAA21A3C15ED6`

`cargo test -p alas-route` covers synthetic missing navaids, regional duplicates,
legacy parity, modern-format compatibility, and the detour rejection behavior.
Set `ALAS_TEST_NAVDATA` to the snapshot directory and run
`cargo test -p alas-route --test airway_coordinates -- --include-ignored --nocapture`
to reproduce the optional installed-data comparison. The navdata files are not
bundled in the repository.
