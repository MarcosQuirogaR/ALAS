// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Design point, polar fit, trimmed point and geometry summary of a [`FullAnalysis`].

use super::*;

impl FullAnalysis {
    /// Required cruise lift coefficient based on weight and dynamic pressure.
    pub fn cruise_cl(&self, plane: &Airplane) -> f64 {
        let req = &self.config.requirements;
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * v * v;
        req.required_cruise_cl(q, plane.s_ref)
    }

    /// The cruise lift coefficient the report shows: the level-flight value at
    /// the mid-cruise mass, the mean of the takeoff mass (`requirements.mtow_kg`,
    /// which a sized report sets to the closed mass) and `zero_fuel_mass_kg`,
    /// i.e. all of the fuel burned in cruise; see [`crate::cruise_mass`]. A
    /// zero-fuel mass above the takeoff mass is clamped to the takeoff mass.
    ///
    /// Only the reported cruise CL, CD and L/D use it. The trim solution
    /// (`trim_ih_deg`, `geometric_body_alpha_deg`), the optimizer stall guard,
    /// the attitude window and the residuals keep the takeoff-mass
    /// [`Self::cruise_cl`]; so does the reference-compatibility path, whose
    /// frozen fixtures were generated at the takeoff mass.
    pub(super) fn reported_cruise_cl(&self, plane: &Airplane, zero_fuel_mass_kg: f64) -> f64 {
        let takeoff_cl = self.cruise_cl(plane);
        if self.reference_compatibility {
            return takeoff_cl;
        }
        let req = &self.config.requirements;
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * v * v;
        let fuel_kg = req.mtow_kg - zero_fuel_mass_kg;
        let mid_mass_kg = crate::cruise_mass::mid_cruise_mass_kg(req.mtow_kg, fuel_kg);
        let cl =
            crate::cruise_mass::cruise_cl_at_mass(mid_mass_kg, req.gravity_m_s2, q, plane.s_ref);
        if cl.is_finite() && cl > 0.0 {
            cl
        } else {
            takeoff_cl
        }
    }

    /// Move a trimmed point, solved at the takeoff-mass lift coefficient, to
    /// the reported cruise lift coefficient `reported_cl`.
    ///
    /// Only `cl`, `cd` and `l_over_d` change: they are read off the polar at
    /// `reported_cl` with the trim point's own trim-drag increment
    /// ([`crate::cruise_mass::cruise_point_at_cl`]). `trim_ih_deg`,
    /// `geometric_body_alpha_deg`, the displayed `alpha_deg` and `cm_residual`
    /// stay those of the trim solution at the takeoff mass, which is what the
    /// attitude window and the mission's tail incidence consume. The second
    /// value is `true` when the polar had to be clamped at its end values. The
    /// reference-compatibility path returns the point unchanged.
    pub(super) fn trimmed_point_at_reported_cl(
        &self,
        trimmed: TrimmedDesignPoint,
        polar: &PolarSweep,
        reported_cl: f64,
    ) -> (TrimmedDesignPoint, bool) {
        if self.reference_compatibility {
            return (trimmed, false);
        }
        match crate::cruise_mass::cruise_point_at_cl(polar, reported_cl, trimmed.cl, trimmed.cd) {
            Some(point) => (
                TrimmedDesignPoint {
                    cl: point.cl,
                    cd: point.cd,
                    l_over_d: point.l_over_d,
                    ..trimmed
                },
                point.clamped,
            ),
            None => (trimmed, false),
        }
    }

    pub(super) fn compute_design_point(
        &self,
        polar: &PolarSweep,
        cl_target: f64,
    ) -> Result<DesignPoint, String> {
        design_point_nearest(polar, cl_target)
    }

    pub(super) fn fit_polar(&self, plane: &Airplane, polar: &PolarSweep) -> PolarFit {
        let cfg = &self.config.analysis;
        let ar = if self.reference_compatibility {
            plane
                .wings
                .first()
                .map(|w| w.aspect_ratio())
                .unwrap_or(10.0)
        } else if plane.s_ref.is_finite() && plane.s_ref > 0.0 && plane.b_ref.is_finite() {
            // Polar normalization follows the same projected reference
            // quantities used by the product aircraft.  The compatibility
            // branch above intentionally retains the unfolded AR.
            plane.b_ref * plane.b_ref / plane.s_ref
        } else {
            10.0
        };
        Self::fit_polar_values(polar, ar, cfg)
    }

