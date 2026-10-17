// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Classical laminate theory for composite spar caps and skins (UAV-PLAN M4).
//!
//! Standard CLT (Jones, *Mechanics of Composite Materials*, ch. 2-4):
//! lamina stiffness `Q` in material axes, transformed to `Qbar(theta)`, summed
//! into the extensional `A`, coupling `B` and bending `D` matrices.  Units: Pa,
//! m, N/m (`A`, resultants), N*m (`D`).  Engineering shear strain `gamma12`.
//!
//! Frame: the laminate axes `x` (spar axis or chord, per the caller) and `y`;
//! a ply angle `theta` is the rotation of the fibre (material 1) axis from `x`.
//!
//! Strength and stiffness of a ply come from the `alas-uav-db` rows
//! ([`LaminateSpec`]).  Nothing is defaulted: a missing modulus or strength is
//! an error.  The DB currently holds no laminate rows with published ply data,
//! so every number used in the tests is a labelled literature value, not a
//! database record.

use alas_uav_db::parts::LaminateSpec;

use super::AirframeError;

/// Lamina elastic constants in material axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaminaProps {
    /// Fibre-direction modulus, Pa.
    pub e1_pa: f64,
    /// Transverse modulus, Pa.
    pub e2_pa: f64,
    /// In-plane shear modulus, Pa.
    pub g12_pa: f64,
    /// Major Poisson ratio (dimensionless).
    pub nu12: f64,
}

impl LaminaProps {
    /// Validated constants.
    ///
    /// # Errors
    ///
    /// Non-positive moduli or a Poisson ratio that violates `nu12^2 < E1/E2`
    /// (positive-definite `Q`).
    pub fn new(e1_pa: f64, e2_pa: f64, g12_pa: f64, nu12: f64) -> Result<Self, AirframeError> {
        let ok = [e1_pa, e2_pa, g12_pa]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && nu12.is_finite()
            && nu12 * nu12 * e2_pa < e1_pa;
        if !ok {
            return Err(AirframeError::Invalid(format!(
                "lamina constants not admissible: E1 {e1_pa}, E2 {e2_pa}, G12 {g12_pa}, nu12 {nu12}"
            )));
        }
        Ok(Self {
            e1_pa,
            e2_pa,
            g12_pa,
            nu12,
        })
    }

    /// Lamina constants of a DB laminate row.
    ///
    /// # Errors
    ///
    /// A missing `ply_e1_pa`, `ply_e2_pa`, `ply_g12_pa` or `ply_nu12`.
    pub fn from_spec(spec: &LaminateSpec, what: &str) -> Result<Self, AirframeError> {
        let get = |v: &Option<alas_uav_db::Num>, field: &'static str| {
            v.as_ref()
                .map(|n| n.value)
                .ok_or_else(|| AirframeError::Missing {
                    what: what.to_owned(),
                    field,
                })
        };
        Self::new(
            get(&spec.ply_e1_pa, "ply_e1_pa")?,
            get(&spec.ply_e2_pa, "ply_e2_pa")?,
            get(&spec.ply_g12_pa, "ply_g12_pa")?,
            get(&spec.ply_nu12, "ply_nu12")?,
        )
    }

    /// Reduced stiffness `Q` in material axes, Pa (row-major `[[Q11,Q12,0],[Q12,Q22,0],[0,0,Q66]]`).
    #[must_use]
    pub fn q_matrix(&self) -> Mat3 {
        let nu21 = self.nu12 * self.e2_pa / self.e1_pa;
        let den = 1.0 - self.nu12 * nu21;
        let q11 = self.e1_pa / den;
        let q22 = self.e2_pa / den;
        let q12 = self.nu12 * self.e2_pa / den;
        [[q11, q12, 0.0], [q12, q22, 0.0], [0.0, 0.0, self.g12_pa]]
    }
}

/// A 3x3 matrix, row-major.
pub type Mat3 = [[f64; 3]; 3];

/// Strengths of a ply in material axes, Pa (magnitudes for compression).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlyStrength {
    /// Fibre tension.
    pub xt_pa: f64,
    /// Fibre compression.
    pub xc_pa: f64,
    /// Transverse tension.
    pub yt_pa: f64,
    /// Transverse compression.
    pub yc_pa: f64,
    /// In-plane shear.
    pub s12_pa: f64,
}

