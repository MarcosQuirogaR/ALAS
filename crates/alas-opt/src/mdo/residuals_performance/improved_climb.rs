// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Improved-climb resolution of the take-off safety speed and the top-of-climb
//! margin at the initial cruise level the mission selects.
//!
//! [`resolve_improved_climb`] evaluates the CS 25.121(b) requirement at the
//! schedule `V2` with the shared engine-out evidence builder, and, if the
//! installed thrust misses it, searches the lowest `V2 / VSR` that meets it
//! (`alas_perf::performance::improved_climb`, with the sources). The result is
//! written into `performance.improved_climb_v2_over_vsr`, which raises `V2` in
//! the speed schedule and charges the longer jet field length, so every
//! downstream consumer (second-segment assessor, field residuals, matching
//! chart, field reference) reads one consistent speed. Jets only: a
//! turboprop's thrust ratio rests on a different static basis.

use std::borrow::Cow;

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, far25_oei_gradient, oei_cl_at_v2,
    select_improved_v2, ImprovedClimbSelection, VSpeeds, TOC_RESIDUAL_CLIMB_M_S,
};

use super::oei_deck::{second_segment_evidence, EngineOutModel};
use crate::mdo::sizing::candidate_model::candidate_mission_model;
use crate::mdo::types::{CandidateDrag, CandidateFuelArtifacts};

/// What the selection needs to know about the candidate.
pub struct ImprovedClimbInputs<'a> {
    /// Engine-out evidence builder bound to the candidate (weight bound for the trim).
    pub model: &'a EngineOutModel,
    /// Departure aerodrome of the check.
    pub departure: &'a Airport,
    /// Take-off mass, kg.
    pub mass_kg: f64,
    /// Reference wing area, m^2.
    pub s_ref_m2: f64,
    /// The candidate's clean drag.
    pub drag: &'a CandidateDrag,
    /// Installed all-engine static thrust over the take-off weight.
    pub installed_tw_sls: f64,
}

/// `config` with the improved-climb `V2 / VSR` resolved into its performance
/// group, and the selection. The original is returned unchanged when the model
/// is off, the engine is a turboprop, the engine count is outside the Part 25
/// table, or the evidence cannot be evaluated.
pub fn resolve_improved_climb<'a>(
    config: &'a AlasConfig,
    inputs: &ImprovedClimbInputs<'_>,
) -> (Cow<'a, AlasConfig>, Option<ImprovedClimbSelection>) {
    let perf = &config.performance;
    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let gradient = far25_oei_gradient(n_engines);
    let (Some(gradient), true) = (
        gradient,
        perf.improved_climb_v2 && !inputs.model.is_turboprop(),
    ) else {
        return (Cow::Borrowed(config), None);
    };
    // The schedule speeds: no previous improved-climb override.
    let mut schedule_perf = perf.clone();
    schedule_perf.improved_climb_v2_over_vsr = None;
    let departure = inputs.departure;
    let schedule = compute_v_speeds_at_masses(
        inputs.mass_kg,
        inputs.mass_kg,
        inputs.s_ref_m2,
        departure,
        perf.cl_max_to,
        perf.cl_max_land,
        &schedule_perf,
    );
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let required = |x: f64| -> Option<f64> {
        let speeds = VSpeeds {
            v2_ms: x * schedule.v_stall_to_ms,
            ..schedule
        };
        let evidence = second_segment_evidence(
            Some(inputs.model),
            &schedule_perf,
            departure,
            &speeds,
            &|mach, altitude_m, isa_deviation_c| {
                inputs
                    .drag
                    .cd_at_atmosphere(0.0, mach, altitude_m, isa_deviation_c)
            },
        );
        let cl = oei_cl_at_v2(perf.cl_max_to, x)?;
        let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
        let cd = inputs.drag.cd_airport_base_at_atmosphere(
            cl,
            mach,
            departure.elevation_m,
            departure.isa_deviation_c,
        );
        assess_oei_climb(
            cd,
            0.0,
            n_engines,
            gradient,
            cl,
            perf.oei_climb_delta_cd,
            perf.cl_max_to,
            Some(evidence.condition),
        )
        .required_sls_tw
    };
    let Some(selection) = select_improved_v2(
        schedule.v2_ms / schedule.v_stall_to_ms,
        inputs.installed_tw_sls,
        &required,
    ) else {
        return (Cow::Borrowed(config), None);
    };
    // Only a speed that meets the gradient is flown (and charged to the field
    // length): when even the best-climb speed misses, the schedule speed stands
    // and the shortfall is reported there, with the best-climb result as a
    // Diagnostic.
    let wanted = (selection.improved && selection.meets).then_some(selection.v2_over_vsr);
    if wanted == perf.improved_climb_v2_over_vsr {
        return (Cow::Borrowed(config), Some(selection));
    }
    let mut resolved = config.clone();
    resolved.performance.improved_climb_v2_over_vsr = wanted;
    (Cow::Owned(resolved), Some(selection))
}

