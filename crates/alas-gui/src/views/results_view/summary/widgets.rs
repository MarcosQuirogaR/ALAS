// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stat tiles, semantic frames and the run status banner.

use alas_pipeline::feasibility::{FindingSeverity, MissionFuelStatus};
use egui::{Frame, Margin, RichText, Rounding, Stroke, Ui};

use crate::views::{tr, tr_fields};

pub(super) fn stat_tile(ui: &mut Ui, label: &str, value: String) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(label)).small());
            ui.add(egui::Label::new(RichText::new(value).strong().size(16.0)).wrap());
        });
    });
}

/// One stat tile, as wide as a tile of a [`show_stat_tiles`] row, whose
/// label explains itself on hover.
pub(super) fn show_explained_tile(ui: &mut Ui, label: &str, value: String, hover: String) {
    let columns = summary_column_count(ui.available_width());
    ui.columns(columns, |columns| {
        let ui = &mut columns[0];
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                let title = ui.add(
                    egui::Label::new(crate::notation::styled(
                        ui,
                        RichText::new(tr(label)).small(),
                    ))
                    .wrap(),
                );
                crate::notation::hover(title, &hover);
                let figure = ui.add(
                    egui::Label::new(crate::notation::styled(
                        ui,
                        RichText::new(value).strong().size(16.0),
                    ))
                    .wrap(),
                );
                crate::notation::hover(figure, &hover);
            });
        });
    });
    ui.add_space(8.0);
}

pub(super) fn semantic_frame(ui: &Ui, color: egui::Color32) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .stroke(Stroke::new(1.5_f32, color))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .rounding(Rounding::same(12.0))
}

/// The banner title: the pipeline's delivery verdict label, or the
/// incomplete-assessment title while the run has not finished.
pub(super) fn status_banner_title(
    completed: bool,
    verdict: alas_pipeline::DeliveryVerdict,
) -> &'static str {
    if completed {
        verdict.label()
    } else {
        "Assessment incomplete"
    }
}

/// Translates the leading category of a blocker message ("physical: ...")
/// and keeps the producer's text after it.
pub(super) fn blocker_text(blocker: &alas_pipeline::DeliveryBlocker) -> String {
    let message = blocker.message();
    match message.split_once(": ") {
        Some((head, tail)) => {
            let label = match head {
                "physical" => "Physical check",
                "design mission" => "Design mission",
                "mass coordinates" => "Mass coordinates",
                "run" => "Run status",
                _ => return message,
            };
            format!("{}: {tail}", tr(label))
        }
        None => message,
    }
}

pub(super) fn show_status_banner(
    ui: &mut Ui,
    result: &alas_pipeline::PipelineResult,
    completed: bool,
) {
    let errors = result
        .feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .count();
    let warnings = result.feasibility.findings.len().saturating_sub(errors);
    let counts = tr_fields(
        "{errors} blocking finding(s), {warnings} warning(s).",
        &[
            ("errors", errors.to_string()),
            ("warnings", warnings.to_string()),
        ],
    );
    let delivery = result.delivery();
    let title = tr(status_banner_title(completed, delivery.verdict));
    let (color, detail) = if !completed {
        (
            ui.visuals().warn_fg_color,
            tr("The figures below come from finished analysis stages, but the overall feasibility assessment is incomplete. Finalized report exports are unavailable until the pipeline completes."),
        )
    } else if !delivery.is_success() {
        (
            ui.visuals().error_fg_color,
            format!("{counts} {}", mission_status_label(result)),
        )
    } else if warnings > 0 {
        (
            ui.visuals().warn_fg_color,
            format!("{counts} {}", mission_status_label(result)),
        )
    } else {
        (
            crate::theme::success_color(ui.visuals()),
            mission_status_label(result),
        )
    };

    semantic_frame(ui, color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).strong().size(19.0).color(color));
            ui.label(RichText::new(detail));
        });
        // Why no feasible design was found, with the worst violated constraint
        // against its limit and the per-constraint rejection counts.
        if let Some(line) = result
            .optimization_result
            .as_ref()
            .and_then(|optimization| optimization.infeasibility_summary())
        {
            ui.label(
                RichText::new(crate::scene::localize_infeasibility(&line).unwrap_or(line)).small(),
            );
        }
        if completed {
            for blocker in &delivery.blockers {
                ui.add(
                    egui::Label::new(
                        RichText::new(blocker_text(blocker)).color(ui.visuals().error_fg_color),
                    )
                    .wrap(),
                );
            }
            for advisory in &delivery.advisories {
                ui.add(
                    egui::Label::new(RichText::new(advisory).color(ui.visuals().warn_fg_color))
                        .wrap(),
                );
            }
            for note in route_notes(result) {
                ui.add(
                    egui::Label::new(RichText::new(note).color(ui.visuals().warn_fg_color)).wrap(),
                );
            }
        }
        // Every finding with its numbers (the engineering warnings such as a
        // held engine above its requirement are not otherwise readable).
        if completed && !result.feasibility.findings.is_empty() {
            egui::CollapsingHeader::new(RichText::new(&counts).small())
                .id_salt("feasibility-findings")
                .default_open(warnings > 0 && errors == 0)
                .open(crate::perf_tour::force_open())
                .show(ui, |ui| {
                    for finding in &result.feasibility.findings {
                        let color = match finding.severity {
                            FindingSeverity::Error => ui.visuals().error_fg_color,
                            FindingSeverity::Warning => ui.visuals().warn_fg_color,
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(tidy_kg_values(&crate::scene::localize_finding_message(
                                    &finding.message,
                                )))
                                .color(color),
                            )
                            .wrap(),
                        );
                    }
                });
        }
    });
}

