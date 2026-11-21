// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parasite drag items outside the smooth-component buildup: engine pylons,
//! the wing-body fairing, aft-fuselage upsweep and base drag.
//!
//! Pylons and the fairing add wetted area to a host component and take its
//! skin friction, form factor and interference factor; upsweep and base are
//! drag areas `D/q` (m^2) on the aircraft reference area. None of them is
//! part of the viscous margin, which is leakage and protuberance drag only
//! (Raymer 6th ed., Sec. 12.5.7); the margin multiplies the sum of the
//! components and these items once, as Raymer's percentage is of the total
//! parasite drag.
//!
//! Sources, read on the page:
//! - E. Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), sec.
//!   3.5.1, p. 94, Fig. 3-27b (after its Ref. 3-26): the drag increment of
//!   an upswept rear fuselage on the frontal area, `Delta C_D,front`,
//!   against the upsweep angle `beta` of the rear-body centreline, at angles
//!   of attack 0 and 5 deg. The measured points are at 5, 10, 15 and 20 deg
//!   on the alpha 0 curve and at 10, 15 and 20 deg on the alpha 5 curve;
//!   below them the curves are faired to the origin, and the text calls the
//!   drag of the slight upsweep that gives rotation clearance negligible.
//!   Only the measured ratio of the two curves is taken: the penalty at
//!   alpha 5 is 0.24, 0.44 and 0.52 of the alpha 0 value at 10, 15 and 20
//!   deg, rising with the upsweep. Below 10 deg both penalties scale as
//!   Raymer's `u^2.5` (below), so their ratio stays the 10 deg value
//!   ([`upsweep_frontal_cd`]). The body is drawn without a wing; behind a
//!   wing the afterbody flies the flow angle of [`super::AfterbodyFlow`].
//! - The transport afterbodies of AIAA Paper 84-0614 (Obert, below) are not
//!   spliced into that ratio. Their short upswept body A3 less its symmetric
//!   twin A4 gives 0.41 / 0.93 = 0.44 at 7.2 deg, about twice the 10 deg
//!   value of Fig. 3-27b and against its trend; at alpha 0 the same family
//!   carries about a third of Fig. 3-27b's penalty at 5 deg. The ratio of
//!   two body families depends on the afterbody shape as well as on the
//!   upsweep, so one curve in upsweep holds one family. Below 10 deg the
//!   trend of Fig. 3-27b would lower the ratio and the AIAA body raises it;
//!   the held 10 deg value lies between the two.
//! - D. P. Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed. (2018),
//!   Sec. 12.5.6, pp. 426-428: Eq. 12.36, `D/q = 3.83 u^2.5 A_max`, gives the
//!   alpha 0 magnitude at every upsweep. It lies within 7 % of the measured
//!   alpha 0 points of Fig. 3-27b at 10 to 20 deg. Near 5 deg the measured
//!   sets disagree (Fig. 3-27b 0.0128 of the frontal area; the transport
//!   afterbodies of AIAA 84-0614, in Obert, *Aerodynamic Design of Transport
//!   Aircraft*, Figs. 12.38 and 12.39, 0.0047 at 4.9 deg) and Eq. 12.36 gives
//!   0.0086, between them; below, it vanishes as `u^2.5`, as the remark on
//!   slight upsweep expects. Its Fig. 12.27 fixes `u` as the upsweep
//!   angle "of the fuselage centerline (not the belly angle)" and `A_max` as the maximum
//!   cross-sectional area, the reading kept here;
//!   Eqs. 12.37-12.38 for the base drag area; `A_base` "includes actual
//!   aft-facing flat surfaces as well as the aft-projected areas of steeply
//!   angled regions likely to experience separated airflow", expected "any
//!   place where the aft angle to the freestream exceeds about 20 deg".
//!   Reading of Fig. 12.27: `u` is the angle of the chord from the centre of
//!   the section where the aft body starts (`aft_body_start_m`, the longest
//!   of the plan, crown and belly closures) to the centre of the tail-end
//!   section; the figure draws the centreline of the upswept part, and the
//!   chord is its mean. For a tail end that is a tall blade (B787-9, end
//!   height 1.44 m) the end centre is the middle of that blade.
//! - E. Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), App. F,
//!   Sec. F-3.1, pp. 502-503: the basic fuselage drag (Eq. F-38, shape
//!   factor F-41) holds for a pointed tail with an afterbody of at least 2
//!   diameters; "small areas normal to the flow at the rear end of the
//!   fuselage ... e.g. in the case of an APU installation or a blunt tail
//!   fairing", less than about 10 % of the fuselage cross-section and with
//!   no jet efflux, add `Delta C_D S = 0.13 x base projected area` (Eq.
//!   F-42); shorter afterbodies and larger bases need the afterbody pressure
//!   increment (Eqs. F-43 to F-45). Swept-up tails measured about 5 % more
//!   drag than the axisymmetric body (p. 502).
//! - E. Torenbeek, *Advanced Aircraft Design* (2013), App. A.4, p. 395: "The
//!   surface area of engine pylons which amounts to typically 20% of the
//!   nacelle area has to be added" (turbofan nacelles); App. A.5, Eq. A.11,
//!   p. 396: `S_wet = pi d (l - 1.3 d) + S_w,net {2 + 0.5 t/c} + k_fair b_cs
//!   c_r + 2 (S_h + S_v)`, whose third term "corrects for the area penalty
//!   due to the fairing", "proportional to the centre section span `b_cs`
//!   and the wing root chord `c_r`", with typical `k_fair` "0 for a mid wing,
//!   0.2 for a high wing and 0.3 for a low wing"; the wing term there is the
//!   exposed wing outboard of the body or fairing, as in this buildup. The
//!   term is wetted area added to the airframe. Readings that are not in the
//!   source: the fairing is body skin, so it takes the fuselage's Reynolds
//!   length, form factor and interference factor; `b_cs` is the body width
//!   at the root leading edge; and the wing position is the third of the
//!   body height that holds the root leading edge (Torenbeek gives no
//!   boundary between low, mid and high).
//! - E. Obert, *Aerodynamic Design of Transport Aircraft* (2009), ch. 40, p.
//!   538: the zero-lift drag table of the Dornier 328 carries the landing gear
//!   fairing as a component row of its own (7.3 counts on 40 m2), so the
//!   sponsons of a high wing are drag of their own and not part of the wing
//!   fairing above. No source gives a coefficient. Reading, as for the wing
//!   fairing: sponsons are body skin, so their net wetted area takes the
//!   fuselage's Reynolds length, form factor and interference factor.
//! - E. Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), App. F,
//!   Sec. F-4.3b, p. 510: the interference penalty of a jet pod is of the
//!   order of 20 % of the "nacelle-plus-pylon" drag, so the pylon shares the
//!   nacelle's interference factor. Reading: the pylon is "added to the
//!   nacelle area" (2013, A.4), so it also takes the nacelle's Reynolds length
//!   and body form factor.

