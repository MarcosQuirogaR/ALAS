// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// generate_vortex_distribution.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Laying panels on a vehicle: VLM steps 1 through 10.
//!
//! Each lifting surface is cut into `n_sw` spanwise strips and each strip
//! into `n_cw` chordwise panels. Every panel gets four corners, a bound
//! vortex a quarter of the way back, and a control point three quarters of
//! the way back, in that panel's own local chord, and then the whole strip
//! is rotated about its leading edge by the local incidence, offset by the
//! accumulated sweep and dihedral, and moved to the wing's origin.
//!
//! # Scope
//!
//! Wings only. `generate_fuselage_and_nacelle_vortex_distribution` runs on
//! every fuselage and nacelle and then, with `model_geometry` false, appends
//! none of what it computed: with `Fidelity_Zero`'s `model_fuselage` and
//! `model_nacelle` both false the whole function is a no-op on the
//! distribution, so it is absent here rather than translated and never
//! called.
//!
//! Left untranslated for the same reason: the camber line, because no wing
//! the mission runner builds carries an `Airfoil` and the fallback camber is
//! thirty zeros, every `np.interp` against it returns zero, so every
//! `z_c_*` term is identically zero and the twist pivot is the leading edge
//! exactly; the control-surface chord cuts, which reduce to the identity
//! interpolation when no surface is discretized; and the quaternion hinge
//! rotation, which only a control surface or an all-moving surface reaches.
//! The corner grids `VD.X`/`Y`/`Z`, the strip centre chords `VD.CS`, the
//! spanwise stations `VD.Y_SW` and the strip widths `VD.DY` are computed and
//! stored upstream and read by nothing on this path; they belong to the
//! figures, which are P11.
//!
//! # Where the rounding is
//!
//! Every corner is computed in `f64` and stored as `f32`, which is what
//! `dtype=precision` does upstream. The rounding therefore happens once, on
//! the way into [`VortexDistribution`], and every later stage reads the
//! rounded value. [`super::types`] says why that has to be reproduced.

mod surface;

use super::types::{VlmError, VlmGeometry, VlmSettings, VlmWing, VortexDistribution};
use super::wings::span_breaks;
use surface::{Section, Strip, Surface};

/// Panel the whole vehicle.
pub fn generate(
    geometry: &VlmGeometry,
    settings: &VlmSettings,
) -> Result<VortexDistribution, VlmError> {
    if geometry.wings.is_empty() {
        return Err(VlmError::NoWings);
    }

    let mut vd = VortexDistribution::default();
    for wing in &geometry.wings {
        generate_wing(&mut vd, wing, settings)?;
    }
    postprocess(&mut vd);
    Ok(vd)
}

/// One wing: its own side, and then its mirror image if it is symmetric.
fn generate_wing(
    vd: &mut VortexDistribution,
    wing: &VlmWing,
    settings: &VlmSettings,
) -> Result<(), VlmError> {
    let n_sw = settings.number_spanwise_vortices;
    let n_cw = settings.number_chordwise_vortices;

    // A symmetric wing is panelized on its half span, twice.
    let span = if wing.symmetric {
        wing.span_projected_m * 0.5
    } else {
        wing.span_projected_m
    };

    let breaks = span_breaks(wing);
    let break_spans = [
        breaks[0].span_fraction * span,
        breaks[1].span_fraction * span,
    ];
    let section_span = break_spans[1] - break_spans[0];
    let section_area = 0.5 * (breaks[0].local_chord_m + breaks[1].local_chord_m) * section_span;

    let y_coordinates =
        spanwise_stations(span, n_sw, settings.spanwise_cosine_spacing, &break_spans).ok_or_else(
            || VlmError::NotEnoughStations {
                tag: wing.tag.clone(),
                breaks: breaks.len(),
                stations: n_sw + 1,
            },
        )?;

    // The last break's outboard sweep is replaced by zero, which is what
    // makes the `1e-8` placeholder `make_VLM_wings` writes there unreachable.
    let break_sweep = breaks[0].sweep_outboard_le_rad;
    let break_dihedral = breaks[0].dihedral_outboard_rad;
    let chord_ratio = (breaks[1].local_chord_m - breaks[0].local_chord_m) / section_span;
    let twist_ratio = (breaks[1].twist_rad - breaks[0].twist_rad) / section_span;

    // `-sign(dihedral - pi/2)`: a surface whose sections lean past the
    // vertical is reflected the other way when its `y` and `z` are swapped.
    let inverted = -(break_dihedral - std::f64::consts::FRAC_PI_2).signum();

    let signs: &[f64] = if wing.symmetric { &[1.0, -1.0] } else { &[1.0] };
    for &sym_sign in signs {
        let (origin_x, origin_y, origin_z) = if wing.vertical {
            (
                wing.origin_m[0],
                wing.origin_m[1],
                wing.origin_m[2] * sym_sign,
            )
        } else {
            (
                wing.origin_m[0],
                wing.origin_m[1] * sym_sign,
                wing.origin_m[2],
            )
        };

        let mut surface = Surface::new(n_cw, n_sw);
        for idx_y in 0..n_sw {
            let y_a = y_coordinates[idx_y];
            let y_b = y_coordinates[idx_y + 1];
            let strip = Strip {
                eta_a: y_a - break_spans[0],
                eta_b: y_b - break_spans[0],
                eta: y_b - (y_b - y_a) * 0.5 - break_spans[0],
                y_a: y_a * sym_sign,
                y_b: y_b * sym_sign,
                y_c: (y_b - (y_b - y_a) * 0.5) * sym_sign,
            };
            let section = Section {
                root_chord_m: breaks[0].local_chord_m,
                root_twist_rad: breaks[0].twist_rad,
                chord_ratio,
                twist_ratio,
                sweep_le_rad: break_sweep,
                dihedral_rad: break_dihedral,
                x_offset_m: breaks[0].x_offset_m,
                z_offset_m: breaks[0].dih_offset_m,
            };
            surface.lay_strip(idx_y, n_cw, &strip, &section, wing.vertical, inverted);
        }

        surface.shift_to_origin(origin_x, origin_y, origin_z);
        surface.append_to(vd, n_cw, n_sw);

        vd.wing_areas_m2.push(0.0f32 + section_area as f32);
        vd.vortex_lift.push(wing.vortex_lift);
    }

    vd.symmetric_wings.push(wing.symmetric);
    Ok(())
}

