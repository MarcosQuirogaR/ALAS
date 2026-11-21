// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometric interference residuals on the built airframe: bodies that must
//! not overlap and surfaces that must not strike the ground.
//!
//! Frame, units and sign: aircraft axes (x aft from the nose, y to the right,
//! z up), metres, degrees. Every clearance below is a gap in metres, positive
//! when clear; the residual is feasible iff the gap is non-negative.
//!
//! # What is checked
//!
//! - `nacelle_clear_of_fuselage`, `nacelle_clear_of_nacelle`: no podded engine
//!   body overlaps the fuselage or another engine body. A physical necessity,
//!   no band: the gap along the line between two section centres at each
//!   common station is the centre distance less the two superellipse radii in
//!   that direction (for two convex sections containing their centres an
//!   overlap along that line is an overlap).
//! - `wing_tip_ground_clearance_in_roll`, `nacelle_ground_clearance_in_roll`
//!   and `tail_ground_clearance_in_roll`: Raymer, *Aircraft Design: A
//!   Conceptual Approach*, ch. 11 (tricycle landing-gear geometry, Fig. 11.5,
//!   p. 342): "The wingtip should be checked to make sure it doesn't strike
//!   the ground when the aircraft is at this nose-up angle while in a 5
//!   degree roll", the nose-up angle being the tail-down (tipback) attitude
//!   about the main wheels. Raymer names the wingtip; the same pose is applied to the
//!   lowest wing point of every section (and the tip device), to the lowest
//!   engine-body points and to the horizontal tail, the other surfaces that
//!   hang out of the body. Raymer's sentence names the wingtip at the tail-down
//!   attitude; extending it to the nacelles and the tail is this module's
//!   choice, and because points forward of the axle rise when the aircraft
//!   pitches up, each surface is judged at both ends of the attitude range,
//!   level and tail-down (height is linear in the pitch rotation of a point to
//!   first order, so the extremes bound it): the gap is the smaller of the
//!   two. The tail-down attitude is the model's tail-scrape angle at the main
//!   gear, the largest nose-up attitude the airframe can reach; the roll
//!   pivots about the outboard main-gear centreline (track/2; the tyre's
//!   outer edge lies slightly outboard, which would reduce the drop a little). Heights are those of the
//!   static ground shape (the shape the airport-planning clearance tables
//!   measure): the flight shape's wing-tip rise is removed from the wing and,
//!   unless the nacelles hold their ground height, from the nacelles, as
//!   `alas_config::WingShape` describes (a uniform dihedral increment, linear
//!   in `y`).
//! - `nacelle_clear_of_wing`: the top of every engine body stays below the
//!   lower surface of the wing over the plan area they share (no penetration;
//!   no gully-depth minimum is applied because Jenkinson ch. 9 names the gully
//!   but gives no number). Not evaluated for a turboprop, whose nacelle is
//!   faired into the wing by design; no geometric exemption otherwise, so a
//!   nacelle pushed into the wing reports its full penetration.
//! - `propeller_ground_clearance` and `propeller_ground_clearance_tyre_deflated`
//!   (turboprops only): CS 25.925(a), see the `propeller` module. The disc is centred on
//!   the first section of the engine body.
//! - `tail_root_attached`: the mid-chord point of the horizontal-tail root
//!   lies inside the fuselage or inside the fin (a T-tail sits on the fin
//!   tip). The root of a surface that is attached to nothing carries no load;
//!   this is the dedicated relation between the tail station and the body
//!   that `docs/optimizer-design-vector.md` records as missing.
//!
//! A body that declares no `fuselage_ground_clearance_m` is judged at both ends
//! of the documented clearance sample of its class
//! ([`alas_mass::stations::GroundPlaneBasis`]) and every row keeps the smaller
//! gap: no single end is the conservative one for every row (the tail gap
//! shrinks with a larger clearance, the nacelle, wing-tip and propeller gaps
//! with a smaller one).
//!
//! The zero gap is the physical limit. No margin is added and none removed.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::spacing::linspace;

use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::ConstraintFamily::Geometry;
use crate::mdo::types::ConstraintResidual;
use crate::mdo::ResidualRole;