    pub(crate) fn fit_polar_values(
        polar: &PolarSweep,
        ar: f64,
        cfg: &alas_config::AnalysisConfig,
    ) -> PolarFit {
        let mut selected_indices: Vec<usize> = polar
            .cl
            .iter()
            .enumerate()
            .filter(|(_, &cl)| cl > cfg.polar_fit_cl_min && cl < cfg.polar_fit_cl_max)
            .map(|(i, _)| i)
            .collect();

        let used_fallback_window = selected_indices.len() < 3;
        if used_fallback_window {
            selected_indices = polar
                .cl
                .iter()
                .enumerate()
                .filter(|(_, &cl)| {
                    cl > cfg.polar_fit_cl_min_fallback && cl < cfg.polar_fit_cl_max_fallback
                })
                .map(|(i, _)| i)
                .collect();
        }

        let (cd0, k, status) = if selected_indices.len() >= 2 {
            // Keep a non-finite selected polar point from entering the QR
            // solve.  `least_squares` intentionally owns matrix-shape and
            // rank errors, while this boundary owns the polar's data
            // contract; otherwise NaN/Inf can flow through the arithmetic
            // and look like a successful fit.
            let selected_values_are_finite = selected_indices
                .iter()
                .all(|&i| polar.cl[i].is_finite() && polar.cd[i].is_finite());
            if !selected_values_are_finite {
                (0.02, 0.04, PolarFitStatus::FallbackLeastSquaresFailure)
            } else {
                let a_mat: Vec<Vec<f64>> = selected_indices
                    .iter()
                    .map(|&i| vec![1.0, polar.cl[i].powi(2)])
                    .collect();
                let b_vec: Vec<f64> = selected_indices.iter().map(|&i| polar.cd[i]).collect();
                match least_squares(&a_mat, &b_vec) {
                    Ok(sol) if sol.len() >= 2 && sol.iter().all(|value| value.is_finite()) => (
                        sol[0],
                        sol[1],
                        if used_fallback_window {
                            PolarFitStatus::FittedFallbackWindow
                        } else {
                            PolarFitStatus::Fitted
                        },
                    ),
                    Ok(_) | Err(_) => (0.02, 0.04, PolarFitStatus::FallbackLeastSquaresFailure),
                }
            }
        } else {
            (0.02, 0.04, PolarFitStatus::FallbackInsufficientPoints)
        };

        let oswald_e = if k > 0.0 {
            1.0 / (PI * ar * k)
        } else {
            f64::NAN
        };

        PolarFit {
            cd0,
            k,
            oswald_e,
            aspect_ratio: ar,
            status,
        }
    }

    pub(super) fn compute_trimmed_design_point(
        &self,
        plane: &Airplane,
        cl_target: f64,
        aero: &AeroAnalysis,
        fine_analysis: &alas_config::AnalysisConfig,
        fine_system: &VlmSystem<'_>,
    ) -> Option<TrimmedDesignPoint> {
        let req = &self.config.requirements;
        let solve_trim = if self.reference_compatibility {
            stability_and_trim_reference_compatibility_with_system
        } else {
            stability_and_trim_with_system
        };
        let trim = solve_trim(
            fine_system,
            plane,
            fine_analysis,
            cl_target,
            req.cruise_mach,
            req.cruise_altitude_m,
        )
        .ok()?;
        if !trim.converged {
            return None;
        }
        let trim_point = TrimPoint {
            trim_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim.trim_ih_deg,
            cl_alpha: trim.cl_alpha,
        };

        let trim_perf = aero
            .trimmed_performance(&trim_point, req.cruise_mach, req.cruise_altitude_m)
            .ok()?;
        if !trim_perf.cm_residual.is_finite() || trim_perf.cm_residual.abs() > 1.0e-3 {
            return None;
        }

        Some(TrimmedDesignPoint {
            alpha_deg: trim_perf.alpha_deg,
            geometric_body_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim_perf.incidence_deg,
            cl: trim_perf.cl,
            cd: trim_perf.cd,
            l_over_d: trim_perf.l_over_d,
            cm_residual: trim_perf.cm_residual,
        })
    }

