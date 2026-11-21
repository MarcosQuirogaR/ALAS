// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reference tire database and bogie-sizing ladder.
//!
//! The tire classes and the bogie-count ladder are independent of the
//! reaction-load and station arithmetic in the parent module.

use alas_config::LandingGearConfig;

#[path = "certified.rs"]
mod certified;
pub use certified::CERTIFIED_TIRES;

/// How a tire's rated load was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingBasis {
    /// A generic conceptual class; no data-book entry.
    Class,
    /// The exact size and ply rating, read from a tire data book.
    Exact,
    /// The fitted designation with a rated load that is a lower bound: the
    /// same size at a lower ply rating, or a dominated envelope (outside
    /// diameter, section width and ply all no larger), from a data book.
    LowerBound,
}

impl RatingBasis {
    /// The word the report prints for how a tire's rated load was established
    /// (class estimate, data book, or lower bound).
    pub fn label(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Exact => "book",
            Self::LowerBound => "bound",
        }
    }
}

/// A tire: a representative transport-category class at conceptual-design
/// fidelity, or a certified data-book size (see [`CERTIFIED_TIRES`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TireSpec {
    /// The class key (`"light"`, `"narrowbody"`, `"widebody"`, `"heavy"`) or the
    /// data-book designation of a certified tire.
    pub code: &'static str,
    /// The human-readable class name.
    pub name: &'static str,
    /// Maximum rated static load per tire, kgf.
    pub rated_load_kg: f64,
    /// Tire outer diameter, m.
    pub diameter_m: f64,
    /// Tire section width, m.
    pub width_m: f64,
    /// Ply rating of the fitted designation; 0 for a generic class.
    pub ply_rating: u32,
    /// Rated speed, mph; 0 for a generic class.
    pub rated_speed_mph: f64,
    /// Rated inflation pressure, psi; 0 for a generic class.
    pub rated_inflation_psi: f64,
    /// How the rated load was established.
    pub rating_basis: RatingBasis,
    /// Maximum braking load per tire, kgf, where the data book rating row
    /// prints one (Goodyear 2022 "maximum braking load": 1.50 x the rated
    /// load for most bias and radial rows, 1.45 x for some); `None` where
    /// the source prints none (Michelin's engineering tables, generic classes).
    pub max_braking_load_kg: Option<f64>,
    /// Data-book page the entry is read from, or the statement that the
    /// class is generic.
    pub source: &'static str,
}

const GENERIC: &str = "Generic conceptual class, not a data-book entry";

/// Light transport (~24x7.7 class).
pub const LIGHT: TireSpec = TireSpec {
    code: "light",
    name: "Light transport (~24x7.7 class)",
    rated_load_kg: 3_500.0,
    diameter_m: 0.61,
    width_m: 0.20,
    ply_rating: 0,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::Class,
    max_braking_load_kg: None,
    source: GENERIC,
};
/// Narrowbody main tire class (46x17.0R20 rating and dimensions).
///
/// The rated load is that of Goodyear Aviation, *Aircraft Tire Data
/// Book*, Data Section 2022, Sec. 4F Radial: size 46x17.0R20, 30 ply rating,
/// rated load 46,000 lb (20,865 kg), rated inflation 222 psi, max braking
/// load 69,000 lb (part numbers 467Q02-3 Amdt A, 467Q02-6). 69,000/46,000 =
/// 1.5, matching the landing-gear configuration's
/// `tire_dynamic_rating_factor` default.
///
/// The outside diameter (47.50 in = 1.2065 m) and section width (17.70 in =
/// 0.4496 m) are the dimensions of the same data-book entry
/// (`R46X17_0R20_30PR`), because the class rating is that tyre's. The earlier
/// generic 1.17 m / 0.44 m understated the diameter by 3.7 cm. The flat-tyre
/// drop of an auto-tyred preset reads this diameter directly.
pub const NARROWBODY: TireSpec = TireSpec {
    code: "narrowbody",
    name: "Narrowbody main (46x17.0R20 class, A320/737 MLG)",
    rated_load_kg: 20_865.25,
    diameter_m: 1.2065,
    width_m: 0.4496,
    ply_rating: 0,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::Class,
    max_braking_load_kg: None,
    source: GENERIC,
};
/// Widebody (~52x21 class, 787/A330).
pub const WIDEBODY: TireSpec = TireSpec {
    code: "widebody",
    name: "Widebody (~52x21 class, 787/A330)",
    rated_load_kg: 24_000.0,
    diameter_m: 1.32,
    width_m: 0.53,
    ply_rating: 0,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::Class,
    max_braking_load_kg: None,
    source: GENERIC,
};
/// Heavy widebody (~54x21 class, A380/747).
pub const HEAVY: TireSpec = TireSpec {
    code: "heavy",
    name: "Heavy widebody (~54x21 class, A380/747)",
    rated_load_kg: 34_000.0,
    diameter_m: 1.40,
    width_m: 0.56,
    ply_rating: 0,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::Class,
    max_braking_load_kg: None,
    source: GENERIC,
};

