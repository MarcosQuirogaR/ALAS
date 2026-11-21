// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The yaw-balance data of a built airplane: fin, rudder, engine arm, wing
//! and fuselage, in the form `alas_perf::performance::yaw_balance` consumes.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::{Airplane, HORIZONTAL_STABILIZER_NAME};
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_perf::performance::failed_engine_drag::YawBalanceGeometry;
use alas_perf::performance::fin_aspect_ratio::{fin_effective_aspect_ratio, FinPanel};
use alas_perf::performance::yaw_balance::{
    fin_sidewash_factor, fuselage_cn_beta_per_rad, span_factor, FuselageYawGeometry,
    YawControlGeometry,
};
use alas_stab::static_stability::fin_sideforce_factor;

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
    // The yaw balance is a handbook method stated on the theoretical fin planform.
    let fin = fin.reference_planform();
    let fin_arm_m = fin.aerodynamic_center(0.25)[0] - frame.cg_x_m;
    let controls = &config.control_surfaces;
    let rudder_area_m2 = fin.area()
        * controls.rudder_chord_fraction
        * (controls.rudder_span_end_frac - controls.rudder_span_start_frac);
    let engine_arm_m = config
        .geometry
        .engine
        .spanwise_positions_m
        .iter()
        .fold(0.0_f64, |arm, y| arm.max(y.abs()));
    // The fin is the DATCOM panel to the body centreline, with the effective
    // aspect ratio of the body and horizontal-tail end plates that Torenbeek
    // asks of Eqs. (G-84) and (G-86) and of the fin lift-curve slope; without
    // a body there is no end plate and the planform stands as drawn.
    let panel = datcom_panel(frame.plane, &fin);
    let (fin_area_m2, fin_effective_aspect_ratio, fin_taper_ratio) = match panel {
        Some(panel) => (
            panel.area_m2,
            fin_effective_aspect_ratio(panel)?,
            panel.taper_ratio,
        ),
        None => (fin.area(), fin.aspect_ratio(), fin.taper_ratio()),
    };
    let balance = YawBalanceGeometry {
        engine_arm_m,
        fin_arm_m,
        fin_area_m2,
        fin_effective_aspect_ratio,
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
    // DATCOM's empirical k(b_v / 2 r_1) scales the fin's sideslip force
    // with the sidewash; without a body panel there is no body to correct.
    let body_factor = panel.map_or(1.0, |panel| {
        fin_sideforce_factor(panel.span_m / panel.body_depth_m)
    });
    let sidewash = body_factor * wing_sidewash(plane, fin_area_m2)?;
    // The rudder's span stations on the panel the fin lift is stated on: the
    // exposed fin carries the rudder from `rudder_span_start_frac` to
    // `rudder_span_end_frac` of its own span, and the panel runs on to the
    // body centreline below it (Roskam Part VI Eq. 10.123a, Fig. 10.13).
    let (exposed_span_m, panel_span_m) = {
        let (root, tip) = (fin.xsecs.first()?, fin.xsecs.last()?);
        let exposed = tip.xyz_le[2] - root.xyz_le[2];
        (exposed, panel.map_or(exposed, |panel| panel.span_m))
    };
    let below_m = panel_span_m - exposed_span_m;
    let station = |fraction: f64| (below_m + fraction * exposed_span_m) / panel_span_m;
    let rudder_span_factor = span_factor(
        station(controls.rudder_span_start_frac),
        station(controls.rudder_span_end_frac),
        fin_taper_ratio,
    )?;
    let control = fuselage_geometry(plane, frame.cg_x_m)
        .and_then(|fuselage| fuselage_cn_beta_per_rad(fuselage, plane.s_ref, plane.b_ref))
        .map(|fuselage_cn_beta| fuselage_cn_beta + nacelle_cn_beta_per_rad(plane))
        .zip(airframe_side_force_per_rad(plane))
        .map(
            |(cn_beta_without_fin_per_rad, side_force_without_fin_per_rad)| YawControlGeometry {
                side_force_without_fin_per_rad,
                balance,
                fin_taper_ratio,
                rudder_chord_ratio: controls.rudder_chord_fraction,
                rudder_span_factor,
                rudder_max_deflection_rad: controls.rudder_max_deflection_deg.to_radians(),
                wing_area_m2: plane.s_ref,
                wing_span_m: plane.b_ref,
                cn_beta_without_fin_per_rad,
                fin_sidewash_factor: sidewash,
            },
        );
    Some(YawModel { balance, control })
}

