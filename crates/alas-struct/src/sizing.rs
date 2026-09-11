// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_sizing.py
// Reference: alas @ rust-port-baseline.

//! Direct strength-based wingbox sizing.
//!
//! [`size_wingbox`] sizes the spar caps directly from strength -- margin of
//! safety zero by construction at the root, the bending-critical station --
//! with no mass-target bisection, then applies the spar-cap taper law and the
//! geometric cap width/height limits, sizes the webs from root shear, fixes
//! the skin at its configured minimum, and derives the rib spacing from a
//! panel-buckling criterion.
//!
//! Loads come from [`crate::loads`] (elliptic distribution, no inertial
//! relief -- the conservative choice for strength sizing). Moment and shear
//! are split across the spars weighted by each spar's local section depth, so
//! a deeper spar carries proportionally more of the bending moment and a
//! partial-span spar, zeroed outboard of the break, carries none of it there.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::{self, LoadCase};

/// Material-modelling qualification carried when a wingbox sizing result
/// uses composite materials under an effective isotropic proxy.
///
/// An effective isotropic proxy is appropriate for preliminary sizing passes,
/// but must never be confused with or presented as a certified laminate
/// stress analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompositeProxyDeclaration {
    /// Material-family evidence tier and citation.
    pub source: &'static str,
    /// Model applicability and non-certification disclosure.
    pub applicability: &'static str,
    /// Calibrated relative uncertainty if known/calibrated, or `None` if uncalibrated.
    ///
    /// Per strict audit override, uncalibrated uncertainty must be represented
    /// explicitly as unknown (`None`), not as zero or a fabricated numeric figure.
    pub relative_uncertainty: Option<f64>,
}

/// Per-spar sizing result, sampled at [`WingboxSizing::y_stations`].
#[derive(Debug, Clone, PartialEq)]
pub struct SparSizing {
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Free web height at each station, m.
    pub h: Vec<f64>,
    /// Cap flange width (tapered) at each station, m.
    pub w_cap: Vec<f64>,
    /// Cap flange thickness (tapered) at each station, m.
    pub t_cap: Vec<f64>,
    /// One-flange cap area at each station, m^2.
    pub a_cap: Vec<f64>,
    /// Uniform web thickness, m.
    pub t_web: f64,
    /// Bending-moment fraction this spar carries at each station.
    pub frac_moment: Vec<f64>,
    /// Margin of safety at each station -- `+inf` where the local demand is
    /// below 1 N.m (near the tip). Expected `>= 0` near the root.
    pub margin_of_safety: Vec<f64>,
}

/// The station returned by [`WingboxSizing::controlling_margin`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllingMargin {
    /// Raw margin of safety at the controlling station, full precision.
    pub margin: f64,
    /// Index into [`WingboxSizing::spars`] and [`WingboxSizing::spar_fracs`].
    pub spar_index: usize,
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Index into [`WingboxSizing::y_stations`] and
    /// [`WingboxSizing::eta_stations`].
    pub station_index: usize,
    /// Spanwise station, m.
    pub y_m: f64,
    /// Normalized spanwise station, `y / semi_span`.
    pub eta: f64,
}

/// The sized wingbox: per-station geometry, rib layout and mass breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct WingboxSizing {
    /// Spanwise stations, m.
    pub y_stations: Vec<f64>,
    /// Normalized spanwise stations, `y / semi_span`.
    pub eta_stations: Vec<f64>,
    /// Local chord at each station, m.
    pub chord: Vec<f64>,
    /// The spar chordwise fractions, in the geometry's sorted order.
    pub spar_fracs: Vec<f64>,
    /// Per-spar sizing.
    pub spars: Vec<SparSizing>,
    /// Skin thickness, m.
    pub t_skin: f64,
    /// Number of ribs.
    pub num_ribs: i64,
    /// Panel-buckling allowable rib spacing, m.
    pub rib_spacing_m: f64,
    /// Semi-wing mass by component, kg.
    pub mass_breakdown_kg: MassBreakdown,
    /// Total semi-wing structural mass, kg.
    pub total_mass_kg: f64,
    /// The name of the load case that sized the box.
    pub sizing_load_case: &'static str,
    /// Material qualification declaration, present whenever any wingbox
    /// material is composite. `None` for all-metallic wings.
    pub composite_declaration: Option<CompositeProxyDeclaration>,
}

