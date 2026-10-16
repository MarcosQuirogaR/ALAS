// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Process-level run-time options for external tools: solver thread count,
//! the opt-in result cache, and the opt-in reduced-fidelity "matrix" profile.
//!
//! These are throughput controls for development and validation studies, not
//! analysis inputs, so they are read from the environment rather than stored
//! in the physics configuration. Every one defaults to the full-fidelity,
//! uncached behaviour the desktop application has always had; a front end
//! (the GUI, the harness) opts in explicitly.
//!
//! | Variable | Meaning | Default |
//! |----------|---------|---------|
//! | `ALAS_FIDELITY_PROFILE` | `full` or `matrix` (reduced external-tool sweeps) | `full` |
//! | `ALAS_TOOL_CACHE_DIR` | directory that enables the input-hash result cache | unset (cache off) |
//! | `ALAS_VSPAERO_THREADS` | OpenMP threads handed to VSPAERO (`-omp N`) | cores / jobs |
//! | `ALAS_VSPAERO_WAKE_ITERATIONS` | free-wake iterations per VSPAERO sweep | `8` (pipeline) |
//! | `ALAS_CONCURRENT_JOBS` | analyses running side by side on this machine | `1` |
//! | `ALAS_TOOL_TIMEOUT_SCALE` | multiplier on every external-tool wall-clock deadline | `1` |

use std::path::PathBuf;
use std::sync::Mutex;

/// Which external-tool sweep density a run uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FidelityProfile {
    /// The shipped sweeps: every alpha point and mesh the analysis requests.
    #[default]
    Full,
    /// Reduced external-tool sweeps for validation matrices: about five
    /// VSPAERO alpha points, three MSES points and an eight-chord AVL mesh,
    /// with the same wake iterations and convergence gates. Never a
    /// full-fidelity result; every output that depends on it says so.
    Matrix,
}

impl FidelityProfile {
    /// Stable text recorded in run outputs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Matrix => "matrix_reduced",
        }
    }

    /// True when the reduced sweeps are active.
    pub fn is_reduced(self) -> bool {
        self == Self::Matrix
    }
}

/// Snapshot of the throughput options in force.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSettings {
    /// Sweep density for external tools.
    pub fidelity: FidelityProfile,
    /// Result-cache root; `None` disables the cache.
    pub cache_dir: Option<PathBuf>,
    /// Explicit VSPAERO thread count, or `None` for cores / concurrent jobs.
    pub vspaero_threads: Option<usize>,
    /// Explicit free-wake iteration count for VSPAERO, or `None` for the
    /// pipeline default (eight, the count the plateau gate was fitted on).
    pub vspaero_wake_iterations: Option<usize>,
    /// Analyses running side by side (divides the core count).
    pub concurrent_jobs: usize,
    /// Multiplier on external-tool wall-clock deadlines. A deadline is a
    /// guard against a hung solver, but on an oversubscribed host (several
    /// analyses side by side) a healthy solver can exceed it and turn a
    /// timed-out status into a different result; a matrix driver raises it.
    pub timeout_scale: f64,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            fidelity: FidelityProfile::Full,
            cache_dir: None,
            vspaero_threads: None,
            vspaero_wake_iterations: None,
            concurrent_jobs: 1,
            timeout_scale: 1.0,
        }
    }
}

static OVERRIDE: Mutex<Option<RuntimeSettings>> = Mutex::new(None);

impl RuntimeSettings {
    /// Options in force: a programmatic override if one is installed,
    /// otherwise the environment.
    pub fn current() -> Self {
        if let Ok(guard) = OVERRIDE.lock() {
            if let Some(settings) = guard.as_ref() {
                return settings.clone();
            }
        }
        Self::from_environment(|name| std::env::var(name).ok())
    }

    /// Install (or with `None` clear) a process-wide override. Intended for
    /// tests and for front ends that expose these options as settings.
    pub fn set_override(settings: Option<Self>) {
        if let Ok(mut guard) = OVERRIDE.lock() {
            *guard = settings;
        }
    }

