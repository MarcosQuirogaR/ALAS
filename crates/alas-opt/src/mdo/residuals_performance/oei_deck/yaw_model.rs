// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The yaw-balance data of a built airplane: fin, rudder, engine arm, wing
//! and fuselage, in the form `alas_perf::performance::yaw_balance` consumes.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_perf::performance::failed_engine_drag::YawBalanceGeometry;
use alas_perf::performance::yaw_balance::{
    fuselage_cn_beta_per_rad, FuselageYawGeometry, YawControlGeometry,
};

use super::{YawAirframe, FIN_NAME};

/// The fin balance data, and the full yaw-control data when the fuselage
/// supplies the airframe yaw stiffness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct YawModel {
    /// Fin, rudder and engine arm of the asymmetric balance (Torenbeek G-84).
    pub balance: YawBalanceGeometry,
    /// Rudder authority and airframe stiffness for the trim and `VMC`.
    pub control: Option<YawControlGeometry>,
}

/// Build the model, or `None` when the airplane has no fin or the geometry
/// is degenerate.
pub(super) fn build(config: &AlasConfig, frame: YawAirframe<'_>) -> Option<YawModel> {
    let fin = frame
        .plane
        .wings
        .iter()
        .find(|wing| wing.name == FIN_NAME)?;
    let fin_area_m2 = fin.area();
    let aspect_ratio = fin.aspect_ratio();
    let fin_arm_m = fin.aerodynamic_center(0.25)[0] - frame.cg_x_m;
    let controls = &config.control_surfaces;
    let rudder_area_m2 = fin_area_m2
        * controls.rudder_chord_fraction
        * (controls.rudder_span_end_frac - controls.rudder_span_start_frac);
    let engine_arm_m = config
        .geometry
        .engine
        .spanwise_positions_m
        .iter()
        .fold(0.0_f64, |arm, y| arm.max(y.abs()));
    let balance = YawBalanceGeometry {
        engine_arm_m,
        fin_arm_m,
        fin_area_m2,
        // Geometric aspect ratio: no end-plate or T-tail gain is credited
        // (Torenbeek Fig. 2-26 puts the T-tail gain near 50 percent), which
        // keeps the induced term and the rudder demand conservative.
        fin_effective_aspect_ratio: aspect_ratio,
        rudder_area_m2,
        fin_sweep_rad: fin.mean_sweep_angle(0.25).to_radians(),
    };
    let usable = [
        balance.engine_arm_m,
        balance.fin_arm_m,
        balance.fin_area_m2,
        balance.fin_effective_aspect_ratio,
        balance.rudder_area_m2,
    ]
    .iter()
    .all(|value| value.is_finite() && *value > 0.0)
        && balance.fin_sweep_rad.is_finite();
    if !usable {
        return None;
    }
    let plane = frame.plane;
    let control = fuselage_geometry(plane, frame.cg_x_m)
        .and_then(|fuselage| fuselage_cn_beta_per_rad(fuselage, plane.s_ref, plane.b_ref))
        .map(|cn_beta_without_fin_per_rad| YawControlGeometry {
            balance,
            fin_taper_ratio: fin.taper_ratio(),
            rudder_chord_ratio: controls.rudder_chord_fraction,
            rudder_max_deflection_rad: controls.rudder_max_deflection_deg.to_radians(),
            wing_area_m2: plane.s_ref,
            wing_span_m: plane.b_ref,
            cn_beta_without_fin_per_rad,
        });
    Some(YawModel { balance, control })
}

/// Linear interpolation of `(x, value)` stations sorted by `x`.
fn interpolate(stations: &[(f64, f64)], x: f64) -> Option<f64> {
    let (first, last) = (stations.first()?, stations.last()?);
    if !(first.0 <= x && x <= last.0) {
        return None;
    }
    let upper = stations.iter().position(|(sx, _)| *sx >= x)?;
    if upper == 0 {
        return Some(first.1);
    }
    let (x0, v0) = stations[upper - 1];
    let (x1, v1) = stations[upper];
    Some(if x1 > x0 {
        v0 + (v1 - v0) * (x - x0) / (x1 - x0)
    } else {
        v1
    })
}

/// The main fuselage's yaw-moment data from its cross-section stations: the
/// longest body (nacelles and pods are shorter), side area as the integral
/// of the section height along the length, heights and widths at a quarter
/// and three quarters of the length (Torenbeek Fig. 9-25).
fn fuselage_geometry(plane: &Airplane, cg_x_m: f64) -> Option<FuselageYawGeometry> {
    let body = plane.fuselages.iter().max_by(|a, b| {
        let length = |f: &alas_geom::aircraft::fuselage::Fuselage| {
            f.xsecs
                .last()
                .zip(f.xsecs.first())
                .map_or(0.0, |(end, start)| end.xyz_c[0] - start.xyz_c[0])
        };
        length(a).total_cmp(&length(b))
    })?;
    let heights: Vec<(f64, f64)> = body.xsecs.iter().map(|s| (s.xyz_c[0], s.height)).collect();
    let widths: Vec<(f64, f64)> = body.xsecs.iter().map(|s| (s.xyz_c[0], s.width)).collect();
    let (nose, tail) = (heights.first()?.0, heights.last()?.0);
    let length_m = tail - nose;
    if !(length_m.is_finite() && length_m > 0.0) {
        return None;
    }
    let side_area_m2: f64 = heights
        .windows(2)
        .map(|pair| 0.5 * (pair[0].1 + pair[1].1) * (pair[1].0 - pair[0].0))
        .sum();
    let quarter = nose + 0.25 * length_m;
    let three_quarter = nose + 0.75 * length_m;
    Some(FuselageYawGeometry {
        length_m,
        cg_from_nose_m: cg_x_m - nose,
        side_area_m2,
        max_height_m: heights.iter().fold(0.0_f64, |m, (_, h)| m.max(*h)),
        height_quarter_m: interpolate(&heights, quarter)?,
        width_quarter_m: interpolate(&widths, quarter)?,
        height_three_quarter_m: interpolate(&heights, three_quarter)?,
        width_three_quarter_m: interpolate(&widths, three_quarter)?,
    })
}
