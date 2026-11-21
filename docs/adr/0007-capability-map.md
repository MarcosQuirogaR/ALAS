# 0007. Capability map

Status: Accepted (2026-10-22)

## Context

ALAS has several hundred configuration fields, pipeline stages, figures,
tools, GUI pages and command-line flags. Fields were added whose value no
model read; features were added with no end-to-end test; and nobody could
answer "where does this live?" without searching.

## Decision

1. A generated map lists configuration fields (unit, level, reading crates,
   GUI page), pipeline stages, report figures, external tools, GUI pages, CLI
   flags, add-on keys, file types and smoke tests. It is produced by
   `cargo xtask capability-map` from the source tree into
   `docs/architecture/capability-map.json` and `.md`; it is never edited by
   hand.
2. `cargo xtask checks` regenerates the map in memory and fails if the
   committed copy differs, so a stale map cannot be committed.
3. Field reachability: a field must be read by a model or pipeline crate
   (directly, or through a config method that model code calls). Reads in
   tests and in the acceptance and test-support crates do not count.
   Otherwise the field is wired, retired (ADR 0004) or listed in
   `field-exemptions.txt` as a display-only label or a deprecated
   compatibility field. Exemptions are checked for staleness.
4. Every capability family has a smoke test named
   `capability_smoke_<family>_...`; the families are listed in
   `xtask/src/capmap/mod.rs`.
5. The scan establishes wiring, not physical correctness; it does not replace
   the plausibility tier (ADR 0006).

## Consequences

- A change that adds or renames a mapped item regenerates the map in the same
  commit.
- Name-based matching can credit a read to a same-named function elsewhere;
  such cases are noted in `docs/architecture/README.md`.

## Evidence

`xtask/src/capmap/`, `docs/architecture/README.md`, `docs/architecture/field-exemptions.txt`.
