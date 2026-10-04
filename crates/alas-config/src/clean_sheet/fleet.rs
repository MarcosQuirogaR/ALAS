// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Class statistics of the registered turbofan transports.
//!
//! A clean-sheet start needs a handful of dimensionless class relations: the
//! cruise lift coefficient a transport wing is sized for, its aspect ratio,
//! the sweep its cruise Mach number asks for, how its chords taper, how its
//! nose and tail cone scale with the body diameter and where its wing and
//! engines sit. Each is measured here, once, on the registered turbofan
//! aircraft, as the median over the fleet, so the relations follow the
//! registry rather than a second set of literals beside it. The median is
//! used rather than the mean so a single unusual type (the A380's
//! span-limited wing, the DC-10's centre engine) cannot drag a relation.
//!
//! The turboprop in the registry is excluded: its straight, low-Mach wing
//! belongs to a different class and would bias every wing relation.
//!
//! Units: every statistic is dimensionless. Lengths are normalised by the
//! fuselage diameter (body shape and vertical positions), the semispan
//! (dihedral rise, engine station), the fuselage length (wing station) or
//! the nacelle length (inlet offset), as each field states.

use std::sync::OnceLock;

use crate::engines::PropulsionTechnology;
use crate::presets::{registry, AircraftPreset};

use super::planform::WingPlanformSummary;

/// Fleet-median class relations of the registered turbofan transports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FleetStatistics {
    /// Cruise lift coefficient at MTOW, `m g / (q S)`, at each type's
    /// declared cruise Mach number and altitude.
    pub cruise_lift_coefficient_at_mtow: f64,
    /// Projected aspect ratio `b^2 / S`.
    pub aspect_ratio: f64,
    /// Cruise Mach number normal to the inboard leading edge, `M cos(L_LE)`.
    pub normal_mach: f64,
    /// Trailing-edge break chord over centreline root chord.
    pub break_root_chord_ratio: f64,
    /// Tip chord over centreline root chord.
    pub tip_root_chord_ratio: f64,
    /// Nose taper length over fuselage diameter.
    pub nose_length_per_diameter: f64,
    /// Tail-cone length over fuselage diameter.
    pub tailcone_length_per_diameter: f64,
    /// Nose-tip vertical offset over fuselage diameter.
    pub nose_z_per_diameter: f64,
    /// Cabin-axis vertical offset over fuselage diameter.
    pub cabin_z_per_diameter: f64,
    /// Tail-tip vertical offset over fuselage diameter.
    pub tail_z_per_diameter: f64,
    /// Wing centreline root height over fuselage diameter.
    pub wing_root_z_per_diameter: f64,
    /// Ground-shape tip rise over semispan (the dihedral slope).
    pub wing_rise_per_semispan: f64,
    /// Wing quarter-MAC station over fuselage length.
    pub quarter_mac_station_per_length: f64,
    /// Twin wing-mounted engine station over semispan.
    pub engine_semispan_fraction: f64,
    /// Engine axis height below the local leading edge over nacelle radius.
    pub engine_z_per_radius: f64,
    /// Inlet offset ahead of the local leading edge over nacelle length.
    pub inlet_offset_per_nacelle_length: f64,
}

/// The fleet statistics, measured once on first use.
///
/// `None` only if the registry held no turbofan aircraft with a buildable
/// planform, which would be a registry defect rather than a user input.
pub fn fleet_statistics() -> Option<&'static FleetStatistics> {
    static STATISTICS: OnceLock<Option<FleetStatistics>> = OnceLock::new();
    STATISTICS.get_or_init(measure).as_ref()
}

/// One registered turbofan aircraft's wing and body, as the relations read it.
struct Sample<'a> {
    preset: &'a AircraftPreset,
    wing: WingPlanformSummary,
}

