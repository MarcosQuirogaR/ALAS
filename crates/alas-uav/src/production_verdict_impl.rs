// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled calculations behind the UAV production-verdict boundary.
//!
//! Positive pitching moment is nose-up and positive lift coefficient is
//! upward, so longitudinal static stability requires `dCm/dCL < 0`: an
//! increase in lift must create a restoring nose-down moment. The production
//! geometry has no elevator primitive. Pitch trim is therefore claimed only
//! for an explicitly evidenced trimmable-horizontal-tail incidence range.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::{self, VlmResult};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;

use crate::catalog::{Catalog, ComponentKind};
use crate::feasibility::{evaluate, Finding, FindingKind, Severity, UavReport};
use crate::optimizer::OptimizedUav;
use crate::shared_core::{
    assess_generated_geometry_with_shared_core, SharedCoreAssessment, SharedCoreFailure,
    SharedCoreInputs,
};

const HORIZONTAL_TAIL_NAME: &str = "UAV Horizontal Tail";
const TRIM_MOMENT_COEFFICIENT_TOLERANCE: f64 = 1.0e-8;
const MAXIMUM_TRIM_ITERATIONS: usize = 48;

/// Explicitly evidenced incidence range of a trimmable horizontal tail.
#[derive(Debug, Clone, PartialEq)]
pub struct HorizontalTailTrimAuthority {
    /// Lower signed horizontal-tail incidence permitted, in degrees.
    pub minimum_incidence_deg: f64,
    /// Upper signed horizontal-tail incidence permitted, in degrees.
    pub maximum_incidence_deg: f64,
    /// Source identifying the tested or analysed actuator and incidence range.
    pub evidence: String,
}

/// Inputs governing the production-core stability and trim evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionVerificationInputs {
    /// Flight point, airfoils, and production VLM mesh.
    pub flight: SharedCoreInputs,
    /// Symmetric angle perturbation used for `dCm/dCL`, in degrees.
    pub stability_probe_delta_deg: f64,
    /// Evaluated pitch authority, absent when the generated geometry has none.
    pub trim_authority: Option<HorizontalTailTrimAuthority>,
}

/// Longitudinal static-stability classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongitudinalStabilityVerdict {
    /// Increased lift produces a restoring nose-down pitching moment.
    Stable,
    /// Increased lift produces a destabilising nose-up pitching moment.
    Unstable,
    /// The finite difference could not establish either sign.
    Indeterminate,
}

/// Finite-difference stability result about the loaded centre of gravity.
#[derive(Debug, Clone, PartialEq)]
pub struct LongitudinalStabilityAssessment {
    /// Loaded CG used as the VLM moment reference, in metres.
    pub center_of_gravity_x_m: f64,
    /// Lower angle of attack in the centred difference, in degrees.
    pub lower_alpha_deg: f64,
    /// Upper angle of attack in the centred difference, in degrees.
    pub upper_alpha_deg: f64,
    /// Lift coefficient at the lower angle.
    pub lower_cl: f64,
    /// Lift coefficient at the upper angle.
    pub upper_cl: f64,
    /// Pitching-moment coefficient at the lower angle.
    pub lower_cm: f64,
    /// Pitching-moment coefficient at the upper angle.
    pub upper_cm: f64,
    /// Static-stability derivative `dCm/dCL` about the loaded CG.
    pub dcm_dcl: Option<f64>,
    /// Sign-based physical classification.
    pub verdict: LongitudinalStabilityVerdict,
}

/// Why the bounded pitch-trim solve cannot establish trim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchTrimUnverifiedReason {
    /// No production-core pitch-control geometry and range were supplied.
    MissingControlAuthority,
    /// The supplied incidence endpoints do not bracket zero pitching moment.
    NoMomentBracket,
    /// The bounded numerical solve did not reach its numerical residual.
    DidNotConverge,
    /// Missing mass or centre-of-gravity evidence prevented the solve.
    MissingMassOrCenterOfGravity,
}

/// VLM coefficient values at one authority endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimEndpointAssessment {
    /// Horizontal-tail incidence, in degrees.
    pub incidence_deg: f64,
    /// Lift coefficient at the verification flight point.
    pub cl_lift: f64,
    /// Pitching-moment coefficient about the loaded CG.
    pub cm_pitch: f64,
}

