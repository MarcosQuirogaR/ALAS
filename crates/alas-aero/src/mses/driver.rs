// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/aerodynamics/aero_2D/mses.py (MSES.run) and
// alas/physics/mses_analysis.py (run_mses_polar, run_mses_pressure_distribution),
// scoped to the polar-sweep and single-point pressure paths this program drives.
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The [`Mses`] driver: mesh once, solve each angle in turn, read the result.
//!
//! This reproduces native aerodynamic model's `MSES.run` loop -- generate a mesh with
//! `mset` at the first angle, then for each angle write the `mses.case` deck,
//! run `mses`, and if it converged dump the summary with `mplot` and parse it.
//! A non-converged angle reinitializes the mesh at the next angle and moves on,
//! upstream's `behavior_after_unconverged_run="reinitialize"` default. The
//! solve continues from the previous converged state within one sweep (the
//! working directory keeps the flow solution between angles), which is why a
//! swept angle and the same angle solved alone can differ -- and why the driver
//! must keep one working directory for the whole sweep.
//!
//! native aerodynamic model's `run` ends by popping `"Ma"` out of its accumulated results,
//! which raises if nothing converged; this driver has no such step, so an
//! all-non-converged sweep surfaces as an empty accumulator instead, which the
//! entry points turn into the same "did not converge" outcome upstream produces
//! from that raise.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use alas_config::MsesConfig;
use alas_geom::aircraft::airfoil::Airfoil;

use super::exec::{self, RunError, WorkDir};
use super::{
    deck, parse, MsesPolarPointDiagnostic, MsesPolarPointStatus, MsesPolarResult,
    MsesPressureResult, MsesStatus,
};

/// The `mplot` timeout for the polar summary dump, native aerodynamic model's `timeout_mplot`
/// default (`mses_analysis.py` overrides only the `mset` and `mses` timeouts).
const TIMEOUT_MPLOT_S: f64 = 10.0;

#[derive(Debug)]
struct MsesFailure {
    status: MsesStatus,
    message: String,
}

#[derive(Debug, Default)]
struct SweepOutcome {
    accumulated: HashMap<String, Vec<f64>>,
    point_diagnostics: Vec<MsesPolarPointDiagnostic>,
}

impl MsesFailure {
    fn solver(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::Error,
            message: message.into(),
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::ParseFailure,
            message: message.into(),
        }
    }

    fn from_run(error: RunError) -> Self {
        let message = error.to_string();
        let status = match &error {
            RunError::Spawn { .. } => MsesStatus::LaunchFailure,
            RunError::Timeout { .. } => MsesStatus::Timeout,
            RunError::Io { .. } | RunError::Reader { .. } | RunError::InvalidTimeout { .. } => {
                MsesStatus::Error
            }
        };
        Self { status, message }
    }
}

/// A configured MSES run on one already-repaneled section.
///
/// Holds the section, the solver settings read off [`MsesConfig`], and the
/// three executable paths. The section is taken already repaneled, as
/// native aerodynamic model's wrapper is constructed with `airfoil.repanel(...)`: the entry
/// points [`super::run_mses_polar`]/[`super::run_mses_pressure_distribution`]
/// repanel and build this, and the parity test builds it directly from the
/// coordinates the reference actually fed to `mset`.
pub struct Mses {
    airfoil: Airfoil,
    airfoil_name: String,
    n_crit: f64,
    xtr_upper: f64,
    xtr_lower: f64,
    max_iterations: i64,
    mset_n: i64,
    mset_e: f64,
    timeout_mset_s: f64,
    timeout_mses_s: f64,
    enabled: bool,
    mses_dir: PathBuf,
    mset_exe: PathBuf,
    mses_exe: PathBuf,
    mplot_exe: PathBuf,
}

