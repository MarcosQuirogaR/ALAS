// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rib-length truncation at the root plane, and the per-spar reference
//! lines a station's cut is intersected against -- the geometry
//! `get_rib_stations` needs before it can place a single point.

use super::support::intersect_line_ray;
use super::types::SparReferenceLine;
use super::WingStructureGeometry;

impl WingStructureGeometry {
    // -- rib length (truncation by the root plane Y=0) ----------------------

    /// `(L_nominal_to_TE, L_actual_after_root-plane_truncation)`, in metres
    /// along the rib's own cut direction `(aft_x, aft_y)` starting at
    /// `(x_le_val, y_le_val)`.
    pub fn get_rib_lengths(
        &self,
        y_le_val: f64,
        x_le_val: f64,
        aft_x: f64,
        aft_y: f64,
    ) -> (f64, f64) {
        if aft_y.abs() < 1e-6 {
            let chord = self.local_chord(y_le_val / self.semi_span);
            return (chord, chord);
        }

        let mut s_te_in: Option<f64> = None;
        let denom_in = aft_x - self.a_in * aft_y;
        if denom_in.abs() > 1e-6 {
            let s = (self.a_in * y_le_val + self.c_root - x_le_val) / denom_in;
            if s > 0.0 {
                let y_int = y_le_val + s * aft_y;
                if (0.0..=self.y_break + 1e-4).contains(&y_int) {
                    s_te_in = Some(s);
                }
            }
        }

        let mut s_te_out: Option<f64> = None;
        let denom_out = aft_x - self.a_out * aft_y;
        if denom_out.abs() > 1e-6 {
            let s =
                (self.a_out * (y_le_val - self.y_break) + self.x_kink_te - x_le_val) / denom_out;
            if s > 0.0 {
                let y_int = y_le_val + s * aft_y;
                if (self.y_break - 1e-4..=self.semi_span + 1e-4).contains(&y_int) {
                    s_te_out = Some(s);
                }
            }
        }

        let l_nominal = match (s_te_in, s_te_out) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => self.local_chord(y_le_val / self.semi_span),
        };

        let s_root = if aft_y < 0.0 {
            let s = -y_le_val / aft_y;
            (s > 0.0).then_some(s)
        } else {
            None
        };

        let l_actual = match s_root {
            Some(s) => l_nominal.min(s),
            None => l_nominal,
        };

        (l_nominal, l_actual)
    }

    // -- spar reference lines + intersections --------------------------------

    /// One 3-point (root/break/tip) reference line per spar, generalizing
    /// the reference's single rear-spar reference line to every spar in
    /// [`WingStructureGeometry::spar_fracs`].
    ///
    /// A spar with `spar_full_span[i] = false` (the optional partial-span
    /// center spar) gets no tip point at all -- it physically ends at the
    /// break/kink station, so there's no break -> tip segment to define.
    pub(super) fn compute_spar_reference_points(&self) -> Vec<SparReferenceLine> {
        let x_le_root = 0.0;
        let aft_root = self.rib_vector(0.0);
        let (l_root, _) = self.get_rib_lengths(0.0, x_le_root, aft_root.0, aft_root.1);

        let x_le_break = self.x_le(self.break_eta);
        let aft_break = self.rib_vector(self.break_eta);
        let (l_break, _) = self.get_rib_lengths(self.y_break, x_le_break, aft_break.0, aft_break.1);

        let x_le_tip = self.x_le(1.0);
        let aft_tip = self.rib_vector(1.0);
        let (l_tip, _) = self.get_rib_lengths(self.semi_span, x_le_tip, aft_tip.0, aft_tip.1);

        self.spar_fracs
            .iter()
            .zip(&self.spar_full_span)
            .map(|(&frac, &full_span)| {
                let root = (
                    x_le_root + frac * l_root * aft_root.0,
                    frac * l_root * aft_root.1,
                );
                let break_pt = (
                    x_le_break + frac * l_break * aft_break.0,
                    self.y_break + frac * l_break * aft_break.1,
                );
                let tip = full_span.then_some((
                    x_le_tip + frac * l_tip * aft_tip.0,
                    self.semi_span + frac * l_tip * aft_tip.1,
                ));
                SparReferenceLine {
                    root,
                    break_pt,
                    tip,
                }
            })
            .collect()
    }

    /// Distance (along the rib cut direction `(aft_x, aft_y)` from
    /// `(x_le_val, y_station)`) to each spar, in metres, in the order of
    /// [`WingStructureGeometry::spar_fracs`].
    ///
    /// Falls back to a straight `%`-chord estimate (`frac * l_nominal`) when
    /// the spar's reference line does not intersect this rib's cut within
    /// its own root/break or break/tip segment -- the same non-fatal
    /// fallback the reference used. Returns `None` for a partial-span spar
    /// at a station beyond its own break-station endpoint: it doesn't exist
    /// there, not even as a fallback estimate.
    pub fn compute_spar_intersections(
        &self,
        y_station: f64,
        x_le_val: f64,
        aft_x: f64,
        aft_y: f64,
        l_nominal: f64,
    ) -> Vec<Option<f64>> {
        let p_rib = (x_le_val, y_station);
        let d_rib = (aft_x, aft_y);
        let mut out = Vec::with_capacity(self.spar_fracs.len());

        for ((&frac, &full_span), reference) in self
            .spar_fracs
            .iter()
            .zip(&self.spar_full_span)
            .zip(&self.spar_ref_pts)
        {
            if !full_span && y_station > self.y_break + 1e-6 {
                out.push(None);
                continue;
            }

            let in_hit = intersect_line_ray(reference.root, reference.break_pt, p_rib, d_rib);
            let out_hit = reference
                .tip
                .and_then(|tip| intersect_line_ray(reference.break_pt, tip, p_rib, d_rib));

            let within =
                |hit: Option<(f64, f64)>| hit.filter(|&(_, t)| (-1e-5..=1.0 + 1e-5).contains(&t));

            let mut chosen = within(in_hit).or_else(|| within(out_hit)).map(|(s, _)| s);
            if chosen.is_none() {
                chosen = match (in_hit, out_hit) {
                    (Some((s_in, t_in)), Some((s_out, t_out))) => {
                        let dist_in = (-t_in).max(t_in - 1.0).max(0.0);
                        let dist_out = (-t_out).max(t_out - 1.0).max(0.0);
                        Some(if dist_in < dist_out { s_in } else { s_out })
                    }
                    (Some((s_in, _)), None) => Some(s_in),
                    (None, Some((s_out, _))) => Some(s_out),
                    (None, None) => None,
                };
            }

            let s_val = match chosen {
                Some(v) if v >= 0.0 => v,
                _ => frac * l_nominal,
            };
            out.push(Some(s_val));
        }
        out
    }
}