/// The tire classes in ascending capacity: the order [`select_tire`] walks
/// to auto-select the smallest that covers a load.
pub const TIRE_DATABASE: [TireSpec; 4] = [LIGHT, NARROWBODY, WIDEBODY, HEAVY];

/// Standard main-gear bogie sizes (wheels per strut) a preliminary design
/// chooses between: odd counts and anything above 6/strut are not realistic
/// for a twin/quad-leg configuration at this design stage.
pub const STANDARD_BOGIE_SIZES: [i64; 3] = [2, 4, 6];

/// The tire key a gear sizes with: its declared data-book tire, else the class.
pub fn main_tire_key<'a>(tire_class: &'a str, mlg_tire: &'a str) -> &'a str {
    if mlg_tire == "auto" {
        tire_class
    } else {
        mlg_tire
    }
}

/// The selectable codes of a tire list: `auto`, then each tire's code.
pub fn option_codes(tires: &[TireSpec]) -> Vec<String> {
    std::iter::once("auto".to_owned())
        .chain(tires.iter().map(|tire| tire.code.to_owned()))
        .collect()
}

/// The tire class or certified data-book tire named by `code`, or `None`.
pub fn tire_by_class(code: &str) -> Option<TireSpec> {
    match code {
        "light" => Some(LIGHT),
        "narrowbody" => Some(NARROWBODY),
        "widebody" => Some(WIDEBODY),
        "heavy" => Some(HEAVY),
        _ => CERTIFIED_TIRES
            .iter()
            .copied()
            .find(|tire| tire.code == code),
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

/// The nose tire: the declared certified tire, else the class selected by `load_kg`.
pub fn nose_tire(load_kg: f64, gear: &LandingGearConfig) -> TireSpec {
    select_tire(load_kg, main_tire_key(&gear.tire_class, &gear.nlg_tire))
}

/// The strut material label: the declared one, else the label of the tire
/// class the worst per-wheel load alone selects, whatever certified tire is
/// declared (the label is informational).
pub fn strut_label(gear: &LandingGearConfig, worst_load_per_wheel_kg: f64) -> String {
    if gear.strut_material != "auto" {
        return gear.strut_material.clone();
    }
    strut_material_for(select_tire(worst_load_per_wheel_kg, &gear.tire_class).code).to_owned()
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
/// The tire is re-selected for each candidate wheel count from the per-wheel
/// design load it would actually see, safety factor included (TYRE107), and
/// the same tire is checked against the design load and returned, so the
/// count and the tire are self-consistent. A positive
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
        let tire = select_tire(design_load / n as f64, tire_class);
        let capacity = n as f64 * tire.rated_load_kg;
        if forced_count > 0 || capacity >= design_load {
            return (n, tire, capacity < design_load);
        }
    }
    // Nothing in the standard ladder covers it: return the largest bogie
    // with the tire sized for its actual per-wheel load (may still be
    // under-rated; a legitimate finding, not silently hidden).
    let n = STANDARD_BOGIE_SIZES[STANDARD_BOGIE_SIZES.len() - 1];
    let tire = select_tire(design_load / n as f64, tire_class);
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
        assert_eq!(tire, select_tire(200_000.0 * 1.07 / 6.0, "auto"));
        // 6 * 34,000 kg (heavy) = 204,000 kg >= 200,000*1.07 = 214,000 kg? No:
        // 204,000 < 214,000, so this forced bogie is under-rated and must say so.
        assert!(overloaded);
    }

    #[test]
    fn certified_tires_resolve_by_designation_and_keep_their_data_book_rating() {
        let b747 = tire_by_class("H49x19.0-22/32PR").expect("certified tire");
        // Goodyear Data Book 2022 Sec. 4A: 56,600 lb rated load.
        assert!((b747.rated_load_kg - 56_600.0 * 0.453_592_37).abs() < 1e-9);
        assert_eq!((b747.ply_rating, b747.rated_inflation_psi), (32, 205.0));
        for tire in CERTIFIED_TIRES {
            assert_eq!(tire_by_class(tire.code), Some(tire));
            match tire.rating_basis {
                RatingBasis::Exact => assert!(tire.ply_rating > 0 && tire.rated_speed_mph > 0.0),
                RatingBasis::LowerBound => assert!(tire.source.starts_with("LOWER BOUND")),
                RatingBasis::Class => panic!("{}: a certified tire is not a class", tire.code),
            }
            assert!(tire.source.contains("Data Book") || tire.source.contains("Engineering Data"));
        }
        assert!(tire_by_class("no such tire").is_none());
    }

    #[test]
    fn the_configuration_accepts_exactly_the_certified_designations() {
        let codes: Vec<&str> = CERTIFIED_TIRES.iter().map(|tire| tire.code).collect();
        assert_eq!(codes, alas_config::landing_gear::CERTIFIED_TIRE_CODES);
    }

    #[test]
    fn auto_selection_never_returns_a_certified_tire() {
        for kg in [1_000.0, 10_000.0, 20_000.0, 25_000.0, 33_000.0, 80_000.0] {
            let tire = select_tire(kg, "auto");
            assert!(CERTIFIED_TIRES
                .iter()
                .all(|certified| certified.code != tire.code));
            assert_eq!(tire.rating_basis, RatingBasis::Class);
        }
        assert!(TIRE_DATABASE.iter().all(|tire| tire.ply_rating == 0));
    }

    #[test]
    fn size_bogie_reports_no_overload_when_the_ladder_covers_the_load() {
        let (n, tire, overloaded) = size_bogie(30_000.0, 1.07, "auto", 0);
        assert_eq!(n, 2);
        assert_eq!(tire.code, "narrowbody");
        assert!(!overloaded);
    }
}