/// The fin sidewash factor of [`fin_sidewash_factor`] from the built
/// airplane: the main wing's reference area, aspect ratio and quarter-chord
/// sweep, the height of its root quarter chord above the main body's
/// centreline and the body depth at that station. Without a body there is
/// no fuselage sidewash: the factor is 1.
fn wing_sidewash(plane: &Airplane, fin_area_to_centreline_m2: f64) -> Option<f64> {
    let wing = plane.main_wing()?.reference_planform();
    let root = wing.xsecs.first()?;
    let x_quarter = root.xyz_le[0] + 0.25 * root.chord;
    let Some(body) = main_body(plane) else {
        return Some(1.0);
    };
    let centres: Vec<(f64, f64)> = body
        .xsecs
        .iter()
        .map(|s| (s.xyz_c[0], s.xyz_c[2]))
        .collect();
    let heights: Vec<(f64, f64)> = body.xsecs.iter().map(|s| (s.xyz_c[0], s.height)).collect();
    let (centre_z, depth) = (
        interpolate(&centres, x_quarter)?,
        interpolate(&heights, x_quarter)?,
    );
    fin_sidewash_factor(
        fin_area_to_centreline_m2,
        plane.s_ref,
        wing.mean_sweep_angle(0.25).to_radians(),
        plane.b_ref * plane.b_ref / plane.s_ref,
        root.xyz_le[2] - centre_z,
        depth,
    )
}

/// The fin as USAF DATCOM (1978) Sec. 5.3.1.1, Method 1, measures it: the
/// reference planform continued to the body centreline (span, area and
/// taper ratio), the body depth `2 r_1` and centreline height averaged over
/// the fin root chord (the "region of the vertical tail" of Fig.
/// 5.3.1.1-22a), and the horizontal tail's area, root height above the
/// centreline over the span, and aerodynamic centre aft of the fin leading
/// edge over the fin chord at that height (Fig. 5.3.1.1-22b). `None` without
/// a body or for a degenerate fin.
fn datcom_panel(plane: &Airplane, fin: &Wing) -> Option<FinPanel> {
    let (root, tip) = (fin.xsecs.first()?, fin.xsecs.last()?);
    let (z_root, z_tip) = (root.xyz_le[2], tip.xyz_le[2]);
    // A tip not above the root, or not comparable (NaN), is a degenerate fin.
    if z_tip.partial_cmp(&z_root) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let body = main_body(plane)?;
    let stations: Vec<(f64, f64)> = body
        .xsecs
        .iter()
        .map(|s| (s.xyz_c[0], s.xyz_c[2]))
        .collect();
    let heights: Vec<(f64, f64)> = body.xsecs.iter().map(|s| (s.xyz_c[0], s.height)).collect();
    // Root-chord stations, kept on the body where the chord overhangs it.
    let (nose, end) = (stations.first()?.0, stations.last()?.0);
    let samples: Vec<f64> = (0..=8)
        .map(|i| (root.xyz_le[0] + root.chord * f64::from(i) / 8.0).clamp(nose, end))
        .collect();
    let mean = |table: &[(f64, f64)]| -> Option<f64> {
        let values: Option<Vec<f64>> = samples.iter().map(|x| interpolate(table, *x)).collect();
        values.map(|v| v.iter().sum::<f64>() / v.len() as f64)
    };
    let (centreline_z, body_depth_m) = (mean(&stations)?, mean(&heights)?);
    // Leading edge and chord along the straight root-to-tip panel.
    let along = |z: f64| (z - z_root) / (z_tip - z_root);
    let chord_at = |z: f64| root.chord + (tip.chord - root.chord) * along(z);
    let le_x_at = |z: f64| root.xyz_le[0] + (tip.xyz_le[0] - root.xyz_le[0]) * along(z);
    let span_m = z_tip - centreline_z;
    let centreline_chord = chord_at(centreline_z);
    let extension_m2 = 0.5 * (centreline_chord + root.chord) * (z_root - centreline_z).max(0.0);
    let area_m2 = fin.area() + extension_m2;
    let horizontal_tail = plane
        .wings
        .iter()
        .find(|wing| wing.name == HORIZONTAL_STABILIZER_NAME)
        .map(|tail| {
            let tail = tail.reference_planform();
            let z_tail = tail.xsecs[0].xyz_le[2];
            let position = (tail.aerodynamic_center(0.25)[0] - le_x_at(z_tail)) / chord_at(z_tail);
            (tail.area(), (z_tail - centreline_z) / span_m, position)
        });
    let panel = FinPanel {
        span_m,
        area_m2,
        taper_ratio: tip.chord / centreline_chord,
        body_depth_m,
        horizontal_tail,
    };
    let usable = [span_m, area_m2, centreline_chord, body_depth_m]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0);
    usable.then_some(panel)
}

