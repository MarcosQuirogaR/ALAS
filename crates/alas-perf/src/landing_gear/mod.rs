// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Landing gear sizing: wheel count, tire selection, position, and the
//! CS-25.147-style lateral turnover check.
//!
//! [`size_landing_gear`] computes the nose- and main-gear static reaction
//! loads from a two-point ground-reaction equation at the aircraft's forward
//! and aft *aerodynamic* centre-of-gravity limits (the gear-independent,
//! stability-derived envelope), then adds enough wheels per strut to carry
//! that load with margin, drawing from [`tires::TIRE_DATABASE`]. The
//! resulting gear capacity is converted back to
//! [`LandingGearLayout::pct_load_nlg_max`] and
//! [`LandingGearLayout::pct_load_mlg_max`]: the same fractions
//! `MassModelConfig` carries as a fallback, so a design is only
//! gear-constrained if its real wheel/tire capacity, sized with margin, still
//! falls short of the aerodynamic envelope.
//!
//! There is no circularity for a *declared* (forced wheel count) layout: the
//! strength check tightens the aerodynamic envelope. For an *auto-sized*
//! layout the tire count/rating is derived from the same envelope it would
//! be checked against, so the strength check cannot bind inside it
//! (`capacity_basis_declared` records which case applies).
//!
//! [`geometry`] adds the tip-back, tail-scrape and dynamic nose-braking
//! checks.

pub mod geometry;
mod layout;
pub mod placement;
pub mod tires;

use alas_config::{EffectiveGearStationExt, LandingGearConfig};

pub use layout::{LandingGearLayout, Wheel};
pub use tires::{TireSpec, TIRE_DATABASE};

/// Return one lateral centre for each main-gear leg.
///
/// Two-leg layouts use the wing gear positions. A three-leg layout is the
/// transport arrangement used by the A340: two wing units plus one
/// fuselage-centreline unit. Four-leg layouts add the two body units.
///
/// Five or more legs are tandem legs in line on each side at the full track
/// (the A400M's three twin-wheel legs per side, EASA TCDS A.169 and the Airbus
/// A400M brochure plan view), `L`, `R`, then `L2`, `R2`, and so on, with an
/// odd last leg on the centreline. The layout is mirrored about the
/// centreline for every count. `main_gear_x_m` follows this same order, so a
/// left and right leg of one row take consecutive entries.
///
/// Four legs of one wheel each (`wheels_per_leg == 1`) are also tandem legs in
/// line at the full track, not wing and body units: the CN-235 and C-295 main
/// gear, "two legs in tandem configuration on each side of the fuselage", each
/// with its own shock absorber and one wheel (EADS-CASA, CN-235-300 Technical
/// Review, 2002, Landing Gear, p. 26). A body unit always carries a bogie.
pub fn mlg_strut_positions(
    n_mlg_struts: i64,
    half_track: f64,
    wheels_per_leg: i64,
) -> Vec<(String, f64)> {
    let count = n_mlg_struts.max(2) as usize;
    let mut positions = Vec::with_capacity(count);
    positions.push(("L".to_owned(), -half_track));
    positions.push(("R".to_owned(), half_track));

    if count == 3 {
        positions.push(("Body-C".to_owned(), 0.0));
    } else if count == 4 && wheels_per_leg != 1 {
        let body_offset = half_track * 0.45;
        positions.push(("Body-L".to_owned(), -body_offset));
        positions.push(("Body-R".to_owned(), body_offset));
    } else if count >= 4 {
        for row in 2..=count / 2 {
            positions.push((format!("L{row}"), -half_track));
            positions.push((format!("R{row}"), half_track));
        }
        if count % 2 == 1 {
            positions.push(("Body-C".to_owned(), 0.0));
        }
    }
    positions
}

/// Size the landing gear from real static reaction loads at the aerodynamic
/// centre-of-gravity limits: `size_landing_gear`.
///
/// `x_nlg`/`x_mlg` are the physical fuselage stations of the nose/main gear;
/// `aero_fwd_lim_x`/`aero_aft_lim_x` are the physical stations of the
/// aerodynamic (gear-independent) forward/aft centre-of-gravity limits, the
/// worst-case loading the gear must carry; `cg_height_estimate_m` is the
/// loaded centre-of-gravity height above the ground for the turnover check.
///
/// Thin wrapper over [`size_landing_gear_at_design_state`] with no maximum
/// ramp weight and no most-aft loading-state override; see that function to
/// size against those as well.
#[allow(clippy::too_many_arguments)] // one argument per physical input
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
        0.0,
        gear_config,
    )
}

