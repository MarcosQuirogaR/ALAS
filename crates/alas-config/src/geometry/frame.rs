// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one longitudinal frame and unit convention this program balances in,
//! and the explicit conversions into it.
//!
//! # The primary aircraft body frame
//!
//! * `x` positive **aft**, origin at the **fuselage nose tip** of the built
//!   model (`Airplane.fuselages[0].xsecs.first().xyz_c[0]`).
//! * `y` positive **starboard**.
//! * `z` positive **up**.
//! * Lengths and stations in **metres**; masses in **kilograms**; angles in
//!   **degrees**; centre-of-gravity positions and their limits in **percent
//!   MAC**, a dimensionless ratio referred to [`MacFrame`].
//!
//! This restates, in one place a consumer can import, the convention
//! `alas_mass::ledger` declares for the mass ledger and `docs/methods.md`
//! declares for the program. The origin is the part that used to be implicit:
//! every published station this crate registers is nose-tip referenced, and
//! nothing in the code said so or checked it.
//!
//! # Why a type rather than a comment
//!
//! `LandingGearConfig::reference_station_frame` is a free-text
//! `Option<String>`; every registered preset sets it to
//! `"nose_tip_drawing_reference"` and no consumer reads it. A station in a
//! manufacturer weighing datum, a fuselage-frame station number and a
//! drawing dimension from the nose tip are three different numbers for the
//! same physical point, and mixing them is a silent metre-scale error in a
//! balance calculation, not a compile failure.
//!
//! [`LongitudinalStationFrame`] makes the conversion the caller's explicit
//! choice and refuses rather than guesses when the inputs cannot support one.
//! It does not, by itself, prove that a registered station was measured in
//! the frame it claims; it removes the case where nobody stated a frame at
//! all.

/// Longitudinal extent of the built model's fuselage in the primary body
/// frame.
///
/// Both members come from the same built geometry, so a resized or optimized
/// fuselage carries its own extent rather than the source drawing's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyFuselageExtent {
    /// Station of the fuselage nose tip, m. Zero for the shipped builder,
    /// carried explicitly because nothing guarantees it.
    pub nose_tip_x_m: f64,
    /// Nose tip to tail tip along `x`, m.
    pub length_m: f64,
}

impl BodyFuselageExtent {
    /// The extent, or `None` when it is not a usable positive length.
    #[must_use]
    pub fn new(nose_tip_x_m: f64, length_m: f64) -> Option<Self> {
        (nose_tip_x_m.is_finite() && length_m.is_finite() && length_m > 0.0).then_some(Self {
            nose_tip_x_m,
            length_m,
        })
    }
}

/// The longitudinal frame a published station was measured in.
///
/// Conversions target the primary body frame described in the module
/// documentation and return `None` rather than an approximation whenever the
/// inputs do not define one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LongitudinalStationFrame {
    /// Already the primary body frame: metres aft of the model nose tip.
    BodyNoseTip,
    /// A source drawing dimensioned from the geometric nose-tip extension,
    /// normalized on that drawing's own overall length so a resized model
    /// re-applies the proportion instead of freezing the source metres.
    ///
    /// This is the frame every `reference_*_x_fraction` anchor in
    /// [`crate::LandingGearConfig`] is registered in.
    SourceDrawingNoseTip {
        /// Overall fuselage length the source drawing dimensions, m.
        drawing_fuselage_length_m: f64,
    },
    /// A published weighing/balance datum stated as a distance forward of the
    /// nose tip, as a type-certificate data sheet gives it (the A320-200
    /// registers 2.540 m, the ATR 72-600 2.362 m).
    PublishedDatumForwardOfNose {
        /// Distance from the datum plane aft to the nose tip, m, positive.
        datum_forward_of_nose_m: f64,
    },
}

impl LongitudinalStationFrame {
    /// Convert one station into the primary body frame, m.
    ///
    /// `station` is read in `self`'s own units: metres for
    /// [`Self::BodyNoseTip`] and [`Self::PublishedDatumForwardOfNose`], and a
    /// dimensionless fraction of the drawing length for
    /// [`Self::SourceDrawingNoseTip`], which is how the anchors are stored.
    ///
    /// Returns `None` when any input is not finite, when a drawing length is
    /// not positive, or when a drawing fraction falls outside `[0, 1]` - a
    /// fraction off the drawing is a data error, not a point to extrapolate.
    #[must_use]
    pub fn to_body_x_m(self, station: f64, extent: BodyFuselageExtent) -> Option<f64> {
        if !station.is_finite() {
            return None;
        }
        match self {
            Self::BodyNoseTip => Some(station),
            Self::SourceDrawingNoseTip {
                drawing_fuselage_length_m,
            } => {
                if !drawing_fuselage_length_m.is_finite() || drawing_fuselage_length_m <= 0.0 {
                    return None;
                }
                (0.0..=1.0)
                    .contains(&station)
                    .then_some(extent.nose_tip_x_m + extent.length_m * station)
            }
            Self::PublishedDatumForwardOfNose {
                datum_forward_of_nose_m,
            } => (datum_forward_of_nose_m.is_finite() && datum_forward_of_nose_m >= 0.0)
                .then_some(extent.nose_tip_x_m + station - datum_forward_of_nose_m),
        }
    }
}

