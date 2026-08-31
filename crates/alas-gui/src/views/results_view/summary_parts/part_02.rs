// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn finding_title(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics => "Cruise aerodynamics unavailable",
        FindingCode::NonPositiveFuel => "No usable fuel mass in the MTOW budget",
        FindingCode::TankLimitedTakeoffMass => "Takeoff mass is tank-limited",
        FindingCode::FuelCapacityUnavailable => "Fuel capacity unavailable",
        FindingCode::CgEnvelopeViolation => "CG envelope violation",
        FindingCode::ModelCgAssessmentUnavailable => "Model CG assessment unavailable",
        FindingCode::ModelCgForwardRangeViolation => "Model CG is forward of its range",
        FindingCode::NoseGearStrengthViolation => "Nose-gear load limit exceeded",
        FindingCode::MainGearStrengthViolation => "Main-gear load limit exceeded",
        FindingCode::MinimumNoseGearLoadViolation => "Insufficient nose-gear load",
        FindingCode::PublicPlanningCgEnvelopeViolation => "Public planning CG envelope exceeded",
        FindingCode::TrimUnavailable => "Cruise trim not demonstrated",
        FindingCode::InsufficientStaticMargin => "Static margin below the configured floor",
        FindingCode::WingAreaLimit => "Wing-area limit exceeded",
        FindingCode::MissionUnavailable => "Mission telemetry unavailable",
        FindingCode::MissionNotConverged => "Mission did not converge",
        FindingCode::InvalidMissionFuelBurn => "Mission fuel burn invalid",
        FindingCode::MissionFuelShortfall => "Mission stopped after fuel shortfall",
        FindingCode::InvalidCruiseForceBalance => "Cruise force balance invalid",
        FindingCode::FieldPerformanceUnavailable => "Field performance unavailable",
        FindingCode::FieldTakeoffDistanceViolation => "Takeoff distance exceeds available runway",
        FindingCode::FieldLandingDistanceViolation => {
            "Landing requirement exceeds available runway"
        }
        FindingCode::LandingMassLimitViolation => "Maximum landing mass exceeded",
        FindingCode::ThrustMarginViolation => "Insufficient takeoff thrust margin",
        FindingCode::MissionThrottleLimitViolation => {
            "Mission throttle exceeds the modeled envelope"
        }
        FindingCode::PassengerCapacityShortfall => "Passenger seating shortfall",
        FindingCode::CargoCapacityShortfall => "Cargo capacity shortfall",
    })
}

