// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing spar sizing as a beam with CLT caps and a web (UAV-PLAN M4, owner
//! decision D9: beam/CLT structures, no finite elements in v1).
//!
//! Geometry: two identical caps at `z = +/- d(y)` (`d` = half the centroid
//! spacing, linear root to tip) with cap width `w(y)` and a web of depth
//! `2 d(y)` (both webs of a box are counted in the shear area).
//!
//! * Bending stiffness `EI(y) = E_x,cap * I_caps(y)`, with `E_x,cap` the
//!   membrane modulus of the symmetric cap laminate (CLT, [`super::clt`]) and
//!   `I_caps = 2 w (d^2 h + h^3/12)`.
//! * Mass per unit span `m(y) = 2 w rho_A,cap + 2 (2 d) rho_A,web`.  Both
//!   semispans are reported.
//! * Limit-load bending: the root cap strain `eps = M d / EI`; the cap resultant
//!   `N_x = sigma h` feeds the CLT ply stresses and the max-stress margin.
//! * Shear: average web shear `tau = V / (2 (2 d) t_web)`, margin on the web
//!   laminate in-plane shear strength (knocked down).
//!
//! Loads are LIMIT loads; the margin uses `SF x limit` against the knocked-down
//! strengths: `MoS = allowable / (SF sigma_limit) - 1`.
//!
//! ESTIMATES, flagged in the results: the spanwise elliptic lift (no taper load
//! shift), the uniform web, and no buckling, joint, or local crushing check.

use super::beam::{
    elliptic_lift_per_m, solve_cantilever, uniform_stations, CantileverGrid, CantileverLoad,
};
use super::clt::{abd, effective_props, plies_from_spec, ply_stresses, Ply, PlyStrength};
use super::AirframeError;
use alas_uav_db::parts::LaminateSpec;

/// Spanwise spar parameters: linear depth and cap width from root to tip.
#[derive(Debug, Clone, PartialEq)]
pub struct SparGeometry {
    /// Semispan, m.
    pub semispan_m: f64,
    /// Centroid-to-centroid cap spacing at the root, m.
    pub root_depth_m: f64,
    /// Centroid-to-centroid cap spacing at the tip, m.
    pub tip_depth_m: f64,
    /// Cap width at the root, m.
    pub root_cap_width_m: f64,
    /// Cap width at the tip, m.
    pub tip_cap_width_m: f64,
}

/// A spar design: geometry, laminates and the strengths they carry.
#[derive(Debug, Clone, PartialEq)]
pub struct SparDesign {
    /// Geometry.
    pub geometry: SparGeometry,
    /// Cap stack (bottom first), fibres along the span.
    pub cap: Vec<Ply>,
    /// Knocked-down cap strengths.
    pub cap_strength: PlyStrength,
    /// Web stack; its in-plane shear strength carries the shear check.
    pub web: Vec<Ply>,
    /// Knocked-down web strengths.
    pub web_strength: PlyStrength,
    /// Stations along the semispan (uniform).
    pub stations: usize,
    /// Ultimate safety factor on the limit loads (for example 1.5).
    pub safety_factor: f64,
}

impl SparDesign {
    /// Build from DB laminate rows: caps at 0 deg (`cap_angle`) and the web as
    /// `web_angles` plies, every strength and modulus taken from the rows.
    ///
    /// # Errors
    ///
    /// A missing row field, or an invalid safety factor.
    pub fn from_db(
        geometry: SparGeometry,
        cap_row: &LaminateSpec,
        cap_plies_deg: &[f64],
        web_row: &LaminateSpec,
        web_plies_deg: &[f64],
        safety_factor: f64,
        stations: usize,
    ) -> Result<Self, AirframeError> {
        if safety_factor.is_nan() || safety_factor < 1.0 {
            return Err(AirframeError::Invalid(
                "ultimate safety factor must be >= 1".into(),
            ));
        }
        Ok(Self {
            geometry,
            cap: plies_from_spec(cap_row, cap_plies_deg, "spar cap")?,
            cap_strength: PlyStrength::from_spec(cap_row, "spar cap")?,
            web: plies_from_spec(web_row, web_plies_deg, "spar web")?,
            web_strength: PlyStrength::from_spec(web_row, "spar web")?,
            stations,
            safety_factor,
        })
    }
}