use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::wing::Wing;
use alas_geom::loft::unit_area;

/// Raymer 6th ed. Eq. 12.36, p. 428: `D/q = 3.83 u^2.5 A_max`, `u` in rad.
const RAYMER_UPSWEEP_COEFFICIENT: f64 = 3.83;

/// Measured ratio of the upsweep penalty at alpha 5 deg to that at alpha 0,
/// against the upsweep angle, deg, ascending: Torenbeek (1982) Fig. 3-27b,
/// the measured points of the two curves at 10, 15 and 20 deg, 0.0110 /
/// 0.0460, 0.0553 / 0.1257 and 0.1488 / 0.2851.
const UPSWEEP_ATTITUDE_RATIO: [(f64, f64); 3] = [
    (10.0, 0.0110 / 0.0460),
    (15.0, 0.0553 / 0.1257),
    (20.0, 0.1488 / 0.2851),
];

/// Name of the upsweep drag-area item in a parasite breakdown.
pub const UPSWEEP_ITEM: &str = "Fuselage upsweep";

/// The angle of attack, deg, of the upper curve of Fig. 3-27b (the lower is
/// alpha 0).
const UPSWEEP_UPPER_ALPHA_DEG: f64 = 5.0;

/// Raymer 6th ed. p. 428: base drag is expected wherever the aft angle to
/// the freestream exceeds about 20 deg.
const BASE_SURFACE_ANGLE_DEG: f64 = 20.0;

