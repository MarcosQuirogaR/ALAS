// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit physical-feasibility findings for a completed design run.
//!
//! A solver completing is not evidence that the aircraft can fly the stated
//! load case. The pipeline therefore carries violated conservation laws and
//! configured limits beside its numerical outputs. Keeping the measured value,
//! limit and unit makes a finding diagnosable without converting it into an
//! arbitrary scalar penalty or hiding it behind a single Boolean.

use alas_config::{
    presets, AircraftReferenceData, AlasConfig, CgEnvelopeCondition, CgEnvelopeEvidence,
    CgEnvelopeSource, DesignVector, PlanningMacReference,
};
use alas_mass::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR,
    H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_mission::MissionResult;
use alas_opt::{assess_model_cg_envelope, ModelCgConstraint, ModelCgEnvelopeAssessment};

use crate::full_analysis::AnalysisReport;

mod cruise_equilibrium;
mod fuel;
mod report_format;

pub(crate) use cruise_equilibrium::assess as assess_cruise_equilibrium;
pub use cruise_equilibrium::CruiseEquilibriumAssessment;
pub(crate) use fuel::plan_fuel_loading;
pub use fuel::{
    CarriedFuelBasis, FuelCapacityAssessment, FuelCapacityEvidence, FuelLoadingAssessment,
    MissionFuelAssessment, MissionFuelStatus,
};

/// Stable identifier for one physical failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingCode {
    /// Cruise aerodynamics did not produce a positive finite efficiency.
    InvalidCruiseAerodynamics,
    /// The MTOW mass closure left no positive fuel.
    NonPositiveFuel,
    /// Usable capacity limits the load case below the configured MTOW.
    TankLimitedTakeoffMass,
    /// No usable-fuel capacity could be established for the analyzed design.
    FuelCapacityUnavailable,
    /// Frozen compatibility code for the former untyped envelope Boolean.
    CgEnvelopeViolation,
    /// The typed model CG assessment could not be constructed.
    ModelCgAssessmentUnavailable,
    /// A loading state lies forward of the configured model CG range.
    ModelCgForwardRangeViolation,
    /// A loading state exceeds the modeled nose-gear tire capacity.
    NoseGearStrengthViolation,
    /// A loading state exceeds the modeled main-gear tire capacity.
    MainGearStrengthViolation,
    /// A loading state carries too little nose load for steering authority.
    MinimumNoseGearLoadViolation,
    /// The analyzed point lies outside a public manufacturer planning envelope.
    PublicPlanningCgEnvelopeViolation,
    /// The cruise trim solve did not produce a finite result.
    TrimUnavailable,
    /// Static margin is below the configured physical floor.
    InsufficientStaticMargin,
    /// The built wing reference area exceeds its configured maximum.
    WingAreaLimit,
    /// A requested mission produced no telemetry.
    MissionUnavailable,
    /// At least one mission segment did not converge.
    MissionNotConverged,
    /// Mission fuel burn is not a positive finite quantity.
    InvalidMissionFuelBurn,
    /// Mission fuel burn exceeds the fuel carried in the analyzed load case.
    MissionFuelShortfall,
    /// A cruise force record contains a non-finite result.
    InvalidCruiseForceBalance,
}

/// Severity of a physical finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    /// The configured aircraft or load case is not physically feasible.
    Error,
    /// The result is usable but requires engineering attention.
    Warning,
}

/// One failed physical check with its measured and limiting values.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalFinding {
    /// Machine-stable failure identifier.
    pub code: FindingCode,
    /// Whether the finding invalidates the load case.
    pub severity: FindingSeverity,
    /// Human-readable explanation.
    pub message: String,
    /// Measured value, when the check is quantitative.
    pub actual: Option<f64>,
    /// Governing limit, when the check is quantitative.
    pub limit: Option<f64>,
    /// Unit shared by `actual` and `limit`.
    pub unit: &'static str,
}

