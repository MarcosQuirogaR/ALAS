// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Argument parsing, help text and configuration loading.

use std::fs;
use std::path::{Path, PathBuf};

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
    /// Show the external-tool install plan; install it when `--yes` is also given.
    pub install_tools: bool,
    /// Consent to the plan shown by `--install-tools` without a prompt.
    pub assume_yes: bool,
    /// Remove one installer-managed external tool by id and exit.
    pub uninstall_tool: Option<String>,
    /// Serve the Model Context Protocol (JSON-RPC 2.0) over stdin and stdout.
    pub mcp: bool,
    /// Folder the stdio server reads and writes in; default
    /// `<user data>/automation`.
    pub workspace: Option<PathBuf>,
    /// Honour program and tool folders named by `--config` (default: ignore
    /// them and use the tool preferences).
    pub trust_config_tool_paths: bool,
    /// Let this run request its route from the SimBrief service for
    /// `mission.simbrief_username` (default: no network request).
    pub live_dispatch: bool,
    /// Analyse a named aircraft preset instead of the default configuration.
    pub preset: Option<String>,
    /// Run only these physics stages (and the stages they need), comma
    /// separated: geometry, mass, aero, stability, field, mission,
    /// structures, feasibility (alias gate). See `alas_pipeline::partial`.
    pub only: Option<String>,
    /// Folder of the partial-run result cache (default: `<output>/.alas-cache`).
    pub cache_dir: Option<PathBuf>,
    /// Recompute every stage of a partial run and overwrite its cache entries.
    pub refresh_cache: bool,
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
            install_tools: false,
            assume_yes: false,
            uninstall_tool: None,
            mcp: false,
            workspace: None,
            trust_config_tool_paths: false,
            live_dispatch: false,
            preset: None,
            only: None,
            cache_dir: None,
            refresh_cache: false,
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
            "--install-tools" => cli.install_tools = true,
            "--yes" => cli.assume_yes = true,
            "--uninstall-tool" => {
                let val = flag_value(&mut iter, "--uninstall-tool")?;
                cli.uninstall_tool = Some(val.to_owned());
            }
            "--mcp" => cli.mcp = true,
            "--trust-config-tool-paths" => cli.trust_config_tool_paths = true,
            "--live-dispatch" => cli.live_dispatch = true,
            "--preset" => {
                let val = flag_value(&mut iter, "--preset")?;
                cli.preset = Some(val.to_owned());
            }
            "--only" => {
                let val = flag_value(&mut iter, "--only")?;
                cli.only = Some(val.to_owned());
            }
            "--cache-dir" => {
                let val = flag_value(&mut iter, "--cache-dir")?;
                cli.cache_dir = Some(PathBuf::from(val));
            }
            "--refresh-cache" => cli.refresh_cache = true,
            "--workspace" => {
                let val = flag_value(&mut iter, "--workspace")?;
                cli.workspace = Some(PathBuf::from(val));
            }
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option: {unknown}"));
            }
            // No option is positional; running on defaults would hide a typo.
            unexpected => return Err(format!("unexpected argument: {unexpected}")),
        }
    }

    if cli.preset.is_some() && cli.config.is_some() {
        return Err(
            "--preset and --config are exclusive: name the preset inside the config file"
                .to_owned(),
        );
    }
    if (cli.cache_dir.is_some() || cli.refresh_cache) && cli.only.is_none() {
        return Err("--cache-dir and --refresh-cache only apply with --only".to_owned());
    }
    if cli.workspace.is_some() && !cli.mcp {
        return Err("--workspace only applies with --mcp".to_owned());
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
    line("      --install-tools       Show the external-tool install plan; with --yes, download and install");
    line("      --yes                 Consent to the plan shown by --install-tools");
    line("      --uninstall-tool <ID> Remove one installer-managed external tool and exit");
    line("      --trust-config-tool-paths  Run the programs a --config file names (MSES, NASTRAN, Patran folders); without it they come from the tool preferences");
    line("      --live-dispatch       Fetch the route from SimBrief (www.simbrief.com) for the account in mission.simbrief_username; without it no request is made and --save-config leaves the account out");
    line("      --mcp                 Serve the Model Context Protocol (JSON-RPC 2.0) on stdin/stdout");
    line("      --workspace <DIR>     Folder --mcp reads and writes in (default: <user data>/automation)");
    line("      --preset <NAME>       Analyse a registered aircraft preset (e.g. A320-200, C295)");
    line("      --only <STAGES>       Run only these physics stages and what they need, comma separated:");
    line("                            geometry, mass, aero, stability, field, mission, structures, feasibility (gate);");
    line("                            prints a JSON summary and writes <output>/partial_run.json");
    line("      --cache-dir <DIR>     Partial-run cache folder (default: <output>/.alas-cache)");
    line("      --refresh-cache       Recompute every stage of a partial run and overwrite the cache");
    line("  -h, --help                Display this help message");
    line("");
    line("Exit codes: 0 accepted design delivered; 1 crash, usage or configuration error;");
    line(
        "            2 unknown --only stage; 3 run completed but no accepted design was delivered",
    );
    line("            (verdict NOT ACCEPTED or physical status INFEASIBLE; output files are written).");
    text
}

/// Read a configuration document as written: JSON for a `.json` file, YAML
/// (a superset) for anything else.
pub(crate) fn read_config_document(path: &Path) -> Result<serde_json::Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        serde_json::from_str(&content).map_err(|e| format!("failed to parse config JSON: {e}"))
    } else {
        serde_yaml::from_str(&content).map_err(|e| format!("failed to parse config YAML: {e}"))
    }
}

/// Load configuration from file (YAML/JSON) or defaults, applying CLI overrides.
pub fn load_config(args: &CliArgs) -> Result<AlasConfig, String> {
    let mut config = if let Some(ref path) = args.config {
        crate::config_load::load_config_value(&read_config_document(path)?)?
    } else if let Some(name) = args.preset.as_deref() {
        // An unknown name would otherwise fall back to the default aircraft.
        alas_config::presets::get(name).map_err(|_| {
            format!(
                "unknown preset `{name}`; available: {}",
                alas_config::presets::available().join(", ")
            )
        })?;
        crate::config_load::load_config_value(&serde_json::json!({ "preset": name }))?
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
pub(crate) fn apply_cli_tool_preferences(
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

/// Reset every program and solver-folder location of a configuration file to
/// its default, so the tool preferences decide which programs run. Returns the
/// names of the fields that differed from the default.
pub(crate) fn discard_config_tool_paths(config: &mut AlasConfig) -> Vec<&'static str> {
    let defaults = AlasConfig::default();
    let mut discarded = Vec::new();
    macro_rules! reset {
        ($name:literal, $($field:tt)+) => {
            if config.$($field)+ != defaults.$($field)+ {
                config.$($field)+ = defaults.$($field)+.clone();
                discarded.push($name);
            }
        };
    }
    reset!("mses.mses_dir", mses.mses_dir);
    reset!("structures.nastran_exe_path", structures.nastran_exe_path);
    reset!(
        "structures.nastran_solver_path",
        structures.nastran_solver_path
    );
    reset!(
        "structures.nastran95_dir_path",
        structures.nastran95_dir_path
    );
    reset!(
        "structures.nastran95_runtime_path",
        structures.nastran95_runtime_path
    );
    reset!(
        "structures.nastran95_rf_stage_path",
        structures.nastran95_rf_stage_path
    );
    reset!("structures.patran_exe_path", structures.patran_exe_path);
    discarded
}
