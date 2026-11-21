# 0010. No known gaps

Status: Accepted (2026-10-22)

## Context

Registries of "known gaps", "accepted exceptions" and "limitations" tend to
grow until they are the real state of the program. A defect, an unread
configuration field, an unsourced value or a failing check labelled as known
stops being work and starts being furniture.

## Decision

1. A defect, an unread field, an unsourced value or a failing check is never
   closed by labelling it a known gap, limitation or accepted exception.
   It is fixed, or investigated to its root cause and reported with the
   evidence still missing and how to obtain it, which becomes a tracked defect.
   "No fix found" is a status of the work, not a result.
2. Registries that exist (`plausibility_known.tsv`, ADR 0006) are temporary
   ratchets removed as the entries are fixed. Entries are added only on
   a new rule's first run, each with a named tracked defect; otherwise the list only
   shrinks. It must be empty for release validation.
3. The capability map has no "known gap" exemption kind (ADR 0007); a field is
   wired, retired or an audited label.
4. A bug found while working on something else is reported, not silently
   absorbed into the current change.

## Consequences

- Closing a task can take longer; the alternative is carrying invisible debt.

## Evidence

`crates/alas-acceptance/data/plausibility_known.tsv`, `docs/architecture/README.md`,
`xtask/src/capmap/`.
