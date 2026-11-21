// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing/fuselage/nacelle aerodynamic centre and the fuselage pitching
//! moment of the forward-limit balances, by Torenbeek, *Synthesis of Subsonic
//! Airplane Design*, 1982, App. E, sec. E-6 to E-8, printed pp. 479-481, as
//! printed:
//!
//! - eq. E-30 and E-33: `(C_L,alpha)_wf = K_I C_L,alpha,w`,
//!   `K_I = (1 + 2.15 b_f/b) S_net/S + pi/(2 C_L,alpha,w) b_f^2/S`, and
//!   eq. E-34: `K_II = (1 + 0.7 b_f/b) S_net/S` (both for `b_f/b < 0.2`);
//! - eq. E-36: `(x_ac/c)_wf = (x_ac/c)_w + Delta_f1 x_ac/c + Delta_f2 x_ac/c`,
//!   `(x_ac/c)_w` the wing's own aerodynamic centre, here the lattice's, the
//!   one the neutral point uses ([`lattice_wing_ac_m`]), not the quarter
//!   chord of a reference frame;
//! - eq. E-37: `Delta_f1 x_ac/c = -1.8/(C_L,alpha)_wf b_f h_f l_fn/(S c)`;
//! - eq. E-38: `Delta_f2 x_ac/c = 0.273/(1 + lambda) b_f c_g (b - b_f) /
//!   (c^2 (b + 2.15 b_f)) tan(Lambda_1/4)`;
//! - eq. E-40: `Delta_f C_m,ac = -1.8 (1 - 2.5 b_f/l_f) pi b_f h_f l_f/(4 S c)
//!   C_L0/(C_L,alpha)_wf`, `C_L0` the clean lift at zero fuselage angle of
//!   attack (sec. E-9.3), times the ratio of the actual cross-section area to
//!   `(pi/4) b_f h_f` for a section that is not near-circular. The flap's own
//!   moment is the wing-body term of Torenbeek, *Advanced Aircraft Design*,
//!   2013, sec. 6.3.3, p. 168 (`-0.3 Delta C_L0`, its "wing" the airplane less
//!   the tail, fuselage included, p. 165), so the flap lift does not enter
//!   eq. E-40 a second time;
//! - eq. E-29 at `alpha_f = 0`: `C_L0 = (C_L,alpha)_wf [-alpha_01 eps_t +
//!   (K_II/K_I) (i_w - (alpha_l0)_r)] + Delta_z C_L`, the root incidence
//!   carried over by `K_II`, the twist term by `K_I`, with Hoerner's
//!   vertical-position term `Delta_z C_L S/(c_r b_f)` = -0.1 for a high wing,
//!   0 mid and +0.1 low (p. 479), linear in the root's height over the body
//!   half height between those three points [E]. The twist term
//!   `-alpha_01 eps_t` is computed numerically on the built twist, as
//!   sec. E-4.3 asks for a twist that is not linearly lofted;
//! - eq. E-41: the nacelles, `sum k_n b_n^2 l_n/(S c (C_L,alpha)_wf)`, and
//!   eq. E-42: windmilling tractor propellers,
//!   `-0.05 sum B_p D_p^2 l_p/(S c (C_L,alpha)_wf)`, both arms positive ahead
//!   of the quarter chord of the MAC.
//!
//! `b_f` and `h_f` are the largest width and height of the primary fuselage,
//! `l_f` its length, `l_fn` the length from its nose to where the wing
//! leading edge meets the fuselage side (Fig. E-13), `c_g = S/b` the mean
//! geometric chord,
//! `c` the mean aerodynamic chord, `lambda` the taper ratio and `S_net` the
//! wing area outside the fuselage. `c` is the planform's own geometric MAC
//! (the lofted wing's) and the arms run to its quarter chord: the terms are
//! lengths and moments of the airplane, which a published reference chord
//! or LEMAC must not move; the balances convert them into the reference
//! frame ([`super::support::wing_body_ac_frac`]).

use super::support::LandingPitchingMoment;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

