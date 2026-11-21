// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Automated plausibility tier.
//!
//! A result inside a 10 % reference band can still be physically absurd (a
//! mission with no cruise, a cabin floor that slopes, seats outside the
//! fuselage). The rules here are generic engineering sanity limits taken from
//! the certification basis or from handbook ranges; none is keyed to an
//! aircraft, none is fitted to a preset. Each rule has a stable id, a
//! statement, a source and a severity, so a finding reads the same in the
//! audit report, the test output and the release gate.
//!
//! Severity:
//! - `High`: physically impossible or clearly absurd for the aircraft class.
//! - `Medium`: outside the usual range, needs a physical explanation.
//! - `Low`: unusual; informational.
//!
//! The rules take plain data (`SegmentFacts`, curves, corners), so each is
//! unit-tested on synthetic good and bad inputs; [`case`] extracts that data
//! from a pipeline run.

pub mod aero;
pub mod case;
pub mod climb_reference;
pub mod engine;
pub mod figures;
pub mod geometry;
pub mod leg_rules;
pub mod mass;
pub mod mission;
pub mod mission_climb;
pub mod mission_facts;
pub mod mission_priced;
pub mod oew_cg;
pub mod takeoff_leg;

use std::fmt;

/// How bad a finding is. Ordered: `High` is the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Unusual but possible; informational.
    Low,
    /// Outside the usual range; needs a physical explanation.
    Medium,
    /// Physically impossible or clearly absurd for the class.
    High,
}

impl Severity {
    /// Upper-case label used in reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }
}

/// One documented rule.
#[derive(Debug, Clone, Copy)]
pub struct RuleInfo {
    /// Stable identifier, `<DOMAIN>-<NAME>`.
    pub id: &'static str,
    /// The statement the rule enforces, with its numeric limits.
    pub statement: &'static str,
    /// Where the limit comes from.
    pub source: &'static str,
}

/// A rule violated by one case.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Case label, `<aircraft>@<mtow mode>`.
    pub case: String,
    /// Rule identifier, a key of [`RULES`].
    pub rule: &'static str,
    /// Severity of this violation.
    pub severity: Severity,
    /// What was observed, with numbers and units.
    pub observation: String,
    /// Which of the rule's distinct failure modes this is, when the rule has
    /// more than one (the violated constraints of a CG state, for example);
    /// empty otherwise. Two findings of one rule with different kinds are
    /// different findings.
    pub kind: String,
    /// How far past the rule's limit the observation lies, in the unit the
    /// rule states, larger being worse; `None` for a rule that is a plain
    /// yes or no.
    pub magnitude: Option<f64>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}: {}",
            self.severity.label(),
            self.case,
            self.rule,
            self.observation
        )
    }
}

/// Collector the rule functions write into.
#[derive(Debug, Default)]
pub struct Findings {
    case: String,
    items: Vec<Finding>,
}

impl Findings {
    /// An empty collector for `case`.
    pub fn new(case: impl Into<String>) -> Self {
        Self {
            case: case.into(),
            items: Vec::new(),
        }
    }

    /// Record a violation of `rule`.
    pub fn add(&mut self, rule: &'static str, severity: Severity, observation: String) {
        debug_assert!(
            RULES.iter().any(|r| r.id == rule),
            "unregistered plausibility rule {rule}"
        );
        self.add_measured(rule, severity, "", None, observation);
    }

    /// Record a violation of `rule` with its failure mode and how far past
    /// the limit it lies (see [`Finding::kind`] and [`Finding::magnitude`]).
    pub fn add_measured(
        &mut self,
        rule: &'static str,
        severity: Severity,
        kind: &str,
        magnitude: Option<f64>,
        observation: String,
    ) {
        debug_assert!(
            RULES.iter().any(|r| r.id == rule),
            "unregistered plausibility rule {rule}"
        );
        self.items.push(Finding {
            case: self.case.clone(),
            rule,
            severity,
            observation,
            kind: kind.to_owned(),
            magnitude,
        });
    }

    /// The case label.
    pub fn case(&self) -> &str {
        &self.case
    }

    /// Append the findings of another collector.
    pub fn extend(&mut self, other: Findings) {
        self.items.extend(other.items);
    }