fn measure() -> Option<FleetStatistics> {
    let samples: Vec<Sample<'_>> = registry()
        .iter()
        .filter(|preset| {
            preset.geometry.engine.propulsion_technology == PropulsionTechnology::Turbofan
        })
        .filter_map(|preset| {
            WingPlanformSummary::of(&preset.geometry.wing, &preset.design_vector)
                .map(|wing| Sample { preset, wing })
        })
        .collect();
    let stat = |f: &dyn Fn(&Sample<'_>) -> Option<f64>| median(samples.iter().filter_map(f));
    let twins: Vec<&Sample<'_>> = samples
        .iter()
        .filter(|s| {
            let positions = &s.preset.geometry.engine.spanwise_positions_m;
            positions.len() == 2 && positions.iter().all(|y| *y != 0.0)
        })
        .collect();
    let twin_stat =
        |f: &dyn Fn(&Sample<'_>) -> Option<f64>| median(twins.iter().filter_map(|s| f(s)));
    Some(FleetStatistics {
        cruise_lift_coefficient_at_mtow: stat(&|s| {
            let r = &s.preset.requirements;
            let q = super::cruise_dynamic_pressure_pa(r.cruise_mach, r.cruise_altitude_m)?;
            Some(r.mtow_kg * r.gravity_m_s2 / (q * s.wing.area_m2))
        })?,
        aspect_ratio: stat(&|s| Some(s.wing.span_m.powi(2) / s.wing.area_m2))?,
        normal_mach: stat(&|s| {
            Some(
                s.preset.requirements.cruise_mach
                    * s.preset.design_vector.sweep_deg.to_radians().cos(),
            )
        })?,
        break_root_chord_ratio: stat(&|s| {
            let dv = &s.preset.design_vector;
            Some(dv.break_chord_m / dv.root_chord_m)
        })?,
        tip_root_chord_ratio: stat(&|s| {
            let dv = &s.preset.design_vector;
            Some(dv.tip_chord_m / dv.root_chord_m)
        })?,
        nose_length_per_diameter: stat(&|s| per_diameter(s, |f| f.cabin_start_x_m))?,
        tailcone_length_per_diameter: stat(&|s| per_diameter(s, |f| f.tailcone_length_m))?,
        nose_z_per_diameter: stat(&|s| per_diameter(s, |f| f.nose_z_m))?,
        cabin_z_per_diameter: stat(&|s| per_diameter(s, |f| f.cabin_z_m))?,
        tail_z_per_diameter: stat(&|s| per_diameter(s, |f| f.tail_z_m))?,
        wing_root_z_per_diameter: stat(&|s| {
            let d = s.preset.geometry.fuselage.diameter_m;
            Some(s.preset.geometry.wing.root_z_m / d)
        })?,
        wing_rise_per_semispan: stat(&|s| {
            let w = &s.preset.geometry.wing;
            Some((w.tip_z_m - w.root_z_m) / (0.5 * s.wing.span_m))
        })?,
        quarter_mac_station_per_length: stat(&|s| {
            let dv = &s.preset.design_vector;
            let station =
                s.preset.geometry.wing.root_datum_x_m + dv.wing_x_shift_m + s.wing.quarter_mac_x_m;
            Some(station / dv.fuselage_length_m)
        })?,
        engine_semispan_fraction: twin_stat(&|s| {
            let y = s.preset.geometry.engine.spanwise_positions_m.first()?.abs();
            Some(y / (0.5 * s.wing.span_m))
        })?,
        engine_z_per_radius: twin_stat(&|s| {
            let e = &s.preset.geometry.engine;
            Some(e.z_m / e.radius_scale_m)
        })?,
        inlet_offset_per_nacelle_length: twin_stat(&|s| {
            let e = &s.preset.geometry.engine;
            Some(e.inlet_x_offset_m / e.nacelle_length_m())
        })?,
    })
}

fn per_diameter(sample: &Sample<'_>, field: impl Fn(&crate::FuselageConfig) -> f64) -> Option<f64> {
    let fuselage = &sample.preset.geometry.fuselage;
    Some(field(fuselage) / fuselage.diameter_m)
}

/// Median of the finite values, `None` when there are none.
pub(super) fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut finite: Vec<f64> = values.filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let mid = finite.len() / 2;
    Some(if finite.len() % 2 == 0 {
        0.5 * (finite[mid - 1] + finite[mid])
    } else {
        finite[mid]
    })
}