/// Roll angle of the ground-clearance check, deg: Raymer ch. 11, p. 342.
pub(crate) const GROUND_CLEARANCE_ROLL_DEG: f64 = 5.0;

/// Points sampled around each engine-body section.
const SECTION_POINTS: usize = 16;

/// Length scale that normalises a gap into a dimensionless violation, m.
const GAP_SCALE_M: f64 = 1.0;

/// Where the aircraft stands and how it is posed for the ground check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroundPose {
    /// Ground plane height, m.
    pub ground_z_m: f64,
    /// Rotation pivot, m aft of the nose: the aft-most main-gear station
    /// (`LandingGearStationPositions::rotation_pivot_x_m`).
    pub x_main_gear_m: f64,
    /// Lateral station of the outboard main wheel, m (positive).
    pub y_outboard_wheel_m: f64,
    /// Nose-up pitch about the main-wheel contact, rad.
    pub pitch_rad: f64,
    /// Roll about the outboard wheel contact, rad.
    pub roll_rad: f64,
}

impl GroundPose {
    /// Height above the ground of a point after the aircraft is pitched nose
    /// up by `pitch_rad` about the main-wheel contact line and then rolled by
    /// `roll_rad` about the outboard wheel contact, m. The point is taken on
    /// the side the roll lowers (`y` mirrored to positive).
    pub(crate) fn height_m(&self, point: [f64; 3]) -> f64 {
        let dx = point[0] - self.x_main_gear_m;
        let h = point[2] - self.ground_z_m;
        let pitched = h * self.pitch_rad.cos() - dx * self.pitch_rad.sin();
        let dy = point[1].abs() - self.y_outboard_wheel_m;
        pitched * self.roll_rad.cos() - dy * self.roll_rad.sin()
    }
}

/// Directional half-extent of a superellipse section along the unit vector
/// `(uy, uz)`: the radius at which the ray leaves `|y/a|^n + |z/b|^n = 1`.
fn section_radius(section: &FuselageXSec, uy: f64, uz: f64) -> f64 {
    let a = 0.5 * section.width;
    let b = 0.5 * section.height;
    let n = if section.shape.is_finite() && section.shape > 0.0 {
        section.shape
    } else {
        2.0
    };
    let sum = (uy.abs() / a).powf(n) + (uz.abs() / b).powf(n);
    if sum > 0.0 {
        sum.powf(-1.0 / n)
    } else {
        f64::INFINITY
    }
}

/// The section of `body` at station `x_m`, interpolated linearly between its
/// stations, or `None` outside its x range.
fn section_at(body: &Fuselage, x_m: f64) -> Option<FuselageXSec> {
    let first = body.xsecs.first()?;
    let last = body.xsecs.last()?;
    if !(first.xyz_c[0]..=last.xyz_c[0]).contains(&x_m) {
        return None;
    }
    for pair in body.xsecs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let span = b.xyz_c[0] - a.xyz_c[0];
        if x_m <= b.xyz_c[0] && span > 0.0 {
            let t = ((x_m - a.xyz_c[0]) / span).clamp(0.0, 1.0);
            let lerp = |p: f64, q: f64| p + t * (q - p);
            return Some(FuselageXSec {
                xyz_c: [
                    x_m,
                    lerp(a.xyz_c[1], b.xyz_c[1]),
                    lerp(a.xyz_c[2], b.xyz_c[2]),
                ],
                width: lerp(a.width, b.width),
                height: lerp(a.height, b.height),
                shape: lerp(a.shape, b.shape),
            });
        }
    }
    Some(*last)
}

/// Smallest gap between two bodies over their common stations, m (negative
/// is overlap); `None` when they share no station.
pub(crate) fn body_gap_m(first: &Fuselage, second: &Fuselage) -> Option<f64> {
    let lo = first.xsecs.first()?.xyz_c[0].max(second.xsecs.first()?.xyz_c[0]);
    let hi = first.xsecs.last()?.xyz_c[0].min(second.xsecs.last()?.xyz_c[0]);
    if lo.is_nan() || hi.is_nan() || lo >= hi {
        return None;
    }
    let mut gap = f64::INFINITY;
    for x_m in linspace(lo, hi, 101) {
        let (Some(p), Some(q)) = (section_at(first, x_m), section_at(second, x_m)) else {
            continue;
        };
        // The exact boundary separation is needed only where the sections
        // can come closer than the best gap found so far.
        if sections::gap_lower_bound_m(&p, &q) >= gap {
            continue;
        }
        if let Some(g) = sections::section_gap_m(&p, &q) {
            gap = gap.min(g);
        }
    }
    gap.is_finite().then_some(gap)
}

