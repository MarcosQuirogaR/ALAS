// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The D02 determination table itself.
//!
//! Split out of [`super`] so the review can grow an entry at a time without
//! the reasoning around it competing for the same file. The shared rationale
//! constants sit here beside the entries that use them; the enum, the
//! lookups and the configuration gate stay in the parent module.

use super::{RelaxationReview, ReviewedLimit};

/// An evaluation that did not produce the quantity its identifier names.
const NO_EVALUATION: &str =
    "The evaluation did not complete, so there is no aircraft state behind \
this candidate for a fractional miss to describe.";

/// A boolean availability flag rather than a measured miss.
const BOOLEAN_FLAG: &str = "A boolean availability flag whose violation is exactly one: the input \
or the evidence is absent. A fraction of an absent quantity has no meaning.";

/// A `PlausibilityLimits` window, which bounds the model rather than the
/// aircraft.
const VALIDITY_DOMAIN: &str = "A plausibility validity-domain bound: it states where this \
program's own mass, drag and stability correlations stop being fitted, not a requirement the \
aircraft misses. Admitting a miss accepts a result computed outside that domain, which is the \
opposite of what a tolerance expresses. The windows are already set wider than every registered \
aircraft, so the margin a tolerance would add has been taken once already.";

/// A `mdo::residuals_layout` conventional-practice band: incidence,
/// dihedral or the wing's longitudinal apex station on the fuselage, sourced
/// to a conceptual-design reference's qualitative rationale and anchored to
/// this program's own registered fleet with margin.
pub(super) const WING_LAYOUT_VALIDITY_DOMAIN: &str =
    "A wing-to-fuselage layout window, in the same sense \
as `PlausibilityLimits`: it states the span of conventional transport practice this program's \
own registered aircraft sit inside, not a requirement measured against an external standard. \
Every band already carries an explicit margin over the registered fleet, so the margin a \
tolerance would add has been taken once already.";

/// A `mdo::residuals_layout` physical containment relation: a structural or
/// geometric component that must fit inside, or connect to, the body that
/// carries it.
pub(super) const WING_FUSELAGE_CONTAINMENT: &str = "A physical containment relation, not a fitted \
correlation boundary: the wing root cannot begin or end outside the fuselage it is mounted on, \
and the primary structure box it carries cannot be deeper than that fuselage. There is no \
fraction of 'the wing is disconnected from its own body' that remains an aeroplane, so no \
primary source states one.";

/// A declared maximum weight.
const ESTABLISHED_WEIGHT: &str = "A declared maximum weight is established under 14 CFR/CS 25.25 \
rather than estimated, and no primary source states a fraction of an established maximum weight \
that may be exceeded. The validation ledger records no measured mass-model error band either \
(subsystem mass breakdown NotAvailable, 7/7).";

/// A capacity or dispatch identity rather than a requirement.
const CAPACITY_IDENTITY: &str = "A capacity identity, not a requirement with an engineering \
margin: the planned fuel does not fit, or the aircraft cannot be dispatched with it. Admitting \
the miss would publish a mission this aircraft cannot fly.";

/// The requested brief, which relaxation does not get to rewrite.
const THE_BRIEF: &str = "The value is the mission brief the user asked for. An aircraft that \
carries less or flies less far is a different requirement, not the same requirement within a \
tolerance.";