/// Torenbeek (1982) Eq. F-42, p. 503: drag area of a small base, per m^2 of
/// base projected area.
const SMALL_BASE_DRAG_COEFFICIENT: f64 = 0.13;

/// Torenbeek (1982) p. 503: Eq. F-42 holds for a base below about 10 % of
/// the fuselage cross-section.
const SMALL_BASE_AREA_FRACTION: f64 = 0.10;

/// Torenbeek (1982) Eqs. F-41 and F-43, pp. 502-503: the pointed-tail
/// fuselage drag holds for an afterbody of at least 2 equivalent diameters.
const SLENDER_AFTERBODY_FINENESS: f64 = 2.0;

/// Torenbeek (2013) App. A.4, p. 395: pylon surface area, typically 20 % of
/// the nacelle area.
pub const PYLON_WETTED_FRACTION: f64 = 0.20;

/// A section counts towards the constant (maximum) part of the body when its
/// area is within this fraction of the largest section; the aft body starts
/// behind the last such section.
const MAX_SECTION_TOLERANCE: f64 = 0.99;

/// One drag area `D/q` on the reference area.
#[derive(Debug, Clone, PartialEq)]
pub struct DragAreaItem {
    /// What the item is.
    pub name: &'static str,
    /// Drag area `D/q`, m^2.
    pub drag_area_m2: f64,
    /// `D/q / S_ref`.
    pub cd: f64,
}

/// Upsweep drag area `Delta C_D,front A_max`, m^2, at centreline upsweep
/// `upsweep_rad` and fuselage angle of attack `body_alpha_deg`
/// ([`upsweep_frontal_cd`]). Zero for a non-positive upsweep.
pub fn upsweep_drag_area(upsweep_rad: f64, max_area_m2: f64, body_alpha_deg: f64) -> f64 {
    upsweep_frontal_cd(upsweep_rad, body_alpha_deg).0 * max_area_m2
}

/// `Delta C_D,front` of an upswept rear fuselage and its slope with the body
/// angle of attack, per degree.
///
/// At alpha 0 it is Raymer's Eq. 12.36, `3.83 u^2.5`. At alpha 5 deg it is
/// that value times the measured ratio of [`UPSWEEP_ATTITUDE_RATIO`], linear
/// in `beta` between the measured angles. Below 10 deg, the lowest measured
/// upsweep, both penalties follow Raymer's `u^2.5` law (Eq. 12.36), which the
/// alpha 0 curve of Fig. 3-27b itself follows from 10 to 15 deg (0.1257 /
/// 0.0460 = 2.73 against 1.5^2.5 = 2.76), so their ratio is the 10 deg value.
/// Above 20 deg, the highest, it is the 20 deg value. Between the two
/// attitudes the ratio follows the smooth step `3 t^2 - 2 t^3`,
/// `t = alpha / 5 deg`, and it is held at the measured
/// attitudes outside them: the figure gives two attitudes only, and the step
/// keeps the polar's slope continuous where the attitude crosses them.
pub fn upsweep_frontal_cd(upsweep_rad: f64, body_alpha_deg: f64) -> (f64, f64) {
    if upsweep_rad.is_nan() || upsweep_rad <= 0.0 {
        return (0.0, 0.0);
    }
    let at_zero = RAYMER_UPSWEEP_COEFFICIENT * upsweep_rad.powf(2.5);
    let change = upsweep_attitude_ratio(upsweep_rad.to_degrees()) - 1.0;
    let t = (body_alpha_deg / UPSWEEP_UPPER_ALPHA_DEG).clamp(0.0, 1.0);
    let step = t * t * (3.0 - 2.0 * t);
    let step_per_deg = if t > 0.0 && t < 1.0 {
        6.0 * t * (1.0 - t) / UPSWEEP_UPPER_ALPHA_DEG
    } else {
        0.0
    };
    (
        at_zero * (1.0 + change * step),
        at_zero * change * step_per_deg,
    )
}

