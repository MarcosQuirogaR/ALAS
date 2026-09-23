// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-branch optimizer configuration and evidence directories.

use std::path::PathBuf;

use alas_config::AlasConfig;

/// The solver configuration a run with `parallel` uses.
///
/// `parallel` is the run-level "use this machine" switch (`--no-parallel`
/// clears it). It decides whether the VLM and AVL branches run side by side,
/// but candidate evaluation inside each branch reads
/// `optimizer.solver.workers` and never sees the flag, so without this clamp
/// a serial request would still start a batch on every worker the setting
/// allows.
///
/// It is a scheduling change only - the batch, its designs and their scores
/// are identical at any worker count - so a serial run returns the same
/// aircraft, more slowly.
///
/// What this does *not* claim is a single-threaded process. One coupled
/// evaluation still fills its vortex-lattice influence matrix and factorises
/// it across the shared rayon pool (`alas_aero::vlm::system`,
/// `alas_math::linalg`). That is data parallelism inside one arithmetic
/// operation, with a result documented and tested as bit-identical to the
/// serial loop, not concurrent evaluation of independent work; no candidate,
/// branch or pipeline stage overlaps another under `--no-parallel`.
pub(super) fn serial_solver_config(config: &AlasConfig, parallel: bool) -> AlasConfig {
    let mut config = config.clone();
    if !parallel {
        config.optimizer.solver.workers = 1;
    }
    config
}

/// Create the VLM branch's evidence directory. The native search writes
/// nothing there itself, so a failure is logged and the search still runs.
pub(super) fn create_branch_directory(output_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = &output_dir {
        if let Err(error) = std::fs::create_dir_all(path) {
            tracing::warn!(%error, path = %path.display(), "solver branch directory could not be created");
        }
    }
    output_dir
}

/// The optimizer configuration with the run's seed applied.
pub(crate) fn seeded_config(config: &AlasConfig, seed: Option<u64>) -> Result<AlasConfig, String> {
    let mut effective = config.clone();
    if let Some(seed) = seed {
        effective
            .optimizer
            .solver
            .set_seed(seed)
            .map_err(|error| error.to_string())?;
    }
    Ok(effective)
}
