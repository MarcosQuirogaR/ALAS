# ALAS audit remediation: closure note

Snapshot: 2026-09-05, Europe/Madrid. Workspace: `C:\Proyectos\ALAS`, HEAD
`be31873`, working tree uncommitted. Nothing was committed or pushed.

The durable record is `docs/STATUS.md` ("Audit remediation, 2026-09-05"),
which lists the closure status of audit findings F1-F12, the physical
findings the re-pinned acceptance matrix surfaced, and what was not
verified. `docs/PORTING.md` rows were flipped to `green` only where the
corresponding parity test passes. This note only records how the final
verification ended and what a reviewer should look at first.

## Final verification on the working tree

- `cargo fmt --all -- --check`: clean.
- `cargo xtask checks`: pass (78 reviewed rows in
  `docs/source-size-budgets.tsv`; the assembled-size ceilings equal the
  post-remediation sizes and may not grow).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --no-fail-fast`: the standalone --no-fail-fast run aborted on a transient Windows linker file lock (LNK1104 on an example binary) before any suite ran; the gate's own `cargo test --workspace` stage completed: 204 suites, 2305 passed, 0 failed, 17 ignored.
- `cargo xtask gate`: pass (checks, format, clippy, tests).
- `cargo deny --locked check` (vendored 0.20.2): advisories, bans, licenses
  and sources ok, with the two recorded maintenance-notice exceptions.

Logs (ignored, local): `.agent/reports/audit-final*.log`,
`.agent/probe-*.log`, `.agent/reports/audit-dependencies-2.log`.

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
- The CI workflow was edited but not executed remotely.
- No physical validation against flight data was attempted; a green suite is
  a verified translation and a consistent product, not a verified aircraft.
