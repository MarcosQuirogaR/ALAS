# WP-13 evidence and campaign contract

## Purpose

This document defines the evidence package required to substantiate an ALAS
release without inventing solver or experimental outputs. The machine-readable
campaign authority is
[`unified-conventional-campaign.json`](../../golden/wp13/unified-conventional-campaign.json).
Its current status is `NotEvaluated`.

## Unified conventional-aircraft campaign

The campaign uses one frozen conventional twin-engine transport identity across
requirements, geometry/payload, mass/fuel, propulsion/systems, aerodynamics,
performance/stability/structures, mission, operations/environment, and MDO/UQ.
The purpose is to expose compensating errors that isolated aggregate matches can
hide. Every stage must retain inputs, outputs, status, residuals, fidelity,
tool/environment identity, and content hashes.

A replay is complete only when it includes fresh execution, deterministic seed
and budget identity, failure/cache/retry history, all hard-requirement
assessments, artifact comparison contracts, sensitivity evidence where
required, and a report-to-artifact trace audit. Independent reproduction is
required when a second supported environment is available.

## Separate benchmark families

The unified aircraft does not replace discipline verification. The retained
[`benchmark-families.json`](../../golden/wp13/benchmark-families.json) keeps ten
separate families:

1. aerodynamics;
2. high lift;
3. propulsion;
4. field performance;
5. mass properties;
6. systems;
7. stability and control;
8. structures and aeroelasticity;
9. mission;
10. operations and environment.

Each family requires case/geometry identity, units and frames, source rights,
comparison tolerance or uncertainty, signed discrepancy, and a validity
disposition. Their current `NotEvaluated` states are intentional.

## Retained replay state

The replay manifest records no executed cases or solver artifacts. That absence
is the honest result: WP-12 is not yet checkpointed into this campaign, and
external tools/data are incomplete. The manifest must fail closed if its
contract hash, hard-requirement review, evidence index, or model-card/release
coverage is missing or inconsistent.

## Promotion rule

Only a finding with ledger status `Verified`, an immutable implementation
revision, and resolved retained evidence may support a release claim. A passing
unit test can verify software behavior but cannot by itself validate aircraft
physics or satisfy an authority requirement.
