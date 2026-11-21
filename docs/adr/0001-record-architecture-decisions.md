# 0001. Record architecture decisions

Status: Accepted (2026-10-22)

## Context

ALAS carries decisions that are not visible in the code: why a field was
retired, why the mass model evaluates at fixed design weights for a registered
aircraft, why a package is built the way it is. They lived in commit messages,
module comments and long documents, and each was found by searching.

## Decision

Keep short, numbered decision records in `docs/adr/`. A record states the
context, the decision, the consequences, and the evidence (file or test) that
implements it. Records are immutable once accepted; a later decision
supersedes an earlier one explicitly. Module comments keep explaining *why*
locally; an ADR is for decisions that span modules, tools or process.

## Consequences

- A change that reverses a recorded decision must add a superseding ADR in the
  same pull request.
- Reviewers can ask "which ADR does this touch?" instead of reconstructing
  history.
- The cost is a page of prose per cross-cutting decision.

## Evidence

`docs/adr/README.md` (index and procedure).
