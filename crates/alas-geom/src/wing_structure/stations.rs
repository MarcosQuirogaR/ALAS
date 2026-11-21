// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The full station generator: cosine-spaced chordwise sampling with each
//! spar's exact chord fraction and the rib's own truncation point injected,
//! so spar elements land on real mesh nodes: `get_rib_stations`.

use super::support::{argmin_abs_diff, is_close, linspace, round6};
use super::types::RibStation;
use super::WingStructureGeometry;

impl WingStructureGeometry {
    /// Generate `num_ribs` spanwise rib stations, cosine-spaced chordwise
    /// with each spar's exact chord fraction injected so spar elements land
    /// on real mesh nodes.
    ///
    /// A rib whose perpendicular cut is truncated before reaching a given
    /// spar (`frac_spar > frac_actual`) gets `j_spars[i] = -1` for that
    /// spar: the mesh builder must skip that spar segment for this rib.
    pub fn get_rib_stations(&self, num_ribs: usize, num_pts_chord: usize) -> Vec<RibStation> {
        self.rib_stations(num_ribs, num_pts_chord, false)
    }

    /// [`Self::get_rib_stations`] with each section rotated nose-up about its
    /// leading edge by the built wing's jig twist ([`Self::twist_at_deg`]), in
    /// the rib plane. The product deck uses this; the frozen reference
    /// construction stays untwisted.
    pub fn get_rib_stations_twisted(
        &self,
        num_ribs: usize,
        num_pts_chord: usize,
    ) -> Vec<RibStation> {
        self.rib_stations(num_ribs, num_pts_chord, true)
    }

    fn rib_stations(
        &self,
        num_ribs: usize,
        num_pts_chord: usize,
        twisted: bool,
    ) -> Vec<RibStation> {
        let y_stations = linspace(0.0, self.semi_span, num_ribs);
        let mut stations = Vec::with_capacity(y_stations.len());

        for (index, &y) in y_stations.iter().enumerate() {
            let eta = y / self.semi_span;
            let x_le_val = self.x_le(eta);
            let z_le_val = self.z_le(eta);
            let (aft_x, aft_y) = self.rib_vector(eta);
            let (l_nominal, l_actual) = self.get_rib_lengths(y, x_le_val, aft_x, aft_y);
            let s_spars = self.compute_spar_intersections(y, x_le_val, aft_x, aft_y, l_nominal);

            let beta = linspace(0.0, std::f64::consts::PI, num_pts_chord);
            let mut xc_fracs: Vec<f64> = beta.iter().map(|&b| 0.5 * (1.0 - b.cos())).collect();

            let l_nominal_safe = l_nominal.max(1e-9);
            let frac_actual = l_actual / l_nominal_safe;
            let frac_spars: Vec<Option<f64>> = s_spars
                .iter()
                .map(|s| s.map(|value| value / l_nominal_safe))
                .collect();

            // Snap the nearest chordwise sample to each spar's exact
            // fraction (in spar order), then to `frac_actual` last: the
            // same order the reference mutates its array in, since a later
            // snap can retarget an index an earlier one already claimed.
            let mut critical_pts: Vec<f64> = frac_spars
                .iter()
                .filter_map(|&f| f)
                .filter(|&f| f <= frac_actual + 1e-5)
                .collect();
            critical_pts.push(frac_actual);
            for &pt in &critical_pts {
                let idx = argmin_abs_diff(&xc_fracs, pt);
                xc_fracs[idx] = pt;
            }

            let mut xc_fracs: Vec<f64> = xc_fracs.iter().map(|&x| round6(x)).collect();
            xc_fracs.sort_by(f64::total_cmp);
            xc_fracs.dedup();
            let frac_actual_rounded = round6(frac_actual);
            xc_fracs.retain(|&x| x <= frac_actual_rounded);

            let j_spars: Vec<i32> = frac_spars
                .iter()
                .map(|f_spar| match f_spar {
                    None => -1,
                    Some(f) => {
                        let target = round6(*f);
                        xc_fracs
                            .iter()
                            .position(|&x| is_close(x, target))
                            .map_or(-1, |position| position as i32)
                    }
                })
                .collect();

            let n = xc_fracs.len();
            let mut extrados = Vec::with_capacity(n);
            let mut intrados = Vec::with_capacity(n);
            let chord = self.local_chord(eta);
            let (sin_t, cos_t) = if twisted {
                self.twist_at_deg(eta).to_radians().sin_cos()
            } else {
                (0.0, 1.0)
            };
            for (j, &frac) in xc_fracs.iter().enumerate() {
                let (zu_norm, mut zl_norm) = self.airfoil_zu_zl(eta, frac);
                if j == n - 1 {
                    // Pinch the physical trailing edge shut (or the
                    // root-plane cut face, where "TE" is really a cut).
                    zl_norm = zu_norm;
                }
                // Jig twist: rotate the section nose-up about its leading edge, in
                // the rib plane (rotation about the plane normal keeps the points
                // in it). `s` runs along the rib from the leading edge.
                let s = frac * l_nominal;
                let place = |z_norm: f64| {
                    let z_rel = z_norm * chord;
                    let mut s_t = s * cos_t + z_rel * sin_t;
                    // A root-truncated rib ends on the root plane; the rotation
                    // must not carry its end across it (a few centimetres at most).
                    if twisted && aft_y < -1.0e-12 {
                        s_t = s_t.min(y / -aft_y);
                    }
                    [
                        x_le_val + s_t * aft_x,
                        y + s_t * aft_y,
                        z_le_val + z_rel * cos_t - s * sin_t,
                    ]
                };
                extrados.push(place(zu_norm));
                intrados.push(place(zl_norm));
            }

            stations.push(RibStation {
                index,
                eta,
                y_station: y,
                // Full vs. transition classification is refined by the mesh
                // builder, which knows the skin-start rib; see this
                // module's doc.
                is_full: true,
                frac_actual,
                extrados,
                intrados,
                j_spars,
                rib_dir_xy: (aft_x, aft_y),
            });
        }
        stations
    }

    /// Height of the upper (`upper`) or lower surface at plan position
    /// `(x, y)` on the streamwise section at `y`, the wing's own loft (the
    /// aerodynamic sections are streamwise), rotated nose-up about its
    /// leading edge by the jig twist when `twisted`, as
    /// [`Self::get_rib_stations_twisted`] rotates a section: a point at chord
    /// coordinate `s` moves to `s cos t + z sin t` aft of the leading edge, so
    /// `s` is solved for (by fixed point: within the box the surface slope
    /// times the twist sine is far below one).
    pub fn streamwise_surface_z(&self, x: f64, y: f64, upper: bool, twisted: bool) -> f64 {
        let eta = (y / self.semi_span).clamp(0.0, 1.0);
        let chord = self.local_chord(eta);
        let along = x - self.x_le(eta);
        let (sin_t, cos_t) = if twisted {
            self.twist_at_deg(eta).to_radians().sin_cos()
        } else {
            (0.0, 1.0)
        };
        let z_rel = |s: f64| {
            let (zu, zl) = self.airfoil_zu_zl(eta, s / chord.max(1e-9));
            (if upper { zu } else { zl }) * chord
        };
        let mut s = along / cos_t;
        for _ in 0..30 {
            s = (along - z_rel(s) * sin_t) / cos_t;
        }
        self.z_le(eta) + z_rel(s) * cos_t - s * sin_t
    }
}
