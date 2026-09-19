// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Controlled constraint relaxation (clarified ledger decisions D01-D03).
//!
//! Mass, Balance, Performance and Geometry stay hard by default, and this
//! group is off by default, so a shipped run behaves exactly as it did before
//! this existed. What it adds is a way for a user who has overconstrained the
//! problem to say, explicitly, that a bounded number of *discipline groups*
//! may be missed - not individual limits, and not by an arbitrary global
//! percentage.
//!
//! The rules, in the ledger's own terms:
//!
//! - **D01, counting.** Violated discipline *groups* are counted, not limits.
//!   Several eligible exceeded limits inside Mass count as one violated
//!   group.
//! - **D02, eligibility and tolerance.** Only a limit on the eligibility list
//!   may be missed, and only by its own declared tolerance. Every entry
//!   carries the provenance of that tolerance, and a limit outside the list
//!   is rejected exactly as before.
//! - **D03, ranking.** A fully feasible design always ranks ahead of a
//!   relaxed one, and a relaxed design is never labelled fully feasible.
//!
//! **The shipped eligibility list is empty, deliberately.** The D02 review
//! itself is recorded in [`super::policy_review`], one determination per
//! residual identifier with the reason and the source behind it, and its
//! present outcome is that no limit is eligible: no primary engineering or
//! regulatory source states a fraction of any of these limits that may be
//! exceeded, and the project's own validation ledger records no measured
//! model-error band that could size one. That module is where a future
//! review adds an entry, and this one is what enforces it.
//!
//! Some limits can never be relaxed whatever a document says, and
//! [`NON_RELAXABLE_RESIDUAL_IDS`] enumerates them: an evaluation that did not
//! complete is not a small violation of anything, and a coupled closure that
//! did not converge is not evidence about an aircraft. Listing one of these
//! is a configuration error, not a licence.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Residual identifiers that are never eligible for relaxation, whatever a
/// configuration asks for.
///
/// Three kinds. The first four are not requirement misses at all: they mean
/// the geometry could not be built, the design vector left its declared
/// space, the coupled sizing did not close, or the dispatch model could not
/// be evaluated. There is no aircraft behind such a candidate to relax a
/// limit on. Then the structural inventory: a wing whose strength-sized box
/// exceeds the whole modelled wing has no non-box remainder, so its mass
/// statement is incomplete rather than merely outside a limit, and admitting
/// it would publish a partial aircraft as a complete one. Last are the
/// boolean availability flags, whose normalized violation is exactly one
/// because an input or a piece of evidence is absent: a fraction of an
/// absent quantity has no meaning, and the tolerance ceiling below means
/// listing one could never admit it anyway. Naming them here turns a silent
/// no-op into the configuration error it is.
///
/// [`super::policy_review`] carries the same set with its per-identifier
/// reasoning, and a test holds the two in agreement.
pub const NON_RELAXABLE_RESIDUAL_IDS: &[&str] = &[
    "geometry_build",
    "design_space",
    "trim_solve",
    "sizing_not_closed",
    "dispatch_not_converged",
    "dispatch_model_failed",
    "structural_inventory_unverified",
    "fuel_capacity_unavailable",
    "cg_model_error",
    "airport_unknown",
    "airport_declared_distance_unavailable",
    "mission_distance_unavailable",
    "oei_part25_engine_count_unsupported",
    "oei_second_segment_evidence_gap",
];

/// Largest relative tolerance any single limit may declare.
///
/// A tenth of the limit is already at the outer edge of what a preliminary
/// design model's own fidelity can distinguish; beyond it the "relaxed"
/// candidate is not near the requirement, it is a different requirement. This
/// is a bound on what a configuration may express, not a default anyone is
/// given.
pub const MAXIMUM_TOLERANCE_FRACTION: f64 = 0.10;

/// One limit an engineering review has declared may be missed, and by how
/// much.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelaxableLimit {
    /// Residual identifier, as `mdo::residuals` spells it.
    pub id: String,
    /// Largest normalized violation that still counts as relaxed rather than
    /// rejected, as a fraction of the limit's own magnitude. The residual
    /// table normalizes every violation the same way, so this is comparable
    /// across quantities with different units.
    pub tolerance_fraction: f64,
    /// Why this limit may be missed by this much, and on whose authority.
    /// A relaxation without a stated source is a rejected configuration.
    pub provenance: String,
}

