// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// The in-process vortex-lattice implementation is retained as ALAS's primary
// aerodynamic model. Numerical provenance is recorded in docs/PORTING.md and
// the repository's third-party notice; this module exposes only ALAS types.

//! The in-process vortex-lattice model, scoped to what the aerodynamic,
//! stability, and dynamics stages
//! reach: constructing one with `airplane`, `op_point`, `spanwise_resolution`,
//! `chordwise_resolution` and `verbose=False`, then calling [`run`] or
//! [`run_with_stability_derivatives`]. Confirmed by grepping every
//! call in those stages.
//!
//! # Every constructor argument no call site ever overrides
//!
//! `xyz_ref` defaults to `airplane.xyz_ref`, which every call site leaves
//! unset, so [`run`] always reads it off the airplane rather than taking it
//! as a separate parameter. `run_symmetric_if_possible` defaults `false`, and
//! when a caller sets it upstream unconditionally raises
//! `NotImplementedError` before reaching the (also dead, commented-out)
//! symmetric-solve branch -- no call site in this program's inputs sets it,
//! so `run_symmetric` is always `false` and there is no symmetric-solve
//! branch to translate at all; this module has no parameter for it.
//! `vortex_core_radius` defaults `1e-8` and is never overridden, so
//! [`crate::singularities::calculate_induced_velocity_horseshoe`]'s smoothed
//! branch is the only one this module's callers ever reach.
//! `align_trailing_vortices_with_wind` defaults `false` and is never
//! overridden, so `trailing_vortex_direction` is always the constant
//! `[1, 0, 0]`, never the freestream direction.
//! `spanwise_spacing_function`/`chordwise_spacing_function` both default to
//! `np.cosspace`, matching this crate's [`SpacingFunction::Cosspace`] (for the
//! spanwise subdivision) and [`Wing::mesh_thin_surface`]'s own hardcoded
//! cosine spacing (for the chordwise stations) respectively; nothing here
//! takes either as a parameter for the same reason.
//!
//! # `run`'s panel mesh
//!
//! Every wing is optionally [`Wing::subdivide_sections`]'d (only when
//! `spanwise_resolution > 1`, upstream's own guard -- at the default
//! resolution of `1` this branch is skipped entirely, but
//! `AnalysisConfig.fine_spanwise_resolution` defaults to `2` and is used by
//! the full-analysis path through this same code, so the branch is real
//! production behavior and not hypothetical; see `docs/PORTING.md`), then
//! meshed with [`Wing::mesh_thin_surface`] at `chordwise_resolution`,
//! `add_camber=true`. `is_trailing_edge` and `areas`, upstream's other two
//! per-panel byproducts of this step, are not computed here: neither is read
//! by anything [`run`] itself does with the mesh -- `is_trailing_edge` only
//! feeds `calculate_streamlines`'s seed-point heuristic and `areas` is never
//! read at all in `run` -- and both are P11-only (`calculate_streamlines`) or
//! entirely unused, confirmed against the upstream source read in full for
//! this row.
//!
//! # `run_with_stability_derivatives`
//!
//! Reached only from `alas/physics/dynamics.py`'s `compute_dynamic_modes`
//! (P7), which needs the full derivative set (`alpha, beta, p, q, r` all
//! `true`, per that module's own doc comment). It is straightforward
//! finite-differencing on top of [`run`] -- central perturbations around each
//! state variable with an explicit step-refinement seam -- and lives in the
//! [`stability_derivatives`] submodule. See `docs/PORTING.md` for why its five
//! per-axis boolean flags are not translated as parameters.
//!
//! # Left untranslated
//!
//! `get_induced_velocity_at_points`/
//! `get_velocity_at_points` as *standalone* public entry points, `draw` and
//! `calculate_streamlines` are P11 (Figures) concerns; the induced-velocity
//! computation itself is translated as a private helper [`run`] calls
//! internally for the near-field force, the same way upstream's
//! `get_velocity_at_points` does.

include!("vlm_parts/part_01.rs");
include!("vlm_parts/part_02.rs");
