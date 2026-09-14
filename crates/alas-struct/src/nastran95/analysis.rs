// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Running the local NASTRAN-95 dialect and adapting its print tables to the
//! structural result contract.
//!
//! The NASA executable is invoked as a separate NOSA-licensed program. This
//! module owns the AGPL-compatible boundary around it: it writes the
//! NASTRAN-95 deck, captures the print file as a retained `.f06` artifact, and
//! maps the SOL 101 and SOL 103 tables into the same result types the MSC path
//! uses. No MSC or Patran installation is needed for these two solutions.

use std::fs;
use std::path::Path;

use alas_config::{DesignRequirements, StructuresConfig};

use super::deck::build_modes_deck_for_nodes;
use super::{
    build_static_deck, displacement_of, read_displacement_tables, read_eigenvalues,
    read_eigenvector_tables, run_nastran95, Dialect, Nastran95Solver, RunOutcome,
};
use crate::loads;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::nastran::{ModesResult, NastranResults, ResultStatus, StaticResult, VibrationResult};

const SOL101: &str = "sol101";
const SOL103: &str = "sol103";

/// Run the local NASTRAN-95 implementation for the enabled static and modal
/// solutions.
///
/// The solution directories are emptied by [`run_nastran95`] before each
/// launch. Once the child exits, this function writes the exact submitted BDF
/// and captured print file beside that solution so a successful or failed run
/// remains inspectable. NASTRAN-95 has no compatible SOL 111 path in this
/// adapter; a request for it is reported explicitly instead of being silently
/// sent to the wrong solver.
pub fn run_nastran95_analysis(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
    requirements: &DesignRequirements,
    work_dir: &Path,
    solver: &Nastran95Solver,
) -> NastranResults {
    let mut results = NastranResults::default();
    if config.run_sol_static {
        let text = build_static_deck(deck, node_index, requirements, config, Dialect::Nastran95);
        results.static_solve = match solve_and_retain(solver, work_dir, SOL101, &text, config) {
            Ok(print) => read_static_print(&print, node_index, requirements, config),
            Err(detail) => StaticResult {
                status: ResultStatus::Error,
                error: Some(detail),
                ..StaticResult::default()
            },
        };
    }

    if config.run_sol_modes {
        let output_nodes = node_index
            .spar_upper_nids
            .first()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let text = build_modes_deck_for_nodes(deck, config, Dialect::Nastran95, output_nodes);
        results.modes = match solve_and_retain(solver, work_dir, SOL103, &text, config) {
            Ok(print) => read_modes_print(&print, deck, node_index, config.n_modes),
            Err(detail) => ModesResult {
                status: ResultStatus::Error,
                error: Some(detail),
                ..ModesResult::default()
            },
        };
    }

    if config.run_sol_vibration_sine || config.run_sol_vibration_random {
        results.vibration = VibrationResult {
            status: ResultStatus::Error,
            error: Some(
                "NASTRAN-95 backend does not provide the modern SOL 111 modal frequency-response contract"
                    .to_owned(),
            ),
            random_response_error: config.run_sol_vibration_random.then(|| {
                "NASTRAN-95 backend does not provide random-response RMS results".to_owned()
            }),
            ..VibrationResult::default()
        };
    }
    results
}

/// Run one local solution and retain the input/output pair beside it.
fn solve_and_retain(
    solver: &Nastran95Solver,
    work_dir: &Path,
    solution: &str,
    deck: &str,
    config: &StructuresConfig,
) -> Result<String, String> {
    let solution_dir = work_dir.join(solution);
    fs::create_dir_all(&solution_dir)
        .map_err(|error| format!("cannot create {solution} artifact directory: {error}"))?;
    let transient_dir = solver.workspace_for(&solution_dir)?;
    let outcome = run_nastran95(solver, deck, &transient_dir, config.timeout_s);
    fs::write(solution_dir.join(format!("wing_{solution}.bdf")), deck)
        .map_err(|error| format!("cannot retain {solution} BDF: {error}"))?;
    match outcome {
        RunOutcome::Print(print) => {
            fs::write(solution_dir.join(format!("wing_{solution}.f06")), &print)
                .map_err(|error| format!("cannot retain {solution} print file: {error}"))?;
            Ok(print)
        }
        RunOutcome::Failed(detail) => {
            fs::write(solution_dir.join("run.error.txt"), &detail)
                .map_err(|error| format!("cannot retain {solution} failure detail: {error}"))?;
            Err(detail)
        }
    }
}