/// The user's controlled-relaxation policy.
///
/// The derived default is the shipped strict policy: off, no group
/// allowance, nothing eligible. Each of the three is independently
/// sufficient to make [`Self::is_active`] false, so a document that sets one
/// of them without the others still relaxes nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields, default)]
pub struct ConstraintRelaxation {
    /// Whether any limit may be missed at all.
    #[config(
        label = "Allow controlled constraint relaxation",
        help = "Off by default: every Mass, Balance, Performance and Geometry limit is hard. Turning it on lets a bounded number of discipline groups be missed, but only for limits on the reviewed eligibility list and only inside each limit's own declared tolerance. A relaxed design is always reported as relaxed, never as fully feasible, and always ranks behind every fully feasible design."
    )]
    pub enabled: bool,

    /// How many discipline groups may carry a relaxed violation.
    #[config(
        label = "Allowed violated discipline groups",
        help = "Counts groups, not limits: several eligible exceeded limits inside Mass count as one violated group. A candidate that exceeds this count, or that misses any limit not on the eligibility list, is rejected exactly as it would be with relaxation off. There are four groups (Mass, Balance, Performance, Geometry)."
    )]
    pub allowed_violated_groups: i64,

    /// Which limits may be missed, and by how much.
    ///
    /// Empty as shipped. See the module documentation for why.
    #[config(skip)]
    pub eligible: Vec<RelaxableLimit>,
}

impl ConstraintRelaxation {
    /// Whether this is the strict shipped policy, for a serializer that omits
    /// defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Whether the policy can admit anything at all.
    ///
    /// A policy that is switched on but allows zero groups, or lists no
    /// eligible limit, is strict: this is what makes the shipped default a
    /// no-op rather than a behaviour anyone has to opt out of.
    pub fn is_active(&self) -> bool {
        self.enabled && self.allowed_violated_groups > 0 && !self.eligible.is_empty()
    }

    /// The declared tolerance for `id`, when the policy admits that limit.
    ///
    /// `None` for a limit that is not listed, for one on
    /// [`NON_RELAXABLE_RESIDUAL_IDS`], and for every limit when the policy is
    /// not active. The non-relaxable check is repeated here rather than left
    /// to validation alone, so a configuration that reached this code by some
    /// other path still cannot relax one.
    pub fn tolerance_for(&self, id: &str) -> Option<f64> {
        if !self.is_active() || NON_RELAXABLE_RESIDUAL_IDS.contains(&id) {
            return None;
        }
        self.eligible
            .iter()
            .find(|limit| limit.id == id)
            .map(|limit| limit.tolerance_fraction)
    }

