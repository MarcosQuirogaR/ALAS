// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The compressible cruise trim (AERL-02): the trim attitude, the lift and
//! moment slopes, the stabilizer Jacobian and the neutral point, all taken on
//! the Goethert/Prandtl-Glauert stretched lattice at the cruise Mach.
//!
//! # Transform
//!
//! With `beta = sqrt(1 - M^2)` the wing and tail are stretched in x by
//! `1 / beta` ([`goethert_stretch`], the transform the stated-Mach neutral
//! point already uses) and solved on the incompressible lattice at the same
//! angle of attack and the same stabilizer twist. For a linear subsonic flow
//!
//! - the lift of the stretched lattice equals the compressible lift at the
//!   same freestream dynamic pressure, so `CL`, `CL_alpha` and `CL_ih` are read
//!   unchanged, the reference area being left unstretched;
//! - the pitching moment about the stretched reference point `x_ref / beta`
//!   is `1 / beta` times the compressible moment (centre of pressure
//!   `x = beta x'`), so `Cm = beta Cm'`, and the same factor applies to
//!   `Cm_alpha` and `Cm_ih`. `Cm = 0` is therefore the same condition on both
//!   lattices and the Newton refinement runs on the stretched one directly.
//!
//! Prandtl-Glauert is not valid near `M = 1` (beta floor 0.05 inside
//! [`goethert_stretch`], Mach cap 0.99 inside `beta_for_mach`). Drag is never
//! read from this solve: the trimmed incompressible lattice stays the source
//! of the Trefftz induced drag, as before.
//!
//! Reference: USAF DATCOM 4.1.3.2 for the swept-wing lift slope this
//! reproduces within the linear theory (+31 % at M 0.78, A 9.5, quarter-chord
//! sweep 25 deg), and Katz and Plotkin, *Low-Speed Aerodynamics*, for the
//! Goethert rule.

use std::sync::{Arc, Mutex};

use alas_aero::vlm::VlmGeometryCache;

use super::*;
use crate::neutral_point::{
    beta_for_mach, compressible_np_from_stretched_probes, goethert_stretch,
};

/// The trim attitude and slopes of the compressible lattice at the cruise
/// Mach: the compressible counterpart of the attitude fields of
/// [`StabilityTrimResult`] (whose own fields stay the incompressible lattice
/// state that the Trefftz-plane drag solves are made at).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressibleTrim {
    /// Prandtl-Glauert factor `sqrt(1 - M^2)` the lattice was stretched by.
    pub beta: f64,
    /// Whether the compressible trim solved with a nonsingular Jacobian and
    /// reached the residual tolerance of the Newton refinement.
    pub converged: bool,
    /// The compressible neutral point, geometry axes, m.
    pub x_np: f64,
    /// Static margin `(x_np - x_cg) / c_bar` at the real CG.
    pub static_margin: f64,
    /// `dCL/dalpha`, per degree.
    pub cl_alpha: f64,
    /// `dCm/dalpha`, per degree, physical (`beta` times the stretched value).
    pub cm_alpha: f64,
    /// The body angle of attack that trims `CL = cl_target`, degrees. This is
    /// the true geometric angle: no further compressibility relabel applies.
    pub trim_alpha_deg: f64,
    /// The stabilizer incidence that trims `Cm = 0`, degrees.
    pub trim_ih_deg: f64,
    /// `dCL/di_h`, per degree.
    pub cl_ih: f64,
    /// `dCm/di_h`, per degree, physical.
    pub cm_ih: f64,
}

