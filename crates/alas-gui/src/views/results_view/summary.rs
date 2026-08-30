// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout-derived aircraft metrics shown above the result figures.

use alas_payload::layout::{LayoutSummary, PayloadLayout};
use alas_pipeline::feasibility::{
    FindingCode, FindingSeverity, FuelCapacityEvidence, MissionFuelStatus, PhysicalFinding,
};
use alas_pipeline::AnalysisReport;
use egui::{Frame, Margin, RichText, Rounding, Stroke, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::format_cg_pct_mac;

fn stat_tile(ui: &mut Ui, label: &str, value: String) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(label)).weak().small());
            ui.add(egui::Label::new(RichText::new(value).strong().size(16.0)).wrap());
        });
    });
}

fn semantic_frame(ui: &Ui, color: egui::Color32) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .stroke(Stroke::new(1.5_f32, color))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .rounding(Rounding::same(12.0))
}

fn status_frame(ui: &Ui, severity: FindingSeverity) -> Frame {
    semantic_frame(
        ui,
        match severity {
            FindingSeverity::Error => ui.visuals().error_fg_color,
            FindingSeverity::Warning => ui.visuals().warn_fg_color,
        },
    )
}

fn show_status_banner(ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    let errors = result
        .feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .count();
    let warnings = result.feasibility.findings.len().saturating_sub(errors);
    let (color, title, detail) = if errors > 0 {
        (
            ui.visuals().error_fg_color,
            tr("Infeasible under implemented checks"),
            format!(
                "{errors} blocking finding(s) | {warnings} warning(s) | {}",
                mission_status_label(result)
            ),
        )
    } else if warnings > 0 {
        (
            ui.visuals().warn_fg_color,
            tr("Feasible with engineering warnings"),
            format!("{warnings} warning(s) | {}", mission_status_label(result)),
        )
    } else {
        (
            crate::theme::success_color(ui.visuals()),
            tr("Feasible under implemented checks"),
            mission_status_label(result),
        )
    };

    semantic_frame(ui, color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).strong().size(19.0).color(color));
            ui.label(RichText::new(detail).weak());
            ui.label(RichText::new("?").strong().color(color))
                .on_hover_text(tr(
                    "This verdict covers only the physical checks implemented by this run; it is not a certification finding.",
                ));
        });
    });
}

fn mission_status_label(result: &alas_pipeline::PipelineResult) -> String {
    let status = match result.feasibility.fuel_loading.mission.status {
        MissionFuelStatus::NotRequested => "Mission not requested",
        MissionFuelStatus::Unavailable => "Mission unavailable",
        MissionFuelStatus::NotConverged => "Mission did not converge",
        MissionFuelStatus::Completed => "Mission completed",
        MissionFuelStatus::Exhausted => "Mission stopped: fuel exhausted",
    };
    tr(status)
}

fn maximum_mission_range_km(result: &alas_pipeline::PipelineResult) -> Option<f64> {
    result.mission_result.as_ref().and_then(|mission| {
        mission
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.aircraft_range_m.iter().copied())
            .filter(|range| range.is_finite())
            .reduce(f64::max)
            .map(|range_m| range_m / 1_000.0)
    })
}

