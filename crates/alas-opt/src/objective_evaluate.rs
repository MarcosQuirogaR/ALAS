// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate scalar-cost evaluation for the aircraft design objective.

use super::{
    apply_candidate_payload_load_case, parasite_drag_reference_compatibility, DesignObjective,
};

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{MassCoordinateModel, PayloadLayoutSummary};
use alas_payload::build::{build_payload_layout, build_payload_layout_reference_compatibility};
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::{stability_and_trim, stability_and_trim_reference_compatibility};

use crate::envelope::{assess_model_cg_envelope, check_cg_envelope};
use crate::mesh_correction::corrected_body_alpha;
use crate::transport_planform::assess_product_transport_planform;

#[path = "objective_evaluate_cost.rs"]
mod cost;
use cost::{score_candidate, ObjectiveCostInputs};

impl DesignObjective {
    /// Evaluate the scalar cost for candidate design vector `x`.
    pub fn evaluate(&mut self, x: &[f64]) -> f64 {
        let w = self.config.optimizer.weights.clone();

        let dv = match DesignVector::from_array(x) {
            Ok(dv) => dv,
            Err(_) => {
                let cost = w.failure_cost;
                self.history.record(
                    DesignVector::default(),
                    false,
                    cost,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    "geometry_build",
                );
                return cost;
            }
        };

        // Resolve capacity only for a load case that explicitly asks candidate
        // geometry to determine payload. Fixed passenger targets stay fixed.
        if apply_candidate_payload_load_case(&mut self.config, &dv).is_err() {
            let cost = w.failure_cost;
            self.history
                .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "geometry_build");
            return cost;
        }