/// Read the local SOL 101 displacement tables into the common static result.
fn read_static_print(
    print: &str,
    node_index: &MeshNodeIndex,
    requirements: &DesignRequirements,
    config: &StructuresConfig,
) -> StaticResult {
    let tables = read_displacement_tables(print);
    let cases = loads::load_cases(requirements, config.additional_safety_factor);
    let mut result = StaticResult {
        status: ResultStatus::Ok,
        ..StaticResult::default()
    };
    for (index, case) in cases.iter().enumerate() {
        if let Some(row) = displacement_of(&tables, index, node_index.tip_nid) {
            result.tip_deflection_m.push(case.name, row[2]);
        }
    }
    result
}

/// Read local SOL 103 eigenvalues and front-spar mode shapes.
fn read_modes_print(
    print: &str,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    requested_modes: i64,
) -> ModesResult {
    let all_modes = read_eigenvalues(print);
    // NASTRAN-95 substitutes zero for rigid-body frequencies (the BULK manual
    // documents this for non-FEER-X extraction). A fixed 0.5 Hz cutoff has no
    // physical basis and discards valid low-frequency elastic roots, so retain
    // every finite positive root and let the solver/model diagnostics describe
    // mesh or constraint quality separately.
    let kept: Vec<usize> = all_modes
        .iter()
        .enumerate()
        .filter(|(_, mode)| {
            mode.eigenvalue.is_finite()
                && mode.eigenvalue > 0.0
                && mode.cyclic_hz.is_finite()
                && mode.cyclic_hz > 0.0
        })
        .map(|(index, _)| index)
        .take(requested_modes.max(1) as usize)
        .collect();
    let frequencies_hz = kept
        .iter()
        .map(|&index| all_modes[index].cyclic_hz)
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return ModesResult {
            status: ResultStatus::Ok,
            frequencies_hz,
            ..ModesResult::default()
        };
    }

    let Some(front_spar) = node_index.spar_upper_nids.first() else {
        return ModesResult {
            status: ResultStatus::Error,
            error: Some("the mesh index has no spar node lines".to_owned()),
            frequencies_hz,
            ..ModesResult::default()
        };
    };
    let mut ordered = front_spar
        .iter()
        .copied()
        .map(|nid| (deck.node_y(nid), nid))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mode_shape_y_m = ordered.iter().map(|&(y, _)| y).collect::<Vec<_>>();
    let displacement_tables = read_displacement_tables(print);
    let eigenvector_tables = read_eigenvector_tables(print);
    let mut mode_shapes = Vec::new();
    for &mode_index in &kept {
        let table = eigenvector_tables
            .get(mode_index)
            .or_else(|| displacement_tables.get(mode_index));
        let Some(table) = table else {
            continue;
        };
        let values = ordered
            .iter()
            .map(|&(_, nid)| {
                table
                    .iter()
                    .find(|&&(grid, _)| grid == nid)
                    .map_or(f64::NAN, |&(_, row)| row[2])
            })
            .collect::<Vec<_>>();
        let scale = values
            .iter()
            .filter(|value| value.is_finite())
            .map(|value| value.abs())
            .fold(0.0, f64::max)
            .max(1.0e-30);
        mode_shapes.push(values.into_iter().map(|value| value / scale).collect());
    }
    ModesResult {
        status: ResultStatus::Ok,
        frequencies_hz,
        mode_shapes,
        mode_shape_y_m: Some(mode_shape_y_m),
        ..ModesResult::default()
    }
}

