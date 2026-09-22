// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The single findings card of the Summary tab.
//!
//! Feasibility warnings and blocking findings share one card. Every row shows
//! the reported problem directly, marked with `>` in the severity colour; the
//! finding's meaning and the next thing to inspect stay in hover help.

use alas_pipeline::feasibility::{FindingCode, FindingSeverity, PhysicalFinding};
use egui::{RichText, Ui};

use super::findings::{
    actual_label, affected_disciplines, finding_margin, finding_meaning, finding_next_step,
    finding_title, limit_label,
};
use super::status_frame;
use crate::views::tr;

/// Findings with the same code and message reported by more than one source
/// are shown once. Order is preserved, errors first.
pub(super) fn deduplicated(findings: &[PhysicalFinding]) -> Vec<&PhysicalFinding> {
    let mut unique: Vec<&PhysicalFinding> = Vec::with_capacity(findings.len());
    for finding in findings {
        if !unique
            .iter()
            .any(|seen| seen.code == finding.code && seen.message == finding.message)
        {
            unique.push(finding);
        }
    }
    unique.sort_by_key(|finding| match finding.severity {
        FindingSeverity::Error => 0,
        FindingSeverity::Warning => 1,
    });
    unique
}

fn worst_severity(findings: &[&PhysicalFinding]) -> FindingSeverity {
    if findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error)
    {
        FindingSeverity::Error
    } else {
        FindingSeverity::Warning
    }
}

/// Render the merged findings card, or nothing when there are no findings.
pub(super) fn show_findings_card(ui: &mut Ui, findings: &[PhysicalFinding]) {
    let findings = deduplicated(findings);
    if findings.is_empty() {
        return;
    }
    ui.label(RichText::new(tr("Findings")).strong().size(18.0))
        .on_hover_ui(show_finding_catalog);
    ui.add_space(5.0);
    status_frame(ui, worst_severity(&findings)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        for (index, finding) in findings.iter().enumerate() {
            if index > 0 {
                ui.add_space(6.0);
            }
            show_finding_row(ui, finding);
        }
    });
    ui.add_space(10.0);
}

fn show_finding_row(ui: &mut Ui, finding: &PhysicalFinding) {
    let color = match finding.severity {
        FindingSeverity::Error => ui.visuals().error_fg_color,
        FindingSeverity::Warning => ui.visuals().warn_fg_color,
    };
    let response = ui
        .vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(">").strong().color(color));
                ui.label(RichText::new(finding_title(finding.code)).strong());
            });
            ui.add(egui::Label::new(tr(&finding.message)).wrap());
            if let (Some(actual), Some(limit)) = (finding.actual, finding.limit) {
                if !finding.unit.is_empty() {
                    let margin = finding_margin(finding.code, actual, limit);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{}: {actual:.3} {}",
                            actual_label(finding.code),
                            finding.unit
                        ));
                        ui.label(format!(
                            "{}: {limit:.3} {}",
                            limit_label(finding.code),
                            finding.unit
                        ));
                        ui.label(
                            RichText::new(format!(
                                "{}: {margin:+.3} {}",
                                tr("Margin"),
                                finding.unit
                            ))
                            .strong()
                            .color(color),
                        );
                    });
                }
            }
        })
        .response;
    response.on_hover_ui(|ui| show_finding_help(ui, finding));
}

fn show_finding_help(ui: &mut Ui, finding: &PhysicalFinding) {
    ui.set_max_width(430.0);
    ui.label(RichText::new(finding_title(finding.code)).strong());
    ui.add(egui::Label::new(finding_meaning(finding.code)).wrap());
    ui.label(RichText::new(affected_disciplines(finding.code)).small());
    ui.separator();
    ui.label(
        RichText::new(format!(
            "{}: {}",
            tr("Inspect next"),
            finding_next_step(finding.code)
        ))
        .small(),
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
        "ALAS may report failures in these groups. Hover a finding for its exact meaning.",
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
        .small(),
    );
}

#[cfg(test)]
mod tests {
    use super::deduplicated;
    use alas_pipeline::feasibility::{FindingCode, FindingSeverity, PhysicalFinding};

    fn finding(code: FindingCode, severity: FindingSeverity, message: &str) -> PhysicalFinding {
        PhysicalFinding {
            code,
            severity,
            message: message.to_owned(),
            actual: None,
            limit: None,
            unit: "",
        }
    }

    #[test]
    fn findings_reported_by_two_sources_are_shown_once_with_errors_first() {
        let findings = vec![
            finding(
                FindingCode::InsufficientStaticMargin,
                FindingSeverity::Warning,
                "static margin 3.0 % below 5.0 %",
            ),
            finding(
                FindingCode::MissionFuelShortfall,
                FindingSeverity::Error,
                "fuel shortfall",
            ),
            finding(
                FindingCode::InsufficientStaticMargin,
                FindingSeverity::Warning,
                "static margin 3.0 % below 5.0 %",
            ),
            finding(
                FindingCode::InsufficientStaticMargin,
                FindingSeverity::Warning,
                "static margin 2.0 % below 5.0 %",
            ),
        ];
        let unique = deduplicated(&findings);
        assert_eq!(unique.len(), 3);
        assert_eq!(unique[0].code, FindingCode::MissionFuelShortfall);
        assert_eq!(unique[1].message, "static margin 3.0 % below 5.0 %");
        assert_eq!(unique[2].message, "static margin 2.0 % below 5.0 %");
    }
}
