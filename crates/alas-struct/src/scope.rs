// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a sized wingbox was sized *to*, and what it was not sized to.
//!
//! [`crate::sizing`] reports a box: gauges, areas, margins and a mass. None of
//! those say which loading envelope produced them, and on this product the
//! answer differs per aircraft - one box is bounded by a certified zero-fuel
//! limit, another rests on an unbounded full-tank assumption, and no box on any
//! aircraft has seen a gust case. A reader handed only the mass cannot tell
//! those apart, and the difference is larger than most of the numbers in the
//! result.
//!
//! This module is the vocabulary for saying it. Every gap is a typed
//! [`NotAvailable`] carrying what is missing, why it is missing here, what
//! supplying it would take, and - the part that decides whether a gap is
//! tolerable - which way the omission moves the sized box. Nothing in this
//! module estimates, substitutes or defaults a missing load: an absent
//! quantity stays absent and becomes visible instead.
//!
//! [`SizingScope`] is the aggregate a consumer publishes alongside the box.

mod wing_fuel;
mod wing_mounted;

pub use wing_fuel::WingFuelDesignCase;
pub use wing_mounted::{wing_mounted_relief, WingMountedRelief};

/// Which way an omitted quantity moves the sized structure.
///
/// This is the only thing that makes an omission reportable rather than a
/// defect: a conservative omission leaves the box heavier than the aircraft
/// needs, which is safe and wasteful; an unconservative one leaves it lighter
/// than the aircraft needs, which is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmissionDirection {
    /// The omission can only make the sized box heavier. A relieving mass that
    /// is left out, or a load that is applied at a station further outboard
    /// than the real one, is of this kind.
    Heavier,
    /// The omission can only make the sized box lighter - a load case that is
    /// not evaluated, or relief credited that the envelope does not guarantee.
    Lighter,
    /// The sign is not established. It must not be assumed to be [`Self::Heavier`].
    Unknown,
}

/// A quantity a structural design case needs that this model does not have.
///
/// Held as static text rather than an error: this is not a failure of a solve,
/// it is the scope of the solve, and it is the same for every aircraft that
/// takes the same path through the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAvailable {
    /// What is missing, named in the units it would be supplied in.
    pub quantity: &'static str,
    /// Why it is absent here, rather than what its value would be.
    pub reason: &'static str,
    /// What resolving it would take - the input, the source or the call site.
    pub resolved_by: &'static str,
    /// Which way leaving it out moves the sized box.
    pub direction: OmissionDirection,
}

/// Representation of the gust and continuous-turbulence design cases.
///
/// [`crate::loads::load_cases`] is the V-n manoeuvre set only: symmetric
/// pull-up and push-down at the ultimate load factor, and 1 g level. The
/// certification gust cases are a separate envelope, and on a real transport
/// they routinely size the outer wing where the manoeuvre case does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GustEnvelope {
    /// No gust case is evaluated. The box is a manoeuvre-sized box, and must
    /// not be published as sized to the certification load envelope.
    NotModelled(NotAvailable),
}

impl GustEnvelope {
    /// The declaration every solve in this crate currently carries.
    pub const fn not_modelled() -> Self {
        Self::NotModelled(NotAvailable {
            quantity: "discrete gust and continuous turbulence load cases, \
                       CS-25.341 / 14 CFR 25.341",
            reason: "the load model evaluates the V-n manoeuvre set only. A gust case needs \
                     the reference gust velocity at the design altitude, the gust gradient \
                     search, the aircraft mass ratio and the lift-curve slope at the case \
                     speed, none of which this crate is given, and none of which may be \
                     assumed into existence.",
            resolved_by: "a gust load model fed by the certified speed schedule and the \
                          aerodynamic lift-curve slope, evaluated alongside \
                          crate::loads::load_cases",
            direction: OmissionDirection::Lighter,
        })
    }

    /// The gap this envelope declares, if any.
    pub const fn not_available(self) -> Option<NotAvailable> {
        match self {
            Self::NotModelled(gap) => Some(gap),
        }
    }
}

/// The spanwise datum the box is sized and integrated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootDatum {
    /// `y = 0` at the aircraft centreline. The wing carry-through structure
    /// inside the fuselage is therefore sized and weighed at the maximum root
    /// moment, where a real box is spliced at the side of body and the
    /// carry-through is a different structure.
    AircraftCentreline(NotAvailable),
}

impl RootDatum {
    /// The datum every solve in this crate currently uses.
    pub const fn aircraft_centreline() -> Self {
        Self::AircraftCentreline(NotAvailable {
            quantity: "side-of-body station, m from the aircraft centreline",
            reason: "the sizing grid runs from y = 0, so the inboard band that is really \
                     carry-through structure inside the fuselage is charged at the maximum \
                     root moment. alas_geom::wing_structure::WingStructureGeometry carries no \
                     fuselage dimension, so the station is not reachable from this crate.",
            resolved_by: "the fuselage width at the wing station, passed in with the wing \
                          geometry",
            direction: OmissionDirection::Heavier,
        })
    }