    pub(super) fn geometry_summary(
        &self,
        plane: &Airplane,
        design: &DesignVector,
    ) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        if let Some(wing) = plane.wings.first() {
            let unfolded_span_m = wing.unfolded_span();
            let unfolded_area_m2 = wing.unfolded_area();
            let projected_span_m = plane.b_ref;
            let projected_area_m2 = plane.s_ref;
            // These two keys are consumed by the mission/report
            // boundary.  Product reports must expose the authoritative
            // projected XY reference; only the explicit compatibility path
            // retains the unfolded values the frozen fixtures were generated with.
            let reference_span_m = if self.reference_compatibility {
                unfolded_span_m
            } else {
                projected_span_m
            };
            let reference_area_m2 = if self.reference_compatibility {
                unfolded_area_m2
            } else {
                projected_area_m2
            };
            let reference_aspect_ratio = if self.reference_compatibility {
                wing.aspect_ratio()
            } else if reference_area_m2.is_finite() && reference_area_m2 > 0.0 {
                reference_span_m * reference_span_m / reference_area_m2
            } else {
                f64::NAN
            };
            map.insert("span_m".to_owned(), reference_span_m);
            map.insert("wing_area_m2".to_owned(), reference_area_m2);
            map.insert("projected_span_m".to_owned(), wing.projected_span());
            map.insert("projected_wing_area_m2".to_owned(), wing.projected_area());
            map.insert("unfolded_span_m".to_owned(), unfolded_span_m);
            map.insert("unfolded_wing_area_m2".to_owned(), unfolded_area_m2);
            map.insert("reference_span_m".to_owned(), projected_span_m);
            map.insert("reference_area_m2".to_owned(), projected_area_m2);
            map.insert("aspect_ratio".to_owned(), reference_aspect_ratio);
            map.insert(
                "mean_aerodynamic_chord_m".to_owned(),
                wing.mean_aerodynamic_chord(),
            );
        }
        map.insert(
            "taper_ratio".to_owned(),
            design.tip_chord_m / design.root_chord_m,
        );
        let wing = &self.config.geometry.wing;
        let break_semi_span_m = wing.break_span_fraction * design.span_m / 2.0;
        let (
            break_span_m,
            break_span_fraction,
            inboard_sweep_deg,
            outboard_sweep_deg,
            break_leading_edge_x_offset_m,
        ) = if self.reference_compatibility {
            (
                break_semi_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                break_semi_span_m * design.sweep_deg.to_radians().tan(),
            )
        } else if let Ok(planform) = wing.transport_planform(design) {
            (
                planform.kink.y_m,
                planform.kink.span_fraction,
                planform.inboard_le_sweep_deg,
                planform.outboard_le_sweep_deg,
                planform.kink.leading_edge_x_m,
            )
        } else {
            (
                break_semi_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                break_semi_span_m * design.sweep_deg.to_radians().tan(),
            )
        };
        map.insert("root_chord_m".to_owned(), design.root_chord_m);
        map.insert("break_chord_m".to_owned(), design.break_chord_m);
        map.insert("tip_chord_m".to_owned(), design.tip_chord_m);
        map.insert("break_span_m".to_owned(), break_span_m);
        map.insert("break_span_fraction".to_owned(), break_span_fraction);
        map.insert("inboard_sweep_deg".to_owned(), inboard_sweep_deg);
        map.insert("outboard_sweep_deg".to_owned(), outboard_sweep_deg);
        map.insert(
            "break_leading_edge_x_offset_m".to_owned(),
            break_leading_edge_x_offset_m,
        );
        map.insert("sweep_deg".to_owned(), design.sweep_deg);
        map.insert("fuselage_length_m".to_owned(), design.fuselage_length_m);
        if plane.wings.len() > 1 {
            let area = if self.reference_compatibility {
                plane.wings[1].unfolded_area()
            } else {
                plane.wings[1].reference_area()
            };
            map.insert("h_stab_area_m2".to_owned(), area);
        }
        if plane.wings.len() > 2 {
            // `reference_area()` is the aircraft XY projection and therefore
            // collapses a vertical tail to zero.  The vertical surface needs
            // its own XZ planform area in both product and compatibility
            // mission/report views.
            let area = plane.wings[2].unfolded_area();
            map.insert("v_stab_area_m2".to_owned(), area);
        }
        map
    }
}

