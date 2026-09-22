// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `mdo::residuals_layout`'s wing-to-fuselage layout family's D02
//! determinations.
//!
//! Split out of [`super::limits`] so that table's own growth does not
//! compete with this family's for the same budgeted file
//! (`docs/source-size-budgets.tsv`'s ratchet). [`super::reviewed_limits`]
//! is the one list a caller reads; this module's only export is consumed
//! there.

use super::limits::{WING_FUSELAGE_CONTAINMENT, WING_LAYOUT_VALIDITY_DOMAIN};
use super::{RelaxationReview, ReviewedLimit};

/// Every layout-family residual identifier, with its D02 determination.
///
/// Ordered by identifier, matching [`super::limits::CORE_LIMITS`]'s own
/// convention.
pub(super) const LAYOUT_LIMITS: &[ReviewedLimit] = &[
    ReviewedLimit {
        id: "sweep_consistent_with_cruise_mach",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "The limit is the Korn-equation transonic wave-drag estimate at this \
candidate's own declared cruise Mach, sweep, root thickness and lift coefficient, computed with \
the same `config.drag_model` coefficients the trim/drag solve itself uses. Admitting a miss would \
publish a candidate whose own aerodynamic model already disagrees with the cruise point it is \
being sized at; no primary source states a fraction of that self-consistency that may be exceeded.",
    },
    ReviewedLimit {
        id: "wing_apex_fraction_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_apex_fraction_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_dihedral_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_dihedral_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_root_incidence_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_root_incidence_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_LAYOUT_VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_root_le_on_fuselage",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_FUSELAGE_CONTAINMENT,
    },
    ReviewedLimit {
        id: "wing_root_te_on_fuselage",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_FUSELAGE_CONTAINMENT,
    },
    ReviewedLimit {
        id: "wing_root_within_fuselage_envelope",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_FUSELAGE_CONTAINMENT,
    },
    ReviewedLimit {
        id: "wingbox_root_depth_fits_fuselage",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: WING_FUSELAGE_CONTAINMENT,
    },
];