impl WingboxSizing {
    /// Installed spanwise pitch between adjacent ribs, including the root and
    /// tip ribs. This is distinct from [`Self::rib_spacing_m`], which is the
    /// maximum pitch permitted by the panel-buckling calculation and may be
    /// larger or smaller than the pitch selected by an explicit rib-count
    /// override.
    pub fn installed_rib_spacing_m(&self) -> f64 {
        if self.num_ribs > 1 {
            let first = self.y_stations.first().copied().unwrap_or(f64::NAN);
            let last = self.y_stations.last().copied().unwrap_or(f64::NAN);
            (last - first) / (self.num_ribs - 1) as f64
        } else {
            f64::NAN
        }
    }

    /// Whether the selected rib count satisfies the panel-buckling limit.
    ///
    /// Automatically sized layouts obey this by construction. An explicit
    /// rib-count override is still checked here so a diagnostic sizing result
    /// cannot be promoted to a structural success when its installed bays
    /// are wider than the applicable allowable spacing.
    pub fn rib_spacing_pass(&self) -> bool {
        let installed = self.installed_rib_spacing_m();
        installed.is_finite() && self.rib_spacing_m.is_finite() && installed <= self.rib_spacing_m
    }

    /// The smallest strength margin found in the sized spars.
    ///
    /// A NaN margin is returned as `NaN` so callers cannot mistake an
    /// incomplete sizing calculation for a successful one. Positive infinity
    /// is a valid margin for a station whose demand is below the numerical
    /// reporting threshold.
    pub fn minimum_margin_of_safety(&self) -> f64 {
        self.controlling_margin().map_or(f64::NAN, |c| c.margin)
    }

    /// The station that controls [`Self::minimum_margin_of_safety`], with its
    /// location, for diagnostics.
    ///
    /// Rounding the controlling margin to a fixed number of decimals -- as a
    /// failure message meant for humans naturally does -- collapses every
    /// value between roughly `-5e-7` and `0` to the same displayed
    /// `-0.000000`, hiding whether the shortfall is floating-point noise at
    /// the active root boundary or a real, if small, structural deficit.
    /// Callers that need to tell those apart must use the raw
    /// [`ControllingMargin::margin`] here, not a display-rounded value.
    ///
    /// `None` only when there are no spar stations at all. As with
    /// [`Self::minimum_margin_of_safety`], a NaN margin takes priority over
    /// any finite one so an incomplete calculation is never reported as a
    /// located structural result.
    pub fn controlling_margin(&self) -> Option<ControllingMargin> {
        let mut best: Option<ControllingMargin> = None;
        for (spar_index, spar) in self.spars.iter().enumerate() {
            for (station_index, &margin) in spar.margin_of_safety.iter().enumerate() {
                let candidate = ControllingMargin {
                    margin,
                    spar_index,
                    chord_fraction: self.spar_fracs.get(spar_index).copied().unwrap_or(f64::NAN),
                    station_index,
                    y_m: self
                        .y_stations
                        .get(station_index)
                        .copied()
                        .unwrap_or(f64::NAN),
                    eta: self
                        .eta_stations
                        .get(station_index)
                        .copied()
                        .unwrap_or(f64::NAN),
                };
                let replace = match &best {
                    None => true,
                    Some(current) if current.margin.is_nan() => false,
                    Some(current) => margin.is_nan() || margin < current.margin,
                };
                if replace {
                    best = Some(candidate);
                }
            }
        }
        best
    }

    /// Whether every sized spar station has a non-NaN non-negative margin.
    pub fn strength_margins_pass(&self) -> bool {
        !self.spars.is_empty()
            && self.spars.iter().all(|spar| {
                !spar.margin_of_safety.is_empty()
                    && spar
                        .margin_of_safety
                        .iter()
                        .all(|&margin| !margin.is_nan() && margin >= 0.0)
            })
    }
}

