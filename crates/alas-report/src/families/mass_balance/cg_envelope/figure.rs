// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)

use super::super::no_data_scene;
use super::helpers::{interp, linspace};
use super::render::{render, CgEnvelopeRenderData, LimitMark, StatePoint};
use crate::chart_kit::draw_title;
use crate::families::{model_cg_gate_assessment, MAIN_GEAR_STATION_NOT_MEASURED};
use crate::scene::{Color, Scene};
use crate::theme::get_palette;
use alas_config::optimizer::StructuralBasis;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD};
use alas_opt::envelope::{AftLimitGovernance, ForwardLimitGovernance};
use alas_opt::ModelCgEnvelopeError;
use alas_pipeline::full_analysis::AnalysisReport;

/// Interpolate one physical-limit component across the loading states'
/// masses (ascending), clamped outside the bracket: the gate reports each
/// component only at its named states, and this is the same "cheap mode"
/// linear-in-mass approximation
/// `crate::feasibility::operational_envelope::interpolated_aft_limit_pct_mac`
/// already uses for the aft governing value alone, applied here to every
/// diagnostic component so the drawn curves and the gate never disagree
/// between the states that anchor them.
fn series_over(w_ops: &[f64], masses_kg: &[f64], values_pct_mac: &[f64]) -> Vec<f64> {
    w_ops
        .iter()
        .map(|&w| interp(w, masses_kg, values_pct_mac))
        .collect()
}

/// Declared structural reference weights for the active preset, when the
/// registry names one: used only to choose between a "design" and a
/// certified/structural weight-line label, never as a value substitution.
fn declared_reference_weights(config: &AlasConfig) -> (bool, bool) {
    let reference = alas_config::presets::get(&config.preset)
        .ok()
        .map(|preset| preset.reference.clone());
    let has_mtow = reference
        .as_ref()
        .and_then(|r| r.mtow_kg)
        .is_some_and(|v| v.is_finite() && v > 0.0);
    let has_mzfw = reference
        .as_ref()
        .and_then(|r| r.mzfw_kg)
        .is_some_and(|v| v.is_finite() && v > 0.0);
    (has_mtow, has_mzfw)
}

