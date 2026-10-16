// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise levels the mission plan flies and the thrust margin there.
//!
//! No new physics: the figures are the ones the feasibility checks already
//! derive ([`alas_pipeline::thrust_checks`], the same call the acceptance
//! audit uses): the initial and step levels of the frozen plan, the highest
//! level at which level flight at the maximum-cruise rating still holds at the
//! initial mass and Mach, and the installed-over-required static thrust ratio
//! at the worst (governing) planned level. Flight levels are the model's
//! altitudes in hundreds of feet, `FL = h / 0.3048 m / 100`, on the ISA day
//! the plan assumes.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use alas_opt::mdo::{ThrustAssessment, ThrustMessage, ThrustVerdict};
use alas_pipeline::PipelineResult;

use crate::views::{tr, tr_fields};

use super::metrics::selected_analysis;

/// Metres per foot.
const M_PER_FT: f64 = 0.3048;

/// The language-independent numbers behind the rows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CruiseLevelSummary {
    /// Planned levels in flight order, as flight levels (distinct, in order).
    pub levels_fl: Vec<f64>,
    /// Whether the levels are the mission plan's; otherwise the design level
    /// at the mid-cruise weight stands in for it.
    pub from_plan: bool,
    /// Highest level at which level flight holds, as a flight level.
    pub highest_fl: Option<f64>,
    /// The governing (worst) planned level, as a flight level.
    pub governing_fl: Option<f64>,
    /// Installed over required static thrust at the governing level, minus
    /// one, in percent. Positive is margin.
    pub margin_pct: f64,
    /// The verdict against the engine's documented thrust scatter.
    pub verdict: ThrustVerdict,
    /// The documented scatter, percent (0 for an engine with a datum).
    pub scatter_pct: f64,
    /// The model's explanation of a shortfall (warning or hard).
    pub shortfall_note: Option<ThrustMessage>,
}

/// The model's thrust sentence in the display language: its English
/// templates are the catalogue keys, the numbers pass through. The model
/// crates stay presentation-free; the lookup lives here.
pub(super) fn localize_thrust_message(message: &ThrustMessage) -> String {
    const TEMPLATE: &str = "{what}: required {required} {unit}, available {available} {unit} (short by {short} %); {basis} +-{scatter} %, so a shortfall inside that band is a warning, not a failure";
    let template = alas_i18n::t(Some(TEMPLATE), None).into_owned();
    let values = [
        ("what", message.what.clone()),
        ("required", message.required.clone()),
        ("available", message.available.clone()),
        ("unit", message.unit.clone()),
        ("short", message.short_pct.clone()),
        (
            "basis",
            alas_i18n::t(Some(message.basis), None).into_owned(),
        ),
        ("scatter", message.scatter_pct.clone()),
    ];
    // One pass, so a substituted value containing braces is never expanded.
    let mut out = String::new();
    let mut rest = template.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        match values.iter().find_map(|(name, value)| {
            tail.strip_prefix(name)?
                .strip_prefix('}')
                .map(|a| (value, a))
        }) {
            Some((value, after)) => {
                out.push_str(value);
                rest = after;
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn flight_level(altitude_m: f64) -> f64 {
    altitude_m / M_PER_FT / 100.0
}

/// Derive the summary; `None` for a turboprop (the jet checks do not apply)
/// or when the engine deck or the carried drag cannot be evaluated.
fn compute(result: &PipelineResult) -> Option<CruiseLevelSummary> {
    let config = &result.config;
    if matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    ) {
        return None;
    }
    let report = selected_analysis(result)?;
    let checks = alas_pipeline::thrust_checks(config, report, &result.feasibility);
    let assessment: ThrustAssessment = checks.level.ok()?;
    let required = assessment.required;
    if !(required.is_finite() && required > 0.0 && assessment.available.is_finite()) {
        return None;
    }
    let mut levels_fl: Vec<f64> = Vec::new();
    let (from_plan, highest_fl, governing_fl) = match &checks.plan {
        Some(plan) => {
            for level in &plan.levels {
                let fl = flight_level(level.altitude_m);
                if levels_fl.last().is_none_or(|last| (last - fl).abs() > 0.5) {
                    levels_fl.push(fl);
                }
            }
            (
                true,
                plan.highest_level_m.map(flight_level),
                Some(flight_level(plan.governing.altitude_m)),
            )
        }
        None => (false, None, None),
    };
    Some(CruiseLevelSummary {
        levels_fl,
        from_plan,
        highest_fl,
        governing_fl,
        margin_pct: 100.0 * (assessment.available / required - 1.0),
        verdict: assessment.verdict(),
        scatter_pct: 100.0 * assessment.scatter,
        shortfall_note: (assessment.verdict() != ThrustVerdict::Pass).then(|| {
            assessment.describe_template(
                "Level cruise at the worst planned level (static T/W)",
                "T/W",
            )
        }),
    })
}

type Memo = Option<(u64, Option<CruiseLevelSummary>)>;

thread_local! {
    /// The last summary with the key of the result it describes: the checks
    /// run the mission model and a level search, too slow for every frame.
    static MEMO: RefCell<Memo> = const { RefCell::new(None) };
}

/// A fingerprint of the inputs of [`compute`] that distinguish one result
/// from the next held in the same place.
fn key(result: &PipelineResult) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (result as *const PipelineResult as usize).hash(&mut hasher);
    let fuel = &result.feasibility.fuel_loading;
    for value in [
        fuel.analyzed_takeoff_mass_kg,
        fuel.analyzed_landing_mass_kg.unwrap_or(f64::NAN),
        fuel.analyzed_carried_fuel_kg,
        result.config.requirements.cruise_mach,
        result.config.mission.profile.cruise_1_air_speed_m_s,
    ] {
        value.to_bits().hash(&mut hasher);
    }
    result.config.preset.hash(&mut hasher);
    result.optimized_report.is_some().hash(&mut hasher);
    result.feasibility.findings.len().hash(&mut hasher);
    hasher.finish()
}

/// The (memoised) summary of `result`.
pub(super) fn cruise_level_summary(result: &PipelineResult) -> Option<CruiseLevelSummary> {
    let key = key(result);
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if let Some((cached, summary)) = memo.as_ref() {
            if *cached == key {
                return summary.clone();
            }
        }
        let summary = compute(result);
        *memo = Some((key, summary.clone()));
        summary
    })
}

