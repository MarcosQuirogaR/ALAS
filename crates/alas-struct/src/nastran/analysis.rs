// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/nastran_runner.py (run_nastran_analysis).
// Reference: alas @ rust-port-baseline.

//! Writing the decks, running each enabled solve, and reading what came back.
//!
//! [`run_nastran_analysis`] is the whole row in one call, and its contract is
//! upstream's: it never fails. Every stage that cannot complete leaves its
//! solution's result at `not_run` or `error` with a reason attached, and the
//! other solutions carry on. A pipeline run must not stop because NASTRAN is
//! absent, and a wingbox with an analytical estimate and no finite-element
//! result is a normal outcome rather than a broken one.
//!
//! The layout on disk is upstream's, and its reasons are worth keeping:
//!
//! * The mesh is written once, at the top of the working directory, and each
//!   solution includes it as `../wing_mesh.bdf`. NASTRAN resolves that relative
//!   to the working directory it was launched in, which upstream confirmed
//!   directly.
//! * Each solution gets a directory of its own, because NASTRAN writes its
//!   `.f04`/`.f06`/`.log`/`.op2` beside the deck, and a force-killed run leaves
//!   scratch fragments behind as well. Flat, four solutions' output would be
//!   impossible to tell apart.
//! * That directory is emptied first. NASTRAN versions its own output when it
//!   finds a file of the same name -- `.f06` becomes `.f06.1`, then `.f06.2` --
//!   so a stale directory accumulates one more set per re-run instead of
//!   holding the latest solve.
//!
//! One scope decision, the same one [`alas_exec`]'s other consumer made: the
//! solver executable arrives already resolved. Upstream finds it through
//! `alas/paths.py`, which is frozen-build-aware and belongs to `alas-app`, a
//! layer above this crate. Upstream's `repo_root` argument existed only to feed
//! that resolver and is dropped with it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alas_config::{DesignRequirements, StructuresConfig};

use crate::loads::load_cases;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::op2::{read_op2, Op2};

use super::results::{
    read_force_psd_rms, read_harmonic_response, read_modes, read_static, ModesResult,
    NastranResults, ResultStatus, StaticResult, VibrationResult,
};
use super::run::{run_nastran_with_solver, NastranRunOutcome};
use super::{build_sol101_bulk, build_sol103_bulk, build_sol111_sine_bulk_msc, monitor_set};

/// The mesh every solution includes, written once at the top of the work tree.
const MESH_FILE: &str = "wing_mesh.bdf";

/// How each solution's deck refers to that mesh, from inside its own directory.
const MESH_INCLUDE: &str = "../wing_mesh.bdf";

/// Which solve a directory and its deck belong to.
///
/// Upstream keys a dictionary by these same four strings; they name directories
/// and files on disk, so they are fixed rather than incidental.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Solution {
    Sol101,
    Sol103,
    Sol111Sine,
}

impl Solution {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sol101 => "sol101",
            Self::Sol103 => "sol103",
            Self::Sol111Sine => "sol111_sine",
        }
    }
}

const RANDOM_RESPONSE_NOT_REQUESTED: &str =
    "Random-vibration RMS was not requested: enable force-PSD RMS calculation to integrate the SOL 111 unit-force response";

/// The default force-PSD bandwidth.  The RMS result is formed from monitor
/// displacements only, so this retains the engineering check without making a
/// fresh configuration pay for the optional extended sine sweep.
const DEFAULT_FORCE_PSD_MAX_HZ: f64 = 60.0;

/// Return the SOL 111 settings appropriate to the requested product.
///
/// A force-PSD-only request needs the unit-force response only over the RMS
/// band.  An explicitly enabled sine sweep keeps the configured upper limit,
/// because its purpose is to inspect the whole response rather than to form a
/// bounded RMS integral.
fn sol111_config(config: &StructuresConfig) -> StructuresConfig {
    let mut solve = config.clone();
    if config.run_sol_vibration_random && !config.run_sol_vibration_sine {
        solve.freq_sweep_max_hz = config.freq_sweep_max_hz.min(DEFAULT_FORCE_PSD_MAX_HZ);
    }
    solve
}