/// Result of comparing one analyzed point with public CG reference evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanningCgStatus {
    /// No public planning curve was evaluated.
    #[default]
    NotEvaluated,
    /// The point lies between both published planning limits.
    WithinPublishedLimits,
    /// The point is forward of the published planning limit.
    ForwardLimitViolation,
    /// The point is aft of the published planning limit.
    AftLimitViolation,
    /// A forward limit exists at this mass, but the source omits an aft limit.
    AftLimitNotPublished,
}

/// Provenance and result of the public planning-envelope comparison.
///
/// This assessment is intentionally separate from the model-derived CG and
/// landing-gear check. A public planning curve is preliminary design evidence;
/// the actual aircraft WBM remains the operational authority.
#[derive(Debug, Clone, PartialEq)]
pub struct CgEnvelopeAssessment {
    /// Kind of source evidence registered for the selected preset.
    pub evidence: CgEnvelopeEvidence,
    /// Outcome of the planning-curve comparison, if one was possible.
    pub planning_status: PlanningCgStatus,
    /// Analyzed aircraft mass, in kilograms.
    pub mass_kg: Option<f64>,
    /// Analyzed longitudinal CG in the manufacturer planning frame, in percent MAC.
    pub cg_pct_mac: Option<f64>,
    /// Interpolated public planning forward limit, in percent MAC.
    pub forward_limit_pct_mac: Option<f64>,
    /// Interpolated public planning aft limit, in percent MAC.
    pub aft_limit_pct_mac: Option<f64>,
    /// Exact source location of the planning curve, when one is registered.
    pub source: Option<CgEnvelopeSource>,
    /// Document that controls actual-aircraft dispatch and loading.
    pub controlling_document: Option<&'static str>,
}

impl Default for CgEnvelopeAssessment {
    fn default() -> Self {
        Self {
            evidence: CgEnvelopeEvidence::Unknown,
            planning_status: PlanningCgStatus::NotEvaluated,
            mass_kg: None,
            cg_pct_mac: None,
            forward_limit_pct_mac: None,
            aft_limit_pct_mac: None,
            source: None,
            controlling_document: None,
        }
    }
}

/// Physical status attached to every completed pipeline result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeasibilityReport {
    /// Every failed check; an empty list means all implemented checks passed.
    pub findings: Vec<PhysicalFinding>,
    /// Public CG evidence and planning-only comparison for the selected preset.
    pub cg_envelope: CgEnvelopeAssessment,
    /// Typed hard model constraints, separate from public planning evidence.
    pub model_cg: Option<ModelCgEnvelopeAssessment>,
    /// Fuel budget, capacity, actual load, takeoff mass, and mission burn.
    pub fuel_loading: FuelLoadingAssessment,
    /// Explicit cruise force-balance evidence from the flown mission.
    pub cruise_equilibrium: Option<CruiseEquilibriumAssessment>,
}

impl FeasibilityReport {
    /// Whether no error-severity physical finding was recorded.
    pub fn is_feasible(&self) -> bool {
        self.findings
            .iter()
            .all(|finding| finding.severity != FindingSeverity::Error)
    }

    /// Whether this report contains a particular failure mode.
    pub fn contains(&self, code: FindingCode) -> bool {
        self.findings.iter().any(|finding| finding.code == code)
    }
}

pub use report_format::format_feasibility;

fn error(
    code: FindingCode,
    message: impl Into<String>,
    actual: Option<f64>,
    limit: Option<f64>,
    unit: &'static str,
) -> PhysicalFinding {
    PhysicalFinding {
        code,
        severity: FindingSeverity::Error,
        message: message.into(),
        actual,
        limit,
        unit,
    }
}

fn model_cg_assessment(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
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
    assess_model_cg_envelope(
        &report.airplane,
        &masses,
        &coordinates,
        analyzed_cg[0],
        report.x_neutral_point,
        report.airplane.c_ref,
        config,
    )
    .map_err(|error| error.to_string())
}