    /// The gap this datum declares, if any.
    pub const fn not_available(self) -> Option<NotAvailable> {
        match self {
            Self::AircraftCentreline(gap) => Some(gap),
        }
    }
}

/// Where the relieving wing fuel's spanwise shape came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingFuelDistribution {
    /// Derived from the box's own enclosed section across the declared integral
    /// tank band ([`crate::tanks`]). The shape follows the candidate wing, which
    /// is what an optimised clean-sheet box needs.
    EnclosedBoxVolume,
    /// Supplied by a caller holding the aircraft's declared cell capacities,
    /// with the cells' own shape scaled to the design-case mass.
    ///
    /// The scaling is the gap: a wing that is not full does not empty its cells
    /// uniformly, and where the remaining fuel sits changes the root moment.
    DeclaredCellsScaled {
        /// The placement gap, present only when the case is a partial fill.
        /// A full-tank case has no placement freedom to declare.
        partial_fill_placement: Option<NotAvailable>,
    },
}

impl WingFuelDistribution {
    /// The declaration for a caller-supplied distribution, given whether the
    /// design case fills the declared tanks.
    pub const fn declared_cells_scaled(is_partial_fill: bool) -> Self {
        Self::DeclaredCellsScaled {
            partial_fill_placement: if is_partial_fill {
                Some(NotAvailable {
                    quantity: "spanwise placement of a partial wing-fuel load, kg/m",
                    reason: "the design case carries less fuel than the declared cells hold, \
                             and the declared cell shape is scaled uniformly to reach it. \
                             Which cells are drawn down first is an operating procedure, not \
                             a property of the wing, and no refuelling sequence or certified \
                             partial-fill limitation was retrieved for these aircraft.",
                    resolved_by: "a published refuelling or fuel-management schedule, or a \
                                  certified partial-fill limitation, for the aircraft",
                    direction: OmissionDirection::Unknown,
                })
            } else {
                None
            },
        }
    }

    /// The gap this distribution declares, if any.
    pub const fn not_available(self) -> Option<NotAvailable> {
        match self {
            Self::EnclosedBoxVolume => None,
            Self::DeclaredCellsScaled {
                partial_fill_placement,
            } => partial_fill_placement,
        }
    }
}

/// Whether the relieved-load fixed point settled inside its own tolerance.
///
/// The box relieves its own bending, so its mass sits on both sides of the
/// sizing equation and the solve iterates. Reporting a box that never settled
/// as if it had is the failure this type exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReliefConvergence {
    /// No iteration was run: the solve carries no relief at all, which is the
    /// frozen reference law.
    NotIterated,
    /// The last pass moved the total box mass by less than the tolerance.
    Settled {
        /// Passes taken, including the first.
        passes: usize,
        /// Relative change in total box mass at the last pass.
        relative_change: f64,
    },
    /// The pass budget ran out with the total still moving. The reported box is
    /// the last pass, and it is **not** a converged solution.
    NotSettled {
        /// Passes taken, the whole budget.
        passes: usize,
        /// Relative change in total box mass at the last pass.
        relative_change: f64,
        /// The tolerance it failed to reach.
        tolerance: f64,
    },
}

impl ReliefConvergence {
    /// Whether the reported box is a settled fixed point.
    pub const fn is_settled(&self) -> bool {
        matches!(self, Self::NotIterated | Self::Settled { .. })
    }
}

/// The loading envelope and model scope one sized wingbox was produced under.
///
/// SI throughout: kg, m, N, N.m.
#[derive(Debug, Clone, PartialEq)]
pub struct SizingScope {
    /// Name of the manoeuvre case that sized the box, as
    /// [`crate::sizing::WingboxSizing::sizing_load_case`] reports it.
    pub sizing_load_case: &'static str,
    /// Design gross mass the manoeuvre cases were applied at, kg - the whole
    /// aircraft, not a semi-wing.
    pub design_gross_mass_kg: f64,
    /// Signed ultimate load factor of the positive manoeuvre case, including
    /// any additional safety factor.
    pub ultimate_load_factor: f64,
    /// Gust and turbulence representation.
    pub gust_envelope: GustEnvelope,
    /// The wing fuel the sizing case was relieved by, and what bounds it.
    pub wing_fuel: WingFuelDesignCase,
    /// Where that fuel's spanwise shape came from.
    pub wing_fuel_distribution: WingFuelDistribution,
    /// Wing-carried items that were not reachable as point relief.
    pub wing_mounted_omissions: Vec<NotAvailable>,
    /// The spanwise datum the box was integrated from.
    pub root_datum: RootDatum,
    /// Whether the relieved-load fixed point settled.
    pub relief_convergence: ReliefConvergence,
    /// Model-scope gaps that belong to the solve itself rather than to one of
    /// its inputs.
    pub solve_omissions: Vec<NotAvailable>,
}

