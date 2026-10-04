// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sized-gear result types, split out of `landing_gear::mod` to keep
//! room for the sizing entry points (see `docs/source-size-budgets.tsv`).

use alas_config::EffectiveMainGearStation;

use super::tires::TireSpec;

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
    /// The tip-back pivot: `max(main_gear_x_m)`.
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
    /// aft CG limit and any supplied most-aft loading-state CG.
    pub r_mlg_total_design_kg: f64,
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
    /// `atan((x_mlg_aft_axle - x_cg_aft) / h_cg)`, evaluated at the same
    /// aft CG limit and CG height used for the static/dynamic reactions
    /// above; see `super::geometry::tip_back_angle_deg`.
    pub tip_back_angle_deg: f64,
    /// Whether the tip-back angle clears the configured minimum
    /// (`min_tip_back_deg`).
    ///
    /// This checks only the fixed floor: the full requirement is
    /// `tip_back_angle_deg >= max(min_tip_back_deg, tail_scrape_angle_deg)`,
    /// and the scrape angle needs fuselage lower-contour geometry this
    /// module does not hold. A caller with that geometry should combine it
    /// with `super::geometry::tail_scrape_angle_deg` directly rather than
    /// trust this flag alone as the complete gate.
    pub tip_back_ok: bool,
}
