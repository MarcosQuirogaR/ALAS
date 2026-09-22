# ALAS geometry-first cabin renderer

Standalone prototype that consumes resolved `alas.cabin-scene/v2` SI geometry, validates it with GEOS/Shapely, and emits deterministic semantic SVG. CairoSVG is an optional final rasterization step. It never stretches or rescales physical components to make them fit: invalid geometry is reported, and `--allow-invalid` may be used to retain an annotated engineering preview.

## Install and run

```powershell
cd tools\cabin_renderer
py -m venv .venv
.venv\Scripts\pip install -r requirements.txt
$env:PYTHONPATH = (Get-Location).Path
.venv\Scripts\python -m cabin_renderer.cli tests\fixtures\a320_export_v2.json --station 10 --deck main --deck lower --svg out\a320.svg --png out\a320.png --allow-invalid
.venv\Scripts\python -m cabin_renderer.cli tests\fixtures\a320_export_v2.json --batch-dir out\all_stations --allow-invalid
.venv\Scripts\python -m cabin_renderer.cli ..\..\outputs\cabin_scene_v2_fixtures\a320_200_cabin_scene_v2.json --recommended --svg out\a320_recommended.svg --png out\a320_recommended.png --allow-invalid
.venv\Scripts\python -m cabin_renderer.cli ..\..\outputs\cabin_scene_v2_fixtures --recommended-dir ..\..\outputs\cabin_renderer_python\recommended --allow-invalid
.venv\Scripts\pytest
```

Exit code 2 means the scene parsed but failed a geometric assertion. `--allow-invalid` changes that to exit code 0 while retaining findings in the CLI JSON and SVG metadata.

## Contract implemented

Required top-level `schema_version` is exactly `alas.cabin-scene/v2`; units must be `m/kg/rad`. The adapter consumes the Rust exporter's `stations`, `decks`, `seat_rows`, `seats`, `windows`, `overhead`, and `cargo` fields directly. `--station` requires an exact exported station. Repeated `--deck` filters the section, while `--batch-dir` renders every exported station as SVG and PNG.

The validator checks finite JSON numbers, valid geometry, liner/shell containment, component containment, seat/person–OHCP intersections, ULD overlap, and a continuous bin-to-liner attachment path. SVG groups retain component IDs, types, accessible labels, ULD standards, and an exact metric view box. Stable sorting and numeric formatting make output hashes reproducible.

Recommended rendering is a single physical transverse section. The selector requires one exported station to intersect a seat row on every passenger deck and, when resolved cargo exists, at least one cargo item. It fails explicitly when no such station exists; it never combines independent deck/hold stations or projects a window from another longitudinal coordinate. Cargo labels report three separate quantities: ULD/hold-liner cross-sectional area utilization, gross liner area below the lowest passenger floor, and the exporter's existing item load/fill proxy. The latter is not presented as cargo-bay area or volume utilization. Unoccupied underfloor and hold-liner polygons are retained and shaded independently.

For multi-aisle rows only, a missing center OHSC may be shown as a `visualization_only_topology_fallback`. Regular, large, and extra-large parameter slots follow the DLR paper's topology taxonomy, but their provisional renderer dimensions are not attributed to DLR. Single-aisle rows are never given a center bin.

The included fixture matches the serialized Rust exporter shape. Missing topology is never fabricated: current exports therefore report `OHCP_NOT_ATTACHED`, and their nominal windows are carried as explicit diagnostics. Production data still needs station-indexed manufacturer contours, occupant envelopes, authoritative window apertures, bin/PSU profiles and attachment geometry, independent hold liners, empty cargo slots, and exact ULD orientation.

## Bibliography and design basis

- Shapely documentation, *The Shapely User Manual*, geometry predicates, set operations, validity, and unary union: https://shapely.readthedocs.io/en/stable/manual.html (accessed 2026-08-30).
- OGC, *Simple Feature Access – Part 1: Common Architecture*, geometry model and topological predicates, version 1.2.1: https://www.ogc.org/standards/sfa/ (accessed 2026-08-30).
- W3C, *Scalable Vector Graphics (SVG) 2*, paths, groups, coordinate systems, metadata, and vector effects: https://www.w3.org/TR/SVG2/ (accessed 2026-08-30).
- CairoSVG documentation, deterministic headless SVG conversion interface: https://cairosvg.org/documentation/ (accessed 2026-08-30).
- Fuchs et al., *An Approach for Linking Heterogenous and Domain-Specific Models to Investigate Cabin System Variants*, DOI 10.1002/iis2.13090: supports persistent cross-domain IDs, multi-fidelity asset substitution, and OHSC variant topology; it does not publish dimensioned bin profiles (accessed 2026-08-30).
- IETF RFC 7946, *The GeoJSON Format*, geometry interchange representation: https://www.rfc-editor.org/rfc/rfc7946 (accessed 2026-08-30).
- Airbus, *A320 Aircraft Characteristics Airport and Maintenance Planning*, cargo-compartment and ULD arrangement source family: https://www.airbus.com/en/products-services/commercial-aircraft/aircraft-characteristics (accessed 2026-08-30). Exact aircraft revision must be recorded by the production exporter.
- IATA, *Unit Load Devices*, ULD identification and operational context: https://www.iata.org/en/programs/cargo/cargo-operations/unit-load-devices/ (accessed 2026-08-30). Certified contours and compatibility require the licensed/current ULDR or aircraft weight-and-balance data.

This bibliography supports the implementation method. It does not turn the provisional fixture geometry into certified aircraft data.
