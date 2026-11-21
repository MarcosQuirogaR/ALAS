// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Balance and ground-clearance residuals measured against the registered
//! aircraft a reference adaptation redesigns, or a clean sheet designs from
//! a registered preset, on its own mission or a new one ([`AlasConfig::holds_to_registered_aircraft`]): the search
//! cannot move either quantity (the gear and upsweep are not sized), so a
//! new mission keeps the registered reference on them too.
//!
//! The absolute usable-CG-range requirement and tail-scrape limit stay
//! diagnostic because the model's own preset aircraft do not all meet them.
//! The optimizer must still not buy mass, drag or cost by degrading either
//! quantity relative to the real aircraft, so a candidate is held to
//! `min(requirement, nominal)`: the requirement where the registered
//! aircraft meets it, the registered aircraft's own modelled value where it
//! does not. Both quantities are evaluated by the same model on the preset
//! design vector, so the comparison cancels the model's bias.
//!
//! The `min` is deliberate. Where the registered aircraft exceeds the
//! requirement, a candidate may give back the margin above the requirement
//! but never go below it: the requirement is what the product promises, and
//! the nominal only lowers the bar where the model says the real aircraft
//! itself does not reach it. The guard therefore forbids degradation below
//! `min(requirement, nominal)`, not every degradation.
//!
//! The same guard runs twice: in the loop against the nominal at the in-loop
//! mesh ([`residuals`]), and on each finalist's reported analysis against
//! the nominal at the reported mesh ([`reporting_relative_balance`]).
//!
//! Units: usable CG range in % MAC, tail-scrape angle in degrees.

use crate::mdo::ResidualRole;

use std::sync::Arc;

use alas_config::AlasConfig;

use crate::envelope::{
    assess_model_cg_envelope_with_ledger, ModelCgConstraint, ModelCgEnvelopeAssessment,
};

use super::super::nominal_cache::NominalCache;
use super::super::sizing::SizingOutcome;
use super::super::types::ConstraintFamily::Balance;
use super::super::types::ConstraintResidual;
use super::balance_ledger::FuelLoad;
use super::{balance_ledger, worst_by_constraint};

/// The two modelled quantities of the registered aircraft's design vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NominalBalance {
    /// Worst-state usable CG range, % MAC.
    pub usable_cg_range_pct_mac: f64,
    /// Tail-scrape angle, degrees.
    pub tail_scrape_deg: f64,
    /// The mean aerodynamic chord the range is a percentage of, m.
    pub mac_m: f64,
}