impl PlyStrength {
    /// Strengths of a DB laminate row, each knocked down by the coupon factor
    /// `knockdown` in (0, 1] (never defaulted: the row must carry them all).
    ///
    /// # Errors
    ///
    /// Any strength or the knock-down absent, or the knock-down outside (0, 1].
    pub fn from_spec(spec: &LaminateSpec, what: &str) -> Result<Self, AirframeError> {
        let get = |v: &Option<alas_uav_db::Num>, field: &'static str| {
            v.as_ref()
                .map(|n| n.value)
                .ok_or_else(|| AirframeError::Missing {
                    what: what.to_owned(),
                    field,
                })
        };
        let kd = get(&spec.knockdown, "knockdown")?;
        if !(kd > 0.0 && kd <= 1.0) {
            return Err(AirframeError::Invalid(format!(
                "{what}: knockdown {kd} outside (0, 1]"
            )));
        }
        Ok(Self {
            xt_pa: kd * get(&spec.ply_xt_pa, "ply_xt_pa")?,
            xc_pa: kd * get(&spec.ply_xc_pa, "ply_xc_pa")?,
            yt_pa: kd * get(&spec.ply_yt_pa, "ply_yt_pa")?,
            yc_pa: kd * get(&spec.ply_yc_pa, "ply_yc_pa")?,
            s12_pa: kd * get(&spec.ply_s12_pa, "ply_s12_pa")?,
        })
    }
}

/// One ply of a stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ply {
    /// Fibre angle from the laminate `x` axis, degrees.
    pub angle_deg: f64,
    /// Cured thickness, m.
    pub thickness_m: f64,
    /// Areal mass, kg/m^2.
    pub areal_mass_kg_m2: f64,
    /// Lamina stiffness.
    pub props: LaminaProps,
}

/// `A`, `B`, `D` of a stack: `N = A eps + B kappa`, `M = B eps + D kappa`.
#[derive(Debug, Clone, PartialEq)]
pub struct Abd {
    /// Extensional stiffness, N/m.
    pub a: Mat3,
    /// Coupling stiffness, N.
    pub b: Mat3,
    /// Bending stiffness, N*m.
    pub d: Mat3,
    /// Total thickness, m.
    pub thickness_m: f64,
    /// Total areal mass, kg/m^2.
    pub areal_mass_kg_m2: f64,
}

/// Transformed stiffness `Qbar(theta)` for a fibre angle `theta` (radians).
#[must_use]
pub fn q_bar(q: &Mat3, theta_rad: f64) -> Mat3 {
    let (s, c) = theta_rad.sin_cos();
    let (m, n) = (c, s);
    let (q11, q12, q22, q66) = (q[0][0], q[0][1], q[1][1], q[2][2]);
    let m2n2 = m * m * n * n;
    let qb11 = q11 * m.powi(4) + 2.0 * (q12 + 2.0 * q66) * m2n2 + q22 * n.powi(4);
    let qb22 = q11 * n.powi(4) + 2.0 * (q12 + 2.0 * q66) * m2n2 + q22 * m.powi(4);
    let qb12 = (q11 + q22 - 4.0 * q66) * m2n2 + q12 * (m.powi(4) + n.powi(4));
    let qb66 = (q11 + q22 - 2.0 * q12 - 2.0 * q66) * m2n2 + q66 * (m.powi(4) + n.powi(4));
    let qb16 = (q11 - q12 - 2.0 * q66) * m.powi(3) * n + (q12 - q22 + 2.0 * q66) * m * n.powi(3);
    let qb26 = (q11 - q12 - 2.0 * q66) * m * n.powi(3) + (q12 - q22 + 2.0 * q66) * m.powi(3) * n;
    [[qb11, qb12, qb16], [qb12, qb22, qb26], [qb16, qb26, qb66]]
}

/// Stack `plies` (bottom first, the first ply at `z = -h/2`) into `A`, `B`, `D`.
///
/// # Errors
///
/// An empty stack or a non-positive ply thickness.
pub fn abd(plies: &[Ply]) -> Result<Abd, AirframeError> {
    if plies.is_empty() {
        return Err(AirframeError::Invalid("laminate has no plies".into()));
    }
    if plies
        .iter()
        .any(|p| p.thickness_m.is_nan() || p.thickness_m <= 0.0)
    {
        return Err(AirframeError::Invalid(
            "every ply needs a positive thickness".into(),
        ));
    }
    let h: f64 = plies.iter().map(|p| p.thickness_m).sum();
    let mut a = [[0.0; 3]; 3];
    let mut b = [[0.0; 3]; 3];
    let mut d = [[0.0; 3]; 3];
    let mut z0 = -0.5 * h;
    let mut mass = 0.0;
    for p in plies {
        let z1 = z0 + p.thickness_m;
        let qb = q_bar(&p.props.q_matrix(), p.angle_deg.to_radians());
        accumulate(&mut a, &qb, z1 - z0);
        accumulate(&mut b, &qb, 0.5 * (z1 * z1 - z0 * z0));
        accumulate(&mut d, &qb, (z1.powi(3) - z0.powi(3)) / 3.0);
        mass += p.areal_mass_kg_m2;
        z0 = z1;
    }
    Ok(Abd {
        a,
        b,
        d,
        thickness_m: h,
        areal_mass_kg_m2: mass,
    })
}

