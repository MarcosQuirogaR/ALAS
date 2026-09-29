// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build the typed model CG envelope assessment and bridge its per-state
//! constraints to top-level findings: a real `mod`, not folded into
//! `feasibility.rs`'s own frozen `docs/source-size-budgets.tsv` ceiling.

use alas_mass::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR,
    H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_opt::{
    assess_model_cg_envelope, assess_model_cg_envelope_with_ledger, LedgerLoadingBasis,
    ModelCgConstraint, ModelCgEnvelopeAssessment,
};

use super::{error, warning, FindingCode, PhysicalFinding};
use crate::full_analysis::AnalysisReport;
use alas_config::AlasConfig;

use super::{FuelLoadingAssessment, MassBalanceAssessment};

/// The ledger's OEW/zero-fuel/flown-takeoff states, in the
/// shape [`assess_model_cg_envelope_with_ledger`] needs, when `mass_balance`
/// carries at least those three named states (see
/// `crates/alas-pipeline/src/feasibility/mass_balance.rs`'s own
/// `"operating empty"`/`"zero fuel"`/`"flown takeoff"` labels, always the
/// first three entries when present). `None` when no ledger was built (no
/// station/tank/FLOPS-grouping failure aside) or its published state order
/// ever changes shape, so this stays a plain type conversion rather than a
/// second, drifting definition of which index means what.
fn ledger_loading_basis(mass_balance: &MassBalanceAssessment) -> Option<LedgerLoadingBasis> {
    let oew = mass_balance
        .states
        .iter()
        .find(|state| state.label == "operating empty")?;
    let zero_fuel = mass_balance
        .states
        .iter()
        .find(|state| state.label == "zero fuel")?;
    let takeoff = mass_balance
        .states
        .iter()
        .find(|state| state.label == "flown takeoff")?;
    Some(LedgerLoadingBasis {
        oew_mass_kg: oew.mass_kg,
        oew_cg_x_m: oew.cg_m[0],
        oew_cg_z_m: oew.cg_m[2],
        zero_fuel_mass_kg: zero_fuel.mass_kg,
        zero_fuel_cg_x_m: zero_fuel.cg_m[0],
        zero_fuel_cg_z_m: zero_fuel.cg_m[2],
        takeoff_mass_kg: takeoff.mass_kg,
        takeoff_cg_x_m: takeoff.cg_m[0],
        takeoff_cg_z_m: takeoff.cg_m[2],
    })
}

pub(super) fn model_cg_assessment(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    mass_balance: Option<&MassBalanceAssessment>,
) -> Result<ModelCgEnvelopeAssessment, String> {
    let mass = |name: &str| {
        report
            .component_masses
            .get(name)
            .copied()
            .ok_or_else(|| format!("model CG assessment is missing {name} mass"))
    };
    let coordinate = |name: &str| {
        report
            .mass_coordinates
            .get(name)
            .copied()
            .ok_or_else(|| format!("model CG assessment is missing {name} coordinates"))
    };
    let mut masses = MassBreakdown {
        wing: mass(WING)?,
        h_stab: mass(H_STAB)?,
        v_stab: mass(V_STAB)?,
        fuselage: mass(FUSELAGE)?,
        gear: mass(GEAR)?,
        propulsion: mass(PROPULSION)?,
        systems: mass(SYSTEMS)?,
        furnishings: mass(FURNISHINGS)?,
        payload: mass(PAYLOAD)?,
        fuel: mass(FUEL)?,
    };
    let coordinates = MassCoordinates {
        wing: coordinate(WING)?,
        h_stab: coordinate(H_STAB)?,
        v_stab: coordinate(V_STAB)?,
        fuselage: coordinate(FUSELAGE)?,
        gear: coordinate(GEAR)?,
        propulsion: coordinate(PROPULSION)?,
        systems: coordinate(SYSTEMS)?,
        furnishings: coordinate(FURNISHINGS)?,
        payload: coordinate(PAYLOAD)?,
        fuel: coordinate(FUEL)?,
    };
    masses.fuel = fuel_loading.analyzed_carried_fuel_kg;
    let analyzed_cg = calculate_physical_cg(&masses, &coordinates);
    // The critical (most-forward) neutral point when available, else clean.
    let critical_x_np = report
        .neutral_point_conditions
        .as_ref()
        .map_or(report.x_neutral_point, |conditions| conditions.critical);
    // Evaluate the hard gate on the item-level mass ledger's
    // own OEW/ZFW/TOW points when the ledger exists (tank fill order,
    // detailed payload); the lumped ten-group model above is only the
    // fallback basis when no ledger could be built, so the report's own mass
    // statement and the feasibility verdict describe the same centre of
    // gravity.
    if let Some(ledger) = mass_balance.and_then(ledger_loading_basis) {
        return assess_model_cg_envelope_with_ledger(
            &report.airplane,
            ledger,
            report.x_neutral_point,
            critical_x_np,
            report.airplane.c_ref,
            config,
        )
        .map_err(|error| error.to_string());
    }
    assess_model_cg_envelope(
        &report.airplane,
        &masses,
        &coordinates,
        analyzed_cg[0],
        report.x_neutral_point,
        critical_x_np,
        report.airplane.c_ref,
        config,
    )
    .map_err(|error| error.to_string())
}