/// Whether a body is a podded engine (the builder names them `Nacelle ...`).
fn is_engine_body(body: &Fuselage) -> bool {
    body.name.contains("Nacelle") || body.name.contains("Pod")
}

/// One clearance row: feasible iff `gap_m >= 0`.
fn gap_row(id: &'static str, gap_m: f64, role: ResidualRole) -> ConstraintResidual {
    ConstraintResidual::direct(
        id,
        Geometry,
        gap_m,
        0.0,
        "m",
        -gap_m,
        ((-gap_m) / GAP_SCALE_M - 1.0e-5).max(0.0),
        role,
    )
}

/// The interference residuals of one sized candidate.
pub(super) fn interference_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let plane = &outcome.plane;
    let mut residuals = Vec::new();
    if let Some(fuselage) = plane.fuselages.first() {
        let engines: Vec<&Fuselage> = plane
            .fuselages
            .iter()
            .skip(1)
            .filter(|body| is_engine_body(body))
            .collect();
        let to_fuselage = engines
            .iter()
            .filter_map(|body| body_gap_m(fuselage, body))
            .fold(f64::INFINITY, f64::min);
        if to_fuselage.is_finite() {
            residuals.push(gap_row("nacelle_clear_of_fuselage", to_fuselage, role));
        }
        let mut between = f64::INFINITY;
        for (i, first) in engines.iter().enumerate() {
            for second in &engines[i + 1..] {
                if let Some(gap) = body_gap_m(first, second) {
                    between = between.min(gap);
                }
            }
        }
        if between.is_finite() {
            residuals.push(gap_row("nacelle_clear_of_nacelle", between, role));
        }
    }
    if let Some(detached) = tail_root_detachment_m(plane) {
        residuals.push(gap_row("tail_root_attached", -detached, role));
    }
    // An undeclared clearance is judged at both ends of the class sample and
    // each row keeps the smaller gap: no single end is the conservative one
    // for every row (the tail gap shrinks with a larger clearance, the nacelle,
    // wing-tip and propeller gaps with a smaller one).
    let mut ground_rows: Vec<ConstraintResidual> = Vec::new();
    for &basis in alas_mass::stations::ground_plane_bases(&config.landing_gear) {
        for row in ground_rows_on(outcome, config, basis, role) {
            match ground_rows.iter_mut().find(|kept| kept.id == row.id) {
                Some(kept) if row.actual < kept.actual => *kept = row,
                Some(_) => {}
                None => ground_rows.push(row),
            }
        }
    }
    residuals.extend(ground_rows);
    residuals
}

/// The ground-pose clearance rows on one ground-plane basis.
fn ground_rows_on(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    basis: alas_mass::stations::GroundPlaneBasis,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let plane = &outcome.plane;
    let mut residuals = Vec::new();
    if let Some(pose) = ground_pose(plane, config, basis) {
        let rise = flight_rise_fn(plane, config);
        let level = GroundPose {
            pitch_rad: 0.0,
            ..pose
        };
        let lowest = |first: Option<f64>, second: Option<f64>| match (first, second) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if let Some(gap) = lowest(
            wing_clearance_m(plane, &level, &rise),
            wing_clearance_m(plane, &pose, &rise),
        ) {
            residuals.push(gap_row("wing_tip_ground_clearance_in_roll", gap, role));
        }
        let hold = config.geometry.wing.nacelles_hold_ground_height;
        let nacelle_rise = |y: f64| if hold { 0.0 } else { rise(y) };
        // A turboprop nacelle is faired into the wing: overlap is the design.
        let turboprop = matches!(
            config.geometry.engine.active_model(),
            Ok(alas_config::ActiveEngineModel::Turboprop(_))
        );
        if let Some(gap) = nacelle_wing_gap_m(plane, &rise, &nacelle_rise).filter(|_| !turboprop) {
            residuals.push(gap_row("nacelle_clear_of_wing", gap, role));
        }
        if let Some(gap) = lowest(
            nacelle_clearance_m(plane, &level, &nacelle_rise),
            nacelle_clearance_m(plane, &pose, &nacelle_rise),
        ) {
            residuals.push(gap_row("nacelle_ground_clearance_in_roll", gap, role));
        }
        if let Some(gap) = gear::main_gear_nacelle_gap_m(outcome, config, &pose, &nacelle_rise) {
            residuals.push(gap_row("main_gear_clear_of_nacelle", gap, role));
        }
        residuals.extend(propeller::propeller_rows(outcome, config, &pose, role));
        if let Some(gap) = lowest(
            tail_clearance_m(plane, &level),
            tail_clearance_m(plane, &pose),
        ) {
            residuals.push(gap_row("tail_ground_clearance_in_roll", gap, role));
        }
    }
    residuals
}

