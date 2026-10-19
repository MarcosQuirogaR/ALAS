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
    /// Also write the raw data behind each figure as CSV and JSON next to the
    /// SVG files (implies plots).
    pub plot_data: bool,
    /// Write the figures as SVG files to the output directory (implies plots).
    /// There is no interactive viewer; open the files in a browser or editor.
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
            plot_data: false,
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

/// The value of an option that takes one. A missing value, or a next token
/// that is itself an option, is an error: `--save-config --quiet` must not
/// write a file named `--quiet`.
fn flag_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<&'a String, String> {
    match iter.next() {
        Some(value) if !value.starts_with('-') => Ok(value),
        _ => Err(format!("missing argument for {flag}")),
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
                let val = flag_value(&mut iter, "--config")?;
                cli.config = Some(PathBuf::from(val));
            }
            "--cpacs-input" => {
                let val = flag_value(&mut iter, "--cpacs-input")?;
                cli.cpacs_input = Some(PathBuf::from(val));
            }
            "-o" | "--output" => {
                let val = flag_value(&mut iter, "--output")?;
                cli.output = PathBuf::from(val);
            }
            "--no-optimize" => cli.no_optimize = true,
            "--no-baseline" => cli.no_baseline = true,
            "--no-mission" => cli.no_mission = true,
            "--no-parallel" => cli.no_parallel = true,
            "--plots" => cli.plots = true,
            "--plot-data" => {
                cli.plot_data = true;
                cli.plots = true;
            }
            "--show" => {
                cli.show = true;
                cli.plots = true;
            }
            "--gui" => cli.gui = true,
            "--seed" => {
                let val = flag_value(&mut iter, "--seed")?;
                let s: u64 = val.parse().map_err(|_| "invalid seed integer")?;
                cli.seed = Some(s);
            }
            "--aero-solver" => {
                let val = flag_value(&mut iter, "--aero-solver")?;
                cli.aerodynamic_solver = val.parse()?;
            }
            "--optimization-solver" => {
                let val = flag_value(&mut iter, "--optimization-solver")?;
                cli.optimization_solver = val.parse()?;
            }
            "--quiet" => cli.quiet = true,
            "--save-config" => {
                let val = flag_value(&mut iter, "--save-config")?;
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
    print!("{}", help_text());
}

/// The help text; kept separate so a test can cross-check it against the flags.
pub(super) fn help_text() -> String {
    let mut text = String::new();
    let mut line = |line: &str| {
        text.push_str(line);
        text.push('\n');
    };
    line("Usage: alas [OPTIONS]");
    line("");
    line("Options:");
    line("  -c, --config <PATH>       Load configuration overlay from YAML or JSON");
    line("      --cpacs-input <PATH>  Use a CPACS 3.5 aircraft as the geometry input");
    line("  -o, --output <PATH>       Output directory for reports (default: outputs)");
    line("      --gui                 Launch the interactive desktop interface");
    line("      --no-optimize         Skip design space optimization");
    line("      --no-baseline         Skip baseline high-fidelity polar analysis");
    line("      --no-mission          Disable native mission analysis");
    line("      --no-parallel         Run stages sequentially");
    line("      --plots               Generate and save figures (SVG files)");
    line("      --plot-data           Also write each figure's raw data as CSV and JSON (implies --plots)");
    line("      --show                Same as --plots: writes SVG files to the output directory");
    line("      --seed <INT>          Random seed for optimization (default 20260922)");
    line("      --aero-solver <MODE>  Result model: vlm, avl, or both");
    line("      --optimization-solver <MODE>  Optimizer: vlm, avl, or both");
    line("      --quiet               Reduce console logging");
    line("      --save-config <PATH>  Write effective configuration to YAML and exit");
    line("      --download-navdata    Download missing navigation-data files and exit");
    line("  -h, --help                Display this help message");
    text
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
