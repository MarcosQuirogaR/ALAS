// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The throughput profile an external-tool stage ran under, and how a reduced
//! run announces itself.
//!
//! `alas_exec::tool_runtime` holds the opt-in controls (reduced "matrix"
//! sweeps, the result cache, solver thread count). This module turns them into
//! the facts a run records: a status block on the pipeline result and the run
//! manifest, a marker file beside the outputs, and a diagnostic event. A
//! reduced-fidelity run therefore cannot be mistaken for a full-fidelity one
//! by reading its outputs.

use std::path::Path;

use alas_config::mses::MsesConfig;
use alas_exec::tool_cache;
use alas_exec::tool_runtime::{FidelityProfile, RuntimeSettings};
use serde::{Deserialize, Serialize};

/// Name of the marker file written into a reduced run's output directory.
pub const REDUCED_FIDELITY_MARKER: &str = "REDUCED_FIDELITY.txt";

/// MSES polar points in the reduced matrix profile (full default is 7).
pub const MATRIX_MSES_ALPHA_POINTS: i64 = 3;

/// Half-width of the reduced matrix profile's alpha bracket around the
/// design (cruise) angle, in degrees. The matrix needs the cruise point and
/// the local lift/drag slope there, so the bracket is the immediate
/// neighbourhood of the design angle (the full +/-3 deg default reaches stall
/// from a proxy angle that is often already near CL 1).
pub const MATRIX_MSES_HALFWIDTH_DEG: f64 = 1.0;

/// Throughput options a run executed under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProfileStatus {
    /// `full` or `matrix_reduced`; anything but `full` is not a
    /// full-fidelity external-tool result.
    pub fidelity: String,
    /// True when the external-tool sweeps were reduced.
    pub reduced: bool,
    /// OpenMP threads handed to VSPAERO.
    pub vspaero_threads: usize,
    /// Whether the external-tool result cache was enabled.
    pub cache_enabled: bool,
    /// Cache replays so far in this process (not per run).
    pub cache_hits_process_total: u64,
    /// Cache lookups that missed so far in this process.
    pub cache_misses_process_total: u64,
    /// Cache entries written so far in this process.
    pub cache_stores_process_total: u64,
}

impl ToolProfileStatus {
    /// The profile in force now, with the cache counters read at this moment.
    pub fn current() -> Self {
        let settings = RuntimeSettings::current();
        let counters = tool_cache::stats();
        Self {
            fidelity: settings.fidelity.as_str().to_owned(),
            reduced: settings.fidelity.is_reduced(),
            vspaero_threads: settings.vspaero_thread_count(),
            cache_enabled: settings.cache_dir.is_some(),
            cache_hits_process_total: counters.hits,
            cache_misses_process_total: counters.misses,
            cache_stores_process_total: counters.stores,
        }
    }

    /// One line for the run log.
    pub fn summary(&self) -> String {
        format!(
            "External-tool profile: {} (VSPAERO threads {}, result cache {}){}",
            self.fidelity,
            self.vspaero_threads,
            if self.cache_enabled { "on" } else { "off" },
            if self.reduced {
                "; REDUCED FIDELITY - not a full-fidelity external-tool result"
            } else {
                ""
            },
        )
    }

    /// Write [`REDUCED_FIDELITY_MARKER`] into `output_dir` when reduced.
    pub fn write_marker(&self, output_dir: &Path) {
        if !self.reduced {
            return;
        }
        let text = format!(
            "This run used the reduced-fidelity '{}' external-tool profile \
             (ALAS_FIDELITY_PROFILE).\n\
             VSPAERO: {} alpha points instead of the full sweep; MSES: {} polar points \
             within {} deg of the design angle instead of the configured sweep; AVL: 8 \
             chordwise vortices instead of the configured fine mesh. Wake iterations and \
             convergence gates are unchanged.\n\
             Native ALAS results are not affected. Do not report these external-tool \
             numbers as full-fidelity results.\n",
            self.fidelity,
            crate::vspaero::MATRIX_VSPAERO_ALPHA_POINTS,
            MATRIX_MSES_ALPHA_POINTS,
            MATRIX_MSES_HALFWIDTH_DEG,
        );
        let _ = std::fs::create_dir_all(output_dir);
        let _ = std::fs::write(output_dir.join(REDUCED_FIDELITY_MARKER), text);
    }
}

/// MSES settings for `fidelity`: the matrix profile caps the polar at
/// [`MATRIX_MSES_ALPHA_POINTS`] points within [`MATRIX_MSES_HALFWIDTH_DEG`] of
/// the design angle.
pub fn mses_config_for_profile(config: &MsesConfig, fidelity: FidelityProfile) -> MsesConfig {
    let mut adjusted = config.clone();
    if fidelity.is_reduced() {
        adjusted.alpha_sweep_n_points = adjusted.alpha_sweep_n_points.min(MATRIX_MSES_ALPHA_POINTS);
        adjusted.alpha_sweep_halfwidth_deg = adjusted
            .alpha_sweep_halfwidth_deg
            .min(MATRIX_MSES_HALFWIDTH_DEG);
    }
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix_profile_caps_the_mses_sweep_and_full_keeps_it() {
        let config = MsesConfig::default();
        assert_eq!(config.alpha_sweep_n_points, 7);
        let reduced = mses_config_for_profile(&config, FidelityProfile::Matrix);
        assert_eq!(reduced.alpha_sweep_n_points, 3);
        assert_eq!(reduced.alpha_sweep_halfwidth_deg, MATRIX_MSES_HALFWIDTH_DEG);
        assert_eq!(
            mses_config_for_profile(&config, FidelityProfile::Full),
            config
        );
        let mut short = config.clone();
        short.alpha_sweep_n_points = 2;
        assert_eq!(
            mses_config_for_profile(&short, FidelityProfile::Matrix).alpha_sweep_n_points,
            2
        );
    }

    #[test]
    fn a_reduced_summary_says_so_and_a_full_one_does_not() {
        let mut status = ToolProfileStatus {
            fidelity: "full".to_owned(),
            reduced: false,
            vspaero_threads: 4,
            cache_enabled: false,
            cache_hits_process_total: 0,
            cache_misses_process_total: 0,
            cache_stores_process_total: 0,
        };
        assert!(!status.summary().contains("REDUCED"));
        status.fidelity = "matrix_reduced".to_owned();
        status.reduced = true;
        assert!(status.summary().contains("REDUCED FIDELITY"));
    }
}