/// Yaw-moment slope of the engine nacelles per rad on `S b`, summed over the
/// built nacelle bodies: Raymer, *Aircraft Design: A Conceptual Approach*,
/// 6th ed., Sec. 16.4.3, Eq. (16.50), `-1.3 (volume / (S_w b)) (D / W)`
/// per rad, which the text states for "the fuselage or nacelle". The volume
/// integrates the elliptic section `pi/4 w h` along the stations; `D / W` is
/// the largest height over the largest width. Destabilizing (negative). The
/// wing term is zero for preliminary design (Roskam Part VI Eq. 10.41,
/// p. 397: important only at high angle of attack; the DATCOM clean-wing
/// expression of Raymer Eq. 16.44 does not cover the flapped approach wing).
fn nacelle_cn_beta_per_rad(plane: &Airplane) -> f64 {
    let (area, span) = (plane.s_ref, plane.b_ref);
    if !(area > 0.0 && span > 0.0) {
        return 0.0;
    }
    plane
        .fuselages
        .iter()
        .filter(|body| body.name.starts_with("Nacelle"))
        .map(|body| {
            let volume: f64 = body
                .xsecs
                .windows(2)
                .map(|pair| {
                    let section = |s: &alas_geom::aircraft::fuselage::FuselageXSec| {
                        0.25 * std::f64::consts::PI * s.width * s.height
                    };
                    0.5 * (section(&pair[0]) + section(&pair[1]))
                        * (pair[1].xyz_c[0] - pair[0].xyz_c[0]).abs()
                })
                .sum();
            let depth = body.xsecs.iter().fold(0.0_f64, |m, s| m.max(s.height));
            let width = body.xsecs.iter().fold(0.0_f64, |m, s| m.max(s.width));
            if volume > 0.0 && depth > 0.0 && width > 0.0 {
                -1.3 * volume / (area * span) * (depth / width)
            } else {
                0.0
            }
        })
        .sum()
}

