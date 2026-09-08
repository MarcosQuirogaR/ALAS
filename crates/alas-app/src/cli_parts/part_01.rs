// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fs;
use std::path::{Path, PathBuf};

use alas_config::AlasConfig;
use alas_exec::download::{download_files, DownloadSpec};
use alas_exec::{ToolLocator, ToolPreferences};
use alas_pipeline::{
    read_cpacs_file, AerodynamicSolverMode, DesignPipeline, OptimizationSolverMode,
    PipelineOptions, PipelineResult,
};

/// Parsed command line flags and parameters for `alas`.
#[derive(Debug, Clone, PartialEq)]
pub struct CliArgs {
    /// Path to a configuration YAML/JSON file.
    pub config: Option<PathBuf>,
    /// CPACS 3.5 aircraft document used as the non-GUI geometry input.
    pub cpacs_input: Option<PathBuf>,
    /// Target directory for output files.
    pub output: PathBuf,
    /// Disable design space optimization (evaluate baseline design only).
    pub no_optimize: bool,
    /// Disable full high-fidelity analysis of the baseline design.
    pub no_baseline: bool,
    /// Disable native mission simulation.
    pub no_mission: bool,
    /// Run stages sequentially rather than in parallel.
    pub no_parallel: bool,
    /// Generate and save comparison plots.
    pub plots: bool,
    /// Interactively display plots (implies plots).
    pub show: bool,
    /// Launch the interactive desktop graphical user interface.
    pub gui: bool,
    /// Override optimizer random seed.
    pub seed: Option<u64>,
    /// Select the aerodynamic result family: vlm, avl, or both.
    pub aerodynamic_solver: AerodynamicSolverMode,
    /// Select the optimizer backend: vlm, avl, or both.
    pub optimization_solver: OptimizationSolverMode,
    /// Override the product optimization algorithm.
    pub optimization_method: Option<String>,
    /// Suppress verbose terminal outputs.
    pub quiet: bool,
    /// Write the effective configuration to this path and exit.
    pub save_config: Option<PathBuf>,
    /// Download any missing/truncated navigation-data files and exit.
    pub download_navdata: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            config: None,
            cpacs_input: None,
            output: PathBuf::from("outputs"),
            no_optimize: false,
            no_baseline: false,
            no_mission: false,
            no_parallel: false,
            plots: false,
            show: false,
            gui: false,
            seed: None,
            aerodynamic_solver: AerodynamicSolverMode::Both,
            optimization_solver: OptimizationSolverMode::Vlm,
            optimization_method: None,
            quiet: false,
            save_config: None,
            download_navdata: false,
        }
    }
}

/// Parse command line arguments into [`CliArgs`].
pub fn parse_args(args: &[String]) -> Result<Option<CliArgs>, String> {
    let mut cli = CliArgs::default();
    let mut iter = args.iter().peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-c" | "--config" => {
                let val = iter.next().ok_or("missing argument for --config")?;
                cli.config = Some(PathBuf::from(val));
            }
            "--cpacs-input" => {
                let val = iter.next().ok_or("missing argument for --cpacs-input")?;
                cli.cpacs_input = Some(PathBuf::from(val));
            }
            "-o" | "--output" => {
                let val = iter.next().ok_or("missing argument for --output")?;
                cli.output = PathBuf::from(val);
            }
            "--no-optimize" => cli.no_optimize = true,
            "--no-baseline" => cli.no_baseline = true,
            "--no-mission" => cli.no_mission = true,
            "--no-parallel" => cli.no_parallel = true,
            "--plots" => cli.plots = true,
            "--show" => {
                cli.show = true;
                cli.plots = true;
            }
            "--gui" => cli.gui = true,
            "--seed" => {
                let val = iter.next().ok_or("missing argument for --seed")?;
                let s: u64 = val.parse().map_err(|_| "invalid seed integer")?;
                cli.seed = Some(s);
            }
            "--aero-solver" => {
                let val = iter.next().ok_or("missing argument for --aero-solver")?;
                cli.aerodynamic_solver = val.parse()?;
            }
            "--optimization-solver" => {
                let val = iter
                    .next()
                    .ok_or("missing argument for --optimization-solver")?;
                cli.optimization_solver = val.parse()?;
            }
            "--optimization-method" => {
                let val = iter
                    .next()
                    .ok_or("missing argument for --optimization-method")?;
                let accepted = alas_config::OptionSource::OptimizerMethod
                    .options()
                    .unwrap_or(&[]);
                if !accepted.contains(&val.as_str()) {
                    return Err(format!(
                        "invalid optimization method '{}'; expected one of {}",
                        val,
                        accepted.join(", ")
                    ));
                }
                cli.optimization_method = Some(val.to_owned());
            }
            "--quiet" => cli.quiet = true,
            "--save-config" => {
                let val = iter.next().ok_or("missing argument for --save-config")?;
                cli.save_config = Some(PathBuf::from(val));
            }
            "--download-navdata" => cli.download_navdata = true,
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option: {unknown}"));
            }
            _ => {}
        }
    }

    Ok(Some(cli))
}

