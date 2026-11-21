# 0003. Honest physics rule

Status: Accepted (2026-10-22)

## Context

A conceptual design tool can be made to agree with reference aircraft by
tuning. The result then agrees and means nothing. ALAS is used for teaching and
research, where a plausible but wrong number is worse than a crash.

## Decision

1. Every coefficient, correlation and limit is sourced to a document that was
   read at the page, and the source sits next to the value. Statistical
   correlations carry their validity range.
2. No per-aircraft factor and no logic keyed to a preset name. A preset is
   data, not a code path.
3. No least-squares fit of model parameters to the registered presets, and
   no moving a reference value to make the model pass.
4. Plausibility comes before agreement: a result "within 10 %" of a reference
   that is physically implausible (see ADR 0006) is a failure.
5. Registered presets stay bit-identical across changes unless the change is
   an intended, validated fix; a pinned value is re-pinned only from a probe
   run, and the commit states the cause.
6. A model that cannot run reports a typed status (`NotRun`, `Error`) and
   never substitutes a plausible number silently.
7. Implementation, numerical verification, calibration and physical
   validation are different claims and are reported separately.

## Consequences

- Some models are conservative or coarse because no honest source exists for
  a finer one; the documentation states that instead of hiding it.
- Reviewers can reject a change on provenance alone.

## Evidence

`CONTRIBUTING.md` (parity and deviation rules), `docs/methods.md` (sources per
model), `docs/ARCHITECTURE.md` (typed analysis status), the plausibility tier
(ADR 0006).
