// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CG envelope of a body that declares no belly clearance: assessed at
//! both ends of the documented clearance sample of its class and merged per
//! state and per constraint (the more forward aft limit, the more aft forward
//! limit, the larger violation of each constraint).
//!
//! No single end is conservative for every mechanism. The tip-back aft
//! boundary and turnover angle are worse with a large clearance (a higher
//! CG); the tail-scrape angle against the rotation angle is worse with a
//! small one; the rotation forward boundary moves either way: by probe the
//! E195-E2 limit is -3.615 % MAC at 0.298 D and -3.775 % at 0.546 D, the AVE's
//! 27.53 % and 29.06 %. A declared clearance is one plane and one pass.

use std::borrow::Cow;

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::stations::{
    clearance_sample, ground_clearance_for, ground_plane_bases, GroundPlaneBasis,
};

use super::constraints::ModelCgConstraintAssessment;
use super::ledger_basis::assess_on_one_ground_plane;
use super::physical_limits::PhysicalCgLimits;
use super::{
    ModelCgEnvelopeAssessment, ModelCgEnvelopeError, ModelCgLoadingAssessment, ModelCgLoadingState,
};

/// `config` with the belly clearance of `basis` declared: the same config for
/// a declared clearance and for the nominal basis.
pub(super) fn config_on<'a>(
    basis: GroundPlaneBasis,
    plane: &Airplane,
    config: &'a AlasConfig,
) -> Result<Cow<'a, AlasConfig>, ModelCgEnvelopeError> {
    if basis == GroundPlaneBasis::Nominal {
        return Ok(Cow::Borrowed(config));
    }
    let fuselage = plane
        .fuselages
        .first()
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;
    let wing = plane
        .wings
        .first()
        .ok_or(ModelCgEnvelopeError::MissingMainWing)?;
    let sample = clearance_sample(wing, fuselage, &config.geometry, &config.landing_gear);
    let mut declared = config.clone();
    declared.landing_gear.fuselage_ground_clearance_m = Some(ground_clearance_for(
        basis,
        sample,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    ));
    Ok(Cow::Owned(declared))
}

/// `a` or `b`, whichever `better` prefers; a NaN limit never wins.
fn pick<T: Copy>(a: T, b: T, key: impl Fn(T) -> f64, larger: bool) -> T {
    let (ka, kb) = (key(a), key(b));
    if kb.is_nan() || (!ka.is_nan() && if larger { ka >= kb } else { ka <= kb }) {
        a
    } else {
        b
    }
}

/// The limits both ends support: the more forward aft boundary and the more
/// aft forward boundary, each with the mechanism that produced it; the
/// diagnostics keep their own conservative end.
fn merge_limits(a: PhysicalCgLimits, b: PhysicalCgLimits) -> PhysicalCgLimits {
    let aft = pick(a, b, |l| l.aft_limit_pct_mac, false);
    let fwd = pick(a, b, |l| l.fwd_limit_pct_mac, true);
    let rotation = pick(a, b, |l| l.rotation_fwd_pct_mac, true);
    let (min, max) = (f64::min, f64::max);
    PhysicalCgLimits {
        aft_limit_pct_mac: aft.aft_limit_pct_mac,
        aft_limit_governance: aft.aft_limit_governance,
        clean_np_pct_mac: a.clean_np_pct_mac,
        aerodynamic_aft_pct_mac: a.aerodynamic_aft_pct_mac,
        ground_aft_pct_mac: min(a.ground_aft_pct_mac, b.ground_aft_pct_mac),
        tip_back_aft_pct_mac: min(a.tip_back_aft_pct_mac, b.tip_back_aft_pct_mac),
        fwd_limit_pct_mac: fwd.fwd_limit_pct_mac,
        fwd_limit_governance: fwd.fwd_limit_governance,
        max_nose_load_fwd_pct_mac: max(a.max_nose_load_fwd_pct_mac, b.max_nose_load_fwd_pct_mac),
        scissor_plot_fwd_pct_mac: max(a.scissor_plot_fwd_pct_mac, b.scissor_plot_fwd_pct_mac),
        rotation_fwd_pct_mac: rotation.rotation_fwd_pct_mac,
        takeoff_trim_fwd_pct_mac: max(a.takeoff_trim_fwd_pct_mac, b.takeoff_trim_fwd_pct_mac),
        clean_trim_fwd_pct_mac: max(a.clean_trim_fwd_pct_mac, b.clean_trim_fwd_pct_mac),
        landing_approach_fwd_pct_mac: max(a.landing_approach_fwd_pct_mac, b.landing_approach_fwd_pct_mac),
        landing_flare_fwd_pct_mac: max(a.landing_flare_fwd_pct_mac, b.landing_flare_fwd_pct_mac),
        rotation_longitudinal_force_shift_pct_mac: rotation
            .rotation_longitudinal_force_shift_pct_mac,
        rotation_tail_lift_coefficient: rotation.rotation_tail_lift_coefficient,
        rotation_stabilizer_nose_up_deg: rotation.rotation_stabilizer_nose_up_deg,
        landing_trim_fwd_pct_mac: max(a.landing_trim_fwd_pct_mac, b.landing_trim_fwd_pct_mac),
        iced_tail_fwd_pct_mac: max(a.iced_tail_fwd_pct_mac, b.iced_tail_fwd_pct_mac),
        landing_tail_lift_coefficient: a.landing_tail_lift_coefficient,
        landing_tail_trim_limited: a.landing_tail_trim_limited,
        landing_tail_sourced: a.landing_tail_sourced,
        landing_trim_condition: a.landing_trim_condition,
        usable_range_pct_mac: aft.aft_limit_pct_mac - fwd.fwd_limit_pct_mac,
    }
}