fn print_help() {
    println!("Usage: alas [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -c, --config <PATH>       Load configuration overlay from YAML or JSON");
    println!("      --cpacs-input <PATH>  Use a CPACS 3.5 aircraft as the geometry input");
    println!("  -o, --output <PATH>       Output directory for reports (default: outputs)");
    println!("      --gui                 Launch the interactive desktop interface");
    println!("      --no-optimize         Skip design space optimization");
    println!("      --no-baseline         Skip baseline high-fidelity polar analysis");
    println!("      --no-mission          Disable native mission analysis");
    println!("      --no-parallel         Run stages sequentially");
    println!("      --plots               Generate and save figures");
    println!("      --show                Display figures interactively");
    println!("      --seed <INT>          Random seed for optimization");
    println!("      --aero-solver <MODE>  Result model: vlm, avl, or both");
    println!("      --optimization-solver <MODE>  Optimizer: vlm, avl, or both");
    println!("      --optimization-method <METHOD>  differential_evolution, feasibility_first_de, nsga2, turbo_1, cma_es, or sqp");
    println!("      --quiet               Reduce console logging");
    println!("      --save-config <PATH>  Write effective configuration to YAML and exit");
    println!("      --download-navdata    Download missing navigation-data files and exit");
    println!("  -h, --help                Display this help message");
}

/// Load configuration from file (YAML/JSON) or defaults, applying CLI overrides.
pub fn load_config(args: &CliArgs) -> Result<AlasConfig, String> {
    let mut config = if let Some(ref path) = args.config {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
        let value: serde_json::Value = if path.extension().and_then(|s| s.to_str()) == Some("json")
        {
            serde_json::from_str(&content)
                .map_err(|e| format!("failed to parse config JSON: {e}"))?
        } else {
            serde_yaml::from_str(&content)
                .map_err(|e| format!("failed to parse config YAML: {e}"))?
        };
        AlasConfig::from_value(&value).map_err(|e| format!("invalid config structure: {e:?}"))?
    } else {
        AlasConfig::default()
    };

    if let Some(seed) = args.seed {
        config.optimizer.solver.seed = Some(seed as i64);
    }
    if let Some(method) = &args.optimization_method {
        config.optimizer.solver.method = method.clone();
    }
    if args.no_mission {
        config.mission.enabled = false;
    }

    Ok(config)
}