impl Mses {
    /// Build a driver from an already-repaneled section and its settings.
    pub fn new(airfoil: Airfoil, config: &MsesConfig, mses_dir: &Path) -> Self {
        let airfoil_name = if airfoil.name.is_empty() {
            "optimized_root_section".to_owned()
        } else {
            airfoil.name.clone()
        };
        Self {
            airfoil,
            airfoil_name,
            n_crit: config.n_crit,
            xtr_upper: config.xtr_upper,
            xtr_lower: config.xtr_lower,
            max_iterations: config.max_iterations,
            mset_n: config.mset_n,
            mset_e: config.mset_e,
            timeout_mset_s: config.timeout_mset_s,
            timeout_mses_s: config.timeout_mses_s,
            enabled: config.enabled,
            mses_dir: mses_dir.to_path_buf(),
            mset_exe: mses_dir.join("mset.exe"),
            mses_exe: mses_dir.join("mses.exe"),
            mplot_exe: mses_dir.join("mplot.exe"),
        }
    }

    /// Run an alpha sweep and return the parsed polar.
    ///
    /// Never returns an error: any failure is reported through the result's
    /// `status`/`error`, as `run_mses_polar` does, so a pipeline run does not
    /// abort because one section would not converge.
    pub fn polar(&self, alphas: &[f64], reynolds: f64, mach: f64) -> MsesPolarResult {
        let mut result = MsesPolarResult {
            airfoil_name: self.airfoil_name.clone(),
            mach,
            reynolds,
            requested_alpha_count: alphas.len(),
            ..MsesPolarResult::default()
        };
        if !self.enabled {
            return result.into_failure(
                MsesStatus::Disabled,
                "MSES analysis is disabled in configuration".to_owned(),
            );
        }
        if let Some(status) = super::installation_status(&self.mses_dir) {
            return result.into_failure(status, installation_message(&self.mses_dir, status));
        }

        let workdir = match WorkDir::new("alas_mses_") {
            Ok(dir) => dir,
            Err(error) => return result.into_error(error.to_string()),
        };
        let outcome = match self.solve_sweep(workdir.path(), alphas, reynolds, mach) {
            Ok(outcome) => outcome,
            Err(failure) => return result.into_failure(failure.status, failure.message),
        };
        result.point_diagnostics = outcome.point_diagnostics;

        let converged = outcome.accumulated.get("alpha").map_or(0, Vec::len);
        result.converged_alpha_count = converged;
        if converged == 0 {
            return result.into_error("MSES did not converge at any swept alpha".to_owned());
        }
        let [alpha_deg, cl, cd, cm, cdv, cdw, xtr_top, xtr_bot] =
            match validated_polar_columns(&outcome.accumulated, converged) {
                Ok(columns) => columns,
                Err(error) => return result.into_failure(MsesStatus::ParseFailure, error),
            };
        result.alpha_deg = alpha_deg;
        result.cl = cl;
        result.cd = cd;
        result.cm = cm;
        result.cdv = cdv;
        result.cdw = cdw;
        result.xtr_top = xtr_top;
        result.xtr_bot = xtr_bot;
        result.status = polar_completion_status(alphas.len(), converged);
        if result.status == MsesStatus::PartialConvergence {
            result.error = Some(format!(
                "MSES converged at {converged} of {} requested alpha points",
                alphas.len()
            ));
        }
        result
    }

