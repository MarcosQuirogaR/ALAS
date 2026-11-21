// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Switches for the evaluation speed-ups.
//!
//! Each lever changes how much work one candidate evaluation repeats, not
//! the model: results move only within the stated closure tolerances (see
//! the closure-speed study for the equivalence
//! evidence). All levers are ON by default. The environment variable
//! `ALAS_SPEEDUPS_OFF` (comma-separated lever names, or `all`) turns levers
//! off for a whole process, which reproduces the numbers before the speed-ups for a
//! bit-diff; [`set`] overrides one lever at run time (examples and tests).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// One evaluation speed-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lever {
    /// A12: the process-wide nominal cache is keyed on, and resolves, the
    /// registered aircraft's CLmax rather than the candidate's scaled one.
    NominalKey = 0,
    /// A13: the main-gear second closure reuses the first closure's mass
    /// state as its start and its trim when the CG shift is small.
    GearReuse = 1,
    /// A2: the closure starts from the converged mass of the registered
    /// nominal and the trip tolerance adapts to the outer-loop residual.
    WarmStart = 2,
    /// E6: the cabin-derived fuselage length of an identical configuration and
    /// design vector is recalled, not re-solved (bit-identical: a pure function
    /// of an exact key, see `cabin_memo`)
    CabinMemo = 3,
}

impl Lever {
    /// Every lever, in the order they were introduced.
    pub const ALL: [Self; 4] = [
        Self::NominalKey,
        Self::GearReuse,
        Self::WarmStart,
        Self::CabinMemo,
    ];

    /// The name used by `ALAS_SPEEDUPS_OFF` and in reports.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NominalKey => "nominal_key",
            Self::GearReuse => "gear_reuse",
            Self::WarmStart => "warm_start",
            Self::CabinMemo => "cabin_memo",
        }
    }
}

/// 0 = follow the environment, 1 = forced off, 2 = forced on.
static OVERRIDES: [AtomicU8; 4] = [
    AtomicU8::new(0),
    AtomicU8::new(0),
    AtomicU8::new(0),
    AtomicU8::new(0),
];

fn environment_off() -> &'static [bool; 4] {
    static OFF: OnceLock<[bool; 4]> = OnceLock::new();
    OFF.get_or_init(|| {
        let text = std::env::var("ALAS_SPEEDUPS_OFF").unwrap_or_default();
        let mut off = [false; 4];
        for token in text.split(',').map(str::trim) {
            for lever in Lever::ALL {
                if token == "all" || token == lever.name() {
                    off[lever as usize] = true;
                }
            }
        }
        off
    })
}

/// Whether `lever` is active: the override when set, else not named in
/// `ALAS_SPEEDUPS_OFF`.
#[must_use]
pub fn enabled(lever: Lever) -> bool {
    match OVERRIDES[lever as usize].load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => !environment_off()[lever as usize],
    }
}

/// Force `lever` on or off for this process (`None` returns it to the
/// environment default). Not for use while a search is running.
pub fn set(lever: Lever, value: Option<bool>) {
    let code = match value {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    OVERRIDES[lever as usize].store(code, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levers_default_on_unless_the_environment_names_them() {
        // Overrides are process-global and other tests read the levers, so
        // this test only reads.
        for lever in Lever::ALL {
            assert_eq!(enabled(lever), !environment_off()[lever as usize]);
        }
    }
}