fn finding_meaning(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics => "The cruise aerodynamic result did not contain a positive finite lift-to-drag ratio, so performance derived from it is not trustworthy.",
        FindingCode::NonPositiveFuel => "Operating empty mass plus payload consumed the configured MTOW budget, leaving no positive finite fuel allocation.",
        FindingCode::TankLimitedTakeoffMass => "The MTOW mass budget could accept more fuel than the established usable tank capacity. The analyzed aircraft therefore departs below MTOW.",
        FindingCode::FuelCapacityUnavailable => "Neither published preset evidence nor the geometry estimate established a usable-fuel capacity for this aircraft.",
        FindingCode::CgEnvelopeViolation => "A retained legacy CG check reported that the analyzed loading state lies outside its allowed envelope.",
        FindingCode::ModelCgAssessmentUnavailable => "The model could not construct the typed CG and landing-gear assessment needed for a physical verdict.",
        FindingCode::ModelCgForwardRangeViolation => "The analyzed CG lies forward of the longitudinal range represented by the current landing-gear and loading model.",
        FindingCode::NoseGearStrengthViolation => "The modeled nose-gear vertical load exceeds the configured tire or gear capacity.",
        FindingCode::MainGearStrengthViolation => "The modeled main-gear vertical load exceeds the configured tire or gear capacity.",
        FindingCode::MinimumNoseGearLoadViolation => "The modeled nose load is below the configured minimum needed to retain preliminary steering authority.",
        FindingCode::PublicPlanningCgEnvelopeViolation => "The point lies outside a manufacturer public planning curve. That curve is preliminary evidence; the actual aircraft weight-and-balance manual controls operations.",
        FindingCode::TrimUnavailable => "ALAS did not retain a finite cruise point that simultaneously satisfies required lift and zero pitching moment. Any displayed untrimmed L/D is a fallback.",
        FindingCode::InsufficientStaticMargin => "The calculated longitudinal static margin is below the physical floor configured for this analysis.",
        FindingCode::WingAreaLimit => "The projected XY wing reference area is non-finite or exceeds the configured maximum.",
        FindingCode::MissionUnavailable => "Mission analysis was requested but returned no usable trajectory telemetry.",
        FindingCode::MissionNotConverged => "At least one native mission segment failed its numerical convergence criteria; totals from the incomplete trajectory are not final requirements.",
        FindingCode::InvalidMissionFuelBurn => "The mission produced a non-finite or non-positive fuel-burn value.",
        FindingCode::MissionFuelShortfall => "Cumulative modeled burn crossed the fuel loaded into this load case. If the mission stopped, the reported deficit is only the overrun observed at the stopping point, not the completed-trip requirement.",
        FindingCode::InvalidCruiseForceBalance => "At least one cruise telemetry record contains a non-finite force or equilibrium result.",
        FindingCode::FieldPerformanceUnavailable => "The selected airport could not be resolved or the mass, wing, thrust, or runway inputs required by the preliminary field model were invalid.",
        FindingCode::FieldTakeoffDistanceViolation => "Modeled takeoff distance required is greater than takeoff distance available at the selected departure conditions.",
        FindingCode::FieldLandingDistanceViolation => "Modeled landing distance or landing wing loading exceeds the selected arrival-field limit.",
        FindingCode::LandingMassLimitViolation => "The analyzed arrival mass is above the configured maximum landing mass. Fuel burn, payload, or the mission/loading definition must change before arrival.",
        FindingCode::ThrustMarginViolation => "Static thrust-to-weight is below the preliminary value required by the selected departure field.",
        FindingCode::MissionThrottleLimitViolation => "At least one mission control point requires a throttle command above the modeled full-throttle limit of 1.0.",
        FindingCode::PassengerCapacityShortfall => "The generated cabin placed fewer passenger seats than the requested passenger count.",
        FindingCode::CargoCapacityShortfall => "The generated ULD layout delivered less net cargo than requested.",
    })
}

fn finding_next_step(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics | FindingCode::InvalidCruiseForceBalance => {
            "Aerodynamics, then Mission & Route"
        }
        FindingCode::TrimUnavailable | FindingCode::InsufficientStaticMargin => {
            "Aerodynamics and Weight & Balance"
        }
        FindingCode::NonPositiveFuel
        | FindingCode::TankLimitedTakeoffMass
        | FindingCode::FuelCapacityUnavailable => "Weight & Balance and Mission & Route",
        FindingCode::MissionUnavailable
        | FindingCode::MissionNotConverged
        | FindingCode::InvalidMissionFuelBurn
        | FindingCode::MissionFuelShortfall
        | FindingCode::MissionThrottleLimitViolation => "Mission & Route",
        FindingCode::CgEnvelopeViolation
        | FindingCode::ModelCgAssessmentUnavailable
        | FindingCode::ModelCgForwardRangeViolation
        | FindingCode::NoseGearStrengthViolation
        | FindingCode::MainGearStrengthViolation
        | FindingCode::MinimumNoseGearLoadViolation
        | FindingCode::PublicPlanningCgEnvelopeViolation => "Weight & Balance",
        FindingCode::WingAreaLimit => "Aerodynamics and Optimization",
        FindingCode::FieldPerformanceUnavailable
        | FindingCode::FieldTakeoffDistanceViolation
        | FindingCode::FieldLandingDistanceViolation
        | FindingCode::LandingMassLimitViolation
        | FindingCode::ThrustMarginViolation => "Field Performance and Weight & Balance",
        FindingCode::PassengerCapacityShortfall | FindingCode::CargoCapacityShortfall => {
            "Weight & Balance payload layout"
        }
    })
}

