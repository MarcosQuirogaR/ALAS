// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-surface neutral-point assembly and the Goethert/Prandtl-Glauert
//! compressibility stretch.
//!
//! The `reference_compatibility` path pivots the tail dynamic-pressure
//! efficiency `eta` about the geometric quarter-MAC point, applying it to the
//! *whole* wing+tail VLM offset instead of to the tail's own lift only. This
//! module reads each wing's own lift and pitching-moment
//! contribution straight off the two-alpha VLM probe
//! (`VlmResult::panels`/`panel_forces_geometry`, keyed by `wing_index`) and
//! forms `x_np = sum_i eta_i dL_i x_i / sum_i eta_i dL_i`, with `eta_i = 1`
//! for every surface but the horizontal tail.

use alas_aero::vlm::VlmResult;
use alas_geom::aircraft::airplane::Airplane;

/// One wing's own lift-curve slope contribution and implied aerodynamic
/// centre between two VLM probes, about `airplane.xyz_ref`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceContribution {
    /// Index into `airplane.wings`.
    pub wing_index: usize,
    /// `d(Fz)` between the two probes, geometry axes, N (not normalised by
    /// `q S`: only ratios of this value are ever used).
    pub d_lift: f64,
    /// The station this surface's own force/moment pair implies as its
    /// aerodynamic centre: `xyz_ref[0] - dM/dFz`, geometry axes, m. Falls
    /// back to `xyz_ref[0]` when `d_lift` is degenerate (no usable slope).
    pub x_ac: f64,
}

/// The `abs(d_lift) < DEGENERACY_FLOOR` guard shared with `trim`.
const DEGENERACY_FLOOR: f64 = 1e-9;

/// Per-wing lift and moment deltas between `lo` and `hi` (two VLM probes on
/// the same mesh, differing only in the operating-point angle of attack),
/// each surface's implied aerodynamic centre about `airplane.xyz_ref[0]`.
///
/// Assumes `lo` and `hi` were solved from the same [`alas_aero::vlm::VlmSystem`]
/// (so `lo.panels` and `hi.panels` are the same mesh, same order): the panel
/// arrays are read pairwise, without re-checking wing/panel identity between
/// the two.
pub fn per_surface_contributions(
    airplane: &Airplane,
    lo: &VlmResult,
    hi: &VlmResult,
) -> Vec<SurfaceContribution> {
    let n = airplane.wings.len();
    let mut d_lift = vec![0.0_f64; n];
    let mut d_moment = vec![0.0_f64; n];
    for ((panel, f_lo), f_hi) in lo
        .panels
        .iter()
        .zip(lo.panel_forces_geometry.iter())
        .zip(hi.panel_forces_geometry.iter())
    {
        let idx = panel.wing_index;
        if idx >= n {
            continue;
        }
        let dx = panel.vortex_center[0] - airplane.xyz_ref[0];
        let dz = panel.vortex_center[2] - airplane.xyz_ref[2];
        let dfx = f_hi[0] - f_lo[0];
        let dfz = f_hi[2] - f_lo[2];
        d_lift[idx] += dfz;
        // Pitch (y) component of r x dF, matching the sign convention the
        // rest of `trim` uses for `cm_pitch`.
        d_moment[idx] += dz * dfx - dx * dfz;
    }
    (0..n)
        .map(|wing_index| {
            let dl = d_lift[wing_index];
            let x_ac = if dl.abs() > DEGENERACY_FLOOR {
                airplane.xyz_ref[0] - d_moment[wing_index] / dl
            } else {
                airplane.xyz_ref[0]
            };
            SurfaceContribution {
                wing_index,
                d_lift: dl,
                x_ac,
            }
        })
        .collect()
}

/// The fractional loss of the tail's in-presence lift slope caused by a
/// downwash-gradient increment `delta` (STAB-04, flaps down).
///
/// The tail slope per unit aircraft angle of attack is proportional to
/// `1 - d_eps/d_alpha` (downwash `eps`, both nondimensional, `delta` and
/// `eps` as `d eps / d alpha`). The vortex lattice already carries the clean
/// gradient `eps_vlm` (`deps_dalpha_vlm`), so raising it to `eps_vlm + delta`
/// scales the in-presence slope by
/// `(1 - eps_vlm - delta) / (1 - eps_vlm) = 1 - delta / (1 - eps_vlm)`, not by
/// `1 - delta`. Returns `delta / (1 - eps_vlm)`, clamped to `[0, 1]` (a tail
/// cannot lose more than all of its slope); the VLM gradient is itself
/// clamped to `[0, 0.9]` upstream. At `eps_vlm = 0.35`, `delta = 0.10` the
/// factor is 0.846 instead of 0.90 (Torenbeek 1982/2013 and DATCOM 4.4
/// downwash definition; algebra, not a fitted constant).
pub(super) fn tail_slope_loss(delta: f64, eps_vlm: f64) -> f64 {
    let denominator = (1.0 - eps_vlm).max(0.1);
    (delta / denominator).clamp(0.0, 1.0)
}

