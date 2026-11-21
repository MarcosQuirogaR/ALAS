// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The hard per-state model constraints: a real
//! `mod`, not an `include!`, so this does not count against
//! `envelope.rs`'s frozen `docs/source-size-budgets.tsv` ceiling.

use super::{ModelCgLoadingAssessment, ModelCgLoadingState, PhaseLimits, PhysicalCgLimits};

/// One independently evaluated hard model constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCgConstraint {
    /// Positive stability buffer from the critical (most-forward) neutral
    /// point, as a fraction of MAC.
    StaticStabilityFloor,
    /// The physical forward CG limit: the most aft of the boundaries this
    /// state's phase admits (rotation at takeoff, landing trim in flight and
    /// at landing, and the maximum-nose-load-handling boundary always). It
    /// does not depend on the aft limit, so it cannot collapse when the
    /// critical neutral point moves forward of the ground boundary.
    PhysicalForwardCgLimit,
    /// Rated nose-gear tire capacity (only meaningful when
    /// [`alas_perf::landing_gear::LandingGearLayout::capacity_basis_declared`]).
    NoseGearStrength,
    /// Rated main-gear tire capacity (only meaningful when
    /// [`alas_perf::landing_gear::LandingGearLayout::capacity_basis_declared`]).
    MainGearStrength,
    /// Nose-load fraction required for ground steering authority.
    MinimumNoseGearLoad,
    /// Nose-load fraction must not exceed the handling limit.
    MaximumNoseGearLoadFraction,
    /// Longitudinal tip-back angle at this state's loaded CG height must
    /// clear `max(min_tip_back_deg, scrape_angle_deg)`.
    TipBack,
    /// The fuselage lower-contour scrape angle at the main gear must clear
    /// the required takeoff rotation angle, independent of loading
    /// state.
    TailScrape,
    /// The physical aft and forward boundaries at this state must admit at
    /// least the configured CG range: a finding, not a limit -- the
    /// configured range does not set the forward boundary itself.
    MinimumUsableCgRange,
}

impl ModelCgConstraint {
    /// Whether this kind of check is reported but never rejects a design.
    ///
    /// `MinimumUsableCgRange` checks a configured assumption, not a physical
    /// limit. `TailScrape` rejects by kind: a built fuselage that touches the
    /// runway below a published tail-strike attitude, compared in the gear
    /// state it was published for, cannot fly that rotation. One assessment
    /// can still be diagnostic when its requirement is an assumption or its
    /// gear state is not published
    /// (`LandingGearConfig::required_rotation_basis`); see
    /// [`ModelCgConstraintAssessment::is_diagnostic`].
    pub const fn is_diagnostic(self) -> bool {
        matches!(self, Self::MinimumUsableCgRange)
    }

    /// Stable report label for this constraint.
    pub const fn label(self) -> &'static str {
        match self {
            Self::StaticStabilityFloor => "static-stability floor",
            Self::PhysicalForwardCgLimit => "physical forward CG limit",
            Self::NoseGearStrength => "nose-gear strength",
            Self::MainGearStrength => "main-gear strength",
            Self::MinimumNoseGearLoad => "minimum nose-gear load",
            Self::MaximumNoseGearLoadFraction => "maximum nose-gear load fraction",
            Self::TipBack => "tip-back angle",
            Self::TailScrape => "tail-scrape clearance",
            Self::MinimumUsableCgRange => "minimum usable CG range",
        }
    }

    /// Unit shared by the measured value and limit.
    pub const fn unit(self) -> &'static str {
        match self {
            Self::StaticStabilityFloor => "fraction MAC",
            Self::PhysicalForwardCgLimit | Self::MinimumUsableCgRange => "% MAC",
            Self::NoseGearStrength | Self::MainGearStrength => "kg",
            Self::MinimumNoseGearLoad | Self::MaximumNoseGearLoadFraction => "fraction weight",
            Self::TipBack | Self::TailScrape => "deg",
        }
    }
}