pub(super) fn append_model_cg_findings(
    findings: &mut Vec<PhysicalFinding>,
    assessment: &ModelCgEnvelopeAssessment,
) {
    // Every new constraint maps to the closest existing `FindingCode`
    // (exhaustively matched by `alas-gui`, which must not gain a variant).
    let constraints = [
        ModelCgConstraint::StaticStabilityFloor,
        ModelCgConstraint::PhysicalForwardCgLimit,
        ModelCgConstraint::MinimumUsableCgRange,
        ModelCgConstraint::NoseGearStrength,
        ModelCgConstraint::MainGearStrength,
        ModelCgConstraint::MinimumNoseGearLoad,
        ModelCgConstraint::MaximumNoseGearLoadFraction,
        ModelCgConstraint::TipBack,
        ModelCgConstraint::TailScrape,
    ];
    for constraint in constraints {
        let worst = assessment
            .loading_states
            .iter()
            .flat_map(|state| {
                state
                    .constraints
                    .iter()
                    .filter(move |item| item.constraint == constraint && item.violated)
                    .map(move |item| (state.state, item))
            })
            .max_by(|(_, left), (_, right)| {
                left.normalized_exceedance
                    .total_cmp(&right.normalized_exceedance)
            });
        let Some((state, result)) = worst else {
            continue;
        };
        let code = match constraint {
            ModelCgConstraint::StaticStabilityFloor => FindingCode::InsufficientStaticMargin,
            ModelCgConstraint::PhysicalForwardCgLimit | ModelCgConstraint::MinimumUsableCgRange => {
                FindingCode::ModelCgForwardRangeViolation
            }
            ModelCgConstraint::NoseGearStrength
            | ModelCgConstraint::MaximumNoseGearLoadFraction => {
                FindingCode::NoseGearStrengthViolation
            }
            ModelCgConstraint::MainGearStrength => FindingCode::MainGearStrengthViolation,
            // `TipBack` and `TailScrape` are ground-clearance/rotation
            // geometry failures at the aft CG limit, not a nose-load
            // steering-authority shortfall: a physically distinct failure
            // mode from `MinimumNoseGearLoad`. `alas-gui`'s
            // `views/results_view/summary/findings.rs` matches
            // `FindingCode` exhaustively (a finding the pipeline can raise
            // but this page cannot explain is a compile error), so this
            // lane may not add a dedicated variant; `MinimumNoseGearLoadViolation`
            // is reused as the closest existing code (both are aft-CG-limit
            // gear findings), with the message below naming the actual
            // mechanism so a reader is never told "nose load" when the
            // failure is tail-scrape or tip-back clearance.
            ModelCgConstraint::MinimumNoseGearLoad
            | ModelCgConstraint::TipBack
            | ModelCgConstraint::TailScrape => FindingCode::MinimumNoseGearLoadViolation,
        };
        let message = match constraint {
            ModelCgConstraint::TipBack | ModelCgConstraint::TailScrape => format!(
                "{} loading state violates the model {} constraint (a ground-clearance/\
                 rotation geometry limit, reported under the minimum-nose-gear-load finding \
                 code because alas-gui's FindingCode match has no dedicated tip-back/tail-\
                 scrape variant; this is not a nose-load steering-authority shortfall)",
                state.label(),
                constraint.label()
            ),
            // `cg_range_pct_mac` is a configured/assumed requirement (a
            // load-and-trim-sheet convention, default 30 %MAC), not a
            // physical limit like the aerodynamic/ground/tip-back
            // boundaries it is checked against: an aircraft with a
            // narrower-than-configured usable range is still physically
            // flyable within it, so this must warn, not reject. The
            // message states both the usable range this design's physical
            // boundaries actually admit and the configured value it fell
            // short of, so a reader can tell the assumption from the
            // physics without cross-referencing the envelope assessment.
            ModelCgConstraint::MinimumUsableCgRange => format!(
                "{} loading state's usable CG range is {:.2} % MAC, below the configured \
                 requirement of {:.2} % MAC (cg_range_pct_mac; a configured assumption, not \
                 a physical limit -- the physical forward/aft boundaries themselves are \
                 unaffected)",
                state.label(),
                result.actual,
                result.limit
            ),
            _ => format!(
                "{} loading state violates the model {} constraint",
                state.label(),
                constraint.label()
            ),
        };
        let push = if constraint.is_diagnostic() {
            warning
        } else {
            error
        };
        findings.push(push(
            code,
            message,
            Some(result.actual),
            Some(result.limit),
            constraint.unit(),
        ));
    }
    // The aft-boundary governance diagnostic that explains a nose-load
    // violation is carried on the typed assessment itself
    // (`ModelCgEnvelopeAssessment::aft_limit_governance`) and reported by
    // `report_format` and `acceptance`. It is deliberately not a
    // `PhysicalFinding`: `FindingCode` is an interface whose exhaustive
    // consumers live outside this crate's ownership boundary, and the
    // diagnostic changes no verdict: a layout whose gear cannot carry the
    // envelope still fails `MinimumNoseGearLoadViolation` above.
}