fn headline_metrics(result: &alas_pipeline::PipelineResult) -> Vec<(&'static str, String)> {
    let fuel = &result.feasibility.fuel_loading;
    let mission = maximum_mission_range_km(result).map_or_else(
        || mission_status_label(result),
        |range_km| format!("{range_km:.0} km | {}", mission_status_label(result)),
    );
    let fuel_margin = match fuel.mission.status {
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted
            if fuel.analyzed_carried_fuel_kg.is_finite()
                && fuel
                    .mission
                    .burned_fuel_kg
                    .is_some_and(|burned| burned.is_finite()) =>
        {
            let burned = fuel.mission.burned_fuel_kg.unwrap_or_default();
            let margin_kg = fuel.analyzed_carried_fuel_kg - burned;
            let qualifier = if fuel.mission.status == MissionFuelStatus::Exhausted {
                tr("observed at stop")
            } else {
                tr("trip margin")
            };
            if fuel.analyzed_carried_fuel_kg > 0.0 {
                let percent = 100.0 * margin_kg / fuel.analyzed_carried_fuel_kg;
                format!(
                    "{:+.2} t | {:+.1}% | {qualifier}",
                    margin_kg / 1_000.0,
                    percent
                )
            } else {
                format!("{:+.2} t | {qualifier}", margin_kg / 1_000.0)
            }
        }
        MissionFuelStatus::NotConverged => tr("Not established | partial telemetry"),
        MissionFuelStatus::Unavailable => tr("Not established | mission unavailable"),
        MissionFuelStatus::NotRequested => tr("Not evaluated"),
        MissionFuelStatus::Completed | MissionFuelStatus::Exhausted => tr("Not established"),
    };
    let mass_margin = if fuel.mtow_shortfall_kg.is_finite() {
        if fuel.mtow_shortfall_kg.abs() < 0.5 {
            tr("At MTOW")
        } else {
            format!("{:.2} t below MTOW", fuel.mtow_shortfall_kg / 1_000.0)
        }
    } else {
        tr("Not established")
    };
    let trim = selected_analysis(result).map_or_else(
        || tr("Not evaluated"),
        |report| {
            report.trimmed_design_point.as_ref().map_or_else(
                || tr("Not demonstrated"),
                |point| {
                    format!(
                        "Solved | alpha {:.2} deg | iH {:.2} deg | |Cm| {:.2e}",
                        point.geometric_body_alpha_deg,
                        point.trim_ih_deg,
                        point.cm_residual.abs()
                    )
                },
            )
        },
    );
    vec![
        ("Mission progress", mission),
        ("Fuel margin", fuel_margin),
        ("Takeoff mass margin", mass_margin),
        ("Cruise trim", trim),
    ]
}

fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 245.0).floor() as usize).clamp(1, 4)
}

fn show_stat_tiles(ui: &mut Ui, metrics: &[(&str, String)]) {
    if metrics.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(metrics.len());
    for row in metrics.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                stat_tile(&mut columns[index], label, value.clone());
            }
        });
        ui.add_space(8.0);
    }
}

pub(super) fn show_summary(state: &AppState, ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    show_status_banner(ui, result);
    ui.add_space(10.0);
    show_stat_tiles(ui, &headline_metrics(result));

    if !result.feasibility.findings.is_empty() {
        show_findings(ui, &result.feasibility.findings);
        ui.add_space(10.0);
    }

    ui.label(
        RichText::new(tr("Aircraft and mission"))
            .strong()
            .size(18.0),
    );
    ui.label(
        RichText::new(tr(
            "Selected design values, limits, and evidence used by this run.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(5.0);
    let mut metrics = vec![("Preset", state.active_preset.clone())];
    if let Some(baseline) = &result.baseline_report {
        metrics.push((
            "Baseline static margin",
            format!("{:.1}%", baseline.static_margin * 100.0),
        ));
        metrics.push(("Baseline CG", format_cg_pct_mac(baseline.cg_pct_mac)));
    }
    if let Some(optimized) = &result.optimized_report {
        metrics.push((
            "Optimized static margin",
            format!("{:.1}%", optimized.static_margin * 100.0),
        ));
        let envelope = match optimized.cg_envelope_ok {
            Some(true) => "OK".to_owned(),
            Some(false) => tr("Violation"),
            None => "-".to_owned(),
        };
        metrics.push(("CG envelope", envelope));
    }
    metrics.extend(aircraft_summary_metrics(result));
    metrics.extend(fuel_detail_metrics(result));
    show_stat_tiles(ui, &metrics);

    if let Some(layout) = result_payload_layout(result) {
        ui.label(RichText::new(tr("Payload and cabin")).strong().size(18.0));
        ui.label(
            RichText::new(tr(
                "Delivered payload, accommodation, and loading arrangement.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(5.0);
        show_stat_tiles(ui, &payload_summary_metrics(layout));
    }

    egui::CollapsingHeader::new(RichText::new(tr("Propulsion cycle details")).strong())
        .default_open(false)
        .show(ui, |ui| show_propulsion_cycle_summary(ui, &result.config));
    ui.add_space(8.0);
    ui.add(
        egui::Label::new(
            RichText::new(tr(
                "Open a discipline tab above for its figures. Slots that read \"Not available for \
             this run\" need data this run did not produce.",
            ))
            .weak(),
        )
        .wrap(),
    );
}

fn show_findings(ui: &mut Ui, findings: &[PhysicalFinding]) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Findings requiring attention"))
                .strong()
                .size(18.0),
        );
        ui.label(RichText::new("?").strong())
            .on_hover_ui(show_finding_catalog);
    });
    ui.add_space(5.0);
    let columns = if ui.available_width() >= 820.0 { 2 } else { 1 };
    for row in findings.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, finding) in row.iter().enumerate() {
                show_finding_card(&mut column_uis[index], finding);
            }
        });
        ui.add_space(6.0);
    }
}