/// Run with the persisted desktop configuration when present, otherwise retain
/// the environment-variable contract used by headless developer workflows.
pub fn run_nastran95_from_config_or_env(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
    requirements: &DesignRequirements,
    work_dir: &Path,
) -> Option<NastranResults> {
    let dir = Path::new(config.nastran95_dir_path.trim());
    let runtime = nonempty_path(&config.nastran95_runtime_path);
    let rf_stage = nonempty_path(&config.nastran95_rf_stage_path);
    let open_core_words = nonempty_text(&config.nastran95_open_core_words);
    let solver = if dir.as_os_str().is_empty() {
        Nastran95Solver::from_env().or_else(Nastran95Solver::from_adjacent_bundle)
    } else {
        Nastran95Solver::from_paths(
            dir,
            runtime.as_deref(),
            rf_stage.as_deref(),
            open_core_words.as_deref(),
        )
    }?;
    Some(run_nastran95_analysis(
        deck,
        node_index,
        config,
        requirements,
        work_dir,
        &solver,
    ))
}

/// Public helper retained for developer and test environments that configure
/// the local solver only through `ALAS_NASTRAN95_*` variables.
pub fn run_nastran95_from_env(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
    requirements: &DesignRequirements,
    work_dir: &Path,
) -> Option<NastranResults> {
    let solver = Nastran95Solver::from_env()?;
    Some(run_nastran95_analysis(
        deck,
        node_index,
        config,
        requirements,
        work_dir,
        &solver,
    ))
}

fn nonempty_path(value: &str) -> Option<std::path::PathBuf> {
    (!value.trim().is_empty()).then(|| std::path::PathBuf::from(value.trim()))
}

fn nonempty_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_static_prints_report_tip_deflection_without_fabricating_stress() {
        let index = MeshNodeIndex {
            root_nid: 1,
            tip_nid: 2,
            kink_nid: 1,
            spar_upper_nids: vec![vec![1, 2]],
            spar_lower_nids: vec![],
            engine_nids: vec![],
        };
        let config = StructuresConfig {
            num_ribs_override: Some(2),
            ..StructuresConfig::default()
        };
        let requirements = DesignRequirements::default();
        let one_case = "\
 D I S P L A C E M E N T   V E C T O R
             1      G      0.0  0.0  0.0  0.0  0.0  0.0
             2      G      0.0  0.0  1.25 0.0  0.0  0.0
";
        let print = format!("{one_case}{one_case}{one_case}");
        let result = read_static_print(&print, &index, &requirements, &config);
        assert_eq!(result.status, ResultStatus::Ok);
        assert_eq!(result.root_von_mises_max_pa.len(), 0);
        assert_eq!(result.tip_deflection_m.labels().len(), 3);
        assert!(result
            .tip_deflection_m
            .iter()
            .all(|(_, value)| (value - 1.25).abs() < 1.0e-12));
    }

    #[test]
    fn local_modes_filter_zero_rigid_body_frequency_and_keep_sub_hz_elastic_modes() {
        let mut deck = Deck::new();
        deck.add_grid(1, [0.0, 0.0, 0.0]);
        deck.add_grid(2, [0.0, 1.0, 0.0]);
        let index = MeshNodeIndex {
            root_nid: 1,
            tip_nid: 2,
            kink_nid: 1,
            spar_upper_nids: vec![vec![1, 2]],
            spar_lower_nids: vec![],
            engine_nids: vec![],
        };
        let print = "\
 R E A L   E I G E N V A L U E S
 1 1 0.0 0.0 0.0
 2 1 1.0 1.0 0.25
 3 1 4.0 2.0 1.0
 R E A L   E I G E N V E C T O R   N O .          1
 1 G 0.0 0.0 0.0 0.0 0.0 0.0
 2 G 0.0 0.0 0.0 0.0 0.0 0.0
 R E A L   E I G E N V E C T O R   N O .          2
 1 G 0.0 0.0 0.5 0.0 0.0 0.0
 2 G 0.0 0.0 1.0 0.0 0.0 0.0
 R E A L   E I G E N V E C T O R   N O .          3
 1 G 0.0 0.0 0.25 0.0 0.0 0.0
 2 G 0.0 0.0 1.0 0.0 0.0 0.0
";
        let result = read_modes_print(print, &deck, &index, 2);
        assert_eq!(result.frequencies_hz, vec![0.25, 1.0]);
        assert_eq!(result.mode_shape_y_m, Some(vec![0.0, 1.0]));
        assert_eq!(result.mode_shapes, vec![vec![0.5, 1.0], vec![0.25, 1.0]]);
    }
}