/// Ratio of the alpha 5 to the alpha 0 penalty at upsweep `beta_deg` from
/// [`UPSWEEP_ATTITUDE_RATIO`]. Below 10 deg the `u^2.5` scaling of both
/// penalties ([`upsweep_frontal_cd`]) leaves the 10 deg ratio; above 20 deg
/// the 20 deg ratio is held.
fn upsweep_attitude_ratio(beta_deg: f64) -> f64 {
    let points = &UPSWEEP_ATTITUDE_RATIO;
    let last = points.len() - 1;
    if beta_deg <= points[0].0 {
        return points[0].1;
    }
    if beta_deg >= points[last].0 {
        return points[last].1;
    }
    let i = points[1..last].partition_point(|&(beta, _)| beta <= beta_deg);
    let ((b0, r0), (b1, r1)) = (points[i], points[i + 1]);
    r0 + (beta_deg - b0) / (b1 - b0) * (r1 - r0)
}

/// Raymer 6th ed. Eqs. 12.37 (subsonic) and 12.38 (supersonic): base drag
/// area `[0.139 + 0.419 (M - 0.161)^2] A_base` and
/// `[0.064 + 0.042 (M - 3.84)^2] A_base`, m^2.
pub fn base_drag_area(mach: f64, base_area_m2: f64) -> f64 {
    let coefficient = if mach < 1.0 {
        0.139 + 0.419 * (mach - 0.161).powi(2)
    } else {
        0.064 + 0.042 * (mach - 3.84).powi(2)
    };
    coefficient * base_area_m2
}

/// Torenbeek (2013) Eq. A.11 fairing factor `k_fair` by wing position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingPosition {
    /// Wing root in the lower third of the body height: `k_fair` 0.3.
    Low,
    /// Middle third: `k_fair` 0.
    Mid,
    /// Upper third: `k_fair` 0.2.
    High,
}

impl WingPosition {
    /// `k_fair` of Torenbeek (2013) Eq. A.11, p. 396.
    pub fn fairing_factor(self) -> f64 {
        match self {
            Self::Low => 0.3,
            Self::Mid => 0.0,
            Self::High => 0.2,
        }
    }
}

/// Area of a lofted section, the superellipse `w h g(n)` with
/// `g(n) = Gamma(1 + 1/n)^2 / Gamma(1 + 2/n)` (`pi/4` for the ellipse,
/// `n = 2`).
fn section_area(xsec: &FuselageXSec) -> f64 {
    xsec.width * xsec.height * unit_area(xsec.shape) / 4.0
}

/// Centre height, width and height of the body at station `x`, linearly
/// interpolated between lofted sections; `None` outside the body.
fn section_at(body: &Fuselage, x: f64) -> Option<(f64, f64, f64)> {
    body.xsecs.windows(2).find_map(|pair| {
        let (a, b) = (&pair[0], &pair[1]);
        let (xa, xb) = (a.xyz_c[0], b.xyz_c[0]);
        if xb <= xa || x < xa || x > xb {
            return None;
        }
        let t = (x - xa) / (xb - xa);
        let lerp = |p: f64, q: f64| p + t * (q - p);
        Some((
            lerp(a.xyz_c[2], b.xyz_c[2]),
            lerp(a.width, b.width),
            lerp(a.height, b.height),
        ))
    })
}

