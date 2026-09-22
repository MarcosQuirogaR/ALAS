// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The D02 engineering review: which limits may be missed, and on what basis.
//!
//! The clarified ledger's D02 asks for "an engineering-reviewed eligibility
//! list and per-limit tolerances", and is explicit that these are engineering
//! work rather than "arbitrary global percentages". This module is that
//! review, recorded per limit so the result is auditable instead of asserted.
//!
//! **The review's present outcome is that no limit is eligible.** That is a
//! finding, not an omission, and it is why [`ConstraintRelaxation::eligible`]
//! ships empty. Each entry below says which of three states its limit is in
//! and why:
//!
//! - [`RelaxationReview::NeverRelaxable`] - the residual reports an
//!   incomplete or failed evaluation, or is a boolean availability flag.
//!   There is no aircraft, or no measured quantity, behind it for a fraction
//!   to apply to. No future review can change this, so it is also enforced at
//!   the point of use by [`super::relaxation::NON_RELAXABLE_RESIDUAL_IDS`].
//! - [`RelaxationReview::Ineligible`] - reviewed, and no traceable primary
//!   engineering or regulatory source states a fraction of this limit that
//!   may be exceeded. A later review that produces such a source may move the
//!   entry; until then the limit stays hard and a configuration listing it is
//!   rejected with the recorded reason.
//! - [`RelaxationReview::Eligible`] - reviewed, with a sourced ceiling on the
//!   tolerance a configuration may declare. **There are currently none.**
//!
//! # Why the review came out empty
//!
//! A tolerance is a statement that a miss of a stated size is not resolvable,
//! and only two kinds of evidence can support one. The first is a published
//! tolerance on the requirement itself. Airworthiness minima do not have one:
//! 14 CFR 25.121(b) prescribes a gradient, ICAO Annex 14 tabulates discrete
//! aerodrome code letters, and a declared maximum weight is established under
//! 14 CFR/CS 25.25 rather than estimated. The second is a measured error band
//! on this program's own model of the quantity. The project's cross-domain
//! physical-validation ledger (an internal audit record dated 2026-09-17)
//! records that no such band exists for any constrained quantity: field
//! lengths are `NotAvailable` because ALAS reports no matched take-off or
//! landing field length at all, static margin and CG envelope are
//! `NotAvailable` 7/7, subsystem masses are `NotAvailable` 7/7, and the
//! propulsion deck's own PSFC sits 21-28 % above the measured record. An
//! unbounded model error cannot justify a bounded tolerance.
//!
//! Writing a percentage anyway is the invented coefficient this product does
//! not ship. The reviewed state is therefore recorded, per limit, as the
//! deliverable.

mod limits;
mod limits_layout;

use std::sync::OnceLock;

use super::relaxation::ConstraintRelaxation;

/// Every residual identifier the optimizer can emit, with its D02
/// determination: [`limits::CORE_LIMITS`] followed by
/// [`limits_layout::LAYOUT_LIMITS`] (`mdo::residuals_layout`'s
/// wing-to-fuselage family, split into its own file so its growth does not
/// compete with this one's for the same budgeted file), joined once and
/// cached so every function below reads one list without restating the join.
pub fn reviewed_limits() -> &'static [ReviewedLimit] {
    static COMBINED: OnceLock<Vec<ReviewedLimit>> = OnceLock::new();
    COMBINED
        .get_or_init(|| {
            limits::CORE_LIMITS
                .iter()
                .chain(limits_layout::LAYOUT_LIMITS)
                .copied()
                .collect()
        })
        .as_slice()
}

/// What the D02 review concluded about one limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelaxationReview {
    /// Never listable, whatever a configuration asks for.
    NeverRelaxable,
    /// Reviewed and not eligible; the entry's rationale says why.
    Ineligible,
    /// Reviewed and eligible, up to this fraction of the limit's magnitude.
    ///
    /// A configuration may declare a smaller tolerance, never a larger one.
    Eligible {
        /// Largest tolerance a configuration may declare for this limit.
        max_tolerance_fraction: f64,
    },
}

/// One limit's D02 determination.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewedLimit {
    /// Residual identifier, as `alas_opt::mdo` spells it.
    pub id: &'static str,
    /// Requirement family, or `"Evaluation"` for a failed or incomplete
    /// evaluation that belongs to no family's requirement set.
    pub family: &'static str,
    /// What the review concluded.
    pub review: RelaxationReview,
    /// Why, in terms a reviewer can check. Never empty.
    pub rationale: &'static str,
}

impl ReviewedLimit {
    /// Whether a configuration may list this limit at all.
    pub const fn is_listable(&self) -> bool {
        matches!(self.review, RelaxationReview::Eligible { .. })
    }
}

/// The review recorded for `id`, or `None` when the identifier is unknown.
///
/// An unknown identifier is a configuration error rather than a limit with no
/// opinion: a typo in a saved document would otherwise be a silent no-op that
/// looks like a policy.
pub fn review_for(id: &str) -> Option<&'static ReviewedLimit> {
    reviewed_limits().iter().find(|limit| limit.id == id)
}