/// Every term of the wing/fuselage/nacelle aerodynamic centre and of the
/// fuselage pitching moment, fractions of the planform's own mean
/// aerodynamic chord ([`Self::mac_m`]) unless stated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct WingBodyTerms {
    /// The planform's geometric mean aerodynamic chord, m: the lofted
    /// wing's, whatever reference chord the airplane publishes.
    pub(super) mac_m: f64,
    /// Its quarter-chord station, m: the geometric `(x_ac/c)_w = 0.25`
    /// fallback when the lattice gives no wing aerodynamic centre.
    pub(super) x_c4_mac_m: f64,
    /// Isolated-wing lift-curve slope, per radian (DATCOM, Mach 0.2).
    pub(super) cl_alpha_wing: f64,
    /// Eq. E-33's `K_I`.
    pub(super) k_i: f64,
    /// Eq. E-34's `K_II`.
    pub(super) k_ii: f64,
    /// `(C_L,alpha)_wf`, per radian (eq. E-30).
    pub(super) cl_alpha_wf: f64,
    /// Eq. E-37, forward (negative).
    pub(super) fuselage_shift: f64,
    /// Eq. E-38, aft (positive) on a swept wing.
    pub(super) lift_loss_shift: f64,
    /// FLAGGED empirical tail-off tunnel-data correction
    /// (`alas_stab::neutral_point::tunnel_ac_correction`), forward.
    pub(super) tunnel_correction: f64,
    /// Eq. E-41, all nacelles.
    pub(super) nacelle_shift: f64,
    /// Eq. E-42, all propellers; zero without a turboprop engine.
    pub(super) propeller_shift: f64,
    /// Eq. E-40 over `C_L0`: `Delta_f C_m,ac = this * C_L0`.
    pub(super) fuselage_cm_ac_per_zero_alpha_lift: f64,
    /// Eq. E-29's `Delta_z C_L`, the wing's vertical position on the body.
    pub(super) vertical_position_lift: f64,
}

impl WingBodyTerms {
    /// The four shifts of eq. E-36 (nacelle and propeller terms added), m,
    /// positive aft.
    pub(super) fn ac_shift_m(&self) -> f64 {
        (self.fuselage_shift
            + self.lift_loss_shift
            + self.tunnel_correction
            + self.nacelle_shift
            + self.propeller_shift)
            * self.mac_m
    }

    /// The wing/fuselage/nacelle aerodynamic centre, m (eq. E-36): the
    /// wing's own aerodynamic centre `x_ac_wing_m` and [`Self::ac_shift_m`].
    pub(super) fn ac_x_m(&self, x_ac_wing_m: f64) -> f64 {
        x_ac_wing_m + self.ac_shift_m()
    }

    /// The fuselage moment about the aerodynamic centre, eq. E-40, with the
    /// clean wing/fuselage lift at zero fuselage angle
    /// ([`Self::clean_zero_alpha_lift`]), flaps retracted whatever the
    /// setting (see the module note on eq. E-40).
    pub(super) fn fuselage_cm_ac(&self, wing: &LandingPitchingMoment) -> f64 {
        self.fuselage_cm_ac_per_zero_alpha_lift * self.clean_zero_alpha_lift(wing)
    }

    /// The clean wing/fuselage lift at zero fuselage angle of attack, eq.
    /// E-29 at `alpha_f = 0`: the wing's root-incidence lift carried over by
    /// `K_II`, its twist lift (the rest of the wing's own) by `K_I`, and the
    /// vertical-position term.
    pub(super) fn clean_zero_alpha_lift(&self, wing: &LandingPitchingMoment) -> f64 {
        self.k_ii * wing.root_zero_alpha_lift
            + self.k_i * (wing.clean_zero_alpha_lift - wing.root_zero_alpha_lift)
            + self.vertical_position_lift
    }

    /// [`Self::clean_zero_alpha_lift`] plus the flap's increment.
    pub(super) fn zero_alpha_lift(&self, wing: &LandingPitchingMoment) -> f64 {
        self.clean_zero_alpha_lift(wing) + wing.flap_zero_alpha_lift
    }
}