/// The aft body of a lofted body: the maximum cross-section area, the
/// centreline upsweep angle and the base area.
///
/// A slender afterbody (at least 2 equivalent diameters, Torenbeek F-41)
/// whose end face is a small base (under 10 % of the largest section, F-42)
/// is the pointed tail of the basic fuselage drag plus an APU or tail-fairing
/// base: only the end face counts, at Torenbeek's 0.13 (Eq. F-42). Its
/// closure is resolved by the drawing fit only to its last few tenths of a
/// metre (the loft notes call the tip "resolution-limited"), and the local
/// angle there carries no separation that the fuselage form factor does not
/// already hold. Any other afterbody takes Raymer's Eqs. 12.37-12.38 on the
/// full `A_base` below.
///
/// `A_base` is read per surface, since Raymer's criterion is the aft angle
/// of the surface itself to the freestream. A section of half-width `a`,
/// half-height `e` and exponent `n` has area `S = a e 4 g(n)`; its change
/// over a step splits into the sides (`4 g e da`) and the crown and keel
/// (`2 g a d(crown)` and `2 g a d(keel)`, as `e` is half their distance).
/// The sides count where their half-angle in plan exceeds 20 deg and the
/// crown where its slope in side view does; the keel never counts, as its
/// rise is the centreline upsweep of Eq. 12.36 and counting it as base too
/// would charge the same separated region twice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AftBody {
    /// Largest section area, m^2.
    pub max_area_m2: f64,
    /// Rise of the centreline from the aft-body start to the tail end, rad
    /// (Raymer Fig. 12.27); zero for a level or drooping tail.
    pub upsweep_rad: f64,
    /// Area of the flat end face, m^2 (zero for a point closure).
    pub end_area_m2: f64,
    /// Aft-projected area of the sides where their plan half-angle exceeds
    /// 20 deg, m^2.
    pub sides_area_m2: f64,
    /// Aft-projected area of the crown where its slope exceeds 20 deg, m^2.
    pub crown_area_m2: f64,
    /// `A_base` of Eqs. 12.37-12.38: end face plus steep sides and crown,
    /// m^2.
    pub base_area_m2: f64,
    /// Afterbody length over the equivalent diameter `sqrt(4 A_max / pi)`
    /// (Torenbeek's `l_A / D_f,eff`).
    pub afterbody_fineness: f64,
}

impl AftBody {
    /// Read the aft body off the lofted sections. `aft_start_x_m` is where
    /// the loft's aft body begins (the last cabin station,
    /// `FuselageConfig::aft_body_start_m` from the nose); `None` takes the
    /// last section within 1 % of the largest one. `None` for a body with
    /// fewer than two sections, no positive area or a start outside it.
    pub fn of(body: &Fuselage, aft_start_x_m: Option<f64>) -> Option<Self> {
        let areas: Vec<f64> = body.xsecs.iter().map(section_area).collect();
        let max_area_m2 = areas.iter().copied().fold(0.0, f64::max);
        // `f64::max` from 0 skips NaN, so the fold is finite or zero.
        if body.xsecs.len() < 2 || max_area_m2 <= 0.0 {
            return None;
        }
        let x_start = match aft_start_x_m {
            Some(x) => x,
            None => {
                let index = areas
                    .iter()
                    .rposition(|&area| area >= MAX_SECTION_TOLERANCE * max_area_m2)?;
                body.xsecs[index].xyz_c[0]
            }
        };
        let (z_start, ..) = section_at(body, x_start)?;
        let last = body.xsecs.last()?;
        let run = last.xyz_c[0] - x_start;
        let upsweep_rad = if run > 0.0 {
            (last.xyz_c[2] - z_start).atan2(run).max(0.0)
        } else {
            0.0
        };
        let steep = BASE_SURFACE_ANGLE_DEG.to_radians().tan();
        let crown = |xsec: &FuselageXSec| xsec.xyz_c[2] + 0.5 * xsec.height;
        let (mut sides_area_m2, mut crown_area_m2) = (0.0, 0.0);
        for pair in body.xsecs.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if a.xyz_c[0] < x_start - 1e-9 {
                continue;
            }
            let dx = b.xyz_c[0] - a.xyz_c[0];
            let g = 0.125 * (unit_area(a.shape) + unit_area(b.shape));
            let is_steep = |drop: f64| drop > 0.0 && (dx <= 0.0 || drop > steep * dx);
            let side_drop = 0.5 * (a.width - b.width);
            if is_steep(side_drop) {
                sides_area_m2 += 4.0 * g * 0.25 * (a.height + b.height) * side_drop;
            }
            let crown_drop = crown(a) - crown(b);
            if is_steep(crown_drop) {
                crown_area_m2 += 2.0 * g * 0.25 * (a.width + b.width) * crown_drop;
            }
        }
        let end_area_m2 = areas[areas.len() - 1];
        Some(Self {
            max_area_m2,
            upsweep_rad,
            end_area_m2,
            sides_area_m2,
            crown_area_m2,
            base_area_m2: end_area_m2 + sides_area_m2 + crown_area_m2,
            afterbody_fineness: run / (4.0 * max_area_m2 / std::f64::consts::PI).sqrt(),
        })
    }

    /// Whether the end face is a small base behind a slender afterbody
    /// (Torenbeek 1982, Eq. F-42 and its validity).
    pub fn has_small_base(&self) -> bool {
        self.afterbody_fineness >= SLENDER_AFTERBODY_FINENESS
            && self.end_area_m2 <= SMALL_BASE_AREA_FRACTION * self.max_area_m2
    }

    /// Base drag area `D/q`, m^2: Torenbeek's `0.13 x` end face for a small
    /// base behind a slender afterbody, else Raymer's Eqs. 12.37-12.38 on
    /// `A_base`.
    pub fn base_drag_area(&self, mach: f64) -> f64 {
        if self.has_small_base() {
            SMALL_BASE_DRAG_COEFFICIENT * self.end_area_m2
        } else {
            base_drag_area(mach, self.base_area_m2)
        }
    }
}