/// Result of applying one hard constraint to one loading state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelCgConstraintAssessment {
    /// Physical or configured constraint being evaluated.
    pub constraint: ModelCgConstraint,
    /// Value produced by the analyzed loading state.
    pub actual: f64,
    /// Governing hard limit.
    pub limit: f64,
    /// Whether the value lies on the infeasible side of the limit.
    pub violated: bool,
    /// Dimensionless violation used only to rank the worst constraint.
    pub normalized_exceedance: f64,
    /// Whether this assessment is reported but never rejects a design.
    pub diagnostic: bool,
}

impl ModelCgConstraintAssessment {
    /// Whether this assessment is reported but never rejects a design: its
    /// kind is diagnostic, or (tail scrape) its requirement is not a
    /// published value compared in its own gear state.
    pub const fn is_diagnostic(&self) -> bool {
        self.diagnostic
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LoadingConstraintInputs {
    pub(super) state: ModelCgLoadingState,
    /// The mechanisms `physical_limits` is scoped to: the state's phase at
    /// its mass ([`PhaseLimits::for_state_at_mass`]).
    pub(super) scope: PhaseLimits,
    pub(super) mass_kg: f64,
    pub(super) cg_x_m: f64,
    pub(super) h_cg_m: f64,
    pub(super) cg_pct_mac: f64,
    pub(super) static_margin: f64,
    /// Static margin measured against the critical (most-forward) neutral
    /// point rather than [`Self::static_margin`]'s clean one: the
    /// quantity [`ModelCgConstraint::StaticStabilityFloor`] actually gates,
    /// so it agrees with `ModelCgEnvelopeAssessment::aerodynamic_aft_limit_pct_mac`.
    pub(super) critical_static_margin: f64,
    pub(super) static_margin_floor: f64,
    pub(super) physical_limits: PhysicalCgLimits,
    pub(super) nose_gear_load_kg: f64,
    pub(super) nose_gear_capacity_kg: f64,
    pub(super) main_gear_load_kg: f64,
    pub(super) main_gear_capacity_kg: f64,
    pub(super) capacity_basis_declared: bool,
    pub(super) minimum_nose_gear_load_fraction: f64,
    pub(super) maximum_nose_gear_load_fraction: f64,
    pub(super) tip_back_angle_deg: f64,
    pub(super) required_tip_back_deg: f64,
    pub(super) scrape_angle_deg: Option<f64>,
    pub(super) required_rotation_angle_deg: f64,
    /// Whether a tail-scrape miss rejects the design
    /// (`RotationAngleBasis::rejects`).
    pub(super) tail_scrape_rejects: bool,
    pub(super) cg_range_pct_mac: f64,
}

fn lower_bound_constraint(
    constraint: ModelCgConstraint,
    actual: f64,
    limit: f64,
    normalization: f64,
) -> ModelCgConstraintAssessment {
    let deficit = (limit - actual).max(0.0);
    ModelCgConstraintAssessment {
        constraint,
        actual,
        limit,
        violated: actual < limit,
        normalized_exceedance: deficit / normalization.max(f64::EPSILON),
        diagnostic: constraint.is_diagnostic(),
    }
}

fn upper_bound_constraint(
    constraint: ModelCgConstraint,
    actual: f64,
    limit: f64,
    normalization: f64,
) -> ModelCgConstraintAssessment {
    let excess = (actual - limit).max(0.0);
    ModelCgConstraintAssessment {
        constraint,
        actual,
        limit,
        violated: actual > limit,
        normalized_exceedance: excess / normalization.max(f64::EPSILON),
        diagnostic: constraint.is_diagnostic(),
    }
}

pub(super) fn assess_loading_constraints(
    inputs: LoadingConstraintInputs,
) -> ModelCgLoadingAssessment {
    let nose_gear_load_fraction = inputs.nose_gear_load_kg / inputs.mass_kg;
    // Both groups in compression is the stated domain of the split that
    // produced these two reactions; see
    // `ModelCgLoadingAssessment::ground_reactions_admissible`. The test is on
    // the reactions themselves, with no margin and no tolerance: zero nose
    // load is the tipping point itself and is still a reaction the model can
    // state.
    let ground_reactions_admissible =
        inputs.nose_gear_load_kg >= 0.0 && inputs.main_gear_load_kg >= 0.0;
    let mut constraints = vec![
        lower_bound_constraint(
            ModelCgConstraint::StaticStabilityFloor,
            inputs.critical_static_margin,
            inputs.static_margin_floor,
            1.0,
        ),
        lower_bound_constraint(
            ModelCgConstraint::PhysicalForwardCgLimit,
            inputs.cg_pct_mac,
            inputs.physical_limits.fwd_limit_pct_mac,
            100.0,
        ),
        lower_bound_constraint(
            ModelCgConstraint::MinimumNoseGearLoad,
            nose_gear_load_fraction,
            inputs.minimum_nose_gear_load_fraction,
            1.0,
        ),
        upper_bound_constraint(
            ModelCgConstraint::MaximumNoseGearLoadFraction,
            nose_gear_load_fraction,
            inputs.maximum_nose_gear_load_fraction,
            1.0,
        ),
        lower_bound_constraint(
            ModelCgConstraint::TipBack,
            inputs.tip_back_angle_deg,
            inputs.required_tip_back_deg,
            1.0,
        ),
        lower_bound_constraint(
            ModelCgConstraint::MinimumUsableCgRange,
            inputs.physical_limits.usable_range_pct_mac,
            inputs.cg_range_pct_mac,
            100.0,
        ),
    ];
    if inputs.capacity_basis_declared {
        // Capacity is only a meaningful, non-tautological check on a
        // declared (forced) wheel count; an auto-sized bogie was fit to
        // exactly this envelope and the comparison cannot bind.
        constraints.push(upper_bound_constraint(
            ModelCgConstraint::NoseGearStrength,
            inputs.nose_gear_load_kg,
            inputs.nose_gear_capacity_kg,
            inputs.mass_kg,
        ));
        constraints.push(upper_bound_constraint(
            ModelCgConstraint::MainGearStrength,
            inputs.main_gear_load_kg,
            inputs.main_gear_capacity_kg,
            inputs.mass_kg,
        ));
    }
    if let Some(scrape_angle_deg) = inputs.scrape_angle_deg {
        // State-independent (pure rotation-clearance geometry), evaluated
        // identically at every state for a uniform per-state constraint list.
        let mut scrape = lower_bound_constraint(
            ModelCgConstraint::TailScrape,
            scrape_angle_deg,
            inputs.required_rotation_angle_deg,
            1.0,
        );
        scrape.diagnostic = !inputs.tail_scrape_rejects;
        constraints.push(scrape);
    }
    // Each state is gated only by the mechanisms of its own phase: a
    // ground-only state keeps no flight CG-range or static-margin verdict.
    // `physical_limits` is expected to be scoped to the same phase
    // (`PhysicalCgLimits::scoped`), so the forward limit and usable range
    // gated here are this state's own.
    let phase = PhaseLimits::for_state(inputs.state);
    constraints.retain(|constraint| phase.admits(constraint.constraint));
    if !ground_reactions_admissible {
        // Drop only the two rated-capacity comparisons. This cannot admit a
        // candidate that would otherwise be rejected: both are upper bounds
        // that a negative reaction already satisfies, so what is removed is a
        // passing verdict on a number the model did not measure, and
        // `MinimumNoseGearLoad` keeps rejecting the state.
        constraints.retain(|constraint| {
            !matches!(
                constraint.constraint,
                ModelCgConstraint::NoseGearStrength | ModelCgConstraint::MainGearStrength
            )
        });
    }
    ModelCgLoadingAssessment {
        state: inputs.state,
        scope: inputs.scope,
        mass_kg: inputs.mass_kg,
        cg_x_m: inputs.cg_x_m,
        h_cg_m: inputs.h_cg_m,
        cg_pct_mac: inputs.cg_pct_mac,
        static_margin: inputs.static_margin,
        critical_static_margin: inputs.critical_static_margin,
        nose_gear_load_fraction,
        ground_reactions_admissible,
        physical_limits: inputs.physical_limits,
        constraints,
    }
}