/// [`WingBodyTerms`] for `plane`, on its own reference area, span and mean
/// aerodynamic chord; `None` without a wing, a fuselage of two stations or a
/// finite set of terms.
pub(super) fn wing_body_terms(plane: &Airplane, config: &AlasConfig) -> Option<WingBodyTerms> {
    let wing = plane.wings.first()?;
    let fuselage = plane.fuselages.first().filter(|fus| fus.xsecs.len() >= 2)?;
    // Every term on the planform's own (geometric) MAC, the chord of
    // Torenbeek's fractions, never the reference frame's: a published
    // reference chord or LEMAC must not move a term in metres.
    let (s, b, c) = (plane.s_ref, plane.b_ref, wing.mean_aerodynamic_chord());
    let x_c4_mac = wing.aerodynamic_center(0.25)[0];
    if !(s > 0.0 && b > 0.0 && c > 0.0 && x_c4_mac.is_finite()) {
        return None;
    }
    let sweep = wing.mean_sweep_angle(0.25).to_radians();
    let taper = wing.taper_ratio();
    let cl_alpha_wing = alas_stab::static_stability::datcom(
        b * b / s,
        sweep,
        wing.reference_planform().taper_ratio(),
        0.2,
    );
    let body = BodyDimensions::of(fuselage)?;
    let (b_f, h_f, l_f) = (body.width_m, body.height_m, body.length_m);
    let net_fraction = exposed_area_fraction(wing, 0.5 * b_f)?;
    let k_i = (1.0 + 2.15 * b_f / b) * net_fraction
        + std::f64::consts::PI / (2.0 * cl_alpha_wing) * b_f * b_f / s;
    let k_ii = (1.0 + 0.7 * b_f / b) * net_fraction;
    let cl_alpha_wf = k_i * cl_alpha_wing;

    let carried = wing.carried_box();
    let (x_le_side, chord_side) = section_at(&carried, 0.5 * b_f).unwrap_or_else(|| {
        let (root_le, root_chord) = wing.design_root();
        (root_le[0], root_chord)
    });
    let l_fn = (x_le_side - body.nose_x_m).max(0.0);
    let fuselage_shift = -1.8 / cl_alpha_wf * b_f * h_f * l_fn / (s * c);
    let c_g = s / b;
    let lift_loss_shift = 0.273 / (1.0 + taper.max(0.0)) * b_f * c_g * (b - b_f)
        / (c * c * (b + 2.15 * b_f))
        * sweep.tan();

    let (nacelle_shift_m, _) = alas_stab::neutral_point::nacelle_terms(
        plane,
        cl_alpha_wf,
        x_c4_mac,
        x_le_side + chord_side,
    );
    let nacelle_shift = nacelle_shift_m / c;
    let propeller_shift = config
        .geometry
        .engine
        .turboprop
        .as_ref()
        .and_then(|spec| {
            Some((
                f64::from(spec.propeller_blade_count?),
                spec.propeller_diameter_m,
            ))
        })
        .map_or(0.0, |(blades, diameter)| {
            plane.fuselages[1..]
                .iter()
                .filter_map(|nacelle| nacelle.xsecs.first())
                .map(|disc| -0.05 * blades * diameter * diameter * (x_c4_mac - disc.xyz_c[0]))
                .sum::<f64>()
                / (s * c * cl_alpha_wf)
        });
    let fuselage_cm_ac_per_zero_alpha_lift =
        -1.8 * (1.0 - 2.5 * b_f / l_f) * std::f64::consts::PI * b_f * h_f * l_f
            / (4.0 * s * c)
            / cl_alpha_wf
            * body.area_over_ellipse;
    let vertical_position_lift = vertical_position_lift(wing, fuselage, b_f, s);
    let terms = WingBodyTerms {
        mac_m: c,
        x_c4_mac_m: x_c4_mac,
        cl_alpha_wing,
        k_i,
        k_ii,
        cl_alpha_wf,
        fuselage_shift,
        lift_loss_shift,
        // The tunnel-data correction is a fraction of the reference MAC (its
        // data basis), carried here as a fraction of the planform MAC.
        tunnel_correction: alas_stab::neutral_point::tunnel_ac_correction(sweep)
            * if plane.c_ref > 0.0 {
                plane.c_ref / c
            } else {
                1.0
            },
        nacelle_shift,
        propeller_shift,
        fuselage_cm_ac_per_zero_alpha_lift,
        vertical_position_lift,
    };
    [
        terms.vertical_position_lift,
        terms.cl_alpha_wf,
        terms.ac_shift_m(),
        terms.k_ii,
        terms.fuselage_cm_ac_per_zero_alpha_lift,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(terms)
}

/// The main wing's name, the one the neutral point looks the wing up by.
const MAIN_WING_NAME: &str = "Main Wing";

/// The wing's own aerodynamic centre `(x_ac/c)_w` of eq. E-36, m: the
/// isolated main wing's lattice aerodynamic centre (the wing named as the
/// neutral point names it, else the first; alone in the lattice, as eq.
/// E-36's wing term is the wing without body or tail), Goethert-stretched at
/// Mach 0.2, sea level, the in-loop lattice resolution and the analysis
/// alpha probes. `None` when the lattice does not assemble or solve or gives
/// no finite centre.
pub(super) fn lattice_wing_ac_m(plane: &Airplane, config: &AlasConfig) -> Option<f64> {
    /// What the centre is a function of: the wing, the lattice resolution and
    /// the two probe angles. The envelope asks for it on every ground-plane
    /// balance of every candidate, each time with the same wing.
    struct Entry {
        wing: Wing,
        key: [u64; 7],
        centre: Option<f64>,
    }
    const ENTRIES: usize = 8;
    static MEMO: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<Entry>>> =
        std::sync::OnceLock::new();
    let memo = MEMO.get_or_init(Default::default);
    let analysis = &config.analysis;
    let key = [
        plane.s_ref.to_bits(),
        plane.c_ref.to_bits(),
        plane.b_ref.to_bits(),
        analysis.spanwise_resolution as u64,
        analysis.chordwise_resolution as u64,
        analysis.autobalance_alpha_low_deg.to_bits(),
        analysis.autobalance_alpha_high_deg.to_bits(),
    ];
    let main = plane
        .wings
        .iter()
        .find(|wing| wing.name == MAIN_WING_NAME)
        .or_else(|| plane.wings.first())?;
    if let Ok(entries) = memo.lock() {
        if let Some(entry) = entries.iter().find(|e| e.key == key && e.wing == *main) {
            return entry.centre;
        }
    }
    let centre = lattice_wing_ac_uncached(plane, config);
    if let Ok(mut entries) = memo.lock() {
        if !entries.iter().any(|e| e.key == key && e.wing == *main) {
            if entries.len() == ENTRIES {
                entries.pop_front();
            }
            entries.push_back(Entry {
                wing: main.clone(),
                key,
                centre,
            });
        }
    }
    centre
}

fn lattice_wing_ac_uncached(plane: &Airplane, config: &AlasConfig) -> Option<f64> {
    use alas_aero::operating_point::OperatingPoint;
    use alas_stab::neutral_point::{beta_for_mach, goethert_stretch, per_surface_contributions};
    const MACH: f64 = 0.2;
    let analysis = &config.analysis;
    let beta = beta_for_mach(MACH);
    let main = plane
        .wings
        .iter()
        .find(|wing| wing.name == MAIN_WING_NAME)
        .or_else(|| plane.wings.first())?;
    let mut alone = plane.clone();
    alone.wings = vec![main.clone()];
    let stretched = goethert_stretch(&alone, beta);
    let atmosphere = alas_atmo::Atmosphere::new(0.0);
    let velocity = MACH * atmosphere.speed_of_sound();
    let system = alas_aero::vlm::VlmSystem::assemble(
        &stretched,
        usize::try_from(analysis.spanwise_resolution.max(1)).ok()?,
        usize::try_from(analysis.chordwise_resolution.max(1)).ok()?,
    )
    .ok()?;
    let solve = |alpha_deg| {
        system
            .solve(&OperatingPoint::new(
                atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0,
            ))
            .ok()
    };
    let lo = solve(analysis.autobalance_alpha_low_deg)?;
    let hi = solve(analysis.autobalance_alpha_high_deg)?;
    let x_ac = per_surface_contributions(&stretched, &lo, &hi)
        .first()?
        .x_ac
        * beta;
    x_ac.is_finite().then_some(x_ac)
}

/// The wing's lift in ground effect over its lift out of it at the same
/// angle of attack, Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982,
/// App. G, eq. G-76 (p. 553):
///
/// `C_L/C_L,inf = 1 + sigma - sigma A cos L / (2 cos L + sqrt(A^2 + (2 cos L)^2))
///  - beta/(4 pi h/c_g) (C_L,inf - C_L,alpha/(16 h/c_g))`,
///
/// `L` the half-chord sweep, `h` the height of the MAC quarter chord over the
/// ground, `c_g = S/b`, and the trailing-vortex and bound-vortex functions of
/// Fig. G-26 (p. 554) in their printed closed forms
/// `sigma = exp(-2.48 (2 h_eff/b)^0.768)`,
/// `beta = sqrt(1 + (2 h_eff/b)^2) - 2 h_eff/b`, `2 h_eff = h_ac + h_TE` the
/// quarter-chord and trailing-edge heights of the MAC. `lift` is `C_L,inf`.
/// 1 for a non-finite or non-positive height.
pub(super) fn ground_effect_lift_ratio(
    plane: &Airplane,
    terms: &WingBodyTerms,
    lift: f64,
    quarter_chord_height_m: f64,
    trailing_edge_height_m: f64,
) -> f64 {
    let Some(wing) = plane.wings.first() else {
        return 1.0;
    };
    ground_effect_lift_ratio_of(
        plane.s_ref,
        plane.b_ref,
        wing.mean_sweep_angle(0.5).to_radians().cos(),
        terms.cl_alpha_wing,
        lift,
        quarter_chord_height_m,
        trailing_edge_height_m,
    )
}

/// [`ground_effect_lift_ratio`] on its scalars: reference area `s` and span
/// `b`, the cosine of the mid-chord sweep, the isolated-wing lift-curve slope
/// per radian, the free-air lift and the two heights.
pub(super) fn ground_effect_lift_ratio_of(
    s: f64,
    b: f64,
    half_chord_cos: f64,
    cl_alpha_wing: f64,
    lift: f64,
    quarter_chord_height_m: f64,
    trailing_edge_height_m: f64,
) -> f64 {
    let (h_ac, h_te) = (quarter_chord_height_m, trailing_edge_height_m);
    if !(h_ac > 0.0 && h_te > 0.0 && s > 0.0 && b > 0.0 && lift.is_finite()) {
        return 1.0;
    }
    let aspect_ratio = b * b / s;
    let relative = (h_ac + h_te) / b;
    let sigma = (-2.48 * relative.powf(0.768)).exp();
    let beta = (1.0 + relative * relative).sqrt() - relative;
    let height_over_chord = h_ac / (s / b);
    let trailing = sigma
        - sigma * aspect_ratio * half_chord_cos
            / (2.0 * half_chord_cos
                + (aspect_ratio * aspect_ratio + 4.0 * half_chord_cos * half_chord_cos).sqrt());
    let bound = beta / (4.0 * std::f64::consts::PI * height_over_chord)
        * (lift - cl_alpha_wing / (16.0 * height_over_chord));
    let ratio = 1.0 + trailing - bound;
    if ratio.is_finite() && ratio > 0.0 {
        ratio
    } else {
        1.0
    }
}

/// Eq. E-29's `Delta_z C_L = k c_r b_f / S`, `k` Hoerner's -0.1 (high wing,
/// the root at the top of the body), 0 (mid) or +0.1 (low, at its bottom),
/// linear in between on the height of the root leading edge over the body
/// centre line, in body half heights, at the root chord's mid station.
fn vertical_position_lift(wing: &Wing, fuselage: &Fuselage, b_f: f64, s: f64) -> f64 {
    let Some(root) = wing.xsecs.first() else {
        return 0.0;
    };
    let x_mid = root.xyz_le[0] + 0.5 * root.chord;
    let Some(section) = fuselage.xsecs.iter().min_by(|a, b| {
        (a.xyz_c[0] - x_mid)
            .abs()
            .total_cmp(&(b.xyz_c[0] - x_mid).abs())
    }) else {
        return 0.0;
    };
    if section.height <= 0.0 {
        return 0.0;
    }
    let position = ((root.xyz_le[2] - section.xyz_c[2]) / (0.5 * section.height)).clamp(-1.0, 1.0);
    -0.1 * position * root.chord * b_f / s
}

/// The primary fuselage's largest width and height, its length and nose
/// station, m, and the area of its largest section over `(pi/4) b_f h_f`.
struct BodyDimensions {
    width_m: f64,
    height_m: f64,
    length_m: f64,
    nose_x_m: f64,
    area_over_ellipse: f64,
}

impl BodyDimensions {
    fn of(fuselage: &Fuselage) -> Option<Self> {
        let xs = fuselage.xsecs.iter().map(|section| section.xyz_c[0]);
        let nose_x_m = xs.clone().fold(f64::INFINITY, f64::min);
        let length_m = xs.fold(f64::NEG_INFINITY, f64::max) - nose_x_m;
        let width_m = fuselage
            .xsecs
            .iter()
            .map(|section| section.width)
            .fold(0.0, f64::max);
        let height_m = fuselage
            .xsecs
            .iter()
            .map(|section| section.height)
            .fold(0.0, f64::max);
        let largest = fuselage
            .xsecs
            .iter()
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))?;
        (length_m > 0.0 && width_m > 0.0 && height_m > 0.0).then(|| Self {
            width_m,
            height_m,
            length_m,
            nose_x_m,
            area_over_ellipse: superellipse_area_over_ellipse(largest.shape),
        })
    }
}