/// Why a solve produced nothing, when the reason is not a run outcome.
fn missing_executable_message(configured: &str) -> String {
    if configured.trim().is_empty() {
        "NASTRAN executable not configured (Setup > External Tools); \
         reporting analytical estimates only"
            .to_owned()
    } else if Path::new(configured).is_dir() {
        format!("NASTRAN installation incomplete at {configured}: executable launcher is missing")
    } else {
        format!("NASTRAN executable not found at {configured}")
    }
}

/// Write the decks for every enabled solution, run each one, and read the
/// results back.
///
/// `nastran_exe` is the resolved solver, or `None` when it could not be found;
/// `configured_path` is what the configuration asked for, which is what a
/// "not found" message has to quote. Nothing is run when
/// [`StructuresConfig::run_nastran`] is off -- the decks are still written, so a
/// user can solve them by hand.
pub fn run_nastran_analysis(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
    requirements: &DesignRequirements,
    work_dir: &Path,
    nastran_exe: Option<&Path>,
) -> NastranResults {
    let mut results = NastranResults::default();
    if !config.run_sol_vibration_random {
        results.vibration.random_response_error = Some(RANDOM_RESPONSE_NOT_REQUESTED.to_owned());
    }
    if let Err(error) = std::fs::create_dir_all(work_dir) {
        let detail = format!("cannot prepare {}: {error}", work_dir.display());
        return all_failed(&detail, config);
    }
    if let Err(error) = std::fs::write(work_dir.join(MESH_FILE), deck.write_bulk_msc()) {
        let detail = format!("cannot write the mesh into {}: {error}", work_dir.display());
        return all_failed(&detail, config);
    }

    let cases = load_cases(requirements, config.additional_safety_factor);
    let mut decks: Vec<(Solution, String)> = Vec::new();
    if config.run_sol_static {
        decks.push((
            Solution::Sol101,
            build_sol101_bulk(deck, node_index, requirements, config, MESH_INCLUDE),
        ));
    }
    if config.run_sol_modes {
        decks.push((Solution::Sol103, build_sol103_bulk(config, MESH_INCLUDE)));
    }
    if config.run_sol_vibration_sine || config.run_sol_vibration_random {
        let vibration_config = sol111_config(config);
        decks.push((
            Solution::Sol111Sine,
            build_sol111_sine_bulk_msc(&vibration_config, node_index, MESH_INCLUDE),
        ));
    }

    let mut written: BTreeMap<Solution, PathBuf> = BTreeMap::new();
    for (solution, content) in &decks {
        match write_deck(work_dir, *solution, content) {
            Ok(path) => {
                written.insert(*solution, path);
            }
            Err(error) => {
                let detail = format!("cannot write the {} deck: {error}", solution.as_str());
                assign_error(&mut results, *solution, &detail);
            }
        }
    }

    if !config.run_nastran {
        return results;
    }

    let Some(exe) = nastran_exe else {
        // MSC is optional. If it is absent but the locally built NASA
        // NASTRAN-95 executable is configured, use its SOL 101/SOL 103
        // dialect instead of reporting a fabricated empty result.
        if let Some(local) = crate::nastran95::run_nastran95_from_config_or_env(
            deck,
            node_index,
            config,
            requirements,
            work_dir,
        ) {
            return local;
        }
        let detail = missing_executable_message(&config.nastran_exe_path);
        for solution in written.keys() {
            assign_error(&mut results, *solution, &detail);
        }
        return results;
    };

    let solver_override = (!config.nastran_solver_path.trim().is_empty())
        .then(|| Path::new(config.nastran_solver_path.trim()));
    let outcomes: BTreeMap<Solution, NastranRunOutcome> = written
        .iter()
        .map(|(&solution, path)| {
            (
                solution,
                run_nastran_with_solver(path, exe, solver_override, config.timeout_s),
            )
        })
        .collect();

    if let Some(path) = written.get(&Solution::Sol101) {
        results.static_solve = match solved(path, outcomes.get(&Solution::Sol101)) {
            Solved::Result(op2) => read_static(&op2, node_index, &cases),
            Solved::Failed(detail) => StaticResult {
                status: ResultStatus::Error,
                error: Some(detail),
                ..StaticResult::default()
            },
        };
    }

    if let Some(path) = written.get(&Solution::Sol103) {
        results.modes = match solved(path, outcomes.get(&Solution::Sol103)) {
            Solved::Result(op2) => read_modes(&op2, deck, node_index),
            Solved::Failed(detail) => ModesResult {
                status: ResultStatus::Error,
                error: Some(detail),
                ..ModesResult::default()
            },
        };
    }

    let wants_vibration = config.run_sol_vibration_sine || config.run_sol_vibration_random;
    if wants_vibration {
        let sine = read_if_solved(
            written.get(&Solution::Sol111Sine),
            &outcomes,
            Solution::Sol111Sine,
        );
        results.vibration = if let Some(sine) = sine {
            let monitors = monitor_set(node_index);
            let mut vibration = read_harmonic_response(&sine, monitors);
            if config.run_sol_vibration_random {
                match read_force_psd_rms(&sine, monitors, config.random_force_psd_n2_per_hz) {
                    Ok(rms) => vibration.nastran_rms_m = rms,
                    Err(detail) => vibration.random_response_error = Some(detail),
                }
            } else {
                vibration.random_response_error = Some(RANDOM_RESPONSE_NOT_REQUESTED.to_owned());
            }
            vibration
        } else {
            let mut details = Vec::new();
            if let Some(outcome) = outcomes.get(&Solution::Sol111Sine) {
                details.push(format!("SOL 111 sine: {}", outcome.detail));
            }
            VibrationResult {
                status: ResultStatus::Error,
                error: Some(details.join("\n")),
                ..VibrationResult::default()
            }
        };
    }

    results
}