/// `acc += weight * m`, element-wise.
fn accumulate(acc: &mut Mat3, m: &Mat3, weight: f64) {
    for (row, mrow) in acc.iter_mut().zip(m) {
        for (x, v) in row.iter_mut().zip(mrow) {
            *x += weight * v;
        }
    }
}

/// Inverse of a 3x3 matrix.
///
/// # Errors
///
/// Singular matrix.
pub fn inverse3(m: &Mat3) -> Result<Mat3, AirframeError> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if !(det.is_finite() && det.abs() > 0.0) {
        return Err(AirframeError::Invalid("singular laminate matrix".into()));
    }
    let inv_det = 1.0 / det;
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            let (r0, r1) = ((j + 1) % 3, (j + 2) % 3);
            let (c0, c1) = ((i + 1) % 3, (i + 2) % 3);
            *v = (m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]) * inv_det;
        }
    }
    Ok(out)
}

/// Effective engineering constants of a symmetric laminate (`B = 0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveProps {
    /// Membrane modulus along `x`, Pa.
    pub ex_pa: f64,
    /// Membrane modulus along `y`, Pa.
    pub ey_pa: f64,
    /// Membrane shear modulus, Pa.
    pub gxy_pa: f64,
    /// Membrane Poisson ratio `nu_xy`.
    pub nu_xy: f64,
    /// Flexural modulus along `x` from `D^-1`, Pa.
    pub e_flex_x_pa: f64,
}

/// Effective membrane and flexural constants of a symmetric laminate.
///
/// # Errors
///
/// A coupled laminate (`B` not zero), or a singular `A` or `D`.
pub fn effective_props(abd: &Abd) -> Result<EffectiveProps, AirframeError> {
    let coupled = abd
        .b
        .iter()
        .flatten()
        .any(|v| v.abs() > 1e-9 * abd.a[0][0].abs());
    if coupled {
        return Err(AirframeError::Invalid(
            "effective constants need a symmetric laminate (B = 0)".into(),
        ));
    }
    let h = abd.thickness_m;
    let a = &abd.a;
    let ainv = inverse3(a)?;
    let dinv = inverse3(&abd.d)?;
    Ok(EffectiveProps {
        ex_pa: 1.0 / (h * ainv[0][0]),
        ey_pa: 1.0 / (h * ainv[1][1]),
        gxy_pa: 1.0 / (h * ainv[2][2]),
        nu_xy: -ainv[0][1] / ainv[0][0],
        e_flex_x_pa: 12.0 / (h.powi(3) * dinv[0][0]),
    })
}

/// Ply-level strain state of a symmetric laminate under in-plane resultants.
///
/// Returns `eps = A^-1 N` (mid-plane strains, `gamma` engineering shear) and,
/// per ply, the material-axis stresses `sigma_1, sigma_2, tau_12` (Pa).
///
/// # Errors
///
/// A coupled laminate or a singular `A`.
pub fn ply_stresses(
    plies: &[Ply],
    abd: &Abd,
    resultants_n_m: [f64; 3],
) -> Result<(EpsilonVec, Vec<[f64; 3]>), AirframeError> {
    let ainv = inverse3(&abd.a)?;
    let eps = mat_vec(&ainv, resultants_n_m);
    let stresses = plies
        .iter()
        .map(|p| {
            let qb = q_bar(&p.props.q_matrix(), p.angle_deg.to_radians());
            let sx = mat_vec(&qb, eps);
            // Rotate the global stress into material axes (theta to 1-2).
            let (s, c) = p.angle_deg.to_radians().sin_cos();
            let s1 = c * c * sx[0] + s * s * sx[1] + 2.0 * c * s * sx[2];
            let s2 = s * s * sx[0] + c * c * sx[1] - 2.0 * c * s * sx[2];
            let t12 = -c * s * sx[0] + c * s * sx[1] + (c * c - s * s) * sx[2];
            [s1, s2, t12]
        })
        .collect();
    Ok((eps, stresses))
}

/// Mid-plane strains `[eps_x, eps_y, gamma_xy]`.
pub type EpsilonVec = [f64; 3];

