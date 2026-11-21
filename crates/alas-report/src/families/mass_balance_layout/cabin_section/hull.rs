// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lining contour the cabin section clips its drawn parts to, read from the
//! payload model's lined section (`CabinGeometry::lining_top_z`), not an ellipse
//! of the figure's own.

use alas_payload::geometry::{
    CabinGeometry, ARMREST_ABOVE_FLOOR_M, LINING_SHOULDER_READ_ABOVE_FLOOR_M,
};

/// Samples of the contour outline on each side of the section.
const OUTLINE_STEPS: usize = 96;

/// Bisection iterations when a chord is inverted from the lining's crown height.
const BISECTIONS: usize = 48;

/// Shortest chord at the floor that carries a seat row or a pallet, m. Below it
/// the section has no usable floor and the figure reports the model's values
/// instead of drawing one.
pub(super) const MIN_FLOOR_CHORD_M: f64 = 0.80;

/// Inner contour of the section at one station: the payload model's lining.
#[derive(Clone, Copy)]
pub(super) struct Hull<'a> {
    cabin: &'a CabinGeometry,
    /// Station the section is cut at, m.
    x: f64,
    /// Inner half-width, in metres.
    pub(super) half_w: f64,
    /// Inner half-height, in metres.
    pub(super) half_h: f64,
    /// Section centre height, in metres.
    pub(super) zc: f64,
    /// The model's lining scale at the armrests and at the shoulders: the
    /// drawn cabin widths (or the class ratios) over the lofted lining's chord.
    scale: [f64; 2],
}

impl<'a> Hull<'a> {
    /// The lining at station `x`, or `None` where the wall consumes the section.
    pub(super) fn new(cabin: &'a CabinGeometry, x: f64) -> Option<Self> {
        let (half_w, half_h) = cabin.inner_semi_axes(x)?;
        Some(Self {
            cabin,
            x,
            half_w,
            half_h,
            zc: cabin.zc_at(x),
            scale: cabin.lining_scale,
        })
    }

    /// Height of the lining's crown at lateral offset `y`; `None` outside it.
    fn crown_z(self, y: f64) -> Option<f64> {
        self.cabin.lining_top_z(self.x, y)
    }

    /// Half the lining width at height `z`; zero at or beyond the crown or keel.
    /// The lining is symmetric about the section centre.
    pub(super) fn chord_half(self, z: f64) -> f64 {
        let above = if z >= self.zc { z } else { 2.0 * self.zc - z };
        if above >= self.zc + self.half_h {
            return 0.0;
        }
        // The crown height falls as the lateral offset grows: find the offset
        // where it passes `above`.
        let (mut low, mut high) = (0.0, self.half_w);
        match self.crown_z(0.0) {
            Some(top) if top >= above => {}
            _ => return 0.0,
        }
        for _ in 0..BISECTIONS {
            let mid = 0.5 * (low + high);
            match self.crown_z(mid) {
                Some(top) if top >= above => low = mid,
                _ => high = mid,
            }
        }
        low
    }

    /// The lining as a closed ring of `[y, z]` points: the crown from port to
    /// starboard, then the keel back.
    pub(super) fn outline(self) -> Vec<[f64; 2]> {
        let at = |k: usize| -> f64 {
            let t = std::f64::consts::PI * k as f64 / OUTLINE_STEPS as f64;
            self.half_w * (1.0 - 1e-9) * (-t.cos())
        };
        let crown: Vec<[f64; 2]> = (0..=OUTLINE_STEPS)
            .map(|k| {
                let y = at(k);
                [y, self.crown_z(y).unwrap_or(self.zc)]
            })
            .collect();
        let keel = crown.iter().rev().map(|&[y, z]| [y, 2.0 * self.zc - z]);
        crown.iter().copied().chain(keel).collect()
    }

    /// The superellipse exponent of the lining at this station, read back from
    /// the model's crown height half-way out (`2` is an ellipse).
    fn exponent(self) -> f64 {
        let ratio = self.crown_z(0.5 * self.half_w).map_or(0.866, |top| {
            ((top - self.zc) / self.half_h).clamp(0.501, 0.999_999)
        });
        // (1 - 0.5^n)^(1/n) rises with n from 0.5 (n = 1) toward 1.
        let (mut low, mut high) = (1.0, 40.0);
        for _ in 0..BISECTIONS {
            let mid = 0.5 * (low + high);
            if (1.0 - 0.5_f64.powf(mid)).powf(1.0 / mid) < ratio {
                low = mid;
            } else {
                high = mid;
            }
        }
        0.5 * (low + high)
    }