fn affected_disciplines(code: FindingCode) -> String {
    tr(match code {
        FindingCode::InvalidCruiseAerodynamics => "Aerodynamics | Performance",
        FindingCode::InvalidCruiseForceBalance => "Mission solver | Aerodynamics | Propulsion",
        FindingCode::TrimUnavailable => "Stability & control | Aerodynamics | Weight & balance",
        FindingCode::InsufficientStaticMargin => "Stability & control | Weight & balance",
        FindingCode::NonPositiveFuel
        | FindingCode::TankLimitedTakeoffMass
        | FindingCode::FuelCapacityUnavailable => "Mass properties | Fuel system | Mission",
        FindingCode::MissionUnavailable
        | FindingCode::MissionNotConverged
        | FindingCode::InvalidMissionFuelBurn => "Mission solver | Numerical integration",
        FindingCode::MissionFuelShortfall => "Mission | Mass properties | Propulsion",
        FindingCode::MissionThrottleLimitViolation => "Propulsion | Mission solver | Performance",
        FindingCode::CgEnvelopeViolation
        | FindingCode::ModelCgAssessmentUnavailable
        | FindingCode::ModelCgForwardRangeViolation
        | FindingCode::PublicPlanningCgEnvelopeViolation => {
            "Weight & balance | Stability & control"
        }
        FindingCode::NoseGearStrengthViolation
        | FindingCode::MainGearStrengthViolation
        | FindingCode::MinimumNoseGearLoadViolation => "Landing gear | Weight & balance",
        FindingCode::WingAreaLimit => "Geometry | Aerodynamics | Optimization",
        FindingCode::FieldPerformanceUnavailable
        | FindingCode::FieldTakeoffDistanceViolation
        | FindingCode::FieldLandingDistanceViolation => "Field performance | Airport constraints",
        FindingCode::LandingMassLimitViolation => "Weight & balance | Mission | Field performance",
        FindingCode::ThrustMarginViolation => "Propulsion | Field performance",
        FindingCode::PassengerCapacityShortfall | FindingCode::CargoCapacityShortfall => {
            "Payload layout | Weight & balance"
        }
    })
}

fn actual_label(code: FindingCode) -> String {
    tr(match code {
        FindingCode::MissionFuelShortfall => "Burn at stop / evaluated burn",
        FindingCode::PassengerCapacityShortfall => "Seats placed",
        FindingCode::CargoCapacityShortfall => "Net cargo loaded",
        FindingCode::FieldTakeoffDistanceViolation => "TODR",
        FindingCode::FieldLandingDistanceViolation => "Required value",
        FindingCode::MissionThrottleLimitViolation => "Maximum throttle",
        FindingCode::TankLimitedTakeoffMass => "MTOW-closure fuel",
        _ => "Calculated",
    })
}

fn limit_label(code: FindingCode) -> String {
    tr(match code {
        FindingCode::MissionFuelShortfall => "Fuel loaded",
        FindingCode::PassengerCapacityShortfall => "Passengers requested",
        FindingCode::CargoCapacityShortfall => "Net cargo requested",
        FindingCode::FieldTakeoffDistanceViolation => "TODA",
        FindingCode::FieldLandingDistanceViolation => "Available / limiting value",
        FindingCode::MissionThrottleLimitViolation => "Full-throttle limit",
        FindingCode::TankLimitedTakeoffMass => "Usable tank capacity",
        _ => "Limit",
    })
}

fn finding_margin(code: FindingCode, actual: f64, limit: f64) -> f64 {
    match code {
        FindingCode::NonPositiveFuel
        | FindingCode::InsufficientStaticMargin
        | FindingCode::ThrustMarginViolation
        | FindingCode::MinimumNoseGearLoadViolation
        | FindingCode::PassengerCapacityShortfall
        | FindingCode::CargoCapacityShortfall => actual - limit,
        _ => limit - actual,
    }
}

