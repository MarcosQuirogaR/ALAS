// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Point queries on a [`Loft`]: sections and their rates, profile lines,
//! area ruling, widths at a height, tip radii and the tail-scrape angle.

use super::curves::Jets;
use super::section::{unit_area, unit_area_dn, Section};
use super::Loft;

/// A squared extent below this is treated as zero (m^2).
const TINY_SQUARE: f64 = 1e-18;
/// Smallest half-extent used to turn a squared-extent slope into a rate, m.
const MIN_EXTENT: f64 = 1e-12;

/// First axial derivatives of a [`Section`]'s five numbers (per metre).
///
/// At a point closure `da` and `de` are unbounded (`a ~ sqrt(2 R x)`); the
/// values returned there are finite only because the half-extent is floored
/// at 1e-12 m, so do not integrate them across the tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionRates {
    /// `da/dx`.
    pub da: f64,
    /// `de/dx`.
    pub de: f64,
    /// `dzc/dx`.
    pub dzc: f64,
    /// `dn/dx`.
    pub dn: f64,
    /// `dk/dx`.
    pub dk: f64,
}

/// Radii of curvature of the tip paraboloid, from the first slope of `A`
/// and `E`: `R_plan = A'/2`, `R_side = E'/2` (a body with `a^2 = 2 R x`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipRadii {
    /// Radius of curvature of the plan outline at the tip, m.
    pub plan_m: f64,
    /// Radius of curvature of the side outline at the tip, m.
    pub side_m: f64,
}

impl Loft {
    /// Section and curve jets at output-frame `x` (before the z origin).
    pub(super) fn local_state(&self, x_out: f64) -> (Section, Jets) {
        let x = x_out - self.params.origin[0];
        let j = self.curves.jets(x, None);
        (section_from(&j, 0.0), j)
    }

    /// The section at output-frame `x`; `x` is clamped to the body, and a
    /// non-finite `x` gives an all-NaN section.
    #[must_use]
    pub fn section(&self, x: f64) -> Section {
        if !x.is_finite() {
            return Section::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        }
        let (_, j) = self.local_state(x);
        section_from(&j, self.params.origin[2])
    }

    /// First axial derivatives of the section at output-frame `x`.
    #[must_use]
    pub fn section_rates(&self, x: f64) -> SectionRates {
        if !x.is_finite() {
            let n = f64::NAN;
            return SectionRates {
                da: n,
                de: n,
                dzc: n,
                dn: n,
                dk: n,
            };
        }
        let (_, j) = self.local_state(x);
        let a = j.a2[0].max(0.0).sqrt().max(MIN_EXTENT);
        let e = j.e2[0].max(0.0).sqrt().max(MIN_EXTENT);
        SectionRates {
            da: j.a2[1] / (2.0 * a),
            de: j.e2[1] / (2.0 * e),
            dzc: j.zc[1],
            dn: j.n[1],
            dk: j.k[1],
        }
    }

    fn inside(&self, x: f64) -> bool {
        let (x0, x1) = self.x_range();
        x.is_finite() && x >= x0 && x <= x1
    }

    /// Crown height `zc + e` at `x`, m.
    #[must_use]
    pub fn crown_z(&self, x: f64) -> f64 {
        self.section(x).crown_z()
    }

    /// Keel height `zc - e` at `x`, m.
    #[must_use]
    pub fn keel_z(&self, x: f64) -> f64 {
        self.section(x).keel_z()
    }

    /// Half-width of the body at station `x` and height `z` (output frame):
    /// the closed-form inverse of the section; zero outside the body.
    #[must_use]
    pub fn half_width_at(&self, x: f64, z: f64) -> f64 {
        if !self.inside(x) {
            return 0.0;
        }
        self.section(x).half_width_at(z)
    }

    /// Section area `S(x)` in m^2, exact; zero outside the body.
    #[must_use]
    pub fn s_of_x(&self, x: f64) -> f64 {
        if !self.inside(x) {
            return 0.0;
        }
        let (_, j) = self.local_state(x);
        area_of(&j)
    }

    /// Axial derivative `dS/dx` in m, exact (area ruling); zero outside.
    ///
    /// `S = sqrt(A E) U(n)` so `dS/dx = U (A' E + A E') / (2 sqrt(A E)) +
    /// sqrt(A E) U'(n) n'`, evaluated as `U (A' sqrt(E/A) + E' sqrt(A/E)) / 2`
    /// to stay finite at a point closure, where the limit `U sqrt(A' E')` is
    /// used at the tip itself.
    #[must_use]
    pub fn ds_dx(&self, x: f64) -> f64 {
        if !self.inside(x) {
            return 0.0;
        }
        let (_, j) = self.local_state(x);
        let (a2, e2) = (j.a2[0].max(0.0), j.e2[0].max(0.0));
        let n = j.n[0].max(1.0);
        let u = unit_area(n);
        let rate = if a2 > TINY_SQUARE && e2 > TINY_SQUARE {
            0.5 * u * (j.a2[1] * (e2 / a2).sqrt() + j.e2[1] * (a2 / e2).sqrt())
        } else if a2 <= TINY_SQUARE && e2 <= TINY_SQUARE {
            let product = j.a2[1] * j.e2[1];
            if product > 0.0 {
                j.a2[1].signum() * u * product.sqrt()
            } else {
                0.0
            }
        } else {
            0.0
        };
        let base = rate + (a2 * e2).sqrt() * unit_area_dn(n) * j.n[1];
        if !has_residual(&j) {
            return base;
        }
        let sec = section_from(&j, 0.0);
        let (a_x, e_x) = (
            j.a2[1] / (2.0 * sec.a.max(MIN_EXTENT)),
            j.e2[1] / (2.0 * sec.e.max(MIN_EXTENT)),
        );
        let b_x = [j.b[0][1], j.b[1][1], j.b[2][1], j.b[3][1]];
        base + sec.residual_area_rate(a_x, e_x, j.n[1], j.k[1], &b_x)
    }

