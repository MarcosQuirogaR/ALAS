// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/landing_gear.py
// Reference: alas @ rust-port-baseline.

//! Landing gear sizing: wheel count, tire selection, position, and the
//! CS-25.147-style lateral turnover check.
//!
//! [`size_landing_gear`] computes the nose- and main-gear static reaction
//! loads from a two-point ground-reaction equation at the aircraft's forward
//! and aft *aerodynamic* centre-of-gravity limits (the gear-independent,
//! stability-derived envelope), then adds enough wheels per strut to carry
//! that load with margin, drawing from [`TIRE_DATABASE`]. The resulting gear
//! capacity is converted back to [`LandingGearLayout::pct_load_nlg_max`] and
//! [`LandingGearLayout::pct_load_mlg_max`] -- the same fractions
//! `MassModelConfig` carries as a fallback -- so a design is only
//! gear-constrained if its real wheel/tire capacity, sized with margin, still
//! falls short of the aerodynamic envelope.
//!
//! There is no circularity: gear sizing consumes the aerodynamic limits as its
//! worst-case loads and returns a strength limit that can only tighten, never
//! widen, that envelope.

use alas_config::LandingGearConfig;

/// A representative transport-category tire class, at conceptual-design
/// fidelity (not a specific certified part number) -- the `TireSpec` records
/// upstream's `TIRE_DATABASE` entries carry.
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
const LIGHT: TireSpec = TireSpec {
    code: "light",
    name: "Light transport (~24x7.7 class)",
    rated_load_kg: 3_500.0,
    diameter_m: 0.61,
    width_m: 0.20,
};
/// Narrowbody (~46x17 class, A320/737).
const NARROWBODY: TireSpec = TireSpec {
    code: "narrowbody",
    name: "Narrowbody (~46x17 class, A320/737)",
    rated_load_kg: 13_000.0,
    diameter_m: 1.17,
    width_m: 0.44,
};
/// Widebody (~52x21 class, 787/A330).
const WIDEBODY: TireSpec = TireSpec {
    code: "widebody",
    name: "Widebody (~52x21 class, 787/A330)",
    rated_load_kg: 24_000.0,
    diameter_m: 1.32,
    width_m: 0.53,
};
/// Heavy widebody (~54x21 class, A380/747).
const HEAVY: TireSpec = TireSpec {
    code: "heavy",
    name: "Heavy widebody (~54x21 class, A380/747)",
    rated_load_kg: 34_000.0,
    diameter_m: 1.40,
    width_m: 0.56,
};

/// The tire classes in ascending capacity -- the order [`select_tire`] walks
/// to auto-select the smallest that covers a load. Mirrors upstream's
/// `_TIRE_ORDER`.
pub const TIRE_DATABASE: [TireSpec; 4] = [LIGHT, NARROWBODY, WIDEBODY, HEAVY];

/// Standard main-gear bogie sizes (wheels per strut) a preliminary design
/// chooses between -- odd counts and anything above 6/strut are not realistic
/// for a twin/quad-leg configuration at this design stage. Upstream's
/// `_STANDARD_BOGIE_SIZES`.
const STANDARD_BOGIE_SIZES: [i64; 3] = [2, 4, 6];

/// The tire class named by `code`, or `None` if it is not one of the four --
/// `TIRE_DATABASE.__contains__`/`__getitem__` on the class key.
fn tire_by_class(code: &str) -> Option<TireSpec> {
    match code {
        "light" => Some(LIGHT),
        "narrowbody" => Some(NARROWBODY),
        "widebody" => Some(WIDEBODY),
        "heavy" => Some(HEAVY),
        _ => None,
    }
}

/// Representative strut material by tire class -- informational/labelling only
/// (this tool runs no structural analysis of the strut). Upstream's
/// `STRUT_MATERIALS.get(code, STRUT_MATERIALS["narrowbody"])`, so an
/// unrecognized code falls back to the narrowbody steel.
fn strut_material_for(code: &str) -> &'static str {
    match code {
        "light" => "7075-T6 aluminium",
        "widebody" => "300M high-strength steel",
        "heavy" => "300M high-strength steel (titanium truck beam)",
        // "narrowbody" and the fallback are the same value.
        _ => "300M high-strength steel",
    }
}

