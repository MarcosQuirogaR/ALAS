# 0004. Configuration schema, metadata and retired keys

Status: Accepted (2026-10-22)

## Context

Saved configurations outlive the code that wrote them. A removed setting must
not make older files unloadable, and a setting nobody reads must not stay in
the interface as if it did something.

## Decision

1. **Metadata drives the interface.** Every public configuration field is
   declared once with `#[derive(ConfigNode)]` and `#[config(...)]` (label, unit,
   help, bounds, level). The settings forms, the YAML round trip and the
   translation keys derive from that declaration; the macro refuses to
   compile a public field with neither metadata nor an explicit `skip`.
2. **Levels.** A field is either routine or advanced; the interface has one
   global Basic/Advanced switch driven by that level.
3. **SI inside.** Stored values are SI unless the field's unit says
   otherwise; unit labels in metadata are the contract.
4. **A field either does something or is not there.** A field is used when
   a model or pipeline crate reads it (ADR 0007 checks this). A field nothing
   reads is wired into a calculation or retired.
5. **Retired keys are dropped at the loading boundary.** Keys an earlier
   build wrote and no setting reads any more are listed per group in
   `retired_keys.rs` (`RETIRED_SOLVER_KEYS`, `RETIRED_OBJECTIVE_KEYS`,
   `RETIRED_WEIGHT_KEYS`, `RETIRED_DRAG_MODEL_KEYS`,
   `RETIRED_PROPULSION_CYCLE_KEYS`, `RETIRED_DESIGN_SPACE_KEYS`) and removed
   when a document is overlaid, because an unrecognised key is otherwise an
   error. A retired key whose value still has meaning is converted, not
   dropped (the generation-based search budget becomes an evaluation budget).
   Each retirement states the reason in the constant's doc comment and is
   append-only: the list is never shortened while old files can exist.
6. **Exemptions are exceptional and audited.** A field that is only a label or
   a deprecated compatibility field is listed in
   `docs/architecture/field-exemptions.txt` with its kind and reason; the
   check fails when an entry is stale.

## Consequences

- Removing a setting is two steps in one change: delete the field and add its
  key to the retired list with the reason.
- Old files keep loading; their retired keys have no effect.
- A retired default that old builds always wrote (the viscous margin 1.00 of
  v1.4.x and 1.10 of v1.3.x) is told from a stated value by a hidden
  `drag_model.schema_version`: an unmarked file migrates those defaults to
  unset with a load note, a file marked `2` keeps any stated value. A
  hand-written file keeps its margin by adding `"schema_version": 2` to its
  `drag_model` block.

## Evidence

`crates/alas-config/src/retired_keys.rs`, `crates/alas-config-derive`,
`docs/architecture/field-exemptions.txt`, `docs/design-constraints.md`.
