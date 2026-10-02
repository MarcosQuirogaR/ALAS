// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The numerical primitives and the cap-sizing laws the solve applies.
//!
//! The NumPy-equivalent helpers reproduce upstream's arithmetic exactly
//! (summation order included), and the cap rules are shared with
//! [`crate::mesh`], which re-derives cap dimensions on its own station grid
//! and has to apply the same law to do it.

use super::types::WingboxSizing;

/// NumPy `linspace(start, stop, n)` with `endpoint=True`: `n` evenly spaced
/// points, the last pinned exactly to `stop`. The geometry crate's function is
/// the one the wingbox geometry is sampled with, so the sizing grid shares its
/// arithmetic rather than carrying a copy of it.
pub(super) use alas_geom::aircraft::spacing::linspace;

/// NumPy `gradient(f)` at unit spacing, `edge_order=1`: central differences
/// interior, one-sided at the two ends. For a uniform `y` this is the constant
/// station spacing, but the general form is reproduced so the arithmetic
/// matches upstream bit for bit.
pub(crate) fn gradient_unit(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut g = vec![0.0; n];
    if n < 2 {
        return g;
    }
    for i in 1..n - 1 {
        g[i] = (f[i + 1] - f[i - 1]) / 2.0;
    }
    g[0] = f[1] - f[0];
    g[n - 1] = f[n - 1] - f[n - 2];
    g
}

/// NumPy `trapezoid(y, x)`: the trapezoidal integral of `y` over the sample
/// points `x`.
pub(crate) fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    let mut acc = 0.0;
    for i in 0..y.len().saturating_sub(1) {
        acc += (x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0;
    }
    acc
}

/// The number of stations needed to keep every uniform rib panel at or below
/// the maximum spacing. Both the root and tip are ribs, so panels plus one is
/// the count. This is the count form of the panel-buckling sizing rule.
pub(super) fn rib_count_from_max_spacing(semi_span_m: f64, max_spacing_m: f64) -> i64 {
    (semi_span_m / max_spacing_m).ceil() as i64 + 1
}

/// The spar-cap taper law: full section up to `eta_lock`, then linear taper to
/// `tip_fraction` at the tip: `_cap_taper`.
///
/// Visible to `crate::mesh` as well: the mesh re-derives cap dimensions on its
/// own, finer station grid rather than sampling this module's arrays, and has
/// to apply the same law to do it.
pub(crate) fn cap_taper(eta: &[f64], eta_lock: f64, tip_fraction: f64) -> Vec<f64> {
    let denom = (1.0 - eta_lock).max(1e-9);
    eta.iter()
        .map(|&e| {
            if e <= eta_lock {
                1.0
            } else {
                1.0 - (1.0 - tip_fraction) * (e - eta_lock) / denom
            }
        })
        .collect()
}

/// The root cap flange width and thickness, m, that carry the required cap
/// area `a_cap0` on a spar of height `h0` at a station of chord `chord0`.
///
/// The flange starts at the lesser of half the chord and 0.6 of the spar
/// height, and its thickness is capped at a fifth of the spar height so the
/// caps never fill the web. When that thickness clip binds (a shallow rear
/// spar with a low-allowable alloy at a high root moment does it) the same
/// area is spread over a wider flange, up to the half-chord bound, rather
/// than left short: the box skins are what carry a wide flange in a real wing,
/// and an under-strength root would contradict the zero root margin this
/// routine sizes to. Only past the half-chord bound is the root reported
/// under strength, which is then a genuine infeasibility.
///
/// Shared with `crate::mesh`, which re-derives the cap dimensions on its own
/// station grid and has to apply the same law.
pub(crate) fn root_cap_dimensions(a_cap0: f64, chord0: f64, h0: f64) -> (f64, f64) {
    let w_max = (0.5 * chord0).max(1e-6);
    let t_max = h0 * 0.20;
    let mut w_cap0 = w_max.min(h0 * 0.6).max(1e-6);
    let mut t_cap0 = (a_cap0 / w_cap0).min(t_max);
    if a_cap0 / w_cap0 > t_max && t_max > 0.0 {
        w_cap0 = (a_cap0 / t_max).min(w_max).max(w_cap0);
        t_cap0 = (a_cap0 / w_cap0).min(t_max);
    }
    (w_cap0, t_cap0)
}

