# 0008. Test tiers and the gate

Status: Accepted (2026-10-22)

## Context

The workspace has roughly 5,800 tests. Running everything on every change took
over thirteen minutes at three threads, which discourages running anything.
Skipping heavy tests silently would let regressions through.

## Decision

1. Tests are split into tiers (disjoint, except that `integration` also runs the nightly tests that live in its binaries) by cargo-nextest profile in
   `.config/nextest.toml`: `default` (also `fast`; every change), `medium`
   (tests of 5 s or more), `integration` (also `slow`; all-preset replays,
   acceptance matrices, staged searches), `nightly` (tests named `*_nightly`),
   `merge` (default plus medium) and `full` (default, medium and integration).
2. The `integration` and `medium` filters are generated from measured
   per-test times (`cargo xtask tiers <junit.xml> --write`), never edited by
   hand; a unit test fails if the profiles disagree. Regenerate after large
   changes.
3. Nothing is skipped by the gate. `cargo xtask gate` runs the repository
   checks, formatting, lints and the `full` profile; `--quick` runs only the
   packages a change affects and their dependents (the pre-commit hook);
   `--integration` runs the heavy tier alone; `--nightly` the nightly tier.
4. CI mirrors this: pull requests run the quick gate on Linux and are skipped
   for docs-only changes through a classification job (so the required check
   never stays pending); pushes to the main branch run the full gate on
   Windows; a nightly schedule adds the nightly tier.
5. Tolerances in parity tests come from the fixed `Tier` enum in
   `alas-testkit` (ADR 0013), never chosen per test.
6. Doctests are not used; the repository checks reject runnable doc examples.
   Tests that need the same nominal assessment share it across processes through
   the shared result store (`alas_testkit::memo`).
7. Tests run with stdin closed; some solver tests hang otherwise.

## Consequences

- The fast tier is the inner development loop; the full tier runs at
  integration points and in CI.
- A test that becomes slow moves tier on the next regeneration instead of
  being deleted.

## Evidence

`.config/nextest.toml`, `xtask/src/gate.rs`, `xtask/src/tiers.rs`,
`.github/workflows/gate.yml`, `docs/RUNNING.md` ("Fast iteration").