/// Notes on a route that did not come from the live dispatch service although
/// an account was configured.
pub(super) fn route_notes(result: &alas_pipeline::PipelineResult) -> Vec<String> {
    use alas_route::simbrief::SimbriefFetchStatus;
    let Some(status) = result.route_status.as_ref() else {
        return Vec::new();
    };
    match &status.simbrief {
        SimbriefFetchStatus::TransportFailed(reason)
            if reason == alas_pipeline::LIVE_DISPATCH_NOT_ENABLED =>
        {
            vec![tr(
                "Live dispatch was not enabled for this run; the route did not come from SimBrief.",
            )]
        }
        SimbriefFetchStatus::TransportFailed(_) => {
            vec![tr(
                "The SimBrief request failed; the route did not come from SimBrief.",
            )]
        }
        SimbriefFetchStatus::ResponseRejected => {
            vec![tr(
                "The SimBrief response was rejected; the route did not come from SimBrief.",
            )]
        }
        _ => Vec::new(),
    }
}

pub(super) fn mission_status_label(result: &alas_pipeline::PipelineResult) -> String {
    let status = match result.feasibility.fuel_loading.mission.status {
        MissionFuelStatus::NotRequested => "Mission not requested",
        MissionFuelStatus::Unavailable => "Mission unavailable",
        MissionFuelStatus::NotConverged => "Mission did not converge",
        MissionFuelStatus::Completed => "Mission completed",
        MissionFuelStatus::Exhausted => "Mission stopped: fuel exhausted",
    };
    tr(status)
}

pub(super) fn maximum_mission_range_km(result: &alas_pipeline::PipelineResult) -> Option<f64> {
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

pub(super) fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 245.0).floor() as usize).clamp(1, 4)
}

pub(super) fn show_stat_tiles<L: AsRef<str>>(ui: &mut Ui, metrics: &[(L, String)]) {
    if metrics.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(metrics.len());
    for row in metrics.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                stat_tile(&mut columns[index], label.as_ref(), value.clone());
            }
        });
        ui.add_space(8.0);
    }
}

pub(super) fn section_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(tr(title)).strong().size(18.0));
    ui.add_space(5.0);
}

/// Show the masses a finding states with three decimals ("73112.705 kg") the
/// way the rest of the page does: grouped thousands and no decimals from
/// 100 kg up, one decimal below.
pub(super) fn tidy_kg_values(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() && (i == 0 || !(chars[i - 1].is_ascii_digit() || chars[i - 1] == '.')) {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let int_end = j;
            let is_three_decimals = j + 4 <= chars.len()
                && chars[j] == '.'
                && chars[j + 1..j + 4].iter().all(char::is_ascii_digit)
                && chars.get(j + 4).is_none_or(|c| !c.is_ascii_digit())
                && chars[j + 4..].iter().take(3).collect::<String>() == " kg";
            if is_three_decimals {
                let number: String = chars[i..j + 4].iter().collect();
                if let Ok(value) = number.parse::<f64>() {
                    if value.abs() >= 100.0 {
                        let rounded = value.round() as i64;
                        let digits = rounded.abs().to_string();
                        let mut grouped = String::new();
                        for (k, d) in digits.chars().enumerate() {
                            if k > 0 && (digits.len() - k) % 3 == 0 {
                                grouped.push(' ');
                            }
                            grouped.push(d);
                        }
                        if rounded < 0 {
                            out.push('-');
                        }
                        out.push_str(&grouped);
                    } else {
                        out.push_str(&format!("{value:.1}"));
                    }
                    i = j + 4;
                    continue;
                }
            }
            out.extend(&chars[i..int_end]);
            i = int_end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod kg_tidy_tests {
    use super::tidy_kg_values;

    #[test]
    fn three_decimal_masses_read_like_the_rest_of_the_page() {
        assert_eq!(
            tidy_kg_values("MTOW 78000.000 kg, payload 73112.705 kg, 1678.923 kg, 12.345 kg, 4.5 t, 7.123 m"),
            "MTOW 78 000 kg, payload 73 113 kg, 1 679 kg, 12.3 kg, 4.5 t, 7.123 m"
        );
    }
}