fn selected_analysis(result: &alas_pipeline::PipelineResult) -> Option<&AnalysisReport> {
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

fn aircraft_summary_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let mut metrics = Vec::new();
    let fuel = &result.feasibility.fuel_loading;
    if let Some(report) = selected_analysis(result) {
        if let Some((oew_kg, tow_kg, mtow_kg)) = mass_triplet_kg(
            &report.component_masses,
            fuel.analyzed_takeoff_mass_kg,
            result.config.requirements.mtow_kg,
        ) {
            metrics.push((
                "Masses (OEW / TOW / MTOW)",
                format!(
                    "{:.1} / {:.1} / {:.1} t",
                    oew_kg / 1_000.0,
                    tow_kg / 1_000.0,
                    mtow_kg / 1_000.0
                ),
            ));
        }
        if report.airplane.b_ref.is_finite()
            && report.airplane.b_ref > 0.0
            && report.airplane.s_ref.is_finite()
            && report.airplane.s_ref > 0.0
        {
            metrics.push((
                "Wing geometry",
                tr_fields(
                    "{span} m span | {area} m^2 area",
                    &[
                        ("span", format!("{:.1}", report.airplane.b_ref)),
                        ("area", format!("{:.1}", report.airplane.s_ref)),
                    ],
                ),
            ));
        }
        let (l_over_d, provenance) = report
            .trimmed_design_point
            .as_ref()
            .map(|point| (point.l_over_d, "trimmed"))
            .unwrap_or((report.design_point.l_over_d, "untrimmed"));
        if l_over_d.is_finite() && l_over_d > 0.0 {
            metrics.push(("Cruise L/D", format!("{l_over_d:.1} ({})", tr(provenance))));
        }
    }

    metrics
}

fn fuel_detail_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mut metrics = Vec::new();
    if fuel.analyzed_carried_fuel_kg.is_finite() && fuel.analyzed_carried_fuel_kg >= 0.0 {
        let capacity = match fuel.usable_capacity.capacity_kg {
            Some(capacity_kg) if capacity_kg.is_finite() && capacity_kg >= 0.0 => tr_fields(
                "{capacity} t ({evidence})",
                &[
                    ("capacity", format!("{:.1}", capacity_kg / 1_000.0)),
                    (
                        "evidence",
                        tr(match fuel.usable_capacity.evidence {
                            FuelCapacityEvidence::PublishedPreset => "published",
                            FuelCapacityEvidence::GeometryEstimate => "geometry estimate",
                            FuelCapacityEvidence::Unavailable => "unavailable",
                        }),
                    ),
                ],
            ),
            _ => tr("unavailable"),
        };
        metrics.push((
            "Fuel carried / usable capacity",
            tr_fields(
                "{carried} t / {capacity}",
                &[
                    (
                        "carried",
                        format!("{:.1}", fuel.analyzed_carried_fuel_kg / 1_000.0),
                    ),
                    ("capacity", capacity),
                ],
            ),
        ));
    }
    if fuel.zero_fuel_mass_kg.is_finite() && fuel.zero_fuel_mass_kg >= 0.0 {
        metrics.push((
            "Zero-fuel mass / MTOW fuel budget",
            format!(
                "{:.1} t / {:.1} t",
                fuel.zero_fuel_mass_kg / 1_000.0,
                fuel.mtow_closure_fuel_kg / 1_000.0
            ),
        ));
    }
    if let Some(burned_kg) = fuel
        .mission
        .burned_fuel_kg
        .filter(|value| value.is_finite())
    {
        metrics.push((
            "Mission burn from available telemetry",
            format!(
                "{:.2} t | {}",
                burned_kg / 1_000.0,
                mission_status_label(result)
            ),
        ));
    }
    metrics
}

fn mass_triplet_kg(
    component_masses: &std::collections::HashMap<String, f64>,
    tow_kg: f64,
    mtow_kg: f64,
) -> Option<(f64, f64, f64)> {
    let total_kg: f64 = component_masses.values().copied().sum();
    let payload_kg = component_masses.get("Payload").copied().unwrap_or(0.0);
    let fuel_kg = component_masses.get("Fuel").copied().unwrap_or(0.0);
    let oew_kg = total_kg - payload_kg - fuel_kg;
    (oew_kg.is_finite()
        && oew_kg > 0.0
        && tow_kg.is_finite()
        && tow_kg > 0.0
        && mtow_kg.is_finite()
        && mtow_kg > 0.0)
        .then_some((oew_kg, tow_kg, mtow_kg))
}

