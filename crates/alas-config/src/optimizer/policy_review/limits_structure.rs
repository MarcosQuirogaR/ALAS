// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural safety and numerical-domain constraints cannot be relaxed.

use super::{RelaxationReview, ReviewedLimit};

const fn structural(id: &'static str) -> ReviewedLimit {
    ReviewedLimit {
        id,
        family: "Structure",
        review: RelaxationReview::NeverRelaxable,
        rationale: "Structural strength, supported response-model validity and complete finite structural inputs are required independently of objective value. A preferred design or finite objective is not evidence permitting a structural limit to be exceeded.",
    }
}

const fn preference(id: &'static str) -> ReviewedLimit {
    ReviewedLimit {
        id, family: "Geometry", review: RelaxationReview::NeverRelaxable,
        rationale: "An opt-in transport shape preference scored softly. It is not a certification or physical limit and cannot authorize relaxation of another constraint.",
    }
}

pub(super) const STRUCTURE_LIMITS: &[ReviewedLimit] = &[
    structural("structural_input_invalid"),
    structural("structural_geometry"),
    structural("structural_material"),
    structural("structural_response_invalid"),
    structural("structural_strength"),
    structural("structural_rib_spacing"),
    structural("structural_linear_model_domain"),
    structural("structural_relief_not_converged"),
    structural("structural_mesh_invalid"),
    structural("structural_stiffness_not_converged"),
    structural("structural_wing_mass_unavailable"),
    structural("structural_cap_packaging"),
    ReviewedLimit {
        id: "structural_primary_mass_discrepancy",
        family: "Structure",
        review: RelaxationReview::Ineligible,
        rationale: "A signed difference between the FLOPS and explicit primary-structure mass inventories, reported for inspection with no rejection or penalty. Its models have different scopes, so it is not a limit and grants no relaxation.",
    },
    ReviewedLimit {
        id: "structural_mesh_mass_discrepancy",
        family: "Structure",
        review: RelaxationReview::Ineligible,
        rationale: "A signed difference between the FLOPS and finite-element mesh primary-structure masses, reported for inspection with no rejection or penalty. Its models have different scopes, so it is not a limit and grants no relaxation.",
    },
    ReviewedLimit {
        id: "root_to_kink_te_angle",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "The inboard trailing edge between root and kink may not sweep forward past 90 deg from the fuselage axis; beyond it the flap and wing-box layout the mass and structural models assume no longer exists.",
    },
    preference("transport_root_wingbox_depth"),
    preference("transport_kink_wingbox_depth"),
    preference("transport_kink_wingbox_width"),
    preference("transport_flap_area_fraction"),
    preference("transport_root_box_slenderness"),
    preference("transport_inboard_te_sweep"),
    preference("transport_break_root_chord_ratio"),
    preference("transport_tip_root_chord_ratio"),
    ReviewedLimit {
        id: "transport_planform_invalid", family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: "A requested planform assessment needs finite geometry and coherent declared preference limits. An unavailable assessment cannot be ranked as a satisfied preference.",
    },
    ReviewedLimit {
        id: "candidate_state_unavailable",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: "A nonfinite objective or constraint is an unavailable physical evaluation. NaN cannot establish feasibility or be replaced by a favorable zero violation.",
    },
];
