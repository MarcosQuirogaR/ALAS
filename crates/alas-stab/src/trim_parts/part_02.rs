// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Horizontal and vertical tail volume coefficients `(Vh, Vv)` --
/// `tail_volume_coefficients`. `Vh = Sh Lh / (S c_bar)`,
/// `Vv = Sv Lv / (S b)`, with the moment arms taken from the wings' quarter-
/// MAC aerodynamic centres. `None` for either when the airplane has fewer than
/// two/three wings. The tails are identified by position (`wings[1]`,
/// `wings[2]`), as upstream indexes them, not by name.
pub fn tail_volume_coefficients(airplane: &Airplane) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, false)
}

/// Frozen translation/parity form of [`tail_volume_coefficients`].
pub fn tail_volume_coefficients_reference_compatibility(
    airplane: &Airplane,
) -> (Option<f64>, Option<f64>) {
    tail_volume_coefficients_with_reference_mode(airplane, true)
}

fn tail_volume_coefficients_with_reference_mode(
    airplane: &Airplane,
    reference_compatibility: bool,
) -> (Option<f64>, Option<f64>) {
    let s_ref = airplane.s_ref.max(1.0);
    let c_bar = airplane.c_ref.max(0.1);
    let b_ref = airplane.b_ref.max(1.0);
    let x_wing_ac = airplane.wings[0].aerodynamic_center(AC_CHORD_FRACTION)[0];

    let vh = if airplane.wings.len() > 1 {
        let hstab = &airplane.wings[1];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let area = if reference_compatibility {
            hstab.unfolded_area()
        } else {
            hstab.reference_area()
        };
        Some(area * l_h / (s_ref * c_bar))
    } else {
        None
    };

    let vv = if airplane.wings.len() > 2 {
        let vstab = &airplane.wings[2];
        let l_v = (vstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        // A vertical fin's planform is the XZ surface, so projecting it onto
        // the aircraft XY reference plane would collapse its area to zero.
        // Keep its physical fin planform explicit; only the main-wing
        // denominator and lateral arm are aircraft XY reference quantities.
        Some(vstab.unfolded_area() * l_v / (s_ref * b_ref))
    } else {
        None
    };

    (vh, vv)
}

/// One low-side/high-side VLM probe: a level operating point (no sideslip, no
/// rotation) at `alpha_deg`, solved at the analysis mesh resolution.
fn probe(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    atmosphere: Atmosphere,
    velocity: f64,
    alpha_deg: f64,
) -> Result<VlmResult, VlmError> {
    let op_point = OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    vlm::run(
        airplane,
        &op_point,
        resolution(analysis.spanwise_resolution),
        resolution(analysis.chordwise_resolution),
    )
}

/// The configuration's `i64` mesh resolution as the `usize` [`vlm::run`]
/// takes -- floored at one, as `alas-aero::analysis` does: a zero or negative
/// resolution is not a mesh, and both fields are documented multipliers of at
/// least one.
fn resolution(value: i64) -> usize {
    value.max(1) as usize
}

/// The main wing (or the first wing when none is named [`MAIN_WING_NAME`]) --
/// upstream's `next((w ... if w.name == "Main Wing"), airplane.wings[0])`.
fn main_wing(airplane: &Airplane) -> &Wing {
    airplane
        .wings
        .iter()
        .find(|w| w.name == MAIN_WING_NAME)
        .unwrap_or(&airplane.wings[0])
}

/// The horizontal stabilizer, if the airplane has one named [`HSTAB_NAME`].
fn hstab(airplane: &Airplane) -> Option<&Wing> {
    airplane.wings.iter().find(|w| w.name == HSTAB_NAME)
}

/// A clone of `airplane` with every [`HSTAB_NAME`] section's twist set to
/// `twist_deg` -- the rigid stabilizer perturbation, on a copy. See the module
/// doc for why this is a clone rather than a mutate-and-restore.
fn with_hstab_twist(airplane: &Airplane, twist_deg: f64) -> Airplane {
    let mut plane = airplane.clone();
    for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
        for xsec in &mut wing.xsecs {
            xsec.twist = twist_deg;
        }
    }
    plane
}

