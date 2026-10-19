// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Swept-box bending: the beam lies along the swept elastic axis, not along
//! the streamwise cut.
//!
//! Sources, verified on the page: Torenbeek, *Advanced Aircraft Design*, 2013,
//! book pp. 327-328 (PDF 343-344): Eq. 11.11 `M_BL(y) = int (y' - y)/cos(L_ea)
//! dL(y')` (bending moment about the section normal to the elastic axis),
//! Eq. 11.15 `b_st = b/cos(L_ea)`, with `L_ea` "nearly equal to the mean sweep
//! angle of a straight line midway between the front and the aft spars";
//! Niu, *Airframe Structural Design*, book pp. 31-32 (PDF 37-38, rendered):
//! swept-wing bending `phi = int M/(E I) dy` and torsion `theta = int T/(G J)
//! dy` integrated along the elastic axis.
//!
//! Derived here (not in either book): with section stiffness `EI_n` normal to
//! the axis and `s = y/cos L`, the vertical deflection obeys `w_ss = M_n/EI_n`,
//! so `w_yy = M_x/(EI_n cos^3 L)`; the Rayleigh strain energy gives the same
//! effective stiffness. `EI_n = k EI_s`, where `EI_s` is the streamwise-cut
//! stiffness of the sized section: cap and web material crossed by a normal cut
//! is the cap area and the web depth times thickness, so `k = 1` for them; a
//! cover panel between the spars is `cos L` as wide on a normal cut, so
//! `k = cos L` for it. Against the shell deck (12 presets, FE sections and FE
//! loads) this predicts the extra flexibility within about 15 percent.

use super::WingboxSizing;
use alas_geom::wing_structure::WingStructureGeometry;

/// Cosine of the box-axis sweep at each station, from the slope of the line
/// midway between the outermost spars (Torenbeek, Eq. 11.11 text). Local
/// slope, one-sided at the ends. All stations 1 for a degenerate input.
pub(crate) fn axis_cos(wsg: &WingStructureGeometry, y: &[f64]) -> Vec<f64> {
    let n = y.len();
    if n < 2 || !wsg.semi_span.is_finite() || wsg.semi_span <= 0.0 {
        return vec![1.0; n];
    }
    let (front, rear) = crate::sizing::box_chord_band(wsg);
    let mid = 0.5 * (front + rear);
    let x_mid: Vec<f64> = y
        .iter()
        .map(|&yi| {
            let eta = (yi / wsg.semi_span).clamp(0.0, 1.0);
            wsg.x_le(eta) + mid * wsg.local_chord(eta)
        })
        .collect();
    (0..n)
        .map(|j| {
            let (a, b) = (j.saturating_sub(1), (j + 1).min(n - 1));
            let dy = y[b] - y[a];
            if dy <= 0.0 || !dy.is_finite() {
                return 1.0;
            }
            let tan = (x_mid[b] - x_mid[a]) / dy;
            if tan.is_finite() {
                1.0 / (1.0 + tan * tan).sqrt()
            } else {
                1.0
            }
        })
        .collect()
}