/// Torenbeek (2013) Eq. A.11 fairing area `k_fair b_cs c_r`, m^2, for the
/// main wing on `body`: `b_cs` the body width and the wing position the
/// third of the body height holding the root leading edge, both at the root
/// leading-edge station; `c_r` the root chord. Zero when the root is
/// outside the body.
pub fn wing_body_fairing_area(wing: &Wing, body: &Fuselage) -> f64 {
    if wing.xsecs.is_empty() {
        return 0.0;
    }
    let ([x, _, z], root_chord_m) = wing.design_root();
    let Some((z_centre, width, height)) = section_at(body, x) else {
        return 0.0;
    };
    let position = if z < z_centre - height / 6.0 {
        WingPosition::Low
    } else if z > z_centre + height / 6.0 {
        WingPosition::High
    } else {
        WingPosition::Mid
    };
    position.fairing_factor() * width * root_chord_m
}

/// Net wetted area, m^2, the main-gear sponsons of a high wing add to `body`.
///
/// The lower half of the largest section (half width `a`, half height `b`) is
/// enclosed, at the sponsons' fullest section, by a half ellipse reaching
/// `half_width_m` from the centreline and `depth_m` below the keel, and the area
/// is the difference of the two half perimeters (Ramanujan's first
/// approximation) over `length_m`. The fullness along the length is that of
/// an ellipse, `pi/4`, since the sponsons close at both ends onto the skin.
/// Zero when the sponsons do not reach outside the hull.
pub fn gear_sponson_area(body: &Fuselage, length_m: f64, half_width_m: f64, depth_m: f64) -> f64 {
    let Some(largest) = body
        .xsecs
        .iter()
        .max_by(|p, q| section_area(p).total_cmp(&section_area(q)))
    else {
        return 0.0;
    };
    let half_perimeter = |a: f64, b: f64| {
        0.5 * std::f64::consts::PI * (3.0 * (a + b) - ((3.0 * a + b) * (a + 3.0 * b)).sqrt())
    };
    let (a, b) = (0.5 * largest.width, 0.5 * largest.height);
    let added = half_perimeter(half_width_m.max(a), b + depth_m.max(0.0)) - half_perimeter(a, b);
    (std::f64::consts::FRAC_PI_4 * length_m.max(0.0) * added).max(0.0)
}

