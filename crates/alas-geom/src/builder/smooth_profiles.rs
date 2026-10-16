// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! C2 interpolation of the main wing's dihedral, twist and thickness profiles
//! at the spanwise mesh nodes (`alas_config::WingProfileInterpolation`).
//!
//! The loft sections (root, side of body, kink, rake, tip, profile stations)
//! are built exactly as in the linear mode; the planform (leading-edge x,
//! chord) between them stays straight. After the spanwise mesher has put its
//! nodes between the sections by linear interpolation, this module replaces,
//! at every node, the leading-edge height and the twist by the natural cubic
//! spline through the configured stations, and rescales the thickness of the
//! node airfoil by `target(eta) / polyline(eta)`, where the target multiplier
//! is the t/c spline through `scale_i * t/c_blend(eta_i)` divided by the blend's
//! own t/c (the stations are multipliers, the drawings give t/c). Every loft section lies on
//! the polyline (the sections include all stations), so that ratio is exactly
//! the correction from the linear to the smooth thickness; at a station it is
//! one, and the station values, the ground clearances pinned at stations and
//! the reference area, which depends on the planform only, do not move.
//!
//! User-defined custom sections are data: their nodes are left untouched.
//! Frame: `eta = y / semispan`, `z` metres up, `twist` degrees (nose up).

