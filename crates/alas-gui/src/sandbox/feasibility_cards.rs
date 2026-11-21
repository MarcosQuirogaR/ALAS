// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Quick Analysis feasibility result as a few cards, one per check
//! category: a pass, warning or fail badge, each raised check as one line with
//! its value against its limit, and a one-line hint on what to change. The
//! full explanation of a check is its hover text, never a wall of text.

use alas_pipeline::quick_analysis::{QuickFeasibility, QuickFlag};
use egui::{Color32, RichText, Ui};

use crate::views::tr;

/// The check categories, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    MassFuel,
    Balance,
    Stability,
    Structure,
    Performance,
    Other,
}

impl Category {
    const SHOWN_ALWAYS: [Category; 5] = [
        Category::MassFuel,
        Category::Balance,
        Category::Stability,
        Category::Structure,
        Category::Performance,
    ];

    fn title(self) -> &'static str {
        match self {
            Category::MassFuel => "Mass, fuel and payload",
            Category::Balance => "Balance and landing gear",
            Category::Stability => "Stability and control",
            Category::Structure => "Structure and aerodynamics",
            Category::Performance => "Performance and mission",
            Category::Other => "Other checks",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Category::MassFuel => "Reduce payload or fuel, or raise the mass limit.",
            Category::Balance => {
                "Move the wing, landing gear or payload to bring the centre of gravity into range."
            }
            Category::Stability => {
                "Resize the tail surfaces or move the wing to restore the margin."
            }
            Category::Structure => "Revisit the wing sizing or lower the cruise Mach number.",
            Category::Performance => "Add thrust or wing area, or use a longer field.",
            Category::Other => "Run the Full Analysis for the detail.",
        }
    }
}

/// The category a check belongs to, from its stable code.
pub(super) fn category_of(code: &str) -> Category {
    let has = |words: &[&str]| words.iter().any(|word| code.contains(word));
    if has(&["Cg", "Gear", "TipBack", "TailScrape"]) {
        Category::Balance
    } else if has(&[
        "StaticMargin",
        "Trim",
        "Directional",
        "Crosswind",
        "MinimumControl",
    ]) {
        Category::Stability
    } else if has(&["Structural", "Aero", "WingArea", "EnvelopeSpeed"]) {
        Category::Structure
    } else if has(&[
        "Mass",
        "Fuel",
        "Tank",
        "Payload",
        "Baggage",
        "Cargo",
        "Passenger",
        "Monument",
        "Dispatch",
        "SizingNotClosed",
    ]) {
        Category::MassFuel
    } else if has(&[
        "Field", "Thrust", "Throttle", "Mission", "Cruise", "Engine", "Takeoff", "Landing",
    ]) {
        Category::Performance
    } else {
        Category::Other
    }
}

/// A stable code such as `NoseGearStrengthViolation` as a sentence-case title.
pub(super) fn code_title(code: &str) -> String {
    let mut out = String::new();
    for (index, ch) in code.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else if index == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// `value` with a precision that suits its size.
fn quantity(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 100.0 {
        format!("{value:.0}")
    } else if magnitude >= 1.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}

/// "actual vs limit unit" for a quantitative check.
pub(super) fn value_against_limit(flag: &QuickFlag) -> Option<String> {
    let (actual, limit) = (flag.actual?, flag.limit?);
    if !actual.is_finite() || !limit.is_finite() {
        return None;
    }
    let unit = if flag.unit.is_empty() {
        String::new()
    } else {
        format!(" {}", flag.unit)
    };
    Some(crate::views::tr_fields(
        "{actual} (limit {limit})",
        &[
            ("actual", format!("{}{unit}", quantity(actual))),
            ("limit", format!("{}{unit}", quantity(limit))),
        ],
    ))
}

/// How a category came out: no raised check, only warnings, or a blocking one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Pass,
    Warning,
    Fail,
}

pub(super) fn verdict_of(flags: &[&QuickFlag]) -> Verdict {
    if flags.iter().any(|flag| flag.blocking) {
        Verdict::Fail
    } else if flags.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Warning
    }
}

fn badge(ui: &mut Ui, verdict: Verdict) {
    let (text, color): (&str, Color32) = match verdict {
        Verdict::Pass => ("Pass", crate::theme::success_color(ui.visuals())),
        Verdict::Warning => ("Warning", ui.visuals().warn_fg_color),
        Verdict::Fail => ("Fail", ui.visuals().error_fg_color),
    };
    egui::Frame::none()
        .fill(color.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.7)))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(8.0, 1.0))
        .show(ui, |ui| {
            ui.label(RichText::new(tr(text)).small().strong().color(color));
        });
}