/// One converged bounded pitch-trim point.
#[derive(Debug, Clone, PartialEq)]
pub struct TrimPointAssessment {
    /// Horizontal-tail incidence giving zero pitching moment, in degrees.
    pub incidence_deg: f64,
    /// Evidence source for the incidence authority.
    pub authority_evidence: String,
    /// Required one-g lift coefficient at this flight point.
    pub required_lift_coefficient: f64,
    /// VLM lift coefficient at the trim point.
    pub trimmed_lift_coefficient: f64,
    /// VLM pitching-moment residual at the trim point.
    pub pitching_moment_coefficient_residual: f64,
    /// Trimmed lift minus weight, in newtons.
    pub lift_margin_n: f64,
    /// Full production VLM result at the trim point.
    pub vlm: VlmResult,
}

/// Result of attempting the bounded pitch-trim solve.
#[derive(Debug, Clone, PartialEq)]
pub enum PitchTrimAssessment {
    /// A zero-moment point was found inside the evidenced authority range.
    Converged(Box<TrimPointAssessment>),
    /// A stated evidence boundary prevented a trim claim.
    Unverified {
        /// The causal reason no trim claim was made.
        reason: PitchTrimUnverifiedReason,
        /// Lower range endpoint when a control range was evaluated.
        lower_endpoint: Option<TrimEndpointAssessment>,
        /// Upper range endpoint when a control range was evaluated.
        upper_endpoint: Option<TrimEndpointAssessment>,
    },
}

/// One physical failure or evidence gap contributing to the product verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum CoupledFinding {
    /// Finding produced by the existing evidence-gated feasibility evaluator.
    Feasibility(Finding),
    /// The production VLM found `dCm/dCL > 0` about the loaded CG.
    LongitudinalInstability {
        /// Evaluated derivative in the documented sign convention.
        dcm_dcl: f64,
    },
    /// The production VLM could not establish a finite nonzero derivative.
    LongitudinalStabilityIndeterminate,
    /// Pitch trim could not be established for the stated reason.
    PitchTrimUnverified(PitchTrimUnverifiedReason),
    /// The zero-moment point exists but its lift is below one-g weight.
    InsufficientTrimmedLift {
        /// One-g lift coefficient required by mass, density, speed, and area.
        required_lift_coefficient: f64,
        /// VLM lift coefficient at zero pitching moment.
        available_lift_coefficient: f64,
    },
}

impl CoupledFinding {
    /// Whether this item proves failure or prevents verification.
    pub fn severity(&self) -> Severity {
        match self {
            Self::Feasibility(finding) => finding.severity,
            Self::LongitudinalInstability { .. } | Self::InsufficientTrimmedLift { .. } => {
                Severity::Failure
            }
            Self::LongitudinalStabilityIndeterminate | Self::PitchTrimUnverified(_) => {
                Severity::Unverified
            }
        }
    }
}

/// Retained numerical evidence behind the product verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct CoupledAudit {
    /// Fresh component, electrical, packaging, CG, flight, and structural pass.
    pub feasibility: UavReport,
    /// Base production-core result at the requested flight point.
    pub shared_core: Option<SharedCoreAssessment>,
    /// Static-stability derivative when mass and CG allowed VLM to run.
    pub longitudinal_stability: Option<LongitudinalStabilityAssessment>,
    /// Bounded trim result or explicit evidence boundary.
    pub pitch_trim: PitchTrimAssessment,
    /// Complete causal finding list used to select the outcome variant.
    pub findings: Vec<CoupledFinding>,
}

/// Single product-level fixed-wing UAV outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum CoupledOutcome {
    /// Every coupled check completed and passed.
    Accepted(CoupledAudit),
    /// At least one supplied physical limit was violated.
    Rejected(CoupledAudit),
    /// No supplied limit failed, but required evidence is absent.
    Unverified(CoupledAudit),
}

impl CoupledOutcome {
    /// Access the retained evidence independently of the outcome variant.
    pub fn audit(&self) -> &CoupledAudit {
        match self {
            Self::Accepted(audit) | Self::Rejected(audit) | Self::Unverified(audit) => audit,
        }
    }
}