/// The four semi-wing mass components upstream keys by name in its
/// `mass_breakdown_kg` dict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    /// Spar caps, kg.
    pub spar_caps: f64,
    /// Spar webs, kg.
    pub spar_webs: f64,
    /// Skin, kg.
    pub skin: f64,
    /// Ribs, kg.
    pub ribs: f64,
}

/// NumPy `linspace(start, stop, n)` with `endpoint=True`: `n` evenly spaced
/// points, the last pinned exactly to `stop`.
fn linspace(start: f64, stop: f64, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![start];
    }
    let step = (stop - start) / (n - 1) as f64;
    let mut values: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
    values[n - 1] = stop;
    values
}

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
fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    let mut acc = 0.0;
    for i in 0..y.len().saturating_sub(1) {
        acc += (x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0;
    }
    acc
}

/// The number of stations needed to keep every uniform rib panel at or below
/// the maximum spacing. Both the root and tip are ribs, so panels plus one is
/// the count. This is the count form of the panel-buckling sizing rule.
fn rib_count_from_max_spacing(semi_span_m: f64, max_spacing_m: f64) -> i64 {
    (semi_span_m / max_spacing_m).ceil() as i64 + 1
}

/// The spar-cap taper law: full section up to `eta_lock`, then linear taper to
/// `tip_fraction` at the tip -- `_cap_taper`.
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
/// caps never fill the web. When that thickness clip binds -- a shallow rear
/// spar with a low-allowable alloy at a high root moment does it -- the same
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

/// Which cap-sizing law a solve applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizingLaw {
    /// Every station carries its own bending moment: the root flange widens
    /// when its thickness clip binds and an outboard station whose tapered
    /// flange falls short of the local moment is sized up to it.
    Product,
    /// The frozen reference law: the root cap alone is sized, its thickness
    /// clipped at a fifth of the spar height, and the outboard caps follow the
    /// taper whatever the local moment. A shallow spar can be left under
    /// strength, which the fixtures record.
    Frozen,
}

/// Size the wingbox directly from strength -- `size_wingbox`.
///
/// The load cases come from [`crate::loads::load_cases`]; the box is sized to
/// whichever produces the larger root bending moment. Every station is left
/// with a non-negative strength margin wherever the section geometry admits
/// one; see [`size_wingbox_reference_compatibility`] for the frozen law.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> WingboxSizing {
    size_wingbox_with_law(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        SizingLaw::Product,
    )
}

/// The frozen reference form of [`size_wingbox`], for the parity fixtures.
///
/// The reference sizes the root cap only, with its thickness clipped at a
/// fifth of the spar height, and tapers the outboard caps regardless of the
/// local moment, so a shallow spar with a low-allowable alloy at a high root
/// moment comes out under strength; the fixtures pin that behaviour. Product
/// callers use [`size_wingbox`], whose result the mass reconciliation gate
/// accepts only with non-negative margins.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox_reference_compatibility(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> WingboxSizing {
    size_wingbox_with_law(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        SizingLaw::Frozen,
    )
}

