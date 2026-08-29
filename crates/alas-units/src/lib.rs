// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversion factors between the units the models are written in and base SI.
//!
//! Every value here is the number of base SI units in one of something else:
//! `FOOT` is 0.3048 because a foot is 0.3048 metres. Multiplying converts into
//! SI, dividing converts out of it, which is the same convention mission analysis model's unit
//! table uses and therefore the same one the translated correlations read as.
//!
//! This crate exists because the transport weight correlations are written in
//! imperial units and bracket themselves with these ratios. A factor wrong in
//! its tenth digit moves an aircraft's empty weight with nothing looking
//! broken, and that class of error is invisible to every other check in the
//! program. It is the first thing translated for that reason.
//!
//! # Definitions, not measurements
//!
//! The values are the exact legal definitions -- the international yard and
//! pound agreement of 1959 for length and mass, the standard gravity of
//! 9.80665 m/s^2 for force -- rather than transcriptions of what mission analysis model's unit
//! library happens to compute. Derived units are written as their definitions
//! (`PSI` is a pound-force over a square inch) so the source of each is on the
//! page.
//!
//! That choice makes a handful of these differ from mission analysis model in the last bit or
//! two, because mission analysis model reaches some of them by division: its inch is a twelfth
//! of its foot, which is not exactly 0.0254. The parity test allows that, and
//! `docs/PORTING.md` records why. Transcribing the artifact instead would mean
//! writing 0.025400000000000002 into a file that claims to define an inch.

/// Standard gravity, which is what makes a pound-force out of a pound-mass.
///
/// Defined exactly by the third CGPM, 1901.
pub const STANDARD_GRAVITY: f64 = 9.80665;

// -- Length. International yard and pound agreement, 1959. ------------------

/// Metres in a metre.
pub const METER: f64 = 1.0;
/// Metres in a kilometre.
pub const KILOMETER: f64 = 1000.0;
/// Metres in a foot.
pub const FOOT: f64 = 0.3048;
/// Metres in an inch.
pub const INCH: f64 = 0.0254;
/// Metres in a nautical mile, defined exactly since 1929.
pub const NAUTICAL_MILE: f64 = 1852.0;

// -- Mass -------------------------------------------------------------------

/// Kilograms in a kilogram.
pub const KILOGRAM: f64 = 1.0;
/// Kilograms in a gram.
///
/// Present because mission analysis model's unit table reads the name `g` as a gram. Nothing on
/// this program's path asks it for gravity, but a reader who assumes otherwise
/// would be wrong by four orders of magnitude, so the name is spelled out.
pub const GRAM: f64 = 0.001;
/// Kilograms in a pound-mass.
pub const POUND_MASS: f64 = 0.45359237;

// -- Force ------------------------------------------------------------------

/// Newtons in a newton.
pub const NEWTON: f64 = 1.0;
/// Newtons in a pound-force: a pound-mass under standard gravity.
pub const POUND_FORCE: f64 = POUND_MASS * STANDARD_GRAVITY;

// -- Time and speed ---------------------------------------------------------

/// Seconds in a second.
pub const SECOND: f64 = 1.0;
/// Seconds in an hour.
pub const HOUR: f64 = 3600.0;
/// Metres per second in a knot: one nautical mile per hour.
pub const KNOT: f64 = NAUTICAL_MILE / HOUR;

// -- Angle ------------------------------------------------------------------

/// Radians in a degree.
pub const DEGREE: f64 = std::f64::consts::PI / 180.0;

// -- Temperature ------------------------------------------------------------

/// Kelvin in a degree Rankine.
///
/// A ratio, not an offset: Rankine and Kelvin share a zero, so this converts
/// intervals and absolute temperatures alike.
pub const DEGREE_RANKINE: f64 = 5.0 / 9.0;

// -- Pressure ---------------------------------------------------------------

/// Pascals in a pascal.
pub const PASCAL: f64 = 1.0;
/// Pascals in a pound-force per square inch.
pub const PSI: f64 = POUND_FORCE / (INCH * INCH);