    /// The outer skin as a closed ring of `[y, z]` points: the lining's contour
    /// with `wall` added to both semi-axes, at the lining's exponent.
    pub(super) fn skin_outline(self, wall: f64) -> Vec<[f64; 2]> {
        let n = self.exponent();
        let (half_w, half_h) = (self.half_w + wall, self.half_h + wall);
        let crown: Vec<[f64; 2]> = (0..=OUTLINE_STEPS)
            .map(|k| {
                let t = std::f64::consts::PI * k as f64 / OUTLINE_STEPS as f64;
                let y = half_w * (-t.cos());
                let share = (y.abs() / half_w).min(1.0);
                [
                    y,
                    self.zc + half_h * (1.0 - share.powf(n)).max(0.0).powf(1.0 / n),
                ]
            })
            .collect();
        let keel = crown.iter().rev().map(|&[y, z]| [y, 2.0 * self.zc - z]);
        crown.iter().copied().chain(keel).collect()
    }

    /// Half the width the model lets a seat row use at height `z`, for a
    /// passenger floor at `floor`: the lining's chord times the model's lining
    /// scale, which is the armrest value up to the armrests, the shoulder value
    /// from the shoulder read height, and linear between (the heights
    /// `ARMREST_ABOVE_FLOOR_M` and `LINING_SHOULDER_READ_ABOVE_FLOOR_M`).
    pub(super) fn seat_chord_half(self, z: f64, floor: f64) -> f64 {
        let above = z - floor;
        let share = ((above - ARMREST_ABOVE_FLOOR_M)
            / (LINING_SHOULDER_READ_ABOVE_FLOOR_M - ARMREST_ABOVE_FLOOR_M))
            .clamp(0.0, 1.0);
        self.chord_half(z) * (self.scale[0] + share * (self.scale[1] - self.scale[0]))
    }

    /// Whether a seat rectangle, on the passenger floor at `floor`, lies inside
    /// the width the model gives a row.
    pub(super) fn seat_rect_inside(self, y0: f64, y1: f64, z0: f64, z1: f64, floor: f64) -> bool {
        [(y0, z0), (y1, z0), (y1, z1), (y0, z1)]
            .iter()
            .all(|&(y, z)| y.abs() <= self.seat_chord_half(z, floor) + 1e-9)
    }

    /// Why the model's floor at height `floor_z` cannot be drawn, or `None`.
    ///
    /// A floor is drawable when it stands above the lining's keel with at least
    /// [`MIN_FLOOR_CHORD_M`] of lining chord. The figure never substitutes a
    /// floor of its own: an invalid floor is a defect of the model's section
    /// and is shown with its values.
    pub(super) fn floor_problem(self, floor_z: f64) -> Option<String> {
        let keel = self.zc - self.half_h;
        let chord = 2.0 * self.chord_half(floor_z);
        if floor_z.is_finite() && chord >= MIN_FLOOR_CHORD_M {
            return None;
        }
        Some(format!(
            "Model floor z = {floor_z:.3} m; lining keel z = {keel:.3} m, crown z = {:.3} m; chord at the floor {chord:.2} m (at least {MIN_FLOOR_CHORD_M:.2} m needed)",
            self.zc + self.half_h
        ))
    }
}

/// Seat groups between aisles for display.
///
/// Twin-aisle rows with up to eight abreast keep two seats at each window
/// (A340 and A330 2-4-2, 2-3-2, 2-2-2); nine and more keep three (3-3-3,
/// 3-4-3). Single-aisle and other rows keep the layout's own grouping.
pub(super) fn display_blocks(abreast: i64, blocks: &[i64]) -> Vec<i64> {
    if blocks.len() == 3 && abreast >= 6 && blocks.iter().sum::<i64>() == abreast {
        let outboard = if abreast >= 9 { 3 } else { 2 };
        let centre = abreast - 2 * outboard;
        if centre >= 1 {
            return vec![outboard, centre, outboard];
        }
    }
    blocks.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_abreast_twin_aisle_is_two_four_two() {
        assert_eq!(display_blocks(8, &[3, 2, 3]), vec![2, 4, 2]);
        assert_eq!(display_blocks(9, &[3, 3, 3]), vec![3, 3, 3]);
        assert_eq!(display_blocks(10, &[3, 4, 3]), vec![3, 4, 3]);
        assert_eq!(display_blocks(6, &[3, 3]), vec![3, 3]);
    }
}