fn fl_text(fl: f64) -> String {
    format!("FL{fl:03.0}")
}

/// Tile rows for the cruise-level section.
pub(super) fn cruise_level_rows(summary: &CruiseLevelSummary) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    if summary.from_plan {
        if let Some((first, steps)) = summary.levels_fl.split_first() {
            rows.push(("Initial cruise level", fl_text(*first)));
            rows.push((
                "Step climb levels",
                if steps.is_empty() {
                    tr("None (single level)")
                } else {
                    steps
                        .iter()
                        .map(|fl| fl_text(*fl))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            ));
        }
        rows.push((
            "Highest reachable level",
            summary.highest_fl.map_or_else(
                || tr("None (no level holds level flight)"),
                |fl| {
                    tr_fields(
                        "{level} at the initial mass and Mach",
                        &[("level", fl_text(fl))],
                    )
                },
            ),
        ));
    } else {
        rows.push((
            "Planned cruise levels",
            tr("Not available (no mission plan; design level checked)"),
        ));
    }
    let at = summary
        .governing_fl
        .map_or_else(|| tr("design level"), fl_text);
    let mut value = tr_fields(
        "{margin} % at {level}",
        &[
            ("margin", format!("{:+.1}", summary.margin_pct)),
            ("level", at),
        ],
    );
    match summary.verdict {
        ThrustVerdict::Pass => {}
        ThrustVerdict::Warning => {
            value.push_str(&tr_fields(
                " (short, within the +-{scatter} % chart scatter)",
                &[("scatter", format!("{:.1}", summary.scatter_pct))],
            ));
        }
        ThrustVerdict::Hard => value.push_str(&tr(" (short beyond the scatter band)")),
    }
    rows.push(("Cruise thrust margin (worst planned level)", value));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(margin_pct: f64, verdict: ThrustVerdict) -> CruiseLevelSummary {
        CruiseLevelSummary {
            levels_fl: vec![350.0, 370.0, 390.0],
            from_plan: true,
            highest_fl: Some(410.0),
            governing_fl: Some(350.0),
            margin_pct,
            verdict,
            scatter_pct: 5.1,
            shortfall_note: None,
        }
    }

    fn get(rows: &[(&'static str, String)], label: &str) -> String {
        rows.iter().find(|(l, _)| *l == label).unwrap().1.clone()
    }

    #[test]
    fn rows_label_levels_as_flight_levels_and_the_margin_in_percent() {
        let rows = cruise_level_rows(&summary(12.34, ThrustVerdict::Pass));
        assert_eq!(get(&rows, "Initial cruise level"), "FL350");
        assert_eq!(get(&rows, "Step climb levels"), "FL370, FL390");
        assert_eq!(
            get(&rows, "Highest reachable level"),
            "FL410 at the initial mass and Mach"
        );
        assert_eq!(
            get(&rows, "Cruise thrust margin (worst planned level)"),
            "+12.3 % at FL350"
        );
    }

    #[test]
    fn a_shortfall_states_whether_it_is_inside_the_scatter_band() {
        let rows = cruise_level_rows(&summary(-3.0, ThrustVerdict::Warning));
        let margin = &rows.last().unwrap().1;
        assert!(margin.starts_with("-3.0 % at FL350"), "{margin}");
        assert!(
            margin.contains("within the +-5.1 % chart scatter"),
            "{margin}"
        );
        let rows = cruise_level_rows(&summary(-9.0, ThrustVerdict::Hard));
        assert!(rows.last().unwrap().1.contains("beyond the scatter band"));
    }

    #[test]
    fn a_single_level_plan_has_no_step_levels_and_a_missing_ceiling_is_stated() {
        let mut single = summary(5.0, ThrustVerdict::Pass);
        single.levels_fl = vec![330.0];
        single.highest_fl = None;
        let rows = cruise_level_rows(&single);
        assert_eq!(get(&rows, "Step climb levels"), "None (single level)");
        assert_eq!(
            get(&rows, "Highest reachable level"),
            "None (no level holds level flight)"
        );
    }

    #[test]
    fn the_thrust_sentence_keeps_the_numbers_and_the_english_text() {
        let message = ThrustAssessment::new(1.0, 0.97, 0.051).describe_template("x", "T/W");
        assert_eq!(localize_thrust_message(&message), message.english);
    }

    #[test]
    fn flight_level_is_hundreds_of_feet() {
        assert!((flight_level(10_668.0) - 350.0).abs() < 1.0e-9);
    }
}