// -- Volume -----------------------------------------------------------------

/// Cubic metres in a US liquid gallon, defined as 231 cubic inches.
pub const GALLON: f64 = 231.0 * INCH * INCH * INCH;

// -- Power ------------------------------------------------------------------

/// Watts in a watt.
pub const WATT: f64 = 1.0;
/// Watts in a kilowatt.
pub const KILOWATT: f64 = 1000.0;
/// Watts in a mechanical horsepower: 550 foot-pounds-force per second.
pub const HORSEPOWER: f64 = 550.0 * FOOT * POUND_FORCE;

/// The factor for a unit named the way mission analysis model's table names it.
///
/// The spellings are mission analysis model's, including its several synonyms for the same
/// unit, because the translated correlations were read against them and the
/// parity test walks this function to prove none was missed.
///
/// Returns `None` for a name this program has no use for, which is the honest
/// answer: an unknown unit should stop a caller rather than silently convert
/// by one.
pub fn factor(name: &str) -> Option<f64> {
    let value = match name {
        "m" | "meter" | "meters" => METER,
        "km" => KILOMETER,
        "ft" | "feet" | "foot" => FOOT,
        "inch" | "inches" => INCH,
        "nmi" | "nautical_mile" | "nautical_miles" => NAUTICAL_MILE,

        "kg" | "kilogram" | "kilograms" => KILOGRAM,
        "g" | "gram" | "grams" => GRAM,
        "lb" | "lbs" | "pound" | "pounds" => POUND_MASS,

        "N" | "newton" | "newtons" => NEWTON,
        "lbf" | "force_pound" => POUND_FORCE,

        "s" | "sec" | "second" | "seconds" => SECOND,
        "hour" | "hours" => HOUR,
        "knots" | "kts" | "knot" => KNOT,

        "deg" | "degree" | "degrees" => DEGREE,
        "degR" => DEGREE_RANKINE,

        "pascal" | "pascals" | "Pa" => PASCAL,
        "psi" => PSI,

        "gallon" | "gallons" => GALLON,

        "W" | "watt" | "watts" => WATT,
        "kW" => KILOWATT,
        "horsepower" | "hp" => HORSEPOWER,

        _ => return None,
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defining_values_are_exact() {
        assert_eq!(FOOT, 0.3048);
        assert_eq!(INCH, 0.0254);
        assert_eq!(POUND_MASS, 0.45359237);
        assert_eq!(NAUTICAL_MILE, 1852.0);
    }

    #[test]
    fn derived_units_follow_from_their_definitions() {
        assert_eq!(POUND_FORCE, 0.45359237 * 9.80665);
        assert_eq!(KNOT, 1852.0 / 3600.0);
        assert_eq!(GALLON, 231.0 * INCH.powi(3));
    }

    #[test]
    fn multiplying_converts_into_si() {
        // Thirty-five thousand feet is a shade under eleven kilometres.
        let altitude_m = 35_000.0 * FOOT;
        assert!((altitude_m - 10_668.0).abs() < 1e-9);
    }

    #[test]
    fn dividing_converts_out_of_si() {
        let cruise_m_per_s = 230.0;
        let knots = cruise_m_per_s / KNOT;
        assert!((knots - 447.08).abs() < 0.01);
    }

    #[test]
    fn synonyms_agree() {
        for (a, b) in [
            ("lb", "lbs"),
            ("ft", "feet"),
            ("deg", "degrees"),
            ("nmi", "nautical_miles"),
        ] {
            assert_eq!(factor(a), factor(b), "{a} and {b} should be the same unit");
        }
    }

    #[test]
    fn an_unknown_unit_is_not_silently_one() {
        assert_eq!(factor("furlong"), None);
    }

    #[test]
    fn gram_is_a_mass_not_an_acceleration() {
        assert_eq!(factor("g"), Some(0.001));
    }
}
