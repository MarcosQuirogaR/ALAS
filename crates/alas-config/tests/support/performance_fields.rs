// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field inputs added after the frozen translated configuration was recorded.

// Each parity binary compiles only the helpers needed by its fixture.
#![allow(dead_code)]

use serde_json::{json, Value};

pub const NATIVE_FIELDS: [&str; 22] = [
    "improved_climb_v2",
    "geometry_scaled_clmax",
    "cruise_thrust_from_deck",
    "cl_max_land_source",
    "engine_match_tolerance",
    "go_around_spool_thrust_fraction",
    "landing_flap_delta_cd",
    "landing_gear_delta_cd",
    "takeoff_gear_delta_cd",
    "v2_min_vsr_factor",
    "wet_landing_runway",
    "takeoff_packs_on",
    "main_gear_truck_wheels",
    "cl_max_to_source",
    "legacy_field_correlations",
    "propeller_dry_landing_distance_share",
    "propeller_landing_deceleration_g",
    "propeller_landing_mean_drag_to_weight",
    "propeller_takeoff_inertia_distance_m",
    "propeller_takeoff_rolling_friction",
    "propeller_takeoff_stop_deceleration_g",
    "cl_max_le_increment",
];

pub fn is_native_field(path: &str, key: &str) -> bool {
    NATIVE_FIELDS.contains(&key)
        && (path == "PerformanceConfig"
            || path.ends_with(".performance")
            || path.starts_with("performance."))
}

/// The frozen landing-distance coefficient of one loaded case and its
/// replacement. The frozen presets used 0.63 for the widebody class and 0.60
/// elsewhere; the product uses one value calibrated on certified landing field
/// lengths (see `PerformanceConfig::default`).
pub fn k_land_change(case: &str) -> (Value, Value) {
    let frozen = match case {
        "A340-300" | "A380-800" | "B787-9" | "DC-10" | "AVE" | "preset_then_field" => 0.63,
        _ => 0.60,
    };
    (json!(frozen), json!(0.505))
}

/// Preserve the frozen input while checking its replacement from published
/// landing evidence rather than copying a computed coefficient into a ledger.
/// The independent landing-reference tests verify the lift balance and source
/// conditions; these checks additionally cover registry and saved-file loading.
pub fn published_landing_change(path: &str) -> Option<(Value, Value)> {
    let (case, key) = path.split_once(".performance.")?;
    if key == "k_land" {
        return Some(k_land_change(case));
    }
    if let Some(change) = configuration_drag_change(case, key) {
        return Some(change);
    }
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    // Take-off CLmax from the declared high-lift system (Raymer 5th ed. Eq. 12.21 / Table 12.2): replaces the frozen
    // 2.10 class default for every jet with a declared system.
    if key == "cl_max_to" {
        let preset = alas_config::presets::get(name).ok()?;
        let parts = alas_config::presets::preset_breakdown(preset)?;
        return Some((json!(2.10), json!(parts.takeoff())));
    }
    // Take-off safety speed at the CS 25.107(b) minimum (1.13 VSR; climb and VMC
    // raise it where they govern) instead of the frozen 1.20 class factor, for
    // every jet with a declared high-lift system.
    if key == "v2_vstall_factor" && alas_config::presets::declared_system(name).is_some() {
        return Some((json!(1.20), json!(1.13)));
    }
    // Clean CLmax from the slats-in flight-test values (presets::clean_lift;
    // Obert Figs. 26.12-26.16) instead of the frozen 1.50 class value:
    // the measured value of the wing, else 1.43 cos(sweep) for a slatted wing.
    if key == "cl_max_clean" {
        if let Some(entry) = alas_config::presets::published_clean_lift(name) {
            return Some((json!(1.5), json!(entry.cl_max_clean)));
        }
        alas_config::presets::declared_system(name)?;
        let preset = alas_config::presets::get(name).ok()?;
        let clean = preset.performance.as_ref()?.cl_max_clean;
        // The preset's own outboard quarter-chord sweep, from its planform.
        let planform = preset
            .geometry
            .wing
            .transport_planform(&preset.design_vector)
            .ok()?;
        let quarter = |s: &alas_config::MainWingStation| s.leading_edge_x_m + 0.25 * s.chord_m;
        let sweep = (quarter(&planform.tip) - quarter(&planform.kink))
            .atan2(planform.tip.y_m - planform.kink.y_m);
        let expected = alas_config::presets::SLATS_IN_CLEAN_OVER_COS_SWEEP * sweep.cos();
        return ((clean - expected).abs() < 1e-9).then(|| (json!(1.5), json!(clean)));
    }
    // Landing CLmax of a declared slatted jet without a published approach
    // speed (the AVE): the pooled slatted-landing law
    // 2.87 cos(outboard quarter-chord sweep) of Obert, Aerodynamic Design of
    // Transport Aircraft (2009), Figs. 26.12-26.16, printed pp. 324-325,
    // replaces the frozen 2.95 class placeholder.
    if key == "cl_max_land"
        && name == "AVE"
        && alas_config::presets::published_landing_reference(name).is_none()
    {
        const SLATTED_LANDING_OVER_COS_SWEEP: f64 = 2.87;
        alas_config::presets::declared_system(name)?;
        let preset = alas_config::presets::get(name).ok()?;
        let landing = preset.performance.as_ref()?.cl_max_land;
        let planform = preset
            .geometry
            .wing
            .transport_planform(&preset.design_vector)
            .ok()?;
        let quarter = |s: &alas_config::MainWingStation| s.leading_edge_x_m + 0.25 * s.chord_m;
        let sweep = (quarter(&planform.tip) - quarter(&planform.kink))
            .atan2(planform.tip.y_m - planform.kink.y_m);
        let expected = SLATTED_LANDING_OVER_COS_SWEEP * sweep.cos();
        return ((landing - expected).abs() < 1e-9).then(|| (json!(2.95), json!(landing)));
    }
    let source = alas_config::presets::published_landing_reference(name)?;
    match key {
        "cl_max_land" => {
            let frozen = match name {
                "A220-300" | "A320-200" => 2.90,
                "A340-300" | "A380-800" | "B787-9" | "DC-10" => 2.95,
                "ATR72-600" => 2.20,
                _ => return None,
            };
            Some((json!(frozen), json!(source.cl_max_land())))
        }
        "vapp_vstall_land_factor" => Some((json!(1.30), json!(1.23))),
        _ => None,
    }
}