/// The area of a superellipse `|y/a|^n + |z/b|^n = 1` over that of the
/// ellipse on the same axes, `(4/pi) integral_0^1 (1 - t^n)^(1/n) dt`
/// (midpoint rule): 1 for `n = 2`, above 1 for a squarer section.
fn superellipse_area_over_ellipse(exponent: f64) -> f64 {
    if !(exponent.is_finite() && exponent > 0.0) {
        return 1.0;
    }
    const STEPS: u32 = 4000;
    let quadrant: f64 = (0..STEPS)
        .map(|k| {
            let t = (f64::from(k) + 0.5) / f64::from(STEPS);
            (1.0 - t.powf(exponent)).max(0.0).powf(1.0 / exponent)
        })
        .sum::<f64>()
        / f64::from(STEPS);
    4.0 / std::f64::consts::PI * quadrant
}

/// Leading-edge station and chord of `wing` at the spanwise station `y_abs`,
/// linear between the bounding sections; `None` outside every pair.
fn section_at(wing: &Wing, y_abs: f64) -> Option<(f64, f64)> {
    wing.xsecs.windows(2).find_map(|pair| {
        let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        (y_abs >= lo && y_abs <= hi && hi - lo > 1e-9).then(|| {
            let t = (y_abs - y0) / (y1 - y0);
            (
                pair[0].xyz_le[0] + t * (pair[1].xyz_le[0] - pair[0].xyz_le[0]),
                pair[0].chord + t * (pair[1].chord - pair[0].chord),
            )
        })
    })
}