/// The residual climb of the design mission at the initial cruise level it
/// selects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopOfClimb {
    /// Initial cruise level of the closure's frozen plan, m.
    pub altitude_m: f64,
    /// Residual rate of climb at maximum-climb thrust at that level and the
    /// top-of-climb mass, m/s.
    pub rate_m_s: f64,
    /// Maximum-climb thrust (all engines) at that state, N.
    pub thrust_n: f64,
    /// Weight at that state, N.
    pub weight_n: f64,
    /// True airspeed at that state, m/s.
    pub tas_m_s: f64,
}

impl TopOfClimb {
    /// The thrust the criterion requires at the same state, N: the available
    /// thrust less the surplus (or plus the deficit) that the rate margin
    /// represents, `T_req = T - (R - R_crit) W / V` (drag and speed fixed).
    pub fn required_thrust_n(&self) -> f64 {
        self.thrust_n - self.margin_m_s() * self.weight_n / self.tas_m_s
    }

    /// The thrust check against the criterion with the correlation scatter.
    pub fn thrust_assessment(&self, scatter: f64) -> super::thrust_uncertainty::ThrustAssessment {
        super::thrust_uncertainty::ThrustAssessment::new(
            self.required_thrust_n(),
            self.thrust_n,
            scatter,
        )
    }

    /// Margin over the 300 ft/min criterion, m/s.
    pub fn margin_m_s(&self) -> f64 {
        self.rate_m_s - TOC_RESIDUAL_CLIMB_M_S
    }
}

/// Numerical tolerance on the rate comparison, m/s.
pub const TOC_TOLERANCE_M_S: f64 = 1.0e-6;

/// The plan level is continuous (no flight-level rounding) and is found against
/// the cruise and the climb-top rung speeds, so the criterion evaluated at the
/// cruise speed holds within a few metres of it (measured 5 m for the AVE,
/// 0.0024 m/s per metre). The criterion is therefore evaluated this far below
/// the plan level, 30 m (100 ft), a tenth of the 1,000 ft flight-level grid.
pub const TOC_LEVEL_TOLERANCE_M: f64 = 30.0;

/// Residual climb at the initial cruise level the candidate's mission plan
/// selected (step-climb practice: the plan lowers the level until the
/// criterion holds), evaluated by the mission model itself at the speeds the
/// plan flies at that level (the quantity the level search holds, not the
/// constant-Mach speed `M a(h)`).
///
/// # Errors
///
/// No frozen plan, or the model/deck rejection, as text.
pub fn top_of_climb(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
) -> Result<TopOfClimb, String> {
    let plan = artifacts
        .frozen_plan
        .as_ref()
        .ok_or_else(|| "the closure carries no frozen mission plan".to_owned())?;
    let model = candidate_mission_model(config, artifacts)?;
    let point = model.top_of_climb_point(
        plan.takeoff_mass_kg,
        plan.cruise_altitude_m - TOC_LEVEL_TOLERANCE_M,
        plan.range_m,
        plan.climb_rate_scales,
    )?;
    Ok(TopOfClimb {
        altitude_m: plan.cruise_altitude_m,
        rate_m_s: point.rate_m_s,
        thrust_n: point.thrust_n,
        weight_n: point.weight_n,
        tas_m_s: point.tas_m_s,
    })
}
