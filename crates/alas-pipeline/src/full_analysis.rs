// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/full_analysis.py
// Reference: alas @ rust-port-baseline.

//! High-fidelity aerodynamic, mass and stability analysis of a candidate design.
//!
//! [`FullAnalysis::run`] builds the 3-D aircraft geometry, calculates the
//! two-pass mass distribution (lumped first, then detailed cabin layout),
//! sets the moment reference to the true physical centre of gravity, runs a
//! fine-mesh vortex-lattice polar sweep, fits the drag polar, finds the
//! operating design point, evaluates neutral point and static margin, checks
//! typed model-derived CG and ground-reaction constraints, and solves the
//! trimmed cruise point.

use std::collections::HashMap;
use std::f64::consts::PI;

use alas_aero::analysis::{AeroAnalysis, PolarSweep, TrimPoint};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{MassBreakdown, MassCoordinateModel, MassCoordinates};
use alas_math::lstsq::least_squares;
use alas_opt::envelope::{assess_model_cg_envelope, check_cg_envelope};
use alas_payload::build::{build_payload_layout, build_payload_layout_reference_compatibility};
use alas_payload::layout::PayloadLayout;
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::{
    neutral_point, neutral_point_reference_compatibility, stability_and_trim,
    stability_and_trim_reference_compatibility,
};
use serde::{Deserialize, Serialize};

/// Operating conditions at the cruise design point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesignPoint {
    /// Angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
}

/// Genuinely trimmed cruise operating point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrimmedDesignPoint {
    /// Compressibility-corrected trim angle shown in summaries, in degrees.
    ///
    /// This is a presentation quantity. It is intentionally distinct from
    /// [`Self::geometric_body_alpha_deg`]: a 2-D section solver and an
    /// aircraft-attitude requirement must use the geometric VLM angle rather
    /// than this Prandtl-Glauert display correction.
    pub alpha_deg: f64,
    /// Geometric aircraft-body angle passed to the trimmed VLM solve, in
    /// degrees.
    ///
    /// Positive values raise the aircraft nose relative to the freestream.
    /// Root incidence, local washout, and induced angle are not included;
    /// consumers that need a local section condition must add them explicitly.
    pub geometric_body_alpha_deg: f64,
    /// Trimmed horizontal-stabilizer incidence, in degrees.
    pub trim_ih_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
    /// Residual pitching moment (should be close to zero).
    pub cm_residual: f64,
}

/// Provenance of a least-squares parabolic fit of the clean polar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolarFitStatus {
    /// The configured fit window produced a successful least-squares fit.
    Fitted,
    /// The configured window was too small, so the documented fallback
    /// window produced the fit instead.
    FittedFallbackWindow,
    /// The configured and fallback windows did not contain enough points;
    /// the historical constant coefficients were retained.
    FallbackInsufficientPoints,
    /// The selected points could not be solved by least squares; the
    /// historical constant coefficients were retained.
    FallbackLeastSquaresFailure,
}

impl PolarFitStatus {
    /// Stable status text for reports and machine-readable exports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fitted => "fitted",
            Self::FittedFallbackWindow => "fitted_fallback_window",
            Self::FallbackInsufficientPoints => "fallback_insufficient_points",
            Self::FallbackLeastSquaresFailure => "fallback_least_squares_failure",
        }
    }
}

/// Least-squares parabolic fit of the clean polar: `CD = CD0 + k * CL^2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarFit {
    /// Zero-lift parasitic drag coefficient.
    pub cd0: f64,
    /// Induced drag factor `k`.
    pub k: f64,
    /// Oswald efficiency factor `e = 1 / (pi * AR * k)`.
    pub oswald_e: f64,
    /// Wing aspect ratio.
    pub aspect_ratio: f64,
    /// Provenance of the fit coefficients, including any retained fallback.
    pub status: PolarFitStatus,
}

