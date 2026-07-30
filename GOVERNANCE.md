# Governance

ALAS is maintained by its author, Marcos Quiroga Rodríguez, who reviews and
approves every change. This document states that plainly so contributors know
what to expect rather than having to infer it.

## Decisions

The maintainer decides what is in scope, what gets merged, and what is
released. Technical disagreement is settled by discussion in the issue or pull
request; where it cannot be, the maintainer decides.

This is a single-maintainer project, so response times vary. An unanswered pull
request means it has not been read yet, not that it has been rejected.

## Merging

- Every change reaches `main` through a pull request. No direct pushes.
- Every pull request needs maintainer approval and green CI.
- The maintainer's own changes follow the same route, so `main` always
  reflects something that was reviewed and passed CI.

## Scope

ALAS is a *conceptual and preliminary* design tool. Contributions that raise
fidelity within that scope, improve usability, or strengthen testing are
welcome. Proposals that turn it into a detailed-design or certification tool
are out of scope — not because they are bad, but because the models here are
not built to carry that weight, and pretending otherwise would mislead users.

## Becoming a maintainer

There is no formal process yet. If someone contributes substantially and
consistently, commit rights will be offered. Adding maintainers means adopting
a contributor licence agreement first, so that the licensing position in
`NOTICE` stays coherent.

## Releases

The maintainer cuts releases. Versions follow semantic versioning, and
`CHANGELOG.md` records what changed in each. Released binaries are built by CI
from a tagged commit, so what ships is always reproducible from public source.

## Code of conduct

Participation is governed by `CODE_OF_CONDUCT.md`. The maintainer enforces it.
