# Architecture decision records

An ADR records one decision that shapes the code or the process, the reason it
was taken, and what it costs. Most records below describe decisions that were
already in force; they were written down on 2026-10-22 so that a new
contributor can find the reason without reading the history. The "Evidence"
line of each record names the file or test that implements it.

## Index

| No. | Title | Status |
|---|---|---|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-workspace-and-crate-structure.md) | Workspace and crate structure | Accepted |
| [0003](0003-honest-physics-rule.md) | Honest physics rule | Accepted |
| [0004](0004-configuration-schema-and-retired-keys.md) | Configuration schema, metadata and retired keys | Accepted |
| [0005](0005-mass-sizing-basis-and-preset-adaptation.md) | Mass sizing basis and preset reference adaptation | Accepted |
| [0006](0006-plausibility-tier.md) | Plausibility tier | Accepted |
| [0007](0007-capability-map.md) | Capability map | Accepted |
| [0008](0008-test-tiers.md) | Test tiers and the gate | Accepted |
| [0009](0009-release-process.md) | Release process and package kinds | Accepted |
| [0010](0010-no-known-gaps.md) | No known gaps | Accepted |
| [0011](0011-add-on-sandbox.md) | Add-on sandbox | Accepted |
| [0012](0012-external-solvers-process-boundary.md) | External solvers as an optional process boundary | Accepted |
| [0013](0013-parity-first-translation.md) | Parity-first translation and deliberate deviations | Accepted |
| [0014](0014-repository-layout.md) | Repository layout | Proposed |

## Writing one

Copy the shape of an existing record: Context, Decision, Consequences,
Evidence. Number it next in sequence, never renumber, and never rewrite an
accepted record: supersede it with a new one and change the old record's
status line to "Superseded by NNNN". Keep the text ASCII and short; link to
the code that enforces the decision rather than describing it twice.