/// The strip edges in wing-local coordinates, with the section breaks
/// snapped onto the nearest of them.
///
/// The snapping is upstream's and matters: with cosine spacing the outermost
/// station is `span * cos(pi/2)`, which is six parts in `1e17` of the span
/// rather than zero, and the innermost is the span to within rounding. Both
/// are overwritten with the exact break location, so the root strip really
/// does start at the root and the tip strip really does end at the tip.
fn spanwise_stations(
    span: f64,
    n_sw: usize,
    cosine_spacing: bool,
    break_spans: &[f64],
) -> Option<Vec<f64>> {
    if n_sw + 1 < break_spans.len() {
        return None;
    }

    let mut stations: Vec<f64> = if cosine_spacing {
        // `np.linspace(n_sw + 1, 0, n_sw + 1)`, which counts *down*, so the
        // first angular station is a right angle and the last is zero.
        let count = n_sw + 1;
        let step = -((n_sw + 1) as f64) / (count - 1) as f64;
        (0..count)
            .map(|i| {
                let n = if i == count - 1 {
                    0.0
                } else {
                    (n_sw + 1) as f64 + step * i as f64
                };
                span * (n * std::f64::consts::FRAC_PI_2 / (n_sw + 1) as f64).cos()
            })
            .collect()
    } else {
        let step = span / n_sw as f64;
        (0..=n_sw)
            .map(|i| if i == n_sw { span } else { step * i as f64 })
            .collect()
    };

    // `argmin` over `|y - y_req| + shifted`, where a station already claimed
    // carries an infinite penalty so two breaks cannot land on one station.
    let mut claimed = vec![false; stations.len()];
    for &required in break_spans {
        let index = (0..stations.len())
            .filter(|&i| !claimed[i])
            .min_by(|&i, &j| {
                (stations[i] - required)
                    .abs()
                    .total_cmp(&(stations[j] - required).abs())
            })?;
        claimed[index] = true;
        stations[index] = required;
    }

    stations.sort_by(f64::total_cmp);
    Some(stations)
}

/// Step 10: the quantities every later stage reads off the finished
/// panelization.
fn postprocess(vd: &mut VortexDistribution) {
    let p = &vd.panels;
    let n = vd.n_cp;

    vd.panel_areas_m2 = (0..n)
        .map(|i| {
            let p1p2 = [
                p.xb1[i] - p.xa1[i],
                p.yb1[i] - p.ya1[i],
                p.zb1[i] - p.za1[i],
            ];
            let p1p3 = [
                p.xa2[i] - p.xa1[i],
                p.ya2[i] - p.ya1[i],
                p.za2[i] - p.za1[i],
            ];
            let p2p3 = [
                p.xa2[i] - p.xb1[i],
                p.ya2[i] - p.yb1[i],
                p.za2[i] - p.zb1[i],
            ];
            let p2p4 = [
                p.xb2[i] - p.xb1[i],
                p.yb2[i] - p.yb1[i],
                p.zb2[i] - p.zb1[i],
            ];
            0.5 * (norm(cross(p1p2, p1p3)) + norm(cross(p2p3, p2p4)))
        })
        .collect();

    vd.normals = (0..n)
        .map(|i| {
            let p1p2 = [
                p.xb1[i] - p.xa1[i],
                p.yb1[i] - p.ya1[i],
                p.zb1[i] - p.za1[i],
            ];
            let p1p3 = [
                p.xa2[i] - p.xa1[i],
                p.ya2[i] - p.ya1[i],
                p.za2[i] - p.za1[i],
            ];
            let c = cross(p1p2, p1p3);
            let magnitude = norm(c);
            let unit = [c[0] / magnitude, c[1] / magnitude, c[2] / magnitude];
            // A panel wound the other way would otherwise present an inward
            // normal, and the boundary condition would come out with the
            // wrong sign on that panel alone.
            if unit[2] < 0.0 {
                [-unit[0], -unit[1], -unit[2]]
            } else {
                unit
            }
        })
        .collect();

    vd.slope = (0..n)
        .map(|i| {
            let x1c = (p.xa1[i] + p.xb1[i]) / 2.0;
            let x2c = (p.xa2[i] + p.xb2[i]) / 2.0;
            let z1c = (p.za1[i] + p.zb1[i]) / 2.0;
            let z2c = (p.za2[i] + p.zb2[i]) / 2.0;
            (z2c - z1c) / (x2c - x1c)
        })
        .collect();

    let leading = vd.leading_edge_panels();
    vd.sle = leading.iter().map(|&i| vd.slope[i]).collect();
    vd.d = leading
        .iter()
        .map(|&i| {
            let dy = p.yah[i] - p.ybh[i];
            let dz = p.zah[i] - p.zbh[i];
            (dy * dy + dz * dz).sqrt()
        })
        .collect();
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