/// Spanwise length, m, of the stiff root triangle of a swept box.
///
/// The box is clamped along the streamwise root rib while its bending acts in
/// planes normal to the swept axis, so a triangle of covers and spar ends
/// next to the fuselage side is held by the rib and deforms little (Obert,
/// *Aerodynamic Design of Transport Aircraft*, 2009, book p. 175: "the
/// deformation of the area A adjacent to the fuselage ... is only a secondary
/// effect"; the figure that bounds area A was not rendered, only the text
/// layer was read). Outboard of it the beam bends as a cantilever
/// ([`station_ei_spanwise`](super::section::station_ei_spanwise)).
///
/// The length is `W tan L`, the root box width `W` times the tangent of the
/// root-axis sweep. This scaling is not in any source read: it is the
/// dimensionally natural measure of the skew-clamp zone and was checked
/// against the shell deck. With the FE sections and the FE (elliptic) loads, the
/// swept-beam deflection of the 12 presets minus the tip deflection of the shell
/// decks, expressed as the rigid root length that closes it, is 1.00, 1.04,
/// 1.11 and 0.85 `W tan L` on A380-800, DC-10, B747-400 and A400M; the other
/// presets close with a length below `W tan L` or have none to give
/// (the shell deck is already stiffer than the beam theory there). The
/// length is capped at a fifth of the semi-span. Zero for an unswept or
/// unrecorded axis.
pub(crate) fn root_zone_length_m(sizing: &WingboxSizing) -> f64 {
    let cos0 = sizing.axis_cos_at(0);
    let (Some(&y_tip), Some(&c0)) = (sizing.y_stations.last(), sizing.chord.first()) else {
        return 0.0;
    };
    if !cos0.is_finite() || cos0 <= 1.0e-3 || cos0 >= 1.0 - 1.0e-9 || !c0.is_finite() || c0 <= 0.0 {
        return 0.0;
    }
    let fractions: Vec<f64> = sizing
        .spars
        .iter()
        .filter(|spar| spar.h.first().is_some_and(|&h| h > 0.0))
        .map(|spar| spar.chord_fraction)
        .collect();
    let (Some(front), Some(rear)) = (
        fractions.iter().copied().reduce(f64::min),
        fractions.iter().copied().reduce(f64::max),
    ) else {
        return 0.0;
    };
    let width = (rear - front) * c0;
    let tan_l = (1.0_f64 - cos0 * cos0).sqrt() / cos0;
    (width * tan_l).clamp(0.0, 0.2 * y_tip)
}

/// Fraction of each station's cell that lies outboard of the stiff root
/// triangle (1 flexible, 0 rigid), for the response integrals.
pub(crate) fn flexible_fraction(y: &[f64], root_zone_m: f64) -> Vec<f64> {
    (0..y.len())
        .map(|j| {
            let below = if j > 0 {
                y[j] - y[j - 1]
            } else {
                y.get(1).map_or(0.0, |&n| n - y[0])
            };
            let above = if j + 1 < y.len() {
                y[j + 1] - y[j]
            } else {
                below
            };
            let width = 0.5 * (below + above);
            if width <= 0.0 || root_zone_m <= 0.0 {
                return 1.0;
            }
            ((y[j] + 0.5 * width - root_zone_m) / width).clamp(0.0, 1.0)
        })
        .collect()
}

/// Bending stiffness the static response integrates: the section `ei` with the
/// stiff root triangle removed (cells divided by their flexible fraction).
/// Only when `swept_product` (the product law credits cover skin); the
/// reference law keeps its beam.
pub(crate) fn response_ei(
    sizing: &WingboxSizing,
    y: &[f64],
    ei: &[f64],
    swept_product: bool,
) -> Vec<f64> {
    let root_zone_m = if swept_product {
        root_zone_length_m(sizing)
    } else {
        0.0
    };
    let flexible = flexible_fraction(y, root_zone_m);
    ei.iter()
        .zip(&flexible)
        .map(|(&e, &f)| e / f.max(1.0e-9))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::flexible_fraction;

    #[test]
    fn the_stiff_root_removes_whole_cells_and_splits_the_straddling_one() {
        let y: Vec<f64> = (0..=10).map(f64::from).collect();
        // No stiff root: every cell flexible.
        assert!(flexible_fraction(&y, 0.0).iter().all(|&f| f == 1.0));
        // 2.25 m: cells centred on 0, 1 are rigid; the cell [1.5, 2.5] around
        // y = 2 has 0.25 of its width outboard of 2.25.
        let f = flexible_fraction(&y, 2.25);
        assert_eq!(f[0], 0.0);
        assert_eq!(f[1], 0.0);
        assert!((f[2] - 0.25).abs() < 1.0e-12, "{}", f[2]);
        assert_eq!(f[3], 1.0);
        assert!(f.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