/// Either the result file a solve produced, or why there is not one.
enum Solved {
    Result(Box<Op2>),
    Failed(String),
}

/// The `.op2` beside `bdf_path`, if the run that should have written it worked.
fn solved(bdf_path: &Path, outcome: Option<&NastranRunOutcome>) -> Solved {
    let Some(outcome) = outcome else {
        return Solved::Failed("the solve was never attempted".to_owned());
    };
    if !outcome.ok {
        return Solved::Failed(outcome.detail.clone());
    }
    match read_result_file(bdf_path) {
        Ok(op2) => Solved::Result(Box::new(op2)),
        Err(detail) => Solved::Failed(detail),
    }
}

/// The same, for the two vibration solves, where an absent one is not an error
/// on its own -- the other may still have produced something.
fn read_if_solved(
    bdf_path: Option<&PathBuf>,
    outcomes: &BTreeMap<Solution, NastranRunOutcome>,
    solution: Solution,
) -> Option<Op2> {
    let path = bdf_path?;
    if !outcomes.get(&solution)?.ok {
        return None;
    }
    read_result_file(path).ok()
}

fn read_result_file(bdf_path: &Path) -> Result<Op2, String> {
    let op2_path = bdf_path.with_extension("op2");
    if !op2_path.exists() {
        return Err(format!(
            "{} ran cleanly but no .op2 was produced",
            bdf_path.display()
        ));
    }
    let bytes = std::fs::read(&op2_path)
        .map_err(|error| format!("cannot read {}: {error}", op2_path.display()))?;
    read_op2(&bytes).map_err(|error| format!("cannot read {}: {error}", op2_path.display()))
}

