// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


#[cfg(test)]
mod tests {
    use super::*;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn two_xsec_wing(symmetric: bool) -> Wing {
        Wing::new(
            "Probe",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    #[test]
    fn taper_ratio_is_tip_chord_over_root_chord() {
        let wing = two_xsec_wing(false);
        assert!((wing.taper_ratio() - 0.5).abs() < 1e-15);
    }

    #[test]
    fn subdivide_sections_rejects_a_ratio_below_two() {
        let wing = two_xsec_wing(false);
        assert_eq!(
            wing.subdivide_sections(1, SpacingFunction::Linspace),
            Err(SubdivideSectionsError::RatioTooSmall(1))
        );
        assert_eq!(
            wing.subdivide_sections(0, SpacingFunction::Linspace),
            Err(SubdivideSectionsError::RatioTooSmall(0))
        );
    }

    #[test]
    fn subdivide_sections_produces_ratio_times_n_minus_one_new_sections_plus_the_tip() {
        let wing = Wing::new(
            "Three",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 5.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([2.0, 10.0, 0.0], 1.0, 0.0, naca("naca2412")),
            ],
            false,
        );
        let subdivided = wing
            .subdivide_sections(4, SpacingFunction::Linspace)
            .expect("valid ratio");
        // Two lofted sections, 4 new xsecs each, plus the unchanged tip.
        assert_eq!(subdivided.xsecs.len(), 2 * 4 + 1);
        assert_eq!(subdivided.xsecs.last(), wing.xsecs.last());
    }

    #[test]
    fn subdivide_sections_reuses_the_shared_airfoil_without_blending() {
        // Same coordinates, same name, but two distinct `Airfoil` values --
        // exercising structural equality rather than a shared reference.
        let a = naca("naca0012");
        let b = naca("naca0012");
        assert_eq!(a, b);
        assert!(!std::ptr::eq(&a, &b));

        let wing = Wing::new(
            "Shared",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, a.clone()),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, b),
            ],
            false,
        );
        let subdivided = wing
            .subdivide_sections(3, SpacingFunction::Linspace)
            .expect("valid ratio");
        for xsec in &subdivided.xsecs {
            assert_eq!(xsec.airfoil, a);
        }
    }

    #[test]
    fn subdivide_sections_first_new_xsec_at_each_boundary_is_the_inner_airfoil_unblended() {
        // span_fractions_along_section[0] is exactly 0 (linspace's forced
        // endpoint), so a_weight == 1 there for every section -- the first
        // new cross-section after each original one always reuses the
        // inner airfoil verbatim, even when the two ends differ.
        let wing = Wing::new(
            "Distinct",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, naca("naca2412")),
            ],
            false,
        );
        let subdivided = wing
            .subdivide_sections(3, SpacingFunction::Linspace)
            .expect("valid ratio");
        assert_eq!(subdivided.xsecs[0].airfoil, naca("naca0012"));
    }

    #[test]
    fn subdivide_sections_with_linspace_places_new_cross_sections_evenly() {
        let wing = Wing::new(
            "ForSpacing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        let subdivided = wing
            .subdivide_sections(4, SpacingFunction::Linspace)
            .expect("valid ratio");
        // xyz_le is a linear blend of the two original xsecs, so Y separation
        // between consecutive new cross-sections mirrors span_fractions'
        // spacing directly.
        let ys: Vec<f64> = subdivided.xsecs.iter().map(|x| x.xyz_le[1]).collect();
        let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        for gap in &gaps {
            assert!(
                (gap - gaps[0]).abs() < 1e-9,
                "linspace gaps should all be equal: {gaps:?}"
            );
        }
    }

    #[test]
    fn subdivide_sections_with_cosspace_bunches_new_cross_sections_near_each_end() {
        // The branch `VortexLatticeMethod.run()` reaches through
        // `spanwise_spacing_function`, exercised directly here rather than
        // only through `alas-aero::vlm`'s fixture -- see the module doc.
        let wing = Wing::new(
            "ForSpacing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        let subdivided = wing
            .subdivide_sections(4, SpacingFunction::Cosspace)
            .expect("valid ratio");
        let ys: Vec<f64> = subdivided.xsecs.iter().map(|x| x.xyz_le[1]).collect();
        let first_gap = ys[1] - ys[0];
        let middle_gap = ys[2] - ys[1];
        assert!(
            first_gap < middle_gap,
            "cosspace should bunch stations near the root: first_gap={first_gap} middle_gap={middle_gap}"
        );
    }

    #[test]
    fn span_of_a_straight_symmetric_wing_doubles_the_half_span() {
        let wing = two_xsec_wing(true);
        // Root and tip share Y and Z with the quarter-chord line, so the
        // untwisted span is just the tip's Y coordinate.
        assert!((wing.span() - 20.0).abs() < 1e-9, "span={}", wing.span());
    }

    #[test]
    fn span_of_an_asymmetric_wing_is_the_half_span() {
        let wing = two_xsec_wing(false);
        assert!((wing.span() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn area_scales_with_span_and_average_chord() {
        let wing = two_xsec_wing(false);
        // Untwisted, unswept, no dihedral: sectional span equals the Y
        // separation, so area is span * mean chord.
        let expected = 10.0 * (2.0 + 1.0) / 2.0;
        assert!(
            (wing.area() - expected).abs() < 1e-9,
            "area={}",
            wing.area()
        );
    }

    #[test]
    fn projected_reference_area_does_not_grow_with_dihedral() {
        let wing = Wing::new(
            "Dihedral",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 10.0], 2.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        assert!((wing.projected_span() - 20.0).abs() < 1e-9);
        assert!((wing.projected_area() - 40.0).abs() < 1e-9);
        assert_eq!(wing.reference_span(), wing.projected_span());
        assert_eq!(wing.reference_area(), wing.projected_area());
        assert!(wing.span() > wing.projected_span());
        assert!(wing.area() > wing.projected_area());
    }

    #[test]
    fn taper_ratio_of_one_leaves_mean_aerodynamic_chord_equal_to_the_chord() {
        let wing = Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!((wing.mean_aerodynamic_chord() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn aerodynamic_center_of_a_symmetric_wing_has_zero_y() {
        let wing = two_xsec_wing(true);
        let ac = wing.aerodynamic_center(0.25);
        assert_eq!(ac[1], 0.0);
    }

    #[test]
    fn aerodynamic_center_y_is_nonzero_for_an_asymmetric_wing_off_the_centerline() {
        let wing = two_xsec_wing(false);
        let ac = wing.aerodynamic_center(0.25);
        assert!(ac[1] > 0.0);
    }

    #[test]
    fn aspect_ratio_is_span_squared_over_area() {
        // A rectangular asymmetric wing: span 10, chord 2, area 20, so the
        // geometric aspect ratio is 100 / 20 = 5.
        let wing = Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!(
            (wing.aspect_ratio() - 5.0).abs() < 1e-9,
            "aspect_ratio={}",
            wing.aspect_ratio()
        );
    }

    #[test]
    fn aspect_ratio_uses_the_symmetric_span_and_area_together() {
        // Mirroring doubles both span and area, so AR = (2b)^2 / (2S) is
        // twice the half-wing's b^2 / S -- the geometric ratio, not a
        // per-half one.
        let asymmetric = two_xsec_wing(false);
        let symmetric = two_xsec_wing(true);
        assert!(
            (symmetric.aspect_ratio() - 2.0 * asymmetric.aspect_ratio()).abs() < 1e-9,
            "symmetric AR={} asymmetric AR={}",
            symmetric.aspect_ratio(),
            asymmetric.aspect_ratio()
        );
    }

    #[test]
    fn mean_sweep_angle_is_zero_for_an_unswept_wing() {
        // A rectangular wing (constant chord, aligned leading edges) has
        // every chordwise station's root-to-tip vector pointing straight
        // along Y, at any x_nondim.
        let wing = Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!(wing.mean_sweep_angle(0.25).abs() < 1e-9);
    }

    #[test]
    fn mean_sweep_angle_is_positive_for_a_wing_swept_aft() {
        let wing = Wing::new(
            "Swept",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([5.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!(wing.mean_sweep_angle(0.25) > 0.0);
    }

    #[test]
    fn control_surface_area_is_always_zero() {
        let wing = two_xsec_wing(true);
        assert_eq!(wing.control_surface_area(), 0.0);
    }

    #[test]
    fn translate_shifts_every_xsec_leading_edge() {
        let wing = two_xsec_wing(false);
        let translated = wing.translate([5.0, -2.0, 1.0]);
        for (original, moved) in wing.xsecs.iter().zip(&translated.xsecs) {
            assert_eq!(moved.xyz_le, add3(original.xyz_le, [5.0, -2.0, 1.0]));
            assert_eq!(moved.chord, original.chord);
            assert_eq!(moved.twist, original.twist);
        }
    }
}