    /// Parse the documented variables through `lookup`.
    pub fn from_environment(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let positive = |name: &str| {
            lookup(name)
                .and_then(|text| text.trim().parse::<usize>().ok())
                .filter(|&value| value > 0)
        };
        let fidelity = match lookup("ALAS_FIDELITY_PROFILE")
            .map(|text| text.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("matrix" | "matrix_reduced" | "reduced") => FidelityProfile::Matrix,
            _ => FidelityProfile::Full,
        };
        let cache_dir = lookup("ALAS_TOOL_CACHE_DIR")
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
            .map(PathBuf::from);
        Self {
            fidelity,
            cache_dir,
            vspaero_threads: positive("ALAS_VSPAERO_THREADS"),
            vspaero_wake_iterations: positive("ALAS_VSPAERO_WAKE_ITERATIONS"),
            concurrent_jobs: positive("ALAS_CONCURRENT_JOBS").unwrap_or(1),
            timeout_scale: lookup("ALAS_TOOL_TIMEOUT_SCALE")
                .and_then(|text| text.trim().parse::<f64>().ok())
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(1.0),
        }
    }

    /// `seconds` scaled by [`Self::timeout_scale`] (exactly `seconds` at the
    /// default scale of one).
    pub fn scaled_timeout_s(&self, seconds: f64) -> f64 {
        seconds * self.timeout_scale
    }

    /// OpenMP threads for one VSPAERO process: the explicit setting, else the
    /// logical cores divided among the concurrent jobs (never below one).
    pub fn vspaero_thread_count(&self) -> usize {
        self.vspaero_threads.unwrap_or_else(|| {
            let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
            (cores / self.concurrent_jobs.max(1)).max(1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> RuntimeSettings {
        RuntimeSettings::from_environment(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn an_empty_environment_is_full_fidelity_uncached() {
        let settings = env(&[]);
        assert_eq!(settings, RuntimeSettings::default());
        assert!(settings.cache_dir.is_none());
        assert!(!settings.fidelity.is_reduced());
    }

    #[test]
    fn variables_select_the_matrix_profile_cache_and_threads() {
        let settings = env(&[
            ("ALAS_FIDELITY_PROFILE", " Matrix "),
            ("ALAS_TOOL_CACHE_DIR", "D:/cache"),
            ("ALAS_VSPAERO_THREADS", "6"),
            ("ALAS_VSPAERO_WAKE_ITERATIONS", "16"),
        ]);
        assert_eq!(settings.fidelity, FidelityProfile::Matrix);
        assert_eq!(settings.cache_dir, Some(PathBuf::from("D:/cache")));
        assert_eq!(settings.vspaero_thread_count(), 6);
        assert_eq!(settings.vspaero_wake_iterations, Some(16));
        assert_eq!(env(&[]).vspaero_wake_iterations, None);
        assert_eq!(
            env(&[("ALAS_VSPAERO_WAKE_ITERATIONS", "0")]).vspaero_wake_iterations,
            None
        );
        assert_eq!(settings.fidelity.as_str(), "matrix_reduced");
    }

    #[test]
    fn the_timeout_scale_multiplies_deadlines_and_ignores_nonsense() {
        assert_eq!(env(&[]).scaled_timeout_s(600.0), 600.0);
        let scaled = env(&[("ALAS_TOOL_TIMEOUT_SCALE", "4")]);
        assert_eq!(scaled.scaled_timeout_s(600.0), 2400.0);
        for junk in ["0", "-2", "nan", "inf", "x"] {
            let leaked: &'static str = Box::leak(junk.to_owned().into_boxed_str());
            let pairs: &'static [(&'static str, &'static str)] =
                Box::leak(vec![("ALAS_TOOL_TIMEOUT_SCALE", leaked)].into_boxed_slice());
            assert_eq!(env(pairs).timeout_scale, 1.0, "{junk}");
        }
    }

    #[test]
    fn default_threads_divide_the_cores_among_concurrent_jobs() {
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        assert_eq!(env(&[]).vspaero_thread_count(), cores);
        let shared = env(&[("ALAS_CONCURRENT_JOBS", "1000000")]);
        assert_eq!(shared.vspaero_thread_count(), 1);
        let junk = env(&[
            ("ALAS_VSPAERO_THREADS", "0"),
            ("ALAS_FIDELITY_PROFILE", "x"),
        ]);
        assert_eq!(junk, RuntimeSettings::default());
    }
}