/// The cards of one feasibility result.
pub(super) fn show_feasibility_cards(ui: &mut Ui, feasibility: &QuickFeasibility) {
    let (text, color) = if feasibility.feasible {
        (
            tr("No blocking flag"),
            crate::theme::success_color(ui.visuals()),
        )
    } else {
        (tr("Blocking flags raised"), ui.visuals().error_fg_color)
    };
    ui.label(RichText::new(text).color(color).strong());
    let mut categories: Vec<Category> = Category::SHOWN_ALWAYS.to_vec();
    if feasibility
        .flags
        .iter()
        .any(|flag| category_of(&flag.code) == Category::Other)
    {
        categories.push(Category::Other);
    }
    for category in categories {
        let flags: Vec<&QuickFlag> = feasibility
            .flags
            .iter()
            .filter(|flag| category_of(&flag.code) == category)
            .collect();
        let verdict = verdict_of(&flags);
        egui::Frame::none()
            .stroke(egui::Stroke::new(
                1.0_f32,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ))
            .rounding(8.0)
            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(category.title())).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        badge(ui, verdict);
                    });
                });
                for flag in &flags {
                    let color = if flag.blocking {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().warn_fg_color
                    };
                    ui.label(RichText::new(code_title(&flag.code)).color(color))
                        .on_hover_text(&flag.message);
                    if let Some(values) = value_against_limit(flag) {
                        ui.label(RichText::new(values).weak().small());
                    }
                }
                if verdict != Verdict::Pass {
                    ui.label(RichText::new(tr(category.hint())).weak().small());
                }
            });
        ui.add_space(3.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(code: &str, blocking: bool, actual: Option<f64>, limit: Option<f64>) -> QuickFlag {
        QuickFlag {
            code: code.to_owned(),
            blocking,
            message: "a long explanation".to_owned(),
            actual,
            limit,
            unit: "kg".to_owned(),
        }
    }

    #[test]
    fn codes_fall_into_the_categories_a_user_would_expect() {
        assert_eq!(category_of("NoseGearStrengthViolation"), Category::Balance);
        assert_eq!(
            category_of("ModelCgForwardRangeViolation"),
            Category::Balance
        );
        assert_eq!(category_of("InsufficientStaticMargin"), Category::Stability);
        assert_eq!(
            category_of("StructuralStrengthViolation"),
            Category::Structure
        );
        assert_eq!(category_of("LandingMassLimitViolation"), Category::MassFuel);
        assert_eq!(category_of("MissionFuelShortfall"), Category::MassFuel);
        assert_eq!(
            category_of("FieldTakeoffDistanceViolation"),
            Category::Performance
        );
        assert_eq!(category_of("Dispatch"), Category::MassFuel);
        assert_eq!(category_of("Unlisted"), Category::Other);
    }

    #[test]
    fn a_code_reads_as_a_sentence_and_a_quantity_as_value_against_limit() {
        assert_eq!(
            code_title("NoseGearStrengthViolation"),
            "Nose gear strength violation"
        );
        let text = value_against_limit(&flag("X", true, Some(81234.5), Some(79000.0)))
            .expect("quantitative");
        assert!(
            text.starts_with("81234 kg") && text.ends_with("(limit 79000 kg)"),
            "{text}"
        );
        assert!(value_against_limit(&flag("X", true, None, None)).is_none());
    }

    #[test]
    fn a_category_is_a_pass_only_without_raised_checks() {
        let warning = flag("A", false, None, None);
        let blocking = flag("B", true, None, None);
        assert_eq!(verdict_of(&[]), Verdict::Pass);
        assert_eq!(verdict_of(&[&warning]), Verdict::Warning);
        assert_eq!(verdict_of(&[&warning, &blocking]), Verdict::Fail);
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::test_raster::{render, write_in};

    #[test]
    fn the_cards_render_for_a_look() {
        let flags = QuickFeasibility {
            feasible: false,
            flags: vec![
                QuickFlag {
                    code: "NoseGearStrengthViolation".to_owned(),
                    blocking: true,
                    message: "A very long explanation that must stay in the hover text only."
                        .to_owned(),
                    actual: Some(81234.5),
                    limit: Some(79000.0),
                    unit: "N".to_owned(),
                },
                QuickFlag {
                    code: "InsufficientStaticMargin".to_owned(),
                    blocking: false,
                    message: "x".to_owned(),
                    actual: Some(2.1),
                    limit: Some(5.0),
                    unit: "% MAC".to_owned(),
                },
                QuickFlag {
                    code: "Dispatch".to_owned(),
                    blocking: true,
                    message: "MTOW-limited by 1200 kg".to_owned(),
                    actual: None,
                    limit: None,
                    unit: String::new(),
                },
            ],
        };
        let shot = render(330.0, 640.0, 2, 0, |ui| {
            ui.set_width(310.0);
            show_feasibility_cards(ui, &flags);
        });
        write_in(
            &crate::test_raster::render_dir(),
            "c9-feasibility-cards.png",
            &shot,
        );
        assert!(shot.texts.iter().any(|(text, _)| text == "Fail"));
        assert!(shot.texts.iter().any(|(text, _)| text == "Pass"));
        let overlaps = crate::test_raster::overlapping_texts(&shot);
        assert!(overlaps.is_empty(), "{overlaps:?}");
    }
}
