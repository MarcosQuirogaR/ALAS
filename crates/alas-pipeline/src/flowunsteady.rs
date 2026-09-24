// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optional FLOWUnsteady adapter orchestration and comparability contract.

use std::fs;
use std::path::{Path, PathBuf};

use alas_aero::flowunsteady::{
    parse_result, render_request, FlowUnsteadyControlSurface, FlowUnsteadyFlightCondition,
    FlowUnsteadyPolar, FlowUnsteadyRequest, FlowUnsteadySection, FlowUnsteadySolverRequest,
    FlowUnsteadySurface,
};
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_exec::flowunsteady::{run_flowunsteady_adapter, FlowUnsteadyProcessStatus};
use alas_geom::airfoil_library::AirfoilLibrary;

use crate::full_analysis::AnalysisReport;

/// Product-level FLOWUnsteady outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowUnsteadyAnalysisStatus {
    /// No explicit adapter executable was configured.
    NotConfigured,
    /// Rust could not render or retain a valid request.
    RequestRejected,
    /// The configured timeout was not a finite number of seconds greater than
    /// zero; the solver was not launched.
    InvalidTimeout,
    /// Adapter executable could not launch.
    LaunchFailed,
    /// Adapter exceeded its deadline.
    TimedOut,
    /// Adapter returned a non-zero process status.
    SolverFailed,
    /// Adapter did not produce a fresh result.
    OutputMissing,
    /// Adapter result violated the strict interchange contract.
    ParseFailed,
    /// Native data exists but is not permitted in an overlay.
    CompletedNotComparable,
    /// Native data shares declared references, frames, and schedule.
    CompletedComparable,
}
impl FlowUnsteadyAnalysisStatus {
    /// Stable runtime-evidence spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::RequestRejected => "request_rejected",
            Self::InvalidTimeout => "invalid_timeout",
            Self::LaunchFailed => "launch_failed",
            Self::TimedOut => "timed_out",
            Self::SolverFailed => "solver_failed",
            Self::OutputMissing => "output_missing",
            Self::ParseFailed => "parse_failed",
            Self::CompletedNotComparable => "completed_not_comparable",
            Self::CompletedComparable => "completed_comparable",
        }
    }
}

/// Retained files, parsed data, and a physical comparison verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyAnalysisResult {
    /// Product-level stage outcome.
    pub status: FlowUnsteadyAnalysisStatus,
    /// Adapter executable used for an attempted invocation.
    pub runtime_executable: Option<PathBuf>,
    /// Retained SI request.
    pub request_path: PathBuf,
    /// Retained adapter result.
    pub result_path: PathBuf,
    /// Captured adapter standard output.
    pub stdout_path: PathBuf,
    /// Captured adapter standard error.
    pub stderr_path: PathBuf,
    /// Strictly parsed native samples, even if uncomparable.
    pub polar: Option<FlowUnsteadyPolar>,
    /// True only if every reference/frame/schedule comparison succeeded.
    pub comparable: bool,
    /// Actionable runtime or comparability detail.
    pub error: Option<String>,
}

/// Build the V2 SI request from the same optimized geometry and clean cruise
/// condition that generated the ALAS final polar. Controls are included with a
/// zero command and an explicit `applied_to_geometry=false` provenance because
/// the current ALAS lifting mesh does not yet model deflectable surfaces.
pub fn request_from_report(report: &AnalysisReport, config: &AlasConfig) -> FlowUnsteadyRequest {
    let atmosphere = Atmosphere::isa(config.requirements.cruise_altitude_m);
    let flight_condition = FlowUnsteadyFlightCondition {
        altitude_m: config.requirements.cruise_altitude_m,
        pressure_pa: atmosphere.pressure(),
        temperature_k: atmosphere.temperature(),
        density_kg_m3: atmosphere.density(),
        speed_of_sound_m_s: atmosphere.speed_of_sound(),
        true_airspeed_m_s: config.requirements.cruise_mach * atmosphere.speed_of_sound(),
        mach: config.requirements.cruise_mach,
        beta_deg: 0.0,
        angular_rates_rad_s: [0.0, 0.0, 0.0],
    };
    let lifting_surfaces = report
        .airplane
        .wings
        .iter()
        .enumerate()
        .map(|(surface_index, wing)| FlowUnsteadySurface {
            name: wing.name.clone(),
            symmetric_about_xz: wing.symmetric,
            sections: wing
                .xsecs
                .iter()
                .enumerate()
                .map(|(section_index, section)| FlowUnsteadySection {
                    leading_edge_m: section.xyz_le,
                    chord_m: section.chord,
                    twist_deg: section.twist,
                    // FLOW request fields are comma-delimited.  Blended
                    // sections legitimately carry names such as
                    // "12% morphed, 88% sc20410" after the CPACS round trip;
                    // serialize a stable delimiter-safe label while keeping
                    // the exact section coordinates below.
                    airfoil_name: flow_airfoil_name(
                        &section.airfoil.name,
                        surface_index,
                        section_index,
                    ),
                    // CPACS and user-imported geometry may preserve an
                    // airfoil name while omitting the contour.  Resolve that
                    // name through the same library used by the geometry
                    // builder before rejecting the adapter request; an
                    // unknown name still yields an honest request rejection.
                    airfoil_coordinates: if section.airfoil.coordinates.len() >= 3 {
                        section.airfoil.coordinates.clone()
                    } else {
                        AirfoilLibrary::get(&section.airfoil.name)
                            .map(|airfoil| airfoil.coordinates)
                            .unwrap_or_default()
                    },
                })
                .collect(),
        })
        .collect();
    let controls = controls_from_config(config);
    FlowUnsteadyRequest {
        area_m2: report.airplane.s_ref,
        chord_m: report.airplane.c_ref,
        span_m: report.airplane.b_ref,
        moment_reference_m: report.airplane.xyz_ref,
        lifting_surfaces,
        controls,
        flight_condition,
        solver: FlowUnsteadySolverRequest {
            model: "unsteady_vortex_lattice",
            steps_per_reference_chord: 20,
            wake_age_reference_chords: 20.0,
            settling_reference_chords: 10.0,
            averaging_reference_chords: 5.0,
        },
        alpha_deg: report.polar.alpha_deg.clone(),
    }
}