/// Solve the compressible trim of `airplane`. `None` when the airplane has no
/// horizontal stabilizer (a pure-alpha trim has no incidence to report; the
/// incompressible result already marks that case unconverged).
///
/// # Errors
///
/// See [`VlmError`].
pub(super) fn compressible_trim(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<Option<CompressibleTrim>, VlmError> {
    let Some(i_h0) = hstab(airplane)
        .and_then(|h| h.xsecs.first())
        .map(|root| root.twist)
    else {
        return Ok(None);
    };
    let beta = beta_for_mach(mach);
    let stretched = goethert_stretch(airplane, beta);
    // The stretched family has its own geometry memo: sharing the physical
    // lattice's cache would rebuild it on every alternation.
    let cache = Arc::new(Mutex::new(VlmGeometryCache::default()));
    let assembly = TrimAssembly::new(analysis, Some(&cache));
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let (a_lo, a_hi) = (analysis.probe_alpha_low_deg, analysis.probe_alpha_high_deg);
    let delta_ih = analysis.trim_incidence_probe_delta_deg;

    let system = assembly.assemble(&stretched)?;
    let r1 = probe(&system, atmosphere, velocity, a_lo)?;
    let r2 = probe(&system, atmosphere, velocity, a_hi)?;
    let d_alpha = a_hi - a_lo;
    if d_alpha.abs() < DEGENERACY_FLOOR {
        return Ok(None);
    }
    let cl_alpha = (r2.cl_lift - r1.cl_lift) / d_alpha;
    let x_cg = airplane.xyz_ref[0];
    let c_ref = airplane.c_ref.max(0.1);
    let (x_np, airframe_shift_m) = compressible_np_from_stretched_probes(
        airplane, &stretched, analysis, atmosphere, velocity, beta, a_lo, a_hi, &r1, &r2,
    )?;
    // STAB-02: the airframe the lattice does not carry (fuselage, nacelles)
    // moved the neutral point by `airframe_shift_m` (m, aft positive), so the
    // pitching-moment slope it adds, per degree and physical, is
    // `d Cm_alpha = -CL_alpha * shift / c_ref` (from `Cm_alpha = -CL_alpha
    // (x_np - x_cg) / c_ref`; a forward shift is destabilising, positive).
    // Its moment at alpha = 0 is taken as zero (Munk body moment, linear in
    // the body angle; no Cm0 term is available, documented limitation). Trim
    // `Cm = 0` and the neutral point therefore describe the same airframe.
    let cm_body = -cl_alpha * airframe_shift_m / c_ref;
    let cm_alpha = beta * (r2.cm_pitch - r1.cm_pitch) / d_alpha + cm_body;
    let static_margin = (x_np - x_cg) / c_ref;

    let perturbed = with_hstab_twist(&stretched, i_h0 + delta_ih);
    let r3 = probe(&assembly.assemble(&perturbed)?, atmosphere, velocity, a_lo)?;
    let cl_ih = (r3.cl_lift - r1.cl_lift) / delta_ih;
    let cm_ih = beta * (r3.cm_pitch - r1.cm_pitch) / delta_ih;

    // The same closed-form 2x2 as the incompressible solve, on the physical
    // moment `beta Cm'`; the Newton refinement below removes its linearization
    // error against the stretched lattice's own residuals.
    let jacobian = vec![vec![cl_alpha, cl_ih], vec![cm_alpha, cm_ih]];
    let rhs = vec![
        vec![cl_target - r1.cl_lift],
        vec![-beta * r1.cm_pitch - cm_body * a_lo],
    ];
    let step = linalg::solve(&jacobian, &rhs).ok().and_then(|solution| {
        let d_a = solution.first()?.first().copied()?;
        let d_ih = solution.get(1)?.first().copied()?;
        let det = cl_alpha * cm_ih - cl_ih * cm_alpha;
        (d_a.is_finite() && d_ih.is_finite() && det.abs() > DEGENERACY_FLOOR).then_some((d_a, d_ih))
    });
    let (trim_alpha_deg, trim_ih_deg, converged) = match step {
        Some((d_a, d_ih)) => refine_trim(
            &stretched,
            &assembly,
            cl_target,
            atmosphere,
            velocity,
            a_lo + d_a,
            i_h0 + d_ih,
            // The lattice moment is `Cm' = Cm_physical / beta`.
            cm_body / beta,
        )?,
        None => (a_lo, i_h0, false),
    };
    Ok(Some(CompressibleTrim {
        beta,
        converged,
        x_np,
        static_margin,
        cl_alpha,
        cm_alpha,
        trim_alpha_deg,
        trim_ih_deg,
        cl_ih,
        cm_ih,
    }))
}

// A test builds its own geometry, so a failed unwrap or expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    /// USAF DATCOM 4.1.3.2 (subsonic lift-curve slope of a swept wing, per
    /// radian): `2 pi A / (2 + sqrt(4 + A^2 beta^2 (1 + tan^2 L_c/2 / beta^2)))`
    /// with the section slope taken as `2 pi` (kappa = 1).
    fn datcom_cl_alpha(aspect: f64, mach: f64, half_chord_sweep_deg: f64) -> f64 {
        let beta2 = 1.0 - mach * mach;
        let tan2 = half_chord_sweep_deg.to_radians().tan().powi(2);
        2.0 * std::f64::consts::PI * aspect
            / (2.0 + (4.0 + aspect * aspect * beta2 * (1.0 + tan2 / beta2)).sqrt())
    }

    /// A transport-like wing (A 9.5, taper 0.28, half-chord sweep 25 deg)
    /// with a conventional tail, on the body axes the trim reads.
    fn transport() -> Airplane {
        let naca = |name: &str| Airfoil::from_name(name).unwrap();
        let (span, root, tip) = (34.0_f64, 5.59_f64, 1.565_f64);
        let semi = span / 2.0;
        let tip_le = semi * 25.0_f64.to_radians().tan() + 0.5 * root - 0.5 * tip;
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], root, 2.0, naca("naca2412")),
                WingXSec::new([tip_le, semi, 0.0], tip, 0.0, naca("naca2412")),
            ],
            true,
        );
        let hstab = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([17.0, 0.0, 0.0], 3.4, -1.0, naca("naca0012")),
                WingXSec::new([19.5, 6.5, 0.0], 1.6, -1.0, naca("naca0012")),
            ],
            true,
        );
        let s_ref = main.reference_area();
        let b_ref = main.reference_span();
        let c_ref = main.mean_aerodynamic_chord();
        Airplane {
            name: "transport".to_owned(),
            xyz_ref: [0.2 * span, 0.0, 0.0],
            wings: vec![main, hstab],
            fuselages: Vec::new(),
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn datcom_closed_form_reproduces_the_audit_numbers() {
        // AERL-02: 4.711 -> 6.167 per rad (+30.9 %) at M 0.78, A 9.5, 25 deg.
        let incompressible = datcom_cl_alpha(9.5, 0.0, 25.0);
        let compressible = datcom_cl_alpha(9.5, 0.78, 25.0);
        assert!((incompressible - 4.711).abs() < 2e-3, "{incompressible}");
        assert!((compressible - 6.167).abs() < 2e-3, "{compressible}");
        assert!((compressible / incompressible - 1.309).abs() < 2e-3);
    }

    #[test]
    fn the_stretched_lattice_raises_the_lift_slope_as_datcom_predicts() {
        let plane = transport();
        let analysis = AnalysisConfig::default();
        let (mach, altitude) = (0.78, 11_000.0);
        let result = super::super::stability_and_trim(&plane, &analysis, 0.5, mach, altitude)
            .expect("trim solves");
        assert!(result.converged);
        let comp = result.compressible.expect("compressible trim is on");
        assert!(comp.converged);
        assert!((comp.beta - (1.0 - mach * mach).sqrt()).abs() < 1e-15);
        // Wing plus tail on the lattice versus the DATCOM wing-alone ratio.
        let ratio = comp.cl_alpha / result.cl_alpha;
        let datcom = datcom_cl_alpha(9.5, mach, 25.0) / datcom_cl_alpha(9.5, 0.0, 25.0);
        assert!(
            (ratio / datcom - 1.0).abs() < 0.03,
            "stretched/incompressible CL_alpha {ratio:.4} vs DATCOM {datcom:.4}"
        );
        // Trim: same lift, lower body angle (alpha - alpha_0 scales by 1/ratio).
        assert!(comp.trim_alpha_deg < result.trim_alpha_deg);
    }

    #[test]
    fn the_incompressible_limit_reproduces_the_incompressible_trim() {
        // M -> 0: beta = 1, the stretch is the identity.
        let plane = transport();
        let analysis = AnalysisConfig::default();
        let result = super::super::stability_and_trim(&plane, &analysis, 0.5, 1.0e-6, 0.0)
            .expect("trim solves");
        let comp = result.compressible.expect("compressible trim is on");
        assert!((comp.trim_alpha_deg - result.trim_alpha_deg).abs() < 1e-6);
        assert!((comp.trim_ih_deg - result.trim_ih_deg).abs() < 1e-6);
        assert!((comp.cl_alpha - result.cl_alpha).abs() < 1e-9);
        assert!((comp.cm_alpha - result.cm_alpha).abs() < 1e-9);
        assert!((comp.cl_ih - result.cl_ih).abs() < 1e-9);
        assert!((comp.cm_ih - result.cm_ih).abs() < 1e-9);
    }

    #[test]
    fn the_compressible_trim_closes_lift_and_moment_on_the_stretched_lattice() {
        let plane = transport();
        let analysis = AnalysisConfig::default();
        let (mach, altitude) = (0.78, 11_000.0);
        let result = super::super::stability_and_trim(&plane, &analysis, 0.5, mach, altitude)
            .expect("trim solves");
        let comp = result.compressible.expect("compressible trim is on");
        let stretched = goethert_stretch(&plane, comp.beta);
        let twisted = with_hstab_twist(&stretched, comp.trim_ih_deg);
        let atmosphere = Atmosphere::new(altitude);
        let velocity = mach * atmosphere.speed_of_sound();
        let system = assemble(&twisted, &analysis).unwrap();
        let solved = probe(&system, atmosphere, velocity, comp.trim_alpha_deg).unwrap();
        assert!((solved.cl_lift - 0.5).abs() < 1e-6, "{}", solved.cl_lift);
        assert!(solved.cm_pitch.abs() < 1e-6, "{}", solved.cm_pitch);
    }

    #[test]
    fn the_flag_and_the_compatibility_path_leave_the_incompressible_result_alone() {
        let plane = transport();
        let mut analysis = AnalysisConfig::default();
        let on = super::super::stability_and_trim(&plane, &analysis, 0.5, 0.78, 11_000.0).unwrap();
        analysis.compressible_trim = false;
        let off = super::super::stability_and_trim(&plane, &analysis, 0.5, 0.78, 11_000.0).unwrap();
        assert!(off.compressible.is_none());
        // Every incompressible field is bit-identical with and without it.
        assert_eq!(on.trim_alpha_deg.to_bits(), off.trim_alpha_deg.to_bits());
        assert_eq!(on.trim_ih_deg.to_bits(), off.trim_ih_deg.to_bits());
        assert_eq!(on.cl_alpha.to_bits(), off.cl_alpha.to_bits());
        assert_eq!(on.x_np.to_bits(), off.x_np.to_bits());
        let frozen = super::super::stability_and_trim_reference_compatibility(
            &plane,
            &AnalysisConfig::default(),
            0.5,
            0.78,
            11_000.0,
        )
        .unwrap();
        assert!(frozen.compressible.is_none());
        assert_eq!(off.attitude().alpha_deg, off.trim_alpha_deg);
        assert!(!off.attitude().compressible);
        assert!(on.attitude().compressible);
    }

    #[test]
    fn the_trim_carries_the_airframe_moment_the_neutral_point_carries() {
        // STAB-02: with a fuselage the neutral point moves forward; the trim
        // moment slope must carry the same airframe, so that with tail
        // efficiency 1 (the lattice has none) `-Cm_alpha / CL_alpha` equals
        // the static margin, and the destabilising body moment is balanced by
        // a larger tail download (lower stabilizer incidence).
        use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE).unwrap()
        };
        let mut plane = transport();
        plane.fuselages = vec![Fuselage::new(
            "Fuselage",
            vec![
                station(-12.0, 0.4),
                station(-9.0, 1.9),
                station(12.0, 1.9),
                station(24.0, 0.6),
            ],
        )];
        let (mach, altitude) = (0.78, 11_000.0);
        let mut analysis = AnalysisConfig {
            tail_efficiency: 1.0,
            include_fuselage_stability: true,
            ..AnalysisConfig::default()
        };
        let with = super::super::stability_and_trim(&plane, &analysis, 0.5, mach, altitude)
            .unwrap()
            .compressible
            .expect("compressible trim is on");
        analysis.include_fuselage_stability = false;
        let without = super::super::stability_and_trim(&plane, &analysis, 0.5, mach, altitude)
            .unwrap()
            .compressible
            .expect("compressible trim is on");
        assert!(with.converged && without.converged);
        assert!(
            with.static_margin < without.static_margin,
            "fuselage destabilises"
        );
        assert!(
            (-with.cm_alpha / with.cl_alpha - with.static_margin).abs() < 2.0e-3,
            "Cm_alpha/CL_alpha {} vs SM {}",
            -with.cm_alpha / with.cl_alpha,
            with.static_margin
        );
        assert!(
            (-without.cm_alpha / without.cl_alpha - without.static_margin).abs() < 2.0e-3,
            "wings-only consistency"
        );
        // The trim closes `beta Cm_lattice + dCm_alpha_body * alpha = 0`, with
        // the body slope read as the difference of the two reported slopes.
        let cm_body = with.cm_alpha - without.cm_alpha;
        assert!(cm_body > 0.0, "destabilising body slope {cm_body}");
        let stretched = goethert_stretch(&plane, with.beta);
        let twisted = with_hstab_twist(&stretched, with.trim_ih_deg);
        let atmosphere = Atmosphere::new(altitude);
        let velocity = mach * atmosphere.speed_of_sound();
        let system = assemble(&twisted, &analysis).unwrap();
        let solved = probe(&system, atmosphere, velocity, with.trim_alpha_deg).unwrap();
        assert!((solved.cl_lift - 0.5).abs() < 1e-6, "{}", solved.cl_lift);
        let cm_total = with.beta * solved.cm_pitch + cm_body * with.trim_alpha_deg;
        assert!(cm_total.abs() < 1e-6, "total moment {cm_total}");
        // A nonzero body angle makes the body moment matter: the stabilizer
        // incidence differs from the wings-only trim by the matching amount.
        assert!((with.trim_ih_deg - without.trim_ih_deg).abs() > 1e-4 * with.trim_alpha_deg.abs());
        // The body angle moves only second order (the tail load is small).
        assert!((with.trim_alpha_deg - without.trim_alpha_deg).abs() < 0.3);
    }
}