/// One physical wheel, positioned for the planform figure.
#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    /// Longitudinal station, m.
    pub x: f64,
    /// Lateral station, m.
    pub y: f64,
    /// `"NLG"` or `"MLG"`.
    pub group: &'static str,
    /// Strut label, e.g. `"MLG-L"`, `"MLG-Body-L"`, `"NLG"`.
    pub strut_label: String,
    /// Tire outer diameter, m.
    pub diameter_m: f64,
    /// Tire section width, m.
    pub width_m: f64,
}

/// Sized landing gear: wheel counts, positions, and derived load limits.
#[derive(Debug, Clone, PartialEq)]
pub struct LandingGearLayout {
    /// Number of nose-gear wheels.
    pub n_nlg_wheels: i64,
    /// Number of main-gear struts.
    pub n_mlg_struts: i64,
    /// Wheels per main-gear strut (bogie size).
    pub wheels_per_mlg_strut: i64,
    /// The selected nose-gear tire.
    pub nlg_tire: TireSpec,
    /// The selected main-gear tire.
    pub mlg_tire: TireSpec,
    /// The strut material label.
    pub strut_material: String,

    /// Nose-gear longitudinal station, m.
    pub x_nlg: f64,
    /// Main-gear longitudinal station, m.
    pub x_mlg: f64,
    /// Lateral track width, m.
    pub track_width_m: f64,
    /// Longitudinal wheelbase, m.
    pub wheelbase_m: f64,
    /// Every wheel, for the planform figure.
    pub wheels: Vec<Wheel>,

    /// Design (worst-case) nose-gear static reaction load, kg.
    pub r_nlg_design_kg: f64,
    /// Design (worst-case) total main-gear static reaction load, kg.
    pub r_mlg_total_design_kg: f64,

    /// Nose-gear strength limit as a fraction of MTOW.
    pub pct_load_nlg_max: f64,
    /// Main-gear strength limit as a fraction of MTOW.
    pub pct_load_mlg_max: f64,

    /// Lateral turnover angle from vertical, degrees.
    pub turnover_angle_deg: f64,
    /// Whether the turnover angle is within the configured limit.
    pub turnover_ok: bool,
}

