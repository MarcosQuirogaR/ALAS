// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sized-gear result types, split out of `landing_gear::mod` to keep
//! room for the sizing entry points (see `docs/source-size-budgets.tsv`).

use alas_config::EffectiveMainGearStation;

use super::tires::{TireLoadRatios, TireSpec};

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
    /// retained for report interfaces. Use
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
    /// Most-aft main-gear axle station across all struts, m.
    ///
    /// The tip-back pivot: `alas_config::rotation_pivot_station(main_gear_x_m)`.
    pub x_mlg_aft_axle_m: f64,
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
    ///
    /// Two-point static reaction at the forward CG limit and the design
    /// weight (`max(MRW, MTOW)` when a maximum ramp weight is
    /// supplied). Does not include the dynamic braking term; see
    /// [`Self::r_nlg_dynamic_kg`].
    pub r_nlg_design_kg: f64,
    /// Design (worst-case) total main-gear static reaction load, kg.
    ///
    /// Evaluated at the design weight and the more-aft of the aerodynamic
    /// aft CG limit (pulled back to the station where the nose gear falls to
    /// the minimum static share passed to the sizing call) and any supplied
    /// most-aft loading-state CG, which is never pulled forward. Without a
    /// loading state behind that station it never exceeds `(1 - f_min) W`;
    /// it never exceeds `W`.
    pub r_mlg_total_design_kg: f64,
    /// Nose-gear static reaction at the design aft CG, kg (never negative);
    /// `W - r_mlg_total_design_kg` unless the total was clamped at zero.
    pub r_nlg_aft_design_kg: f64,
    /// Nose-gear reaction of the raw two-point moment balance at the design
    /// aft CG, before the minimum static share is applied, kg. Negative when
    /// that CG lies behind the main-gear station: the aerodynamic aft limit
    /// is then not a ground-admissible CG: the aerodynamic limit (a
    /// neutral-point defect) or, when the takeoff state is further aft, the
    /// loading CG.
    pub r_nlg_aft_raw_kg: f64,
    /// True when the aerodynamic aft limit lay behind the minimum-nose-load
    /// station and was pulled back to it, so the main reaction and tip-back
    /// are evaluated at that station (or at a loading state behind it, which
    /// is never pulled forward) instead of at the unphysical raw limit.
    pub aero_aft_limit_pulled_back: bool,
    /// Dynamic nose-gear braking reaction, kg (CS/14 CFR 25.733).
    ///
    /// `N_dyn = W (l_m + (a/g) h_cg) / B`; see
    /// `super::geometry::dynamic_nose_braking_load_kg`. This is the load the
    /// nose tire's *dynamic* rating (`tire_dynamic_rating_factor` times
    /// static) is checked against, in addition to the margined static
    /// reaction.
    pub r_nlg_dynamic_kg: f64,

    /// Nose-gear strength limit as a fraction of MTOW.
    pub pct_load_nlg_max: f64,
    /// Main-gear strength limit as a fraction of MTOW.
    pub pct_load_mlg_max: f64,

    /// Margin of the selected nose tire's rated capacity over its governing
    /// (static-with-margin vs. dynamic-with-rating-factor) design load per
    /// wheel: `capacity / design_load`. A value `< 1.0` means the
    /// tire is under-rated for the load it was given (`tire_overloaded`).
    pub nlg_tire_margin: f64,
    /// Margin of the selected main tire's rated capacity over its per-wheel
    /// design load: `capacity / design_load`.
    pub mlg_tire_margin: f64,
    /// Whether the nose or the main tire's rated capacity falls short of the
    /// load it was actually given, on *any* sizing path, including a forced
    /// wheel count. Never hidden: a caller must not need to recompute
    /// `nlg_tire_margin`/`mlg_tire_margin` to notice an under-rated tire.
    pub tire_overloaded: bool,
    /// Tire loads over the tire ratings without the design factor: the nose
    /// static and braking ratios and the worst main static ratio. A ratio
    /// above 1.0 is a real overload, whereas `tire_overloaded` also fires
    /// when only the design margin is missing.
    pub tire_load_ratios: TireLoadRatios,
    /// Whether every main-gear bogie wheel count was declared (forced) by
    /// the configuration rather than auto-sized from the aerodynamic
    /// envelope.
    ///
    /// Gear-strength boundaries derived from an auto-sized layout are
    /// tautological (the tire was sized to exactly cover the same loads the
    /// strength check re-evaluates) and add no independent information; they
    /// are only a meaningful adequacy check when the wheel count is a
    /// declared, published configuration this field marks `true`.
    pub capacity_basis_declared: bool,

    /// Lateral turnover angle from vertical, degrees.
    pub turnover_angle_deg: f64,
    /// Whether the turnover angle is within the configured limit.
    pub turnover_ok: bool,

    /// Longitudinal tip-back angle from vertical, degrees.
    ///
    /// `atan((x_mlg_aft_axle - x_cg_aft) / h_cg)` at the most aft CG the
    /// aircraft reaches on the ground: the design CG, or the supplied
    /// reachable ground CG (operational loading extreme, or the model's
    /// worst aft limit when no envelope exists) when it is further aft; see
    /// `super::geometry::TipBackBasis` and `super::geometry::tip_back_angle_deg`.
    pub tip_back_angle_deg: f64,
    /// The angle `tip_back_ok` is judged against, degrees:
    /// `max(min_tip_back_deg, tail-scrape angle)`.
    pub required_tip_back_deg: f64,
    /// Whether `tip_back_angle_deg >= required_tip_back_deg`, i.e. the angle
    /// clears `max(min_tip_back_deg, tail-scrape angle)`. The scrape angle is
    /// the one supplied in `TipBackBasis` (none: the fixed floor only). The
    /// definition of the required angle and of the tip-back limit station is
    /// shared with the CG envelope (`geometry::required_tip_back_deg`,
    /// `geometry::tip_back_limit_x_m`), so the two cannot disagree.
    pub tip_back_ok: bool,
}

/// Every wheel at its station, for the planform figure: the nose wheels in a
/// row across the centreline, each main-gear strut's bogie about its strut
/// station (`main_gear_x_m[i]`, lateral centre from `strut_positions`).
pub(super) fn planform_wheels(
    x_nlg: f64,
    n_nlg_wheels: i64,
    nlg_tire: &TireSpec,
    strut_positions: Vec<(String, f64)>,
    main_gear_x_m: &[f64],
    mlg_wheels_per_strut: &[i64],
    mlg_tires: &[TireSpec],
) -> Vec<Wheel> {
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

    for (index, ((label, y_center), (&wheels_per_strut, tire))) in strut_positions
        .into_iter()
        .zip(mlg_wheels_per_strut.iter().zip(mlg_tires))
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
    wheels
}