fn mat_vec(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (o, row) in out.iter_mut().zip(m) {
        *o = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
    }
    out
}

/// Plies of a DB laminate row, stacked as one orientation (the caller gives
/// the angle and count).  The thickness and areal mass come from the row.
///
/// # Errors
///
/// A missing thickness, areal mass or lamina constant.
pub fn plies_from_spec(
    spec: &LaminateSpec,
    angles_deg: &[f64],
    what: &str,
) -> Result<Vec<Ply>, AirframeError> {
    let t = spec
        .ply_thickness_m
        .as_ref()
        .map(|n| n.value)
        .ok_or_else(|| AirframeError::Missing {
            what: what.to_owned(),
            field: "ply_thickness_m",
        })?;
    let rho_a = spec
        .areal_mass_kg_m2
        .as_ref()
        .map(|n| n.value)
        .ok_or_else(|| AirframeError::Missing {
            what: what.to_owned(),
            field: "areal_mass_kg_m2",
        })?;
    let props = LaminaProps::from_spec(spec, what)?;
    Ok(angles_deg
        .iter()
        .map(|&angle_deg| Ply {
            angle_deg,
            thickness_m: t,
            areal_mass_kg_m2: rho_a,
            props,
        })
        .collect())
}

#[cfg(test)]
// Unit tests: a failed unwrap or expect is the assertion failing on fixtures the test builds.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::needless_range_loop)]
mod tests {
    use super::*;

    /// Literature-typical unidirectional carbon/epoxy (T300/5208-class, Jones
    /// 1999 Table 2.2 order of magnitude).  Test fixture, NOT a DB record.
    fn cfrp() -> LaminaProps {
        LaminaProps::new(181.0e9, 10.3e9, 7.17e9, 0.28).unwrap()
    }