/// Whether `other` is the more restrictive of two assessments of one constraint.
fn tighter(other: &ModelCgConstraintAssessment, kept: &ModelCgConstraintAssessment) -> bool {
    other.normalized_exceedance > kept.normalized_exceedance
        || (other.normalized_exceedance == kept.normalized_exceedance
            && !kept.violated
            && (other.limit - other.actual).abs() < (kept.limit - kept.actual).abs())
}

/// One state on both ends: each constraint keeps the end with the larger
/// violation, and with both feasible the end nearer its limit (so a bound
/// constraint carries the same limit as the merged physical limits); the
/// limits keep the bound both ends support.
fn merge_state(
    mut first: ModelCgLoadingAssessment,
    second: ModelCgLoadingAssessment,
) -> ModelCgLoadingAssessment {
    for other in second.constraints {
        match first
            .constraints
            .iter_mut()
            .find(|kept| kept.constraint == other.constraint)
        {
            Some(kept) if tighter(&other, kept) => *kept = other,
            Some(_) => {}
            None => first.constraints.push(other),
        }
    }
    first.physical_limits = merge_limits(first.physical_limits, second.physical_limits);
    first.ground_reactions_admissible &= second.ground_reactions_admissible;
    first
}

/// The assessment both ends support, state by state and constraint by
/// constraint (the plane moves each limit monotonically only per mechanism,
/// not for the envelope as a whole, so no single end is kept whole).
fn merge(
    first: ModelCgEnvelopeAssessment,
    second: ModelCgEnvelopeAssessment,
) -> ModelCgEnvelopeAssessment {
    let aft = pick(
        (first.worst_aft_limit_pct_mac, first.aft_limit_governance),
        (second.worst_aft_limit_pct_mac, second.aft_limit_governance),
        |(limit, _)| limit,
        false,
    );
    let scrape = match (first.scrape_angle_deg, second.scrape_angle_deg) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    ModelCgEnvelopeAssessment {
        loading_states: first
            .loading_states
            .into_iter()
            .zip(second.loading_states)
            .map(|(a, b)| merge_state(a, b))
            .collect(),
        configured_forward_limit_pct_mac: first
            .configured_forward_limit_pct_mac
            .max(second.configured_forward_limit_pct_mac),
        max_nose_load_fwd_limit_pct_mac: first
            .max_nose_load_fwd_limit_pct_mac
            .max(second.max_nose_load_fwd_limit_pct_mac),
        landing_trim_fwd_limit_pct_mac: first
            .landing_trim_fwd_limit_pct_mac
            .max(second.landing_trim_fwd_limit_pct_mac),
        scissor_plot_fwd_limit_pct_mac: first
            .scissor_plot_fwd_limit_pct_mac
            .max(second.scissor_plot_fwd_limit_pct_mac),
        ground_aft_limit_pct_mac: first
            .ground_aft_limit_pct_mac
            .min(second.ground_aft_limit_pct_mac),
        worst_aft_limit_pct_mac: aft.0,
        aft_limit_governance: aft.1,
        tire_overloaded: first.tire_overloaded || second.tire_overloaded,
        required_tip_back_deg: first
            .required_tip_back_deg
            .max(second.required_tip_back_deg),
        scrape_angle_deg: scrape,
        ..first
    }
}

/// [`assess_on_one_ground_plane`] on every ground-plane end the clearance
/// leaves open, keeping the worse snapshot.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_model_cg_envelope_from_states(
    plane: &Airplane,
    states: Vec<(ModelCgLoadingState, f64, f64, f64)>,
    pitch_inertia: Option<&super::LedgerLoadingBasis>,
    cg_x: f64,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let mut kept: Option<ModelCgEnvelopeAssessment> = None;
    for &basis in ground_plane_bases(&config.landing_gear) {
        let on_end = config_on(basis, plane, config)?;
        let assessment = assess_on_one_ground_plane(
            plane,
            states.clone(),
            pitch_inertia,
            cg_x,
            x_np,
            critical_x_np,
            mac,
            &on_end,
        )?;
        kept = Some(match kept {
            None => assessment,
            Some(previous) => merge(previous, assessment),
        });
    }
    kept.ok_or(ModelCgEnvelopeError::MissingFuselage)
}
