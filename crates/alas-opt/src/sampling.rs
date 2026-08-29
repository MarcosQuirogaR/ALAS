// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/sampling.py
// Reference: alas @ rust-port-baseline.

//! Random sampling in the aircraft design variable space.

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::validation::{validate, Severity, ValidationIssue};
use alas_config::AlasConfig;

/// A fast pseudo-random number generator for reproducible sampling and optimization.
#[derive(Debug, Clone)]
pub struct Rng {
    state: [u64; 2],
}

impl Rng {
    /// Initialize PRNG from a 64-bit integer seed.
    pub fn seed(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let s0 = (z ^ (z >> 31)).max(1);

        let mut z2 = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z2 = (z2 ^ (z2 >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z2 = (z2 ^ (z2 >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let s1 = (z2 ^ (z2 >> 31)).max(1);

        Self { state: [s0, s1] }
    }

    /// Next random 64-bit unsigned integer (xoroshiro128+).
    pub fn next_u64(&mut self) -> u64 {
        let s0 = self.state[0];
        let mut s1 = self.state[1];
        let result = s0.wrapping_add(s1);
        s1 ^= s0;
        self.state[0] = s0.rotate_left(24) ^ s1 ^ (s1 << 16);
        self.state[1] = s1.rotate_left(37);
        result
    }

    /// Uniform float in `[lo, hi)`.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let frac = (self.next_u64() >> 11) as f64 * (1.0 / ((1_u64 << 53) as f64));
        lo + frac * (hi - lo)
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::seed(42)
    }
}

/// Compute widened bounds expanded by `slack` fraction of margin, capped at spec limits.
pub fn widened_bounds(bounds: &[(f64, f64)], slack: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(bounds.len());
    for (spec, &(lo, hi)) in SPECS.iter().zip(bounds) {
        let margin = (hi - lo) * slack;
        let spec_range = spec.upper - spec.lower;
        let spec_floor = spec.lower - spec_range * slack * 0.5;
        let spec_ceil = spec.upper + spec_range * slack * 0.5;
        let lo_val = (lo - margin).max(spec_floor);
        let hi_val = (hi + margin).min(spec_ceil);
        out.push((lo_val, hi_val));
    }
    out
}

/// Draw one uniform random design vector from `bounds`.
pub fn draw_one(bounds: &[(f64, f64)], rng: &mut Rng) -> DesignVector {
    let mut vals = Vec::with_capacity(bounds.len());
    for &(lo, hi) in bounds {
        vals.push(rng.uniform(lo, hi));
    }
    DesignVector::from_array(&vals).unwrap_or_default()
}

/// Return error-severity validation issues for `candidate` on `config`.
pub fn error_issues(config: &AlasConfig) -> Vec<ValidationIssue> {
    let issues = validate(config);
    issues
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect()
}

/// Sample a random design candidate that satisfies configuration validation rules.
pub fn sample_design(
    bounds: Option<&[(f64, f64)]>,
    config: Option<&AlasConfig>,
    rng: &mut Rng,
    max_attempts: usize,
) -> DesignVector {
    let default_bounds = DesignVector::bounds();
    let raw_bounds = bounds.unwrap_or(&default_bounds);

    for _ in 0..max_attempts {
        let candidate = draw_one(raw_bounds, rng);
        if let Some(cfg) = config {
            let issues = error_issues(cfg);
            if !issues.is_empty() {
                continue;
            }
        }
        return candidate;
    }

    let widened = widened_bounds(raw_bounds, 0.2);
    draw_one(&widened, rng)
}