/// Apply persisted machine-tool locations when no explicit config file owns
/// those settings. A project config must remain authoritative over machine
/// preferences, just as it is in the GUI's configuration flow.
fn apply_cli_tool_preferences(
    config: &mut AlasConfig,
    preferences: &ToolPreferences,
    use_persisted_preferences: bool,
) {
    if !use_persisted_preferences {
        return;
    }

    if let Some(path) = &preferences.mses_dir {
        config.mses.mses_dir.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_exe {
        config.structures.nastran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_solver {
        config.structures.nastran_solver_path.clone_from(path);
    }
    if let Some(path) = &preferences.patran_exe {
        config.structures.patran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.navdata_dir {
        config.mission.navdata_dir.clone_from(path);
    }
    if let Some(path) = &preferences.routes_dir {
        config.mission.routes_dir.clone_from(path);
    }
}

/// Main application orchestration entry point.
pub fn run_cli(args: &[String]) -> i32 {
    let cli = match parse_args(args) {
        Ok(Some(c)) => c,
        Ok(None) => return 0,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    // If --gui or run without headless options, launch desktop GUI
    if cli.gui || (args.is_empty() && cli.config.is_none() && cli.save_config.is_none()) {
        if let Err(e) = alas_gui::run() {
            eprintln!("GUI Launch Error: {e}");
            return 1;
        }
        return 0;
    }

    let mut config = match load_config(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration Error: {e}");
            return 1;
        }
    };

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();
    let openvsp_dir = preferences.openvsp_dir.clone();
    let avl_exe = preferences.avl_exe.clone();
    apply_cli_tool_preferences(&mut config, &preferences, cli.config.is_none());

    if cli.download_navdata {
        let target = locator.resolve_data_path(Path::new(&config.mission.navdata_dir));
        let specs = alas_route::assets::NAVDATA_FILES
            .iter()
            .map(|file| {
                DownloadSpec::new(
                    file.name,
                    alas_route::assets::navdata_file_url(file),
                    file.min_bytes,
                )
            })
            .collect::<Vec<_>>();
        match download_files(&specs, &target, 120.0) {
            Ok(summary) => {
                println!(
                    "Navigation data ready at {} (downloaded {}, skipped {}).",
                    summary.target_dir.display(),
                    summary.downloaded.len(),
                    summary.skipped.len()
                );
                return 0;
            }
            Err(error) => {
                eprintln!("Navigation-data download failed: {error}");
                return 1;
            }
        }
    }

    if let Some(ref save_path) = cli.save_config {
        let yaml = match serde_yaml::to_string(&config) {
            Ok(y) => y,
            Err(e) => {
                eprintln!("Serialization Error: {e}");
                return 1;
            }
        };
        if let Err(e) = fs::write(save_path, yaml) {
            eprintln!("Failed to write config file {}: {e}", save_path.display());
            return 1;
        }
        println!("Effective configuration written to {}", save_path.display());
        return 0;
    }

    let pipeline = if let Some(path) = cli.cpacs_input.as_ref() {
        if !cli.no_optimize || !cli.no_baseline {
            eprintln!("CPACS input currently requires both --no-optimize and --no-baseline");
            return 1;
        }
        let document = match read_cpacs_file(path) {
            Ok(document) => document,
            Err(error) => {
                eprintln!("CPACS Input Error: {error}");
                return 1;
            }
        };
        match DesignPipeline::new_with_cpacs_document(config.clone(), document) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                eprintln!("CPACS Input Error: {error}");
                return 1;
            }
        }
    } else {
        DesignPipeline::new(config.clone())
    };
    let options = PipelineOptions {
        optimize: !cli.no_optimize,
        compare_baseline: !cli.no_baseline,
        parallel: !cli.no_parallel,
        aerodynamic_solver: cli.aerodynamic_solver,
        optimization_solver: cli.optimization_solver,
        output_dir: Some(cli.output.clone()),
        save_plots: cli.plots || cli.show,
        seed: cli.seed,
        quiet: cli.quiet,
    };

    let environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(openvsp_dir.as_deref().unwrap_or("")),
        Path::new(avl_exe.as_deref().unwrap_or("")),
    );

    let result = match pipeline.run_with_environment(&options, &environment) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Pipeline Execution Error: {e}");
            return 1;
        }
    };

    if options.save_plots {
        match save_result_plots(&result, options.output_dir.as_deref()) {
            Ok(count) if !cli.quiet => {
                println!(
                    "Saved {count} SVG plot(s) to {}.",
                    plots_dir(options.output_dir.as_deref()).display()
                );
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Plot Export Error: {e}");
                return 1;
            }
        }
    }

    if !cli.quiet {
        if let Some(ref rep) = result.optimized_report {
            println!(
                "\n{}\n{}",
                alas_pipeline::format_summary(rep, Some(&result.config)),
                alas_pipeline::format_feasibility(&result.feasibility)
            );
        }
    }

    print_mses_summary(&result, cli.quiet);
    print_structural_summary(&result, cli.quiet);
    print_cpacs_summary(&result, cli.quiet);

    0
}

fn print_cpacs_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    if let Some(export) = &result.cpacs_export {
        println!(
            "\n--- CPACS aircraft: {} (v{}) ---",
            export.path.display(),
            export.cpacs_version
        );
    }
}

fn print_mses_summary(result: &PipelineResult, quiet: bool) {
    if quiet {
        return;
    }
    let mses = match result.mses_result {
        Some(ref m) => m,
        None => return,
    };
    if !mses.has_usable_data() {
        println!("\n--- MSES analysis: {} ---", mses.status.as_str());
        if let Some(ref err) = mses.error {
            println!("  {err}");
        }
        return;
    }

    println!("\n--- MSES 2-D polar analysis ---");
    println!("  airfoil          : {}", mses.airfoil_name);
    println!(
        "  Mach / Re        : {:.3} / {:.3e}",
        mses.mach, mses.reynolds
    );
    println!("  converged points : {}", mses.alpha_deg.len());
    if !mses.is_complete() {
        println!(
            "  sweep status     : {} of {} requested points converged",
            mses.converged_alpha_count, mses.requested_alpha_count
        );
        let nonconverged = mses.nonconverged_alpha_deg();
        if !nonconverged.is_empty() {
            let values = nonconverged
                .iter()
                .map(|alpha| format!("{alpha:.4}"))
                .collect::<Vec<_>>()
                .join(", ");
            println!("  not converged    : {values} deg (solver transcripts retained)");
        }
    }
    if let Some(ref pressure) = result.mses_pressure {
        if pressure.status.as_str() != "ok" {
            println!(
                "  pressure distribution: {} ({:?})",
                pressure.status.as_str(),
                pressure.error
            );
        }
    }
}