/// The mean-aerodynamic-chord reference that turns a body-frame station into
/// percent MAC.
///
/// Both members are in the primary body frame; percent MAC is a ratio, never
/// a length, and `100.0` is the trailing edge of the mean chord rather than
/// any aircraft-level fraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacFrame {
    /// Leading edge of the mean aerodynamic chord, m aft of the nose tip.
    pub leading_edge_x_m: f64,
    /// Mean aerodynamic chord length, m.
    pub chord_m: f64,
}

impl MacFrame {
    /// The reference, or `None` when the chord is not a usable positive
    /// length.
    #[must_use]
    pub fn new(leading_edge_x_m: f64, chord_m: f64) -> Option<Self> {
        (leading_edge_x_m.is_finite() && chord_m.is_finite() && chord_m > 0.0).then_some(Self {
            leading_edge_x_m,
            chord_m,
        })
    }

    /// A body-frame station in percent MAC.
    #[must_use]
    pub fn pct_mac(self, x_m: f64) -> Option<f64> {
        x_m.is_finite()
            .then(|| 100.0 * (x_m - self.leading_edge_x_m) / self.chord_m)
    }

    /// A percent-MAC position back as a body-frame station, m.
    #[must_use]
    pub fn x_m(self, pct_mac: f64) -> Option<f64> {
        pct_mac
            .is_finite()
            .then(|| self.leading_edge_x_m + pct_mac / 100.0 * self.chord_m)
    }
}

// Tests assert on values they constructed here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn a320_extent() -> BodyFuselageExtent {
        // The shipped builder lofts the A320-200 fuselage from x = 0 to the
        // design vector's 37.57 m overall length.
        BodyFuselageExtent {
            nose_tip_x_m: 0.0,
            length_m: 37.57,
        }
    }

    #[test]
    fn a_drawing_fraction_scales_onto_the_active_fuselage() {
        // A320-200 main-gear anchor: 17.71 m on a 37.57 m drawing.
        let frame = LongitudinalStationFrame::SourceDrawingNoseTip {
            drawing_fuselage_length_m: 37.57,
        };
        let station = frame
            .to_body_x_m(17.71 / 37.57, a320_extent())
            .expect("an in-range drawing fraction converts");
        assert!((station - 17.71).abs() < 1.0e-9, "{station} m");

        let shrunk = BodyFuselageExtent {
            nose_tip_x_m: 0.0,
            length_m: 30.0,
        };
        let scaled = frame
            .to_body_x_m(17.71 / 37.57, shrunk)
            .expect("the same fraction re-applies to a shorter fuselage");
        assert!(
            (scaled - 30.0 * (17.71 / 37.57)).abs() < 1.0e-9,
            "{scaled} m"
        );
    }

    #[test]
    fn a_published_datum_station_is_moved_to_the_nose_tip_origin() {
        // EASA.A.064 Issue 12 items 15-16: the A320 datum is 2.540 m forward
        // of the nose, so a 20.000 m datum station is 17.460 m aft of the
        // nose tip. Converting it is the step that used to be absent.
        let frame = LongitudinalStationFrame::PublishedDatumForwardOfNose {
            datum_forward_of_nose_m: 2.540,
        };
        let station = frame
            .to_body_x_m(20.0, a320_extent())
            .expect("a finite datum station converts");
        assert!((station - 17.460).abs() < 1.0e-9, "{station} m");
    }

    #[test]
    fn a_fraction_off_the_drawing_is_refused_rather_than_extrapolated() {
        let frame = LongitudinalStationFrame::SourceDrawingNoseTip {
            drawing_fuselage_length_m: 37.57,
        };
        assert_eq!(frame.to_body_x_m(1.2, a320_extent()), None);
        assert_eq!(frame.to_body_x_m(-0.01, a320_extent()), None);
        assert_eq!(frame.to_body_x_m(f64::NAN, a320_extent()), None);
    }

    #[test]
    fn a_degenerate_extent_or_chord_is_refused() {
        assert_eq!(BodyFuselageExtent::new(0.0, 0.0), None);
        assert_eq!(BodyFuselageExtent::new(f64::NAN, 10.0), None);
        assert_eq!(MacFrame::new(16.29, 0.0), None);
        assert_eq!(MacFrame::new(16.29, f64::NAN), None);
    }

    #[test]
    fn percent_mac_round_trips_through_the_body_frame() {
        let mac = MacFrame::new(16.29, 4.195_872_915_957_396).expect("a positive chord");
        let pct = mac.pct_mac(17.71).expect("a finite station");
        // A320-200 main-gear station in the built model's own MAC frame.
        assert!((pct - 33.842).abs() < 1.0e-3, "{pct} % MAC");
        let back = mac.x_m(pct).expect("a finite percentage");
        assert!((back - 17.71).abs() < 1.0e-9, "{back} m");
    }
}
