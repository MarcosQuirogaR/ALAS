// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/objective.py
// Reference: alas @ rust-port-baseline.

//! Scalar objective function for aircraft design space optimization.
//!
//! Evaluates aerodynamic performance (L/D) alongside physical constraints
//! (stability, CG envelope, tank capacity, cabin sizing, geometry bounds).

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked, MassCoordinateModel, PayloadLayoutSummary,
};
use alas_payload::build::{apply_cabin_preset, build_payload_layout, CabinPresetError};
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::{stability_and_trim, tail_volume_coefficients};

use crate::envelope::{assess_model_cg_envelope, check_cg_envelope};
use crate::history::OptimizationHistory;
use crate::mesh_correction::corrected_body_alpha;
use crate::transport_planform::{assess_product_transport_planform, transport_planform_penalty};

/// Torenbeek geometric wing fuel-tank volume estimate in cubic meters.
pub fn wing_fuel_volume_m3(wing: &Wing, usable_fraction: f64) -> f64 {
    let s = wing.area();
    let b = wing.span();
    let taper = wing.taper_ratio();
    let sample = linspace(0.0, 1.0, 101);
    let t_over_c_root = if !wing.xsecs.is_empty() {
        wing.xsecs[0].airfoil.max_thickness(&sample)
    } else {
        0.12
    };
    let term_taper = (1.0 + taper + taper.powi(2)) / (1.0 + taper).powi(2);
    let v_geo = 0.54 * (s.powi(2) / b.max(1e-6)) * t_over_c_root * term_taper;
    v_geo * usable_fraction.clamp(0.0, 1.0)
}

/// Resolve a geometry-driven payload only when the configured load case asks.
///
/// A fixed passenger target must survive every candidate evaluation. Cargo
/// presets retain their existing capacity-derived behavior, while passenger
/// capacity sizing is an explicit opt-in on [`alas_config::DesignRequirements`].
pub(crate) fn apply_candidate_payload_load_case(
    config: &mut AlasConfig,
    design_vector: &DesignVector,
) -> Result<(), CabinPresetError> {
    if !config
        .requirements
        .resolves_payload_from_candidate_geometry()
    {
        return Ok(());
    }
    apply_cabin_preset(config, Some(design_vector))
}

/// Callable cost function for aircraft design space optimization.
#[derive(Debug, Clone)]
pub struct DesignObjective {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    /// Trajectory of evaluated candidates.
    pub history: OptimizationHistory,
    /// Original passenger count target before cabin preset scaling.
    pub target_num_passengers: i64,
    /// Original cargo payload target in kg.
    pub target_cargo_payload_kg: f64,
    reference_mass_coordinates: bool,
    body_alpha_mesh_correction_deg: Option<f64>,
}