/// The cap flange at one station: the root flange rule applied to the local
/// section, then floored at the minimum practical cover gauge.
///
/// `a_req` is the flange area the local bending demand needs, zero where there
/// is no demand to speak of. The floor is a thickness, so a station the demand
/// does not reach still carries a manufacturable cover rather than a film; it
/// is clipped at a third of the local section depth so a section that has run
/// out of height cannot be handed a flange deeper than itself.
pub(super) fn station_cap_dimensions(
    a_req: f64,
    chord: f64,
    h: f64,
    min_thickness: f64,
) -> (f64, f64) {
    let (width, thickness) = root_cap_dimensions(a_req, chord, h);
    let floor = min_thickness.min(h / 3.0).max(0.0);
    if thickness >= floor {
        (width, thickness)
    } else {
        (width.max(floor), floor)
    }
}

/// The cover skin area, m^2 per cover, that works with each spar's cap in
/// bending at one station, in the order of `chord_fractions`.
///
/// The upper and lower covers between the spars sit at the same lever arm as
/// the caps and carry the same bending stress, so their skin is bending
/// material and not a passive fairing. The standard boom idealization lumps a
/// skin panel of thickness `t` and width `b` between two booms into those
/// booms as `t b / 6 (2 + sigma_2 / sigma_1)` each (T. H. G. Megson,
/// *Aircraft Structures for Engineering Students*, 4th ed., Butterworth-
/// Heinemann, 2007, ch. 20 "Structural idealization"). The strength law sizes
/// every cap to the same allowable, so the two boom stresses are equal and
/// each boom receives `t b / 2` of every panel beside it.
///
/// A spar with no depth at this station (a partial-span spar outboard of its
/// break) is not a boom; the panel then runs between its neighbours.
pub(crate) fn cover_skin_boom_areas_m2(
    chord_fractions: &[f64],
    heights_m: &[f64],
    chord_m: f64,
    t_skin_m: f64,
) -> Vec<f64> {
    let mut areas = vec![0.0; chord_fractions.len()];
    let mut active: Vec<usize> = (0..chord_fractions.len().min(heights_m.len()))
        .filter(|&i| heights_m[i] > 0.0)
        .collect();
    active.sort_by(|&a, &b| chord_fractions[a].total_cmp(&chord_fractions[b]));
    let t = t_skin_m.max(0.0);
    for pair in active.windows(2) {
        let width = (chord_fractions[pair[1]] - chord_fractions[pair[0]]).max(0.0) * chord_m;
        let half_panel = 0.5 * t * width;
        areas[pair[0]] += half_panel;
        areas[pair[1]] += half_panel;
    }
    areas
}

/// [`cover_skin_boom_areas_m2`] of a product-law box at one of its stations,
/// in the order of [`WingboxSizing::spars`].
///
/// For the stress recovery of a box the product law sized; a frozen-law box
/// credits no skin and must not be read through this.
pub(crate) fn sizing_cover_skin_boom_areas_m2(sizing: &WingboxSizing, station: usize) -> Vec<f64> {
    let fractions: Vec<f64> = sizing.spars.iter().map(|s| s.chord_fraction).collect();
    let heights: Vec<f64> = sizing
        .spars
        .iter()
        .map(|s| s.h.get(station).copied().unwrap_or(0.0))
        .collect();
    let chord = sizing.chord.get(station).copied().unwrap_or(0.0);
    cover_skin_boom_areas_m2(&fractions, &heights, chord, sizing.t_skin)
}

/// Which cap-sizing law a solve applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SizingLaw {
    /// Every station carries its own bending moment: the root flange widens
    /// when its thickness clip binds and an outboard station whose tapered
    /// flange falls short of the local moment is sized up to it. The box is
    /// sized against the relieved load and its skin and ribs span the
    /// structural box, not the whole chord. The cover skin carries bending
    /// beside the caps.
    Product,
    /// The frozen reference law: the root cap alone is sized, its thickness
    /// clipped at a fifth of the spar height, and the outboard caps follow the
    /// taper whatever the local moment. A shallow spar can be left under
    /// strength, which the fixtures record. The load carries no inertia relief
    /// and the skin and ribs run the whole chord, both of which the fixtures
    /// also record.
    Frozen,
}

impl SizingLaw {
    /// Whether this law charges skin and rib area over the whole aerofoil
    /// chord instead of over the structural box between the outermost spars.
    pub(super) fn charges_the_whole_chord(self) -> bool {
        matches!(self, Self::Frozen)
    }

    /// Whether the cover skin between the spars is credited as bending
    /// material beside the caps ([`cover_skin_boom_areas_m2`]). The frozen law
    /// charges the skin as mass but sizes the caps for the whole moment, which
    /// is what its fixtures record.
    pub(super) fn credits_the_cover_skin(self) -> bool {
        matches!(self, Self::Product)
    }
}