/// The envelope assessment of one sized candidate at the critical
/// neutral-point condition set of this stage's mesh fidelity.
#[cfg(test)]
pub(super) fn assess(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, String> {
    assess_states(outcome, config).map(|(design, _)| design)
}

/// [`assess`] plus the dispatched route's own loading, when its fuel loads:
/// the takeoff and landing states the reporting verdict evaluates as the
/// flown mission (`feasibility::model_cg`), beside the design loading.
pub(super) fn assess_states(
    outcome: &SizingOutcome,
    config: &AlasConfig,
) -> Result<(ModelCgEnvelopeAssessment, Option<ModelCgEnvelopeAssessment>), String> {
    assess_states_for(outcome, config, FuelLoad::Mission)
}

/// [`assess_states`] under an explicit fuel-loading policy.
pub(super) fn assess_states_for(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    fuel_load: FuelLoad,
) -> Result<(ModelCgEnvelopeAssessment, Option<ModelCgEnvelopeAssessment>), String> {
    let (ledger, flown) = balance_ledger::loading_bases_for(outcome, config, fuel_load)?;
    if !config.requirements.cruise_mach.is_finite()
        || !(0.0..1.0).contains(&config.requirements.cruise_mach)
        || !config.requirements.cruise_altitude_m.is_finite()
    {
        return Err(
            "critical neutral-point condition is outside the finite subsonic domain".to_owned(),
        );
    }
    let conditions = alas_stab::neutral_point::neutral_point_conditions(
        &outcome.plane,
        &config.analysis,
        &alas_stab::neutral_point::NpConditionsInput {
            low_speed_altitude_m: 0.0,
            cruise_mach: config.requirements.cruise_mach,
            cruise_altitude_m: config.requirements.cruise_altitude_m,
            max_operating: config.max_operating_condition(),
            high_lift: alas_stab::neutral_point::HighLiftDevices::declared(
                config.performance.high_lift_chord_extension,
                &config.control_surfaces,
            ),
            jet_inlets: {
                // The candidate cruise polar as a parabola through CL 0 and 0.5.
                let drag = &outcome.sized.fuel_artifacts.drag;
                let (mach, altitude_m) = (
                    config.requirements.cruise_mach,
                    config.requirements.cruise_altitude_m,
                );
                let cd0 = drag.cd(0.0, mach, altitude_m);
                let k = (drag.cd(0.5, mach, altitude_m) - cd0) / 0.25;
                crate::envelope::jet_inlet_spec(
                    config,
                    crate::envelope::InletDragPolar { cd0, k },
                    outcome.plane.s_ref,
                )
            },
            propeller: config.geometry.engine.turboprop.as_ref().and_then(|spec| {
                Some(alas_stab::neutral_point::PropellerSpec {
                    blades: spec.propeller_blade_count?,
                    diameter_m: spec.propeller_diameter_m,
                })
            }),
            wing_stiffness: alas_mass::wing_reconciliation::wing_box_stiffness(
                config,
                &outcome.history.dv,
                &outcome.plane,
            )
            .ok()
            .map(|table| alas_stab::neutral_point::WingStiffness {
                y_m: table.y_m,
                ei_nm2: table.ei_nm2,
                gj_nm2: table.gj_nm2,
                box_axis_fraction: table.box_axis_fraction,
                inertia_relief: Some(alas_stab::neutral_point::InertiaRelief {
                    carried_kg_m: table.carried_kg_m,
                    point_masses_kg: table.point_masses_kg,
                    aircraft_mass_kg: table.aircraft_mass_kg,
                }),
            }),
            tail_flexibility: alas_mass::wing_reconciliation::tail_flexibility(
                config,
                &outcome.history.dv,
                &outcome.plane,
            )
            .ok()
            .map(|t| alas_stab::neutral_point::TailFlexibility {
                fuselage_x_m: t.fuselage_x_m,
                fuselage_ei_nm2: t.fuselage_ei_nm2,
                stabilizer: alas_stab::neutral_point::WingStiffness {
                    y_m: t.stabilizer_y_m,
                    ei_nm2: t.stabilizer_ei_nm2,
                    gj_nm2: t.stabilizer_gj_nm2,
                    box_axis_fraction: t.stabilizer_axis_fraction,
                    inertia_relief: None,
                },
            }),
            ..Default::default()
        },
    )
    .map_err(|error| format!("critical neutral-point conditions: {error:?}"))?;
    // The envelopes of one pair of loading bases. The neutral-point
    // conditions do not depend on where the payload sits, so a load plan
    // re-assesses with the same ones.
    let assess = |(ledger, flown): balance_ledger::LoadingBases| -> Result<States, String> {
        let design = assess_model_cg_envelope_with_ledger(
            &outcome.plane,
            ledger,
            outcome.x_np,
            conditions.critical,
            outcome.mac,
            config,
        )
        .map_err(|error| format!("{error}"))?;
        let flown = flown
            .map(|(basis, landing)| {
                crate::envelope::assess_model_cg_envelope_with_ledger_and_landing(
                    &outcome.plane,
                    basis,
                    Some(landing),
                    outcome.x_np,
                    conditions.critical,
                    outcome.mac,
                    config,
                )
                .map_err(|error| format!("flown loading: {error}"))
            })
            .transpose()?;
        Ok((design, flown))
    };
    let nominal = assess((ledger, flown))?;
    // The report judges the loading its load plan places
    // (`alas_pipeline::full_analysis::load_plan`); the search judges the same.
    Ok(super::load_plan::planned(
        nominal,
        outcome.plane.mac_frame(),
        |steer_x_m| {
            let mut steered = config.clone();
            steered.cabin.cargo.plan_payload_cg_x_m = Some(steer_x_m);
            assess(balance_ledger::loading_bases_for(
                outcome, &steered, fuel_load,
            )?)
        },
    ))
}

/// The design-loading envelope and, when the dispatch fuel loads, the flown
/// one.
pub(super) type States = (ModelCgEnvelopeAssessment, Option<ModelCgEnvelopeAssessment>);

fn measure(assessment: &ModelCgEnvelopeAssessment) -> Option<NominalBalance> {
    // The narrowest usable range of any state: with a mass-indexed requirement
    // the state furthest short of its own requirement need not be the narrowest.
    let range = assessment
        .loading_states
        .iter()
        .flat_map(|state| state.constraints.iter())
        .filter(|state| state.constraint == ModelCgConstraint::MinimumUsableCgRange)
        .map(|state| state.actual)
        .filter(|actual| actual.is_finite())
        .reduce(f64::min)?;
    let scrape = worst_by_constraint(&assessment.loading_states, ModelCgConstraint::TailScrape)?;
    let nominal = NominalBalance {
        usable_cg_range_pct_mac: range,
        mac_m: assessment.mac_m,
        tail_scrape_deg: scrape.actual,
    };
    (nominal.usable_cg_range_pct_mac.is_finite() && nominal.tail_scrape_deg.is_finite())
        .then_some(nominal)
}

/// Size the preset design vector under `config` and measure its balance.
pub(super) fn resolve(config: &AlasConfig) -> Result<NominalBalance, String> {
    let design = alas_config::presets::get(&config.preset)
        .map_err(|error| format!("preset: {error}"))?
        .design_vector;
    let outcome =
        super::super::sizing::run_candidate_with_fuselage_policy(config, &design.to_array(), false)
            .map_err(|failure| failure.reason.to_owned())?;
    // The registered aircraft is loaded as its tanks allow (`FuelLoad`).
    let (assessment, _) = assess_states_for(&outcome, config, FuelLoad::TankLimited)?;
    measure(&assessment).ok_or_else(|| "non-finite nominal balance".to_owned())
}

/// The in-loop nominals, keyed on the complete configuration, each with the
/// reason it could not be resolved.
pub(super) static NOMINAL: NominalCache<Result<NominalBalance, Arc<str>>> = NominalCache::new();

/// The registered aircraft's modelled balance for a run held to it
/// ([`AlasConfig::holds_to_registered_aircraft`]: a reference adaptation, or a
/// clean sheet from a registered preset), or why it could not be resolved;
/// `None` otherwise.
fn nominal_resolution(config: &AlasConfig) -> Option<Result<NominalBalance, Arc<str>>> {
    if !config.holds_to_registered_aircraft() {
        return None;
    }
    NOMINAL.get_or_resolve(config, |config| {
        Some(resolve(config).map_err(|reason| {
            tracing::warn!(
                %reason,
                preset = %config.preset,
                "nominal balance unavailable; every candidate violates relative_balance_nominal_unavailable"
            );
            Arc::from(reason)
        }))
    })
}

/// The registered aircraft's modelled balance for a reference adaptation;
/// `None` in any other mode or when it cannot be resolved.
#[cfg(test)]
pub(super) fn nominal_balance(config: &AlasConfig) -> Option<NominalBalance> {
    nominal_resolution(config)?.ok()
}

/// The relative residuals of one assessed candidate: none without a
/// registered aircraft, else [`residuals_against`] its in-loop nominal.
pub(super) fn residuals(
    assessment: &ModelCgEnvelopeAssessment,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    match nominal_resolution(config) {
        Some(nominal) => residuals_against(
            assessment,
            nominal.as_ref().map_err(|r| &**r).copied(),
            role,
        ),
        None => Vec::new(),
    }
}

/// The two relative residuals against `nominal`, or, when the nominal could
/// not be resolved, one `relative_balance_nominal_unavailable` violation
/// under the requested `role`, carrying the reason as its detail: a failed
/// reference cannot switch a hard guard off, and the flag is diagnostic only
/// under a diagnostic role.
pub(super) fn residuals_against(
    assessment: &ModelCgEnvelopeAssessment,
    nominal: Result<NominalBalance, &str>,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    match nominal {
        Ok(nominal) => relative_residuals(assessment, nominal, role),
        Err(reason) => vec![ConstraintResidual::direct(
            "relative_balance_nominal_unavailable",
            Balance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )
        .with_detail(format!(
            "registered aircraft's nominal balance unavailable: {reason}"
        ))],
    }
}

/// The relative balance residuals of a reported analysis's assessment
/// `candidate` against `nominal`, the registered aircraft's assessment at the
/// same reporting fidelity (`None` when it could not be evaluated), under the
/// hard Balance requirements. Empty unless the run is held to the registered
/// aircraft ([`AlasConfig::holds_to_registered_aircraft`]).
pub fn reporting_relative_balance(
    candidate: &ModelCgEnvelopeAssessment,
    nominal: Option<&ModelCgEnvelopeAssessment>,
    config: &AlasConfig,
) -> Vec<ConstraintResidual> {
    let role = ResidualRole::Constraint;
    if !config.holds_to_registered_aircraft() {
        return Vec::new();
    }
    let nominal = nominal
        .ok_or("registered aircraft not assessed at reporting fidelity")
        .and_then(|nominal| measure(nominal).ok_or("non-finite nominal balance"));
    residuals_against(candidate, nominal, role)
}

/// Each quantity of `assessment` held to `min(requirement, nominal)`.
pub(super) fn relative_residuals(
    assessment: &ModelCgEnvelopeAssessment,
    nominal: NominalBalance,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    [
        (
            "usable_cg_range_vs_nominal",
            ModelCgConstraint::MinimumUsableCgRange,
            nominal.usable_cg_range_pct_mac,
            "% MAC",
        ),
        (
            "tail_scrape_vs_nominal",
            ModelCgConstraint::TailScrape,
            nominal.tail_scrape_deg,
            "deg",
        ),
    ]
    .into_iter()
    .filter_map(|(id, constraint, nominal_value, unit)| {
        // The centre-of-gravity travel the registered aircraft offers is a
        // length: its cabin, holds and tanks are loaded along the same
        // fuselage whatever the candidate's chord. A candidate with a shorter
        // mean aerodynamic chord must therefore offer the same metres, which
        // are more of its own chord (the B747-400 search shrank the chord
        // 10 % and the row asked 10 % less travel of it). Exactly one at the
        // registered design.
        let chord_ratio = if constraint == ModelCgConstraint::MinimumUsableCgRange
            && nominal.mac_m.is_finite()
            && assessment.mac_m.is_finite()
            && nominal.mac_m > 0.0
            && assessment.mac_m > 0.0
        {
            nominal.mac_m / assessment.mac_m
        } else {
            1.0
        };
        // Each state's own requirement is already on the candidate's chord (a
        // published envelope width is converted from metres at its mass; the
        // configured value is a percentage of whatever chord the design has),
        // so only the nominal is rescaled. The row reports the state that falls
        // furthest short of min(its requirement, nominal).
        let (actual, limit) = assessment
            .loading_states
            .iter()
            .flat_map(|state| state.constraints.iter())
            .filter(|candidate| candidate.constraint == constraint)
            .map(|state| (state.actual, state.limit.min(nominal_value * chord_ratio)))
            .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))?;
        Some(ConstraintResidual::scaled(
            id,
            Balance,
            actual,
            limit,
            unit,
            limit - actual,
            role,
        ))
    })
    .collect()
}
