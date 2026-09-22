# Audit remediation: reviewer notes

This is a maintainer coordination note, not a user guide or legal notice. The
durable record is `docs/STATUS.md` ("Audit remediation, 2026-09-05"), which
lists the closure status of audit findings F1-F12, the physical findings the
re-pinned acceptance matrix surfaced, and what was not verified. `docs/PORTING.md`
rows were flipped to `green` only where the corresponding parity test passes.
This note records what a reviewer should look at first.

## Review first

1. `crates/alas-acceptance/tests/acceptance_matrix.rs`: the A220 and A320
   pins were re-derived on the source-corrected presets. The A220 test
   recomputes the model and public CG frames from primitives and requires
   them to name one station; the aft-limit, nose-gear and landing-mass
   findings are asserted as reported, not passed.
2. `crates/alas-payload/src/cabin/seating.rs` and
   `crates/alas-payload/src/cargo/manager_parts/impl.rs`: the
   reference-compatibility interior resolves the historical premium slot and
   keeps the frozen bulk position; the product envelope path is unchanged.
3. `crates/alas-mission/src/vehicle.rs`: the compatibility request keeps
   the typed engine payload, because the flat thrust key is derived from it.
4. `crates/alas-i18n/data/es_catalog.json` is back to the 819 frozen
   entries; the 37 GUI strings it had accumulated, plus the uncatalogued
   strings from the latest GUI additions, live in
   `es_native_desktop_catalog.json` (duplicate keys in that file were
   collapsed, last value winning, which is what the parser already did).
5. `crates/alas-pipeline/src/pipeline_tests_parts/part_01.rs`: the
   fixed-design finalist test requests 340 passengers; a new test pins the
   default brief's one-seat shortfall as a reported finding.

## Not done

- NASTRAN-95 was not rebuilt or re-timed; the NOSA record stays incomplete.
- No physical validation against flight data was attempted; a green suite is
  a verified translation and a consistent product, not a verified aircraft.
