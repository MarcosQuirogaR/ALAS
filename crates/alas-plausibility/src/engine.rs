// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Consistency between a held clean-sheet engine and the thrust its own
//! closed design requires. A held engine is the user's choice and the design
//! stays feasible; an engine far above its
//! requirement is a finding to explain, not a refusal.

use alas_config::AlasConfig;
use alas_pipeline::{FindingCode, PhysicalFinding};

use super::{Findings, RuleInfo, Severity};

pub(super) const E_OVERSIZE: RuleInfo = RuleInfo {
    id: "ENGINE-OVERSIZE",
    statement: "A held clean-sheet engine's own closed-design requirement (take-off field, engine-out climbs, design-level cruise) is not more than the engine rating window (default 20 percent) below its rating, i.e. the rating is not above requirement / (1 - window), 25 percent above the requirement at the default window; MEDIUM when it is",
    source: "Jenkinson, Civil Jet Aircraft Design, Sec. 9.10 (an engine is scaled by at most about 20 percent from its design rating, the window reused here as a matching limit); the requirement is the thrust-row bisection of `alas_opt::mdo::engine::sizing`; the severity is a programme rule, not a handbook band",
};

/// The held engine and its requirement, in one unit (kN or kW).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineFacts {
    /// Installed rating.
    pub rating: f64,
    /// Largest rating that still matches the requirement, `required / (1 - window)`.
    pub largest_match: f64,
    /// Rating the requirement asks for.
    pub required: f64,
    /// The rating window, fraction.
    pub window: f64,
    /// `kN` or `kW`.
    pub unit: &'static str,
}

/// Judge a held engine against its requirement: the rating may not exceed
/// `required / (1 - window)`.
pub fn check_engine_oversize(out: &mut Findings, facts: &EngineFacts) {
    let finite = [
        facts.rating,
        facts.largest_match,
        facts.required,
        facts.window,
    ]
    .iter()
    .all(|v| v.is_finite());
    if finite && facts.rating <= facts.largest_match {
        return;
    }
    out.add(
        E_OVERSIZE.id,
        Severity::Medium,
        format!(
            "held engine {:.1} {unit} is {:.1} percent above its requirement {:.1} {unit}; the requirement may be at most {:.0} percent below the rating, i.e. a rating up to {:.1} {unit}",
            facts.rating,
            100.0 * (facts.rating / facts.required - 1.0),
            facts.required,
            100.0 * facts.window,
            facts.largest_match,
            unit = facts.unit,
        ),
    );
}

/// Run the check on a pipeline result: the run's own feasibility carries the
/// warning finding `engine_oversize` (a held clean-sheet engine beyond the
/// rating window, evaluated by the same assessment the report shows), so the
/// rule reads that finding instead of assessing the design a second time.
pub fn judge_engine(out: &mut Findings, config: &AlasConfig, findings: &[PhysicalFinding]) {
    let window = config.optimizer.design_space.engine_rating_window;
    for finding in findings
        .iter()
        .filter(|finding| finding.code == FindingCode::EngineOversize)
    {
        let (Some(rating), Some(largest_match)) = (finding.actual, finding.limit) else {
            continue;
        };
        check_engine_oversize(
            out,
            &EngineFacts {
                rating,
                largest_match,
                required: largest_match * (1.0 - window),
                window,
                unit: if finding.unit == "kW" { "kW" } else { "kN" },
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(rating: f64) -> EngineFacts {
        EngineFacts {
            rating,
            largest_match: 100.0 / 0.8,
            required: 100.0,
            window: 0.2,
            unit: "kN",
        }
    }

    #[test]
    fn an_engine_inside_the_window_is_not_flagged() {
        let mut out = Findings::new("t");
        check_engine_oversize(&mut out, &facts(110.0));
        check_engine_oversize(&mut out, &facts(125.0));
        assert!(out.items().is_empty());
    }

    #[test]
    fn an_engine_above_the_window_is_a_medium_finding_with_the_numbers() {
        let mut out = Findings::new("t");
        check_engine_oversize(&mut out, &facts(140.0));
        assert_eq!(out.severity_of(E_OVERSIZE.id), Some(Severity::Medium));
        let text = &out.items()[0].observation;
        assert!(
            text.contains("140.0 kN") && text.contains("40.0 percent"),
            "{text}"
        );
        assert!(
            text.contains("100.0 kN") && text.contains("125.0 kN"),
            "{text}"
        );
    }
}