fn append_model_cg_findings(
    findings: &mut Vec<PhysicalFinding>,
    assessment: &ModelCgEnvelopeAssessment,
) {
    let constraints = [
        ModelCgConstraint::StaticStabilityFloor,
        ModelCgConstraint::ConfiguredForwardCgRange,
        ModelCgConstraint::NoseGearStrength,
        ModelCgConstraint::MainGearStrength,
        ModelCgConstraint::MinimumNoseGearLoad,
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
            ModelCgConstraint::ConfiguredForwardCgRange => {
                FindingCode::ModelCgForwardRangeViolation
            }
            ModelCgConstraint::NoseGearStrength => FindingCode::NoseGearStrengthViolation,
            ModelCgConstraint::MainGearStrength => FindingCode::MainGearStrengthViolation,
            ModelCgConstraint::MinimumNoseGearLoad => FindingCode::MinimumNoseGearLoadViolation,
        };
        findings.push(error(
            code,
            format!(
                "{} loading state violates the model {} constraint",
                state.label(),
                constraint.label()
            ),
            Some(result.actual),
            Some(result.limit),
            constraint.unit(),
        ));
    }
}

/// Evaluate conservation laws and configured limits on a completed run.
pub fn assess_physical_feasibility(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    mission: Option<&MissionResult>,
) -> FeasibilityReport {
    let mut findings = Vec::new();
    let lift_to_drag = report.design_point.l_over_d;
    if !lift_to_drag.is_finite() || lift_to_drag <= 0.0 {
        findings.push(error(
            FindingCode::InvalidCruiseAerodynamics,
            "cruise analysis did not produce a positive finite lift-to-drag ratio",
            Some(lift_to_drag),
            Some(0.0),
            "dimensionless",
        ));
    }

    let mut fuel_loading = plan_fuel_loading(config, design, report);
    let cruise_equilibrium = mission.map(assess_cruise_equilibrium);
    if let Some(assessment) = &cruise_equilibrium {
        if !assessment.is_finite() {
            findings.push(error(
                FindingCode::InvalidCruiseForceBalance,
                "cruise force-balance record contains no finite solved control point",
                None,
                None,
                "",
            ));
        }
    }
    // Public planning limits apply to the load that is actually carried. A
    // mass-closure remainder can exceed the usable tank capacity, so using
    // `report.physical_cg` here would compare a capped mass case with a CG
    // that still contains the uncarried fuel remainder.
    let cg_envelope =
        assess_public_cg_reference(config, report, fuel_loading.analyzed_carried_fuel_kg);
    fuel_loading.mission = fuel::assess_mission_fuel(config.mission.enabled, mission);
    findings.extend(fuel::findings(config.requirements.mtow_kg, &fuel_loading));

    let model_cg = match model_cg_assessment(config, report, &fuel_loading) {
        Ok(assessment) => {
            append_model_cg_findings(&mut findings, &assessment);
            Some(assessment)
        }
        Err(message) => {
            findings.push(error(
                FindingCode::ModelCgAssessmentUnavailable,
                message,
                None,
                None,
                "",
            ));
            None
        }
    };

    if matches!(
        cg_envelope.planning_status,
        PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation
    ) {
        let limit = match cg_envelope.planning_status {
            PlanningCgStatus::ForwardLimitViolation => cg_envelope.forward_limit_pct_mac,
            PlanningCgStatus::AftLimitViolation => cg_envelope.aft_limit_pct_mac,
            _ => None,
        };
        findings.push(error(
            FindingCode::PublicPlanningCgEnvelopeViolation,
            "analyzed CG lies outside the manufacturer public planning envelope; actual aircraft WBM controls",
            cg_envelope.cg_pct_mac,
            limit,
            "% MAC",
        ));
    }

    let trim_is_finite = report.trimmed_design_point.as_ref().is_some_and(|trim| {
        trim.alpha_deg.is_finite()
            && trim.geometric_body_alpha_deg.is_finite()
            && trim.trim_ih_deg.is_finite()
            && trim.cl.is_finite()
            && trim.cd.is_finite()
            && trim.cm_residual.is_finite()
    });
    if !trim_is_finite {
        findings.push(error(
            FindingCode::TrimUnavailable,
            "cruise trim did not produce a finite solved operating point",
            None,
            None,
            "dimensionless",
        ));
    }

    let projected_area_m2 = report
        .airplane
        .wings
        .first()
        .map_or(f64::NAN, alas_geom::aircraft::wing::Wing::projected_area);
    let area_roundoff_m2 = config.requirements.max_wing_area_m2.abs().max(1.0) * 1.0e-12;
    if !projected_area_m2.is_finite()
        || projected_area_m2 > config.requirements.max_wing_area_m2 + area_roundoff_m2
    {
        findings.push(error(
            FindingCode::WingAreaLimit,
            "projected wing reference area exceeds the configured maximum",
            Some(projected_area_m2),
            Some(config.requirements.max_wing_area_m2),
            "m^2",
        ));
    }

    if config.mission.enabled {
        match mission {
            None => findings.push(error(
                FindingCode::MissionUnavailable,
                "mission analysis was requested but produced no telemetry",
                None,
                None,
                "",
            )),
            Some(result) => {
                if let Some(exhaustion) = &result.fuel_exhaustion {
                    findings.push(error(
                        FindingCode::MissionFuelShortfall,
                        format!(
                            "usable fuel was exhausted during mission segment {}",
                            exhaustion.segment_tag
                        ),
                        Some(exhaustion.burned_fuel_kg),
                        Some(exhaustion.available_fuel_kg),
                        "kg",
                    ));
                }
                if result.solutions.is_empty()
                    || result.solutions.iter().any(|solution| !solution.converged)
                {
                    findings.push(error(
                        FindingCode::MissionNotConverged,
                        "at least one native mission segment did not converge",
                        None,
                        None,
                        "",
                    ));
                }
                let burned_kg = fuel_loading
                    .mission
                    .burned_fuel_kg
                    .unwrap_or_else(|| result.fuel_burned_kg());
                if !burned_kg.is_finite() || burned_kg <= 0.0 {
                    findings.push(error(
                        FindingCode::InvalidMissionFuelBurn,
                        "mission fuel burn is not a positive finite quantity",
                        Some(burned_kg),
                        Some(0.0),
                        "kg",
                    ));
                } else if result.fuel_exhaustion.is_none()
                    && fuel_loading.analyzed_carried_fuel_kg.is_finite()
                    && burned_kg > fuel_loading.analyzed_carried_fuel_kg
                {
                    findings.push(error(
                        FindingCode::MissionFuelShortfall,
                        "mission fuel burn exceeds the fuel carried in this load case",
                        Some(burned_kg),
                        Some(fuel_loading.analyzed_carried_fuel_kg),
                        "kg",
                    ));
                }
            }
        }
    }

    FeasibilityReport {
        findings,
        cg_envelope,
        model_cg,
        fuel_loading,
        cruise_equilibrium,
    }
}

