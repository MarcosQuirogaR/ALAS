// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The extended main gear against the bodies hung from the wing.
//!
//! NEEDS-DATA: the main gear against the flap track and the gear bay, the pylon and
//! fairing clearances and the tail against the engine exhaust plume have no sourced criterion
//! in this tree. Each needs a published criterion (for example Raymer, *Aircraft Design: A
//! Conceptual Approach*, 6th ed., or Torenbeek, *Synthesis of Subsonic Airplane Design*,
//! 1982) before a row is added; none is invented here.
//!
//! Frame: aircraft axes (x aft from the nose, y right, z up), metres. The
//! gear is the static, extended one the airport-planning drawings show: each
//! main wheel is a cylinder of the sized tyre's diameter and section width
//! with its axle `diameter / 2` above the ground plane and its axis along y.
//! At a station `x` the wheel's cross-section in the `(y, z)` plane is a
//! rectangle as wide as the tyre and as high as the chord of the wheel circle
//! at that station. Each engine body is judged against that rectangle at
//! every station the wheel spans; the gap is the smallest separation, m
//! (negative is penetration). A body that does not reach the wheel's
//! stations is clear by at least its distance along x.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_perf::landing_gear::size_landing_gear_with_group_stations;

use super::sections::{section_rect_gap_m, Rect};
use super::{is_engine_body, section_at, GroundPose};
use crate::mdo::sizing::SizingOutcome;

/// Stations sampled across one wheel.
const WHEEL_STATIONS: usize = 9;

/// Smallest gap between any main wheel and any engine body, m; `None` when
/// there is no wheel or no engine body to compare.
pub(crate) fn main_gear_nacelle_gap_m(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    pose: &GroundPose,
    nacelle_rise: &dyn Fn(f64) -> f64,
) -> Option<f64> {
    let plane: &Airplane = &outcome.plane;
    let fuselage = plane.fuselages.first()?;
    let nose_x = fuselage.xsecs.first()?.xyz_c[0];
    let length = fuselage.xsecs.last()?.xyz_c[0] - nose_x;
    let frame = plane.mac_frame()?;
    let stations = config.landing_gear.resolved_station_positions(
        nose_x + length * config.mass_model.nlg_x_fraction,
        frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
        nose_x,
        length,
    );
    let aft_x = outcome.x_np - config.requirements.target_static_margin * outcome.mac;
    let forward_x = aft_x
        - crate::envelope::usable_range::UsableRangeRequirement::of(config)
            .length_m(outcome.sized.takeoff_mass_kg, outcome.mac);
    let diameter_m = config.geometry.fuselage.diameter_m;
    let layout = size_landing_gear_with_group_stations(
        outcome.sized.takeoff_mass_kg,
        stations.x_nlg_m,
        stations.x_mlg_m,
        forward_x,
        aft_x,
        diameter_m,
        diameter_m * 1.1,
        &stations.main_gear_x_m,
        config.mass_model.pct_load_nlg_min,
        &config.landing_gear,
    );
    let engines: Vec<_> = plane
        .fuselages
        .iter()
        .skip(1)
        .filter(|body| is_engine_body(body))
        .collect();
    let mut gap = f64::INFINITY;
    for wheel in layout.wheels.iter().filter(|wheel| wheel.group == "MLG") {
        let radius = 0.5 * wheel.diameter_m;
        for body in &engines {
            // A body that ends before the wheel starts (or begins after it
            // ends) is clear by at least the distance along x.
            if let (Some(first), Some(last)) = (body.xsecs.first(), body.xsecs.last()) {
                let along =
                    (first.xyz_c[0] - (wheel.x + radius)).max((wheel.x - radius) - last.xyz_c[0]);
                if along > 0.0 {
                    gap = gap.min(along);
                    continue;
                }
            }
            for x_m in linspace(wheel.x - radius, wheel.x + radius, WHEEL_STATIONS) {
                let Some(mut section) = section_at(body, x_m) else {
                    continue;
                };
                // The static ground shape: the flight rise is removed.
                section.xyz_c[2] -= nacelle_rise(section.xyz_c[1].abs());
                let half_chord = (radius * radius - (x_m - wheel.x).powi(2)).max(0.0).sqrt();
                let axle_z = pose.ground_z_m + radius;
                let tyre = Rect {
                    y0: wheel.y - 0.5 * wheel.width_m,
                    y1: wheel.y + 0.5 * wheel.width_m,
                    z0: axle_z - half_chord,
                    z1: axle_z + half_chord,
                };
                if let Some(g) = section_rect_gap_m(&section, &tyre) {
                    gap = gap.min(g);
                }
            }
        }
    }
    gap.is_finite().then_some(gap)
}
