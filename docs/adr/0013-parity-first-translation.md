# 0013. Parity-first translation and deliberate deviations

Status: Accepted (2026-10-22)

## Context

The physics kernels began as a translation of a working Python program. A
translation is only worth having if it agrees with the original, and once two
unchecked modules exist a disagreement in the second may come from the first.

## Decision

1. A translated module lands together with the test that compares it with the
   reference, in the same commit, with its row in `docs/PORTING.md` flipped to
   `parity: green` naming the fixture and tolerance tier. Nothing depends on a
   module that has not passed parity.
2. Tolerances come from the fixed tiers in `alas-testkit` (`Tier`): closed-form
   arithmetic is held tighter than anything through a linear solve, and `f32`
   kernels tighter than iterative solutions. A module that cannot pass at its
   tier is a finding, not a reason to raise a number.
3. Faithful translation is the default, including of upstream behaviour that is
   wrong, so that the port can be validated against the reference; the quirk is
   left unchanged in the translation commit and opened as a tracked defect (ADR
   0010).
4. Improvements are made afterwards, one at a time, each config-gated with the
   previous behaviour selectable, cited to a source, and quantified against the
   parity baseline (case, quantity, before, after).
5. Fixtures record the commit of the reference they came from and the
   generators refuse to run on a dirty reference tree.
6. Orchestration layers written natively (`alas-pipeline`, `alas-gui`) are not
   covered by parity; they are covered by ADRs 0003, 0006 and 0008.

## Consequences

- Parity says the port matches the reference, not that the reference is
  physically right; ADR 0003 and the plausibility tier address that.

## Evidence

`CONTRIBUTING.md` (parity rule, deliberate deviations), `docs/PORTING.md`,
`crates/alas-testkit`, `golden/`.
