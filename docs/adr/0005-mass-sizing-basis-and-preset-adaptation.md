# 0005. Mass sizing basis and preset reference adaptation

Status: Accepted (2026-10-22)

## Context

The FLOPS-based mass model evaluates wing, tail, fuselage, control and gear
terms at a design gross mass `DG` and a design landing mass `WLDG`. Three
different questions use it: what does a registered aircraft weigh empty, how
does it behave on a given mission, and what does a new aircraft close to.
Before this decision the mission closure re-evaluated every component at the
current takeoff-mass iterate under all three, so a registered aircraft flown
on a short route quietly became a lighter aircraft with the same name.

## Decision

1. The basis is an explicit state: `MassSizingBasis` is `FixedAircraft`
   (declared `DG` and `WLDG`) or `Coupled` (the design mass follows the closed
   takeoff mass, landing mass through a configured fraction).
2. It follows from `DesignMode`: `BaselineSandbox` and `ReferenceAdaptation`
   replay or adapt a registered aircraft whose certified weights are inputs
   and use `FixedAircraft`; `CleanSheet` sizes a new aircraft and uses
   `Coupled`. `AlasConfig::at_closure_mass()` applies it.
3. Registered presets load in `ReferenceAdaptation`. A file overlay may select
   another mode; `BaselineSandbox` replays a preset with no resizing.
4. In preset mode the field classes are fixed by `preset_policy.rs`:
   locked geometry, the optimizer envelope (design variables within
   `x_ref +- 10 %` of the reference for lengths and scales, configured
   absolute windows for angles and shifts), operational inputs (editable),
   derived quantities (never stored), and advanced model fields. The envelope
   is anchored to the registry's design vector, never to the latest candidate,
   so repeated runs cannot compound the allowance.
5. Preset operational defaults carry a generic climb schedule; none is tuned
   to a preset (ADR 0003).

## Consequences

- A short-route mission on a registered aircraft changes fuel and dispatch
  mass only; the component ledger is the one from the declared weights.
- Clean-sheet results are not comparable with preset results without saying
  which basis each used; reports state it.

## Evidence

`crates/alas-config/src/sizing_basis.rs`, `crates/alas-config/src/preset_policy.rs`,
`docs/mass-model-architecture.md`, `docs/flops-mass-model.md`.