/// Side-force slope of the airframe without the fin, `C_Y_beta` per rad on
/// `S`, Roskam, *Airplane Design* Part VI, Sec. 10.2.4.1, pp. 383-385:
/// the wing term `-0.00573 |Gamma|` (Eq. 10.26, dihedral `Gamma` in deg,
/// from the reference planform's root and tip leading edges) plus the body
/// term `-2 K_i S_0 / S` (Eq. 10.27). `S_0` is the body cross-section (the
/// elliptic `pi/4 w h` of the stations) at `x_0`, where the flow stops
/// being potential: `x_0 / l_f = 0.378 + 0.527 x_1 / l_f` (Fig. 10.11) with
/// `x_1` the station of the most negative `dS/dx`. `K_i` (Fig. 10.8) is
/// linear in `z_w / (d_f / 2)`: 1.85 for a high wing (-1), 1.0 mid (0), 1.5
/// low (+1), `z_w` the depth of the wing root quarter chord below the body
/// centreline and `d_f` the body height there. The section gives no
/// nacelle term, so nacelles are left out. `None` without a main wing or
/// body, or for degenerate stations.
fn airframe_side_force_per_rad(plane: &Airplane) -> Option<f64> {
    let wing = plane.main_wing()?.reference_planform();
    let (root, tip) = (wing.xsecs.first()?, wing.xsecs.last()?);
    let dihedral_deg = (tip.xyz_le[2] - root.xyz_le[2])
        .atan2(tip.xyz_le[1] - root.xyz_le[1])
        .to_degrees();
    let wing_term = -0.00573 * dihedral_deg.abs();
    let body = main_body(plane)?;
    let areas: Vec<(f64, f64)> = body
        .xsecs
        .iter()
        .map(|s| (s.xyz_c[0], 0.25 * std::f64::consts::PI * s.width * s.height))
        .collect();
    let (nose, end) = (areas.first()?.0, areas.last()?.0);
    let length_m = end - nose;
    if !(length_m.is_finite() && length_m > 0.0 && plane.s_ref > 0.0) {
        return None;
    }
    // The first segment of the steepest fall of the cross-section.
    let x_1 = areas
        .windows(2)
        .filter(|pair| pair[1].0 > pair[0].0)
        .map(|pair| {
            let slope = (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0);
            (slope, 0.5 * (pair[0].0 + pair[1].0))
        })
        .fold(None, |best: Option<(f64, f64)>, item| match best {
            Some(b) if b.0 <= item.0 => Some(b),
            _ => Some(item),
        })?
        .1;
    let x_0 = nose + length_m * (0.378 + 0.527 * (x_1 - nose) / length_m);
    let s_0 = interpolate(&areas, x_0.clamp(nose, end))?;
    let x_quarter = root.xyz_le[0] + 0.25 * root.chord;
    let centres: Vec<(f64, f64)> = body
        .xsecs
        .iter()
        .map(|s| (s.xyz_c[0], s.xyz_c[2]))
        .collect();
    let heights: Vec<(f64, f64)> = body.xsecs.iter().map(|s| (s.xyz_c[0], s.height)).collect();
    let (centre_z, depth) = (
        interpolate(&centres, x_quarter)?,
        interpolate(&heights, x_quarter)?,
    );
    if !(depth > 0.0) {
        return None;
    }
    let below = ((centre_z - root.xyz_le[2]) / (0.5 * depth)).clamp(-1.0, 1.0);
    let k_i = if below >= 0.0 {
        1.0 + 0.5 * below
    } else {
        1.0 - 0.85 * below
    };
    let body_term = -2.0 * k_i * s_0 / plane.s_ref;
    let total = wing_term + body_term;
    (total.is_finite() && total <= 0.0).then_some(total)
}

/// The main fuselage: the longest body (nacelles and pods are shorter).
fn main_body(plane: &Airplane) -> Option<&Fuselage> {
    plane.fuselages.iter().max_by(|a, b| {
        let length = |f: &Fuselage| {
            f.xsecs
                .last()
                .zip(f.xsecs.first())
                .map_or(0.0, |(end, start)| end.xyz_c[0] - start.xyz_c[0])
        };
        length(a).total_cmp(&length(b))
    })
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
    let body = main_body(plane)?;
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