impl DesignObjective {
    /// Construct a new design objective initialized from `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self::with_mass_coordinate_compatibility(config, false)
    }

    /// Construct an objective that reproduces the frozen Python mass point.
    ///
    /// This is only for reference-parity replay. Product optimization uses
    /// [`Self::new`], which evaluates the configured structural-wingbox
    /// centroid and refuses an invalid structural configuration.
    pub fn new_reference_compatibility(config: AlasConfig) -> Self {
        Self::with_mass_coordinate_compatibility(config, true)
    }

    fn with_mass_coordinate_compatibility(
        mut config: AlasConfig,
        reference_mass_coordinates: bool,
    ) -> Self {
        if reference_mass_coordinates {
            // The parity fixture predates the native transport-planform
            // defaults. Restore its three-station planform before geometry,
            // mass and fuel-volume calculations so the frozen cost remains
            // attributable to the translated reference model.
            config.geometry.wing.side_of_body_span_fraction = None;
            config.geometry.wing.side_of_body_chord_ratio = None;
            config.geometry.wing.kink_span_fraction = None;
            config.geometry.wing.outboard_le_sweep_deg = None;
        }
        config.geometry.engine.apply_engine_spec();
        let target_num_passengers = config.requirements.num_passengers;
        let target_cargo_payload_kg = config.requirements.cargo_payload_kg;
        Self {
            config,
            history: OptimizationHistory::new(),
            target_num_passengers,
            target_cargo_payload_kg,
            reference_mass_coordinates,
            body_alpha_mesh_correction_deg: None,
        }
    }

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
        let plane: Airplane = match builder.build(Some(&dv), false) {
            Ok(p) => p,
            Err(_) => {
                let cost = w.failure_cost;
                self.history
                    .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "geometry_build");
                return cost;
            }
        };

        if plane.s_ref <= 0.0 || plane.c_ref <= 0.0 {
            let cost = w.failure_cost;
            self.history
                .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "geometry_build");
            return cost;
        }

        let req = &self.config.requirements;
        let mm = &self.config.mass_model;

        let transport_constraints_active =
            w.transport_planform_constraints_enabled && !self.reference_mass_coordinates;
        let analysis_config = self.config.analysis.clone();
        // These bounds describe a preferred transport-aircraft shape rather
        // than a feasibility condition. Keep them for frozen reference replay,
        // but do not silently steer the native product search unless the user
        // explicitly enables the shape-prior model.
        let subjective_shape_priors_active =
            self.reference_mass_coordinates || w.transport_shape_priors_enabled;
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
        let (m1, c1, cg1) = match run_mass_analysis_with_model_checked(
            &plane,
            req,
            &self.config.geometry,
            &self.config.cabin,
            &self.config.control_surfaces,
            Some(&self.config.mass_model),
            None,
            coordinate_model,
        ) {
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

        let second_pass = (|| {
            let (oew, x_oew) = oew_and_cg(&m1, &c1);
            let payload_layout = build_payload_layout(&plane, &self.config, oew, x_oew).ok()?;
            let summary = PayloadLayoutSummary {
                total_mass: payload_layout.total_mass,
                cg_x: payload_layout.cg_x,
                cg_y: payload_layout.cg_y,
            };
            run_mass_analysis_with_model_checked(
                &plane,
                req,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                Some(&summary),
                coordinate_model,
            )
            .ok()
        })();

        let (masses, coords, cg) = second_pass.unwrap_or((m1, c1, cg1));
        let m_fuel = masses.fuel;
        let cg_x = cg[0];

        let mut plane = plane;
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
        let trim_res = stability_and_trim(
            &plane,
            &analysis_config,
            cl_target,
            req.cruise_mach,
            req.cruise_altitude_m,
        );

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
                    let cd0_val = aero.parasite_drag(
                        req.cruise_mach,
                        req.cruise_altitude_m,
                        cl_target,
                        None,
                        None,
                    );
                    (perf.l_over_d, perf.alpha_deg, perf.incidence_deg, cd0_val)
                }
                Err(_) => {
                    let cost = w.failure_cost;
                    self.history
                        .record(dv, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, "trim_solve");
                    return cost;
                }
            };

        // Assemble cost: primary L/D reward
        let mut cost = -w.ld_weight * ld;

        // Payload / Seating shortfall penalty
        let mut shortfall_pct = 0.0;
        if req.aircraft_type == "cargo" {
            if req.cargo_payload_kg < self.target_cargo_payload_kg {
                shortfall_pct = (self.target_cargo_payload_kg - req.cargo_payload_kg)
                    / self.target_cargo_payload_kg.max(1.0);
            }
        } else if req.optimize_passenger_capacity && req.num_passengers < self.target_num_passengers
        {
            shortfall_pct = (self.target_num_passengers - req.num_passengers) as f64
                / (self.target_num_passengers as f64).max(1.0);
        }

        if shortfall_pct > 0.0 {
            cost += shortfall_pct.powi(2) * w.payload_shortfall_penalty_scale;
        }

        // Tail-area fractions
        let s_wing_ref = plane.s_ref.max(1.0);
        if plane.wings.len() > 1 {
            let ratio_h = plane.wings[1].area() / s_wing_ref;
            if ratio_h < w.min_hstab_area_fraction {
                let deficit = (w.min_hstab_area_fraction - ratio_h) / w.min_hstab_area_fraction;
                cost += deficit.powi(2) * w.tail_area_penalty_scale;
            }
        }
        if plane.wings.len() > 2 {
            let ratio_v = plane.wings[2].area() / s_wing_ref;
            if ratio_v < w.min_vstab_area_fraction {
                let deficit = (w.min_vstab_area_fraction - ratio_v) / w.min_vstab_area_fraction;
                cost += deficit.powi(2) * w.tail_area_penalty_scale;
            }
        }

        // Tail volume coefficients
        let (vh_opt, vv_opt) = tail_volume_coefficients(&plane);
        if let Some(vh) = vh_opt {
            if vh < w.min_hstab_volume_coef {
                let deficit = (w.min_hstab_volume_coef - vh) / w.min_hstab_volume_coef.max(1e-6);
                cost += deficit.powi(2) * w.tail_volume_penalty_scale;
            } else if vh > w.max_hstab_volume_coef {
                let excess = (vh - w.max_hstab_volume_coef) / w.max_hstab_volume_coef.max(1e-6);
                cost += excess.powi(2) * w.tail_volume_penalty_scale;
            }
        }
        if let Some(vv) = vv_opt {
            if vv < w.min_vstab_volume_coef {
                let deficit = (w.min_vstab_volume_coef - vv) / w.min_vstab_volume_coef.max(1e-6);
                cost += deficit.powi(2) * w.tail_volume_penalty_scale;
            } else if vv > w.max_vstab_volume_coef {
                let excess = (vv - w.max_vstab_volume_coef) / w.max_vstab_volume_coef.max(1e-6);
                cost += excess.powi(2) * w.tail_volume_penalty_scale;
            }
        }

        if subjective_shape_priors_active {
            // Wing position and fuselage proportions are configurable design
            // priors, not universal laws. In particular, the historical
            // absolute fuselage floor is not valid across aircraft classes.
            let x_wing_le = if !plane.wings.is_empty() && !plane.wings[0].xsecs.is_empty() {
                plane.wings[0].xsecs[0].xyz_le[0]
            } else {
                0.0
            };
            let wing_pos_frac = x_wing_le / dv.fuselage_length_m.max(1.0);
            if wing_pos_frac < w.min_wing_position_fraction {
                let deficit = w.min_wing_position_fraction - wing_pos_frac;
                cost += deficit.powi(2) * w.wing_position_penalty_scale;
            }

            let fuselage_diameter_m = self.config.geometry.fuselage.diameter_m.max(1e-6);
            let fineness_ratio = dv.fuselage_length_m / fuselage_diameter_m;
            if fineness_ratio > w.fineness_ratio_max {
                cost += (fineness_ratio - w.fineness_ratio_max).powi(2)
                    * w.fineness_ratio_penalty_scale;
            }
        }

        // Alpha window penalty
        if alpha < w.alpha_min_penalty_deg {
            cost += (alpha - w.alpha_min_penalty_deg).powi(2) * w.alpha_penalty_scale;
        } else if alpha > w.alpha_max_penalty_deg {
            cost += (alpha - w.alpha_max_penalty_deg).powi(2) * w.alpha_penalty_scale;
        }

        // Span structural proxy
        cost += dv.span_m * w.span_penalty_per_m;

        // Parasitic drag floor
        cost += cd0 * w.cd0_penalty_scale;

        // Preserve a usable search gradient around the declared area limit.
        // A hard pre-physics rejection made the default and much of the
        // initial population share the identical failure score, preventing
        // differential evolution from learning which direction was better.
        let area_roundoff_m2 = req.max_wing_area_m2.abs().max(1.0) * 1.0e-12;
        let wing_area_excess = plane.s_ref > req.max_wing_area_m2 + area_roundoff_m2;
        let wing_area_limit_violation = wing_area_excess && !self.reference_mass_coordinates;
        if wing_area_excess {
            let excess_frac = (plane.s_ref - req.max_wing_area_m2) / req.max_wing_area_m2.max(1.0);
            cost += excess_frac.powi(2) * w.area_penalty_scale;
        }

        // Wing loading floor
        let ws = req.mtow_kg / plane.s_ref.max(1e-6);
        let wing_loading_below_floor = ws < req.min_wing_loading_kg_m2;
        let wing_loading_violation = wing_loading_below_floor && !self.reference_mass_coordinates;
        if wing_loading_below_floor {
            let deficit_frac =
                (req.min_wing_loading_kg_m2 - ws) / req.min_wing_loading_kg_m2.max(1.0);
            cost += deficit_frac.powi(2) * w.wing_loading_penalty_scale;
        }

        // Static margin target deviation
        let sm_err = sm_physical - req.target_static_margin;
        if sm_err.abs() > 0.5 {
            cost += (sm_err.abs() * 10.0).powi(3);
        } else {
            cost += sm_err.powi(2) * w.static_margin_penalty_scale;
        }

        // Instability floor penalty
        if sm_floor_violation {
            let deficit = if sm_physical.is_finite() {
                (req.min_physical_static_margin - sm_physical).max(0.0)
            } else {
                1.0
            };
            let severity = (w.instability_failure_cost / 1000.0).cbrt() * 20.0;
            cost += (deficit * severity).powi(3);
        }

        // CG envelope penalty / reward
        if cg_envelope_violation {
            cost += (cg_exc_max * 100.0).powi(2) * w.cg_envelope_penalty_scale;
        } else {
            cost -= w.cg_envelope_reward;
        }

        // Fuel budget
        if m_fuel < 0.0 {
            cost += (-m_fuel / req.mtow_kg.max(1.0)) * w.fuel_penalty_scale;
        }

        // The native transport path integrates the actual spar-box volume and
        // reserves non-tankable inboard and outboard bays. The frozen path
        // retains the reference global correlation below.
        if !transport_constraints_active && !plane.wings.is_empty() {
            let tank_capacity_kg =
                wing_fuel_volume_m3(&plane.wings[0], mm.fuel_tank_usable_fraction)
                    * mm.fuel_density_kg_m3;
            let required_fuel_kg = m_fuel.max(0.0);
            if required_fuel_kg > tank_capacity_kg {
                let shortfall = (required_fuel_kg - tank_capacity_kg) / required_fuel_kg.max(1.0);
                cost += shortfall.powi(2) * w.fuel_volume_penalty_scale;
            }
        }

        if subjective_shape_priors_active {
            if dv.airfoil_thickness_scale < w.thickness_floor {
                cost +=
                    (w.thickness_floor - dv.airfoil_thickness_scale) * w.thickness_penalty_scale;
            }

            if dv.fuselage_length_m < w.fuselage_floor_m {
                cost += (w.fuselage_floor_m - dv.fuselage_length_m) * w.fuselage_penalty_scale;
            }
        }

        if !transport_constraints_active {
            // Legacy product and reference replay retain the historical taper
            // and root-corner proxies when transport constraints are disabled.
            let taper_ratio_break = dv.break_chord_m / dv.root_chord_m.max(0.1);
            if taper_ratio_break > w.max_break_root_chord_ratio {
                let excess = taper_ratio_break - w.max_break_root_chord_ratio;
                cost += excess.powi(2) * w.taper_realism_penalty_scale;
            }

            let wing_g = &self.config.geometry.wing;
            let y_break_root = wing_g.break_span_fraction * (dv.span_m / 2.0);
            if y_break_root > 1e-9 {
                let dx_break_root = y_break_root * dv.sweep_deg.to_radians().tan();
                let te_dx_root = dx_break_root + dv.break_chord_m - dv.root_chord_m;
                let te_angle_deg = y_break_root.atan2(te_dx_root).to_degrees();
                if te_angle_deg > 90.0 {
                    let excess_deg = te_angle_deg - 90.0;
                    cost += excess_deg.powi(2) * w.te_root_angle_penalty_scale;
                }
            }
        }

        if let Some(assessment) = transport_planform {
            let planform_penalty = match transport_planform_penalty(
                assessment,
                geometric_body_alpha_deg,
                m_fuel.max(0.0),
                mm.fuel_density_kg_m3,
                mm.fuel_tank_usable_fraction,
                &w,
            ) {
                Some(penalty) => penalty,
                None => {
                    let cost = w.failure_cost;
                    self.history.record(
                        dv,
                        false,
                        cost,
                        ld,
                        dv.span_m,
                        alpha,
                        plane.s_ref,
                        trim_ih,
                        "transport_planform",
                    );
                    return cost;
                }
            };
            cost += planform_penalty;
        }

        // Fuselage empty stretch
        let cabin_start = self.config.geometry.fuselage.cabin_start_x_m;
        let tailcone_len = self.config.geometry.fuselage.tailcone_length_m;
        let cabin_len = (dv.fuselage_length_m - cabin_start - tailcone_len).max(1.0);
        let occupied_len =
            cabin_len.min(req.payload_kg() / mm.cabin_payload_density_kg_m.max(1e-6));
        let empty_stretch = (cabin_len - occupied_len).max(0.0);
        if empty_stretch > 0.0 {
            cost += empty_stretch.powi(2) * w.fuselage_penalty_scale;
        }

        // Record history
        let mut reasons = Vec::new();
        if sm_floor_violation {
            reasons.push("static_margin");
        }
        if cg_envelope_violation {
            reasons.push("cg_envelope");
        }
        if shortfall_pct > 0.0 {
            reasons.push("payload_shortfall");
        }
        if wing_area_limit_violation {
            reasons.push("wing_area_limit");
        }
        if wing_loading_violation {
            reasons.push("wing_loading");
        }
        if transport_constraints_active
            && (geometric_body_alpha_deg < w.geometric_body_alpha_min_deg
                || geometric_body_alpha_deg > w.geometric_body_alpha_max_deg)
        {
            reasons.push("body_alpha_window");
        }
        let is_valid = reasons.is_empty();
        let history_alpha_deg = if self.reference_mass_coordinates {
            alpha
        } else {
            geometric_body_alpha_deg
        };

        self.history.record(
            dv,
            is_valid,
            cost,
            ld,
            dv.span_m,
            history_alpha_deg,
            plane.s_ref,
            trim_ih,
            reasons.join("+"),
        );

        cost
    }
}

#[cfg(test)]
#[path = "objective_tests.rs"]
mod objective_tests;
