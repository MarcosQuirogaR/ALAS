// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The transonic cruise-envelope family's determinations.

use super::{RelaxationReview, ReviewedLimit};

/// Every cruise-envelope residual identifier, with its determination,
/// ordered by identifier.
pub(super) const AERO_LIMITS: &[ReviewedLimit] = &[
    ReviewedLimit {
        id: "buffet_margin",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "The limit is the 1.3 g load factor to buffet onset at the cruise point, the \
operating margin applied to the buffet-onset boundaries CS 25.251(e) requires to be determined; in a \
reference adaptation it is the lower of that and what the registered wing reaches under the same estimate. \
The estimate is the Korn drag-divergence boundary on the sweep, thickness and technology factor the \
wave-drag build-up uses. No primary source states a fraction of the margin that may be given up, \
and the program has no measured error band on its buffet-onset estimate.",
    },
    ReviewedLimit {
        id: "buffet_margin_absolute",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "A report-only reading of the same load factor against the absolute 1.3 g \
margin, kept visible where the reference-adaptation floor is lower. It is never ranked, so there \
is nothing to relax.",
    },
];