/// Re-evaluate a generated UAV and issue one evidence-aware product outcome.
///
/// # Errors
///
/// Invalid verification inputs, geometry-conversion failures, or production
/// VLM solve failures remain distinct from a completed physical verdict.
pub fn verify_coupled_production(
    optimized: &OptimizedUav,
    catalog: &Catalog,
    inputs: &ProductionVerificationInputs,
) -> Result<CoupledOutcome, SharedCoreFailure> {
    validate_inputs(inputs)?;
    let mut feasibility = evaluate(&optimized.design);
    merge_retained_findings(&mut feasibility, &optimized.report);
    audit_catalog_evidence(optimized, catalog, &mut feasibility);
    let mut findings = feasibility
        .findings
        .iter()
        .cloned()
        .map(CoupledFinding::Feasibility)
        .collect::<Vec<_>>();

    let (Some(takeoff_mass_kg), Some(center_of_gravity_x_m)) = (
        feasibility.takeoff_mass_kg,
        feasibility.center_of_gravity_x_m,
    ) else {
        let reason = PitchTrimUnverifiedReason::MissingMassOrCenterOfGravity;
        findings.push(CoupledFinding::PitchTrimUnverified(reason));
        return Ok(classify(CoupledAudit {
            feasibility,
            shared_core: None,
            longitudinal_stability: None,
            pitch_trim: unverified_trim(reason, None, None),
            findings,
        }));
    };

    let shared_core = assess_generated_geometry_with_shared_core(
        optimized.geometry,
        takeoff_mass_kg,
        center_of_gravity_x_m,
        optimized.design.airframe.induced_drag_factor,
        optimized.design.airframe.zero_lift_drag_coefficient,
        &inputs.flight,
    )?;
    let longitudinal_stability = assess_stability(&shared_core, inputs)?;
    match longitudinal_stability.verdict {
        LongitudinalStabilityVerdict::Stable => {}
        LongitudinalStabilityVerdict::Unstable => {
            if let Some(dcm_dcl) = longitudinal_stability.dcm_dcl {
                findings.push(CoupledFinding::LongitudinalInstability { dcm_dcl });
            }
        }
        LongitudinalStabilityVerdict::Indeterminate => {
            findings.push(CoupledFinding::LongitudinalStabilityIndeterminate);
        }
    }

    let pitch_trim = match &inputs.trim_authority {
        Some(authority) => solve_trim(&shared_core, inputs, authority)?,
        None => unverified_trim(
            PitchTrimUnverifiedReason::MissingControlAuthority,
            None,
            None,
        ),
    };
    match &pitch_trim {
        PitchTrimAssessment::Converged(trim) if trim.lift_margin_n < 0.0 => {
            findings.push(CoupledFinding::InsufficientTrimmedLift {
                required_lift_coefficient: trim.required_lift_coefficient,
                available_lift_coefficient: trim.trimmed_lift_coefficient,
            });
        }
        PitchTrimAssessment::Converged(_) => {}
        PitchTrimAssessment::Unverified { reason, .. } => {
            findings.push(CoupledFinding::PitchTrimUnverified(*reason));
        }
    }

    Ok(classify(CoupledAudit {
        feasibility,
        shared_core: Some(shared_core),
        longitudinal_stability: Some(longitudinal_stability),
        pitch_trim,
        findings,
    }))
}