// A test asserts on values it constructed here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    /// A short circular fuselage spanning past the wing trailing edge, so
    /// [`fuselage_cm_alpha`] reaches both its fore-body and after-body
    /// branches -- what the VLM-fed functions need present (upstream's
    /// `fuselages[0]` would `IndexError` on an airplane with none).
    fn probe_fuselage() -> Fuselage {
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.5),
                station(2.0, 1.5),
                station(10.0, 1.5),
                station(18.0, 0.8),
            ],
        )
    }

    /// A small probe: a symmetric main wing, a circular fuselage, and
    /// optionally a horizontal stabilizer aft of the wing.
    fn probe_airplane(with_hstab: bool) -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        let mut wings = vec![main];
        if with_hstab {
            wings.push(Wing::new(
                HSTAB_NAME,
                vec![
                    WingXSec::new([15.0, 0.0, 0.0], 1.5, -2.0, naca("naca0012")),
                    WingXSec::new([15.0, 3.0, 0.0], 1.5, -2.0, naca("naca0012")),
                ],
                true,
            ));
        }
        let s_ref = wings[0].reference_area();
        let b_ref = wings[0].reference_span();
        let c_ref = wings[0].mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [1.0, 0.0, 0.0],
            wings,
            fuselages: vec![probe_fuselage()],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn munk_factor_clamps_to_the_tabulated_range() {
        assert_eq!(munk_apparent_mass_factor(2.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(4.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(20.0), 0.98);
        assert_eq!(munk_apparent_mass_factor(100.0), 0.98);
    }

    #[test]
    fn munk_factor_interpolates_between_table_points() {
        // Fineness 9 sits halfway between the 8 (0.91) and 10 (0.94) rows.
        assert!((munk_apparent_mass_factor(9.0) - 0.925).abs() < 1e-15);
    }

    #[test]
    fn fuselage_cm_alpha_is_zero_for_a_single_station_body() {
        // The `len(xs) < 2` guard: a one-station fuselage has no area to
        // integrate over.
        let mut plane = probe_airplane(true);
        plane.fuselages = vec![Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(2.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        )];
        assert_eq!(fuselage_cm_alpha(&plane, 5.5), 0.0);
    }

    #[test]
    fn static_margin_is_nan_when_the_two_probes_carry_the_same_lift() {
        // Coincident probe alphas make dCL exactly zero, reaching the
        // degeneracy guard -- the branch no real geometry reaches.
        let plane = probe_airplane(true);
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = static_margin(&plane, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
    }

    #[test]
    fn autobalance_leaves_the_cg_unshifted_when_the_static_margin_is_nan() {
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = autobalance(&mut plane, 0.1, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
        assert_eq!(plane.xyz_ref[0], before);
    }

    #[test]
    fn autobalance_shifts_the_cg_aft_toward_a_lower_target_margin() {
        // A stable probe has a positive static margin; asking for a smaller
        // one moves the CG aft (a positive shift), by SM being measured in
        // fractions of the chord.
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig::default();
        let sm_before =
            autobalance(&mut plane, 0.05, &analysis).expect("the probe meshes and solves");
        assert!(sm_before > 0.05, "sm_before={sm_before}");
        let expected = before + (sm_before - 0.05) * plane.c_ref;
        assert!(
            (plane.xyz_ref[0] - expected).abs() < 1e-12,
            "xyz_ref_x={} expected={expected}",
            plane.xyz_ref[0]
        );
    }

    #[test]
    fn stability_and_trim_degrades_to_pure_alpha_without_a_stabilizer() {
        let plane = probe_airplane(false);
        let analysis = AnalysisConfig::default();
        let result = stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0)
            .expect("the probe meshes and solves");
        assert!(result.trim_ih_deg.is_nan(), "i_h={}", result.trim_ih_deg);
        assert_eq!(result.cl_ih, 0.0);
        assert_eq!(result.cm_ih, 0.0);
        assert!(result.trim_alpha_deg.is_finite());
    }

    #[test]
    fn stability_and_trim_leaves_the_stabilizer_twist_unchanged() {
        // The perturbation is a clone, so the caller's aircraft comes back as
        // it went in -- the property that keeps repeated evaluations from
        // drifting onto altered geometry.
        let plane = probe_airplane(true);
        let before = plane.clone();
        let analysis = AnalysisConfig::default();
        stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0).expect("the probe meshes and solves");
        assert_eq!(plane.wings, before.wings);
    }

    #[test]
    fn with_hstab_twist_perturbs_the_copy_and_not_the_source() {
        let plane = probe_airplane(true);
        let perturbed = with_hstab_twist(&plane, 5.0);
        for xsec in &perturbed.wings[1].xsecs {
            assert_eq!(xsec.twist, 5.0);
        }
        for xsec in &plane.wings[1].xsecs {
            assert_eq!(xsec.twist, -2.0);
        }
    }

    #[test]
    fn tail_volume_coefficients_report_none_for_missing_surfaces() {
        let main_only = probe_airplane(false);
        let (vh, vv) = tail_volume_coefficients(&main_only);
        assert!(vh.is_none());
        assert!(vv.is_none());

        let with_hstab = probe_airplane(true);
        let (vh, vv) = tail_volume_coefficients(&with_hstab);
        assert!(vh.is_some_and(|v| v > 0.0), "Vh={vh:?}");
        assert!(vv.is_none());
    }

    #[test]
    fn tail_volume_uses_projected_stabilizer_area() {
        let mut plane = probe_airplane(true);
        // Introduce a nonzero stabilizer dihedral so unfolded and reference
        // planform areas are observably different.
        plane.wings[1].xsecs[1].xyz_le[2] = 1.0;
        let (vh, _) = tail_volume_coefficients(&plane);
        let hstab = &plane.wings[1];
        let x_wing_ac = plane.wings[0].aerodynamic_center(AC_CHORD_FRACTION)[0];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let expected = hstab.reference_area() * l_h / (plane.s_ref * plane.c_ref);
        assert!((vh.expect("horizontal tail volume") - expected).abs() < 1e-12);
        assert!((hstab.reference_area() - hstab.unfolded_area()).abs() > 1e-6);
    }
}