/// Complete aerodynamic, mass, and stability report of an aircraft design.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisReport {
    /// The input design vector.
    pub design: DesignVector,
    /// The built 3-D aircraft geometry.
    pub airplane: Airplane,
    /// Fine-resolution polar sweep.
    pub polar: PolarSweep,
    /// Un-trimmed operating point closest to cruise `CL_required`.
    pub design_point: DesignPoint,
    /// Parabolic polar fit.
    pub polar_fit: PolarFit,
    /// Aerodynamic static margin `(x_np - x_cg) / c_ref`.
    pub static_margin: f64,
    /// Neutral point longitudinal position, in meters.
    pub x_neutral_point: f64,
    /// Scalar geometry summary metrics.
    pub geometry_summary: HashMap<String, f64>,
    /// Breakdown of masses by component name, in kg.
    pub component_masses: HashMap<String, f64>,
    /// Centroid positions by component name, in meters `[x, y, z]`.
    pub mass_coordinates: HashMap<String, [f64; 3]>,
    /// Global mass-weighted center of gravity `[x, y, z]`, in meters.
    pub physical_cg: [f64; 3],
    /// Detailed cabin and cargo payload layout, if successfully constructed.
    pub payload_layout: Option<PayloadLayout>,
    /// Jointly trimmed cruise operating point, if trim solve converged.
    pub trimmed_design_point: Option<TrimmedDesignPoint>,
    /// True if the active model CG and gear constraints are compliant.
    ///
    /// Reference-compatibility analyses preserve the frozen Python Boolean;
    /// product analyses use the hard physical floor and typed gear checks.
    pub cg_envelope_ok: Option<bool>,
}

impl AnalysisReport {
    /// Extract the view required by mission vehicle generation.
    pub fn mission_view(&self) -> alas_mission::ReportView {
        alas_mission::ReportView {
            trimmed_l_over_d: self.trimmed_design_point.as_ref().map(|p| p.l_over_d),
            plain_l_over_d: Some(self.design_point.l_over_d),
            design_vector: serde_json::to_value(self.design).unwrap_or_default(),
            geometry_summary: serde_json::to_value(&self.geometry_summary).unwrap_or_default(),
            component_masses: serde_json::to_value(&self.component_masses).unwrap_or_default(),
        }
    }
}

/// Evaluates high-fidelity multidisciplinary analyses for candidate aircraft designs.
#[derive(Debug, Clone)]
pub struct FullAnalysis {
    /// Configuration governing geometry, requirements, analysis fidelity, and mass models.
    pub config: AlasConfig,
    reference_compatibility: bool,
}

mod station_coordinates;
pub(crate) use station_coordinates::station_coordinates_for;

include!("full_analysis_parts/part_01.rs");
include!("full_analysis_parts/part_02.rs");

