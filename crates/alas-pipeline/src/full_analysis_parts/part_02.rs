// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

impl FullAnalysis {
    /// Required cruise lift coefficient based on weight and dynamic pressure.
    pub fn cruise_cl(&self, plane: &Airplane) -> f64 {
        let req = &self.config.requirements;
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * v * v;
        req.required_cruise_cl(q, plane.s_ref)
    }

    fn compute_design_point(&self, plane: &Airplane, polar: &PolarSweep) -> DesignPoint {
        let cl_target = self.cruise_cl(plane);
        let mut best_idx = 0usize;
        let mut min_diff = f64::INFINITY;
        for (i, &cl) in polar.cl.iter().enumerate() {
            let diff = (cl - cl_target).abs();
            if diff < min_diff {
                min_diff = diff;
                best_idx = i;
            }
        }
        DesignPoint {
            alpha_deg: polar.alpha_deg[best_idx],
            cl: polar.cl[best_idx],
            cd: polar.cd[best_idx],
            l_over_d: polar.l_over_d[best_idx],
        }
    }

    fn fit_polar(&self, plane: &Airplane, polar: &PolarSweep) -> PolarFit {
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
            // branch above intentionally retains the legacy unfolded AR.
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

    fn compute_trimmed_design_point(
        &self,
        plane: &Airplane,
        aero: &AeroAnalysis,
        fine_analysis: &alas_config::AnalysisConfig,
    ) -> Option<TrimmedDesignPoint> {
        let req = &self.config.requirements;
        let cl_target = self.cruise_cl(plane);
        let trim = if self.reference_compatibility {
            stability_and_trim_reference_compatibility(
                plane,
                fine_analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
        } else {
            stability_and_trim(
                plane,
                fine_analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
        }
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

    fn geometry_summary(&self, plane: &Airplane, design: &DesignVector) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        if let Some(wing) = plane.wings.first() {
            let unfolded_span_m = wing.unfolded_span();
            let unfolded_area_m2 = wing.unfolded_area();
            let projected_span_m = plane.b_ref;
            let projected_area_m2 = plane.s_ref;
            // These two legacy keys are consumed by the mission/report
            // boundary.  Product reports must expose the authoritative
            // projected XY reference; only the explicit compatibility path
            // retains the old unfolded values for frozen evidence.
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
        let legacy_break_span_m = wing.break_span_fraction * design.span_m / 2.0;
        let (
            break_span_m,
            break_span_fraction,
            inboard_sweep_deg,
            outboard_sweep_deg,
            break_leading_edge_x_offset_m,
        ) = if self.reference_compatibility {
            (
                legacy_break_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                legacy_break_span_m * design.sweep_deg.to_radians().tan(),
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
                legacy_break_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                legacy_break_span_m * design.sweep_deg.to_radians().tan(),
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
