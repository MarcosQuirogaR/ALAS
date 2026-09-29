// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reference tire database and bogie-sizing ladder.
//!
//! The tire classes and the bogie-count ladder are independent of the
//! reaction-load and station arithmetic in the parent module.

/// A representative transport-category tire class, at conceptual-design
/// fidelity (not a specific certified part number).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TireSpec {
    /// The class key: `"light"`, `"narrowbody"`, `"widebody"` or `"heavy"`.
    pub code: &'static str,
    /// The human-readable class name.
    pub name: &'static str,
    /// Maximum rated static load per tire, kgf.
    pub rated_load_kg: f64,
    /// Tire outer diameter, m.
    pub diameter_m: f64,
    /// Tire section width, m.
    pub width_m: f64,
}

/// Light transport (~24x7.7 class).
pub const LIGHT: TireSpec = TireSpec {
    code: "light",
    name: "Light transport (~24x7.7 class)",
    rated_load_kg: 3_500.0,
    diameter_m: 0.61,
    width_m: 0.20,
};
/// Narrowbody main tire (46x17.0R20 class, A320/737 main gear).
///
/// Primary source: Goodyear Aviation, *Aircraft Tire Data
/// Book*, Data Section 2022, Sec. 4F Radial: size 46x17.0R20, 30 ply rating,
/// rated load 46,000 lb (20,865 kg), rated inflation 222 psi, max braking
/// load 69,000 lb (part numbers 467Q02-3 Amdt A, 467Q02-6). 69,000/46,000 =
/// 1.5, matching the landing-gear configuration's
/// `tire_dynamic_rating_factor` default.
pub const NARROWBODY: TireSpec = TireSpec {
    code: "narrowbody",
    name: "Narrowbody main (46x17.0R20 class, A320/737 MLG)",
    rated_load_kg: 20_865.25,
    diameter_m: 1.17,
    width_m: 0.44,
};
/// Widebody (~52x21 class, 787/A330).
pub const WIDEBODY: TireSpec = TireSpec {
    code: "widebody",
    name: "Widebody (~52x21 class, 787/A330)",
    rated_load_kg: 24_000.0,
    diameter_m: 1.32,
    width_m: 0.53,
};
/// Heavy widebody (~54x21 class, A380/747).
pub const HEAVY: TireSpec = TireSpec {
    code: "heavy",
    name: "Heavy widebody (~54x21 class, A380/747)",
    rated_load_kg: 34_000.0,
    diameter_m: 1.40,
    width_m: 0.56,
};

/// The tire classes in ascending capacity: the order [`select_tire`] walks
/// to auto-select the smallest that covers a load.
pub const TIRE_DATABASE: [TireSpec; 4] = [LIGHT, NARROWBODY, WIDEBODY, HEAVY];

/// Standard main-gear bogie sizes (wheels per strut) a preliminary design
/// chooses between: odd counts and anything above 6/strut are not realistic
/// for a twin/quad-leg configuration at this design stage.
pub const STANDARD_BOGIE_SIZES: [i64; 3] = [2, 4, 6];

/// The tire class named by `code`, or `None` if it is not one of the four.
pub fn tire_by_class(code: &str) -> Option<TireSpec> {
    match code {
        "light" => Some(LIGHT),
        "narrowbody" => Some(NARROWBODY),
        "widebody" => Some(WIDEBODY),
        "heavy" => Some(HEAVY),
        _ => None,
    }
}

/// Representative strut material by tire class, informational/labelling only
/// (this tool runs no structural analysis of the strut). An unrecognized code
/// falls back to the narrowbody steel.
pub fn strut_material_for(code: &str) -> &'static str {
    match code {
        "light" => "7075-T6 aluminium",
        "widebody" => "300M high-strength steel",
        "heavy" => "300M high-strength steel (titanium truck beam)",
        // "narrowbody" and the fallback are the same value.
        _ => "300M high-strength steel",
    }
}

