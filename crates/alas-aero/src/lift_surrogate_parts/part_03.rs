// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vorlax::VlmWing;

    fn probe_geometry() -> VlmGeometry {
        let wing = |tag: &str, symmetric: bool, vertical: bool, origin: [f64; 3]| VlmWing {
            tag: tag.to_string(),
            symmetric,
            vertical,
            vortex_lift: false,
            span_projected_m: 10.0,
            chord_root_m: 2.0,
            chord_tip_m: 1.0,
            taper: 0.5,
            aspect_ratio: 100.0 / 7.5,
            sweep_quarter_chord_rad: 0.2,
            sweep_leading_edge_rad: None,
            twist_root_rad: 0.02,
            twist_tip_rad: -0.01,
            dihedral_rad: 0.0,
            area_reference_m2: 7.5,
            origin_m: origin,
        };
        VlmGeometry {
            reference_area_m2: 7.5,
            center_of_gravity_m: [0.0, 0.0, 0.0],
            mean_aerodynamic_chord_m: 1.6,
            reference_span_m: 10.0,
            moment_reference_m: [0.5, 0.0],
            wings: vec![
                wing("main_wing", true, false, [0.0, 0.0, 0.0]),
                wing("fin", false, true, [8.0, 0.0, 0.0]),
            ],
        }
    }

    /// A coarser grid than the production one, so the unit tests stay cheap.
    /// Four points is the fewest a cubic can be fitted through.
    fn coarse_grid() -> TrainingGrid {
        TrainingGrid {
            angle_of_attack_rad: vec![
                (-4.0f64).to_radians(),
                0.0,
                4.0f64.to_radians(),
                8.0f64.to_radians(),
                12.0f64.to_radians(),
            ],
            mach: vec![0.0, 0.2, 0.5, 0.8],
        }
    }

    fn trained() -> LiftSurrogate {
        LiftSurrogate::train(
            &probe_geometry(),
            &VlmSettings {
                number_spanwise_vortices: 4,
                number_chordwise_vortices: 2,
                ..Default::default()
            },
            &coarse_grid(),
        )
        .expect("the probe geometry trains")
    }

    #[test]
    fn the_surface_passes_through_every_point_it_was_fitted_through() {
        let surrogate = trained();
        let grid = coarse_grid();
        for (i, &alpha) in grid.angle_of_attack_rad.iter().enumerate() {
            for (j, &mach) in grid.mach.iter().enumerate() {
                let solution = surrogate.evaluate(alpha, mach);
                let sampled = surrogate.training().lift_coefficient[i][j];
                assert!(
                    (solution.inviscid_lift_coefficient - sampled).abs() < 1e-9,
                    "interpolation, not approximation: the fit is at zero smoothing"
                );
            }
        }
    }

    #[test]
    fn outside_the_training_rectangle_the_answer_is_the_edge_value() {
        let surrogate = trained();
        let grid = coarse_grid();
        let (lowest, highest) = (grid.mach[0], grid.mach[grid.mach.len() - 1]);
        let alpha = 4.0f64.to_radians();

        let at_edge = surrogate.evaluate(alpha, highest).inviscid_lift_coefficient;
        let beyond = surrogate.evaluate(alpha, 3.0).inviscid_lift_coefficient;
        assert!(
            (beyond - at_edge).abs() < 1e-12,
            "clamped, not extrapolated"
        );

        let at_floor = surrogate.evaluate(alpha, lowest).inviscid_lift_coefficient;
        let below = surrogate.evaluate(alpha, -1.0).inviscid_lift_coefficient;
        assert!((below - at_floor).abs() < 1e-12);
    }

    #[test]
    fn outside_queries_are_reported_and_checked_evaluations_reject_them() {
        let surrogate = trained();
        let in_domain = surrogate.evaluate(4.0f64.to_radians(), 0.3);
        assert!(in_domain.domain.in_domain());

        let edge = surrogate.evaluate(4.0f64.to_radians(), 3.0);
        assert!(edge.domain.mach_clamped);
        assert!(!edge.domain.in_domain());
        let mach_max = surrogate.grid().mach[surrogate.grid().mach.len() - 1];
        assert!((edge.domain.mach_distance - (3.0 - mach_max)).abs() < 1e-12);
        assert_eq!(edge.domain.alpha_distance_rad, 0.0);
        assert!(matches!(
            surrogate.evaluate_checked(4.0f64.to_radians(), 3.0),
            Err(SurrogateDomainError::OutOfDomain { .. })
        ));
        assert!(matches!(
            surrogate.evaluate_checked(f64::NAN, 0.3),
            Err(SurrogateDomainError::NonFinite { .. })
        ));
    }

    #[test]
    fn the_training_grid_is_flattened_mach_major_and_reshaped_back() {
        // A transposed reshape would still produce a smooth surface, so the
        // check is that lift grows with angle of attack at fixed Mach and
        // barely moves with Mach at fixed angle -- which is the wrong way
        // round for a transposed table.
        let surrogate = trained();
        let table = &surrogate.training().lift_coefficient;
        for row in table.windows(2) {
            assert!(
                row[1][0] > row[0][0],
                "lift grows down the angle-of-attack axis"
            );
        }
        let span_across_mach = table[0][table[0].len() - 1] - table[0][0];
        let span_across_alpha = table[table.len() - 1][0] - table[0][0];
        assert!(span_across_alpha.abs() > span_across_mach.abs() * 5.0);
    }

    #[test]
    fn a_supersonic_training_grid_is_refused_rather_than_silently_mis_evaluated() {
        let mut grid = coarse_grid();
        grid.mach.push(1.5);
        let error =
            LiftSurrogate::train(&probe_geometry(), &VlmSettings::default(), &grid).unwrap_err();
        assert_eq!(error, SurrogateError::Supersonic { mach: 1.5 });
    }

    #[test]
    fn the_default_grid_is_the_one_fidelity_zero_overrides_vortex_lattice_with() {
        let grid = TrainingGrid::default();
        assert_eq!(grid.mach, vec![0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]);
        assert_eq!(grid.angle_of_attack_rad.len(), 10);
        assert!(
            grid.mach.iter().all(|&mach| mach < 1.0),
            "the whole reason the supersonic branch is unreachable"
        );
    }

    #[test]
    fn the_fuselage_allowance_is_a_scale_on_the_wings_only_lift() {
        assert_eq!(
            aircraft_lift_coefficient(0.5, FUSELAGE_LIFT_CORRECTION),
            0.5 * 1.14
        );
    }

    #[test]
    fn each_wing_reports_a_coefficient_on_its_own_reference_area() {
        let surrogate = trained();
        assert_eq!(surrogate.wing_tags(), ["main_wing", "fin"]);
        let solution = surrogate.evaluate(4.0f64.to_radians(), 0.3);
        assert_eq!(solution.wing_lift_coefficient.len(), 2);
        // The fin is vertical, so it carries no lift in the aircraft's own
        // sense; the main wing carries essentially all of it.
        assert!(solution.wing_lift_coefficient[0] > 0.1);
        assert!(solution.wing_lift_coefficient[1].abs() < 1e-3);
    }
}

