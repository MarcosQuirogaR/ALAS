// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The geometry a preset-less brief's dimensions depend on.
//!
//! The geometry defaults are the AVE reference twin's: a 6 m nose taper, a
//! 14 m tail cone, a wing datum 26.24 m aft of the nose, engines 9.8 m out
//! and an 8 m-chord tailplane. Under a 3 m body and a 30 m wing they describe
//! no aircraft. For a derived clean sheet (see the parent module) each is
//! re-derived from the brief before the file is overlaid:
//!
//! - nose taper, tail cone and the three body heights in proportion to the
//!   fuselage diameter, at the fleet-median ratios;
//! - the side-of-body station at the body half-width, the wing root height
//!   in proportion to the diameter and a single fleet-median dihedral rise;
//! - a twin's engines at the fleet-median fraction of the class semispan,
//!   hung below the wing in proportion to the selected nacelle's radius;
//! - the wing datum so the class wing's quarter MAC sits at the fleet-median
//!   station of the estimated fuselage;
//! - the empennage scaled from the reference twin's at constant horizontal
//!   and vertical tail volume coefficients (Raymer, *Aircraft Design: A
//!   Conceptual Approach*, 6th ed., chapter 6, tail volume coefficient
//!   method): `S_h ~ S c / L` and `S_v ~ S b / L` with the arms in proportion
//!   to the fuselage length, so lengths scale with their square roots.
//!
//! A field the file sets explicitly keeps the file's value and is used as
//! such by every relation downstream of it.

use serde_json::Value;

use super::fleet::fleet_statistics;
use super::planform::WingPlanformSummary;
use crate::{
    overlay, AerodromeReferenceCode, AlasConfig, DesignVector, GeometryConfig, OverlayError,
};

/// Minimum main aisle width above 25 in from the floor for 20 or more seats,
/// m (CS 25.815 / 14 CFR 25.815: 20 in).
const MIN_AISLE_WIDTH_M: f64 = 0.508;

/// Most seats abreast a single aisle and two aisles admit (CS 25.817: at
/// most three seats between any passenger and an aisle).
const MAX_ABREAST: [(f64, f64); 2] = [(1.0, 6.0), (2.0, 12.0)];

/// Cabin length taken by galleys, lavatories and exit rows, as a fraction
/// of the seated rows' length. Engineering allowance for the first length
/// estimate only; a cabin-sized clean sheet replaces that estimate with the
/// detailed layout's shortest seating length.
const MONUMENT_ALLOWANCE: f64 = 0.15;

/// Prepare `instance` for a file that names no preset: bind a selected
/// catalogue engine, then derive the brief-dependent geometry when the brief
/// departs from the shipped reference aircraft.
///
/// # Errors
///
/// None in practice: an overlay the file cannot pass is left for the
/// caller's own overlay to report.
pub(crate) fn seed_preset_less_defaults(
    instance: &mut AlasConfig,
    data: &Value,
) -> Result<(), OverlayError> {
    let Ok(brief) = overlay::<AlasConfig>(instance, data) else {
        return Ok(());
    };
    // A file may state the mark itself, an explicit false included; otherwise
    // its brief decides it, and a clean-sheet brief is saved marked so a
    // reload keeps the decision. (An undecided reference brief re-decides
    // to the same answer on every load.)
    if data
        .pointer("/optimizer/design_space/clean_sheet_brief")
        .is_none()
        && brief.optimizer.design_space.mode == crate::DesignMode::CleanSheet
        && brief.brief_departs_from_reference()
    {
        instance.optimizer.design_space.clean_sheet_brief = Some(true);
    }
    let Ok(probe) = overlay::<AlasConfig>(instance, data) else {
        return Ok(());
    };
    if !probe.derives_clean_sheet_start() {
        return Ok(());
    }
    // In a clean-sheet brief an engine name selects the catalogue entry, as
    // a preset's does; any cycle or nacelle field the file sets is overlaid
    // on top of it.
    if let Some(name) = data
        .pointer("/geometry/engine/engine_name")
        .and_then(Value::as_str)
    {
        if name != instance.geometry.engine.engine_name {
            instance.geometry.engine.engine_name = name.to_owned();
            instance.geometry.engine.apply_engine_spec();
        }
    }
    // A clean-sheet brief that states no aerodrome code is `Auto`, never a
    // silent code F.
    if data
        .pointer("/optimizer/objective/aerodrome_reference_code")
        .is_none()
    {
        instance.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::Auto;
    }
    let Ok(mut brief) = overlay::<AlasConfig>(instance, data) else {
        return Ok(());
    };
    brief.apply_clean_sheet_commit(data, true);
    instance.optimizer.objective.derived_aerodrome_code =
        brief.optimizer.objective.derived_aerodrome_code;
    instance.geometry = brief.geometry;
    Ok(())
}