fn flow_airfoil_name(name: &str, surface_index: usize, section_index: usize) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            ',' | '\n' | '\r' | '=' => '_',
            _ => character,
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        format!("section_{surface_index}_{section_index}")
    } else {
        sanitized
    }
}

fn controls_from_config(config: &AlasConfig) -> Vec<FlowUnsteadyControlSurface> {
    let control = &config.control_surfaces;
    [
        (
            "slat",
            "Main Wing",
            "leading",
            control.slat_chord_fraction,
            control.slat_span_start_frac,
            control.slat_span_end_frac,
        ),
        (
            "flap",
            "Main Wing",
            "trailing",
            control.flap_chord_fraction,
            control.flap_span_start_frac,
            control.flap_span_end_frac,
        ),
        (
            "aileron",
            "Main Wing",
            "trailing",
            control.aileron_chord_fraction,
            control.aileron_span_start_frac,
            control.aileron_span_end_frac,
        ),
        (
            "spoiler",
            "Main Wing",
            "upper",
            control.spoiler_chord_fraction,
            control.spoiler_span_start_frac,
            control.spoiler_span_end_frac,
        ),
        (
            "elevator",
            "Horizontal Stabilizer",
            "trailing",
            control.elevator_chord_fraction,
            control.elevator_span_start_frac,
            control.elevator_span_end_frac,
        ),
        (
            "rudder",
            "Vertical Stabilizer",
            "trailing",
            control.rudder_chord_fraction,
            control.rudder_span_start_frac,
            control.rudder_span_end_frac,
        ),
    ]
    .into_iter()
    .map(
        |(role, surface_name, edge, chord_fraction, span_start_fraction, span_end_fraction)| {
            FlowUnsteadyControlSurface {
                role,
                surface_name,
                edge,
                chord_fraction,
                span_start_fraction,
                span_end_fraction,
                deflection_deg: 0.0,
                applied_to_geometry: false,
            }
        },
    )
    .collect()
}