    /// Area of the flat end faces: the base closures, m^2 (zero for point
    /// closures). The base is not part of the wetted skin.
    #[must_use]
    pub fn base_area(&self) -> f64 {
        let mut total = 0.0;
        let (x0, x1) = self.x_range();
        if matches!(self.spec.front, alas_config::loft::Closure::Base) {
            total += self.s_of_x(x0);
        }
        if matches!(self.spec.rear, alas_config::loft::Closure::Base) {
            total += self.s_of_x(x1);
        }
        total
    }

    /// Tip radii of the nose (`front`) and tail (`rear`) paraboloids, `None`
    /// for a base closure. A body fitted with a prescribed tip radius has
    /// `plan_m == side_m == tip_radius_m`.
    #[must_use]
    pub fn tip_radii(&self) -> (Option<TipRadii>, Option<TipRadii>) {
        let (x0, x1) = self.x_range();
        let radii = |x: f64, sign: f64| {
            let (_, j) = self.local_state(x);
            TipRadii {
                plan_m: 0.5 * sign * j.a2[1],
                side_m: 0.5 * sign * j.e2[1],
            }
        };
        let front = matches!(self.spec.front, alas_config::loft::Closure::Point { .. })
            .then(|| radii(x0, 1.0));
        let rear = matches!(self.spec.rear, alas_config::loft::Closure::Point { .. })
            .then(|| radii(x1, -1.0));
        (front, rear)
    }

    /// The exact tail-scrape angle in degrees: the minimum over the body aft
    /// of the main gear of `atan2(keel_z(x) - z_ground, x - x_main_gear)`.
    ///
    /// `x_main_gear` and `z_ground` are in the output frame (the gear
    /// contact point and the ground height under it). The minimum is found
    /// by an 800-point scan and refined by golden-section search around the
    /// best sample, then compared with the tail end. `None` when the gear
    /// is at or aft of the tail end, an argument is not finite, or the keel is
    /// below the ground somewhere aft of the gear (a negative clearance has no
    /// scrape angle: the body would already be on the ground).
    #[must_use]
    pub fn tail_scrape_deg(&self, x_main_gear: f64, z_ground: f64) -> Option<f64> {
        let (x_nose, x_end) = self.x_range();
        if !(x_main_gear.is_finite() && z_ground.is_finite()) || x_main_gear >= x_end {
            return None;
        }
        let angle = |x: f64| (self.keel_z(x) - z_ground).atan2(x - x_main_gear);
        let x_start = x_main_gear.max(x_nose);
        let steps = 800usize;
        let step = (x_end - x_start) / steps as f64;
        let first = usize::from(x_start <= x_main_gear);
        let mut best = (f64::INFINITY, steps);
        for i in first..=steps {
            let a = angle(x_start + step * i as f64);
            if a < best.0 {
                best = (a, i);
            }
        }
        let lo = (x_start + step * best.1.saturating_sub(1) as f64).max(x_main_gear + 1e-9);
        let hi = (x_start + step * (best.1 + 1).min(steps) as f64).min(x_end);
        let (mut a, mut b) = (lo, hi);
        let g = 0.5 * (5.0_f64.sqrt() - 1.0);
        let (mut c, mut d) = (b - g * (b - a), a + g * (b - a));
        let (mut fc, mut fd) = (angle(c), angle(d));
        for _ in 0..200 {
            if (b - a).abs() < 1e-13 * (1.0 + x_end.abs()) {
                break;
            }
            if fc < fd {
                b = d;
                d = c;
                fd = fc;
                c = b - g * (b - a);
                fc = angle(c);
            } else {
                a = c;
                c = d;
                fc = fd;
                d = a + g * (b - a);
                fd = angle(d);
            }
        }
        let refined = fc.min(fd);
        let minimum = refined.min(best.0).min(angle(x_end));
        (minimum >= 0.0).then(|| minimum.to_degrees())
    }
}

/// The section of a station's jets, with `z_offset` added to `zc`.
pub(super) fn section_from(j: &Jets, z_offset: f64) -> Section {
    Section::new(
        j.a2[0].max(0.0).sqrt(),
        j.e2[0].max(0.0).sqrt(),
        j.zc[0] + z_offset,
        j.n[0].max(1.0),
        j.k[0],
    )
    .with_residual([j.b[0][0], j.b[1][0], j.b[2][0], j.b[3][0]])
}

/// `true` if any residual coefficient is not zero at this station.
pub(super) fn has_residual(j: &Jets) -> bool {
    j.b.iter().any(|b| b[0] != 0.0)
}

/// `S = sqrt(A E) U(n)` (plus the residual part, if any) from a station's jets.
pub(super) fn area_of(j: &Jets) -> f64 {
    let base = (j.a2[0].max(0.0) * j.e2[0].max(0.0)).sqrt() * unit_area(j.n[0].max(1.0));
    if has_residual(j) {
        base + section_from(j, 0.0).residual_area()
    } else {
        base
    }
}