/// First estimate of the fuselage length, m: nose taper and tail cone plus
/// the rows of economy seats the brief needs, abreast by CS 25.817 in the
/// configured usable width, at the configured economy pitch with
/// [`MONUMENT_ALLOWANCE`]. A freighter has no seated cabin and takes the
/// reference twin's length scaled by its diameter.
pub(super) fn estimated_fuselage_length_m(config: &AlasConfig) -> Option<f64> {
    let f = &config.geometry.fuselage;
    let d = f.diameter_m;
    if !(d.is_finite() && d > 0.0) {
        return None;
    }
    if config.requirements.aircraft_type == "cargo" {
        let reference = crate::FuselageConfig::default().diameter_m;
        return Some(DesignVector::default().fuselage_length_m * d / reference);
    }
    let cabin = &config.cabin.passenger;
    let seat_m = cabin.economy.width_m;
    let usable_m = d - 2.0 * cabin.wall_thickness_m;
    let abreast = MAX_ABREAST
        .iter()
        .map(|(aisles, max)| {
            ((usable_m - aisles * MIN_AISLE_WIDTH_M) / seat_m)
                .floor()
                .min(*max)
        })
        .fold(1.0_f64, f64::max);
    let rows = (config.requirements.num_passengers.max(1) as f64 / abreast).ceil();
    let length_m = f.cabin_start_x_m
        + f.tailcone_length_m
        + rows * cabin.economy.pitch_m * (1.0 + MONUMENT_ALLOWANCE);
    (length_m.is_finite() && length_m > 0.0).then_some(length_m)
}

/// `brief`'s geometry with every brief-dependent field the file leaves
/// unset re-derived; `None` when the brief cannot be sized.
pub(super) fn derive(brief: &AlasConfig, data: &Value) -> Option<GeometryConfig> {
    let stats = fleet_statistics()?;
    let set = |pointer: &str| data.pointer(pointer).is_some();
    let mut config = brief.clone();
    let g = &mut config.geometry;
    let d = g.fuselage.diameter_m;
    if !(d.is_finite() && d > 0.0) {
        return None;
    }
    let put = |pointer: &str, slot: &mut f64, value: f64| {
        if !set(pointer) && value.is_finite() {
            *slot = value;
        }
    };
    let f = &mut g.fuselage;
    put(
        "/geometry/fuselage/cabin_start_x_m",
        &mut f.cabin_start_x_m,
        stats.nose_length_per_diameter * d,
    );
    put(
        "/geometry/fuselage/tailcone_length_m",
        &mut f.tailcone_length_m,
        stats.tailcone_length_per_diameter * d,
    );
    put(
        "/geometry/fuselage/nose_z_m",
        &mut f.nose_z_m,
        stats.nose_z_per_diameter * d,
    );
    put(
        "/geometry/fuselage/cabin_z_m",
        &mut f.cabin_z_m,
        stats.cabin_z_per_diameter * d,
    );
    put(
        "/geometry/fuselage/tail_z_m",
        &mut f.tail_z_m,
        stats.tail_z_per_diameter * d,
    );

    let class = config.class_wing()?;
    let semispan_m = 0.5 * class.span_m;
    let g = &mut config.geometry;
    let w = &mut g.wing;
    let kink = w.kink_span_fraction.unwrap_or(w.break_span_fraction);
    let side_of_body = 0.5 * d / semispan_m;
    if !set("/geometry/wing/side_of_body_span_fraction") && side_of_body < kink {
        w.side_of_body_span_fraction = Some(side_of_body);
    }
    put(
        "/geometry/wing/root_z_m",
        &mut w.root_z_m,
        stats.wing_root_z_per_diameter * d,
    );
    let rise_m = stats.wing_rise_per_semispan * semispan_m;
    let root_z = w.root_z_m;
    put(
        "/geometry/wing/break_z_m",
        &mut w.break_z_m,
        root_z + rise_m * kink,
    );
    put("/geometry/wing/tip_z_m", &mut w.tip_z_m, root_z + rise_m);

    let e = &mut g.engine;
    let twin =
        e.spanwise_positions_m.len() == 2 && e.spanwise_positions_m.iter().all(|y| *y != 0.0);
    if twin && !set("/geometry/engine/spanwise_positions_m") {
        let y = stats.engine_semispan_fraction * semispan_m;
        e.spanwise_positions_m = vec![y, -y];
    }
    if twin {
        let radius = e.radius_scale_m;
        let length = e.nacelle_length_m();
        put(
            "/geometry/engine/z_m",
            &mut e.z_m,
            stats.engine_z_per_radius * radius,
        );
        put(
            "/geometry/engine/inlet_x_offset_m",
            &mut e.inlet_x_offset_m,
            stats.inlet_offset_per_nacelle_length * length,
        );
    }

    let start = config.chords_for_area(class)?;
    let length_m = estimated_fuselage_length_m(&config)?;
    let wing = WingPlanformSummary::of(&config.geometry.wing, &start)?;
    let datum = stats.quarter_mac_station_per_length * length_m - wing.quarter_mac_x_m;
    put(
        "/geometry/wing/root_datum_x_m",
        &mut config.geometry.wing.root_datum_x_m,
        datum,
    );
    scale_empennage(&mut config.geometry, data, wing, length_m);
    Some(config.geometry)
}