// Fixtures are registered presets; a failed unwrap/expect is the assertion
// failing, not a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::{assess_physical_feasibility, FindingSeverity};

    /// The A220-300 preset's configured `cg_range_pct_mac` (30 %MAC default)
    /// exceeds its physical usable range, per the probe that found this
    /// defect. That must raise `MinimumNoseGearLoadViolation`... no: it must
    /// raise `ModelCgForwardRangeViolation` at `Warning` severity (a
    /// configured assumption, not a physical limit), never `Error`, and the
    /// message must name both the usable range and the configured value.
    #[test]
    fn a220_300_minimum_usable_cg_range_is_a_warning_not_an_error() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" }))
            .expect("A220-300 preset config");
        let preset = alas_config::presets::get("A220-300").expect("registered preset");
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("A220-300 must analyze");
        let feasibility =
            assess_physical_feasibility(&config, &preset.design_vector, &report, None);
        let finding = feasibility
            .findings
            .iter()
            .find(|finding| {
                finding.code == FindingCode::ModelCgForwardRangeViolation
                    && finding.message.contains("usable CG range")
            })
            .expect("the A220-300 preset raises the minimum-usable-cg-range finding");
        assert_eq!(finding.severity, FindingSeverity::Warning);
        assert!(finding.message.contains("configured requirement"));
        assert!(!feasibility.findings.iter().any(|other| {
            other.code == FindingCode::ModelCgForwardRangeViolation
                && other.message.contains("usable CG range")
                && other.severity == FindingSeverity::Error
        }));
    }

    /// When the ledger exists, the hard gate's analyzed-TOW
    /// centre of gravity must be the ledger's own point (the tank-fill-order,
    /// detailed-payload state the mass statement reports), not the lumped
    /// ten-group model's -- the A220-300 is the example of the two
    /// paths disagreeing by several percent MAC.
    #[test]
    fn a220_300_hard_gate_uses_the_ledgers_takeoff_cg_not_the_lumped_one() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" }))
            .expect("A220-300 preset config");
        let preset = alas_config::presets::get("A220-300").expect("registered preset");
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("A220-300 must analyze");
        let feasibility =
            assess_physical_feasibility(&config, &preset.design_vector, &report, None);
        let mass_balance = feasibility
            .mass_balance
            .as_ref()
            .expect("A220-300 builds an item-level mass ledger");
        let ledger_takeoff_pct_mac = mass_balance
            .states
            .iter()
            .find(|state| state.label == "flown takeoff")
            .expect("the ledger reports a flown-takeoff state")
            .cg_pct_mac;
        let gate_takeoff_pct_mac = feasibility
            .model_cg
            .as_ref()
            .expect("A220-300 produces a model CG assessment")
            .loading_states
            .iter()
            .find(|state| state.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
            .expect("the gate reports an analyzed-takeoff state")
            .cg_pct_mac;
        assert!(
            (gate_takeoff_pct_mac - ledger_takeoff_pct_mac).abs() < 0.5,
            "gate {gate_takeoff_pct_mac} should track the ledger {ledger_takeoff_pct_mac}, not \
             the lumped model {}",
            mass_balance.lumped_takeoff_cg_pct_mac
        );
        assert!(
            (mass_balance.lumped_takeoff_cg_pct_mac - ledger_takeoff_pct_mac).abs() > 1.0,
            "this preset must still be the case where the lumped model disagrees with the \
             ledger, or this test is not exercising the fix"
        );
    }
}
