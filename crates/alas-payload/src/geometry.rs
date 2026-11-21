// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`DeckSpec`, `CabinGeometry`)

//! Where the cabin is, how wide the floor is at each station, and where each
//! deck sits inside the fuselage.
//!
//! Both layout engines, the deck-plan drawings and the cargo loader all read
//! from this one object, which is the point of it: seats packed against a
//! floor width the hold loader disagreed about would produce a cabin and a
//! belly that cannot both be in the same aeroplane. It samples the *built*
//! fuselage rather than the configuration, so a body the design vector
//! stretched is the body the interior is laid out in.
//!
//! # Two things worth knowing before reading the numbers
//!
//! The deck fractions are of the internal half-height `b`, measured from the
//! section centre with +z up, so a deck occupies the band
//! `[zc + floor_frac*b, zc + ceil_frac*b]`. Items rest *on* the floor and have
//! their height clamped to the band, which is what stops a double-deck body's
//! two cabins from overlapping vertically.
//!
//! And the double-deck test is a shape test, not a name test: a body whose
//! declared height clears 1.15 diameters is treated as an A380-style ovoid and
//! gets two passenger decks. A circular fuselage leaves `height_m` unset and
//! can never reach it, which is why the test reads the raw `Option` rather
//! than [`alas_config::FuselageConfig::effective_height_m`]: the latter
//! falls back to the diameter, and `d >= 1.15 d` would be a different
//! question that happens to have the same answer today.
//!
//! `_xsec_width` and `_xsec_height` do not survive the translation. Upstream
//! keeps them in `stability.py` to read a cross-section that may carry either
//! a `radius` or a `width`/`height` pair; `alas-geom::aircraft::fuselage`'s
//! `FuselageXSec` resolves that in its constructor, so both accessors are the
//! fields themselves here.

use crate::numeric::interp;
use alas_config::GeometryConfig;
use alas_geom::aircraft::airplane::Airplane;

mod envelope;

mod model;
pub use envelope::HoldVertexClearance;
pub use hold::HoldSection;
pub(crate) use hold::MIN_HOLD_HEIGHT_M;
pub use model::*;

/// Armrest height above the passenger floor, m: 25 in (0.635 m). The armrest
/// tops measure 0.62 +- 0.03 m on Airbus AC A320 fig. 2-5-0-991-005 (pixel
/// measure on the vector figure, floor at the seat-rail base, scale the printed
/// 3.63 m row span); the lined chord at this height is the width a row of seats
/// has between the walls.
pub const ARMREST_ABOVE_FLOOR_M: f64 = 0.635;

/// Seat-pan height above the floor, m: measured on the manufacturers' cabin
/// sections, 0.47 m on the A320 (Airbus AC A320 fig. 2-5-0-991-005) and on the
/// 787 (Boeing D6-58333 fig. 2.5.1). Seat height is a design value, so the
/// ANSUR II popliteal height (men 0.430 m median, 0.471 m 95th percentile) is
/// only the check that it suits the population. Below the pan the seat base
/// stands on its tracks narrower than the row, so a row's occupied envelope
/// starts here.
pub const SEAT_PAN_ABOVE_FLOOR_M: f64 = 0.47;

/// Sitting acromion (shoulder) height above the seat, m: the ANSUR II 5th-
/// percentile adult woman (1,986 women: 5th 0.515, median 0.564, 95th 0.609 m; men
/// 0.552 / 0.603 / 0.653 m; `sittingheight - (stature - acromialheight)`). The
/// rule uses the highest ANSUR II percentile the manufacturer's own drawn A320
/// cabin passes at the window seat;
/// that is the 5th-percentile woman. The median woman is 0.025 m over the A320
/// lining there and the 95th-percentile man 0.120 m (`cabin_section.rs`).
pub const SHOULDER_ABOVE_SEAT_M: f64 = 0.515;

/// Height of the governing occupant's shoulders above the floor, m.
pub const SHOULDER_ABOVE_FLOOR_M: f64 = SEAT_PAN_ABOVE_FLOOR_M + SHOULDER_ABOVE_SEAT_M;

/// Height above the floor at which the drawn shoulder-level lining widths
/// (`CabinWidthsConfig::shoulder_m`) were read on the manufacturers' sections, m:
/// the seat pan plus the median woman's sitting acromion (0.47 + 0.564 m), the
/// earlier shoulder height. The widths stay anchored where they were measured;
/// the rule's own shoulder height (`SHOULDER_ABOVE_FLOOR_M`) is read through the
/// lofted lining shape from there.
pub const LINING_SHOULDER_READ_ABOVE_FLOOR_M: f64 = 1.034;

/// Half the bideltoid breadth of the 5th-percentile adult woman, m (ANSUR II:
/// 5th 0.406, median 0.450, 95th 0.499 m; men 0.459 / 0.509 / 0.567 m).
pub const SHOULDER_HALF_BREADTH_M: f64 = 0.203;

