// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Balance and ground-clearance residuals measured against the registered
//! aircraft a reference adaptation redesigns.
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

use alas_config::{AlasConfig, DesignMode};

use crate::envelope::{
    assess_model_cg_envelope_with_ledger, ModelCgConstraint, ModelCgEnvelopeAssessment,
};

use super::super::nominal_cache::NominalCache;
use super::super::sizing::SizingOutcome;
use super::super::types::ConstraintFamily::Balance;
use super::super::types::ConstraintResidual;
use super::{balance_ledger, worst_by_constraint};

/// The two modelled quantities of the registered aircraft's design vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NominalBalance {
    /// Worst-state usable CG range, % MAC.
    pub usable_cg_range_pct_mac: f64,
    /// Tail-scrape angle, degrees.
    pub tail_scrape_deg: f64,
}

/// The envelope assessment of one sized candidate at the critical
/// neutral-point condition set of this stage's mesh fidelity.
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
    let (ledger, flown) = balance_ledger::loading_bases(outcome, config)?;
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
            ..Default::default()
        },
    )
    .map_err(|error| format!("critical neutral-point conditions: {error:?}"))?;
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
}

fn measure(assessment: &ModelCgEnvelopeAssessment) -> Option<NominalBalance> {
    let range = worst_by_constraint(
        &assessment.loading_states,
        ModelCgConstraint::MinimumUsableCgRange,
    )?;
    let scrape = worst_by_constraint(&assessment.loading_states, ModelCgConstraint::TailScrape)?;
    let nominal = NominalBalance {
        usable_cg_range_pct_mac: range.actual,
        tail_scrape_deg: scrape.actual,
    };
    (nominal.usable_cg_range_pct_mac.is_finite() && nominal.tail_scrape_deg.is_finite())
        .then_some(nominal)
}

/// Size the preset design vector under `config` and measure its balance.
fn resolve(config: &AlasConfig) -> Result<NominalBalance, String> {
    let design = alas_config::presets::get(&config.preset)
        .map_err(|error| format!("preset: {error}"))?
        .design_vector;
    let outcome =
        super::super::sizing::run_candidate_with_fuselage_policy(config, &design.to_array(), false)
            .map_err(|failure| failure.reason.to_owned())?;
    measure(&assess(&outcome, config)?).ok_or_else(|| "non-finite nominal balance".to_owned())
}

/// The in-loop nominals, keyed on the complete configuration.
pub(super) static NOMINAL: NominalCache<NominalBalance> = NominalCache::new();

/// The registered aircraft's modelled balance for a reference adaptation;
/// `None` in any other mode or when it cannot be resolved.
pub(super) fn nominal_balance(config: &AlasConfig) -> Option<NominalBalance> {
    if config.optimizer.design_space.mode != DesignMode::ReferenceAdaptation {
        return None;
    }
    NOMINAL.get_or_resolve(config, |config| {
        resolve(config)
            .map_err(|reason| {
                tracing::warn!(
                    %reason,
                    preset = %config.preset,
                    "nominal balance unavailable; every candidate violates relative_balance_nominal_unavailable"
                );
            })
            .ok()
    })
}

/// The relative residuals of one assessed candidate: none without a
/// registered aircraft, else [`residuals_against`] its in-loop nominal.
pub(super) fn residuals(
    assessment: &ModelCgEnvelopeAssessment,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if config.optimizer.design_space.mode != DesignMode::ReferenceAdaptation {
        return Vec::new();
    }
    residuals_against(assessment, nominal_balance(config), role)
}

/// The two relative residuals against `nominal`, or, when the nominal could
/// not be resolved, one `relative_balance_nominal_unavailable` violation
/// under the requested `role`: a failed reference cannot switch a hard
/// guard off, and the flag is diagnostic only under a diagnostic role.
pub(super) fn residuals_against(
    assessment: &ModelCgEnvelopeAssessment,
    nominal: Option<NominalBalance>,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    match nominal {
        Some(nominal) => relative_residuals(assessment, nominal, role),
        None => vec![ConstraintResidual::direct(
            "relative_balance_nominal_unavailable",
            Balance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )],
    }
}

/// The relative balance residuals of a reported analysis's assessment
/// `candidate` against `nominal`, the registered aircraft's assessment at the
/// same reporting fidelity (`None` when it could not be evaluated), under the
/// hard Balance requirements. Empty outside a reference adaptation.
pub fn reporting_relative_balance(
    candidate: &ModelCgEnvelopeAssessment,
    nominal: Option<&ModelCgEnvelopeAssessment>,
    config: &AlasConfig,
) -> Vec<ConstraintResidual> {
    let role = ResidualRole::Constraint;
    if config.optimizer.design_space.mode != DesignMode::ReferenceAdaptation {
        return Vec::new();
    }
    residuals_against(candidate, nominal.and_then(measure), role)
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
        let worst = worst_by_constraint(&assessment.loading_states, constraint)?;
        let limit = worst.limit.min(nominal_value);
        Some(ConstraintResidual::scaled(
            id,
            Balance,
            worst.actual,
            limit,
            unit,
            limit - worst.actual,
            role,
        ))
    })
    .collect()
}