/// Fix A1 (2026-10): the take-off flap increment replaces the frozen
/// 0.025 / 0.030 class scalar (Roskam, Airplane Design Part I, Table 3.6 and
/// the component methods of Roskam VI Sec. 4.6, Raymer 6th ed. Eq. 12.61-12.62,
/// Torenbeek App. G-4), and the approach-speed factor becomes the CS 25.125
/// VREF floor of 1.23 VSR0 for every case, not only those with a published
/// landing reference.
///
/// A registered aircraft's take-off flap
/// increment is Raymer Eq. 12.61 on its declared flap run; it is recomputed
/// here, independently of the preset code, by [`raymer_takeoff_flap_drag`].
/// 0.015 stays the default of a case that loads no registered aircraft.
fn configuration_drag_change(case: &str, key: &str) -> Option<(Value, Value)> {
    match key {
        "oei_climb_delta_cd" => {
            let frozen = match case {
                "A340-300" | "A380-800" | "B787-9" | "DC-10" | "AVE" | "preset_then_field" => 0.030,
                _ => 0.025,
            };
            let Ok(preset) = alas_config::presets::get(preset_name(case)) else {
                return Some((json!(frozen), json!(0.015)));
            };
            let actual = preset.performance.as_ref()?.oei_climb_delta_cd;
            let expected = raymer_takeoff_flap_drag(preset)?;
            // The recomputed value is the pin; the preset's own value is returned
            // when it agrees, so the comparison reports the recomputed value
            // when it does not.
            let pinned = if (actual - expected).abs() < 1.0e-12 {
                actual
            } else {
                expected
            };
            Some((json!(frozen), json!(pinned)))
        }
        "vapp_vstall_land_factor" => Some((json!(1.30), json!(1.23))),
        _ => None,
    }
}

/// The registered aircraft a loaded case stands for.
fn preset_name(case: &str) -> &str {
    match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    }
}

/// Raymer, Aircraft Design: A Conceptual Approach, 5th ed., Sec. 12.6.5,
/// Eq. 12.61 (printed p. 452): flap factor of a slotted flap (plain 0.0144).
/// Every declared trailing-edge device of the registered aircraft is slotted.
const RAYMER_SLOTTED_FLAP_FACTOR: f64 = 0.0074;
/// Eq. 12.61: the flap adds no profile drag below 10 deg.
const RAYMER_FLAP_DRAG_ONSET_DEG: f64 = 10.0;
/// Take-off flap when no document states it: the lower end of Raymer's
/// "about 20-40 deg for takeoff" (Sec. 12.6.5, p. 452) and the largest
/// take-off setting of Obert, Aerodynamic Design of Transport Aircraft (2009),
/// Fig. 27.34, printed p. 361.
const RAYMER_DEFAULT_TAKEOFF_FLAP_DEG: f64 = 20.0;

