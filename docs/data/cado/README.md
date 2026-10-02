# CADO airplane database (ENAC), v1.3

`CADO_airplane_database_v1.0.csv` is the unmodified CADO airplane database
published by ENAC. It is a separately licensed database under the Open Data
Commons Open Database License 1.0 (ODbL), whose complete text is retained
beside it as `DATABASE_LICENSE.txt`. It is **not** relicensed under the ALAS
code licence (AGPL-3.0-or-later), and it is not compiled into, embedded in or
read by any ALAS production code.

## Citation and attribution

Monrolin, N.; Druot, T.; Peteilh, N.; Roches, P.; Kambiri, Y.-A.,
*CADO airplane database*, Version 1.3, Recherche Data Gouv, 2024.
DOI [10.57745/LLRJO0](https://doi.org/10.57745/LLRJO0).
Contains information from the CADO airplane database, which is made available
under the ODbL 1.0 (<https://opendatacommons.org/licenses/odbl/1-0/>).

The authors ask that reuse also cite: Kambiri, Y.-A. et al., "Energy
consumption of aircraft with new propulsion systems and storage media",
AIAA SciTech 2024 Forum, doi:10.2514/6.2024-1707.

## Provenance

| File | Upstream datafile | Bytes | MD5 (matches upstream) | SHA-256 |
| --- | --- | ---: | --- | --- |
| `CADO_airplane_database_v1.0.csv` | 188268 | 63,422 | `9186f33806c37552e17961d5a034085a` | `6b986349811e66f50039347f7c3450636f8700b1ed0ad5ee7e523d89b8777180` |
| `DATABASE_LICENSE.txt` | 188266 | 25,819 | `15cb7e8a7484bea21b9e937c62c108c4` | `3e6a0a93517183d271666f3ec2f91f9770b62a7b3464a61259b33c8ee058d1a7` |

Retrieved 2026-10-02 with
`curl -L -f -o <file> https://entrepot.recherche.data.gouv.fr/api/access/datafile/<id>`.
The upstream file name keeps `v1.0` although the dataset version is 1.3.
Both files are stored byte-for-byte (CRLF line endings; see `.gitattributes`).

## Format and units

Semicolon-separated, 288 aircraft rows after three header rows: column names,
units, and a per-column override row. Units as declared by the file: lengths
m, areas m², sweep deg, masses kg, thrust N (sea-level static, per engine),
power kW, range km, altitude ft, approach speed km/h, TOFL/LFL m. `max_speed`
and `cruise_speed` are Mach for jet rows (the override row says `mach`) and
km/h for propeller rows. `max_fuel` is a mass in kg; its density basis and
usable/total definition are not stated.

## Status of the data

The dataset description states that it is compiled from manufacturer
websites, flight manuals, books and the EUROCONTROL aircraft performance
database to give orders of magnitude and trends for preliminary design, and
that it is **not intended for operational use**. ALAS treats every value as
secondary evidence: it may corroborate or flag a primary anchor in
`golden/aircraft/real_aircraft_parity.json` but never scores a model output.
Individual values carried there are an insubstantial extract cited back to this
database; the database itself is not merged into that file.
