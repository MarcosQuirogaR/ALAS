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

use alas_config::{EffectiveGearStationExt, EffectiveMainGearStation, LandingGearConfig};

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
    ///
    /// For a heterogeneous arrangement this is the largest per-strut count,
    /// retained for the existing report and parity interfaces. Use
    /// `mlg_wheels_per_strut` for the physical count on each leg.
    pub wheels_per_mlg_strut: i64,
    /// Physical wheel count for each main-gear strut, in layout order.
    ///
    /// The order is left wing, right wing, then centreline/body units. An
    /// entry is emitted for every generated main-gear leg, including the
    /// automatic two- or four-leg layouts.
    pub mlg_wheels_per_strut: Vec<i64>,
    /// The selected nose-gear tire.
    pub nlg_tire: TireSpec,
    /// The selected main-gear tire.
    pub mlg_tire: TireSpec,
    /// The strut material label.
    pub strut_material: String,

    /// Nose-gear longitudinal station, m.
    pub x_nlg: f64,
    /// Primary main-gear longitudinal station (e.g. wing gear), m.
    pub x_mlg: f64,
    /// Effective main-gear longitudinal station (wheel-weighted centroid), m.
    ///
    /// On a rejected `mlg_strut_bogie_wheels` declaration this holds the
    /// primary station fallback (see [`Self::effective_gear_station`] for
    /// the rejection reason); it is never a fabricated value.
    pub effective_x_mlg_m: f64,
    /// The typed resolution outcome behind [`Self::effective_x_mlg_m`].
    ///
    /// `Err` means the configured `mlg_strut_bogie_wheels` was malformed and
    /// the effective station above is the unweighted primary-strut fallback,
    /// not a wheel-weighted centroid. Check this before trusting
    /// `effective_x_mlg_m` as a weighted result.
    pub effective_gear_station: EffectiveMainGearStation,
    /// Longitudinal station for each main-gear strut, in layout order.
    ///
    /// The first entry is the primary wing-main-gear station retained by
    /// `x_mlg` and `wheelbase_m`; additional entries preserve distinct
    /// centreline/body gear stations from a source drawing.
    pub main_gear_x_m: Vec<f64>,
    /// Lateral track width, m.
    pub track_width_m: f64,
    /// Longitudinal wheelbase to primary main-gear station (wing gear), m.
    ///
    /// Retained for comparison against published manufacturer reference
    /// wheelbase baselines (e.g. 28.61 m for A380).
    pub wheelbase_m: f64,
    /// Effective longitudinal wheelbase to the wheel-weighted main-gear centroid, m.
    ///
    /// Used for two-point static reaction equilibrium and steering authority
    /// verification across multi-bogie gear layouts.
    pub effective_wheelbase_m: f64,
    /// Published wheelbase retained for comparison, when the source defines
    /// one. It never moves `x_nlg` or `x_mlg`.
    pub reference_wheelbase_m: Option<f64>,
    /// Published track retained with the source definition, when supplied.
    pub reference_track_m: Option<f64>,
    /// Published nose-gear to body-main-gear wheelbase, when supplied.
    pub reference_body_wheelbase_m: Option<f64>,
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

/// Return one lateral centre for each main-gear leg.
///
/// Two-leg layouts use the wing gear positions. A three-leg layout is the
/// transport arrangement used by the A340: two wing units plus one
/// fuselage-centreline unit. Four-leg layouts add the two body units. The
/// fallback for a larger explicit count keeps every configured leg visible,
/// but it does not invent a new aircraft-specific track definition.
fn mlg_strut_positions(n_mlg_struts: i64, half_track: f64) -> Vec<(String, f64)> {
    let count = n_mlg_struts.max(2) as usize;
    let mut positions = Vec::with_capacity(count);
    positions.push(("L".to_owned(), -half_track));
    positions.push(("R".to_owned(), half_track));

    if count == 3 {
        positions.push(("Body-C".to_owned(), 0.0));
    } else if count >= 4 {
        let body_offset = half_track * 0.45;
        positions.push(("Body-L".to_owned(), -body_offset));
        positions.push(("Body-R".to_owned(), body_offset));
        for index in 4..count {
            // No public generic convention establishes a fifth or later
            // leg's station. Keep it on the centreline and expose it as an
            // explicit estimated position instead of dropping its geometry.
            positions.push((format!("Body-{index}"), 0.0));
        }
    }
    positions
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
    size_landing_gear_with_group_stations(
        mtow_kg,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fuselage_diameter_m,
        cg_height_estimate_m,
        std::slice::from_ref(&x_mlg),
        gear_config,
    )
}