fn show_finding_card(ui: &mut Ui, finding: &PhysicalFinding) {
    let (symbol, color) = match finding.severity {
        FindingSeverity::Error => ("x", ui.visuals().error_fg_color),
        FindingSeverity::Warning => ("!", ui.visuals().warn_fg_color),
    };
    status_frame(ui, finding.severity).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(symbol).strong().color(color));
            ui.label(RichText::new(finding_title(finding.code)).strong());
            ui.label(RichText::new("?").strong().color(color))
                .on_hover_ui(|ui| show_finding_help(ui, finding));
        });
        ui.label(
            RichText::new(affected_disciplines(finding.code))
                .small()
                .weak(),
        );
        if let (Some(actual), Some(limit)) = (finding.actual, finding.limit) {
            if !finding.unit.is_empty() {
                let margin = finding_margin(finding.code, actual, limit);
                ui.label(
                    RichText::new(format!("{} {margin:+.3} {}", tr("Margin"), finding.unit))
                        .strong()
                        .color(color),
                );
            }
        }
    });
}

fn show_finding_help(ui: &mut Ui, finding: &PhysicalFinding) {
    ui.set_max_width(430.0);
    ui.label(RichText::new(finding_title(finding.code)).strong());
    ui.add(egui::Label::new(finding_meaning(finding.code)).wrap());
    ui.separator();
    ui.label(RichText::new(tr("Solver output")).strong());
    ui.add(egui::Label::new(tr(&finding.message)).wrap());
    if let (Some(actual), Some(limit)) = (finding.actual, finding.limit) {
        if !finding.unit.is_empty() {
            ui.label(format!(
                "{}: {actual:.3} {} | {}: {limit:.3} {}",
                actual_label(finding.code),
                finding.unit,
                limit_label(finding.code),
                finding.unit
            ));
        }
    }
    ui.label(
        RichText::new(format!(
            "{}: {}",
            tr("Inspect next"),
            finding_next_step(finding.code)
        ))
        .small()
        .weak(),
    );
    if finding.code == FindingCode::TrimUnavailable {
        ui.separator();
        ui.label(
            RichText::new(tr("Possible causes hidden by the current solver output:")).strong(),
        );
        ui.label(tr(
            "Aerodynamic probe failure; singular or nearly singular lift/moment response; non-finite angle or stabilizer incidence; a non-converged coupled solve; trimmed-performance evaluation failure; or a pitching-moment residual above |Cm| = 0.001. The Summary tab cannot distinguish these without a future model-output change.",
        ));
    }
}

fn show_finding_catalog(ui: &mut Ui) {
    ui.set_max_width(520.0);
    ui.label(RichText::new(tr("Finding guide")).strong());
    ui.label(tr(
        "ALAS may report failures in these groups. Hover the question mark on a specific finding for its exact meaning.",
    ));
    ui.separator();
    for (group, text) in [
        ("Aerodynamics and trim", "invalid cruise aerodynamics; cruise trim unavailable; non-finite cruise force balance"),
        ("Fuel and mission", "non-positive fuel; tank-limited takeoff mass; unknown tank capacity; unavailable or non-converged mission; invalid burn; fuel shortfall; throttle above the modeled envelope"),
        ("Mass, CG, and stability", "model CG unavailable or outside its range; public planning-envelope violation; nose/main gear strength or minimum nose-load violation; insufficient static margin"),
        ("Geometry and payload", "wing-area limit; passenger seating shortfall; cargo capacity shortfall"),
        ("Field performance", "airport/input unavailable; takeoff or landing distance violation; maximum landing mass exceeded; insufficient thrust margin"),
    ] {
        ui.label(RichText::new(tr(group)).strong());
        ui.add(egui::Label::new(tr(text)).wrap());
    }
    ui.separator();
    ui.label(
        RichText::new(tr(
            "A finding means an implemented preliminary-design check failed or could not be demonstrated. It is not by itself a certification determination.",
        ))
        .small()
        .weak(),
    );
}