/// How many limits the review covers.
pub fn reviewed_count() -> usize {
    reviewed_limits().len()
}

/// How many limits the review currently admits for relaxation.
///
/// Zero as shipped. See the module documentation for why.
pub fn eligible_count() -> usize {
    reviewed_limits()
        .iter()
        .filter(|limit| limit.is_listable())
        .count()
}

/// A one-line statement of the review's outcome, for a user-facing surface.
///
/// Deliberately says the count rather than "relaxation is unavailable": a
/// reader has to be able to tell a reviewed-and-empty policy from a missing
/// feature.
pub fn review_summary() -> String {
    format!(
        "{} of {} limits reviewed under D02 are currently eligible for relaxation.",
        eligible_count(),
        reviewed_count()
    )
}

/// Configuration issues in the optimizer's two policy groups.
///
/// Both groups own a `validate` that nothing called before this existed, so a
/// document with an inverted plausibility window or an unattributed
/// relaxation entry reached the search unchecked. Returned as plain sentences
/// paired with the field path the interface scrolls to.
pub(crate) fn policy_group_issues(config: &crate::AlasConfig) -> Vec<crate::ValidationIssue> {
    let mut issues = Vec::new();
    if let Err(message) = config.optimizer.plausibility.validate() {
        issues.push(crate::ValidationIssue {
            field_path: "optimizer.plausibility".to_owned(),
            message,
            severity: crate::Severity::Error,
        });
    }
    if let Err(message) = ConstraintRelaxation::validate(&config.optimizer.relaxation) {
        issues.push(crate::ValidationIssue {
            field_path: "optimizer.relaxation".to_owned(),
            message,
            severity: crate::Severity::Error,
        });
    }
    issues
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::relaxation::{RelaxableLimit, NON_RELAXABLE_RESIDUAL_IDS};

    #[test]
    fn the_review_covers_every_identifier_once_and_says_why() {
        let mut seen = std::collections::BTreeSet::new();
        for limit in reviewed_limits() {
            assert!(
                seen.insert(limit.id),
                "{} is reviewed twice; a limit has one determination",
                limit.id
            );
            assert!(
                !limit.rationale.trim().is_empty(),
                "{} has no recorded reason, which is what makes a review a review",
                limit.id
            );
            assert!(
                matches!(
                    limit.family,
                    "Evaluation" | "Mass" | "Balance" | "Performance" | "Geometry"
                ),
                "{} names family {}, which is not a requirement family",
                limit.id,
                limit.family
            );
        }
    }

    #[test]
    fn the_review_currently_admits_nothing_and_says_so() {
        // This is the D02 finding, not a placeholder: see the module
        // documentation for the evidence behind each determination. A future
        // review that produces a sourced tolerance changes this assertion
        // deliberately, which is the point of asserting it.
        assert_eq!(eligible_count(), 0);
        assert!(reviewed_count() >= 50);
        assert!(review_summary().starts_with("0 of "));
    }

    #[test]
    fn the_structural_never_relaxable_set_agrees_with_the_review() {
        for id in NON_RELAXABLE_RESIDUAL_IDS {
            let limit = review_for(id).unwrap_or_else(|| panic!("{id} is not reviewed"));
            assert_eq!(
                limit.review,
                RelaxationReview::NeverRelaxable,
                "{id} is enforced as never relaxable but reviewed otherwise"
            );
        }
        for limit in reviewed_limits() {
            if limit.review == RelaxationReview::NeverRelaxable {
                assert!(
                    NON_RELAXABLE_RESIDUAL_IDS.contains(&limit.id),
                    "{} is reviewed as never relaxable but is not enforced as one",
                    limit.id
                );
            }
        }
    }

    #[test]
    fn an_unknown_or_ineligible_limit_is_rejected_with_its_recorded_reason() {
        let unknown = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![RelaxableLimit {
                id: "wing_are".to_owned(),
                tolerance_fraction: 0.02,
                provenance: "typo".to_owned(),
            }],
        };
        let message = unknown.validate().expect_err("an unknown id is rejected");
        assert!(message.contains("wing_are"), "{message}");

        let ineligible = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![RelaxableLimit {
                id: "takeoff_field".to_owned(),
                tolerance_fraction: 0.02,
                provenance: "a reviewer's opinion".to_owned(),
            }],
        };
        let message = ineligible
            .validate()
            .expect_err("an ineligible id is rejected");
        assert!(
            message.contains("no matched take-off field length"),
            "{message}"
        );
    }

    #[test]
    fn the_shipped_configuration_raises_no_policy_issue() {
        assert!(policy_group_issues(&crate::AlasConfig::default()).is_empty());
    }

    #[test]
    fn an_inverted_plausibility_window_is_a_blocking_configuration_error() {
        let mut config = crate::AlasConfig::default();
        config.optimizer.plausibility.max_aspect_ratio = 1.0;
        let issues = policy_group_issues(&config);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field_path, "optimizer.plausibility");
        assert_eq!(issues[0].severity, crate::Severity::Error);
    }
}
