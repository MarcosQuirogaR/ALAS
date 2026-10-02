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
        id: "structural_ultimate_tip_deflection_ratio",
        family: "Structure",
        review: RelaxationReview::Ineligible,
        rationale: "The ultimate-load tip deflection over the semispan, published as a diagnostic with no rejection or penalty. CS 25.305 states no universal ultimate tip-displacement limit and zero is only the undeformed reference, so it is not a limit and grants no relaxation.",
    },
    ReviewedLimit {
        id: "structural_ultimate_curvature",
        family: "Structure",
        review: RelaxationReview::Ineligible,
        rationale: "The ultimate-load linear-curvature error, published against the 1 g model-validity budget as a diagnostic with no rejection or penalty. The 1 g domain stays hard as structural_linear_model_domain; the ultimate value flags load redistribution needing separate substantiation, so it is not a limit and grants no relaxation.",
    },
    ReviewedLimit {
        id: "root_to_kink_te_angle",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "The exposed trailing edge between side of body and kink may not run forward past 90 deg from the fuselage axis: design practice keeps the flap hinge line and the rear spar from running forward (Torenbeek, Synthesis of Subsonic Airplane Design, 1982; Obert, Aerodynamic Design of Transport Aircraft, 2009). A design-practice limit, not a bound of the models, and not relaxable.",
    },
    ReviewedLimit {
        id: "root_to_kink_te_angle_unavailable",
        family: "Geometry",
        review: RelaxationReview::NeverRelaxable,
        rationale: "The exposed trailing-edge angle could not be evaluated from the declared wing geometry. This boolean availability flag supplies no measured angle to compare with the limit; a fraction of a missing geometric assessment has no physical meaning.",
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
