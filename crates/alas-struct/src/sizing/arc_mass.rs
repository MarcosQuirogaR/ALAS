// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Production mass correction for cap and web centerline arc length.
//!
//! Running masses use projected Y (kg/m), whereas material follows the 3-D
//! streamwise spar lines. Segment length and trapezoidal area quadrature are
//! conservative by construction. The analytical section geometry remains a
//! streamwise approximation; it is not the perpendicular-rib FE geometry.
//! Skin and rib totals retain their sized convention, redistributed with
//! exactly conserved totals. Complete FE skin/rib/section mass is checked
//! independently for a finite physical inventory. Its disagreement with an
//! empirical mass estimate is a diagnostic, not an acceptance constraint.

use super::WingboxSizing;
use alas_config::materials::MaterialSpec;
use alas_geom::wing_structure::WingStructureGeometry;

fn integrate(values: &[f64], y: &[f64]) -> f64 {
    values
        .windows(2)
        .zip(y.windows(2))
        .map(|(v, x)| 0.5 * (v[0] + v[1]) * (x[1] - x[0]))
        .sum()
}

/// Nodal ds/dY whose trapezoid integral equals segment arc-length integration
/// even at a sweep break; no derivative smoothing across the break is used.
fn stretch(y: &[f64], x: &[f64], z: &[f64]) -> Vec<f64> {
    let mut lengths = vec![0.0; y.len()];
    let mut widths = vec![0.0; y.len()];
    for j in 1..y.len() {
        let dy = y[j] - y[j - 1];
        let ds = dy.hypot(x[j] - x[j - 1]).hypot(z[j] - z[j - 1]);
        for i in [j - 1, j] {
            lengths[i] += ds;
            widths[i] += dy;
        }
    }
    lengths
        .into_iter()
        .zip(widths)
        .map(|(ds, dy)| ds / dy)
        .collect()
}

pub(super) fn spar_stretches(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    spar_index: usize,
) -> (Vec<f64>, Vec<f64>) {
    let spar = &sizing.spars[spar_index];
    let count = sizing.y_stations.len();
    let mut x = Vec::with_capacity(count);
    let mut upper = Vec::with_capacity(count);
    let mut lower = Vec::with_capacity(count);
    let mut middle = Vec::with_capacity(count);
    for (j, &y) in sizing.y_stations.iter().enumerate() {
        let eta = y / wsg.semi_span;
        let chord = wsg.local_chord(eta);
        let z = wsg.z_le(eta);
        let (zu, zl) = wsg.airfoil_zu_zl(eta, spar.chord_fraction);
        x.push(wsg.x_le(eta) + spar.chord_fraction * chord);
        upper.push(z + zu * chord - 0.5 * spar.t_cap[j]);
        lower.push(z + zl * chord + 0.5 * spar.t_cap[j]);
        middle.push(z + 0.5 * (zu + zl) * chord);
    }
    let upper_stretch = stretch(&sizing.y_stations, &x, &upper);
    let lower_stretch = stretch(&sizing.y_stations, &x, &lower);
    let cap_pair = upper_stretch
        .into_iter()
        .zip(lower_stretch)
        .map(|(a, b)| a + b)
        .collect();
    (cap_pair, stretch(&sizing.y_stations, &x, &middle))
}

/// Update semi-wing cap/web mass and return the complete, mass-conserving
/// structural running density used for inertial relief and modal response.
pub(super) fn update(
    wsg: &WingStructureGeometry,
    sizing: &mut WingboxSizing,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> Vec<f64> {
    let count = sizing.y_stations.len();
    if count < 2
        || sizing.chord.len() != count
        || sizing.spars.iter().any(|spar| {
            spar.a_cap.len() != count || spar.h.len() != count || spar.t_cap.len() != count
        })
        || sizing
            .y_stations
            .windows(2)
            .any(|pair| !pair[0].is_finite() || !pair[1].is_finite() || pair[1] <= pair[0])
    {
        sizing.total_mass_kg = f64::NAN;
        return vec![f64::NAN; count];
    }
    let mut caps = vec![0.0; count];
    let mut webs = vec![0.0; count];
    for (i, spar) in sizing.spars.iter().enumerate() {
        let (cap_pair_stretch, web_stretch) = spar_stretches(wsg, sizing, i);
        for j in 0..count {
            caps[j] += cap.rho_kg_m3 * spar.a_cap[j] * cap_pair_stretch[j];
            webs[j] += web.rho_kg_m3 * spar.t_web * spar.h[j] * web_stretch[j];
        }
    }
    sizing.mass_breakdown_kg.spar_caps = integrate(&caps, &sizing.y_stations);
    sizing.mass_breakdown_kg.spar_webs = integrate(&webs, &sizing.y_stations);
    sizing.total_mass_kg = sizing.mass_breakdown_kg.spar_caps
        + sizing.mass_breakdown_kg.spar_webs
        + sizing.mass_breakdown_kg.skin
        + sizing.mass_breakdown_kg.ribs;
    let skin_scale = sizing.mass_breakdown_kg.skin / integrate(&sizing.chord, &sizing.y_stations);
    let rib_running =
        sizing.mass_breakdown_kg.ribs / (sizing.y_stations[count - 1] - sizing.y_stations[0]);
    (0..count)
        .map(|j| caps[j] + webs[j] + skin_scale * sizing.chord[j] + rib_running)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swept_line_mass_uses_physical_length_and_preserves_taper() {
        let y = [0.0, 1.0, 3.0, 8.0];
        let sweep = 40.0_f64.to_radians();
        let x: Vec<_> = y.iter().map(|value| value * sweep.tan()).collect();
        let stretch = stretch(&y, &x, &[0.0; 4]);
        let area = [0.20, 0.18, 0.12, 0.02];
        let density: Vec<_> = area
            .iter()
            .zip(stretch)
            .map(|(a, s)| 2700.0 * a * s)
            .collect();
        let mass = integrate(&density, &y);
        let expected = 2700.0 * integrate(&area, &y) / sweep.cos();
        assert!((mass - expected).abs() < 1.0e-9);
    }

    #[test]
    fn unswept_straight_line_has_unit_stretch() {
        assert_eq!(
            stretch(&[0.0, 1.0, 3.0], &[0.0; 3], &[0.0; 3]),
            vec![1.0; 3]
        );
    }
}