/// Flight-shape height to remove from the wing at lateral station `|y|`, m:
/// the declared static-to-1 g tip rise, linear in `y` over the semispan.
pub(super) fn flight_rise_fn(plane: &Airplane, config: &AlasConfig) -> impl Fn(f64) -> f64 {
    let semi_span_m = plane
        .wings
        .first()
        .and_then(|wing| wing.xsecs.last())
        .map_or(0.0, |tip| tip.xyz_le[1].abs());
    let tip_rise_m = config
        .geometry
        .wing
        .flight_tip_rise_m(semi_span_m)
        .unwrap_or(0.0);
    move |y: f64| {
        if semi_span_m > 0.0 {
            tip_rise_m * (y / semi_span_m).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The resolved gear stations of the built airframe, m aft of the nose.
fn gear_stations(
    plane: &Airplane,
    config: &AlasConfig,
) -> Option<alas_config::LandingGearStationPositions> {
    let fuselage = plane.fuselages.first()?;
    let nose_x = fuselage.xsecs.first()?.xyz_c[0];
    let tail_x = fuselage.xsecs.last()?.xyz_c[0];
    let length = tail_x - nose_x;
    let frame = plane.mac_frame()?;
    Some(config.landing_gear.resolved_station_positions(
        nose_x + length * config.mass_model.nlg_x_fraction,
        frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
        nose_x,
        length,
    ))
}

/// The ground pose: ground plane, main-gear station, outboard wheel, the
/// tail-scrape pitch attitude and the 5 degree roll.
fn ground_pose(
    plane: &Airplane,
    config: &AlasConfig,
    basis: alas_mass::stations::GroundPlaneBasis,
) -> Option<GroundPose> {
    let fuselage = plane.fuselages.first()?;
    let stations = gear_stations(plane, config)?;
    let sample = alas_mass::stations::clearance_sample(
        plane.wings.first()?,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    let ground_z_m = alas_mass::stations::ground_plane_z_for(
        basis,
        sample,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    // Pitched about the aft-most main station, the envelope's scrape pivot.
    let pivot_x_m = stations.rotation_pivot_x_m();
    let lower = crate::envelope::lower_contour_aft_of(plane, fuselage, pivot_x_m);
    let pitch_deg =
        alas_perf::landing_gear::geometry::tail_scrape_angle_deg(&lower, pivot_x_m, ground_z_m)
            .filter(|angle| angle.is_finite())
            .unwrap_or(config.landing_gear.required_rotation_angle_deg);
    let y_outboard_wheel_m =
        0.5 * config.geometry.fuselage.diameter_m * config.landing_gear.track_diameter_factor;
    let pose = GroundPose {
        ground_z_m,
        x_main_gear_m: pivot_x_m,
        y_outboard_wheel_m,
        pitch_rad: pitch_deg.to_radians(),
        roll_rad: GROUND_CLEARANCE_ROLL_DEG.to_radians(),
    };
    [
        pose.ground_z_m,
        pose.x_main_gear_m,
        pose.y_outboard_wheel_m,
        pose.pitch_rad,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(pose)
}

mod gear;
mod propeller;
mod sections;
mod surfaces;
pub(crate) use surfaces::{
    nacelle_clearance_m, nacelle_wing_gap_m, tail_clearance_m, tail_root_detachment_m,
    wing_clearance_m,
};

#[cfg(test)]
mod tests;