    /// Solve one point and read back its surface pressure and Mach distribution.
    ///
    /// Never returns an error, for the same reason [`Mses::polar`] does not.
    pub fn pressure(
        &self,
        alpha_deg: f64,
        reynolds: f64,
        mach: f64,
        retry_offsets: &[f64],
    ) -> MsesPressureResult {
        let mut result = MsesPressureResult {
            alpha_deg,
            ..MsesPressureResult::default()
        };
        if !self.enabled {
            return result.into_failure(
                MsesStatus::Disabled,
                "MSES analysis is disabled in configuration".to_owned(),
            );
        }
        if let Some(status) = super::installation_status(&self.mses_dir) {
            return result.into_failure(status, installation_message(&self.mses_dir, status));
        }

        let mut attempt_failures: Vec<MsesFailure> = Vec::new();
        let mut winner: Option<(WorkDir, f64)> = None;
        for &offset in retry_offsets {
            let candidate = alpha_deg + offset;
            let workdir = match WorkDir::new("alas_mses_cp_") {
                Ok(dir) => dir,
                Err(error) => return result.into_error(error.to_string()),
            };
            match self.solve_sweep(workdir.path(), &[candidate], reynolds, mach) {
                Ok(outcome) if outcome.accumulated.get("alpha").map_or(0, Vec::len) > 0 => {
                    winner = Some((workdir, candidate));
                    break;
                }
                Ok(_) => attempt_failures.push(MsesFailure::solver(format!(
                    "alpha={candidate:.2}: did not converge"
                ))),
                Err(mut failure) => {
                    failure.message = format!("alpha={candidate:.2}: {}", failure.message);
                    attempt_failures.push(failure);
                }
            }
        }

        let (workdir, converged_alpha) = match winner {
            Some(pair) => pair,
            None => {
                let tried = retry_offsets
                    .iter()
                    .map(|offset| format!("{:.2}", alpha_deg + offset))
                    .collect::<Vec<_>>()
                    .join(", ");
                let last = attempt_failures
                    .last()
                    .map(|failure| failure.message.clone())
                    .unwrap_or_else(|| "unknown".to_owned());
                let status = attempt_failures
                    .last()
                    .map_or(MsesStatus::Error, |failure| failure.status);
                return result.into_failure(
                    status,
                    format!(
                        "MSES did not converge at alpha={alpha_deg:.2} deg or any retry offset \
                     (tried: {tried} deg). Last error: {last}"
                    ),
                );
            }
        };
        result.alpha_deg = converged_alpha;

        let dump_name = "bl_dump.txt";
        let _dump = match exec::run_tool(
            &self.mplot_exe,
            &["case"],
            workdir.path(),
            &deck::mplot_dump_keystrokes(12, dump_name),
            self.timeout_mses_s,
        ) {
            Ok(run) if run.status.success() => run,
            Ok(run) => {
                return result.into_error(format!("mplot exited with {}", run.status));
            }
            Err(error) => {
                let failure = MsesFailure::from_run(error);
                return result.into_failure(failure.status, failure.message);
            }
        };
        let dump_path = workdir.path().join(dump_name);
        if !dump_path.exists() {
            return result.into_failure(
                MsesStatus::ParseFailure,
                "mplot did not produce a BL dump file".to_owned(),
            );
        }

        let flowfield_name = "flowfield.txt";
        let _flowfield = match exec::run_tool(
            &self.mplot_exe,
            &["case"],
            workdir.path(),
            &deck::mplot_dump_keystrokes(11, flowfield_name),
            self.timeout_mses_s,
        ) {
            // Python does not check option 11's return code: the flow field is
            // an optional figure export, while the option-12 surface result is
            // already a valid converged analysis.
            Ok(run) => run,
            Err(error) => {
                let failure = MsesFailure::from_run(error);
                return result.into_failure(failure.status, failure.message);
            }
        };
        let flowfield_path = workdir.path().join(flowfield_name);

        let dump_text = read_lossy(&dump_path);
        let flowfield_text = if flowfield_path.exists() {
            read_lossy(&flowfield_path)
        } else {
            String::new()
        };
        match MsesPressureResult::replay_raw_exports(
            converged_alpha,
            dump_text.clone(),
            flowfield_text.clone(),
            &self.airfoil.coordinates,
        ) {
            Ok(parsed) => parsed,
            Err(error) => {
                result.raw_bl_dump = dump_text;
                result.raw_flowfield_dump = flowfield_text;
                result.into_failure(MsesStatus::ParseFailure, error.to_string())
            }
        }
    }

