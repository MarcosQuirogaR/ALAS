// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn print_structural_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    let st = match result.structural_result {
        Some(ref s) => s,
        None => return,
    };
    if st.status.as_str() != "ok" {
        println!("\n--- Structural analysis: {} ---", st.status.as_str());
        if let Some(ref err) = st.error {
            println!("  {err}");
        }
        return;
    }

    println!("\n--- Wingbox structural sizing ---");
    if let Some(ref sizing) = st.sizing {
        println!(
            "  semi-wing mass   : {:.1} kg (Torenbeek: {:.1} kg)",
            sizing.total_mass_kg, st.torenbeek_wing_mass_kg
        );
        println!(
            "  ribs / spacing   : {} / {:.2} m installed (max {:.2} m allowable)",
            sizing.num_ribs,
            sizing.installed_rib_spacing_m(),
            sizing.rib_spacing_m
        );
        println!("  sizing load case : {}", sizing.sizing_load_case);
    }
    if let Some(ref health) = st.mesh_health {
        println!(
            "  mesh elements    : CQUAD4: {}, CTRIA3: {}",
            health.n_cquad4, health.n_ctria3
        );
    }
    if let Some(ref ana) = st.analysis {
        if let Some(lc) = ana.load_cases.first() {
            println!(
                "  tip deflection (case: {}) : {:.3} m",
                lc.name, lc.tip_deflection_m
            );
        }
        if let Some(&first_freq) = ana.modal.frequencies_hz.first() {
            println!("  first bending mode freq : {:.2} Hz", first_freq);
        }
    }
}

/// One registry entry recorded by the headless plot exporter.
///
/// Keeping unavailable entries in the manifest makes an incomplete export
/// explicit instead of making a missing SVG indistinguishable from a writer
/// failure or an unrequested figure.
#[derive(Debug, serde::Serialize)]
struct PlotExportEntry {
    id: &'static str,
    title: &'static str,
    required_stage: String,
    status: &'static str,
    path: Option<String>,
    reason: Option<String>,
}

fn plots_dir(output_dir: Option<&Path>) -> PathBuf {
    output_dir
        .map(|dir| dir.join("plots"))
        .unwrap_or_else(|| PathBuf::from("plots"))
}