/// Pick a tire class: an explicit choice, or the smallest whose rating covers
/// `design_load_per_wheel_kg`. Falls back to the heaviest class (which may
/// still be under-rated): `_select_tire`.
pub fn select_tire(design_load_per_wheel_kg: f64, tire_class: &str) -> TireSpec {
    if let Some(spec) = tire_by_class(tire_class) {
        return spec;
    }
    for spec in TIRE_DATABASE {
        if spec.rated_load_kg >= design_load_per_wheel_kg {
            return spec;
        }
    }
    HEAVY
}

/// Smallest standard bogie size + matching tire whose capacity covers
/// `strut_load_kg`: `_size_bogie`.
///
/// The tire is re-selected for each candidate wheel count (the per-wheel load
/// it would actually see) and the same tire is checked against the design load
/// and returned, so the count and the tire are self-consistent. A positive
/// `forced_count` short-circuits the ladder and takes that count directly.
///
/// Returns `(wheel_count, tire, overloaded)`, where `overloaded` is true when
/// the returned tire's rated capacity (at this bogie's actual per-wheel load)
/// does not cover `design_load`: this is reported on every path,
/// including a forced wheel count, rather than silently accepted.
pub fn size_bogie(
    strut_load_kg: f64,
    safety_factor: f64,
    tire_class: &str,
    forced_count: i64,
) -> (i64, TireSpec, bool) {
    let design_load = strut_load_kg * safety_factor;
    let counts: &[i64] = if forced_count > 0 {
        std::slice::from_ref(&forced_count)
    } else {
        &STANDARD_BOGIE_SIZES
    };
    for &n in counts {
        let tire = select_tire(strut_load_kg / n as f64, tire_class);
        let capacity = n as f64 * tire.rated_load_kg;
        if forced_count > 0 || capacity >= design_load {
            return (n, tire, capacity < design_load);
        }
    }
    // Nothing in the standard ladder covers it: return the largest bogie
    // with the tire sized for its actual per-wheel load (may still be
    // under-rated; a legitimate finding, not silently hidden).
    let n = STANDARD_BOGIE_SIZES[STANDARD_BOGIE_SIZES.len() - 1];
    let tire = select_tire(strut_load_kg / n as f64, tire_class);
    let overloaded = n as f64 * tire.rated_load_kg < design_load;
    (n, tire, overloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_tire_honours_an_explicit_class_even_when_it_is_under_rated() {
        // An explicit "light" is returned though the load far exceeds its
        // rating: the explicit branch does not fall through to a bigger tire.
        let tire = select_tire(1_000_000.0, "light");
        assert_eq!(tire.code, "light");
    }

    #[test]
    fn select_tire_auto_picks_the_smallest_class_that_covers_the_load() {
        assert_eq!(select_tire(2_000.0, "auto").code, "light");
        assert_eq!(select_tire(15_000.0, "auto").code, "narrowbody");
        assert_eq!(select_tire(21_000.0, "auto").code, "widebody");
        assert_eq!(select_tire(30_000.0, "auto").code, "heavy");
        // Above every rating, the heaviest class is returned.
        assert_eq!(select_tire(50_000.0, "auto").code, "heavy");
    }

    #[test]
    fn size_bogie_forced_count_takes_that_count_regardless_of_capacity() {
        let (n, tire, overloaded) = size_bogie(200_000.0, 1.07, "auto", 6);
        assert_eq!(n, 6);
        // The tire is sized for the forced per-wheel load, not re-derived from
        // the standard ladder.
        assert_eq!(tire, select_tire(200_000.0 / 6.0, "auto"));
        // 6 * 34,000 kg (heavy) = 204,000 kg >= 200,000*1.07 = 214,000 kg? No:
        // 204,000 < 214,000, so this forced bogie is under-rated and must say so.
        assert!(overloaded);
    }

    #[test]
    fn size_bogie_reports_no_overload_when_the_ladder_covers_the_load() {
        let (n, tire, overloaded) = size_bogie(30_000.0, 1.07, "auto", 0);
        assert_eq!(n, 2);
        assert_eq!(tire.code, "narrowbody");
        assert!(!overloaded);
    }
}