/// Spar results.  Units in the names; limit loads unless stated.
#[derive(Debug, Clone, PartialEq)]
pub struct SparAnalysis {
    /// Limit total lift on the wing, N.
    pub limit_lift_n: f64,
    /// Root bending moment of one semispan at limit load, N m.
    pub root_moment_limit_nm: f64,
    /// Root shear of one semispan at limit load, N.
    pub root_shear_limit_n: f64,
    /// Root bending stiffness EI, N m^2.
    pub root_ei_nm2: f64,
    /// Tip deflection of one semispan at limit load, m.
    pub tip_deflection_m: f64,
    /// Tip deflection over semispan, dimensionless.
    pub tip_deflection_over_semispan: f64,
    /// Spar mass of both semispans, kg.
    pub spar_mass_kg: f64,
    /// Root cap axial stress at limit load, Pa.
    pub root_cap_stress_limit_pa: f64,
    /// Smallest ply margin of the root cap under `SF x limit`, dimensionless.
    pub bending_margin_of_safety: f64,
    /// Average web shear stress at the root at limit load, Pa.
    pub root_web_shear_limit_pa: f64,
    /// Web shear margin under `SF x limit`, dimensionless.
    pub shear_margin_of_safety: f64,
    /// Basis tag: this is an estimate with the stated simplifications.
    pub estimate: bool,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Analyse the spar under `limit_lift_n` (total lift of the wing, N).
///
/// # Errors
///
/// Invalid geometry, laminate, or grid; any failure of the beam or CLT.
pub fn analyse_spar(design: &SparDesign, limit_lift_n: f64) -> Result<SparAnalysis, AirframeError> {
    let g = &design.geometry;
    if !(g.semispan_m > 0.0 && g.root_depth_m > 0.0 && g.tip_depth_m > 0.0) {
        return Err(AirframeError::Invalid(
            "spar geometry must be positive".into(),
        ));
    }
    if !(g.root_cap_width_m > 0.0 && g.tip_cap_width_m > 0.0) {
        return Err(AirframeError::Invalid("cap widths must be positive".into()));
    }
    if !(limit_lift_n.is_finite() && limit_lift_n > 0.0) {
        return Err(AirframeError::Invalid("limit lift must be positive".into()));
    }
    let cap = abd(&design.cap)?;
    let web = abd(&design.web)?;
    let cap_eff = effective_props(&cap)?;
    let h_cap = cap.thickness_m;
    let t_web = web.thickness_m;
    let rho_cap = cap.areal_mass_kg_m2;
    let rho_web = web.areal_mass_kg_m2;

    let y = uniform_stations(g.semispan_m, design.stations)?;
    let depth = |t: f64| lerp(g.root_depth_m, g.tip_depth_m, t);
    let width = |t: f64| lerp(g.root_cap_width_m, g.tip_cap_width_m, t);
    let mut ei = Vec::with_capacity(y.len());
    let mut mass_per_m = Vec::with_capacity(y.len());
    for &yy in &y {
        let t = yy / g.semispan_m;
        let (dd, w) = (0.5 * depth(t), width(t));
        let i_caps = 2.0 * w * (dd * dd * h_cap + h_cap.powi(3) / 12.0);
        ei.push(cap_eff.ex_pa * i_caps);
        mass_per_m.push(2.0 * w * rho_cap + 2.0 * depth(t) * rho_web);
    }
    // The limit lift is the wing total; `elliptic_lift_per_m` gives the
    // half-wing distribution carrying half of it.
    let w_lift = elliptic_lift_per_m(limit_lift_n, g.semispan_m, &y)?;
    let grid = CantileverGrid {
        y_m: y.clone(),
        ei_nm2: ei.clone(),
    };
    let sol = solve_cantilever(
        &grid,
        &CantileverLoad {
            w_n_per_m: w_lift,
            tip_force_n: 0.0,
        },
    )?;

    // Root cap strain and resultant.
    let m_root = sol.root_moment_nm();
    let d_root = 0.5 * g.root_depth_m;
    let ei_root = ei[0];
    let eps_x = m_root * d_root / ei_root;
    let sigma_cap = cap_eff.ex_pa * eps_x;
    let nx = sigma_cap * h_cap;
    let (_, plies_stress) = ply_stresses(&design.cap, &cap, [nx, 0.0, 0.0])?;
    let s = &design.cap_strength;
    let sf = design.safety_factor;
    let mut bending_mos = f64::INFINITY;
    for st in &plies_stress {
        let checks = [
            (st[0], if st[0] >= 0.0 { s.xt_pa } else { s.xc_pa }),
            (st[1], if st[1] >= 0.0 { s.yt_pa } else { s.yc_pa }),
            (st[2], s.s12_pa),
        ];
        for (sigma, allow) in checks {
            if sigma.abs() > 0.0 {
                bending_mos = bending_mos.min(allow / (sf * sigma.abs()) - 1.0);
            }
        }
    }
    if !bending_mos.is_finite() {
        bending_mos = f64::INFINITY;
    }

    // Shear: average web shear at the root, both webs of depth 2 d.
    let v_root = sol.root_shear_n();
    let web_area = 2.0 * (2.0 * d_root) * t_web;
    let tau = v_root / web_area;
    let tau_allow = design.web_strength.s12_pa;
    let shear_mos = tau_allow / (sf * tau) - 1.0;

    let mass_half = {
        let mut m = 0.0;
        for i in 1..y.len() {
            m += 0.5 * (mass_per_m[i] + mass_per_m[i - 1]) * (y[i] - y[i - 1]);
        }
        m
    };
    let tip = sol.tip_deflection_m();
    Ok(SparAnalysis {
        limit_lift_n,
        root_moment_limit_nm: m_root,
        root_shear_limit_n: v_root,
        root_ei_nm2: ei_root,
        tip_deflection_m: tip,
        tip_deflection_over_semispan: tip / g.semispan_m,
        spar_mass_kg: 2.0 * mass_half,
        root_cap_stress_limit_pa: sigma_cap,
        bending_margin_of_safety: bending_mos,
        root_web_shear_limit_pa: tau,
        shear_margin_of_safety: shear_mos,
        estimate: true,
    })
}

#[cfg(test)]
// Unit tests: a failed unwrap or expect is the assertion failing on fixtures the test builds.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::needless_range_loop)]
mod tests {
    use super::*;