fn assess_public_cg_reference(
    config: &AlasConfig,
    report: &AnalysisReport,
    analyzed_carried_fuel_kg: f64,
) -> CgEnvelopeAssessment {
    let Ok(preset) = presets::get(&config.preset) else {
        return CgEnvelopeAssessment::default();
    };
    let mass_and_cg = preset.reference.planning_cg_envelope.and_then(|envelope| {
        analyzed_mass_and_cg_pct_mac(report, envelope.mac_reference, analyzed_carried_fuel_kg)
    });
    assess_reference_limits(&preset.reference, mass_and_cg)
}

fn analyzed_mass_and_cg_pct_mac(
    report: &AnalysisReport,
    mac_reference: PlanningMacReference,
    analyzed_carried_fuel_kg: f64,
) -> Option<(f64, f64)> {
    let names = [
        alas_mass::breakdown::WING,
        alas_mass::breakdown::H_STAB,
        alas_mass::breakdown::V_STAB,
        alas_mass::breakdown::FUSELAGE,
        alas_mass::breakdown::GEAR,
        alas_mass::breakdown::PROPULSION,
        alas_mass::breakdown::SYSTEMS,
        alas_mass::breakdown::FURNISHINGS,
        alas_mass::breakdown::PAYLOAD,
    ];
    let fuel_name = alas_mass::breakdown::FUEL;
    if !analyzed_carried_fuel_kg.is_finite() || analyzed_carried_fuel_kg < 0.0 {
        return None;
    }
    let mut mass_kg = analyzed_carried_fuel_kg;
    let mut moment_x_kg_m = 0.0;
    for name in names {
        let component_mass = report.component_masses.get(name).copied()?;
        let coordinate = report.mass_coordinates.get(name).copied()?;
        if !component_mass.is_finite() || !coordinate[0].is_finite() {
            return None;
        }
        mass_kg += component_mass.max(0.0);
        moment_x_kg_m += component_mass.max(0.0) * coordinate[0];
    }
    let fuel_coordinate = report.mass_coordinates.get(fuel_name).copied()?;
    if !fuel_coordinate[0].is_finite() {
        return None;
    }
    moment_x_kg_m += analyzed_carried_fuel_kg * fuel_coordinate[0];

    if !mass_kg.is_finite() || mass_kg <= 0.0 || !moment_x_kg_m.is_finite() {
        return None;
    }
    let cg_pct_mac = planning_cg_pct_mac(moment_x_kg_m / mass_kg, mac_reference)?;
    cg_pct_mac.is_finite().then_some((mass_kg, cg_pct_mac))
}

