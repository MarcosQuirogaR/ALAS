// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