/// `x_np = sum_i eta_i dL_i x_i / sum_i eta_i dL_i`: [`per_surface_contributions`]
/// combined with a per-surface tail efficiency. `eta_i = 1.0` for every wing
/// but `tail_wing_index`, which gets `eta_tail * (1.0 - tail_deps_extra)`
/// (`tail_deps_extra` is the fractional tail-slope loss of a high-lift
/// downwash increment, see [`tail_slope_loss`]; `0.0` for the clean case).
/// `None` when the
/// combined denominator is degenerate (no usable lift slope anywhere).
pub fn combine(
    contributions: &[SurfaceContribution],
    tail_wing_index: Option<usize>,
    eta_tail: f64,
    tail_deps_extra: f64,
) -> Option<f64> {
    combine_scaled(
        contributions,
        None,
        1.0,
        0.0,
        tail_wing_index,
        eta_tail,
        tail_deps_extra,
    )
}

/// [`combine`] with the lift slope of the surface `scaled_wing_index` (the
/// main wing) multiplied by `wing_slope_scale`: the high-lift condition, where
/// extending flaps raise the wing's lift slope by about the ratio of extended
/// to original wing area (Raymer, 6th ed., printed p. 413, PDF p. 447), with
/// its aerodynamic centre moved by `wing_ac_shift` (the added lift acts at the
/// quarter chord of the extended section).
pub fn combine_scaled(
    contributions: &[SurfaceContribution],
    scaled_wing_index: Option<usize>,
    wing_slope_scale: f64,
    wing_ac_shift: f64,
    tail_wing_index: Option<usize>,
    eta_tail: f64,
    tail_deps_extra: f64,
) -> Option<f64> {
    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;
    for contribution in contributions {
        let eta = if Some(contribution.wing_index) == tail_wing_index {
            eta_tail * (1.0 - tail_deps_extra)
        } else if Some(contribution.wing_index) == scaled_wing_index {
            wing_slope_scale
        } else {
            1.0
        };
        let x_ac = if Some(contribution.wing_index) == scaled_wing_index {
            contribution.x_ac + wing_ac_shift
        } else {
            contribution.x_ac
        };
        numerator += eta * contribution.d_lift * x_ac;
        denominator += eta * contribution.d_lift;
    }
    if denominator.abs() > DEGENERACY_FLOOR {
        Some(numerator / denominator)
    } else {
        None
    }
}

/// Mach ceiling of [`beta_for_mach`], so `1 - M^2` is never evaluated at
/// `M >= 1` (no transonic or supersonic model here).
const MAX_MACH_FOR_BETA: f64 = 0.99;

/// Prandtl-Glauert factor `sqrt(1 - M^2)` of the freestream Mach: the stretch
/// of [`goethert_stretch`].
pub fn beta_for_mach(mach: f64) -> f64 {
    let m = mach.abs().min(MAX_MACH_FOR_BETA);
    (1.0 - m * m).sqrt()
}