/// Size the landing gear while retaining one longitudinal station per main
/// gear strut.
///
/// `main_gear_x_m` is ordered left wing, right wing, then centreline/body
/// units. If its length does not match the resolved strut count, the scalar
/// `x_mlg` is repeated for every leg, preserving the clean-sheet and legacy
/// behavior of [`size_landing_gear`]. Static reactions remain the existing
/// two-point approximation at the primary (first) main-gear station; the
/// additional positions describe geometry and do not silently calibrate mass
/// or loads.
#[allow(clippy::too_many_arguments)]
pub fn size_landing_gear_with_group_stations(
    mtow_kg: f64,
    x_nlg: f64,
    x_mlg: f64,
    aero_fwd_lim_x: f64,
    aero_aft_lim_x: f64,
    fuselage_diameter_m: f64,
    cg_height_estimate_m: f64,
    requested_main_gear_x_m: &[f64],
    gear_config: &LandingGearConfig,
) -> LandingGearLayout {
    // Resolve the number of legs before accepting group stations, because an
    // automatic design can choose two or four legs from MTOW.
    let n_mlg_struts = if gear_config.n_mlg_struts != 0 {
        gear_config.n_mlg_struts
    } else if mtow_kg >= gear_config.mlg_body_gear_mtow_kg {
        4
    } else {
        2
    }
    .max(2);
    let main_gear_x_m = if requested_main_gear_x_m.len() == n_mlg_struts as usize
        && requested_main_gear_x_m
            .iter()
            .all(|value| value.is_finite())
    {
        requested_main_gear_x_m.to_vec()
    } else {
        vec![x_mlg; n_mlg_struts as usize]
    };
    let x_mlg_primary = main_gear_x_m[0];
    let effective_gear_station = alas_config::effective_main_gear_station(
        &main_gear_x_m,
        gear_config.mlg_strut_bogie_wheels.as_deref(),
    );
    // A rejected explicit bogie declaration must not silently vanish into
    // the reaction arithmetic below (see gear-integration-review.md F2): the
    // typed outcome is retained on the layout via `effective_gear_station`
    // and only its acknowledged scalar fallback is used here.
    let x_mlg_effective = effective_gear_station.primary_station_ignoring_rejection();
    let primary_wheelbase = (x_mlg_primary - x_nlg).max(0.5);
    let effective_wheelbase = (x_mlg_effective - x_nlg).max(0.5);

    // Two-point static reaction: R_nlg = W*(x_mlg_effective - x_cg)/effective_wheelbase. Max NLG
    // load is at the forward CG limit (x_cg small); max total MLG load is at
    // the aft limit (x_cg large -> R_nlg small).
    let r_nlg = |x_cg: f64| mtow_kg * (x_mlg_effective - x_cg) / effective_wheelbase;
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
    // Keep one load reaction per leg for preliminary sizing. A source-backed
    // heterogeneous wheel list then controls each bogie's actual count; an
    // omitted or malformed list falls back to the existing scalar/automatic
    // sizing path so clean-sheet and optimized aircraft remain adaptable.
    let load_per_strut = r_mlg_total_design / n_mlg_struts.max(1) as f64;

    // The source track is a baseline for the active geometry. Scale its
    // centreline spacing with the current fuselage diameter through the
    // configured track/diameter factor; this keeps an optimized or shrunk
    // design adaptable instead of freezing the source aircraft's metres.
    // The provisional value only determines body-leg ordering; the final
    // value below is computed after the bogie sizes are known.
    let half_track_for_layout = fuselage_diameter_m * gear_config.track_diameter_factor / 2.0;
    let provisional_strut_positions = mlg_strut_positions(n_mlg_struts, half_track_for_layout);
    let configured_bogie_counts = gear_config
        .mlg_strut_bogie_wheels
        .as_deref()
        .filter(|counts| {
            counts.len() == provisional_strut_positions.len()
                && counts.iter().all(|count| matches!(count, 2 | 4 | 6))
        });

    let mut mlg_wheels_per_strut = Vec::with_capacity(provisional_strut_positions.len());
    let mut mlg_tires = Vec::with_capacity(provisional_strut_positions.len());
    for index in 0..provisional_strut_positions.len() {
        let forced_count = configured_bogie_counts
            .map(|counts| counts[index])
            .unwrap_or(gear_config.wheels_per_mlg_strut);
        let (wheels, tire) = size_bogie(
            load_per_strut,
            gear_config.tire_safety_factor,
            &gear_config.tire_class,
            forced_count,
        );
        mlg_wheels_per_strut.push(wheels);
        mlg_tires.push(tire);
    }
    let wheels_per_strut = mlg_wheels_per_strut.iter().copied().max().unwrap_or(0);
    let mlg_tire = mlg_tires
        .iter()
        .copied()
        .max_by(|left, right| left.rated_load_kg.total_cmp(&right.rated_load_kg))
        .unwrap_or(NARROWBODY);

    // -- Derived strength limits (fraction of MTOW) ------------------------
    let nlg_capacity_kg = n_nlg_wheels as f64 * nlg_tire.rated_load_kg;
    let mlg_capacity_kg: f64 = mlg_wheels_per_strut
        .iter()
        .zip(&mlg_tires)
        .map(|(&wheels, tire)| wheels as f64 * tire.rated_load_kg)
        .sum();
    let pct_load_nlg_max = nlg_capacity_kg / mtow_kg.max(1.0);
    let pct_load_mlg_max = mlg_capacity_kg / mtow_kg.max(1.0);

    // -- Strut material label ----------------------------------------------
    let strut_material = if gear_config.strut_material != "auto" {
        gear_config.strut_material.clone()
    } else {
        strut_material_for(mlg_tire.code).to_owned()
    };

    // -- Lateral track width ------------------------------------------------
    // A published track is a source baseline whose definition is already a
    // centreline-to-centreline dimension (A380's 14.34 m is specifically the
    // wing-gear track). The preset's track_diameter_factor is the source
    // ratio, so the active design scales naturally with its fuselage
    // diameter. Do not add the automatic bogie-footprint allowance when a
    // source baseline is present. With no reference, retain the existing
    // automatic sizing convention.
    let has_reference_track = gear_config
        .reference_track_m
        .is_some_and(|value| value.is_finite() && value > 0.0);
    let track_width_m = if has_reference_track {
        fuselage_diameter_m * gear_config.track_diameter_factor
    } else {
        fuselage_diameter_m * gear_config.track_diameter_factor + wheels_per_strut as f64 * 0.05
    };

    // -- Lateral turnover angle (Raymer Ch.11 / Currey overturn criterion).
    // The tip-over axis runs from the nose-gear contact to a main-gear
    // contact; in plan view it makes angle delta with the centreline. The
    // lateral lever arm is l_n*sin(delta), smallest at the forward CG limit.
    // The overturn angle is measured from vertical (tan(theta) = h_cg /
    // lever), so a higher CG, a narrower track or a more forward CG all raise
    // theta toward the tip-over limit.
    let half_track = track_width_m / 2.0;
    let delta = half_track.atan2(primary_wheelbase);
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

    let strut_sides = mlg_strut_positions(n_mlg_struts, half_track);
    for (index, ((label, y_center), (&wheels_per_strut, tire))) in strut_sides
        .into_iter()
        .zip(mlg_wheels_per_strut.iter().zip(&mlg_tires))
        .enumerate()
    {
        let bogie_spacing = tire.width_m * 1.6;
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
            let x = main_gear_x_m[index]
                + (row as f64 - ((wheels_per_strut as f64 / 2.0).ceil() - 1.0) / 2.0)
                    * (tire.diameter_m * 1.3);
            wheels.push(Wheel {
                x,
                y,
                group: "MLG",
                strut_label: format!("MLG-{label}"),
                diameter_m: tire.diameter_m,
                width_m: tire.width_m,
            });
        }
    }

    LandingGearLayout {
        n_nlg_wheels,
        n_mlg_struts,
        wheels_per_mlg_strut: wheels_per_strut,
        mlg_wheels_per_strut,
        nlg_tire,
        mlg_tire,
        strut_material,
        x_nlg,
        x_mlg: x_mlg_primary,
        effective_x_mlg_m: x_mlg_effective,
        effective_gear_station,
        main_gear_x_m,
        track_width_m,
        wheelbase_m: primary_wheelbase,
        effective_wheelbase_m: effective_wheelbase,
        reference_wheelbase_m: gear_config.reference_wheelbase_m,
        reference_track_m: gear_config.reference_track_m,
        reference_body_wheelbase_m: gear_config.reference_body_wheelbase_m,
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

    #[test]
    fn a_three_leg_arrangement_keeps_the_centreline_bogie_and_each_count() {
        let config = LandingGearConfig {
            n_mlg_struts: 3,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            track_diameter_factor: 10.684 / 5.64,
            reference_track_m: Some(10.684),
            reference_wheelbase_m: Some(25.375),
            ..Default::default()
        };
        let layout = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);

        assert_eq!(layout.n_mlg_struts, 3);
        assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 2]);
        assert_eq!(
            layout
                .wheels
                .iter()
                .filter(|wheel| wheel.group == "MLG")
                .count(),
            10
        );
        assert_eq!(layout.track_width_m, 10.684);
        // The source wheelbase is metadata. Model-derived stations and the
        // reaction geometry continue to use the caller's x stations.
        assert_eq!(layout.reference_wheelbase_m, Some(25.375));
        assert_eq!(layout.wheelbase_m, 25.0);
        let centreline: Vec<&Wheel> = layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label == "MLG-Body-C")
            .collect();
        assert_eq!(centreline.len(), 2);
        assert!((centreline[0].y + centreline[1].y).abs() < 1e-12);
        assert!(centreline
            .iter()
            .all(|wheel| (wheel.x - 31.0).abs() < 1e-12));
    }

    #[test]
    fn four_leg_source_topology_uses_wing_and_body_bogie_sizes() {
        let config = LandingGearConfig {
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            track_diameter_factor: 14.34 / 7.14,
            reference_track_m: Some(14.34),
            reference_wheelbase_m: Some(28.61),
            reference_body_wheelbase_m: Some(31.88),
            ..Default::default()
        };
        let layout = size_landing_gear(560_000.0, 6.0, 35.0, 8.0, 31.0, 7.14, 8.0, &config);
        assert_eq!(layout.mlg_wheels_per_strut, vec![4, 4, 6, 6]);
        assert_eq!(
            layout
                .wheels
                .iter()
                .filter(|wheel| wheel.group == "MLG")
                .count(),
            20
        );
        assert_eq!(layout.track_width_m, 14.34);
        assert_eq!(layout.reference_wheelbase_m, Some(28.61));
        assert_eq!(layout.reference_body_wheelbase_m, Some(31.88));
        assert_eq!(
            layout
                .wheels
                .iter()
                .filter(|wheel| wheel.strut_label == "MLG-Body-L")
                .count(),
            6
        );
        assert_eq!(
            layout
                .wheels
                .iter()
                .filter(|wheel| wheel.strut_label == "MLG-Body-R")
                .count(),
            6
        );
    }

    #[test]
    fn reference_track_scales_with_active_fuselage_diameter() {
        let config = LandingGearConfig {
            n_mlg_struts: 3,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            track_diameter_factor: 10.684 / 5.64,
            reference_track_m: Some(10.684),
            ..Default::default()
        };

        let baseline = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 5.64, 6.0, &config);
        let resized = size_landing_gear(260_000.0, 6.0, 31.0, 8.0, 27.0, 6.20, 6.0, &config);

        assert!((baseline.track_width_m - 10.684).abs() < 1e-12);
        assert!((resized.track_width_m - 6.20 * (10.684 / 5.64)).abs() < 1e-12);
        assert_eq!(resized.reference_track_m, Some(10.684));
        assert!(resized.track_width_m > baseline.track_width_m);
    }

    #[test]
    fn malformed_bogie_list_falls_back_without_dropping_a_leg() {
        let config = LandingGearConfig {
            n_mlg_struts: 3,
            mlg_strut_bogie_wheels: Some(vec![4, 4]),
            ..Default::default()
        };
        let layout = size_landing_gear(120_000.0, 6.0, 25.0, 8.0, 22.0, 5.0, 5.0, &config);
        assert_eq!(layout.mlg_wheels_per_strut.len(), 3);
        assert_eq!(
            layout
                .wheels
                .iter()
                .filter(|wheel| wheel.strut_label == "MLG-Body-C")
                .count(),
            layout.mlg_wheels_per_strut[2] as usize
        );
    }

    #[test]
    fn group_station_sizing_keeps_each_main_gear_axle_in_its_source_position() {
        let config = LandingGearConfig {
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            track_diameter_factor: 14.34 / 7.14,
            reference_track_m: Some(14.34),
            ..Default::default()
        };
        let main_gear_x_m = [33.58, 33.58, 36.85, 36.85];
        let layout = size_landing_gear_with_group_stations(
            560_000.0,
            4.97,
            33.58,
            20.0,
            30.0,
            7.14,
            8.0,
            &main_gear_x_m,
            &config,
        );
        assert_eq!(layout.main_gear_x_m, main_gear_x_m);
        let body_wheels: Vec<&Wheel> = layout
            .wheels
            .iter()
            .filter(|wheel| wheel.strut_label.starts_with("MLG-Body"))
            .collect();
        let body_wheel_centroid =
            body_wheels.iter().map(|wheel| wheel.x).sum::<f64>() / body_wheels.len() as f64;
        assert!((body_wheel_centroid - 36.85).abs() < 1.0e-12);
        assert!((layout.wheelbase_m - (33.58 - 4.97)).abs() < 1.0e-12);
        assert!((layout.effective_x_mlg_m - 35.542).abs() < 1.0e-12);
        assert!((layout.effective_wheelbase_m - (35.542 - 4.97)).abs() < 1.0e-12);
    }
}