/// Build every series [`render::render`] draws, or the placeholder message
/// to show instead. Split out of [`figure_cg_envelope`] so a test can
/// recompute the exact same [`CgEnvelopeRenderData`] a rendered figure used,
/// map a gate value through [`super::render::axes_view`] itself, and check
/// the two agree -- without re-deriving this construction a second time.
pub(super) fn prepare(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Result<CgEnvelopeRenderData, &'static str> {
    if report.component_masses.is_empty() || report.mass_coordinates.is_empty() {
        return Err("No mass / coordinate data available");
    }

    let assessment = match model_cg_gate_assessment(report, config) {
        Ok(assessment) => assessment,
        Err(ModelCgEnvelopeError::MainGearStationNotMeasured(_)) => {
            return Err(MAIN_GEAR_STATION_NOT_MEASURED);
        }
        Err(_) => {
            return Err("No mass / coordinate data available");
        }
    };

    let mut states: Vec<_> = assessment
        .loading_states
        .iter()
        .filter(|state| state.mass_kg.is_finite() && state.cg_pct_mac.is_finite())
        .collect();
    states.sort_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg));
    if states.is_empty() {
        return Err("No mass / coordinate data available");
    }

    let masses_kg: Vec<f64> = states.iter().map(|s| s.mass_kg).collect();
    let aft_gov: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.aft_limit_pct_mac)
        .collect();
    let aero_aft: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.aerodynamic_aft_pct_mac)
        .collect();
    let ground_aft: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.ground_aft_pct_mac)
        .collect();
    let tip_aft: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.tip_back_aft_pct_mac)
        .collect();
    let fwd_gov: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.fwd_limit_pct_mac)
        .collect();
    let max_nose_fwd: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.max_nose_load_fwd_pct_mac)
        .collect();
    let scissor_fwd: Vec<f64> = states
        .iter()
        .map(|s| s.physical_limits.scissor_plot_fwd_pct_mac)
        .collect();

    let aero_active = states
        .iter()
        .any(|s| s.physical_limits.aft_limit_governance == AftLimitGovernance::Aerodynamic);
    let ground_active = states.iter().any(|s| {
        s.physical_limits.aft_limit_governance == AftLimitGovernance::GroundMinimumNoseLoad
    });
    let tip_active = states
        .iter()
        .any(|s| s.physical_limits.aft_limit_governance == AftLimitGovernance::TipBack);
    let max_nose_active = states.iter().any(|s| {
        s.physical_limits.fwd_limit_governance == ForwardLimitGovernance::MaxNoseLoadHandling
    });
    let scissor_active = states.iter().any(|s| {
        s.physical_limits.fwd_limit_governance == ForwardLimitGovernance::ScissorPlotEstimate
    });

    let w_lo = masses_kg[0];
    let w_hi = masses_kg[masses_kg.len() - 1].max(w_lo * 1.001);
    let w_ops = linspace(w_lo, w_hi, 150);
    let poly_aft = series_over(&w_ops, &masses_kg, &aft_gov);
    let poly_fwd = series_over(&w_ops, &masses_kg, &fwd_gov);
    let aero_aft = series_over(&w_ops, &masses_kg, &aero_aft);
    let ground_aft = series_over(&w_ops, &masses_kg, &ground_aft);
    let tip_aft = series_over(&w_ops, &masses_kg, &tip_aft);
    let max_nose_fwd = series_over(&w_ops, &masses_kg, &max_nose_fwd);
    let scissor_fwd = series_over(&w_ops, &masses_kg, &scissor_fwd);

    let limit_marks: Vec<LimitMark> = states
        .iter()
        .map(|s| LimitMark {
            mass_kg: s.mass_kg,
            fwd_pct_mac: s.physical_limits.fwd_limit_pct_mac,
            fwd_label: forward_mark(s.physical_limits.fwd_limit_governance),
            aft_pct_mac: s.physical_limits.aft_limit_pct_mac,
            aft_label: aft_mark(s.physical_limits.aft_limit_governance),
        })
        .collect();

    let get_mass = |k: &str| report.component_masses.get(k).copied().unwrap_or(0.0);
    let oew_mass: f64 = OEW_KEYS.iter().map(|&k| get_mass(k)).sum();
    let payload = get_mass(PAYLOAD);
    let fuel = get_mass(FUEL);
    let mtow_mass = oew_mass + payload + fuel.max(0.0);
    // A run that designs its structure at the closed takeoff mass reads the
    // design landing mass at that closure; every other run keeps the
    // declared landing limit.
    let mlw_mass = if let Some(design_kg) = report.design_landing_mass_kg() {
        design_kg
    } else if config.mtow_plan().structural_basis == StructuralBasis::ClosureMass {
        config.design_landing_mass_at_closure(mtow_mass)
    } else {
        config.landing_mass_limit_kg(mtow_mass)
    };
    let mzfw_mass = oew_mass + payload;
    let (has_structural_mtow, has_structural_mzfw) = declared_reference_weights(config);

    let state_points: Vec<StatePoint> = states
        .iter()
        .map(|s| StatePoint {
            mass_kg: s.mass_kg,
            cg_pct_mac: s.cg_pct_mac,
            label: s.state.label().to_owned(),
        })
        .collect();

    Ok(CgEnvelopeRenderData {
        mtow_mass,
        mlw_mass,
        mzfw_mass,
        mtow_label: if report.sized_takeoff_mass_kg().is_some() {
            // The line is the run's mission-sized takeoff mass, not a
            // certified maximum.
            "sized TOW"
        } else if has_structural_mtow {
            "MTOW"
        } else {
            "design TOW"
        },
        mzfw_label: if has_structural_mzfw {
            "MZFW"
        } else {
            "design ZFW"
        },
        clean_np_pct_mac: assessment.clean_np_pct_mac,
        w_ops,
        poly_fwd,
        poly_aft,
        aero_aft,
        ground_aft,
        tip_aft,
        aero_active,
        ground_active,
        tip_active,
        max_nose_fwd,
        scissor_fwd,
        max_nose_active,
        scissor_active,
        state_points,
        limit_marks,
    })
}

/// Short name of the mechanism governing a forward limit.
fn forward_mark(governance: ForwardLimitGovernance) -> &'static str {
    match governance {
        ForwardLimitGovernance::MaxNoseLoadHandling => "max nose load (ground)",
        ForwardLimitGovernance::ScissorPlotEstimate => "scissor estimate",
        ForwardLimitGovernance::RotationNoseWheelLiftoff => "rotation (takeoff)",
        ForwardLimitGovernance::LandingTrimGroundEffect => "landing trim",
    }
}

/// Short name of the mechanism governing an aft limit.
fn aft_mark(governance: AftLimitGovernance) -> &'static str {
    match governance {
        AftLimitGovernance::Aerodynamic => "static-margin floor",
        AftLimitGovernance::GroundMinimumNoseLoad => "min nose load (ground)",
        AftLimitGovernance::TipBack => "tip-back (ground)",
    }
}

/// Generate a model-derived CG loading-state check figure.
///
/// Every aft/forward physical boundary drawn here is read from
/// [`crate::families::model_cg_gate_assessment`] -- the same gate the
/// feasibility pipeline evaluates a design against -- rather than re-derived
/// from a simplified aerodynamic/gear formula local to this figure.
/// The gate reports each boundary only at five named
/// loading states; the curves drawn between them are this figure's own
/// linear-in-mass interpolation (documented on [`series_over`]), not
/// additional gate evaluations.
///
/// The figure is not an AFM/WBM operational envelope or evidence of
/// certified loading-order, fuel-sequence, or mission coverage.
pub fn figure_cg_envelope(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 620.0, Some(Color::from_hex(pal.bg)));
    let title = "Weight & Balance / Model CG Loading-State Check";
    scene.title = Some(title.to_owned());
    draw_title(&mut scene, title, pal);
    scene.suppress_derived_title();

    match prepare(report, config) {
        Ok(data) => {
            render(&mut scene, pal, data);
            scene
        }
        Err(message) => no_data_scene(scene, pal, message),
    }
}