        let builder = AircraftBuilder::new(Some(self.config.geometry.clone()));
        let mut plane: Airplane = match builder.build(Some(&dv), false) {
            Ok(p) => p,
            Err(_) => {
                let cost = w.failure_cost;
                self.history
                    .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "geometry_build");
                return cost;
            }
        };

        if self.reference_mass_coordinates {
            // The frozen objective predates the projected XY reference
            // contract. Keep its historical scales at this explicit parity
            // seam while native optimization consumes builder references.
            if let Some(wing) = plane.wings.first() {
                let s_ref = wing.unfolded_area();
                // The reference builder stored the design-vector span rather
                // than deriving a 3-D unfolded span from section coordinates.
                let b_ref = dv.span_m;
                plane.s_ref = s_ref;
                plane.b_ref = b_ref;
            }
        }

        if plane.s_ref <= 0.0 || plane.c_ref <= 0.0 {
            let cost = w.failure_cost;
            self.history
                .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "geometry_build");
            return cost;
        }

        let req = &self.config.requirements;

        let transport_constraints_active =
            w.transport_planform_constraints_enabled && !self.reference_mass_coordinates;
        let analysis_config = self.config.analysis.clone();
        // These bounds describe a preferred transport-aircraft shape rather
        // than a feasibility condition. Keep them for frozen reference replay,
        // but do not silently steer the native product search unless the user
        // explicitly enables the shape-prior model.
        let transport_planform = if transport_constraints_active {
            match assess_product_transport_planform(&plane, &dv, &self.config) {
                Some(assessment) => Some(assessment),
                None => {
                    let cost = w.failure_cost;
                    self.history.record(
                        dv,
                        false,
                        cost,
                        0.0,
                        dv.span_m,
                        0.0,
                        plane.s_ref,
                        0.0,
                        "transport_planform",
                    );
                    return cost;
                }
            }
        } else {
            None
        };

        // Weight and balance / mass analysis: two passes
        let coordinate_model = if self.reference_mass_coordinates {
            MassCoordinateModel::ReferenceCompatibility
        } else {
            MassCoordinateModel::StructuralWingbox(&self.config.structures)
        };
        let initial_mass_result = if self.reference_mass_coordinates {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                None,
                coordinate_model,
                &self.config.landing_gear,
            )
        } else {
            alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                None,
                coordinate_model,
                &self.config.landing_gear,
            )
        };
        let (m1, c1, _cg1) = match initial_mass_result {
            Ok(result) => result,
            Err(_) => {
                let cost = w.failure_cost;
                self.history.record(
                    dv,
                    false,
                    cost,
                    0.0,
                    0.0,
                    0.0,
                    plane.s_ref,
                    0.0,
                    "mass_coordinates",
                );
                return cost;
            }
        };

        let second_pass = (|| -> Result<_, String> {
            let (oew, x_oew) = oew_and_cg(&m1, &c1);
            let payload_layout = if self.reference_mass_coordinates {
                build_payload_layout_reference_compatibility(&plane, &self.config, oew, x_oew)
            } else {
                build_payload_layout(&plane, &self.config, oew, x_oew)
            }
            .map_err(|error| format!("payload layout error: {error}"))?;
            let summary = PayloadLayoutSummary {
                total_mass: payload_layout.total_mass,
                cg_x: payload_layout.cg_x,
                cg_y: payload_layout.cg_y,
            };
            let result = if self.reference_mass_coordinates {
                alas_mass::breakdown::run_mass_analysis_with_model_checked_with_gear(
                    &plane,
                    req,
                    &self.config.geometry,
                    &self.config.cabin,
                    &self.config.control_surfaces,
                    Some(&self.config.mass_model),
                    Some(&summary),
                    coordinate_model,
                    &self.config.landing_gear,
                )
            } else {
                alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
                    &plane,
                    req,
                    &self.config.geometry,
                    &self.config.cabin,
                    &self.config.control_surfaces,
                    Some(&self.config.mass_model),
                    Some(&summary),
                    coordinate_model,
                    &self.config.landing_gear,
                )
            };
            result.map_err(|error| format!("mass coordinates after payload layout: {error}"))
        })();

        let (masses, coords, cg) = match second_pass {
            Ok(result) => result,
            Err(stage_error) => {
                let cost = w.failure_cost;
                self.history.record(
                    dv,
                    false,
                    cost,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    plane.s_ref,
                    if stage_error.starts_with("payload layout") {
                        "payload_layout"
                    } else {
                        "mass_coordinates"
                    },
                );
                return cost;
            }
        };
        let m_fuel = masses.fuel;
        let cg_x = cg[0];

        plane.xyz_ref[0] = cg_x;

        // Cruise target CL
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * v.powi(2);
        let cl_target = req.required_cruise_cl(q, plane.s_ref);

        // Python rejects candidates outside the cruise lift limit before trim;
        // allowing them through would score physically stalled designs.
        if cl_target > req.max_cruise_cl || cl_target <= 0.0 {
            let cost = w.failure_cost;
            self.history
                .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "stall_guard");
            return cost;
        }

        // Longitudinal stability & trim solve + trimmed aero performance
        let trim_res = if self.reference_mass_coordinates {
            stability_and_trim_reference_compatibility(
                &plane,
                &analysis_config,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
        } else {
            stability_and_trim(
                &plane,
                &analysis_config,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
        };

        let (trim, sm_physical, sm_floor_violation, cg_envelope_violation, cg_exc_max) =
            match trim_res {
                Ok(t) => {
                    let sm = t.static_margin;
                    let sm_viol = sm.is_nan() || sm < req.min_physical_static_margin;
                    let (cg_violation, cg_exceedance) = if self.reference_mass_coordinates {
                        let cg_res = check_cg_envelope(
                            &plane,
                            &masses,
                            &coords,
                            cg_x,
                            t.x_np,
                            plane.c_ref,
                            &self.config,
                        );
                        (cg_res.violation, cg_res.worst_exceedance)
                    } else {
                        let Ok(cg_res) = assess_model_cg_envelope(
                            &plane,
                            &masses,
                            &coords,
                            cg_x,
                            t.x_np,
                            plane.c_ref,
                            &self.config,
                        ) else {
                            let cost = w.failure_cost;
                            self.history.record(
                                dv,
                                false,
                                cost,
                                0.0,
                                0.0,
                                0.0,
                                plane.s_ref,
                                0.0,
                                "cg_model",
                            );
                            return cost;
                        };
                        (
                            !cg_res.hard_constraints_pass(),
                            cg_res.worst_hard_exceedance(),
                        )
                    };
                    (t, sm, sm_viol, cg_violation, cg_exceedance)
                }
                Err(_) => {
                    let cost = w.failure_cost;
                    self.history
                        .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "trim_solve");
                    return cost;
                }
            };

        // Calibrate the one-panel alpha bias once; candidate changes still
        // come from the fast solver rather than a prescribed target shape.
        let geometric_body_alpha_deg = if transport_constraints_active {
            match corrected_body_alpha(
                &plane,
                &self.config.analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
                trim.trim_alpha_deg,
                &mut self.body_alpha_mesh_correction_deg,
            ) {
                Ok(alpha) => alpha,
                Err(()) => {
                    let cost = w.failure_cost;
                    self.history.record(
                        dv,
                        false,
                        cost,
                        0.0,
                        0.0,
                        0.0,
                        plane.s_ref,
                        0.0,
                        "fine_trim_solve",
                    );
                    return cost;
                }
            }
        } else {
            trim.trim_alpha_deg
        };

        let aero = AeroAnalysis::new(
            &plane,
            dv.sweep_deg,
            Some(self.config.geometry.clone()),
            Some(self.config.drag_model.clone()),
            Some(analysis_config),
        );

        let tp = TrimPoint {
            trim_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim.trim_ih_deg,
            cl_alpha: trim.cl_alpha,
        };

        let (ld, alpha, trim_ih, cd0) =
            match aero.trimmed_performance(&tp, req.cruise_mach, req.cruise_altitude_m) {
                Ok(perf) => {
                    let product_cd0 = aero.parasite_drag(
                        req.cruise_mach,
                        req.cruise_altitude_m,
                        cl_target,
                        None,
                        None,
                    );
                    let cd0_val = if self.reference_mass_coordinates {
                        parasite_drag_reference_compatibility(
                            &aero,
                            req.cruise_mach,
                            req.cruise_altitude_m,
                        )
                    } else {
                        product_cd0
                    };
                    let total_cd = perf.cd + cd0_val - product_cd0;
                    let l_over_d = perf.cl / total_cd;
                    (l_over_d, perf.alpha_deg, perf.incidence_deg, cd0_val)
                }
                Err(_) => {
                    let cost = w.failure_cost;
                    self.history
                        .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "trim_solve");
                    return cost;
                }
            };

        score_candidate(
            self,
            ObjectiveCostInputs {
                dv,
                plane,
                m_fuel,
                alpha,
                trim_ih,
                ld,
                cd0,
                sm_physical,
                sm_floor_violation,
                cg_envelope_violation,
                cg_exc_max,
                transport_planform,
                transport_constraints_active,
                geometric_body_alpha_deg,
            },
        )
    }
}