    fn ply(angle: f64, t: f64, rho: f64) -> Ply {
        Ply {
            angle_deg: angle,
            thickness_m: t,
            areal_mass_kg_m2: rho,
            props: super::super::clt::LaminaProps::new(181.0e9, 10.3e9, 7.17e9, 0.28).unwrap(),
        }
    }

    fn strengths() -> PlyStrength {
        PlyStrength {
            xt_pa: 1.5e9,
            xc_pa: 1.2e9,
            yt_pa: 4.0e7,
            yc_pa: 1.5e8,
            s12_pa: 6.0e7,
        }
    }

    fn uniform_design(stations: usize) -> SparDesign {
        // A uniform, unidirectional-cap spar.  Cap 4 x 0.125 mm = 0.5 mm,
        // web [+45/-45] of 2 x 0.125 mm.
        let t = 0.125e-3;
        SparDesign {
            geometry: SparGeometry {
                semispan_m: 1.5,
                root_depth_m: 0.06,
                tip_depth_m: 0.06,
                root_cap_width_m: 0.02,
                tip_cap_width_m: 0.02,
            },
            cap: vec![
                ply(0.0, t, 0.2),
                ply(0.0, t, 0.2),
                ply(0.0, t, 0.2),
                ply(0.0, t, 0.2),
            ],
            cap_strength: strengths(),
            web: vec![ply(45.0, t, 0.2), ply(-45.0, t, 0.2)],
            web_strength: strengths(),
            stations,
            safety_factor: 1.5,
        }
    }