    /// Reject a policy that is not physically bounded or not attributable.
    ///
    /// # Errors
    ///
    /// A description of the first offending entry.
    pub fn validate(&self) -> Result<(), String> {
        if self.allowed_violated_groups < 0 {
            return Err(format!(
                "allowed_violated_groups must not be negative, got {}",
                self.allowed_violated_groups
            ));
        }
        for limit in &self.eligible {
            if limit.id.trim().is_empty() {
                return Err("a relaxable limit must name a residual identifier".to_owned());
            }
            if NON_RELAXABLE_RESIDUAL_IDS.contains(&limit.id.as_str()) {
                return Err(format!(
                    "'{}' can never be relaxed: it reports an incomplete or failed evaluation, not a bounded miss of a requirement",
                    limit.id
                ));
            }
            if !limit.tolerance_fraction.is_finite()
                || limit.tolerance_fraction <= 0.0
                || limit.tolerance_fraction > MAXIMUM_TOLERANCE_FRACTION
            {
                return Err(format!(
                    "the tolerance for '{}' must lie in (0, {MAXIMUM_TOLERANCE_FRACTION}], got {}",
                    limit.id, limit.tolerance_fraction
                ));
            }
            if limit.provenance.trim().is_empty() {
                return Err(format!(
                    "the tolerance for '{}' must state its source; an unattributed relaxation is not reviewable",
                    limit.id
                ));
            }
            // The D02 review is consulted last, so the bounds above stay
            // reachable for every identifier, and it is enforced here rather
            // than inside `tolerance_for` because an `Ineligible`
            // determination is a statement about today's evidence: a later
            // review that produces a source moves the entry, and the place
            // that decision is applied is when a configuration is accepted.
            // The structural `NeverRelaxable` set is different and is also
            // enforced at the point of use, because no evidence overturns it.
            let Some(reviewed) = super::policy_review::review_for(&limit.id) else {
                return Err(format!(
                    "'{}' is not a residual this optimizer emits, so no engineering review covers it; an entry naming an unknown limit would silently do nothing",
                    limit.id
                ));
            };
            match reviewed.review {
                super::policy_review::RelaxationReview::Eligible {
                    max_tolerance_fraction,
                } if limit.tolerance_fraction <= max_tolerance_fraction => {}
                super::policy_review::RelaxationReview::Eligible {
                    max_tolerance_fraction,
                } => {
                    return Err(format!(
                        "the tolerance for '{}' must not exceed the reviewed ceiling {max_tolerance_fraction}, got {}",
                        limit.id, limit.tolerance_fraction
                    ))
                }
                _ => {
                    return Err(format!(
                        "'{}' is not eligible for relaxation under the D02 review: {}",
                        limit.id, reviewed.rationale
                    ))
                }
            }
        }
        Ok(())
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory policy entry, used to exercise the mechanism.
    ///
    /// It deliberately bypasses the D02 review gate in `validate`, which no
    /// identifier passes today (see `super::policy_review`). Tests that
    /// assert on the gate itself live there; these assert on what the
    /// mechanism does once an entry exists.
    fn reviewed(id: &str, tolerance_fraction: f64) -> RelaxableLimit {
        RelaxableLimit {
            id: id.to_owned(),
            tolerance_fraction,
            provenance: "test fixture: not an engineering-reviewed tolerance".to_owned(),
        }
    }

    #[test]
    fn the_shipped_policy_is_strict_and_lists_nothing() {
        let policy = ConstraintRelaxation::default();
        assert!(!policy.enabled);
        assert_eq!(policy.allowed_violated_groups, 0);
        assert!(policy.eligible.is_empty());
        assert!(!policy.is_active());
        assert_eq!(policy.tolerance_for("wing_area"), None);
        policy.validate().expect("the shipped policy is valid");
    }

    #[test]
    fn a_switched_on_policy_with_nothing_eligible_is_still_strict() {
        let policy = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 2,
            eligible: Vec::new(),
        };
        assert!(!policy.is_active());
        assert_eq!(policy.tolerance_for("wing_area"), None);
    }

    #[test]
    fn a_zero_group_allowance_admits_nothing_even_with_an_eligible_list() {
        let policy = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 0,
            eligible: vec![reviewed("wing_area", 0.02)],
        };
        assert!(!policy.is_active());
        assert_eq!(policy.tolerance_for("wing_area"), None);
    }

    #[test]
    fn an_active_policy_admits_only_the_limits_it_lists() {
        let policy = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![reviewed("wing_area", 0.02)],
        };
        assert_eq!(policy.tolerance_for("wing_area"), Some(0.02));
        assert_eq!(policy.tolerance_for("static_margin_floor"), None);
    }

    #[test]
    fn a_failed_evaluation_can_never_be_relaxed_by_configuration() {
        for id in NON_RELAXABLE_RESIDUAL_IDS {
            let policy = ConstraintRelaxation {
                enabled: true,
                allowed_violated_groups: 4,
                eligible: vec![reviewed(id, 0.05)],
            };
            assert_eq!(
                policy.tolerance_for(id),
                None,
                "{id} must stay hard whatever the document says"
            );
            assert!(policy.validate().is_err(), "{id} must be rejected");
        }
    }

    #[test]
    fn an_unbounded_or_unattributed_tolerance_is_rejected() {
        let over_bound = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![reviewed("wing_area", MAXIMUM_TOLERANCE_FRACTION + 1.0e-9)],
        };
        assert!(over_bound.validate().is_err());

        let non_positive = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![reviewed("wing_area", 0.0)],
        };
        assert!(non_positive.validate().is_err());

        let unattributed = ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![RelaxableLimit {
                id: "wing_area".to_owned(),
                tolerance_fraction: 0.02,
                provenance: "   ".to_owned(),
            }],
        };
        assert!(unattributed.validate().is_err());
    }
}