fn result_payload_layout(result: &alas_pipeline::PipelineResult) -> Option<&PayloadLayout> {
    result
        .optimized_report
        .as_ref()
        .and_then(|report| report.payload_layout.as_ref())
        .or_else(|| {
            result
                .baseline_analysis
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
        .or_else(|| {
            result
                .baseline_report
                .as_ref()
                .and_then(|report| report.payload_layout.as_ref())
        })
}

fn payload_summary_metrics(layout: &PayloadLayout) -> Vec<(&'static str, String)> {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => {
            let classes = summary
                .classes
                .iter()
                .filter(|(_, seats)| *seats > 0)
                .map(|(class, seats)| format!("{} {seats}", tr(class)))
                .collect::<Vec<_>>()
                .join(" | ");
            vec![
                (
                    "Seating capacity",
                    tr_fields(
                        "{seated} / {requested} seats requested",
                        &[
                            ("seated", summary.seated_pax.to_string()),
                            ("requested", summary.total_pax.to_string()),
                        ],
                    ),
                ),
                ("Unseated passengers", summary.unseated_pax.to_string()),
                ("Cabin class mix", classes),
                ("Passenger payload", format!("{:.1} t", summary.payload_t)),
                (
                    "Hold loading",
                    format!(
                        "{:.1} / {:.1} t | {} ULD",
                        summary.hold_used_t, summary.hold_capacity_t, summary.hold_ulds
                    ),
                ),
                (
                    "Checked bags / belly freight",
                    format!("{:.1} / {:.1} t", summary.bag_mass_t, summary.belly_cargo_t),
                ),
                (
                    "Cabin arrangement",
                    tr_fields(
                        "{abreast} abreast | {aisles} aisle(s) | {decks}",
                        &[
                            ("abreast", summary.max_abreast.to_string()),
                            ("aisles", summary.n_aisles.to_string()),
                            (
                                "decks",
                                tr(if summary.double_deck {
                                    "double deck"
                                } else {
                                    "single deck"
                                }),
                            ),
                        ],
                    ),
                ),
                (
                    "Galleys / lavatories / exit pairs",
                    format!(
                        "{} / {} / {}",
                        summary.galleys, summary.lavatories, summary.exit_pairs
                    ),
                ),
                (
                    "Accessibility provisions",
                    format!(
                        "{} accessible lavatory / {} wheelchair stowage",
                        summary.accessible_lavatories, summary.wheelchair_stowages
                    ),
                ),
                ("Payload CG", format!("{:.1}% MAC", summary.cg_pct_mac)),
            ]
        }
        LayoutSummary::Cargo(summary) => vec![
            ("Cargo payload", format!("{:.1} t", summary.payload_t)),
            (
                "Net cargo / requested",
                format!(
                    "{:.1} / {:.1} t",
                    summary.loaded_net_payload_t, summary.requested_net_payload_t
                ),
            ),
            ("ULD tare", format!("{:.1} t", summary.tare_mass_t)),
            (
                "ULD loading",
                tr_fields(
                    "{loaded} / {slots} positions",
                    &[
                        ("loaded", summary.n_ulds.to_string()),
                        ("slots", summary.n_slots.to_string()),
                    ],
                ),
            ),
            (
                "Cargo capacity",
                format!("{:.1} t | {:.1}%", summary.capacity_t, summary.fill_pct),
            ),
            ("Cargo volume", format!("{:.1} m^3", summary.volume_m3)),
            (
                "Main / lower deck ULDs",
                format!("{} / {}", summary.n_main_deck, summary.n_lower_deck),
            ),
            (
                "Payload CG",
                format!("{:.1}% MAC", summary.achieved_cg_pct_mac),
            ),
            ("Loading strategy", tr(&summary.strategy)),
        ],
    }
}