fn show_propulsion_cycle_summary(ui: &mut Ui, config: &alas_config::AlasConfig) {
    let lines = alas_report::families::propulsion::propulsion_cycle_summary(config);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Propulsion cycle summary")).strong());
        ui.add_space(4.0);
        let entries: Vec<(String, String)> = lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| propulsion_summary_entry(&line))
            .collect();
        let columns = usize::from(ui.available_width() >= 620.0).max(1);
        for row in entries.chunks(columns) {
            ui.columns(columns, |columns| {
                for (index, (label, value)) in row.iter().enumerate() {
                    columns[index].horizontal_wrapped(|ui| {
                        if !label.is_empty() {
                            ui.label(RichText::new(label).weak().small());
                        }
                        ui.label(RichText::new(value).strong());
                    });
                }
            });
            ui.add_space(4.0);
        }
    });
}

fn propulsion_summary_entry(line: &str) -> (String, String) {
    line.split_once(':')
        .or_else(|| line.split_once('='))
        .map(|(label, value)| (label.trim().to_owned(), value.trim().to_owned()))
        .unwrap_or_else(|| (String::new(), line.trim().to_owned()))
}

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

#[cfg(test)]
mod tests {
    use super::{finding_margin, mass_triplet_kg, payload_summary_metrics};
    use alas_payload::layout::{LayoutSummary, PassengerSummary, PayloadLayout};
    use alas_pipeline::FindingCode;

    #[test]
    fn passenger_summary_metrics_come_from_the_built_layout() {
        let layout = PayloadLayout {
            mode: alas_payload::layout::Mode::Passenger,
            items: Vec::new(),
            total_mass: 21_000.0,
            cg_x: 10.0,
            cg_y: 0.0,
            summary: LayoutSummary::Passenger(Box::new(PassengerSummary {
                total_pax: 204,
                seated_pax: 198,
                unseated_pax: 6,
                classes: vec![("Business", 18), ("Economy", 180)],
                lavatories: 4,
                galleys: 3,
                accessible_lavatories: 1,
                wheelchair_stowages: 1,
                exit_type: "A",
                exit_pairs: 4,
                exit_capacity: 220,
                max_certifiable_capacity: 220,
                payload_t: 21.0,
                seat_mass_t: 17.5,
                bag_mass_t: 2.8,
                belly_cargo_t: 0.7,
                hold_capacity_t: 8.0,
                hold_used_t: 3.5,
                hold_ulds: 5,
                aisle_width_m: 0.51,
                max_abreast: 6,
                n_aisles: 1,
                deck_utilization: vec![("main", 0.82)],
                cg_pct_mac: 25.4,
                double_deck: false,
            })),
        };
        let metrics = payload_summary_metrics(&layout);
        assert!(metrics.iter().any(|(label, value)| {
            *label == "Seating capacity" && value == "198 / 204 seats requested"
        }));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Cabin class mix" && value.contains("Business 18")));
        assert!(metrics
            .iter()
            .any(|(label, value)| *label == "Hold loading" && value.contains("3.5 / 8.0 t")));
        assert!(metrics.iter().any(|(label, value)| {
            *label == "Accessibility provisions" && value.contains("1 accessible lavatory")
        }));
    }

    #[test]
    fn aircraft_mass_summary_excludes_payload_and_fuel_from_oew() {
        let masses = std::collections::HashMap::from([
            ("Wing".to_owned(), 12_000.0),
            ("Fuselage".to_owned(), 8_000.0),
            ("Payload".to_owned(), 6_000.0),
            ("Fuel".to_owned(), 4_000.0),
        ]);
        assert_eq!(
            mass_triplet_kg(&masses, 29_000.0, 32_000.0),
            Some((20_000.0, 29_000.0, 32_000.0))
        );
    }

    #[test]
    fn finding_margins_are_negative_on_both_upper_and_lower_bound_failures() {
        assert_eq!(
            finding_margin(FindingCode::MissionFuelShortfall, 51_410.0, 50_400.0),
            -1_010.0
        );
        assert!(
            (finding_margin(FindingCode::InsufficientStaticMargin, 0.03, 0.05) + 0.02).abs()
                < 1.0e-12
        );
    }
}