fn breakdown_to_map(mb: &MassBreakdown) -> HashMap<String, f64> {
    mb.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn coordinates_to_map(mc: &MassCoordinates) -> HashMap<String, [f64; 3]> {
    mc.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{FullAnalysis, PolarFitStatus};
    use alas_aero::analysis::PolarSweep;
    use alas_config::design_variables::DesignVector;
    use alas_config::{AlasConfig, AnalysisConfig};
    use alas_geom::builder::AircraftBuilder;

    fn sweep(cl: Vec<f64>, cd: Vec<f64>) -> PolarSweep {
        let n = cl.len();
        PolarSweep {
            alpha_deg: vec![0.0; n],
            geometric_alpha_deg: vec![0.0; n],
            cl,
            cd,
            cd_induced: vec![0.0; n],
            cd_wave: vec![0.0; n],
            cd_parasite: vec![0.0; n],
            cm: vec![0.0; n],
            l_over_d: vec![0.0; n],
        }
    }

    #[test]
    fn product_full_analysis_preserves_the_live_engine_configuration() {
        let mut config = AlasConfig::default();
        config.geometry.engine.engine_name = "Trent 900".to_owned();
        config.geometry.engine.nacelle_profile = vec![(0.0, 0.33), (2.5, 1.0), (6.2, 0.41)];
        config.geometry.engine.radius_scale_m = 1.91;
        config
            .geometry
            .engine
            .turbofan
            .as_mut()
            .unwrap()
            .rated_thrust_kn = 399.0;
        config.geometry.engine.bypass_ratio = 9.2;
        config.geometry.engine.overall_pressure_ratio = 43.0;
        config.geometry.engine.fan_pressure_ratio = 1.59;
        config.geometry.engine.turbine_inlet_temp_k = 1712.0;
        config.geometry.engine.cruise_tsfc_kg_kgf_hr = 0.481;
        config.geometry.engine.fan_diameter_m = 3.11;
        let expected = config.geometry.engine.clone();

        let analysis = FullAnalysis::new(config);

        assert_eq!(analysis.config.geometry.engine, expected);
        let builder = AircraftBuilder::new(Some(analysis.config.geometry.clone()));
        assert_eq!(builder.geometry.engine, expected);
    }

    #[test]
    fn degenerate_polar_fit_preserves_constants_with_explicit_status() {
        let fit = FullAnalysis::fit_polar_values(
            &sweep(vec![0.0], vec![0.03]),
            10.0,
            &AnalysisConfig::default(),
        );

        assert_eq!(fit.status, PolarFitStatus::FallbackInsufficientPoints);
        assert_eq!(fit.status.as_str(), "fallback_insufficient_points");
        assert_eq!(fit.cd0, 0.02);
        assert_eq!(fit.k, 0.04);
    }

    #[test]
    fn rank_deficient_polar_fit_preserves_constants_with_explicit_status() {
        let fit = FullAnalysis::fit_polar_values(
            &sweep(vec![0.4, 0.4, 0.4], vec![0.03, 0.04, 0.05]),
            10.0,
            &AnalysisConfig::default(),
        );

        assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
        assert_eq!(fit.status.as_str(), "fallback_least_squares_failure");
        assert_eq!(fit.cd0, 0.02);
        assert_eq!(fit.k, 0.04);
    }

    #[test]
    fn nonfinite_selected_polar_values_use_the_least_squares_fallback() {
        for invalid_cd in [f64::NAN, f64::INFINITY] {
            let fit = FullAnalysis::fit_polar_values(
                &sweep(vec![0.35, 0.45, 0.55], vec![0.0249, invalid_cd, 0.0321]),
                10.0,
                &AnalysisConfig::default(),
            );

            assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
            assert_eq!(fit.cd0, 0.02);
            assert_eq!(fit.k, 0.04);
        }
    }

    #[test]
    fn nonfinite_least_squares_solution_uses_the_least_squares_fallback() {
        let config = AnalysisConfig {
            polar_fit_cl_min: 0.0,
            polar_fit_cl_max: f64::MAX,
            ..AnalysisConfig::default()
        };

        // These source values are finite, but CL squared overflows while building the
        // fit matrix. The non-finite QR result must not acquire fitted status.
        let fit = FullAnalysis::fit_polar_values(
            &sweep(vec![1.0e200, 2.0e200, 3.0e200], vec![0.02, 0.03, 0.04]),
            10.0,
            &config,
        );

        assert_eq!(fit.status, PolarFitStatus::FallbackLeastSquaresFailure);
        assert_eq!(fit.cd0, 0.02);
        assert_eq!(fit.k, 0.04);
    }

    #[test]
    fn successful_fits_identify_their_window_provenance() {
        let primary = FullAnalysis::fit_polar_values(
            &sweep(vec![0.35, 0.45, 0.55], vec![0.0249, 0.0281, 0.0321]),
            10.0,
            &AnalysisConfig::default(),
        );
        assert_eq!(primary.status, PolarFitStatus::Fitted);

        let fallback_window = FullAnalysis::fit_polar_values(
            &sweep(vec![0.2, 0.4], vec![0.0216, 0.0264]),
            10.0,
            &AnalysisConfig::default(),
        );
        assert_eq!(fallback_window.status, PolarFitStatus::FittedFallbackWindow);
    }

    #[test]
    fn successful_full_analysis_retains_the_detailed_payload_layout() {
        let report = FullAnalysis::new(AlasConfig::default())
            .run(&DesignVector::default(), false)
            .unwrap_or_else(|error| {
                panic!("default full analysis should resolve payload: {error}")
            });

        let layout = match report.payload_layout.as_ref() {
            Some(layout) => layout,
            None => panic!("a successful full analysis must carry its detailed layout"),
        };
        assert!(layout.total_mass.is_finite());
        assert!(layout.cg_x.is_finite());
        assert!(layout.cg_y.is_finite());
    }
}
