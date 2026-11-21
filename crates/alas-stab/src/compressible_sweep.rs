// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The compressible lift sweep (A1): `CL(alpha)` of the whole airplane on the
//! Goethert/Prandtl-Glauert stretched lattice at the true geometric angle.
//!
//! The report polar (`alas-pipeline`, `full_analysis::run_polar`) solves the
//! same stretched lattice through `alas_aero::analysis::AeroAnalysis`, which
//! adds the moment (`beta Cm'`), the Trefftz induced drag and the empirical
//! drag at the compressible `CL`. This module is the lift-only form of that
//! sweep, used for the lift-slope evidence (`compressible_lift_probe`) and to
//! pin the transform against the closed-form slope.
//!
//! Reference: Goethert's rule (Katz and Plotkin, *Low-Speed Aerodynamics*,
//! sect. 12.4, not in the local library; the transform is checked here
//! against the incompressible limit and against DATCOM) and USAF DATCOM
//! 4.1.3.2, Method 1 (Lowry and Polhamus, NACA TR 3911), for the swept-wing
//! lift slope.

use alas_aero::vlm::VlmError;
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

use crate::neutral_point::{beta_for_mach, goethert_stretch};
use crate::trim::{assemble, probe};

/// `CL` of the stretched lattice at each true geometric angle of a sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct CompressibleLiftSweep {
    /// Prandtl-Glauert factor `sqrt(1 - M^2)` the lattice was stretched by.
    pub beta: f64,
    /// True geometric angles of attack, degrees.
    pub alpha_deg: Vec<f64>,
    /// Compressible lift coefficient at each angle (reference area
    /// unstretched).
    pub cl: Vec<f64>,
}

/// USAF DATCOM 4.1.3.2 Method 1 (Figure 4.1.3.2-49) subsonic lift-curve slope
/// of a straight-tapered wing, per radian:
/// `2 pi A / (2 + sqrt(4 + A^2 beta^2 (1 + tan^2(L_c/2) / beta^2)))`, with the
/// section slope `2 pi / beta` (`K = 1`) and `L_c/2` the midchord sweep. Taper
/// is not a parameter of the method (DATCOM 1978, p. 4.1.3.2-2). The method
/// summary lists Mach below 0.8 and thickness ratio up to 0.1 as its limits.
pub fn datcom_lift_slope_per_rad(aspect: f64, mach: f64, midchord_sweep_deg: f64) -> f64 {
    let beta2 = 1.0 - mach * mach;
    let tan2 = midchord_sweep_deg.to_radians().tan().powi(2);
    2.0 * std::f64::consts::PI * aspect
        / (2.0 + (4.0 + aspect * aspect * beta2 * (1.0 + tan2 / beta2)).sqrt())
}

/// The compressible-to-incompressible ratio of [`datcom_lift_slope_per_rad`]
/// at `mach`.
pub fn datcom_slope_ratio(aspect: f64, mach: f64, midchord_sweep_deg: f64) -> f64 {
    datcom_lift_slope_per_rad(aspect, mach, midchord_sweep_deg)
        / datcom_lift_slope_per_rad(aspect, 0.0, midchord_sweep_deg)
}

/// Solve the stretched lattice of `airplane` at `alphas_deg` (true geometric
/// angles) at `mach` and `altitude_m`, one assembly and factorization for the
/// whole sweep.
///
/// # Errors
///
/// See [`VlmError`].
pub fn compressible_lift_sweep(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    altitude_m: f64,
    alphas_deg: &[f64],
) -> Result<CompressibleLiftSweep, VlmError> {
    let beta = beta_for_mach(mach);
    let stretched = goethert_stretch(airplane, beta);
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let system = assemble(&stretched, analysis)?;
    let cl = alphas_deg
        .iter()
        .map(|&alpha| probe(&system, atmosphere, velocity, alpha).map(|r| r.cl_lift))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CompressibleLiftSweep {
        beta,
        alpha_deg: alphas_deg.to_vec(),
        cl,
    })
}

