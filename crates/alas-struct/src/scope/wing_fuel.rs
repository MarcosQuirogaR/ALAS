// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing fuel a structural design case may be relieved by, and what bounds it.

use super::{NotAvailable, OmissionDirection};

/// The wing fuel the sizing case is relieved by, with the limit that bounds it.
///
/// Masses are kilogrammes for the **whole aircraft**, as published capacities
/// are; a caller distributing one over a semi-wing halves it.
///
/// # Why a declared tank capacity is not a load case on its own
///
/// [`crate::loads::load_cases`] applies the ultimate manoeuvre at the design
/// gross mass `DG`. Relieving that case with the **full** integral wing capacity
/// `C` asserts that the aircraft is at `DG` *and* has full wings, which is one
/// point of the loading envelope and not, in general, the one that sizes the
/// box. An operator may dispatch at `DG` with the maximum payload the airframe
/// permits, and then the fuel on board is only `DG - MZFW`; if that is less than
/// `C`, every kilogram of the difference is relief the structure was credited
/// with and does not have.
///
/// # The bound, and why it is an identity rather than a factor
///
/// Root bending is produced by the mass the wing does not carry, so at a mass
/// `W` with wing fuel `F` the relieved (bending) mass is `W - F` less the wing's
/// own structure and anything hung on it. Zero-fuel mass is limited by
/// certification, `W - F_total <= MZFW`, and wing fuel cannot exceed either the
/// total on board or the tanks, so the least wing fuel at `W` is
/// `min(C, max(0, W - MZFW))`. Maximising `W - F` over `W <= DG`:
///
/// ```text
/// case                     least wing fuel   greatest bending mass
/// W <= MZFW                F >= 0            W - F <= W     <= MZFW
/// MZFW < W, W - MZFW <= C  F >= W - MZFW     W - F <= MZFW
/// W - MZFW > C             F >= C            W - F <= W - C <= DG - C
/// ```
///
/// so the bending mass is `max(min(MZFW, DG), DG - C)` - the `min` only binds on
/// a configuration declaring a zero-fuel limit above its own design gross mass,
/// which is not an aircraft - which is `DG` less `min(C, max(0, DG - MZFW))`,
/// the design case below. This is the reason maximum zero-fuel mass is a
/// certified limit at all: it *is* the wing-bending limit. No coefficient is
/// introduced and nothing is calibrated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WingFuelDesignCase {
    /// No usable integral wing capacity is declared, so the case carries no
    /// fuel relief. Nothing is assumed and the box comes out heavier.
    NoDeclaredCapacity,
    /// The fuel was estimated from the volume the sized box encloses rather
    /// than from a declared capacity - the clean-sheet default, where there is
    /// no published tank to read.
    EnclosedBoxVolumeEstimate {
        /// Estimated usable integral wing fuel, both wings, kg.
        estimated_kg: f64,
        /// The estimate is not a declared quantity and is not envelope-bounded.
        gap: NotAvailable,
    },
    /// The tanks bind: `DG - MZFW >= C`, so the whole declared capacity is
    /// guaranteed on board at the design gross mass.
    TankLimited {
        /// Declared usable integral wing capacity, both wings, kg.
        capacity_kg: f64,
        /// Design gross mass the manoeuvre case is applied at, kg.
        design_gross_mass_kg: f64,
        /// Declared maximum zero-fuel mass, kg.
        max_zero_fuel_mass_kg: f64,
    },
    /// The zero-fuel limit binds: `DG - MZFW < C`, so the envelope guarantees
    /// less wing fuel than the tanks hold and only that much is credited.
    ZeroFuelLimited {
        /// Declared usable integral wing capacity, both wings, kg.
        capacity_kg: f64,
        /// Design gross mass the manoeuvre case is applied at, kg.
        design_gross_mass_kg: f64,
        /// Declared maximum zero-fuel mass, kg.
        max_zero_fuel_mass_kg: f64,
        /// Fuel the envelope guarantees in the wings at `DG`, both wings, kg.
        design_case_kg: f64,
    },
    /// No usable zero-fuel limit is declared, so the full capacity is credited
    /// as an **assumption about the loading**, not as a bounded load case.
    FullTanksAssumed {
        /// Declared usable integral wing capacity, both wings, kg.
        capacity_kg: f64,
        /// The missing limit.
        gap: NotAvailable,
    },
}

