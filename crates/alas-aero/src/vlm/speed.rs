// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Switches for the lattice-update speed-ups of a rotating stabilizer.
//!
//! Each lever defaults ON. A lever can be turned off for a bit-diff or
//! timing comparison, either by the environment variable named below (read
//! once, `0`/`off`/`false` disables). A cache can also pin the Schur lever
//! with `VlmGeometryCache::with_schur_update`, and tests and harnesses pin
//! the kernel lever with the setter. Switching a lever never changes a model: the Schur
//! factorization differs from the ordinary LU only by rounding, the fast
//! kernel path is bit-identical.
//!
//! * `ALAS_VLM_SCHUR_LU`: Schur-complement update of the influence matrix
//!   factorization over the unchanged-panel block (see `system::schur`).
//! * `ALAS_VLM_FAST_KERNEL`: bit-identical fast path of the core-regularized
//!   horseshoe kernel for the matrix fill.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// 0 = follow the environment, 1 = forced off, 2 = forced on.
static FAST_KERNEL_OVERRIDE: AtomicU8 = AtomicU8::new(0);

fn environment_enabled(name: &str) -> bool {
    !matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("0" | "off" | "false" | "OFF" | "FALSE")
    )
}

fn resolve(override_state: &AtomicU8, from_environment: &OnceLock<bool>, name: &str) -> bool {
    match override_state.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => *from_environment.get_or_init(|| environment_enabled(name)),
    }
}

/// Whether the Schur-complement factorization update is enabled.
pub fn schur_lu() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| environment_enabled("ALAS_VLM_SCHUR_LU"))
}

/// Whether the bit-identical fast kernel path is enabled.
pub fn fast_kernel() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    resolve(&FAST_KERNEL_OVERRIDE, &ENV, "ALAS_VLM_FAST_KERNEL")
}

/// Force the fast kernel path on or off (`None` returns to the environment).
pub fn set_fast_kernel(state: Option<bool>) {
    FAST_KERNEL_OVERRIDE.store(state.map_or(0, |on| 1 + u8::from(on)), Ordering::Relaxed);
}