/// The share of the wing's planform area outside `|y| = half_width`
/// (`S_net/S`): chord integrated linearly between the sections.
fn exposed_area_fraction(wing: &Wing, half_width: f64) -> Option<f64> {
    let (mut total, mut exposed) = (0.0, 0.0);
    for pair in wing.xsecs.windows(2) {
        let (y0, y1) = (pair[0].xyz_le[1].abs(), pair[1].xyz_le[1].abs());
        let (c0, c1) = (pair[0].chord, pair[1].chord);
        let span = (y1 - y0).abs();
        if span <= 1e-12 {
            continue;
        }
        total += 0.5 * (c0 + c1) * span;
        let (lo, hi, c_lo, c_hi) = if y0 <= y1 {
            (y0, y1, c0, c1)
        } else {
            (y1, y0, c1, c0)
        };
        if hi <= half_width {
            continue;
        }
        let start = lo.max(half_width);
        let c_start = c_lo + (c_hi - c_lo) * (start - lo) / (hi - lo);
        exposed += 0.5 * (c_start + c_hi) * (hi - start);
    }
    (total > 0.0).then_some(exposed / total)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_circular_section_has_the_ellipse_area() {
        assert!((superellipse_area_over_ellipse(2.0) - 1.0).abs() < 1.0e-4);
        assert!(superellipse_area_over_ellipse(4.0) > 1.0);
        assert_eq!(superellipse_area_over_ellipse(f64::NAN), 1.0);
    }

    /// Every preset by hand from eq. E-30 to E-42 on the built dimensions,
    /// and the per-term table (printed).
    // The test log carries the terms behind the assertions.
    #[allow(clippy::print_stdout)]
    #[test]
    fn the_terms_follow_torenbeeks_appendix_e_on_every_preset() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let terms = wing_body_terms(&plane, &config).expect("finite terms");
            let body = BodyDimensions::of(&plane.fuselages[0]).expect("a body");
            let (s, b, c) = (
                plane.s_ref,
                plane.b_ref,
                plane.wings[0].mean_aerodynamic_chord(),
            );
            assert_eq!(terms.mac_m, c, "{name}");
            assert!(
                body.width_m / b < 0.2,
                "{name}: b_f/b outside eq. E-33's range"
            );
            assert!(
                terms.k_i > terms.k_ii && terms.k_ii > 0.0,
                "{name}: {terms:?}"
            );
            assert!((terms.cl_alpha_wf - terms.k_i * terms.cl_alpha_wing).abs() < 1.0e-12);
            assert!(terms.fuselage_shift < 0.0, "{name}");
            assert!(terms.lift_loss_shift >= 0.0, "{name}");
            let e40 = -1.8
                * (1.0 - 2.5 * body.width_m / body.length_m)
                * std::f64::consts::PI
                * body.width_m
                * body.height_m
                * body.length_m
                / (4.0 * s * c)
                / terms.cl_alpha_wf
                * body.area_over_ellipse;
            assert!(
                (terms.fuselage_cm_ac_per_zero_alpha_lift - e40).abs() < 1.0e-12,
                "{name}"
            );
            assert!(terms.fuselage_cm_ac_per_zero_alpha_lift < 0.0, "{name}");
            let root = &plane.wings[0].xsecs[0];
            assert!(
                terms.vertical_position_lift.abs() <= 0.1 * root.chord * body.width_m / s + 1.0e-12,
                "{name}"
            );
            let turboprop = config.geometry.engine.turboprop.is_some();
            assert_eq!(terms.propeller_shift != 0.0, turboprop, "{name}");
            println!(
                "{name}: CLa_w {:.3} K_I {:.3} K_II {:.3} CLa_wf {:.3} E-37 {:+.4} E-38 {:+.4} \
                 E-41 {:+.4} E-42 {:+.4} shift {:+.3} m E-40/CL0 {:+.4} dzCL {:+.4} b_f {:.2} h_f {:.2} \
                 l_f {:.2}",
                terms.cl_alpha_wing,
                terms.k_i,
                terms.k_ii,
                terms.cl_alpha_wf,
                terms.fuselage_shift,
                terms.lift_loss_shift,
                terms.nacelle_shift,
                terms.propeller_shift,
                terms.ac_shift_m(),
                terms.fuselage_cm_ac_per_zero_alpha_lift,
                terms.vertical_position_lift,
                body.width_m,
                body.height_m,
                body.length_m,
            );
        }
    }
}