/// Build and save every result scene registered by [`alas_report`].
///
/// The CLI already depends on `alas-gui`, whose public scene factory is the
/// single dispatch point for report figures. Keeping this call here avoids a
/// second id-to-family mapping while allowing headless runs to use a caller's
/// output directory. Every registered ID is attempted, and unavailable scenes
/// are retained in `plot_manifest.json` with the stage metadata that explains
/// why no SVG was emitted. Scene serialization is used only as the bridge to
/// this crate's SVG writer; it preserves the report scene primitives and does
/// not invent plot data.
fn save_result_plots(result: &PipelineResult, output_dir: Option<&Path>) -> Result<usize, String> {
    let dir = plots_dir(output_dir);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create plot directory {}: {e}", dir.display()))?;

    // Construct through the public default state so the CLI does not need to
    // name the GUI's private window bookkeeping.  Use the shared completion
    // transition so GUI figure/export helpers do not treat this valid CLI
    // result as an in-flight snapshot.
    let mut state = alas_gui::AppState::default();
    state.set_completed_pipeline_result(result.clone());

    let mut written = 0;
    let mut manifest = Vec::with_capacity(alas_report::RESULT_FIGURES.len());
    for descriptor in alas_report::RESULT_FIGURES {
        let (status, path, reason) = match alas_gui::scene::build_result_figure(
            &state,
            descriptor.id,
            &result.config,
            "dark",
        ) {
            Some(Some(scene)) => {
                let svg = alas_pipeline::render_scene_svg(&scene)?;
                let path = dir.join(format!("{}.svg", descriptor.id));
                fs::write(&path, svg)
                    .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
                written += 1;
                ("written", Some(format!("{}.svg", descriptor.id)), None)
            }
            Some(None) => (
                "unavailable",
                None,
                Some("the registered figure has no usable data for this run".to_owned()),
            ),
            None => (
                "unavailable",
                None,
                Some("the pipeline result does not expose the required report stage".to_owned()),
            ),
        };
        manifest.push(PlotExportEntry {
            id: descriptor.id,
            title: descriptor.title,
            required_stage: format!("{:?}", descriptor.required_stage),
            status,
            path,
            reason,
        });
    }

    let manifest_path = dir.join("plot_manifest.json");
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize plot manifest: {error}"))?;
    fs::write(&manifest_path, manifest_json)
        .map_err(|error| format!("failed to write {}: {error}", manifest_path.display()))?;

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_cli_tool_preferences, load_config, parse_args, AerodynamicSolverMode, CliArgs,
        OptimizationSolverMode,
    };
    use alas_config::AlasConfig;
    use alas_exec::ToolPreferences;

    #[test]
    fn solver_flags_select_independent_analysis_and_optimization_backends() {
        let args = [
            "--aero-solver".to_owned(),
            "avl".to_owned(),
            "--optimization-solver".to_owned(),
            "both".to_owned(),
        ];
        let parsed = parse_args(&args)
            .unwrap_or_else(|error| panic!("solver flags parse: {error}"))
            .unwrap_or_else(|| panic!("solver flags do not request help"));
        assert_eq!(parsed.aerodynamic_solver, AerodynamicSolverMode::Avl);
        assert_eq!(parsed.optimization_solver, OptimizationSolverMode::Both);
    }

    #[test]
    fn solver_flags_reject_unknown_modes() {
        let args = ["--aero-solver".to_owned(), "panel".to_owned()];
        let error = parse_args(&args)
            .err()
            .unwrap_or_else(|| panic!("unknown solver mode must be rejected"));
        assert!(error.contains("unknown aerodynamic solver"), "{error}");
    }

    #[test]
    fn optimization_method_accepts_the_one_supported_search() {
        let method = "differential_evolution";
        let args = ["--optimization-method".to_owned(), method.to_owned()];
        let parsed = parse_args(&args)
            .unwrap_or_else(|error| panic!("optimization method parses: {error}"))
            .unwrap_or_else(|| panic!("optimization method does not request help"));
        assert_eq!(parsed.optimization_method.as_deref(), Some(method));
    }

    #[test]
    fn optimization_method_rejects_a_retired_legacy_token() {
        // Legacy tokens are migrated when a saved configuration document
        // loads (`alas_config::settings_load_notes`), not accepted as a
        // distinct CLI flag value; the CLI flag is validated against the
        // dispatch contract directly.
        for method in ["feasibility_first_de", "nsga2", "turbo_1", "cma_es", "sqp"] {
            let args = ["--optimization-method".to_owned(), method.to_owned()];
            let error = parse_args(&args)
                .err()
                .unwrap_or_else(|| panic!("{method} must be rejected as a CLI flag value"));
            assert!(error.contains("invalid optimization method"), "{error}");
        }
    }

    #[test]
    fn optimization_method_rejects_unknown_strategy() {
        let args = [
            "--optimization-method".to_owned(),
            "random_search".to_owned(),
        ];
        let error = parse_args(&args)
            .err()
            .unwrap_or_else(|| panic!("unknown optimization method must be rejected"));
        assert!(error.contains("invalid optimization method"), "{error}");
    }

    #[test]
    fn optimization_method_overrides_the_effective_configuration() {
        let args = CliArgs {
            optimization_method: Some("differential_evolution".to_owned()),
            ..CliArgs::default()
        };
        let config = load_config(&args)
            .unwrap_or_else(|error| panic!("effective configuration loads: {error}"));
        assert_eq!(config.optimizer.solver.method, "differential_evolution");
    }

    #[test]
    fn cli_applies_nastran_solver_preference_without_overriding_explicit_config() {
        let preferences = ToolPreferences {
            nastran_solver: Some("C:/MSC/analysis.exe".to_owned()),
            navdata_dir: Some("C:/ALAS/navdata".to_owned()),
            routes_dir: Some("C:/ALAS/routes".to_owned()),
            ..ToolPreferences::default()
        };

        let mut default_config = AlasConfig::default();
        apply_cli_tool_preferences(&mut default_config, &preferences, true);
        assert_eq!(
            default_config.structures.nastran_solver_path,
            "C:/MSC/analysis.exe"
        );
        assert_eq!(default_config.mission.navdata_dir, "C:/ALAS/navdata");
        assert_eq!(default_config.mission.routes_dir, "C:/ALAS/routes");

        let mut explicit_config = AlasConfig::default();
        explicit_config.structures.nastran_solver_path = "D:/project/analysis.exe".to_owned();
        apply_cli_tool_preferences(&mut explicit_config, &preferences, false);
        assert_eq!(
            explicit_config.structures.nastran_solver_path,
            "D:/project/analysis.exe"
        );
        assert_ne!(explicit_config.mission.navdata_dir, "C:/ALAS/navdata");
        assert_ne!(explicit_config.mission.routes_dir, "C:/ALAS/routes");
    }

    #[test]
    fn cpacs_input_path_is_parsed_without_changing_solver_defaults() {
        let args = [
            "--cpacs-input".to_owned(),
            "aircraft.cpacs.xml".to_owned(),
            "--no-optimize".to_owned(),
            "--no-baseline".to_owned(),
        ];
        let parsed = parse_args(&args)
            .unwrap_or_else(|error| panic!("CPACS input flags parse: {error}"))
            .unwrap_or_else(|| panic!("CPACS input flags do not request help"));
        assert_eq!(
            parsed.cpacs_input.as_deref(),
            Some(std::path::Path::new("aircraft.cpacs.xml"))
        );
        assert!(parsed.no_optimize);
        assert!(parsed.no_baseline);
        assert_eq!(parsed.aerodynamic_solver, AerodynamicSolverMode::Both);
    }

    #[test]
    fn navdata_download_action_is_parsed_as_a_non_pipeline_command() {
        let args = ["--download-navdata".to_owned()];
        let parsed = parse_args(&args)
            .unwrap_or_else(|error| panic!("navdata flag parses: {error}"))
            .unwrap_or_else(|| panic!("navdata flag does not request help"));
        assert!(parsed.download_navdata);
    }
}