use super::{AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::Wing;
use crate::airfoil_library::{morph_airfoil, DEFAULT_MORPH_N_POINTS};
use alas_config::{DesignVector, TransportPlanform, WingHeights, WingProfileInterpolation};

/// Scale (0 to 1) applied to both chord and thickness by an elliptical
/// closure over the last `cap_fraction` of the semispan: `sqrt(1 - u^2)` with
/// `u = (eta - (1 - cap_fraction)) / cap_fraction`, 1 inboard of the cap and 0
/// at the tip. For the surface export only (it closes the tip of the mesh);
/// the vortex lattice and the reference area never see it. `cap_fraction` is
/// clamped to `(0, 0.1]`; the design value is 0.01 to 0.02 of the semispan.
pub fn tip_cap_scale(eta: f64, cap_fraction: f64) -> f64 {
    let cap = cap_fraction.clamp(1.0e-6, 0.1);
    let u = ((eta - (1.0 - cap)) / cap).clamp(0.0, 1.0);
    (1.0 - u * u).sqrt()
}

impl AircraftBuilder {
    /// Apply the C2 profile interpolation to the meshed main-wing nodes. A
    /// no-op in [`WingProfileInterpolation::Linear`] mode.
    pub(super) fn smooth_wing_profiles(
        &self,
        dv: &DesignVector,
        planform: &TransportPlanform,
        heights: &WingHeights,
        (root_section, tip_airfoil): (&Airfoil, &Airfoil),
        wing: &mut Wing,
    ) -> Result<(), BuildError> {
        let cfg = &self.geometry.wing;
        if cfg.wing_profile_interpolation != WingProfileInterpolation::C2Spline {
            return Ok(());
        }
        let semispan = planform.tip.y_m;
        let z_profile = heights.profile_spline(planform);
        let twist_profile = cfg.twist_spline(planform, dv.tip_twist_deg);
        // t/c of the root-kink-tip blend the thickness stations multiply.
        let max_tc = |a: &Airfoil| {
            let ys = a.coordinates.iter().map(|c| c.1);
            ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min)
        };
        let (tc_root, tc_tip) = (max_tc(root_section), max_tc(tip_airfoil));
        let kink = planform.kink.span_fraction;
        let base_tc =
            |eta: f64| tc_root + (tc_tip - tc_root) * ((eta - kink) / (1.0 - kink)).clamp(0.0, 1.0);
        let thickness_profile = cfg.thickness_spline(base_tc);
        let custom_y: Vec<f64> = cfg
            .custom_sections
            .iter()
            .map(|section| section.span_fraction * semispan)
            .collect();
        for xsec in &mut wing.xsecs {
            let y_m = xsec.xyz_le[1];
            if custom_y
                .iter()
                .any(|&c| (c - y_m).abs() <= 1.0e-9 * semispan)
            {
                continue;
            }
            let eta = y_m / semispan;
            xsec.xyz_le[2] = z_profile.eval(eta);
            xsec.twist = twist_profile.eval(eta);
            // Beyond the first and last station the multiplier is held (factor 1).
            let held = cfg
                .thickness_stations
                .first()
                .is_none_or(|s| eta <= s.span_fraction)
                || cfg
                    .thickness_stations
                    .last()
                    .is_none_or(|s| eta >= s.span_fraction);
            if let Some(profile) = thickness_profile.as_ref().filter(|_| !held) {
                let factor = profile.eval(eta) / base_tc(eta) / cfg.thickness_scale_at(eta);
                if (factor - 1.0).abs() > 1.0e-12 {
                    xsec.airfoil = morph_airfoil(
                        &xsec.airfoil.coordinates,
                        factor,
                        1.0,
                        DEFAULT_MORPH_N_POINTS,
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{GeometryConfig, WingDihedralStation, WingThicknessStation};

    fn profiled(mode: WingProfileInterpolation) -> GeometryConfig {
        let mut g = GeometryConfig::default();
        g.wing.wing_profile_interpolation = mode;
        g.wing.dihedral_stations = vec![
            WingDihedralStation {
                span_fraction: 0.2,
                z_m: -1.0,
                flight_rise_m: None,
            },
            WingDihedralStation {
                span_fraction: 0.6,
                z_m: 0.3,
                flight_rise_m: None,
            },
        ];
        g.wing.thickness_stations = vec![
            WingThicknessStation {
                span_fraction: 0.0,
                thickness_scale: 1.15,
            },
            WingThicknessStation {
                span_fraction: 0.37,
                thickness_scale: 0.9,
            },
            WingThicknessStation {
                span_fraction: 1.0,
                thickness_scale: 0.85,
            },
        ];
        g
    }

    fn build(g: GeometryConfig) -> crate::aircraft::airplane::Airplane {
        AircraftBuilder::new(Some(g))
            .build(Some(&DesignVector::default()), false)
            .expect("wing builds")
    }

    fn thickness(a: &crate::aircraft::airfoil::Airfoil) -> f64 {
        let hi = a.coordinates.iter().map(|c| c.1).fold(f64::MIN, f64::max);
        let lo = a.coordinates.iter().map(|c| c.1).fold(f64::MAX, f64::min);
        hi - lo
    }

    #[test]
    fn linear_mode_is_the_independent_polyline_bit_for_bit() {
        let plane = build(profiled(WingProfileInterpolation::Linear));
        let default_mode = build(profiled(WingProfileInterpolation::default()));
        assert_eq!(
            plane.wings[0].xsecs.len(),
            default_mode.wings[0].xsecs.len()
        );
        let g = profiled(WingProfileInterpolation::Linear);
        let design = DesignVector::default();
        let planform = g.wing.transport_planform(&design).expect("planform");
        let heights = g
            .wing
            .heights(alas_config::WingShape::Flight, &planform)
            .expect("heights");
        for (a, b) in plane.wings[0]
            .xsecs
            .iter()
            .zip(&default_mode.wings[0].xsecs)
        {
            assert_eq!(a.xyz_le, b.xyz_le);
            assert_eq!(a.twist.to_bits(), b.twist.to_bits());
            // Polyline of root, stations, kink, tip, computed independently.
            let eta = a.xyz_le[1] / planform.tip.y_m;
            let mut nodes = vec![
                (0.0, heights.root_z_m),
                (planform.kink.span_fraction, heights.break_z_m),
                (1.0, heights.tip_z_m),
            ];
            nodes.extend(heights.interior.iter().copied());
            nodes.sort_by(|p, q| p.0.total_cmp(&q.0));
            let w = nodes
                .windows(2)
                .find(|w| eta <= w[1].0)
                .expect("inside the span");
            let t = (eta - w[0].0) / (w[1].0 - w[0].0);
            let z = w[0].1 + t * (w[1].1 - w[0].1);
            // Mesh nodes interpolate sections linearly, which is the same
            // polyline up to rounding (the sections lie on it).
            assert!((a.xyz_le[2] - z).abs() < 1.0e-9, "{} vs {z}", a.xyz_le[2]);
        }
    }

    #[test]
    fn c2_spline_interpolates_every_station_exactly_and_keeps_the_planform() {
        let lin = build(profiled(WingProfileInterpolation::Linear));
        let c2 = build(profiled(WingProfileInterpolation::C2Spline));
        assert_eq!(lin.wings[0].xsecs.len(), c2.wings[0].xsecs.len());
        assert_eq!(lin.s_ref.to_bits(), c2.s_ref.to_bits());
        assert_eq!(lin.b_ref.to_bits(), c2.b_ref.to_bits());
        let semispan = lin.wings[0].xsecs.last().expect("tip").xyz_le[1];
        let mut max_dz: f64 = 0.0;
        for (a, b) in lin.wings[0].xsecs.iter().zip(&c2.wings[0].xsecs) {
            // Planform (x, y, chord) identical; only z, twist and thickness move.
            assert_eq!(a.xyz_le[0].to_bits(), b.xyz_le[0].to_bits());
            assert_eq!(a.xyz_le[1].to_bits(), b.xyz_le[1].to_bits());
            assert_eq!(a.chord.to_bits(), b.chord.to_bits());
            max_dz = max_dz.max((a.xyz_le[2] - b.xyz_le[2]).abs());
            let eta = a.xyz_le[1] / semispan;
            // Stations: the section built at a station is a node and equal.
            for s in [0.0, 0.2, 0.37, 0.6, 1.0] {
                if (eta - s).abs() < 1.0e-9 {
                    assert!((a.xyz_le[2] - b.xyz_le[2]).abs() < 1.0e-9, "z at {s}");
                    if [0.0, 0.37, 1.0].contains(&s) {
                        assert!((a.twist - b.twist).abs() < 1.0e-9, "twist at {s}");
                        let ratio = thickness(&b.airfoil) / thickness(&a.airfoil);
                        assert!((ratio - 1.0).abs() < 1.0e-9, "thickness at {s}: {ratio}");
                    }
                }
            }
        }
        assert!(max_dz > 1.0e-3, "the spline must differ between stations");
    }

    #[test]
    fn c2_profile_has_continuous_slope_and_curvature_across_stations() {
        let g = profiled(WingProfileInterpolation::C2Spline);
        let design = DesignVector::default();
        let planform = g.wing.transport_planform(&design).expect("planform");
        let heights = g
            .wing
            .heights(alas_config::WingShape::Ground, &planform)
            .expect("heights");
        let tip = planform.tip.y_m;
        let f = |eta: f64| heights.at(&planform, eta * tip);
        let h = 1.0e-3;
        for s in [0.2, planform.kink.span_fraction, 0.6] {
            let slope = |e: f64| (f(e + h) - f(e - h)) / (2.0 * h);
            let curv = |e: f64| (f(e + h) - 2.0 * f(e) + f(e - h)) / (h * h);
            let (sl, sr) = (slope(s - 2.0 * h), slope(s + 2.0 * h));
            assert!(
                (sl - sr).abs() < 0.02 * sl.abs().max(1.0),
                "slope jump at {s}"
            );
            let (cl, cr) = (curv(s - 3.0 * h), curv(s + 3.0 * h));
            assert!(
                (cl - cr).abs() < 0.05 * cl.abs().max(cr.abs()).max(10.0),
                "curvature jump at {s}: {cl} vs {cr}"
            );
        }
    }

    #[test]
    fn natural_spline_overshoot_on_the_preset_wings_is_small() {
        for preset in alas_config::presets::registry() {
            let mut geometry = preset.geometry.clone();
            geometry.wing.wing_profile_interpolation = WingProfileInterpolation::C2Spline;
            let design = &preset.design_vector;
            let Ok(planform) = geometry.wing.transport_planform(design) else {
                continue;
            };
            let heights = geometry
                .wing
                .heights(alas_config::WingShape::Ground, &planform)
                .expect("heights");
            let mut nodes = vec![
                (0.0, heights.root_z_m),
                (planform.kink.span_fraction, heights.break_z_m),
                (1.0, heights.tip_z_m),
            ];
            nodes.extend(heights.interior.iter().copied());
            nodes.sort_by(|p, q| p.0.total_cmp(&q.0));
            let range = nodes.iter().map(|n| n.1).fold(f64::MIN, f64::max)
                - nodes.iter().map(|n| n.1).fold(f64::MAX, f64::min);
            let mut worst: f64 = 0.0;
            for w in nodes.windows(2) {
                let (lo, hi) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
                for i in 0..=50 {
                    let eta = w[0].0 + (w[1].0 - w[0].0) * f64::from(i) / 50.0;
                    let z = heights.at(&planform, eta * planform.tip.y_m);
                    worst = worst.max(lo - z).max(z - hi);
                }
            }
            // Overshoot beyond the neighbouring station values, as a fraction
            // of the dihedral height range and in metres.
            assert!(
                worst <= 0.35 * range.max(1.0e-9) + 1.0e-9,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn thickness_spline_is_held_outside_and_exact_at_stations() {
        let g = profiled(WingProfileInterpolation::C2Spline);
        let s = g.wing.thickness_spline(|_| 1.0).expect("stations");
        for st in &g.wing.thickness_stations {
            assert!((s.eval(st.span_fraction) - st.thickness_scale).abs() < 1.0e-14);
        }
        assert!((s.eval(-0.1) - 1.15).abs() < 1.0e-14);
        assert!((s.eval(1.2) - 0.85).abs() < 1.0e-14);
        // Two stations: the natural spline is the straight line.
        let line = alas_config::ProfileSpline::natural(&[(0.0, 1.0), (1.0, 3.0)]).expect("two");
        assert!((line.eval(0.25) - 1.5).abs() < 1.0e-14);
    }

    #[test]
    fn monotone_data_never_overshoots_and_smooth_data_stays_c2() {
        let step = [
            (0.0, 0.0),
            (1.0, 0.01),
            (2.0, 0.02),
            (3.0, 10.0),
            (4.0, 10.01),
        ];
        let p = alas_config::ProfileSpline::natural(&step).expect("nodes");
        assert!(!p.is_c2(), "a step in monotone data must trigger the guard");
        for (x, y) in step {
            assert!((p.eval(x) - y).abs() < 1.0e-12);
        }
        let mut prev = p.eval(0.0);
        for i in 1..=400 {
            let v = p.eval(f64::from(i) * 0.01);
            assert!(v >= prev - 1.0e-12, "non-monotone at {i}");
            prev = v;
        }
        let smooth: Vec<(f64, f64)> = (0..6)
            .map(|i| (f64::from(i), (0.5 * f64::from(i)).sin()))
            .collect();
        assert!(alas_config::ProfileSpline::natural(&smooth)
            .expect("nodes")
            .is_c2());
    }

    #[test]
    fn c2_thickness_follows_monotone_drawing_tc_on_the_a380_and_b747() {
        for name in ["A380-800", "B747-400"] {
            let preset = alas_config::presets::get(name).expect("preset");
            let mut geometry = preset.geometry.clone();
            geometry.wing.wing_profile_interpolation = WingProfileInterpolation::C2Spline;
            let plane = AircraftBuilder::new(Some(geometry))
                .build(Some(&preset.design_vector), false)
                .expect("builds");
            let tc: Vec<f64> = plane.wings[0]
                .xsecs
                .iter()
                .map(|x| thickness(&x.airfoil))
                .collect();
            for pair in tc.windows(2) {
                assert!(
                    pair[1] <= pair[0] + 1.0e-4,
                    "{name}: t/c rises {} to {}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn tip_cap_is_elliptical_and_closes_the_tip() {
        assert_eq!(tip_cap_scale(0.5, 0.02), 1.0);
        assert!(tip_cap_scale(1.0, 0.02).abs() < 1.0e-12);
        let mid = tip_cap_scale(1.0 - 0.01, 0.02);
        assert!((mid - (0.75_f64).sqrt()).abs() < 1.0e-12);
    }
}