/// Raymer Eq. 12.61, `0.0074 (c_f/c) (S_flapped/S_ref) (delta - 10 deg)`, from
/// the preset's own data, without the preset code's measurement: the declared
/// flap run (`preset_control_surfaces`, the class run when none is declared)
/// less the band an inboard aileron cuts out of it, integrated as full-chord
/// area on the stations of the built wing without its blends (no flap sits in a
/// fillet), over the area of that same wing; the
/// take-off deflection of [`declared_flap_setting_deg`], else the 20 deg
/// default. A run logged A220-300 0.012026, A320-200 0.006230,
/// A340-300 0.014221, A380-800 0.011565, AVE 0.011412, B787-9 0.005573 and
/// DC-10 0.006229 (flapped-area ratios 0.53 to 0.67), a cross-check only.
fn raymer_takeoff_flap_drag(preset: &alas_config::AircraftPreset) -> Option<f64> {
    let controls =
        alas_config::preset_control_surfaces::config_for(preset.name).unwrap_or_default();
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .ok()?
        .without_blends();
    let stations = planform.stations_with_rake();
    let semi_span_m = planform.tip.y_m;
    // Full-chord area of both semispans between `y_in` and `y_out`, chord linear
    // in span between stations.
    let area_m2 = |y_in: f64, y_out: f64| -> f64 {
        stations
            .windows(2)
            .map(|pair| {
                let (inboard, outboard) = (pair[0], pair[1]);
                let (low, high) = (inboard.y_m.max(y_in), outboard.y_m.min(y_out));
                if outboard.y_m <= inboard.y_m || high <= low {
                    return 0.0;
                }
                let chord = |y: f64| {
                    inboard.chord_m
                        + (y - inboard.y_m) / (outboard.y_m - inboard.y_m)
                            * (outboard.chord_m - inboard.chord_m)
                };
                (chord(low) + chord(high)) * (high - low)
            })
            .sum()
    };
    let reference_m2 = area_m2(0.0, semi_span_m);
    let (start, end) = (controls.flap_span_start_frac, controls.flap_span_end_frac);
    let (gap_start, gap_end) = (
        controls.inboard_aileron_span_start_frac.max(start),
        controls.inboard_aileron_span_end_frac.min(end),
    );
    let bands = if controls.inboard_aileron_chord_fraction > 0.0 && gap_end > gap_start {
        vec![(start, gap_start), (gap_end, end)]
    } else {
        vec![(start, end)]
    };
    let flapped_m2: f64 = bands
        .iter()
        .map(|&(band_start, band_end)| area_m2(band_start * semi_span_m, band_end * semi_span_m))
        .sum();
    if !(reference_m2 > 0.0 && controls.flap_chord_fraction > 0.0) {
        return None;
    }
    let deflection_deg = declared_flap_setting_deg(preset.name)
        .map_or(RAYMER_DEFAULT_TAKEOFF_FLAP_DEG, |(takeoff_deg, _)| {
            takeoff_deg
        });
    Some(
        RAYMER_SLOTTED_FLAP_FACTOR
            * controls.flap_chord_fraction
            * (flapped_m2 / reference_m2)
            * (deflection_deg - RAYMER_FLAP_DRAG_ONSET_DEG),
    )
}

/// The take-off and landing flap deflections a registered aircraft documents,
/// degrees, `None` where no document states them (the preset then carries
/// none and Eq. 12.61 takes the 20 deg default). Literals with their sources:
/// - A320-200 15 / 35: take-off CONF 1+F, 2 and 3 (Obert, Aerodynamic Design of
///   Transport Aircraft, 2009, Fig. 27.34, printed p. 361; CONF 1+F is flaps
///   10 deg, AAIB Bulletin 9/2017, A319 G-EZEW, p. 1); Jane's All the World's
///   Aircraft 2008-09, A320 entry, PDF p. 315, flaps 10/15/20/35 deg.
/// - A340-300 22 / 32: Airbus A340-200/-300 AC Fig. 3-3-1-991-006-A01 flaps-3
///   branch; 22 deg is the A330-200 flaps-3 detent of Obert Fig. 26.14 (the
///   shared A330/A340 flap system, not an A340 document); Jane's 2008-09
///   A330/A340 entry, PDF p. 323, flaps 32 deg.
/// - B787-9 15 / 30: take-off flaps 5, 15 and 20 (ATSB AO-2017-051); landing
///   flaps 30, the largest of the Boeing D6-58333 Rev Q landing charts 3.4;
///   Boeing flap positions taken as degrees (Obert Fig. 26.16).
/// - DC-10 15 / 50: AAIB Bulletin 7/97, EW/C97/2/1 (DC-10-30 PP-VMD), p. 2,
///   15 deg take-off flap at maximum weight; landing 50 deg, the Boeing DC-10
///   ACAP landing chart basis ("flaps 50").
pub fn declared_flap_setting_deg(case: &str) -> Option<(f64, f64)> {
    match preset_name(case) {
        "A320-200" => Some((15.0, 35.0)),
        "A340-300" => Some((22.0, 32.0)),
        "B787-9" => Some((15.0, 30.0)),
        "DC-10" => Some((15.0, 50.0)),
        _ => None,
    }
}