    /// The findings recorded so far.
    pub fn items(&self) -> &[Finding] {
        &self.items
    }

    /// Consume the collector.
    pub fn into_items(self) -> Vec<Finding> {
        self.items
    }

    /// Whether `rule` was violated.
    pub fn has(&self, rule: &str) -> bool {
        self.items.iter().any(|f| f.rule == rule)
    }

    /// The highest severity recorded for `rule`.
    pub fn severity_of(&self, rule: &str) -> Option<Severity> {
        self.items
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.severity)
            .max()
    }
}

/// Every rule of the tier, in report order.
pub static RULES: &[RuleInfo] = &[
    mission::M_COMPLETE,
    mission::M_MONOTONIC,
    mission::M_CRUISE_PRESENT,
    mission::M_CRUISE_SHARE,
    mission::M_CLIMB_RATE,
    mission::M_DESCENT_RATE,
    mission::M_DESCENT_TIME,
    mission::M_SPEED_CHANGE,
    mission::M_PATH_ANGLE,
    mission::M_STEP_CLIMB,
    mission::M_CONTINUITY,
    mission::M_MACH,
    mission::M_LOW_ALT_SPEED,
    mission::M_CRUISE_LD,
    mission::M_CRUISE_CL,
    mission_priced::P_FLOWN,
    mission_priced::P_DECLARED,
    mission_priced::P_FUEL,
    mission_priced::P_TIME,
    mission_priced::P_LEVEL,
    mission_priced::P_FOOTPRINT,
    leg_rules::P_TAKEOFF,
    leg_rules::P_RESTRICTION,
    aero::A_PR_ORDER,
    aero::A_PR_MONOTONIC,
    aero::A_POLAR_CONVEX,
    aero::A_POLAR_CD0,
    aero::A_POLAR_LD,
    aero::A_CLMAX,
    aero::A_VN_ORDER,
    aero::A_VN_LOAD_FACTORS,
    aero::A_VSPEED_ORDER,
    aero::A_VSPEED_MARGINS,
    mass::W_CG_ENVELOPE,
    mass::W_GEAR_MAIN_REACTION,
    mass::W_GEAR_AERO_AFT_BEHIND_GEAR,
    mass::W_STATIC_MARGIN,
    mass::W_STATIC_MARGIN_TOO_STABLE,
    mass::W_STATIC_MARGIN_UNSTABLE,
    mass::W_PUBLISHED_AFT_LIMIT,
    oew_cg::W_OEW_CG_REFERENCE,
    mass::W_MASS_CLOSURE,
    mass::W_FREQUENCY_ORDER,
    mass::W_DEFLECTION_SHAPE,
    geometry::G_FLOOR_LEVEL,
    geometry::G_SEATS_IN_CONTOUR,
    geometry::G_AISLE_WIDTH,
    geometry::G_CARGO_IN_CONTOUR,
    geometry::G_NO_OVERLAP,
    geometry::G_TAILCONE_MONOTONIC,
    geometry::G_SECTION_SMOOTH,
    geometry::G_NOSE_MONOTONIC,
    figures::F_SPAN_LIFT,
    figures::P_LAPSE,
    engine::E_OVERSIZE,
];

/// The rules as a Markdown table (the source of `plausibility-rules.md`).
pub fn rules_markdown() -> String {
    let mut text = String::from("| Rule | Statement | Source |\n|---|---|---|\n");
    for rule in RULES {
        text.push_str(&format!(
            "| {} | {} | {} |\n",
            rule.id, rule.statement, rule.source
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_ids_are_unique_and_documented() {
        for (index, rule) in RULES.iter().enumerate() {
            assert!(
                !rule.statement.is_empty() && !rule.source.is_empty(),
                "{}",
                rule.id
            );
            assert!(
                RULES
                    .iter()
                    .skip(index + 1)
                    .all(|other| other.id != rule.id),
                "duplicate rule {}",
                rule.id
            );
        }
    }

    #[test]
    fn severity_orders_high_above_low() {
        assert!(Severity::High > Severity::Medium && Severity::Medium > Severity::Low);
    }
}