    #[test]
    fn uniform_spar_root_moment_bending_stress_and_mass_match_closed_form() {
        let d = uniform_design(3000);
        let lift = 100.0;
        let r = analyse_spar(&d, lift).unwrap();
        // Closed form: M_root = L b / (3 pi), b = 2 s (elliptic, total lift L).
        let s = 1.5;
        let m_closed = lift * 2.0 * s / (3.0 * std::f64::consts::PI);
        assert!(((r.root_moment_limit_nm - m_closed) / m_closed).abs() < 1e-3);
        // Uniform EI: sigma = M d / I, I = 2 w (d^2 h + h^3/12).  The cap is a
        // unidirectional stack loaded in x with N_y = 0 (free transverse):
        // E_x = 1/S11 = (Q11 Q22 - Q12^2)/Q22 = E1.
        let h: f64 = 4.0 * 0.125e-3;
        let (w, dd): (f64, f64) = (0.02, 0.03);
        let i_caps = 2.0 * w * (dd * dd * h + h.powi(3) / 12.0);
        let p = super::super::clt::LaminaProps::new(181.0e9, 10.3e9, 7.17e9, 0.28).unwrap();
        let q = p.q_matrix();
        let ex_closed = (q[0][0] * q[1][1] - q[0][1] * q[0][1]) / q[1][1];
        assert!(((ex_closed - 181.0e9) / 181.0e9).abs() < 1e-9);
        assert!(((r.root_ei_nm2 / i_caps - ex_closed) / ex_closed).abs() < 1e-9);
        let sigma_closed = m_closed * dd / i_caps;
        assert!(((r.root_cap_stress_limit_pa - sigma_closed) / sigma_closed).abs() < 1e-3);
        // Mass: caps 2 w rho_A (per unit span, both caps), web 2 (2 d) rho_A,web.
        let rho_cap = 0.8;
        let rho_web = 0.4;
        let per_m = 2.0 * w * rho_cap + 2.0 * (2.0 * dd) * rho_web;
        let mass_both = 2.0 * s * per_m;
        assert!(((r.spar_mass_kg - mass_both) / mass_both).abs() < 1e-9);
        assert!(r.estimate);
    }

    #[test]
    fn margins_are_consistent_with_their_definitions() {
        let d = uniform_design(1200);
        let r = analyse_spar(&d, 40.0).unwrap();
        let sf = d.safety_factor;
        // Shear margin from its definition: tau_allow / (SF tau) - 1.
        let tau = r.root_web_shear_limit_pa;
        let expect = d.web_strength.s12_pa / (sf * tau) - 1.0;
        assert!((r.shear_margin_of_safety - expect).abs() < 1e-12);
        assert!(r.bending_margin_of_safety.is_finite());
        // Doubling the lift halves the margin headroom: MoS(2L)+1 = (MoS(L)+1)/2.
        let r2 = analyse_spar(&d, 80.0).unwrap();
        assert!(
            ((r2.bending_margin_of_safety + 1.0) - 0.5 * (r.bending_margin_of_safety + 1.0)).abs()
                < 1e-3
        );
    }

    #[test]
    fn tip_deflection_scales_with_load_and_is_positive() {
        let d = uniform_design(1500);
        let a = analyse_spar(&d, 30.0).unwrap();
        let b = analyse_spar(&d, 60.0).unwrap();
        assert!(a.tip_deflection_m > 0.0);
        assert!(((b.tip_deflection_m / a.tip_deflection_m) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn refuses_invalid_inputs() {
        let mut d = uniform_design(100);
        assert!(analyse_spar(&d, -1.0).is_err());
        d.geometry.tip_cap_width_m = 0.0;
        assert!(analyse_spar(&d, 10.0).is_err());
    }
}
