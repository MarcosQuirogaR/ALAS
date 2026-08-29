# WP-13 release coverage

## Release decision

This development snapshot is **evidence incomplete**. It has no release-verified
physical capability claim. The matrix deliberately distinguishes the presence
of typed implementations from a verified, replayed, independently substantiated
claim. Only ledger status `Verified` can support `ReleaseVerified`.

The machine-readable authority is
[`wp13-release-coverage.json`](wp13-release-coverage.json). The current unified
campaign and all hard-requirement review artifacts are retained under
[`golden/wp13`](../../golden/wp13/).

## Claim matrix

| Claim ID | Safe development statement | Status | Model card | Release rationale |
|---|---|---|---|---|
| `claim.readme` | The README.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.aircraft-design-doctrine` | The AIRCRAFT_DESIGN_DOCTRINE.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.requirements-systems` | The requirements-systems.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.mission-sizing` | The mission-sizing.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.cabin-interiors-cargo` | The cabin-interiors-cargo.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.geometry-configuration` | The geometry-configuration.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.aerodynamics` | The aerodynamics.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.propulsion-energy` | The propulsion-energy.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.aircraft-systems` | The aircraft-systems.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.mass-balance` | The mass-balance.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.stability-control` | The stability-control.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.performance-airport` | The performance-airport.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.structures-aeroelasticity` | The structures-aeroelasticity.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.optimization-mdo` | The optimization-mdo.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.digital-thread` | The digital-thread.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.certification-safety` | The certification-safety.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.environment-lifecycle` | The environment-lifecycle.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.operations-economics` | The operations-economics.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |
| `claim.integration-plan` | The INTEGRATION_PLAN.md implementation slice has typed conceptual contracts or bookkeeping in the current worktree. | NotReleaseVerified | `model-card.alas-conceptual-aircraft` | At least one implementation is unverified and the fresh WP-13 campaign is NotEvaluated. |

## Hard-requirement review

The frozen default programme inspected on 2026-08-27 contains 18 hard
requirements. Three have registered evaluator plans; the other hard case rows
are blocked by missing evaluator coverage. A registration is not a result, so
all 18 remain release-blocking until a fresh requirements-first campaign
produces comparable retained evidence. The complete pre-execution disposition
is in
[`not-evaluated-hard-requirements-review.json`](../../golden/wp13/not-evaluated-hard-requirements-review.json).

## Evidence boundary

Repository validation proves schema, linkage, identifier uniqueness, declared
coverage, local path containment, and SHA-256 agreement. It does not establish
physical correctness, legal authority, certification compliance, or validation
accuracy. Those require discipline review and, where applicable, independent
solver, experimental, supplier, operator, airport, lifecycle, climate, noise,
or authority evidence.

## Release blockers

- The unified conventional-aircraft campaign has not been executed.
- No independent replay on a second supported environment is retained.
- External-tool decks, raw results, sensitivity studies, and comparison
  contracts are incomplete.
- The default hard-requirement review contains no release-verified assessment.
- Supplier, operator, legal, controlled-test, and authority data remain outside
  the repository evidence boundary.
