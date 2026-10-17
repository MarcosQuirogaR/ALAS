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

use super::deck::build_static_deck_product_in;
use super::units::UnitScale;
use super::{
    displacement_of, read_displacement_tables, read_eigenvalues, read_eigenvector_tables,
    run_nastran95, Dialect, Nastran95Solver, RunOutcome,
};
use crate::loads;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::nastran::{ModesResult, NastranResults, ResultStatus, StaticResult, VibrationResult};

const SOL101: &str = "sol101";

/// The unit system of the product decks written here: decimetre - 10 kg -
/// newton. See [`super::units`] for why metres cannot be used (NASTRAN-95's
/// absolute `CQUAD4` degeneracy threshold) and why millimetres are not used
/// either (its results move with the length unit). Print-file displacements
/// and stresses are converted back to SI by the readers below.
pub(super) const PRODUCT_UNITS: UnitScale = UnitScale::DECIMETRE;

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
    // Name any panel the solver's absolute CQUAD4 test would reject before it
    // is started; the solver itself only reports an opaque fatal 2224.
    let flagged = super::quad_guard::degenerate_quads(deck, PRODUCT_UNITS);
    if let Some(message) = super::quad_guard::degeneracy_message(&flagged, PRODUCT_UNITS) {
        return guard_rejection(config, message);
    }
    if config.run_sol_static {
        let text = build_static_deck_product_in(
            deck,
            node_index,
            requirements,
            config,
            Dialect::Nastran95,
            PRODUCT_UNITS,
        );
        results.static_solve = match solve_and_retain(solver, work_dir, SOL101, &text, config) {
            Ok(print) => {
                let mut result =
                    read_static_print(&print, node_index, requirements, config, PRODUCT_UNITS);
                let cases = loads::load_cases(requirements, config.additional_safety_factor);
                let mut response =
                    super::read_static_spanwise_print(&print, deck, node_index, &cases);
                for case in &mut response.cases {
                    for translation in &mut case.translations_m {
                        for component in translation {
                            *component /= PRODUCT_UNITS.length;
                        }
                    }
                }
                crate::nastran::static_spanwise::attach(&mut result, response);
                let mut stress = super::read_static_shell_stress_print(&print, deck, &cases);
                shell_stress_to_si(&mut stress, PRODUCT_UNITS);
                if let Some(error) = &stress.error {
                    result.status = ResultStatus::Error;
                    result.error = Some(format!(
                        "{}shell stress output: {error}",
                        result
                            .error
                            .as_ref()
                            .map_or(String::new(), |e| format!("{e}; "))
                    ));
                } else {
                    let excluded = deck.load_introduction_elements(
                        &node_index.engine_nids,
                        crate::mesh::LOAD_INTRODUCTION_LAYERS,
                    );
                    for case in &stress.cases {
                        let peak = case
                            .samples
                            .iter()
                            .map(|s| s.von_mises_pa)
                            .fold(0.0_f64, f64::max);
                        result.root_von_mises_max_pa.push(case.name, peak);
                        // Unavailable (not 0 Pa) when nothing is assessed.
                        if let Some(assessed) = crate::mesh::assessed_peak(
                            case.samples
                                .iter()
                                .map(|s| (excluded.contains(&s.element_id), s.von_mises_pa)),
                        ) {
                            result.assessed_von_mises_max_pa.push(case.name, assessed);
                        }
                    }
                }
                result.shell_stress = Some(stress);
                result
            }
            Err(detail) => StaticResult {
                status: ResultStatus::Error,
                error: Some(detail),
                ..StaticResult::default()
            },
        };
    }

    if config.run_sol_modes {
        results.modes =
            super::modal_search::solve_modes(solver, work_dir, deck, node_index, config);
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

/// The results of a run the element guard stopped: every requested solution
/// is an `Error` carrying the guard's diagnostic, never a fabricated value.
fn guard_rejection(config: &StructuresConfig, message: String) -> NastranResults {
    let mut results = NastranResults::default();
    if config.run_sol_static {
        results.static_solve = StaticResult {
            status: ResultStatus::Error,
            error: Some(message.clone()),
            ..StaticResult::default()
        };
    }
    if config.run_sol_modes {
        results.modes = ModesResult {
            status: ResultStatus::Error,
            error: Some(message),
            ..ModesResult::default()
        };
    }
    results
}

/// Run one local solution and retain the input/output pair beside it.
pub(super) fn solve_and_retain(
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
    units: UnitScale,
) -> StaticResult {
    let tables = read_displacement_tables(print);
    let cases = loads::load_cases(requirements, config.additional_safety_factor);
    let mut result = StaticResult {
        status: ResultStatus::Ok,
        ..StaticResult::default()
    };
    for (index, case) in cases.iter().enumerate() {
        if let Some(row) = displacement_of(&tables, index, node_index.tip_nid) {
            result
                .tip_deflection_m
                .push(case.name, row[2] / units.length);
        }
    }
    result
}

/// Read local SOL 103 eigenvalues and front-spar mode shapes.
#[cfg(test)]
fn read_modes_print(
    print: &str,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    requested_modes: i64,
) -> ModesResult {
    read_modes_print_limited(print, deck, node_index, requested_modes, usize::MAX)
}

/// As [`read_modes_print`], keeping at most `confirmed` leading roots: the
/// prefix the Sturm counts confirm to be gap-free.
pub(super) fn read_modes_print_limited(
    print: &str,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    requested_modes: i64,
    confirmed: usize,
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
        .take((requested_modes.max(1) as usize).min(confirmed))
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
    let eigenvector_tables = match read_eigenvector_tables(print) {
        Ok(tables) => tables,
        Err(error) => {
            return ModesResult {
                status: ResultStatus::Error,
                error: Some(error.to_string()),
                frequencies_hz,
                ..ModesResult::default()
            };
        }
    };
    let mut mode_shapes = Vec::new();
    for &mode_index in &kept {
        // Keep one shape slot for every retained frequency, even when the
        // print file omitted that mode's eigenvector table.  The public result
        // promises parallel frequency/shape vectors; skipping a missing table
        // would shift every later NASTRAN-95 shape onto the wrong mode.
        let values = eigenvector_tables
            .get(mode_index)
            .or_else(|| displacement_tables.get(mode_index))
            .map(|table| {
                ordered
                    .iter()
                    .map(|&(_, nid)| {
                        table
                            .iter()
                            .find(|&&(grid, _)| grid == nid)
                            .map_or(f64::NAN, |&(_, row)| row[2])
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![f64::NAN; ordered.len()]);
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

/// Convert printed shell stresses (deck units) to SI in place.
fn shell_stress_to_si(response: &mut crate::nastran::StaticShellStressResponse, units: UnitScale) {
    for case in &mut response.cases {
        for sample in &mut case.samples {
            sample.fiber_distance_m /= units.length;
            sample.normal_x_pa /= units.stress();
            sample.normal_y_pa /= units.stress();
            sample.shear_xy_pa /= units.stress();
            sample.von_mises_pa /= units.stress();
        }
    }
}

/// Run with the persisted desktop configuration when present, otherwise retain
/// the environment-variable contract used by headless developer workflows.
/// `ALAS_TOOL_DISCOVERY=disabled` suppresses this automatic fallback.
pub fn run_nastran95_from_config_or_env(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
    requirements: &DesignRequirements,
    work_dir: &Path,
) -> Option<NastranResults> {
    let solver = configured_solver(config, alas_exec::tools::tool_discovery_enabled())?;
    Some(run_nastran95_analysis(
        deck,
        node_index,
        config,
        requirements,
        work_dir,
        &solver,
    ))
}

fn configured_solver(
    config: &StructuresConfig,
    discovery_enabled: bool,
) -> Option<Nastran95Solver> {
    if !discovery_enabled {
        return None;
    }
    let dir = Path::new(config.nastran95_dir_path.trim());
    let runtime = nonempty_path(&config.nastran95_runtime_path);
    let rf_stage = nonempty_path(&config.nastran95_rf_stage_path);
    let open_core_words = nonempty_text(&config.nastran95_open_core_words);
    if dir.as_os_str().is_empty() {
        Nastran95Solver::from_env().or_else(Nastran95Solver::from_adjacent_bundle)
    } else {
        Nastran95Solver::from_paths(
            dir,
            runtime.as_deref(),
            rf_stage.as_deref(),
            open_core_words.as_deref(),
        )
    }
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
    fn disabled_discovery_skips_configured_local_solver_selection() -> std::io::Result<()> {
        let root =
            std::env::temp_dir().join(format!("alas-nastran95-discovery-{}", std::process::id()));
        let executable = root.join("build/bin/nastran.exe");
        fs::create_dir_all(root.join("build/bin"))?;
        fs::create_dir_all(root.join("rf"))?;
        fs::write(&executable, b"test")?;
        fs::write(root.join("rf/NASINFO"), b"test")?;
        let config = StructuresConfig {
            nastran95_dir_path: root.display().to_string(),
            ..StructuresConfig::default()
        };

        assert!(configured_solver(&config, false).is_none());
        assert_eq!(
            configured_solver(&config, true).map(|solver| solver.exe),
            Some(executable.clone())
        );
        assert_eq!(
            Nastran95Solver::from_paths(&root, None, None, None).map(|solver| solver.exe),
            Some(executable)
        );
        fs::remove_dir_all(root)
    }

    #[test]
    fn a_degenerate_panel_stops_the_run_with_a_named_element_before_any_solver_starts() {
        // A 0.5 mm high trailing-edge panel at deck-scale x10 is fine; the same
        // geometry 100000x smaller is below the solver's absolute CQUAD4 test.
        let mut deck = Deck::new();
        for (nid, xyz) in [
            (1, [0.0, 0.0, 0.0]),
            (2, [1.0e-5, 0.0, 0.0]),
            (3, [1.0e-5, 0.0, 1.0e-6]),
            (4, [0.0, 0.0, 1.0e-6]),
        ] {
            deck.add_grid(nid, xyz);
        }
        deck.quads.push(crate::mesh::Shell {
            eid: 4242,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        let index = MeshNodeIndex {
            root_nid: 1,
            tip_nid: 2,
            kink_nid: 1,
            spar_upper_nids: vec![vec![1, 2]],
            spar_lower_nids: vec![],
            engine_nids: vec![],
        };
        // The solver path is never reached, so a placeholder installation is
        // enough; its executable does not exist.
        let solver = Nastran95Solver {
            exe: "nonexistent/nastran.exe".into(),
            rf_source: "nonexistent/rf".into(),
            runtime: None,
            runtime_source: super::super::run::Nastran95RuntimeSource::Unspecified,
            rf_stage: None,
            open_core_words: None,
            max_open_core_words: 1,
        };
        let config = StructuresConfig {
            run_sol_static: true,
            run_sol_modes: true,
            ..StructuresConfig::default()
        };
        let results = run_nastran95_analysis(
            &deck,
            &index,
            &config,
            &DesignRequirements::default(),
            Path::new("unused"),
            &solver,
        );
        for (status, error) in [
            (results.static_solve.status, &results.static_solve.error),
            (results.modes.status, &results.modes.error),
        ] {
            assert_eq!(status, ResultStatus::Error);
            let message = error.as_deref().unwrap_or_default();
            assert!(
                message.contains("4242") && message.contains("element guard"),
                "{message}"
            );
        }
    }

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
        let result = read_static_print(&print, &index, &requirements, &config, UnitScale::SI);
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

    #[test]
    fn local_modes_keep_shape_slots_when_a_print_vector_is_missing() {
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
 1 G 0.0 0.0 0.5 0.0 0.0 0.0
 2 G 0.0 0.0 1.0 0.0 0.0 0.0
 R E A L   E I G E N V E C T O R   N O .          3
 1 G 0.0 0.0 0.25 0.0 0.0 0.0
 2 G 0.0 0.0 1.0 0.0 0.0 0.0
";

        let result = read_modes_print(print, &deck, &index, 2);

        assert_eq!(result.frequencies_hz, vec![0.25, 1.0]);
        assert_eq!(result.mode_shapes.len(), result.frequencies_hz.len());
        assert!(result.mode_shapes[0].iter().all(|value| value.is_nan()));
        assert!(result.mode_shapes[1].iter().all(|value| value.is_finite()));
    }
}