// Failed expectations here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod reported_cruise_tests {
    use super::*;

    fn plane(s_ref: f64) -> Airplane {
        Airplane {
            name: "reference area probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref,
            c_ref: 1.0,
            b_ref: 1.0,
        }
    }

    fn parabolic_polar() -> PolarSweep {
        let cl: Vec<f64> = (0..=40).map(|i| i as f64 * 0.025).collect();
        let cd: Vec<f64> = cl.iter().map(|c| 0.02 + 0.04 * c * c).collect();
        let n = cl.len();
        PolarSweep {
            alpha_deg: vec![0.0; n],
            geometric_alpha_deg: vec![0.0; n],
            cd_induced: vec![0.0; n],
            cd_wave: vec![0.0; n],
            cd_parasite: vec![0.0; n],
            cm: vec![0.0; n],
            l_over_d: cl.iter().zip(&cd).map(|(c, d)| c / d).collect(),
            cl,
            cd,
        }
    }

    fn trimmed_at(cl: f64) -> TrimmedDesignPoint {
        let cd = 0.02 + 0.04 * cl * cl + 0.0008;
        TrimmedDesignPoint {
            alpha_deg: 2.4,
            geometric_body_alpha_deg: 2.1,
            trim_ih_deg: -1.3,
            cl,
            cd,
            l_over_d: cl / cd,
            cm_residual: 1.0e-6,
        }
    }

    #[test]
    fn the_reported_cruise_cl_is_the_lift_coefficient_at_the_breguet_mid_mass() {
        let config = AlasConfig::default();
        let analysis = FullAnalysis::new(config.clone());
        let plane = plane(120.0);
        let takeoff_kg = config.requirements.mtow_kg;
        let zero_fuel_kg = 0.8 * takeoff_kg;
        let takeoff_cl = analysis.cruise_cl(&plane);
        let reported = analysis.reported_cruise_cl(&plane, zero_fuel_kg);
        let expected = takeoff_cl * 0.5 * (takeoff_kg + zero_fuel_kg) / takeoff_kg;
        assert!(
            (reported - expected).abs() < 1e-12 * expected,
            "{reported} vs {expected}"
        );
        assert!(reported < takeoff_cl);
        // A zero-fuel mass above the takeoff mass clamps to the takeoff CL.
        let clamped = analysis.reported_cruise_cl(&plane, 1.1 * takeoff_kg);
        assert!((clamped - takeoff_cl).abs() < 1e-12 * takeoff_cl);
    }

    #[test]
    fn the_reference_compatibility_path_reports_the_takeoff_mass_point_unchanged() {
        let analysis = FullAnalysis::new_reference_compatibility(AlasConfig::default());
        let plane = plane(120.0);
        let zero_fuel_kg = 0.8 * analysis.config.requirements.mtow_kg;
        assert_eq!(
            analysis.reported_cruise_cl(&plane, zero_fuel_kg),
            analysis.cruise_cl(&plane)
        );
        let trimmed = trimmed_at(0.6);
        let (reported, clamped) =
            analysis.trimmed_point_at_reported_cl(trimmed, &parabolic_polar(), 0.5);
        assert_eq!(reported, trimmed);
        assert!(!clamped);
    }

    #[test]
    fn only_the_reported_cl_cd_and_l_over_d_move_to_the_mid_cruise_point() {
        let analysis = FullAnalysis::new(AlasConfig::default());
        let trimmed = trimmed_at(0.6);
        let (reported, clamped) =
            analysis.trimmed_point_at_reported_cl(trimmed, &parabolic_polar(), 0.5);
        assert!(!clamped);
        assert_eq!(reported.cl, 0.5);
        // Same trim-drag increment as the takeoff-mass trim point.
        assert!((reported.cd - (0.02 + 0.04 * 0.25 + 0.0008)).abs() < 1e-12);
        assert!((reported.l_over_d - 0.5 / reported.cd).abs() < 1e-12);
        // The trim solution the attitude window and the mission consume stays.
        assert_eq!(reported.trim_ih_deg, trimmed.trim_ih_deg);
        assert_eq!(
            reported.geometric_body_alpha_deg,
            trimmed.geometric_body_alpha_deg
        );
        assert_eq!(reported.alpha_deg, trimmed.alpha_deg);
        assert_eq!(reported.cm_residual, trimmed.cm_residual);
        let (_, outside) = analysis.trimmed_point_at_reported_cl(trimmed, &parabolic_polar(), 1.4);
        assert!(outside, "a CL above the polar is flagged");
    }
}