/// Size the landing gear while retaining one longitudinal station per main
/// gear strut.
///
/// `main_gear_x_m` is ordered left wing, right wing, then centreline/body
/// units. If its length does not match the resolved strut count, the scalar
/// `x_mlg` is repeated for every leg, as in [`size_landing_gear`]. Static
/// reactions use the two-point approximation at the primary (first)
/// main-gear station; the
/// additional positions describe geometry and do not silently calibrate mass
/// or loads.
///
/// Thin wrapper over [`size_landing_gear_at_design_state`] with no maximum
/// ramp weight and no most-aft loading-state override.
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
    min_static_nose_fraction: f64,
    gear_config: &LandingGearConfig,
) -> LandingGearLayout {
    size_landing_gear_at_design_state(
        mtow_kg,
        None,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        None,
        min_static_nose_fraction,
        geometry::TipBackBasis::default(),
        fuselage_diameter_m,
        cg_height_estimate_m,
        requested_main_gear_x_m,
        gear_config,
    )
}

/// Size the landing gear at the governing design state: the more-aft
/// of the aerodynamic aft CG limit and a supplied most-aft loading-state CG,
/// evaluated at `max(mrw_kg, mtow_kg)` when a maximum ramp weight is given.
///
/// `mrw_kg`: the maximum ramp (taxi) weight, when registered; CS 25.733 /
/// Currey size the main gear at MRW, not MTOW, because taxi/turn loads are
/// carried before any fuel is burned for takeoff. `None` (or a value that
/// does not exceed `mtow_kg`) sizes at MTOW alone.
///
/// `most_aft_state_x`: the CG station of the most-aft *loading state* the
/// caller has evaluated (e.g. the analyzed takeoff or a full-aft-cargo
/// state), if more aft than the aerodynamic aft limit. `None` reproduces the
/// sizes at the aerodynamic aft limit alone.
#[allow(clippy::too_many_arguments)]
pub fn size_landing_gear_at_design_state(
    mtow_kg: f64,
    mrw_kg: Option<f64>,
    x_nlg: f64,
    x_mlg: f64,
    aero_fwd_lim_x: f64,
    aero_aft_lim_x: f64,
    most_aft_state_x: Option<f64>,
    min_static_nose_fraction: f64,
    tip_back: geometry::TipBackBasis,
    fuselage_diameter_m: f64,
    cg_height_estimate_m: f64,
    requested_main_gear_x_m: &[f64],
    gear_config: &LandingGearConfig,
) -> LandingGearLayout {
    // Main-gear (and nose-gear) design weight is the more severe of
    // MTOW and a registered maximum ramp weight; the aft design CG is the
    // more-aft of the aerodynamic limit and any supplied most-aft state.
    let design_weight_kg = mrw_kg
        .filter(|value| value.is_finite() && *value > 0.0)
        .map_or(mtow_kg, |mrw| mrw.max(mtow_kg));
    // Needs data: the nose reaction is sized at the aft limit here, not at the loading-envelope
    // forward CG; the validation source (Airbus AC 7-3-0 Fig 7-3-0-991-006, E2 APM section 7) is not on disk.
    let design_aft_lim_x = most_aft_state_x
        .filter(|value| value.is_finite())
        .map_or(aero_aft_lim_x, |aft_state| aft_state.max(aero_aft_lim_x));

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
    // The tip-back pivot, the one rotation pivot the CG envelope also uses.
    let x_mlg_aft_axle_m = alas_config::rotation_pivot_station(&main_gear_x_m, x_mlg);
    let effective_gear_station = alas_config::effective_main_gear_station(
        &main_gear_x_m,
        gear_config.mlg_strut_bogie_wheels.as_deref(),
    );
    // A rejected explicit bogie declaration must not silently vanish into
    // the reaction arithmetic below: the
    // typed outcome is retained on the layout via `effective_gear_station`
    // and only its acknowledged scalar fallback is used here.
    let x_mlg_effective = effective_gear_station.primary_station_ignoring_rejection();
    let primary_wheelbase = (x_mlg_primary - x_nlg).max(0.5);
    let effective_wheelbase = (x_mlg_effective - x_nlg).max(0.5);

    // Two-point static reaction: R_nlg = W*(x_mlg_effective - x_cg)/effective_wheelbase. Max NLG
    // load is at the forward CG limit (x_cg small) and the design weight
    // (max(MRW, MTOW)); max total MLG load is at the design
    // (most-aft) CG limit (x_cg large -> R_nlg small) and the design weight.
    let r_nlg = |x_cg: f64| design_weight_kg * (x_mlg_effective - x_cg) / effective_wheelbase;
    let r_nlg_design = r_nlg(aero_fwd_lim_x).max(0.0);
    // The main gear and tip-back are judged at the most-aft ground CG: the
    // aerodynamic limit is pulled back to the minimum-nose-load station, and a
    // real loading state is never pulled forward (see `geometry::aft_design_state`).
    let aft = geometry::aft_design_state(
        design_weight_kg,
        x_mlg_effective,
        effective_wheelbase,
        aero_aft_lim_x,
        most_aft_state_x,
        min_static_nose_fraction,
    );
    let (r_nlg_aft_design, r_mlg_total_design) = (aft.nose_kg, aft.main_total_kg);
    let aero_aft_limit_pulled_back = aft.aero_limit_pulled_back;
    let design_aft_cg_x = aft.x_cg_m;
    // Raw moment balance at the unpulled design CG, for the diagnostic.
    let r_nlg_aft_raw = r_nlg(design_aft_lim_x);

    // Dynamic nose-gear braking reaction at the forward CG limit and
    // the design weight, certification deceleration
    // `a = g * nlg_dynamic_braking_decel_g`.
    let l_m_to_main_gear = (x_mlg_effective - aero_fwd_lim_x).max(0.0);
    let r_nlg_dynamic = geometry::dynamic_nose_braking_load_kg(
        design_weight_kg,
        l_m_to_main_gear,
        effective_wheelbase,
        cg_height_estimate_m,
        gear_config.nlg_dynamic_braking_decel_g,
    );

    // Nose gear
    let n_nlg_wheels = if gear_config.n_nlg_wheels != 0 {
        gear_config.n_nlg_wheels
    } else if mtow_kg >= gear_config.nlg_dual_wheel_mtow_kg {
        2
    } else {
        1
    };
    let nlg_static_per_wheel = r_nlg_design / n_nlg_wheels.max(1) as f64;
    let nlg_dynamic_per_wheel = r_nlg_dynamic / n_nlg_wheels.max(1) as f64;
    let (nlg_tire, nlg_design_load_per_wheel) =
        tires::nose_tire_and_load(nlg_static_per_wheel, nlg_dynamic_per_wheel, gear_config);
    let nlg_capacity_per_wheel = nlg_tire.rated_load_kg;
    let nlg_tire_margin = tires::margin(nlg_capacity_per_wheel, nlg_design_load_per_wheel);
    let nlg_overloaded = nlg_capacity_per_wheel < nlg_design_load_per_wheel;
    let mut tire_load_ratios = tires::nose_load_ratios(
        nlg_static_per_wheel,
        nlg_dynamic_per_wheel,
        &nlg_tire,
        gear_config.tire_dynamic_rating_factor,
    );

    // Main gear
    // The reaction point is the wheel-count-weighted centroid, so the
    // load is distributed per wheel (R/N_wheels) and each strut then carries
    // its own physical wheel count's share, rather than an equal-per-strut
    // split that would put 50% more load on a wing-gear wheel than a body-
    // gear wheel on a layout like the A380's [4,4,6,6].
    // The source track is a baseline for the active geometry. Scale its
    // centreline spacing with the current fuselage diameter through the
    // configured track/diameter factor; this keeps an optimized or shrunk
    // design adaptable instead of freezing the source aircraft's metres.
    // The provisional value only determines body-leg ordering; the final
    // value below is computed after the bogie sizes are known.
    let half_track_for_layout = fuselage_diameter_m * gear_config.track_diameter_factor / 2.0;
    let provisional_strut_positions = mlg_strut_positions(
        n_mlg_struts,
        half_track_for_layout,
        gear_config.wheels_per_mlg_strut,
    );
    let configured_bogie_counts = gear_config
        .mlg_strut_bogie_wheels
        .as_deref()
        .filter(|counts| {
            counts.len() == provisional_strut_positions.len()
                && counts.iter().all(|count| matches!(count, 2 | 4 | 6))
        });

    // When every strut's wheel count is already known (a declared
    // heterogeneous list, or a uniform forced count), the physically correct
    // per-wheel load is the wheel-count-weighted resultant across *all*
    // wheels, R_mlg_total / total_wheels -- not an equal-per-strut split,
    // which would put 50% more load on a wing-gear wheel than a body-gear
    // wheel on a layout like the A380's [4,4,6,6]. Each strut's total load
    // then follows its own wheel count, `load_per_wheel * that count`, and
    // feeding that into `size_bogie`'s forced-count path reproduces exactly
    // `load_per_wheel` as the per-wheel design load regardless of the forced
    // count's value. When no count is known yet (the pure auto-sizing path,
    // used to escalate 2 -> 4 -> 6 wheels/strut from the load itself), there
    // is no wheel count to weight by yet, so the equal-per-strut split is
    // retained as the ladder's starting estimate.
    let all_forced_counts: Option<Vec<i64>> = if let Some(counts) = configured_bogie_counts {
        Some(counts.to_vec())
    } else if gear_config.wheels_per_mlg_strut > 0 {
        Some(vec![
            gear_config.wheels_per_mlg_strut;
            provisional_strut_positions.len()
        ])
    } else {
        None
    };
    let load_per_wheel_estimate = all_forced_counts.as_ref().map(|counts| {
        let total_wheels: i64 = counts.iter().sum();
        r_mlg_total_design / total_wheels.max(1) as f64
    });
    let load_per_strut_equal_split = r_mlg_total_design / n_mlg_struts.max(1) as f64;

    let mut mlg_wheels_per_strut = Vec::with_capacity(provisional_strut_positions.len());
    let mut mlg_tires = Vec::with_capacity(provisional_strut_positions.len());
    let mut label_load = 0.0_f64;
    let mut mlg_overloaded = false;
    let mut mlg_worst_margin = f64::INFINITY;
    for index in 0..provisional_strut_positions.len() {
        let forced_count = configured_bogie_counts
            .map(|counts| counts[index])
            .unwrap_or(gear_config.wheels_per_mlg_strut);
        let strut_load_for_sizing = match load_per_wheel_estimate {
            Some(per_wheel) => per_wheel * forced_count.max(1) as f64,
            None => load_per_strut_equal_split,
        };
        let (wheels, tire, overloaded) = tires::size_bogie(
            strut_load_for_sizing,
            gear_config.tire_safety_factor,
            tires::main_tire_key(&gear_config.tire_class, &gear_config.mlg_tire),
            forced_count,
        );
        let per_wheel_load = strut_load_for_sizing / wheels.max(1) as f64;
        label_load = label_load.max(per_wheel_load * gear_config.tire_safety_factor);
        let margin = tires::margin(
            tire.rated_load_kg,
            per_wheel_load * gear_config.tire_safety_factor,
        );
        mlg_overloaded |= overloaded;
        mlg_worst_margin = mlg_worst_margin.min(margin);
        tire_load_ratios.note_main(per_wheel_load, &tire);
        mlg_wheels_per_strut.push(wheels);
        mlg_tires.push(tire);
    }
    let wheels_per_strut = mlg_wheels_per_strut.iter().copied().max().unwrap_or(0);
    let mlg_tire = mlg_tires
        .iter()
        .copied()
        .max_by(|left, right| left.rated_load_kg.total_cmp(&right.rated_load_kg))
        .unwrap_or(tires::NARROWBODY);
    let mlg_tire_margin = mlg_worst_margin;

    // Capacity is only a meaningful, non-tautological check on a
    // declared (forced) wheel count; an auto-sized bogie was fit to exactly
    // this envelope.
    let capacity_basis_declared =
        configured_bogie_counts.is_some() || gear_config.wheels_per_mlg_strut > 0;
    let tire_overloaded = nlg_overloaded || mlg_overloaded;

    // Derived strength limits (fraction of MTOW)
    let nlg_capacity_kg = n_nlg_wheels as f64 * nlg_tire.rated_load_kg;
    let mlg_capacity_kg: f64 = mlg_wheels_per_strut
        .iter()
        .zip(&mlg_tires)
        .map(|(&wheels, tire)| wheels as f64 * tire.rated_load_kg)
        .sum();
    let pct_load_nlg_max = nlg_capacity_kg / mtow_kg.max(1.0);
    let pct_load_mlg_max = mlg_capacity_kg / mtow_kg.max(1.0);

    // Strut material label
    let strut_material = tires::strut_label(gear_config, label_load);

    // Lateral track width
    // A published track is a source baseline whose definition is already a
    // centreline-to-centreline dimension (A380's 14.34 m is specifically the
    // wing-gear track). The preset's track_diameter_factor is the source
    // ratio, so the active design scales naturally with its fuselage
    // diameter. Do not add the automatic bogie-footprint allowance when a
    // source baseline is present. With no reference, retain the existing
    // automatic sizing convention.
    //
    // The `wheels_per_strut * 0.05 m` term is an undocumented bogie
    // footprint allowance, not derived from turnover
    // geometry or a wing-root/nacelle clearance study; treat it as a
    // heuristic packaging margin, not a sized value.
    let has_reference_track = gear_config
        .reference_track_m
        .is_some_and(|value| value.is_finite() && value > 0.0);
    let track_width_m = if has_reference_track {
        fuselage_diameter_m * gear_config.track_diameter_factor
    } else {
        fuselage_diameter_m * gear_config.track_diameter_factor + wheels_per_strut as f64 * 0.05
    };

    //: Lateral turnover angle (Raymer Ch.11 / Currey overturn criterion).
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

    // Longitudinal tip-back at the most-aft main-gear axle and the most aft CG
    // the aircraft reaches on the ground (the design CG, or the supplied
    // reachable ground CG when it is further aft), against
    // max(min_tip_back_deg, tail-scrape angle): the same two functions the
    // CG envelope's per-state limits use.
    let tip_back_cg_x = tip_back
        .most_aft_ground_cg_x_m
        .filter(|x| x.is_finite())
        .map_or(design_aft_cg_x, |x| x.max(design_aft_cg_x));
    let tip_back_h_cg = tip_back
        .cg_height_m
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(cg_height_estimate_m);
    let tip_back_angle_deg =
        geometry::tip_back_angle_deg(x_mlg_aft_axle_m, tip_back_cg_x, tip_back_h_cg);
    let required_tip_back_deg =
        geometry::required_tip_back_deg(gear_config.min_tip_back_deg, tip_back.scrape_angle_deg);
    let tip_back_ok = tip_back_angle_deg.is_finite() && tip_back_angle_deg >= required_tip_back_deg;

    let wheels = layout::planform_wheels(
        x_nlg,
        n_nlg_wheels,
        &nlg_tire,
        mlg_strut_positions(n_mlg_struts, half_track, gear_config.wheels_per_mlg_strut),
        &main_gear_x_m,
        &mlg_wheels_per_strut,
        &mlg_tires,
    );

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
        x_mlg_aft_axle_m,
        track_width_m,
        wheelbase_m: primary_wheelbase,
        effective_wheelbase_m: effective_wheelbase,
        reference_wheelbase_m: gear_config.reference_wheelbase_m,
        reference_track_m: gear_config.reference_track_m,
        reference_body_wheelbase_m: gear_config.reference_body_wheelbase_m,
        wheels,
        r_nlg_design_kg: r_nlg_design,
        r_mlg_total_design_kg: r_mlg_total_design,
        r_nlg_aft_design_kg: r_nlg_aft_design,
        r_nlg_aft_raw_kg: r_nlg_aft_raw,
        aero_aft_limit_pulled_back,
        r_nlg_dynamic_kg: r_nlg_dynamic,
        pct_load_nlg_max,
        pct_load_mlg_max,
        nlg_tire_margin,
        mlg_tire_margin,
        tire_overloaded,
        tire_load_ratios,
        capacity_basis_declared,
        turnover_angle_deg,
        turnover_ok,
        tip_back_angle_deg,
        required_tip_back_deg,
        tip_back_ok,
    }
}

#[cfg(test)]
mod tests;