/// Write one solution's deck into a directory of its own, emptied first.
fn write_deck(
    work_dir: &Path,
    solution: Solution,
    content: &str,
) -> Result<PathBuf, std::io::Error> {
    let directory = work_dir.join(solution.as_str());
    // Ignored on purpose: the directory not existing is the desired state, and
    // that is exactly the case that reports an error here.
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("wing_{}.bdf", solution.as_str()));
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Record `detail` against whichever result the solution feeds.
fn assign_error(results: &mut NastranResults, solution: Solution, detail: &str) {
    match solution {
        Solution::Sol101 => {
            results.static_solve.status = ResultStatus::Error;
            results.static_solve.error = Some(detail.to_owned());
        }
        Solution::Sol103 => {
            results.modes.status = ResultStatus::Error;
            results.modes.error = Some(detail.to_owned());
        }
        Solution::Sol111Sine => {
            results.vibration.status = ResultStatus::Error;
            results.vibration.error = Some(detail.to_owned());
        }
    }
}

/// Report the same failure against every solution the configuration enabled.
///
/// Used when the working directory itself could not be prepared, which stops
/// all four before any of them has a deck.
fn all_failed(detail: &str, config: &StructuresConfig) -> NastranResults {
    let mut results = NastranResults::default();
    if config.run_sol_static {
        assign_error(&mut results, Solution::Sol101, detail);
    }
    if config.run_sol_modes {
        assign_error(&mut results, Solution::Sol103, detail);
    }
    if config.run_sol_vibration_sine || config.run_sol_vibration_random {
        assign_error(&mut results, Solution::Sol111Sine, detail);
    }
    results
}