    /// Mesh once at the first angle, solve each in turn, accumulate the parsed
    /// summary of every angle that converged.
    fn solve_sweep(
        &self,
        dir: &Path,
        alphas: &[f64],
        reynolds: f64,
        mach: f64,
    ) -> Result<SweepOutcome, MsesFailure> {
        std::fs::write(dir.join("airfoil.dat"), self.airfoil.write_dat())
            .map_err(|error| MsesFailure::solver(error.to_string()))?;
        let first = *alphas
            .first()
            .ok_or_else(|| MsesFailure::solver("no angles to sweep"))?;
        self.run_mset(dir, first)?;

        let mut outcome = SweepOutcome::default();
        let mut index = 0;
        while index < alphas.len() {
            let alpha = alphas[index];
            std::fs::write(
                dir.join("mses.case"),
                deck::mses_case(
                    mach,
                    alpha,
                    reynolds,
                    self.n_crit,
                    self.xtr_lower,
                    self.xtr_upper,
                ),
            )
            .map_err(|error| MsesFailure::solver(error.to_string()))?;

            let solve = exec::run_tool(
                &self.mses_exe,
                &["case"],
                dir,
                &deck::mses_keystrokes(self.max_iterations),
                self.timeout_mses_s,
            )
            .map_err(MsesFailure::from_run)?;
            if !solve.status.success() {
                return Err(MsesFailure::solver(format!(
                    "mses exited with {}",
                    solve.status
                )));
            }

            let point_status = if parse::is_converged(&solve.stdout) {
                MsesPolarPointStatus::Converged
            } else {
                MsesPolarPointStatus::NotConverged
            };
            outcome.point_diagnostics.push(MsesPolarPointDiagnostic {
                requested_alpha_deg: alpha,
                status: point_status,
                solver_output: solve.stdout.clone(),
            });

            if point_status == MsesPolarPointStatus::NotConverged {
                match alphas.get(index + 1) {
                    Some(&next) => self.run_mset(dir, next)?,
                    None => break,
                }
                index += 1;
                continue;
            }

            let plot = exec::run_tool(
                &self.mplot_exe,
                &["case"],
                dir,
                deck::MPLOT_POLAR_KEYSTROKES,
                TIMEOUT_MPLOT_S,
            )
            .map_err(MsesFailure::from_run)?;
            if !plot.status.success() {
                return Err(MsesFailure::solver(format!(
                    "mplot exited with {}",
                    plot.status
                )));
            }
            let summary = parse::parse_polar_summary(&plot.stdout).map_err(MsesFailure::parse)?;
            for (key, value) in summary {
                outcome.accumulated.entry(key).or_default().push(value);
            }
            index += 1;
        }

        Ok(outcome)
    }

    /// Generate the mesh at `mset_alpha`.
    fn run_mset(&self, dir: &Path, mset_alpha: f64) -> Result<(), MsesFailure> {
        let run = exec::run_tool(
            &self.mset_exe,
            &["airfoil.dat"],
            dir,
            &deck::mset_keystrokes(self.mset_n, self.mset_e, mset_alpha),
            self.timeout_mset_s,
        )
        .map_err(MsesFailure::from_run)?;
        if !run.status.success() {
            return Err(MsesFailure::solver(format!(
                "mset exited with {}",
                run.status
            )));
        }
        Ok(())
    }
}

/// Validate the accumulated MPlot polar schema before publishing it.
///
/// Each converged MPlot invocation must provide every public polar column.
/// Checking both length and finiteness here keeps this boundary defensive if
/// aggregation changes later, and prevents a missing/ragged column from being
/// silently replaced with fabricated zeros.
fn validated_polar_columns(
    accumulated: &HashMap<String, Vec<f64>>,
    expected_len: usize,
) -> Result<[Vec<f64>; 8], String> {
    let mut columns = Vec::with_capacity(parse::POLAR_REQUIRED_COLUMNS.len());
    for &key in &parse::POLAR_REQUIRED_COLUMNS {
        let Some(values) = accumulated.get(key) else {
            return Err(format!(
                "mplot polar output missing required column '{key}'"
            ));
        };
        if values.len() != expected_len {
            return Err(format!(
                "mplot polar column '{key}' has {} values for {expected_len} alpha points",
                values.len()
            ));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "mplot polar column '{key}' is non-finite or malformed"
            ));
        }
        columns.push(values.clone());
    }
    columns
        .try_into()
        .map_err(|_| "internal MPlot polar schema length mismatch".to_owned())
}

