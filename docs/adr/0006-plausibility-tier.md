# 0006. Plausibility tier

Status: Accepted (2026-10-22)

## Context

Agreement with a reference does not prove a result is physical. A mission with
no cruise, a descent that glides at one degree, a static margin of 50 % or a
speed that jumps 100 knots between segments can all coexist with a mass within
a few percent of the published value.

## Decision

1. An automated tier checks generic engineering sanity limits on the results
   of every registered aircraft: mission shape and continuity, climb and
   descent rates, speed schedule, drag polar, lift coefficients, design
   speeds and load factors, payload-range ordering, balance and stability.
   It lives in `alas-acceptance` (`src/plausibility/`, test `plausibility_tier`).
2. Each rule is generic: a band stated as usual / MEDIUM outside / HIGH
   outside, with its source (textbook table, regulation or standard; a few bands record an engineering-judgement band, stated as such in the rule). No
   rule is keyed to an aircraft or fitted to a preset (ADR 0003). The rule
   table is generated from the code (`cargo run -p alas-acceptance --example
   plausibility_rules_md`), so documentation cannot drift.
3. A HIGH finding fails the gate. Known HIGH pairs (aircraft, rule) are kept
   in `crates/alas-acceptance/data/plausibility_known.tsv` as a temporary
   ratchet. Entries are added only on a new rule's first run, each with a named
   tracked defect; otherwise the list only shrinks. The gate fails on a HIGH not
   listed and on a listed pair that no longer reproduces, so a fix must delete
   its line.
4. `ALAS_PLAUS_STRICT=1` requires an empty list. A release is validated with
   it set.

## Consequences

- A new defect class is added as a rule with its source; the pairs it flags
  are listed with a tracked defect and removed as they are fixed.
- Thresholds are engineering bands, not certification limits; MEDIUM findings
  need a physical explanation but do not fail.

## Evidence

`crates/alas-acceptance/src/plausibility/`, `crates/alas-acceptance/tests/plausibility_tier.rs`,
`crates/alas-acceptance/data/plausibility_known.tsv`.