/// Point masses relieve the bending moment but not the shear.
///
/// [`crate::loads::apply_point_mass_relief`] subtracts the moment a wing-mounted
/// item relieves; the shear the webs are sized from is integrated from the net
/// distributed load only, so it still carries the item's inertia across the
/// root. Webs are a small part of the box, and leaving them unrelieved makes
/// them heavier, not lighter.
pub const POINT_MASS_SHEAR_RELIEF: NotAvailable = NotAvailable {
    quantity: "shear relief from wing-mounted point masses, N",
    reason: "crate::loads::apply_point_mass_relief acts on the bending moment only. The root \
             shear the web thickness is sized from is integrated from the net distributed load, \
             so a wing-mounted engine relieves the caps but not the webs.",
    resolved_by: "applying the same point loads to the shear integral as to the moment",
    direction: OmissionDirection::Heavier,
};

/// Wing-mounted landing gear is not given to the sizing entry points.
pub const WING_MOUNTED_GEAR_RELIEF: NotAvailable = NotAvailable {
    quantity: "wing-mounted landing-gear installation mass and station, kg at m",
    reason: "the sizing entry points are not given the gear configuration, so a wing-mounted \
             main gear relieves nothing even though it is carried by the wing.",
    resolved_by: "the resolved gear stations and masses, passed in as wing-mounted point masses",
    direction: OmissionDirection::Heavier,
};

impl SizingScope {
    /// Every declared gap this box carries, in one list.
    ///
    /// A consumer that publishes the box, a margin or a solved deck publishes
    /// this beside it; an empty list would mean the box was sized to a complete
    /// envelope, which on this product it is not.
    pub fn not_available(&self) -> Vec<NotAvailable> {
        let mut gaps = Vec::new();
        gaps.extend(self.gust_envelope.not_available());
        gaps.extend(self.wing_fuel.not_available());
        gaps.extend(self.wing_fuel_distribution.not_available());
        gaps.extend(self.wing_mounted_omissions.iter().copied());
        gaps.extend(self.root_datum.not_available());
        gaps.extend(self.solve_omissions.iter().copied());
        gaps
    }

    /// Whether any declared gap can leave the box lighter than the aircraft
    /// needs.
    ///
    /// The conservative gaps are reported all the same; this predicate is what
    /// separates "heavier than it needs to be, and we know why" from "possibly
    /// under-sized, and we know why".
    pub fn has_unconservative_gap(&self) -> bool {
        self.not_available()
            .iter()
            .any(|gap| !matches!(gap.direction, OmissionDirection::Heavier))
    }

    /// Whether the loading the box was sized at is bounded by declared limits
    /// rather than assumed.
    ///
    /// This is about the load case only. Whether the solve that applied it
    /// settled is [`Self::relief_convergence`], and the two are kept apart on
    /// purpose: a bounded envelope solved badly and an assumed envelope solved
    /// well are different problems with different owners.
    pub fn envelope_is_bounded(&self) -> bool {
        self.wing_fuel.is_bounded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gust_envelope_is_declared_missing_and_declared_unconservative() {
        // A gust case that is not evaluated cannot make the box heavier, so
        // this gap must never be reported as a conservatism.
        let gap = GustEnvelope::not_modelled()
            .not_available()
            .expect("the gust envelope declares its own gap");
        assert_eq!(gap.direction, OmissionDirection::Lighter);
        assert!(gap.quantity.contains("25.341"));
    }

    #[test]
    fn a_full_tank_case_declares_no_placement_gap_and_a_partial_one_does() {
        // Placement is only a free choice when the tanks are not full.
        assert_eq!(
            WingFuelDistribution::declared_cells_scaled(false).not_available(),
            None
        );
        let gap = WingFuelDistribution::declared_cells_scaled(true)
            .not_available()
            .expect("a partial fill has an unsourced placement");
        assert_eq!(gap.direction, OmissionDirection::Unknown);
    }

    #[test]
    fn the_geometric_distribution_declares_no_placement_gap() {
        // The enclosed-volume shape is derived, not chosen, so there is no
        // unsourced placement to declare.
        assert_eq!(
            WingFuelDistribution::EnclosedBoxVolume.not_available(),
            None
        );
    }

    #[test]
    fn an_unsettled_relief_solve_is_not_reported_as_converged() {
        assert!(ReliefConvergence::NotIterated.is_settled());
        assert!(ReliefConvergence::Settled {
            passes: 3,
            relative_change: 1e-12,
        }
        .is_settled());
        assert!(!ReliefConvergence::NotSettled {
            passes: 8,
            relative_change: 1e-3,
            tolerance: 1e-9,
        }
        .is_settled());
    }

    #[test]
    fn the_two_solve_level_omissions_are_both_conservative() {
        // Both leave the box heavier; if either flips, the predicate that
        // separates conservative from unconservative gaps has to be revisited.
        assert_eq!(
            POINT_MASS_SHEAR_RELIEF.direction,
            OmissionDirection::Heavier
        );
        assert_eq!(
            WING_MOUNTED_GEAR_RELIEF.direction,
            OmissionDirection::Heavier
        );
    }
}