fn planning_cg_pct_mac(
    cg_from_aircraft_nose_m: f64,
    reference: PlanningMacReference,
) -> Option<f64> {
    if !cg_from_aircraft_nose_m.is_finite()
        || !reference.lemac_from_aircraft_nose_m.is_finite()
        || !reference.mean_aerodynamic_chord_m.is_finite()
        || reference.mean_aerodynamic_chord_m <= 0.0
    {
        return None;
    }
    Some(
        100.0 * (cg_from_aircraft_nose_m - reference.lemac_from_aircraft_nose_m)
            / reference.mean_aerodynamic_chord_m,
    )
}

fn assess_reference_limits(
    reference: &AircraftReferenceData,
    mass_and_cg: Option<(f64, f64)>,
) -> CgEnvelopeAssessment {
    let mut assessment = CgEnvelopeAssessment {
        evidence: reference.cg_evidence,
        ..CgEnvelopeAssessment::default()
    };
    if reference.cg_evidence != CgEnvelopeEvidence::PublicPlanning {
        return assessment;
    }
    let Some(envelope) = reference.planning_cg_envelope else {
        return assessment;
    };
    assessment.source = Some(envelope.source);
    assessment.controlling_document = Some(envelope.controlling_document);

    let Some((mass_kg, cg_pct_mac)) = mass_and_cg else {
        return assessment;
    };
    assessment.mass_kg = Some(mass_kg);
    assessment.cg_pct_mac = Some(cg_pct_mac);

    let Some(limits) = envelope.limits_at(CgEnvelopeCondition::Flight, mass_kg) else {
        return assessment;
    };
    assessment.forward_limit_pct_mac = Some(limits.forward_pct_mac);
    assessment.aft_limit_pct_mac = limits.aft_pct_mac;
    assessment.planning_status = if cg_pct_mac < limits.forward_pct_mac {
        PlanningCgStatus::ForwardLimitViolation
    } else if let Some(aft_limit) = limits.aft_pct_mac {
        if cg_pct_mac > aft_limit {
            PlanningCgStatus::AftLimitViolation
        } else {
            PlanningCgStatus::WithinPublishedLimits
        }
    } else {
        PlanningCgStatus::AftLimitNotPublished
    };
    assessment
}

// The registry lookups are test preconditions: a missing shipped preset is the
// failure being reported, rather than a recoverable library condition.
#[allow(clippy::expect_used)]
#[cfg(test)]
#[path = "feasibility_tests.rs"]
mod tests;