// The public entry points' signature plus the law they differ in.
#[allow(clippy::too_many_arguments)]
fn size_wingbox_with_law(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
    law: SizingLaw,
) -> WingboxSizing {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();

    let cases = loads::load_cases(req, cfg.additional_safety_factor);
    // Sized by whichever case has the larger |root moment|; for an elliptic
    // cantilever this is the largest |total_force_n|, but comparing moments is
    // robust to future load-model changes.
    let mut worst_idx = 0usize;
    let mut worst_m0 = -1.0;
    let mut case_moments: Vec<(Vec<f64>, Vec<f64>)> = Vec::with_capacity(cases.len());
    for (idx, case) in cases.iter().enumerate() {
        let q = loads::elliptic_distributed_load(&y, wsg.semi_span, case.total_force_n);
        let (_, m) = loads::cantilever_shear_moment(&y, &q);
        if m[0].abs() > worst_m0 {
            worst_m0 = m[0].abs();
            worst_idx = idx;
        }
        case_moments.push((q, m));
    }
    let worst_case: &LoadCase = &cases[worst_idx];
    let (q_sizing, m_sizing) = &case_moments[worst_idx];
    let (v_sizing, _) = loads::cantilever_shear_moment(&y, q_sizing);

    // Per-spar section height (n_spars x n), with a partial-span spar zeroed
    // outboard of the break so it carries no moment, shear or mass there.
    let mut h_all: Vec<Vec<f64>> = wsg
        .spar_fracs
        .iter()
        .map(|&f| eta.iter().map(|&e| wsg.spar_height(e, f)).collect())
        .collect();
    for (i, &full_span) in wsg.spar_full_span.iter().enumerate() {
        if !full_span {
            for j in 0..n {
                if eta[j] > wsg.break_eta + 1e-9 {
                    h_all[i][j] = 0.0;
                }
            }
        }
    }
    let h_sum: Vec<f64> = (0..n)
        .map(|j| {
            let s: f64 = h_all.iter().map(|h| h[j]).sum();
            if s > 1e-9 {
                s
            } else {
                1e-9
            }
        })
        .collect();
    let frac_moment_all: Vec<Vec<f64>> = h_all
        .iter()
        .map(|h| (0..n).map(|j| h[j] / h_sum[j]).collect())
        .collect();

    let tau_allow_web = web_mat.f_allow_pa / (2.0 * 3.0_f64.sqrt());
    let taper = cap_taper(&eta, cfg.cap_taper_eta_lock, cfg.cap_taper_tip_fraction);

    let m0 = m_sizing[0].abs();
    let v0 = v_sizing[0].abs();

    let mut spars: Vec<SparSizing> = Vec::with_capacity(wsg.spar_fracs.len());
    for (i, &frac_c) in wsg.spar_fracs.iter().enumerate() {
        let h_i = &h_all[i];
        let frac_m = &frac_moment_all[i];
        let h_eff: Vec<f64> = h_i.iter().map(|&h| h * 0.85).collect();

        // Root cap: MS = 0 by construction.
        let h_eff0 = h_eff[0].max(1e-6);
        let a_cap0 = (frac_m[0] * m0) / (cap_mat.f_allow_pa * h_eff0);
        let (w_cap0, t_cap0) = match law {
            SizingLaw::Product => root_cap_dimensions(a_cap0, chord[0], h_i[0]),
            SizingLaw::Frozen => {
                let w_cap0 = (0.5 * chord[0]).min(h_i[0] * 0.6).max(1e-6);
                (w_cap0, (a_cap0 / w_cap0).min(h_i[0] * 0.20))
            }
        };

        // Taper outboard; keep width >= thickness and thickness <= H_local/3.
        // The tapered section is the floor: where the local moment demands
        // more than it carries -- a shallow spar whose height falls faster
        // than the moment inboard of the taper lock -- the station is sized
        // up to its own demand, thickness first within the H/3 clip and
        // then width within the half-chord bound, so no station is left
        // short by construction. A station whose tapered flange already
        // carries its moment is untouched.
        let mut t_cap = Vec::with_capacity(n);
        let mut w_cap = Vec::with_capacity(n);
        for j in 0..n {
            let mut t = (t_cap0 * taper[j]).min(h_i[j] / 3.0);
            let mut w = (w_cap0 * taper[j]).max(t);
            let demand = (frac_m[j] * m_sizing[j]).abs();
            if law == SizingLaw::Product && demand > 1.0 {
                let a_req = demand / (cap_mat.f_allow_pa * h_eff[j].max(1e-6));
                if w * t < a_req {
                    t = (a_req / w).min(h_i[j] / 3.0);
                    if w * t < a_req && t > 0.0 {
                        w = (a_req / t).min((0.5 * chord[j]).max(w));
                    }
                }
            }
            t_cap.push(t);
            w_cap.push(w);
        }
        let a_cap: Vec<f64> = (0..n).map(|j| w_cap[j] * t_cap[j]).collect();

        // Web: uniform thickness sized from root shear.
        let t_web = cfg
            .t_web_min_m
            .max((frac_m[0] * v0) / (tau_allow_web * h_eff0));

        // Margin of safety at every station.
        let margin_of_safety: Vec<f64> = (0..n)
            .map(|j| {
                let m_adm = a_cap[j] * cap_mat.f_allow_pa * h_eff[j];
                let demand = (frac_m[j] * m_sizing[j]).abs();
                if demand > 1.0 {
                    m_adm / demand - 1.0
                } else {
                    f64::INFINITY
                }
            })
            .collect();

        spars.push(SparSizing {
            chord_fraction: frac_c,
            h: h_i.clone(),
            w_cap,
            t_cap,
            a_cap,
            t_web,
            frac_moment: frac_m.clone(),
            margin_of_safety,
        });
    }

    // Skin fixed at the configured minimum (no torsional shear-flow upsizing,
    // the same fidelity the analytical model uses).
    let t_skin = cfg.t_skin_min_m;

    // Rib spacing: Euler panel-buckling on the skin between the outermost two
    // spars.
    let frac_min = wsg.spar_fracs.iter().copied().fold(f64::INFINITY, f64::min);
    let frac_max = wsg
        .spar_fracs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let b_box_root = (frac_max - frac_min) * chord[0];
    let h_root_mid = wsg.spar_height(0.0, 0.5 * (frac_min + frac_max));
    let nx = (m_sizing[0].abs() / h_root_mid.max(1e-6)) / b_box_root.max(1e-6);
    let sig_panel = (nx / t_skin).max(1e6);
    let l_rib = (cfg.rib_radius_of_gyration_m
        * (cfg.rib_buckling_coeff * std::f64::consts::PI.powi(2) * skin_mat.e_pa / sig_panel)
            .sqrt())
    .max(0.5);
    let num_ribs = match cfg.num_ribs_override {
        Some(value) => value,
        None => rib_count_from_max_spacing(wsg.semi_span, l_rib).max(10),
    };

    // Mass breakdown (semi-wing).
    let dy = gradient_unit(&y);
    let mut m_caps = 0.0;
    let mut m_webs = 0.0;
    for s in &spars {
        let caps: f64 = (0..n)
            .map(|j| 2.0 * s.a_cap[j] * cap_mat.rho_kg_m3 * dy[j])
            .sum();
        let webs: f64 = (0..n)
            .map(|j| s.t_web * s.h[j] * web_mat.rho_kg_m3 * dy[j])
            .sum();
        m_caps += caps;
        m_webs += webs;
    }
    let m_skin: f64 = (0..n)
        .map(|j| 2.0 * chord[j] * t_skin * skin_mat.rho_kg_m3 * dy[j])
        .sum();

    let xc_full = linspace(0.01, 0.99, 60);
    let mut m_ribs = 0.0;
    for &eta_r in &linspace(0.0, 1.0, num_ribs.max(2) as usize) {
        let c_r = wsg.local_chord(eta_r);
        let heights: Vec<f64> = xc_full
            .iter()
            .map(|&xc| {
                let (zu, zl) = wsg.airfoil_zu_zl(eta_r, xc);
                (zu - zl) * c_r
            })
            .collect();
        let x: Vec<f64> = xc_full.iter().map(|&xc| xc * c_r).collect();
        m_ribs += trapezoid(&heights, &x) * cfg.t_rib_m * rib_mat.rho_kg_m3;
    }

    let total = m_caps + m_webs + m_skin + m_ribs;

    let any_composite = [skin_mat, web_mat, cap_mat, rib_mat]
        .iter()
        .any(|m| m.category == "composite");

    let composite_declaration = if any_composite {
        Some(CompositeProxyDeclaration {
            source: "Open source gap: the real wing box is composite, but no document in .agent/evidence/ \
                     states it. Assigned as an effective isotropic proxy, not a verified material.",
            applicability: "Effective isotropic proxy for a laminate wing box. f_allow is a single \
                            strength-based design allowable, not a laminate allowable; no ply schedule, \
                            stacking sequence, compression-after-impact knockdown, inter-laminar check \
                            or aeroelastic tailoring is modelled. Not a certified laminate analysis. \
                            Gauge is a declared class assumption, not a measured gauge.",
            // Represent uncalibrated relative uncertainty explicitly as unknown (None),
            // never inventing a spurious number or 0.0 per strict audit instructions.
            relative_uncertainty: None,
        })
    } else {
        None
    };

    WingboxSizing {
        y_stations: y,
        eta_stations: eta,
        chord,
        spar_fracs: wsg.spar_fracs.clone(),
        spars,
        t_skin,
        num_ribs,
        rib_spacing_m: l_rib,
        mass_breakdown_kg: MassBreakdown {
            spar_caps: m_caps,
            spar_webs: m_webs,
            skin: m_skin,
            ribs: m_ribs,
        },
        total_mass_kg: total,
        sizing_load_case: worst_case.name,
        composite_declaration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_cap_that_does_not_fit_its_height_clip_widens_instead_of_falling_short() {
        // Fits: the flange stays at 0.6 h and carries the area.
        let (w, t) = root_cap_dimensions(0.01, 6.0, 0.5);
        assert!((w - 0.3).abs() < 1e-12 && (w * t - 0.01).abs() < 1e-12);
        // Does not fit at 0.6 h: the thickness clips at 0.2 h and the flange
        // widens until the area is carried.
        let (w, t) = root_cap_dimensions(0.05, 6.0, 0.5);
        assert!((t - 0.1).abs() < 1e-12);
        assert!((w - 0.5).abs() < 1e-12 && (w * t - 0.05).abs() < 1e-12);
        // Past the half-chord bound the area cannot be carried: reported short.
        let (w, t) = root_cap_dimensions(0.5, 6.0, 0.5);
        assert!((w - 3.0).abs() < 1e-12 && w * t < 0.5);
    }

    #[test]
    fn linspace_pins_both_endpoints_and_spaces_evenly() {
        let v = linspace(0.0, 1.0, 5);
        assert_eq!(v, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(2.0, 3.0, 1), vec![2.0]);
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn gradient_unit_is_the_constant_spacing_for_a_uniform_grid() {
        // NumPy's gradient at unit spacing on a uniform ramp is the step at
        // every station, endpoints included.
        let g = gradient_unit(&[0.0, 2.0, 4.0, 6.0]);
        assert_eq!(g, vec![2.0, 2.0, 2.0, 2.0]);
        // Fewer than two points has no derivative to take.
        assert_eq!(gradient_unit(&[5.0]), vec![0.0]);
    }

    #[test]
    fn trapezoid_integrates_a_line_to_its_exact_area() {
        // Area under y = x from 0 to 1 is 1/2, exact for the trapezoidal rule
        // on a straight line at any sampling.
        let x = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let y = x.clone();
        assert!((trapezoid(&y, &x) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn automatic_rib_count_uses_ceiling_panels_and_includes_both_end_ribs() {
        assert_eq!(rib_count_from_max_spacing(5.0, 2.0), 4);
        assert_eq!(rib_count_from_max_spacing(6.0, 2.0), 4);
    }

    #[test]
    fn cap_taper_is_flat_inboard_and_reaches_the_tip_fraction_at_the_tip() {
        let taper = cap_taper(&[0.0, 0.5, 1.0], 0.5, 0.3);
        assert_eq!(taper[0], 1.0);
        assert_eq!(taper[1], 1.0);
        assert!((taper[2] - 0.3).abs() < 1e-12);
    }

    #[test]
    fn installed_rib_spacing_uses_the_selected_count_not_the_allowable_limit() {
        let mut sizing = WingboxSizing {
            y_stations: vec![0.0, 35.875],
            num_ribs: 25,
            rib_spacing_m: 0.96738451494986,
            ..test_sizing()
        };
        assert!((sizing.installed_rib_spacing_m() - 35.875 / 24.0).abs() < 1e-12);
        assert_ne!(sizing.installed_rib_spacing_m(), sizing.rib_spacing_m);
        assert!(!sizing.rib_spacing_pass());
        sizing.num_ribs = 39;
        assert!(sizing.rib_spacing_pass());
    }

    #[test]
    fn installed_rib_spacing_uses_the_station_span_when_the_grid_has_a_datum_offset() {
        let sizing = WingboxSizing {
            y_stations: vec![4.0, 14.0],
            num_ribs: 6,
            rib_spacing_m: 2.0,
            ..test_sizing()
        };
        assert!((sizing.installed_rib_spacing_m() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn negative_or_non_finite_strength_margins_do_not_pass() {
        let mut sizing = test_sizing();
        sizing.spars = vec![SparSizing {
            chord_fraction: 0.25,
            h: vec![1.0],
            w_cap: vec![1.0],
            t_cap: vec![1.0],
            a_cap: vec![1.0],
            t_web: 0.1,
            frac_moment: vec![1.0],
            margin_of_safety: vec![-0.1],
        }];
        assert!(!sizing.strength_margins_pass());
        assert_eq!(sizing.minimum_margin_of_safety(), -0.1);
        sizing.spars[0].margin_of_safety[0] = f64::NAN;
        assert!(!sizing.strength_margins_pass());
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn controlling_margin_locates_the_smallest_finite_margin_across_spars_and_stations() {
        let mut sizing = test_sizing();
        sizing.spars = vec![
            SparSizing {
                chord_fraction: 0.25,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![0.5, 1.0],
            },
            SparSizing {
                chord_fraction: 0.75,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![-4.2e-7, 2.0],
            },
        ];
        let controlling = sizing.controlling_margin().expect("spar stations present");
        assert_eq!(controlling.margin, -4.2e-7);
        assert_eq!(controlling.spar_index, 1);
        assert_eq!(controlling.station_index, 0);
        assert!((controlling.chord_fraction - 0.75).abs() < 1e-12);
        assert_eq!(controlling.y_m, 0.0);
        assert_eq!(controlling.eta, 0.0);
        // The raw value survives at full precision -- this is exactly what a
        // `{:.6}`-rounded display collapses to the ambiguous "-0.000000".
        assert_ne!(controlling.margin, 0.0);
        assert_eq!(sizing.minimum_margin_of_safety(), controlling.margin);
    }

    #[test]
    fn controlling_margin_prefers_a_nan_station_over_any_finite_margin() {
        let mut sizing = test_sizing();
        sizing.spars = vec![
            SparSizing {
                chord_fraction: 0.25,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![-1.0, f64::NAN],
            },
            SparSizing {
                chord_fraction: 0.75,
                h: vec![1.0],
                w_cap: vec![1.0],
                t_cap: vec![1.0],
                a_cap: vec![1.0],
                t_web: 0.1,
                frac_moment: vec![1.0],
                margin_of_safety: vec![-5.0],
            },
        ];
        let controlling = sizing.controlling_margin().expect("spar stations present");
        assert!(controlling.margin.is_nan());
        assert_eq!(controlling.spar_index, 0);
        assert_eq!(controlling.station_index, 1);
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn controlling_margin_is_none_when_there_are_no_spar_stations() {
        let sizing = test_sizing();
        assert!(sizing.spars.is_empty());
        assert!(sizing.controlling_margin().is_none());
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn composite_declaration_structure_and_uncalibrated_uncertainty_contract() {
        let decl = CompositeProxyDeclaration {
            source: "Open source gap...",
            applicability: "Effective isotropic proxy...",
            relative_uncertainty: None,
        };
        assert!(decl.relative_uncertainty.is_none());
        assert!(decl.applicability.contains("Effective isotropic proxy"));
    }

    fn test_sizing() -> WingboxSizing {
        WingboxSizing {
            y_stations: vec![0.0, 35.875],
            eta_stations: vec![0.0, 1.0],
            chord: vec![1.0, 1.0],
            spar_fracs: vec![0.25, 0.75],
            spars: Vec::new(),
            t_skin: 0.01,
            num_ribs: 2,
            rib_spacing_m: 1.0,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 0.0,
                spar_webs: 0.0,
                skin: 0.0,
                ribs: 0.0,
            },
            total_mass_kg: 0.0,
            sizing_load_case: "test",
            composite_declaration: None,
        }
    }
}