/// Scale the reference twin's empennage to `wing` on a fuselage of
/// `length_m` at constant tail volume coefficients; body-relative heights
/// follow the diameter.
pub(super) fn scale_empennage(
    g: &mut GeometryConfig,
    data: &Value,
    wing: WingPlanformSummary,
    length_m: f64,
) {
    let reference = GeometryConfig::default();
    let reference_dv = DesignVector::default();
    let Some(ref_wing) = WingPlanformSummary::of(&reference.wing, &reference_dv) else {
        return;
    };
    let ref_length_m = reference_dv.fuselage_length_m;
    let k_h = ((wing.area_m2 * wing.mac_m / length_m)
        / (ref_wing.area_m2 * ref_wing.mac_m / ref_length_m))
        .sqrt();
    let k_v = ((wing.area_m2 * wing.span_m / length_m)
        / (ref_wing.area_m2 * ref_wing.span_m / ref_length_m))
        .sqrt();
    let k_d = g.fuselage.diameter_m / reference.fuselage.diameter_m;
    if !(k_h.is_finite() && k_v.is_finite() && k_d.is_finite()) {
        return;
    }
    let set = |field: &str| {
        data.pointer(&format!("/geometry/empennage/{field}"))
            .is_some()
    };
    let r = &reference.empennage;
    let t = &mut g.empennage;
    let put = |field: &str, slot: &mut f64, value: f64| {
        if !set(field) {
            *slot = value;
        }
    };
    put(
        "hstab_root_chord_m",
        &mut t.hstab_root_chord_m,
        r.hstab_root_chord_m * k_h,
    );
    put(
        "hstab_tip_chord_m",
        &mut t.hstab_tip_chord_m,
        r.hstab_tip_chord_m * k_h,
    );
    put(
        "hstab_offset_from_tail_m",
        &mut t.hstab_offset_from_tail_m,
        r.hstab_offset_from_tail_m * k_h,
    );
    put("hstab_z_m", &mut t.hstab_z_m, r.hstab_z_m * k_d);
    put(
        "vstab_root_chord_m",
        &mut t.vstab_root_chord_m,
        r.vstab_root_chord_m * k_v,
    );
    put(
        "vstab_tip_chord_m",
        &mut t.vstab_tip_chord_m,
        r.vstab_tip_chord_m * k_v,
    );
    put(
        "vstab_offset_from_tail_m",
        &mut t.vstab_offset_from_tail_m,
        r.vstab_offset_from_tail_m * k_v,
    );
    put("vstab_z_m", &mut t.vstab_z_m, r.vstab_z_m * k_d);
    if !set("hstab_tip_le_m") {
        let (x, y, z) = r.hstab_tip_le_m;
        t.hstab_tip_le_m = (x * k_h, y * k_h, z * k_h);
    }
    if !set("vstab_tip_le_m") {
        let (x, y, z) = r.vstab_tip_le_m;
        t.vstab_tip_le_m = (x * k_v, y * k_v, z * k_v);
    }
}
