// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Switches for the lattice-update speed-ups of a rotating stabilizer.
//!
//! Each lever defaults ON. A lever can be turned off for a bit-diff or
//! timing comparison, either by the environment variable named below (read
//! once, `0`/`off`/`false` disables). A cache can also pin the Schur lever
//! with `VlmGeometryCache::with_schur_update`. Switching a lever never
//! changes a model: the Schur
//! factorization and the superposed solves differ from the ordinary LU and
//! the direct solve only by rounding, the fast kernel path is bit-identical.
//!
//! * `ALAS_VLM_SCHUR_LU`: Schur-complement update of the influence matrix
//!   factorization over the unchanged-panel block (see `system::schur`).
//! * `ALAS_VLM_FAST_KERNEL`: bit-identical fast path of the core-regularized
//!   horseshoe kernel for the matrix fill.
//! * `ALAS_VLM_SUPERPOSITION`: repeated solves of one lattice as combinations
//!   of its basis solves (see `system::superposition`).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

fn environment_enabled(name: &str) -> bool {
    !matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("0" | "off" | "false" | "OFF" | "FALSE")
    )
}

/// Whether the Schur-complement factorization update is enabled.
pub fn schur_lu() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| environment_enabled("ALAS_VLM_SCHUR_LU"))
}

/// Whether the bit-identical fast kernel path is enabled.
pub fn fast_kernel() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| environment_enabled("ALAS_VLM_FAST_KERNEL"))
}

/// Whether repeated solves combine the lattice's basis solves.
pub fn superposition() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| environment_enabled("ALAS_VLM_SUPERPOSITION"))
}

/// 0 = follow the environment, 1 = forced off, 2 = forced on.
static NORMAL_ROTATION_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// Whether the trim iteration first moves the stabilizer incidence on the
/// boundary condition of one reference lattice
/// ([`crate::vlm::VlmSystem::solve_with_surface_pitch`]) before settling on
/// the exactly rebuilt lattice (`ALAS_VLM_NORMAL_ROTATION=1`; default OFF).
///
/// Off by default because it does not pay: the settled trim must meet the
/// same residual tolerance on the exact lattice, which takes the same number
/// of lattice assemblies as the exact Newton iteration (rotating the normals
/// leaves the tail's vortex positions out, a 0.5 to 1.5 % error in the
/// incidence effect), and each rotated-normal solve costs a quarter of an
/// incremental assembly.
pub fn normal_rotation_trim() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    match NORMAL_ROTATION_OVERRIDE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => *ENV.get_or_init(|| {
            matches!(
                std::env::var("ALAS_VLM_NORMAL_ROTATION")
                    .ok()
                    .as_deref()
                    .map(str::trim),
                Some("1" | "on" | "true" | "ON" | "TRUE")
            )
        }),
    }
}

/// Force the normal-rotation trim iteration on or off (`None` returns to the
/// environment).
pub fn set_normal_rotation_trim(state: Option<bool>) {
    NORMAL_ROTATION_OVERRIDE.store(state.map_or(0, |on| 1 + u8::from(on)), Ordering::Relaxed);
}