/// The estimate's spread against the declared capacities it stands in for.
///
/// Measured on this product's own registered fleet at their nominal designs
/// (an internal structures study, 2026-09-17):
/// the enclosed-volume estimate lands between `0.86` (A340-300) and `2.24`
/// (B787-9) times the declared integral wing capacity, because how much of an
/// aircraft's fuel sits in the wing rather than in a centre or auxiliary tank is
/// not a property of the box. It is therefore not a conservative estimate in
/// either direction.
const ENCLOSED_VOLUME_ESTIMATE_GAP: NotAvailable = NotAvailable {
    quantity: "declared usable integral wing-tank capacity, kg",
    reason: "the relieving fuel was estimated from the volume the sized box encloses because no \
             declared capacity was supplied. Measured across the registered fleet the estimate \
             lands between 0.86 and 2.24 times the declared wing capacity, so it is not bounded \
             in either direction, and no zero-fuel limit is applied to it.",
    resolved_by: "the aircraft's published usable wing-cell capacities and its maximum zero-fuel \
                  mass, supplied to the scoped sizing entry point",
    direction: OmissionDirection::Unknown,
};

/// A declared capacity with no zero-fuel limit to bound it.
const ZERO_FUEL_LIMIT_NOT_DECLARED: NotAvailable = NotAvailable {
    quantity: "maximum zero-fuel mass, kg",
    reason: "the configuration declares an integral wing capacity but no zero-fuel limit, so the \
             full capacity is credited as relief. That asserts the aircraft reaches its design \
             gross mass with full wings, which is one point of the loading envelope and not, in \
             general, the one that sizes the box.",
    resolved_by: "a declared or certified maximum zero-fuel mass for the aircraft",
    direction: OmissionDirection::Lighter,
};

impl WingFuelDesignCase {
    /// Resolve the case from a declared capacity and the loading limits.
    ///
    /// `max_zero_fuel_mass_kg` of `None` - or a non-finite or non-positive one,
    /// or a non-finite design gross mass - means there is no envelope to bound
    /// the case with, and the result is [`Self::FullTanksAssumed`]. A
    /// non-finite or non-positive capacity is [`Self::NoDeclaredCapacity`].
    pub fn declared(
        wing_tank_capacity_kg: f64,
        design_gross_mass_kg: f64,
        max_zero_fuel_mass_kg: Option<f64>,
    ) -> Self {
        if !wing_tank_capacity_kg.is_finite() || wing_tank_capacity_kg <= 0.0 {
            return Self::NoDeclaredCapacity;
        }
        let unbounded = Self::FullTanksAssumed {
            capacity_kg: wing_tank_capacity_kg,
            gap: ZERO_FUEL_LIMIT_NOT_DECLARED,
        };
        let Some(mzfw_kg) = max_zero_fuel_mass_kg else {
            return unbounded;
        };
        if !mzfw_kg.is_finite() || mzfw_kg <= 0.0 || !design_gross_mass_kg.is_finite() {
            return unbounded;
        }
        let guaranteed_kg = (design_gross_mass_kg - mzfw_kg).max(0.0);
        if guaranteed_kg >= wing_tank_capacity_kg {
            Self::TankLimited {
                capacity_kg: wing_tank_capacity_kg,
                design_gross_mass_kg,
                max_zero_fuel_mass_kg: mzfw_kg,
            }
        } else {
            Self::ZeroFuelLimited {
                capacity_kg: wing_tank_capacity_kg,
                design_gross_mass_kg,
                max_zero_fuel_mass_kg: mzfw_kg,
                design_case_kg: guaranteed_kg,
            }
        }
    }

    /// The case for a box relieved by the fuel its own geometry encloses.
    pub fn enclosed_box_volume(estimated_kg: f64) -> Self {
        Self::EnclosedBoxVolumeEstimate {
            estimated_kg,
            gap: ENCLOSED_VOLUME_ESTIMATE_GAP,
        }
    }

    /// Wing fuel credited as relief at the design case, both wings, kg.
    pub fn design_case_kg(&self) -> f64 {
        match *self {
            Self::NoDeclaredCapacity => 0.0,
            Self::EnclosedBoxVolumeEstimate { estimated_kg, .. } => estimated_kg,
            Self::TankLimited { capacity_kg, .. } | Self::FullTanksAssumed { capacity_kg, .. } => {
                capacity_kg
            }
            Self::ZeroFuelLimited { design_case_kg, .. } => design_case_kg,
        }
    }

