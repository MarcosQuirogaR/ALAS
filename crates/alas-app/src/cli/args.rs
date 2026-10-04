// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Argument parsing, help text and configuration loading.

use std::fs;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::ToolPreferences;
use alas_pipeline::{AerodynamicSolverMode, OptimizationSolverMode};

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
            "--quiet" => cli.quiet = true,
            "--save-config" => {
                let val = iter.next().ok_or("missing argument for --save-config")?;
                cli.save_config = Some(PathBuf::from(val));
            }
            "--download-navdata" => cli.download_navdata = true,
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option: {unknown}"));
            }
            // No option is positional; running on defaults would hide a typo.
            unexpected => return Err(format!("unexpected argument: {unexpected}")),
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
        crate::config_load::load_config_value(&value)?
    } else {
        AlasConfig::default()
    };

    if let Some(seed) = args.seed {
        config
            .optimizer
            .solver
            .set_seed(seed)
            .map_err(|error| error.to_string())?;
    }
    if args.no_mission {
        config.mission.enabled = false;
    }

    Ok(config)
}

/// Apply persisted machine-tool locations when no explicit config file owns
/// those settings. A project config must remain authoritative over machine
/// preferences, just as it is in the GUI's configuration flow.
pub(super) fn apply_cli_tool_preferences(
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