fn polar_completion_status(requested: usize, converged: usize) -> MsesStatus {
    if requested > 0 && converged == requested {
        MsesStatus::Ok
    } else if converged > 0 {
        MsesStatus::PartialConvergence
    } else {
        MsesStatus::Error
    }
}

fn installation_message(directory: &Path, status: MsesStatus) -> String {
    match status {
        MsesStatus::Absent => format!("MSES installation was not found at {}", directory.display()),
        MsesStatus::Incomplete => format!(
            "MSES installation at {} is incomplete; required mset.exe, mses.exe, and mplot.exe",
            directory.display()
        ),
        _ => format!(
            "MSES installation at {} is unavailable",
            directory.display()
        ),
    }
}

/// Read a file as text, replacing invalid bytes rather than failing -- the
/// counterpart of upstream's `open(..., errors="replace")`. The dump files are
/// ASCII, so the replacement never fires; it is faithfulness, not a fix.
fn read_lossy(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

// Test fixtures use `expect`/`expect_err` so malformed cases fail at the
// assertion site; this allowance is intentionally scoped to the test module.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_sweep_is_not_reported_as_ok() {
        assert_eq!(
            polar_completion_status(7, 4),
            MsesStatus::PartialConvergence
        );
    }

    #[test]
    fn complete_and_empty_sweeps_have_distinct_statuses() {
        assert_eq!(polar_completion_status(7, 7), MsesStatus::Ok);
        assert_eq!(polar_completion_status(7, 0), MsesStatus::Error);
    }

    #[test]
    fn subprocess_timeout_becomes_a_timeout_status() {
        let failure = MsesFailure::from_run(RunError::Timeout {
            command: "mset.exe".to_owned(),
            seconds: 1.0,
        });
        assert_eq!(failure.status, MsesStatus::Timeout);
    }

    #[test]
    fn subprocess_spawn_failure_becomes_a_launch_failure_status() {
        let failure = MsesFailure::from_run(RunError::Spawn {
            command: "mset.exe".to_owned(),
            source: std::io::Error::other("test launch failure"),
        });
        assert_eq!(failure.status, MsesStatus::LaunchFailure);
    }

    #[test]
    fn malformed_output_becomes_a_parse_failure_status() {
        assert_eq!(
            MsesFailure::parse("bad output").status,
            MsesStatus::ParseFailure
        );
    }

    fn complete_accumulated_polar() -> HashMap<String, Vec<f64>> {
        parse::POLAR_REQUIRED_COLUMNS
            .into_iter()
            .map(|key| {
                let values = if key == "CDw" {
                    vec![0.0, 0.0]
                } else {
                    vec![1.0, 2.0]
                };
                (key.to_owned(), values)
            })
            .collect()
    }

    #[test]
    fn validated_polar_columns_keep_zero_wave_drag() {
        let accumulated = complete_accumulated_polar();
        let columns = validated_polar_columns(&accumulated, 2).expect("complete polar");
        assert_eq!(columns[0], vec![1.0, 2.0]);
        assert_eq!(columns[5], vec![0.0, 0.0]);
    }

    #[test]
    fn validated_polar_columns_reject_ragged_required_data() {
        let mut accumulated = complete_accumulated_polar();
        accumulated.insert("CD".to_owned(), vec![0.03]);
        let error = validated_polar_columns(&accumulated, 2)
            .expect_err("a ragged coefficient column must be rejected");
        assert_eq!(
            error,
            "mplot polar column 'CD' has 1 values for 2 alpha points"
        );
    }

    #[test]
    fn validated_polar_columns_reject_nonfinite_required_data() {
        let mut accumulated = complete_accumulated_polar();
        accumulated.insert("CL".to_owned(), vec![1.0, f64::NAN]);
        let error = validated_polar_columns(&accumulated, 2)
            .expect_err("a non-finite coefficient column must be rejected");
        assert_eq!(error, "mplot polar column 'CL' is non-finite or malformed");
    }
}