// These tests write into a real temporary directory, so a failed unwrap is the
// test environment failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A working directory that deletes itself.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("alas_analysis_{label}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn node_index() -> MeshNodeIndex {
        MeshNodeIndex {
            root_nid: 4,
            tip_nid: 110,
            kink_nid: 46,
            spar_upper_nids: vec![vec![4, 46, 110]],
            spar_lower_nids: vec![vec![1004, 1046, 1110]],
            engine_nids: vec![46],
        }
    }

    /// Write the decks without running anything, and return the work directory.
    fn write_only(label: &str, config: &StructuresConfig) -> (TempDir, NastranResults) {
        let work = TempDir::new(label);
        let deck = Deck::new();
        let results = run_nastran_analysis(
            &deck,
            &node_index(),
            config,
            &DesignRequirements::default(),
            &work.path,
            None,
        );
        (work, results)
    }

    /// A configuration that enables exactly the named solutions.
    ///
    /// `exe` is what the configuration asks for, which is separate from whether
    /// the solver was found: the two produce different messages.
    fn config(run: bool, solutions: [bool; 4], exe: &str) -> StructuresConfig {
        StructuresConfig {
            run_nastran: run,
            nastran_exe_path: exe.to_owned(),
            run_sol_static: solutions[0],
            run_sol_modes: solutions[1],
            run_sol_vibration_sine: solutions[2],
            run_sol_vibration_random: solutions[3],
            ..StructuresConfig::default()
        }
    }

    #[test]
    fn a_force_psd_request_reuses_the_validated_unit_force_sol111_deck() {
        let config = config(false, [true, true, true, true], "");
        let (work, results) = write_only("all_four", &config);

        for name in ["sol101", "sol103", "sol111_sine"] {
            let deck = work.path.join(name).join(format!("wing_{name}.bdf"));
            assert!(deck.exists(), "{} was not written", deck.display());
        }
        assert!(!work.path.join("sol111_random").exists());
        // The mesh is written once, at the top, where every deck's relative
        // include resolves to it.
        assert!(work.path.join(MESH_FILE).exists());
        // Nothing ran, so nothing was read: the results stay at their default.
        assert_eq!(results.static_solve.status, ResultStatus::NotRun);
        assert_eq!(results.modes.status, ResultStatus::NotRun);
        assert_eq!(results.vibration.status, ResultStatus::NotRun);
        assert!(results.vibration.random_response_error.is_none());
    }

    #[test]
    fn a_disabled_solution_gets_no_deck_at_all() {
        let config = config(false, [true, false, false, false], "");
        let (work, _) = write_only("static_only", &config);

        assert!(work.path.join("sol101").join("wing_sol101.bdf").exists());
        assert!(!work.path.join("sol103").exists());
        assert!(!work.path.join("sol111_sine").exists());
    }

    #[test]
    fn a_previous_runs_output_is_wiped_rather_than_versioned_alongside() {
        // The failure this prevents: NASTRAN renames rather than overwrites, so
        // a directory left alone accumulates one more set of output per re-run.
        let config = config(false, [true, false, false, false], "");

        let work = TempDir::new("stale");
        let stale = work.path.join("sol101");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("wing_sol101.f06"), "an earlier solve").unwrap();

        let _ = run_nastran_analysis(
            &Deck::new(),
            &node_index(),
            &config,
            &DesignRequirements::default(),
            &work.path,
            None,
        );
        assert!(!stale.join("wing_sol101.f06").exists());
        assert!(stale.join("wing_sol101.bdf").exists());
    }

    #[test]
    fn an_unconfigured_solver_is_reported_against_the_enabled_solutions_only() {
        let config = config(true, [true, true, false, false], "");
        let (_work, results) = write_only("unconfigured", &config);

        assert_eq!(results.static_solve.status, ResultStatus::Error);
        assert_eq!(results.modes.status, ResultStatus::Error);
        // Vibration was never asked for, so it is not an error -- it did not run.
        assert_eq!(results.vibration.status, ResultStatus::NotRun);
        let error = results.static_solve.error.clone().unwrap();
        assert!(error.contains("not configured"), "{error}");
    }

    #[test]
    fn a_configured_but_missing_solver_names_the_path_that_was_looked_for() {
        let config = config(true, [true, false, false, false], r"C:\nowhere\nastran.exe");
        let (_work, results) = write_only("missing", &config);

        let error = results.static_solve.error.clone().unwrap();
        assert!(error.contains(r"C:\nowhere\nastran.exe"), "{error}");
    }

    #[test]
    fn a_solver_that_cannot_be_launched_leaves_each_solution_with_its_own_reason() {
        let config = config(true, [true, true, true, true], "");

        let work = TempDir::new("unlaunchable");
        let results = run_nastran_analysis(
            &Deck::new(),
            &node_index(),
            &config,
            &DesignRequirements::default(),
            &work.path,
            Some(Path::new("no_such_nastran_executable_anywhere")),
        );

        assert_eq!(results.static_solve.status, ResultStatus::Error);
        assert_eq!(results.modes.status, ResultStatus::Error);
        assert_eq!(results.vibration.status, ResultStatus::Error);
        let error = results.vibration.error.clone().unwrap();
        assert!(error.contains("SOL 111 sine:"), "{error}");
        assert!(results.vibration.nastran_rms_m.is_empty());
        assert!(results.vibration.miles_rms_m.is_empty());
    }

    #[test]
    fn a_random_only_request_writes_the_unit_force_deck_needed_for_rms() {
        let config = config(true, [false, false, false, true], "configured.exe");
        let (work, results) = write_only("random_force_psd", &config);

        assert!(!work.path.join("sol111_random").exists());
        assert!(work.path.join("sol111_sine/wing_sol111_sine.bdf").exists());
        assert_eq!(results.vibration.status, ResultStatus::Error);
        assert!(results
            .vibration
            .error
            .as_deref()
            .is_some_and(|error| error.contains("configured.exe")));
        assert!(results.vibration.frf_freq_hz.is_none());
        assert!(results.vibration.nastran_rms_m.is_empty());
    }

    #[test]
    fn a_force_psd_only_request_is_limited_to_the_default_rms_band() {
        let config = StructuresConfig {
            run_sol_vibration_sine: false,
            run_sol_vibration_random: true,
            freq_sweep_max_hz: 500.0,
            ..StructuresConfig::default()
        };

        assert_eq!(sol111_config(&config).freq_sweep_max_hz, 60.0);
    }

    #[test]
    fn an_explicit_sine_sweep_keeps_its_requested_upper_frequency() {
        let config = StructuresConfig {
            run_sol_vibration_sine: true,
            run_sol_vibration_random: true,
            freq_sweep_max_hz: 500.0,
            ..StructuresConfig::default()
        };

        assert_eq!(sol111_config(&config).freq_sweep_max_hz, 500.0);
    }
}