    fn ply(angle: f64, t: f64) -> Ply {
        Ply {
            angle_deg: angle,
            thickness_m: t,
            areal_mass_kg_m2: 0.2,
            props: cfrp(),
        }
    }

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-30)
    }

    #[test]
    fn q_matrix_matches_closed_form() {
        let p = cfrp();
        let q = p.q_matrix();
        let nu21 = 0.28 * 10.3 / 181.0;
        let den = 1.0 - 0.28 * nu21;
        assert!(close(q[0][0], 181.0e9 / den, 1e-12));
        assert!(close(q[1][1], 10.3e9 / den, 1e-12));
        assert!(close(q[0][1], 0.28 * 10.3e9 / den, 1e-12));
        assert!(close(q[2][2], 7.17e9, 1e-12));
    }

    #[test]
    fn inverse3_round_trips_a_general_matrix() {
        let m = [[4.0, 1.0, 0.5], [2.0, 3.0, 0.0], [0.3, 1.0, 2.0]];
        let inv = inverse3(&m).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let s: f64 = (0..3).map(|k| m[i][k] * inv[k][j]).sum();
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((s - want).abs() < 1e-12, "({i},{j}) = {s}");
            }
        }
        assert!(inverse3(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]]).is_err());
    }

    #[test]
    fn transform_is_identity_at_zero_and_swaps_axes_at_ninety() {
        let q = cfrp().q_matrix();
        let q0 = q_bar(&q, 0.0);
        let q90 = q_bar(&q, std::f64::consts::FRAC_PI_2);
        for i in 0..3 {
            for j in 0..3 {
                assert!(close(q0[i][j] + 1.0, q[i][j] + 1.0, 1e-12));
            }
        }
        assert!(close(q90[0][0], q[1][1], 1e-12));
        assert!(close(q90[1][1], q[0][0], 1e-12));
        assert!(close(q90[2][2], q[2][2], 1e-12));
    }

    #[test]
    fn unidirectional_stack_matches_h_q11_and_h3_q11_over_12() {
        let h_ply = 0.125e-3;
        let plies: Vec<Ply> = (0..4).map(|_| ply(0.0, h_ply)).collect();
        let h = 4.0 * h_ply;
        let s = abd(&plies).unwrap();
        let q = cfrp().q_matrix();
        assert!(close(s.a[0][0], h * q[0][0], 1e-12));
        assert!(close(s.d[0][0], h.powi(3) * q[0][0] / 12.0, 1e-12));
        assert!(s.b.iter().flatten().all(|v| v.abs() < 1e-6));
        assert!(close(s.areal_mass_kg_m2, 0.8, 1e-12));
    }

    #[test]
    fn cross_ply_zero_ninety_ninety_zero_matches_hand_closed_form() {
        // [0/90/90/0]: four equal plies, h = 4 t.  Hand results:
        // A11 = h (Q11+Q22)/2, A12 = h Q12, A66 = h Q66,
        // D11 = h^3 (7 Q11 + Q22) / 96, D12 = h^3 Q12 / 12, B = 0.
        let t = 0.1e-3;
        let plies = [ply(0.0, t), ply(90.0, t), ply(90.0, t), ply(0.0, t)];
        let s = abd(&plies).unwrap();
        let h = 4.0 * t;
        let q = cfrp().q_matrix();
        assert!(close(s.a[0][0], h * (q[0][0] + q[1][1]) / 2.0, 1e-10));
        assert!(close(s.a[0][1], h * q[0][1], 1e-10));
        assert!(close(s.a[2][2], h * q[2][2], 1e-10));
        assert!(close(
            s.d[0][0],
            h.powi(3) * (7.0 * q[0][0] + q[1][1]) / 96.0,
            1e-10
        ));
        assert!(close(s.d[0][1], h.powi(3) * q[0][1] / 12.0, 1e-10));
        assert!(s.b.iter().flatten().all(|v| v.abs() < 1e-6 * s.a[0][0]));
        // Effective membrane modulus of the cross-ply laminate, closed form
        // E_x = (A11^2 - A12^2) / (A11 h).
        let eff = effective_props(&s).unwrap();
        let a11 = s.a[0][0];
        let a12 = s.a[0][1];
        assert!(close(eff.ex_pa, (a11 * a11 - a12 * a12) / (a11 * h), 1e-9));
    }

    #[test]
    fn angle_ply_is_balanced_and_shear_coupling_free() {
        let t = 0.1e-3;
        let plies = [ply(45.0, t), ply(-45.0, t), ply(-45.0, t), ply(45.0, t)];
        let s = abd(&plies).unwrap();
        // Balanced symmetric: A16 = A26 = 0.
        assert!(s.a[0][2].abs() < 1e-6 * s.a[0][0]);
        assert!(s.a[1][2].abs() < 1e-6 * s.a[0][0]);
        assert!(s.b.iter().flatten().all(|v| v.abs() < 1e-6 * s.a[0][0]));
    }

    #[test]
    fn asymmetric_stack_is_coupled_and_effective_props_refuse_it() {
        let t = 0.1e-3;
        let s = abd(&[ply(0.0, t), ply(90.0, t)]).unwrap();
        assert!(s.b.iter().flatten().any(|v| v.abs() > 1.0));
        assert!(effective_props(&s).is_err());
    }

    #[test]
    fn uniaxial_stress_splits_into_ply_axes_with_poisson_restraint() {
        // A single 0 deg ply under a pure fibre force N_x: eps_x = Nx/(h Q11-
        // reduced), sigma_1 = Nx/h, sigma_2 = 0 (no transverse restraint in a
        // free strip).  With A^-1 the transverse strain is -nu12 eps_x in the
        // unidirectional case only when the strip is restrained, so check the
        // fibre stress equals Nx / h for the exact A^-1 solution.
        let t = 0.2e-3;
        let plies = [ply(0.0, t)];
        let s = abd(&plies).unwrap();
        let n = 1.0e5;
        let (eps, st) = ply_stresses(&plies, &s, [n, 0.0, 0.0]).unwrap();
        assert!(close(st[0][0], n / t, 1e-9));
        assert!(st[0][1].abs() < 1e-3 * n / t);
        assert!(eps[0] > 0.0);
    }

    #[test]
    fn ply_strength_requires_every_value_and_applies_knockdown() {
        let mut spec = LaminateSpec::default();
        assert!(PlyStrength::from_spec(&spec, "cap").is_err());
        let f = |v: f64| {
            Some(alas_uav_db::Num::new(
                v,
                "Pa",
                alas_uav_db::SourceId::new("test-src").unwrap(),
                None,
                alas_uav_db::Method::User,
            ))
        };
        spec.ply_xt_pa = f(2.0e9);
        spec.ply_xc_pa = f(1.5e9);
        spec.ply_yt_pa = f(5.0e7);
        spec.ply_yc_pa = f(2.0e8);
        spec.ply_s12_pa = f(9.0e7);
        spec.knockdown = f(0.8);
        let st = PlyStrength::from_spec(&spec, "cap").unwrap();
        assert!(close(st.xt_pa, 1.6e9, 1e-12));
        assert!(close(st.s12_pa, 7.2e7, 1e-12));
        spec.knockdown = f(1.2);
        assert!(PlyStrength::from_spec(&spec, "cap").is_err());
    }
}