/// The Goethert/Prandtl-Glauert subsonic compressibility transform of a
/// linear vortex-lattice geometry: stretch every wing's X station and chord
/// by `1 / beta` (`beta = sqrt(1 - M^2)`), solve the incompressible VLM on
/// the stretched geometry, then scale the resulting neutral-point station
/// (and any other X-only station read off it) back by `beta`. Exact within
/// linear subsonic (Prandtl-Glauert) theory for the wing/tail lifting
/// surfaces; fuselage and nacelle stations are left unstretched (this
/// transform is applied to the lifting-surface mesh only).
///
/// `beta` is clamped to a `0.05` floor: this function does not model
/// transonic or supersonic flow, and a `beta` near zero would blow up the
/// stretch.
pub fn goethert_stretch(airplane: &Airplane, beta: f64) -> Airplane {
    let beta = beta.max(0.05);
    let mut stretched = airplane.clone();
    for wing in &mut stretched.wings {
        for xsec in &mut wing.xsecs {
            xsec.xyz_le[0] /= beta;
            xsec.chord /= beta;
        }
        wing.planform_datum = wing.planform_datum.map(|datum| datum.stretch_x(1.0 / beta));
        wing.tip_te_flare_m /= beta;
        for xsec in wing.reference_xsecs.iter_mut().flatten() {
            xsec.xyz_le[0] /= beta;
            xsec.chord /= beta;
        }
    }
    stretched.xyz_ref[0] /= beta;
    stretched
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn probe_airplane() -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        let s_ref = main.reference_area();
        let b_ref = main.reference_span();
        let c_ref = main.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [1.0, 0.0, 0.0],
            wings: vec![main],
            fuselages: vec![],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn goethert_stretch_divides_x_stations_and_chords_by_beta() {
        let plane = probe_airplane();
        let stretched = goethert_stretch(&plane, 0.5);
        assert!((stretched.wings[0].xsecs[0].chord - 6.0).abs() < 1e-12);
        assert!((stretched.xyz_ref[0] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn beta_for_mach_is_the_prandtl_glauert_factor_capped_below_mach_one() {
        assert_eq!(beta_for_mach(0.0), 1.0);
        assert_eq!(beta_for_mach(-0.6), beta_for_mach(0.6));
        assert!((beta_for_mach(0.6) - 0.8).abs() < 1e-15);
        let floor = (1.0_f64 - 0.99 * 0.99).sqrt();
        assert_eq!(beta_for_mach(1.0), floor);
        assert_eq!(beta_for_mach(2.5), floor);
    }

    #[test]
    fn goethert_stretch_floors_beta_away_from_zero() {
        let plane = probe_airplane();
        let stretched = goethert_stretch(&plane, 0.0);
        assert!(stretched.wings[0].xsecs[0].chord.is_finite());
        assert!(stretched.wings[0].xsecs[0].chord > 0.0);
    }

    #[test]
    fn combine_returns_none_for_a_fully_degenerate_lift_set() {
        let contributions = vec![SurfaceContribution {
            wing_index: 0,
            d_lift: 0.0,
            x_ac: 5.0,
        }];
        assert!(combine(&contributions, None, 0.9, 0.0).is_none());
    }

    #[test]
    fn combine_weights_the_tail_by_its_own_efficiency_only() {
        // Wing at x_ac=10 with dL=2, tail at x_ac=50 with dL=1, eta=0.5:
        // x_np = (2*10 + 0.5*1*50) / (2 + 0.5*1) = 45/2.5 = 18.
        let contributions = vec![
            SurfaceContribution {
                wing_index: 0,
                d_lift: 2.0,
                x_ac: 10.0,
            },
            SurfaceContribution {
                wing_index: 1,
                d_lift: 1.0,
                x_ac: 50.0,
            },
        ];
        let x_np = combine(&contributions, Some(1), 0.5, 0.0).expect("finite denominator");
        assert!((x_np - 18.0).abs() < 1e-12, "x_np={x_np}");
    }

    #[test]
    fn combine_applies_the_high_lift_downwash_increment_to_the_tail_only() {
        let contributions = vec![
            SurfaceContribution {
                wing_index: 0,
                d_lift: 2.0,
                x_ac: 10.0,
            },
            SurfaceContribution {
                wing_index: 1,
                d_lift: 1.0,
                x_ac: 50.0,
            },
        ];
        let clean = combine(&contributions, Some(1), 0.9, 0.0).expect("finite");
        let flapped = combine(&contributions, Some(1), 0.9, 0.10).expect("finite");
        // A larger downwash increment de-weights the tail's aft AC, so the
        // combined NP moves forward.
        assert!(flapped < clean, "flapped={flapped} clean={clean}");
    }

    #[test]
    fn the_flap_downwash_factor_is_one_minus_delta_over_one_minus_eps() {
        // STAB-04 closed form: eps 0.35, delta 0.10 -> 1 - 0.10/0.65 = 0.846.
        let loss = tail_slope_loss(0.10, 0.35);
        assert!((1.0 - loss - 0.846_153_846).abs() < 1e-8, "loss={loss}");
        // No increment, no loss; zero downwash recovers the old (1 - delta).
        assert_eq!(tail_slope_loss(0.0, 0.35), 0.0);
        assert!((tail_slope_loss(0.10, 0.0) - 0.10).abs() < 1e-15);
        // A gradient increment can remove at most the whole slope.
        assert_eq!(tail_slope_loss(5.0, 0.35), 1.0);
        // The NP shift of the derived example: wing AC 0.25c, tail 4.5c aft
        // with the tail carrying 0.35 of the unweighted slope; the corrected
        // factor moves the NP forward by more than the old one.
        let contributions = vec![
            SurfaceContribution {
                wing_index: 0,
                d_lift: 1.0,
                x_ac: 0.0,
            },
            SurfaceContribution {
                wing_index: 1,
                d_lift: 0.35,
                x_ac: 4.5,
            },
        ];
        let old = combine(&contributions, Some(1), 0.9, 0.10).expect("finite");
        let new =
            combine(&contributions, Some(1), 0.9, tail_slope_loss(0.10, 0.35)).expect("finite");
        assert!(new < old, "new={new} old={old}");
    }
}