// A test builds its own geometry, so a failed unwrap or expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    /// A single-trapezoid wing alone of the given semi-span, root chord, tip
    /// chord and midchord sweep (the trailing-edge x follows from them).
    fn wing_alone(semi: f64, root: f64, tip: f64, half_chord_sweep_deg: f64) -> Airplane {
        let naca = |name: &str| Airfoil::from_name(name).unwrap();
        // Midchord x at the tip: semi * tan(sweep); the root midchord is root/2.
        let tip_le = semi * half_chord_sweep_deg.to_radians().tan() + 0.5 * root - 0.5 * tip;
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], root, 0.0, naca("naca2412")),
                WingXSec::new([tip_le, semi, 0.0], tip, 0.0, naca("naca2412")),
            ],
            true,
        );
        let (s_ref, b_ref, c_ref) = (
            main.reference_area(),
            main.reference_span(),
            main.mean_aerodynamic_chord(),
        );
        Airplane {
            name: "wing".to_owned(),
            xyz_ref: [0.2 * 2.0 * semi, 0.0, 0.0],
            wings: vec![main],
            fuselages: Vec::new(),
            s_ref,
            c_ref,
            b_ref,
        }
    }

    fn slope(plane: &Airplane, analysis: &AnalysisConfig, mach: f64) -> f64 {
        let sweep = compressible_lift_sweep(plane, analysis, mach, 11_000.0, &[0.0, 4.0]).unwrap();
        (sweep.cl[1] - sweep.cl[0]) / 4.0
    }

    fn fine() -> AnalysisConfig {
        let mut analysis = AnalysisConfig::default();
        analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
        analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
        analysis
    }

    /// Lattice-to-DATCOM deviation of the slope ratio `CL_a(M) / CL_a(0)` of
    /// an A 9.5, 19.6 deg midchord-sweep wing alone of the given taper.
    fn deviation(taper: f64, mach: f64) -> f64 {
        let root = 7.16 / (1.0 + taper);
        let plane = wing_alone(17.0, root, taper * root, 19.6);
        let main = &plane.wings[0];
        let (aspect, half) = (main.aspect_ratio(), main.mean_sweep_angle(0.5));
        let analysis = fine();
        let ratio = slope(&plane, &analysis, mach) / slope(&plane, &analysis, 1.0e-6);
        ratio / (datcom_slope_ratio(aspect, mach, half)) - 1.0
    }

    #[test]
    fn the_sweep_slope_ratio_follows_datcom_across_the_cruise_mach_range() {
        // DATCOM Method 1 is a closed form in A, midchord sweep and Mach only:
        // "the wing taper ratio has been eliminated as a parameter" (DATCOM
        // 1978, p. 4.1.3.2-2). The page gives no error figure for Method 1,
        // so the bound is the measured taper spread of the lattice itself:
        // at M 0.75 the deviation runs 1.5 % (taper 0.1) to 2.9 % (taper 0.6),
        // 3 % bounds the whole range. The floor at taper 0.1 is the two-section
        // test wing's spanwise mesh, not chordwise (4 to 32 panels change the
        // ratio by < 0.01 %): doubling the spanwise multiplier takes it from
        // 1.5 % to 0.45 % (`a1f_datcom_probe`); the 24-segment preset wings
        // are span-converged. Its stated Mach limit is 0.8
        // (methods summary), so M 0.82 is reported by the probe, not asserted.
        for mach in [0.5, 0.65, 0.75] {
            let d = deviation(0.28, mach);
            assert!(
                d.abs() < 0.03,
                "M {mach}: lattice vs DATCOM {:+.2} %",
                d * 100.0
            );
        }
    }

    #[test]
    fn the_deviation_from_datcom_is_the_taper_it_leaves_out() {
        // Root cause of the residual: at fixed A, sweep and Mach the lattice
        // slope ratio rises with taper while DATCOM's does not.
        let (low, mid, high) = (
            deviation(0.1, 0.75),
            deviation(0.28, 0.75),
            deviation(0.6, 0.75),
        );
        assert!(low < mid && mid < high, "{low} {mid} {high}");
        assert!(high - low > 0.01, "taper spread {:.3}", high - low);
    }

    #[test]
    fn the_incompressible_limit_is_the_unstretched_lattice() {
        let plane = wing_alone(17.0, 5.59, 1.565, 19.6);
        let analysis = fine();
        let sweep =
            compressible_lift_sweep(&plane, &analysis, 1.0e-6, 0.0, &[-2.0, 0.0, 3.0]).unwrap();
        assert!((sweep.beta - 1.0).abs() < 1e-11);
        let system = assemble(&plane, &analysis).unwrap();
        let atmosphere = Atmosphere::new(0.0);
        for (alpha, cl) in sweep.alpha_deg.iter().zip(&sweep.cl) {
            let direct = probe(
                &system,
                atmosphere,
                1.0e-6 * atmosphere.speed_of_sound(),
                *alpha,
            )
            .unwrap()
            .cl_lift;
            assert!((direct - cl).abs() < 1e-9, "{direct} vs {cl}");
        }
    }
}