/// Tire loads over the tire ratings, with no design factor applied: what
/// separates a tire that lacks the design margin from one that carries more
/// than it is rated for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TireLoadRatios {
    /// Static load per nose wheel over the rated load. Above 1.0 the nose
    /// tire carries more than its rated static load.
    pub nose_static: f64,
    /// Braking load per nose wheel over the dynamic rating (the rated load
    /// times `tire_dynamic_rating_factor`). Above 1.0 the nose tire is
    /// overloaded in braking.
    pub nose_dynamic: f64,
    /// Worst static load per main wheel over the rated load.
    pub main_static: f64,
}

/// The nose-tire part of [`TireLoadRatios`] (`main_static` is filled by the
/// caller from the main-gear sizing loop).
pub fn nose_load_ratios(
    static_per_wheel_kg: f64,
    dynamic_per_wheel_kg: f64,
    tire: &TireSpec,
    dynamic_rating_factor: f64,
) -> TireLoadRatios {
    let rated = tire.rated_load_kg.max(1e-9);
    TireLoadRatios {
        nose_static: static_per_wheel_kg / rated,
        nose_dynamic: dynamic_per_wheel_kg
            / (dynamic_factor(Some(tire), dynamic_rating_factor) * rated).max(1e-9),
        main_static: 0.0,
    }
}

/// The dynamic (braking) rating factor: the rule factor, or the tire's own
/// printed braking load over its rated load where that is lower. A printed
/// ratio of 1.5 is a lb-to-kg quotient that may round a last bit below the
/// rule factor, so only a ratio lower by more than 1e-9 replaces it (the
/// printed 1.45 rows do; the 1.50 rows keep the rule factor bit for bit).
fn dynamic_factor(tire: Option<&TireSpec>, rule_factor: f64) -> f64 {
    tire.and_then(|t| t.max_braking_load_kg.map(|b| b / t.rated_load_kg.max(1e-9)))
        .filter(|own| *own < rule_factor - 1e-9)
        .unwrap_or(rule_factor)
        .max(1e-9)
}

/// The nose tire and its design load per wheel: the larger of the static load
/// times the design safety factor and the braking load over the dynamic factor
/// (a certified tire with a printed braking load is checked against that load;
/// a certified tire is declared, so its selection ignores the load).
pub fn nose_tire_and_load(
    static_per_wheel_kg: f64,
    dynamic_per_wheel_kg: f64,
    gear: &LandingGearConfig,
) -> (TireSpec, f64) {
    let load = |tire: Option<&TireSpec>| {
        (static_per_wheel_kg * gear.tire_safety_factor)
            .max(dynamic_per_wheel_kg / dynamic_factor(tire, gear.tire_dynamic_rating_factor))
    };
    let tire = nose_tire(load(None), gear);
    (tire, load(Some(&tire)))
}

/// Rated load over design load; unbounded when nothing is carried.
pub fn margin(rated_load_kg: f64, design_load_kg: f64) -> f64 {
    if design_load_kg > 0.0 {
        rated_load_kg / design_load_kg
    } else {
        f64::INFINITY
    }
}

impl TireLoadRatios {
    /// Fold one main-gear wheel's static load into the worst main ratio.
    pub fn note_main(&mut self, per_wheel_load_kg: f64, tire: &TireSpec) {
        self.main_static = self
            .main_static
            .max(per_wheel_load_kg / tire.rated_load_kg.max(1e-9));
    }
}