fn validate_inputs(inputs: &ProductionVerificationInputs) -> Result<(), SharedCoreFailure> {
    if !inputs.stability_probe_delta_deg.is_finite() || inputs.stability_probe_delta_deg <= 0.0 {
        return Err(SharedCoreFailure::InvalidInput(
            "the longitudinal stability probe must be finite and positive".to_owned(),
        ));
    }
    if let Some(authority) = &inputs.trim_authority {
        if !authority.minimum_incidence_deg.is_finite()
            || !authority.maximum_incidence_deg.is_finite()
            || authority.minimum_incidence_deg >= authority.maximum_incidence_deg
            || authority.evidence.trim().is_empty()
        {
            return Err(SharedCoreFailure::InvalidInput(
                "pitch-trim authority needs ordered finite incidence bounds and evidence"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}

fn assess_stability(
    shared_core: &SharedCoreAssessment,
    inputs: &ProductionVerificationInputs,
) -> Result<LongitudinalStabilityAssessment, SharedCoreFailure> {
    let lower_alpha_deg = inputs.flight.angle_of_attack_deg - inputs.stability_probe_delta_deg;
    let upper_alpha_deg = inputs.flight.angle_of_attack_deg + inputs.stability_probe_delta_deg;
    let lower = run_at_alpha(&shared_core.airplane, &inputs.flight, lower_alpha_deg)?;
    let upper = run_at_alpha(&shared_core.airplane, &inputs.flight, upper_alpha_deg)?;
    let delta_cl = upper.cl_lift - lower.cl_lift;
    let derivative = (delta_cl != 0.0).then(|| (upper.cm_pitch - lower.cm_pitch) / delta_cl);
    let dcm_dcl = derivative.filter(|value| value.is_finite());
    let verdict = match dcm_dcl {
        Some(value) if value < 0.0 => LongitudinalStabilityVerdict::Stable,
        Some(value) if value > 0.0 => LongitudinalStabilityVerdict::Unstable,
        _ => LongitudinalStabilityVerdict::Indeterminate,
    };
    Ok(LongitudinalStabilityAssessment {
        center_of_gravity_x_m: shared_core.airplane.xyz_ref[0],
        lower_alpha_deg,
        upper_alpha_deg,
        lower_cl: lower.cl_lift,
        upper_cl: upper.cl_lift,
        lower_cm: lower.cm_pitch,
        upper_cm: upper.cm_pitch,
        dcm_dcl,
        verdict,
    })
}

fn solve_trim(
    shared_core: &SharedCoreAssessment,
    inputs: &ProductionVerificationInputs,
    authority: &HorizontalTailTrimAuthority,
) -> Result<PitchTrimAssessment, SharedCoreFailure> {
    let mut lower = run_at_incidence(
        &shared_core.airplane,
        &inputs.flight,
        authority.minimum_incidence_deg,
    )?;
    let mut upper = run_at_incidence(
        &shared_core.airplane,
        &inputs.flight,
        authority.maximum_incidence_deg,
    )?;
    let lower_endpoint = endpoint(authority.minimum_incidence_deg, &lower);
    let upper_endpoint = endpoint(authority.maximum_incidence_deg, &upper);
    if lower.cm_pitch.abs() <= TRIM_MOMENT_COEFFICIENT_TOLERANCE {
        return Ok(converged_trim(
            shared_core,
            authority,
            authority.minimum_incidence_deg,
            lower,
        ));
    }
    if upper.cm_pitch.abs() <= TRIM_MOMENT_COEFFICIENT_TOLERANCE {
        return Ok(converged_trim(
            shared_core,
            authority,
            authority.maximum_incidence_deg,
            upper,
        ));
    }
    if lower.cm_pitch.is_sign_negative() == upper.cm_pitch.is_sign_negative() {
        return Ok(unverified_trim(
            PitchTrimUnverifiedReason::NoMomentBracket,
            Some(lower_endpoint),
            Some(upper_endpoint),
        ));
    }

    let mut lower_incidence = authority.minimum_incidence_deg;
    let mut upper_incidence = authority.maximum_incidence_deg;
    for _ in 0..MAXIMUM_TRIM_ITERATIONS {
        let denominator = upper.cm_pitch - lower.cm_pitch;
        let linear_incidence =
            lower_incidence - lower.cm_pitch * (upper_incidence - lower_incidence) / denominator;
        let incidence = if linear_incidence.is_finite()
            && linear_incidence > lower_incidence
            && linear_incidence < upper_incidence
        {
            linear_incidence
        } else {
            0.5 * (lower_incidence + upper_incidence)
        };
        let solved = run_at_incidence(&shared_core.airplane, &inputs.flight, incidence)?;
        if solved.cm_pitch.abs() <= TRIM_MOMENT_COEFFICIENT_TOLERANCE {
            return Ok(converged_trim(shared_core, authority, incidence, solved));
        }
        if solved.cm_pitch.is_sign_negative() == lower.cm_pitch.is_sign_negative() {
            lower_incidence = incidence;
            lower = solved;
        } else {
            upper_incidence = incidence;
            upper = solved;
        }
    }
    Ok(unverified_trim(
        PitchTrimUnverifiedReason::DidNotConverge,
        Some(lower_endpoint),
        Some(upper_endpoint),
    ))
}

fn run_at_alpha(
    airplane: &Airplane,
    inputs: &SharedCoreInputs,
    alpha_deg: f64,
) -> Result<VlmResult, SharedCoreFailure> {
    run_vlm(airplane, inputs, alpha_deg)
}

fn run_at_incidence(
    airplane: &Airplane,
    inputs: &SharedCoreInputs,
    incidence_deg: f64,
) -> Result<VlmResult, SharedCoreFailure> {
    let mut adjusted = airplane.clone();
    for wing in adjusted
        .wings
        .iter_mut()
        .filter(|wing| wing.name == HORIZONTAL_TAIL_NAME)
    {
        for section in &mut wing.xsecs {
            section.twist = incidence_deg;
        }
    }
    run_vlm(&adjusted, inputs, inputs.angle_of_attack_deg)
}

fn run_vlm(
    airplane: &Airplane,
    inputs: &SharedCoreInputs,
    alpha_deg: f64,
) -> Result<VlmResult, SharedCoreFailure> {
    let operating_point = OperatingPoint::new(
        Atmosphere::new(inputs.altitude_m),
        inputs.speed_m_s,
        alpha_deg,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    vlm::run(
        airplane,
        &operating_point,
        inputs.spanwise_resolution,
        inputs.chordwise_resolution,
    )
    .map_err(SharedCoreFailure::from)
}

fn converged_trim(
    shared_core: &SharedCoreAssessment,
    authority: &HorizontalTailTrimAuthority,
    incidence_deg: f64,
    vlm: VlmResult,
) -> PitchTrimAssessment {
    let aircraft_weight_n = shared_core.vlm.lift - shared_core.lift_margin_n;
    let lift_margin_n = vlm.lift - aircraft_weight_n;
    PitchTrimAssessment::Converged(Box::new(TrimPointAssessment {
        incidence_deg,
        authority_evidence: authority.evidence.clone(),
        required_lift_coefficient: shared_core.required_lift_coefficient,
        trimmed_lift_coefficient: vlm.cl_lift,
        pitching_moment_coefficient_residual: vlm.cm_pitch,
        lift_margin_n,
        vlm,
    }))
}

fn endpoint(incidence_deg: f64, result: &VlmResult) -> TrimEndpointAssessment {
    TrimEndpointAssessment {
        incidence_deg,
        cl_lift: result.cl_lift,
        cm_pitch: result.cm_pitch,
    }
}

fn unverified_trim(
    reason: PitchTrimUnverifiedReason,
    lower_endpoint: Option<TrimEndpointAssessment>,
    upper_endpoint: Option<TrimEndpointAssessment>,
) -> PitchTrimAssessment {
    PitchTrimAssessment::Unverified {
        reason,
        lower_endpoint,
        upper_endpoint,
    }
}

fn merge_retained_findings(current: &mut UavReport, retained: &UavReport) {
    for finding in &retained.findings {
        if !current.findings.contains(finding) {
            current.findings.push(finding.clone());
        }
    }
}

fn audit_catalog_evidence(optimized: &OptimizedUav, catalog: &Catalog, report: &mut UavReport) {
    if let Err(error) = catalog.validate() {
        report.findings.push(failure(
            FindingKind::InvalidInput,
            "component catalogue",
            format!("catalogue validation failed: {error}"),
            None,
            None,
            None,
        ));
    }
    if optimized.components.propulsion_evidence.trim().is_empty() {
        report.findings.push(missing(
            "propulsion map",
            "traceable thrust-at-speed/current/power evidence",
        ));
    }
    let Some(record) = catalog.get(&optimized.components.landing_gear_id) else {
        report.findings.push(missing(
            &optimized.components.landing_gear_id,
            "selected landing-gear catalogue record",
        ));
        return;
    };
    let ComponentKind::LandingGear(spec) = &record.kind else {
        report.findings.push(failure(
            FindingKind::InvalidInput,
            &record.id,
            "selected landing-gear identifier has the wrong component kind".to_owned(),
            None,
            None,
            None,
        ));
        return;
    };
    if spec.mass_kg.is_none() {
        report
            .findings
            .push(missing(&record.id, "published landing-gear mass"));
    }
    if spec.dimensions.is_none() {
        report
            .findings
            .push(missing(&record.id, "published landing-gear envelope"));
    }
    match (report.takeoff_mass_kg, spec.max_aircraft_mass_kg) {
        (Some(mass), Some(limit)) if mass > limit => report.findings.push(failure(
            FindingKind::LandingGearOverload,
            &record.id,
            "takeoff mass exceeds the published landing-gear rating".to_owned(),
            Some(mass),
            Some(limit),
            Some("kg"),
        )),
        (_, None) => report.findings.push(missing(
            &record.id,
            "published maximum supported aircraft mass",
        )),
        _ => {}
    }
}

fn classify(audit: CoupledAudit) -> CoupledOutcome {
    if audit
        .findings
        .iter()
        .any(|finding| finding.severity() == Severity::Failure)
    {
        CoupledOutcome::Rejected(audit)
    } else if audit.findings.is_empty() {
        CoupledOutcome::Accepted(audit)
    } else {
        CoupledOutcome::Unverified(audit)
    }
}

fn missing(subject: &str, evidence: &str) -> Finding {
    Finding {
        severity: Severity::Unverified,
        kind: FindingKind::MissingData,
        subject: subject.to_owned(),
        message: format!("missing {evidence}; no value was inferred"),
        required: None,
        available: None,
        units: None,
    }
}

fn failure(
    kind: FindingKind,
    subject: &str,
    message: String,
    required: Option<f64>,
    available: Option<f64>,
    units: Option<&'static str>,
) -> Finding {
    Finding {
        severity: Severity::Failure,
        kind,
        subject: subject.to_owned(),
        message,
        required,
        available,
        units,
    }
}