/// Pick a tire class: an explicit choice, or the smallest whose rating covers
/// `design_load_per_wheel_kg`. Falls back to the heaviest class (which may
/// still be under-rated) -- `_select_tire`.
fn select_tire(design_load_per_wheel_kg: f64, tire_class: &str) -> TireSpec {
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
/// `strut_load_kg` -- `_size_bogie`.
///
/// The tire is re-selected for each candidate wheel count (the per-wheel load
/// it would actually see) and the same tire is checked against the design load
/// and returned, so the count and the tire are self-consistent. A positive
/// `forced_count` short-circuits the ladder and takes that count directly.
fn size_bogie(
    strut_load_kg: f64,
    safety_factor: f64,
    tire_class: &str,
    forced_count: i64,
) -> (i64, TireSpec) {
    let design_load = strut_load_kg * safety_factor;
    let counts: &[i64] = if forced_count > 0 {
        std::slice::from_ref(&forced_count)
    } else {
        &STANDARD_BOGIE_SIZES
    };
    for &n in counts {
        let tire = select_tire(strut_load_kg / n as f64, tire_class);
        if forced_count > 0 || n as f64 * tire.rated_load_kg >= design_load {
            return (n, tire);
        }
    }
    // Nothing in the standard ladder covers it -- return the largest bogie
    // with the tire sized for its actual per-wheel load (may still be
    // under-rated; a legitimate finding, not silently hidden).
    let n = STANDARD_BOGIE_SIZES[STANDARD_BOGIE_SIZES.len() - 1];
    (n, select_tire(strut_load_kg / n as f64, tire_class))
}

/// Size the landing gear from real static reaction loads at the aerodynamic
/// centre-of-gravity limits -- `size_landing_gear`.
///
/// `x_nlg`/`x_mlg` are the physical fuselage stations of the nose/main gear;
/// `aero_fwd_lim_x`/`aero_aft_lim_x` are the physical stations of the
/// aerodynamic (gear-independent) forward/aft centre-of-gravity limits, the
/// worst-case loading the gear must carry; `cg_height_estimate_m` is the
/// loaded centre-of-gravity height above the ground for the turnover check.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_landing_gear(
    mtow_kg: f64,
    x_nlg: f64,
    x_mlg: f64,
    aero_fwd_lim_x: f64,
    aero_aft_lim_x: f64,
    fuselage_diameter_m: f64,
    cg_height_estimate_m: f64,
    gear_config: &LandingGearConfig,
) -> LandingGearLayout {
    let wheelbase = (x_mlg - x_nlg).max(0.5);

    // Two-point static reaction: R_nlg = W*(x_mlg - x_cg)/wheelbase. Max NLG
    // load is at the forward CG limit (x_cg small); max total MLG load is at
    // the aft limit (x_cg large -> R_nlg small).
    let r_nlg = |x_cg: f64| mtow_kg * (x_mlg - x_cg) / wheelbase;
    let r_nlg_design = r_nlg(aero_fwd_lim_x).max(0.0);
    let r_mlg_total_design = (mtow_kg - r_nlg(aero_aft_lim_x)).max(0.0);

    // -- Nose gear --------------------------------------------------------
    let n_nlg_wheels = if gear_config.n_nlg_wheels != 0 {
        gear_config.n_nlg_wheels
    } else if mtow_kg >= gear_config.nlg_dual_wheel_mtow_kg {
        2
    } else {
        1
    };
    let nlg_load_per_wheel = r_nlg_design / n_nlg_wheels.max(1) as f64;
    let nlg_tire = select_tire(nlg_load_per_wheel, &gear_config.tire_class);

    // -- Main gear --------------------------------------------------------
    let n_mlg_struts = if gear_config.n_mlg_struts != 0 {
        gear_config.n_mlg_struts
    } else if mtow_kg >= gear_config.mlg_body_gear_mtow_kg {
        4
    } else {
        2
    };
    let load_per_strut = r_mlg_total_design / n_mlg_struts.max(1) as f64;
    let (wheels_per_strut, mlg_tire) = size_bogie(
        load_per_strut,
        gear_config.tire_safety_factor,
        &gear_config.tire_class,
        gear_config.wheels_per_mlg_strut,
    );

    // -- Derived strength limits (fraction of MTOW) ------------------------
    let nlg_capacity_kg = n_nlg_wheels as f64 * nlg_tire.rated_load_kg;
    let mlg_capacity_kg = n_mlg_struts as f64 * wheels_per_strut as f64 * mlg_tire.rated_load_kg;
    let pct_load_nlg_max = nlg_capacity_kg / mtow_kg.max(1.0);
    let pct_load_mlg_max = mlg_capacity_kg / mtow_kg.max(1.0);

    // -- Strut material label ----------------------------------------------
    let strut_material = if gear_config.strut_material != "auto" {
        gear_config.strut_material.clone()
    } else {
        strut_material_for(mlg_tire.code).to_owned()
    };

    // -- Lateral track width: main gear outboard of the fuselage, wide enough
    // to clear it with margin (`track_diameter_factor`), plus a small additive
    // allowance for the bogie's own footprint width.
    let track_width_m =
        fuselage_diameter_m * gear_config.track_diameter_factor + wheels_per_strut as f64 * 0.05;

    // -- Lateral turnover angle (Raymer Ch.11 / Currey overturn criterion).
    // The tip-over axis runs from the nose-gear contact to a main-gear
    // contact; in plan view it makes angle delta with the centreline. The
    // lateral lever arm is l_n*sin(delta), smallest at the forward CG limit.
    // The overturn angle is measured from vertical (tan(theta) = h_cg /
    // lever), so a higher CG, a narrower track or a more forward CG all raise
    // theta toward the tip-over limit.
    let half_track = track_width_m / 2.0;
    let delta = half_track.atan2(wheelbase);
    let l_n_fwd = (aero_fwd_lim_x - x_nlg).max(0.1);
    let lever = (l_n_fwd * delta.sin()).max(1e-3);
    let turnover_angle_deg = cg_height_estimate_m.max(0.1).atan2(lever).to_degrees();
    let turnover_ok = turnover_angle_deg <= gear_config.turnover_angle_limit_deg;

    // -- Wheel positions for the planform figure -----------------------------
    let mut wheels: Vec<Wheel> = Vec::new();
    let nlg_spacing = nlg_tire.width_m * 1.6;
    for i in 0..n_nlg_wheels {
        let y = (i as f64 - (n_nlg_wheels - 1) as f64 / 2.0) * nlg_spacing;
        wheels.push(Wheel {
            x: x_nlg,
            y,
            group: "NLG",
            strut_label: "NLG".to_owned(),
            diameter_m: nlg_tire.diameter_m,
            width_m: nlg_tire.width_m,
        });
    }

    let mut strut_sides: Vec<(&str, f64)> = vec![("L", -half_track), ("R", half_track)];
    if n_mlg_struts >= 4 {
        let body_offset = half_track * 0.45;
        strut_sides.push(("Body-L", -body_offset));
        strut_sides.push(("Body-R", body_offset));
    }
    strut_sides.truncate(n_mlg_struts.max(2) as usize);

    let bogie_spacing = mlg_tire.width_m * 1.6;
    for (label, y_center) in strut_sides {
        for i in 0..wheels_per_strut {
            // Even wheel counts pair up fore/aft in a bogie; odd (which
            // STANDARD_BOGIE_SIZES never yields) centres the extra wheel.
            let row = if wheels_per_strut > 1 { i / 2 } else { 0 };
            let side: f64 = if i % 2 == 0 { -1.0 } else { 1.0 };
            let y = if wheels_per_strut > 1 {
                y_center + side * bogie_spacing / 2.0
            } else {
                y_center
            };
            let x = x_mlg
                + (row as f64 - ((wheels_per_strut as f64 / 2.0).ceil() - 1.0) / 2.0)
                    * (mlg_tire.diameter_m * 1.3);
            wheels.push(Wheel {
                x,
                y,
                group: "MLG",
                strut_label: format!("MLG-{label}"),
                diameter_m: mlg_tire.diameter_m,
                width_m: mlg_tire.width_m,
            });
        }
    }

    LandingGearLayout {
        n_nlg_wheels,
        n_mlg_struts,
        wheels_per_mlg_strut: wheels_per_strut,
        nlg_tire,
        mlg_tire,
        strut_material,
        x_nlg,
        x_mlg,
        track_width_m,
        wheelbase_m: wheelbase,
        wheels,
        r_nlg_design_kg: r_nlg_design,
        r_mlg_total_design_kg: r_mlg_total_design,
        pct_load_nlg_max,
        pct_load_mlg_max,
        turnover_angle_deg,
        turnover_ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_tire_honours_an_explicit_class_even_when_it_is_under_rated() {
        // An explicit "light" is returned though the load far exceeds its
        // rating -- the explicit branch does not fall through to a bigger tire.
        let tire = select_tire(1_000_000.0, "light");
        assert_eq!(tire.code, "light");
    }

    #[test]
    fn select_tire_auto_picks_the_smallest_class_that_covers_the_load() {
        assert_eq!(select_tire(2_000.0, "auto").code, "light");
        assert_eq!(select_tire(10_000.0, "auto").code, "narrowbody");
        assert_eq!(select_tire(20_000.0, "auto").code, "widebody");
        assert_eq!(select_tire(30_000.0, "auto").code, "heavy");
        // Above every rating, the heaviest class is returned.
        assert_eq!(select_tire(50_000.0, "auto").code, "heavy");
    }

    #[test]
    fn size_bogie_forced_count_takes_that_count_regardless_of_capacity() {
        let (n, tire) = size_bogie(200_000.0, 1.07, "auto", 6);
        assert_eq!(n, 6);
        // The tire is sized for the forced per-wheel load, not re-derived from
        // the standard ladder.
        assert_eq!(tire, select_tire(200_000.0 / 6.0, "auto"));
    }

    #[test]
    fn a_wider_track_lowers_the_turnover_angle() {
        let narrow = LandingGearConfig {
            track_diameter_factor: 1.0,
            ..Default::default()
        };
        let wide = LandingGearConfig {
            track_diameter_factor: 3.0,
            ..Default::default()
        };

        let common = |cfg: &LandingGearConfig| {
            size_landing_gear(120_000.0, 6.0, 12.0, 7.0, 11.0, 6.0, 6.0, cfg).turnover_angle_deg
        };
        assert!(common(&wide) < common(&narrow));
    }
}