    /// Whether a declared loading limit, rather than an assumption, sets the
    /// credited relief.
    pub fn is_bounded(&self) -> bool {
        matches!(
            self,
            Self::NoDeclaredCapacity | Self::TankLimited { .. } | Self::ZeroFuelLimited { .. }
        )
    }

    /// Whether the zero-fuel limit, rather than the tank, sets the relief.
    pub fn zero_fuel_limited(&self) -> bool {
        matches!(self, Self::ZeroFuelLimited { .. })
    }

    /// Whether the credited fuel is less than the declared tanks hold, so the
    /// spanwise placement of the remainder is a choice rather than a given.
    pub fn is_partial_fill(&self) -> bool {
        match *self {
            Self::ZeroFuelLimited {
                capacity_kg,
                design_case_kg,
                ..
            } => design_case_kg < capacity_kg,
            _ => false,
        }
    }

    /// The gap this case declares, if any.
    pub fn not_available(&self) -> Option<NotAvailable> {
        match *self {
            Self::EnclosedBoxVolumeEstimate { gap, .. } | Self::FullTanksAssumed { gap, .. } => {
                Some(gap)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wing_that_cannot_be_avoided_is_tank_limited_at_its_whole_capacity() {
        // DG - MZFW = 60 000 >= C = 40 000: the aircraft cannot reach its design
        // gross mass without filling its wings, so the tank is the bound.
        let case = WingFuelDesignCase::declared(40_000.0, 200_000.0, Some(140_000.0));
        assert!(matches!(case, WingFuelDesignCase::TankLimited { .. }));
        assert_eq!(case.design_case_kg(), 40_000.0);
        assert!(case.is_bounded());
        assert!(!case.is_partial_fill());
        assert_eq!(case.not_available(), None);
    }

    #[test]
    fn a_wing_that_can_be_avoided_is_limited_by_the_zero_fuel_mass() {
        // DG - MZFW = 20 000 < C = 40 000: only 20 000 kg is guaranteed.
        let case = WingFuelDesignCase::declared(40_000.0, 200_000.0, Some(180_000.0));
        assert_eq!(case.design_case_kg(), 20_000.0);
        assert!(case.zero_fuel_limited());
        assert!(
            case.is_partial_fill(),
            "a partial fill has a placement choice"
        );
        assert!(case.is_bounded());
    }

    #[test]
    fn a_design_mass_at_or_below_the_zero_fuel_limit_guarantees_no_wing_fuel() {
        // The identity clamps at zero rather than crediting negative relief.
        let case = WingFuelDesignCase::declared(40_000.0, 150_000.0, Some(180_000.0));
        assert_eq!(case.design_case_kg(), 0.0);
        assert!(case.is_bounded());
    }

    #[test]
    fn an_undeclared_zero_fuel_limit_is_an_assumption_and_says_so() {
        // This is the AVE's path: full tanks credited with nothing bounding it.
        for limit in [None, Some(f64::NAN), Some(0.0), Some(-1.0)] {
            let case = WingFuelDesignCase::declared(40_000.0, 200_000.0, limit);
            assert_eq!(case.design_case_kg(), 40_000.0, "limit {limit:?}");
            assert!(!case.is_bounded(), "limit {limit:?}");
            let gap = case
                .not_available()
                .expect("an unbounded case declares its gap");
            assert_eq!(gap.direction, OmissionDirection::Lighter);
        }
        // A design gross mass that is not a number cannot bound anything either.
        let case = WingFuelDesignCase::declared(40_000.0, f64::NAN, Some(180_000.0));
        assert!(!case.is_bounded());
    }

    #[test]
    fn no_declared_capacity_credits_no_relief_and_declares_no_gap() {
        // Zero relief is not an assumption about the loading: it is the
        // heavier box, and needs nothing disclosed with it.
        for capacity in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let case = WingFuelDesignCase::declared(capacity, 200_000.0, Some(140_000.0));
            assert_eq!(case, WingFuelDesignCase::NoDeclaredCapacity, "{capacity}");
            assert_eq!(case.design_case_kg(), 0.0);
            assert!(case.is_bounded());
            assert_eq!(case.not_available(), None);
        }
    }

    #[test]
    fn a_geometric_estimate_is_reported_as_unbounded_in_both_directions() {
        let case = WingFuelDesignCase::enclosed_box_volume(31_981.0);
        assert_eq!(case.design_case_kg(), 31_981.0);
        assert!(!case.is_bounded());
        let gap = case.not_available().expect("an estimate declares its gap");
        assert_eq!(gap.direction, OmissionDirection::Unknown);
    }
}