/// Export, run, parse, and admit only identically-defined coefficients.
pub fn run_flowunsteady_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
) -> FlowUnsteadyAnalysisResult {
    let request_path = output_dir.join("flowunsteady/optimized_aircraft.request.txt");
    let fallback = |status, error: Option<String>| FlowUnsteadyAnalysisResult {
        status,
        runtime_executable: executable.map(Path::to_path_buf),
        result_path: request_path.with_extension("flowunsteady.result.txt"),
        stdout_path: request_path.with_extension("flowunsteady.stdout.txt"),
        stderr_path: request_path.with_extension("flowunsteady.stderr.txt"),
        request_path: request_path.clone(),
        polar: None,
        comparable: false,
        error,
    };
    let request = request_from_report(report, config);
    let text = match render_request(&request) {
        Ok(text) => text,
        Err(error) => {
            return fallback(
                FlowUnsteadyAnalysisStatus::RequestRejected,
                Some(error.to_string()),
            )
        }
    };
    if let Err(error) = fs::create_dir_all(request_path.parent().unwrap_or(output_dir))
        .and_then(|_| fs::write(&request_path, text))
    {
        return fallback(
            FlowUnsteadyAnalysisStatus::RequestRejected,
            Some(format!("cannot retain adapter request: {error}")),
        );
    }
    let Some(executable) = executable else {
        return fallback(FlowUnsteadyAnalysisStatus::NotConfigured, Some("FLOWUnsteady request V2 retained, but execution requires an explicitly configured reviewed adapter (ALAS_FLOWUNSTEADY_EXE)".to_owned()));
    };
    let process = run_flowunsteady_adapter(executable, &request_path, timeout_seconds);
    let mut result = FlowUnsteadyAnalysisResult {
        status: FlowUnsteadyAnalysisStatus::NotConfigured,
        runtime_executable: Some(executable.to_path_buf()),
        request_path,
        result_path: process.result_path.clone(),
        stdout_path: process.stdout_path,
        stderr_path: process.stderr_path,
        polar: None,
        comparable: false,
        error: process.error,
    };
    match process.status {
        FlowUnsteadyProcessStatus::InputMissing => {
            result.status = FlowUnsteadyAnalysisStatus::RequestRejected;
            return result;
        }
        FlowUnsteadyProcessStatus::InvalidTimeout => {
            result.status = FlowUnsteadyAnalysisStatus::InvalidTimeout;
            return result;
        }
        FlowUnsteadyProcessStatus::LaunchFailed => {
            result.status = FlowUnsteadyAnalysisStatus::LaunchFailed;
            return result;
        }
        FlowUnsteadyProcessStatus::TimedOut => {
            result.status = FlowUnsteadyAnalysisStatus::TimedOut;
            return result;
        }
        FlowUnsteadyProcessStatus::SolverFailed => {
            result.status = FlowUnsteadyAnalysisStatus::SolverFailed;
            return result;
        }
        FlowUnsteadyProcessStatus::OutputMissing => {
            result.status = FlowUnsteadyAnalysisStatus::OutputMissing;
            return result;
        }
        FlowUnsteadyProcessStatus::Completed => {}
    }
    let text = match fs::read_to_string(&result.result_path) {
        Ok(text) => text,
        Err(error) => {
            result.status = FlowUnsteadyAnalysisStatus::ParseFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    let polar = match parse_result(&text) {
        Ok(polar) => polar,
        Err(error) => {
            result.status = FlowUnsteadyAnalysisStatus::ParseFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    result.comparable = is_comparable(&polar, report, config);
    result.status = if result.comparable {
        FlowUnsteadyAnalysisStatus::CompletedComparable
    } else {
        FlowUnsteadyAnalysisStatus::CompletedNotComparable
    };
    if !result.comparable {
        result.error = Some("FLOWUnsteady result does not declare the shared whole-aircraft reference, frames, and schedule; no overlay is admitted".to_owned());
    }
    result.polar = Some(polar);
    result
}

fn is_comparable(polar: &FlowUnsteadyPolar, report: &AnalysisReport, config: &AlasConfig) -> bool {
    polar.lifting_surfaces_only
        && polar.lift_is_wind_axis
        && polar.pitch_moment_is_body_axis
        && close(polar.area_m2, report.airplane.s_ref)
        && close(polar.chord_m, report.airplane.c_ref)
        && close(polar.span_m, report.airplane.b_ref)
        && polar
            .moment_reference_m
            .iter()
            .zip(report.airplane.xyz_ref)
            .all(|(&a, b)| close(a, b))
        && polar.points.len() == report.polar.alpha_deg.len()
        && polar
            .points
            .iter()
            .zip(&report.polar.alpha_deg)
            .all(|(p, &alpha)| {
                close(p.alpha_deg, alpha)
                    && close(p.mach, config.requirements.cruise_mach)
                    && close(p.beta_deg, 0.0)
            })
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0e-9 * a.abs().max(b.abs()).max(1.0)
}

#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::design_variables::DesignVector;

    #[test]
    fn exported_controls_preserve_clean_polar_provenance() {
        let controls = controls_from_config(&AlasConfig::default());
        assert_eq!(controls.len(), 6);
        assert!(controls
            .iter()
            .all(|control| { control.deflection_deg == 0.0 && !control.applied_to_geometry }));
        assert!(controls.iter().any(|control| {
            control.role == "rudder" && control.surface_name == "Vertical Stabilizer"
        }));
    }

    #[test]
    fn status_text_is_stable() {
        assert_eq!(
            FlowUnsteadyAnalysisStatus::CompletedNotComparable.as_str(),
            "completed_not_comparable"
        );
    }

    #[test]
    fn blended_airfoil_names_are_safe_for_the_comma_delimited_request() {
        assert_eq!(
            flow_airfoil_name("12% morphed, 88% sc20410", 0, 4),
            "12% morphed_ 88% sc20410"
        );
        assert_eq!(flow_airfoil_name("", 2, 7), "section_2_7");
    }

    #[test]
    fn default_report_with_blended_sections_renders_a_flow_request() {
        let config = AlasConfig::default();
        let report = FullAnalysis::new(config.clone())
            .run(&DesignVector::default(), false)
            .expect("default product report should build");
        let request = request_from_report(&report, &config);
        let rendered = render_request(&request).expect("default report should satisfy V2");
        assert!(rendered.contains("airfoil_point="));
        assert!(!rendered.contains("morphed, 88%"));
    }
}