/// Height of a seated occupant's body centre of gravity above the seat surface, m:
/// the mean of the ANSUR II median man (0.263 m, 4,082 men; 5th-95th 0.242-0.285) and
/// median woman (0.243 m, 1,986 women; 0.224-0.263), from Winter's (2009,
/// Table 4.1, after Dempster 1955, printed p. 86) segment masses and centre positions
/// placed on each person's own ANSUR II landmarks: trunk from the hip joint (half
/// the thigh clearance above the seat) to the glenohumeral joint, which is at
/// 0.818 of the stature in Winter's own Fig. 4.1 (printed p. 83; 0.004 m below the
/// ANSUR II median acromion);
/// head and neck centred at the ear canal (Winter: centre at the ear-canal end);
/// upper arm to the elbow rest, forearms and hands on the thighs, thighs level,
/// shanks and feet down to the floor (feet that cannot reach it, popliteal height
/// plus a 0.025 m shoe below the pan, would raise the value by 0.003 m). The 50/50
/// male/female mean is an assumed passenger mix (no airline mix is carried; the two
/// medians differ by 0.020 m). Hand baggage is not part of it: see
/// [`occupant_row_cg_above_floor_m`].
pub const SEATED_OCCUPANT_CG_ABOVE_SEAT_M: f64 = 0.253;

/// Mean carry-on mass of an economy or premium-economy passenger, kg: EASA,
/// Review of Standard Passenger Weights (2022), Table 13 (passenger and carry-on
/// luggage per class in 2022, p. 42 of the report): 7.6 kg,
/// standard deviation 5.0 kg, n = 4,005. The all-adult mean of Table 6 (7.7 kg)
/// is the figure the earlier single-value model used.
pub const CARRY_ON_MASS_ECONOMY_KG: f64 = 7.6;

/// Mean carry-on mass of a business or first-class passenger, kg: EASA 2022
/// Table 13: 8.7 kg, standard deviation 5.2 kg, n = 126 (the survey's sample is
/// small for this class).
pub const CARRY_ON_MASS_BUSINESS_FIRST_KG: f64 = 8.7;

/// Mean carry-on mass for a cabin class by its name, kg: the business and first
/// classes take [`CARRY_ON_MASS_BUSINESS_FIRST_KG`], the others
/// [`CARRY_ON_MASS_ECONOMY_KG`] (EASA 2022 Table 13 groups economy with premium
/// economy). The class names are those of the seating pass (`First`, `Business`,
/// `Premium`, `Economy`).
pub fn carry_on_mass_kg(class_name: &str) -> f64 {
    match class_name {
        "First" | "Business" => CARRY_ON_MASS_BUSINESS_FIRST_KG,
        _ => CARRY_ON_MASS_ECONOMY_KG,
    }
}

/// Height above the floor at which the carry-on luggage's centre of gravity is
/// carried, m: the centre of the overhead bin's interior on the A320 section
/// (AC A320 fig. 2-5-0-991-005: the bin spans 1.69-2.04 m above the floor, pixel
/// reading to +-0.03 m). The carry-on mass is the EASA 2022 per-class mean
/// (Table 13, see [`carry_on_mass_kg`]), and all of it is taken in the bin. The
/// survey counts under-seat items (purses, pillows) in that mean and does not
/// split them out, so the all-in-bin placement is an assumption, not a sourced
/// split (open: no under-seat share source was found).
/// Each 10 % of the carry-on mass under the seat (0.2 m) instead lowers a row's CG
/// by 0.015 m.
pub const HAND_BAGGAGE_ABOVE_FLOOR_M: f64 = 1.865;

/// Height above the floor of the centre of gravity of one seat row's occupants,
/// m, for a standard passenger mass of `mass_per_pax_kg` that includes
/// `carry_on_kg` of carry-on (the class mean, see [`carry_on_mass_kg`]): the body
/// at the seated CG, the carry-on in the bin.
pub fn occupant_row_cg_above_floor_m(mass_per_pax_kg: f64, carry_on_kg: f64) -> f64 {
    let bag = carry_on_kg.min(mass_per_pax_kg);
    let body = SEAT_PAN_ABOVE_FLOOR_M + SEATED_OCCUPANT_CG_ABOVE_SEAT_M;
    if mass_per_pax_kg <= 0.0 {
        return body;
    }
    ((mass_per_pax_kg - bag) * body + bag * HAND_BAGGAGE_ABOVE_FLOOR_M) / mass_per_pax_kg
}

/// How much narrower a seat's back is than its unit width, per outboard side,
/// m: the armrest and its housing. Measured on Airbus AC A320 fig.
/// 2-5-0-991-005 (pixel measure on the figure: the outer seat backs span 3.534 m
/// in the 3.63 m row of 0.50 m units, so 0.048 m per side).
pub const SEAT_BACK_INSET_M: f64 = 0.048;

/// Height up to which a seat back keeps its full width, m: measured on the
/// same figure (3.534 m wide to 1.03 m, 3.519 m at 1.10 m, rounding off above
/// it to the back's top at 1.15 m).
pub const SEAT_BACK_FULL_WIDTH_TOP_M: f64 = 1.10;

/// How far the outboard edge of what stands at shoulder height (the occupant's
/// shoulder or the seat back, whichever is wider) reaches beyond the row's
/// outboard edge, m, for a seat `seat_w` wide (the unit width, armrests
/// included): half the bideltoid breadth less half the seat, never less than
/// the back's inset (a wide seat's back is `seat_w - 2 * SEAT_BACK_INSET_M`
/// wide, so it sits inside the row edge by that much).
pub fn shoulder_overhang_m(seat_w: f64) -> f64 {
    (SHOULDER_HALF_BREADTH_M - 0.5 * seat_w).max(-SEAT_BACK_INSET_M)
}
mod deck_tables;
use deck_tables::*;
mod hold;
mod hump_deck;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