/// Every residual identifier the optimizer can emit that is not part of
/// `mdo::residuals_layout`'s wing-to-fuselage family
/// ([`super::limits_layout::LAYOUT_LIMITS`]), with its D02 determination.
///
/// Split from that family into its own file so this table's own growth does
/// not compete with the layout family's for the same budgeted file
/// (`docs/source-size-budgets.tsv`'s ratchet); [`super::reviewed_limits`]
/// is the one list a caller reads. Ordered by family, then by identifier, so
/// a reader can find a limit and a diff shows a review change rather than a
/// reordering.
pub(super) const CORE_LIMITS: &[ReviewedLimit] = &[
    // --- Evaluation: no aircraft, or no measured quantity, behind the id ---
    ReviewedLimit {
        id: "design_space",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: "The design vector left its own declared space. A point outside the search's \
domain is not a design with a small violation.",
    },
    ReviewedLimit {
        id: "dispatch_model_failed",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: NO_EVALUATION,
    },
    ReviewedLimit {
        id: "dispatch_not_converged",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: NO_EVALUATION,
    },
    ReviewedLimit {
        id: "geometry_build",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: NO_EVALUATION,
    },
    ReviewedLimit {
        id: "sizing_not_closed",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: "The coupled mass and mission closure did not converge, so the reported masses \
are an iterate rather than an aircraft.",
    },
    ReviewedLimit {
        id: "trim_solve",
        family: "Evaluation",
        review: RelaxationReview::NeverRelaxable,
        rationale: "The trim solution did not converge, so no force and moment state exists to \
measure a miss against.",
    },
    // --- Mass ---
    ReviewedLimit {
        id: "dispatch_mtow_limited",
        family: "Mass",
        review: RelaxationReview::Ineligible,
        rationale: CAPACITY_IDENTITY,
    },
    ReviewedLimit {
        id: "dispatch_tank_limited",
        family: "Mass",
        review: RelaxationReview::Ineligible,
        rationale: CAPACITY_IDENTITY,
    },
    ReviewedLimit {
        id: "fuel_capacity",
        family: "Mass",
        review: RelaxationReview::Ineligible,
        rationale: CAPACITY_IDENTITY,
    },
    ReviewedLimit {
        id: "fuel_capacity_unavailable",
        family: "Mass",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "landing_mass",
        family: "Mass",
        review: RelaxationReview::Ineligible,
        rationale: ESTABLISHED_WEIGHT,
    },
    ReviewedLimit {
        id: "mtow_ceiling",
        family: "Mass",
        review: RelaxationReview::Ineligible,
        rationale: ESTABLISHED_WEIGHT,
    },
    ReviewedLimit {
        id: "structural_inventory_unverified",
        family: "Mass",
        review: RelaxationReview::NeverRelaxable,
        rationale: "The strength-sized wingbox exceeds the whole modelled wing, so the mass \
statement is incomplete rather than outside a limit; admitting it would publish a partial \
aircraft as a complete one.",
    },
    // --- Balance ---
    ReviewedLimit {
        id: "cg_model_error",
        family: "Balance",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "forward_cg_range",
        family: "Balance",
        review: RelaxationReview::Ineligible,
        rationale: "The centre-of-gravity envelope is NotAvailable 7/7 against any published \
weight-and-balance envelope, so nothing external bounds this model's error and no tolerance can \
be sized from it.",
    },
    ReviewedLimit {
        id: "main_gear_strength",
        family: "Balance",
        review: RelaxationReview::Ineligible,
        rationale: "A gear reaction ceiling is a component strength limit; exceeding it is a \
structural overload, and no primary source states a fraction of a gear rating that may be \
exceeded.",
    },
    ReviewedLimit {
        id: "min_nose_gear_load",
        family: "Balance",
        review: RelaxationReview::Ineligible,
        rationale:
            "The minimum nose-gear load separates a steerable aeroplane from a tail-sitter. \
The validation ledger already records that the present bound admits a physically inadmissible \
negative reaction, so widening it is the wrong direction.",
    },
    ReviewedLimit {
        id: "nose_gear_strength",
        family: "Balance",
        review: RelaxationReview::Ineligible,
        rationale: "A gear reaction ceiling is a component strength limit; exceeding it is a \
structural overload, and no primary source states a fraction of a gear rating that may be \
exceeded.",
    },
    ReviewedLimit {
        id: "static_margin_floor",
        family: "Balance",
        review: RelaxationReview::Ineligible,
        rationale: "No published static margin or neutral-point station exists for any registered \
aircraft (validation ledger, 7/7 NotAvailable), so there is no measured error band; and a \
stability floor missed by a fraction is a different handling-qualities aircraft rather than the \
same one inside a tolerance.",
    },
    // --- Performance ---
    ReviewedLimit {
        id: "airport_declared_distance_unavailable",
        family: "Performance",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "airport_unknown",
        family: "Performance",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "approach_speed",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "The limit is the approach speed the run declares for its arrival aerodrome, \
and the approach-category boundaries behind such a declaration are discrete. No matched \
approach-speed comparison exists in the validation ledger, so no model-error band is available \
either.",
    },
    ReviewedLimit {
        id: "cruise_thrust",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "An installed-thrust shortfall means the aircraft does not hold its cruise \
condition. The propulsion deck is itself unvalidated - the ledger records PSFC 21-28 % above the \
measured record and a -4.96 % level-flight shortfall at FL170 - so the model's own error exceeds \
any tolerance that could be written, and an unbounded error cannot justify a bounded one.",
    },
    ReviewedLimit {
        id: "landing_field",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "The limit comes from the aerodrome's declared landing distance, which is \
surveyed data rather than an estimate. ALAS additionally reports no matched landing field length \
anywhere in the validation ledger, so no model-error band exists to size a tolerance from.",
    },
    ReviewedLimit {
        id: "mission_distance_unavailable",
        family: "Performance",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "mission_profile_range",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: THE_BRIEF,
    },
    ReviewedLimit {
        id: "oei_part25_engine_count_unsupported",
        family: "Performance",
        review: RelaxationReview::NeverRelaxable,
        rationale: "14 CFR 25.121(b) tabulates gradients for two-, three- and four-engine \
aeroplanes only. An unsupported engine count has no Part 25 requirement to miss, which is why \
this residual is reported rather than a fallback gradient applied.",
    },
    ReviewedLimit {
        id: "oei_second_segment",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "14 CFR 25.121(b) prescribes a minimum steady gradient of climb - 2.4 % for \
two engines, 2.7 % for three, 3.0 % for four (14 CFR Ch. I, 1-1-25). An airworthiness minimum is \
pass or fail and states no tolerance.",
    },
    ReviewedLimit {
        id: "oei_second_segment_evidence_gap",
        family: "Performance",
        review: RelaxationReview::NeverRelaxable,
        rationale: BOOLEAN_FLAG,
    },
    ReviewedLimit {
        id: "takeoff_field",
        family: "Performance",
        review: RelaxationReview::Ineligible,
        rationale: "The limit comes from the aerodrome's declared take-off distance, which is \
surveyed data rather than an estimate. ALAS additionally reports no matched take-off field length \
anywhere in the validation ledger, so no model-error band exists to size a tolerance from.",
    },
    // --- Geometry ---
    ReviewedLimit {
        id: "aspect_ratio_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "aspect_ratio_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "cargo_target_excess",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: THE_BRIEF,
    },
    ReviewedLimit {
        id: "cargo_target_shortfall",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: THE_BRIEF,
    },
    ReviewedLimit {
        id: "fuselage_fineness_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "fuselage_fineness_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "geometric_body_alpha",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "A configured cruise body-attitude window. A design outside it is outside the \
attitude the run itself declared, and no primary source states a fraction of a declared window \
that may be exceeded.",
    },
    ReviewedLimit {
        id: "passenger_shortfall",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale:
            "Seats are integers and the count is the brief: a fractional shortfall does not \
exist, and a smaller cabin is a different aircraft.",
    },
    ReviewedLimit {
        id: "planform_break_ordering",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "Ordinal: the trailing-edge break chord either lies between the tip and root \
chords or it does not. There is no fraction of an ordering.",
    },
    ReviewedLimit {
        id: "plausibility_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "Fallback identifier for a windowed plausibility quantity with no dedicated \
identifier pair. It cannot be reviewed as a named limit, and the validity-domain reasoning \
applies to it as to every other window.",
    },
    ReviewedLimit {
        id: "plausibility_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "Fallback identifier for a windowed plausibility quantity with no dedicated \
identifier pair. It cannot be reviewed as a named limit, and the validity-domain reasoning \
applies to it as to every other window.",
    },
    ReviewedLimit {
        id: "root_thickness_ratio_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "root_thickness_ratio_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "span",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale:
            "The span limit is the intended ICAO Annex 14 aerodrome reference code letter - \
36 m for C, 52 m for D, 65 m for E, 80 m for F. The code letters are discrete: a span over the \
limit moves the aircraft into the next code and changes which aerodromes accept it, which is not \
a bounded miss of the same requirement.",
    },
    ReviewedLimit {
        id: "tail_arm_fraction_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "tail_arm_fraction_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "tail_volume_h",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "A statistical plausibility band, already ranked soft under a hard geometry \
family. A tolerance on a band that is itself the spread of historical practice adds nothing a \
reviewer could check.",
    },
    ReviewedLimit {
        id: "tail_volume_v",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "A statistical plausibility band, already ranked soft under a hard geometry \
family. A tolerance on a band that is itself the spread of historical practice adds nothing a \
reviewer could check.",
    },
    ReviewedLimit {
        id: "tip_root_chord_ratio_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "tip_root_chord_ratio_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "tip_washout_max",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "tip_washout_min",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: VALIDITY_DOMAIN,
    },
    ReviewedLimit {
        id: "wing_area",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "A declared design limit with no external standard behind it. Nothing states a \
fraction of it that may be exceeded, and the reference area feeds every aerodynamic and mass \
coefficient in the run.",
    },
    ReviewedLimit {
        id: "wing_loading",
        family: "Geometry",
        review: RelaxationReview::Ineligible,
        rationale: "A declared minimum design wing loading with no external standard behind it. \
Nothing states a fraction of it that may be missed.",
    },
];