// A test constructs the geometry it asserts on directly, so a failed expect is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    #[test]
    fn gear_sponsons_add_the_net_area_of_their_fullest_section() {
        let body = Fuselage::new(
            "hull",
            vec![station(0.0, 0.0, 3.0, 3.0), station(20.0, 0.0, 3.0, 3.0)],
        );
        // Nothing outside the hull, nothing added.
        assert_eq!(gear_sponson_area(&body, 6.0, 1.5, 0.0), 0.0);
        assert_eq!(gear_sponson_area(&body, 6.0, 1.0, -0.2), 0.0);
        // A half circle of radius 2 m around one of 1.5 m: pi/2 m more arc,
        // over 8 m at pi/4 fullness.
        let area = gear_sponson_area(&body, 8.0, 2.0, 0.5);
        assert!((area - PI * PI).abs() < 1e-9, "{area}");
    }

    #[test]
    fn upsweep_and_base_drag_areas_equal_their_sources_by_hand() {
        let area = |beta: f64, alpha: f64| upsweep_drag_area(beta.to_radians(), 10.0, alpha);
        let raymer = |beta: f64| 3.83 * f64::to_radians(beta).powf(2.5) * 10.0;
        // Alpha 0: Raymer's Eq. 12.36 at every upsweep, 0.0086 of the
        // frontal area at 5 deg and within 7 % of Torenbeek's measured
        // 0.046, 0.1257 and 0.2851 at 10, 15 and 20 deg.
        assert!((area(5.0, 0.0) / 10.0 - 0.0086).abs() < 5e-5);
        for (beta, measured) in [(10.0, 0.0460), (15.0, 0.1257), (20.0, 0.2851)] {
            assert!(
                (area(beta, 0.0) / 10.0 / measured - 1.0).abs() < 0.07,
                "{beta}"
            );
        }
        // Alpha 5: the measured ratio, 0.0110 / 0.0460 at 10 deg; below,
        // both penalties scale as u^2.5, so the ratio stays.
        let r10 = 0.0110 / 0.0460;
        assert!((area(10.0, 5.0) - r10 * raymer(10.0)).abs() < 1e-12);
        assert!((area(8.6, 5.0) - r10 * raymer(8.6)).abs() < 1e-12);
        assert!((area(4.0, 5.0) - r10 * raymer(4.0)).abs() < 1e-12);
        // One body family: the ratio rises monotonically with the upsweep.
        assert!(UPSWEEP_ATTITUDE_RATIO.windows(2).all(|w| w[1].1 > w[0].1));
        // Linear in beta between the measured points: 12.5 deg.
        let r15 = 0.0553 / 0.1257;
        let r = r10 + 0.5 * (r15 - r10);
        assert!((area(12.5, 5.0) - r * raymer(12.5)).abs() < 1e-12);
        // The smooth step is halfway at alpha 2.5 deg.
        assert!((area(15.0, 2.5) - (1.0 + 0.5 * (r15 - 1.0)) * raymer(15.0)).abs() < 1e-12);
        // Held at the measured attitudes outside 0-5 deg, with zero slope.
        assert_eq!(area(12.0, -3.0), area(12.0, 0.0));
        assert_eq!(area(12.0, 9.0), area(12.0, 5.0));
        assert_eq!(upsweep_frontal_cd(0.2, 0.0).1, 0.0);
        // The analytic slope is the derivative.
        let (_, slope) = upsweep_frontal_cd(0.1, 1.7);
        let h = 1e-6;
        let numeric =
            (upsweep_frontal_cd(0.1, 1.7 + h).0 - upsweep_frontal_cd(0.1, 1.7 - h).0) / (2.0 * h);
        assert!((slope - numeric).abs() < 1e-9);
        assert_eq!(upsweep_drag_area(-0.1, 10.0, 0.0), 0.0);
        assert_eq!(upsweep_drag_area(0.0, 10.0, 2.0), 0.0);
        assert_eq!(upsweep_drag_area(f64::NAN, 10.0, 2.0), 0.0);
        // Eq. 12.37 at M 0.8, A_base 1 m2: 0.139 + 0.419 * 0.639^2 = 0.310087.
        assert!((base_drag_area(0.8, 1.0) - 0.310_087).abs() < 1e-6);
        // Eq. 12.38 at M 2: 0.064 + 0.042 * 1.84^2 = 0.206195.
        assert!((base_drag_area(2.0, 1.0) - 0.206_195).abs() < 1e-6);
    }

    fn station(x: f64, z: f64, w: f64, h: f64) -> FuselageXSec {
        FuselageXSec::new([x, 0.0, z], None, Some(w), Some(h), 2.0).unwrap()
    }

    #[test]
    fn the_aft_body_reads_upsweep_and_base_off_the_loft() {
        // A 4 m cylinder to x = 30, then a tail cone to x = 40 whose centre
        // rises 2 m to a blunt 0.4 x 0.4 m end.
        let body = Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.0, 4.0, 4.0),
                station(30.0, 0.0, 4.0, 4.0),
                station(40.0, 2.0, 0.4, 0.4),
            ],
        );
        let aft = AftBody::of(&body, None).unwrap();
        assert!((aft.max_area_m2 - 4.0 * PI).abs() < 1e-12);
        assert!((aft.upsweep_rad - 0.2_f64.atan()).abs() < 1e-12);
        // Radius 2 -> 0.2 over 10 m is 10.2 deg and the crown rises: only
        // the end section.
        assert!((aft.base_area_m2 - PI * 0.04).abs() < 1e-12);
        assert_eq!(aft.sides_area_m2 + aft.crown_area_m2, 0.0);
        // A 10 m afterbody on a 4 m body (2.5 diameters) ending in 1 % of
        // the section: Torenbeek's small base, 0.13 x the end face.
        assert!((aft.afterbody_fineness - 2.5).abs() < 1e-12);
        assert!(aft.has_small_base());
        assert!((aft.base_drag_area(0.8) - 0.13 * PI * 0.04).abs() < 1e-12);
        // The same closure over 3 m (31 deg) is a base on the sides
        // (pi * 1.1 * 1.8) and the crown (half that); the keel's share
        // (the other half) is upsweep territory and stays out.
        let blunt = Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.0, 4.0, 4.0),
                station(30.0, 0.0, 4.0, 4.0),
                station(33.0, 0.0, 0.4, 0.4),
            ],
        );
        let aft = AftBody::of(&blunt, None).unwrap();
        assert!((aft.sides_area_m2 - PI * 1.1 * 1.8).abs() < 1e-12);
        assert!((aft.crown_area_m2 - 0.5 * PI * 1.1 * 1.8).abs() < 1e-12);
        let keel = 0.5 * PI * 1.1 * 1.8;
        assert!((aft.base_area_m2 + keel - 4.0 * PI).abs() < 1e-12);
        assert_eq!(aft.upsweep_rad, 0.0);
        // A 0.75-diameter afterbody is short: Raymer on the full A_base.
        assert!(!aft.has_small_base());
        assert_eq!(
            aft.base_drag_area(0.8),
            base_drag_area(0.8, aft.base_area_m2)
        );
    }

    #[test]
    fn a_superellipse_section_has_its_own_area() {
        // n = 2 is the ellipse; n -> large tends to the w x h rectangle.
        let ellipse = station(0.0, 0.0, 2.0, 1.0);
        assert!((section_area(&ellipse) - PI / 2.0).abs() < 1e-12);
        let boxy = FuselageXSec::new([0.0; 3], None, Some(2.0), Some(1.0), 4.0).unwrap();
        // Gamma(5/4)^2 / Gamma(3/2) = 0.9270373387 per unit w h.
        assert!((section_area(&boxy) - 2.0 * 0.927_037_338_7).abs() < 1e-9);
    }
}
