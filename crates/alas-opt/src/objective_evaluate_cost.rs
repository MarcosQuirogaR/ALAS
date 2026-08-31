// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::objective_history::record_objective_result;
use super::super::DesignObjective;
use crate::transport_planform::{transport_planform_penalty, TransportPlanformAssessment};
use alas_config::design_variables::DesignVector;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::{tail_volume_coefficients, tail_volume_coefficients_reference_compatibility};

/// Values produced by the aerodynamic and mass passes before cost penalties are applied.
pub(super) struct ObjectiveCostInputs {
    pub(super) dv: DesignVector,
    pub(super) plane: Airplane,
    pub(super) m_fuel: f64,
    pub(super) alpha: f64,
    pub(super) trim_ih: f64,
    pub(super) ld: f64,
    pub(super) cd0: f64,
    pub(super) sm_physical: f64,
    pub(super) sm_floor_violation: bool,
    pub(super) cg_envelope_violation: bool,
    pub(super) cg_exc_max: f64,
    pub(super) transport_planform: Option<TransportPlanformAssessment>,
    pub(super) transport_constraints_active: bool,
    pub(super) geometric_body_alpha_deg: f64,
}

/// Apply the scalar objective penalties and record the candidate result.
pub(super) fn score_candidate(objective: &mut DesignObjective, inputs: ObjectiveCostInputs) -> f64 {
    let ObjectiveCostInputs {
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
    } = inputs;
    let w = objective.config.optimizer.weights.clone();
    let req = &objective.config.requirements;
    let mm = &objective.config.mass_model;
    let subjective_shape_priors_active =
        objective.reference_mass_coordinates || w.transport_shape_priors_enabled;

    // Assemble cost: primary L/D reward
    let mut cost = -w.ld_weight * ld;

    // Payload / Seating shortfall penalty
    let mut shortfall_pct = 0.0;
    if req.aircraft_type == "cargo" {
        if req.cargo_payload_kg < objective.target_cargo_payload_kg {
            shortfall_pct = (objective.target_cargo_payload_kg - req.cargo_payload_kg)
                / objective.target_cargo_payload_kg.max(1.0);
        }
    } else if req.num_passengers < objective.target_num_passengers {
        shortfall_pct = (objective.target_num_passengers - req.num_passengers) as f64
            / (objective.target_num_passengers as f64).max(1.0);
    }

    if shortfall_pct > 0.0 {
        cost += shortfall_pct.powi(2) * w.payload_shortfall_penalty_scale;
    }

    // Tail-area fractions
    let s_wing_ref = if objective.reference_mass_coordinates {
        plane.wings[0].unfolded_area().max(1.0)
    } else {
        plane.s_ref.max(1.0)
    };
    if plane.wings.len() > 1 {
        let hstab_area = if objective.reference_mass_coordinates {
            plane.wings[1].unfolded_area()
        } else {
            plane.wings[1].reference_area()
        };
        let ratio_h = hstab_area / s_wing_ref;
        if ratio_h < w.min_hstab_area_fraction {
            let deficit = (w.min_hstab_area_fraction - ratio_h) / w.min_hstab_area_fraction;
            cost += deficit.powi(2) * w.tail_area_penalty_scale;
        }
    }
    if plane.wings.len() > 2 {
        // The vertical fin is an XZ planform; XY projection would
        // collapse its surface area to zero.
        let vstab_area = plane.wings[2].unfolded_area();
        let ratio_v = vstab_area / s_wing_ref;
        if ratio_v < w.min_vstab_area_fraction {
            let deficit = (w.min_vstab_area_fraction - ratio_v) / w.min_vstab_area_fraction;
            cost += deficit.powi(2) * w.tail_area_penalty_scale;
        }
    }

    // Tail volume coefficients
    let (vh_opt, vv_opt) = if objective.reference_mass_coordinates {
        tail_volume_coefficients_reference_compatibility(&plane)
    } else {
        tail_volume_coefficients(&plane)
    };
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

        let fuselage_diameter_m = objective.config.geometry.fuselage.diameter_m.max(1e-6);
        let fineness_ratio = dv.fuselage_length_m / fuselage_diameter_m;
        if fineness_ratio > w.fineness_ratio_max {
            cost +=
                (fineness_ratio - w.fineness_ratio_max).powi(2) * w.fineness_ratio_penalty_scale;
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
    let wing_area_limit_violation = wing_area_excess && !objective.reference_mass_coordinates;
    if wing_area_excess {
        let excess_frac = (plane.s_ref - req.max_wing_area_m2) / req.max_wing_area_m2.max(1.0);
        cost += excess_frac.powi(2) * w.area_penalty_scale;
    }

    // Wing loading floor
    let ws = req.mtow_kg / plane.s_ref.max(1e-6);
    let wing_loading_below_floor = ws < req.min_wing_loading_kg_m2;
    let wing_loading_violation = wing_loading_below_floor && !objective.reference_mass_coordinates;
    if wing_loading_below_floor {
        let deficit_frac = (req.min_wing_loading_kg_m2 - ws) / req.min_wing_loading_kg_m2.max(1.0);
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

    // A positive MTOW mass-closure remainder is an allowance, not a fuel
    // requirement. Tank capacity is therefore not scored against it here.
    // Mission range and reserve-policy feasibility belong in the staged
    // mission assessment; the negative remainder above remains a mass
    // infeasibility because it means ZFM exceeds MTOW.

    if subjective_shape_priors_active {
        if dv.airfoil_thickness_scale < w.thickness_floor {
            cost += (w.thickness_floor - dv.airfoil_thickness_scale) * w.thickness_penalty_scale;
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

        let wing_g = &objective.config.geometry.wing;
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
                objective.history.record(
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
    let cabin_start = objective.config.geometry.fuselage.cabin_start_x_m;
    let tailcone_len = objective.config.geometry.fuselage.tailcone_length_m;
    let cabin_len = (dv.fuselage_length_m - cabin_start - tailcone_len).max(1.0);
    let occupied_len = cabin_len.min(req.payload_kg() / mm.cabin_payload_density_kg_m.max(1e-6));
    let empty_stretch = (cabin_len - occupied_len).max(0.0);
    if empty_stretch > 0.0 {
        cost += empty_stretch.powi(2) * w.fuselage_penalty_scale;
    }

    let history_alpha_deg = if objective.reference_mass_coordinates {
        alpha
    } else {
        geometric_body_alpha_deg
    };
    let history_span_m = dv.span_m;
    record_objective_result(
        &mut objective.history,
        dv,
        cost,
        ld,
        history_span_m,
        history_alpha_deg,
        plane.s_ref,
        trim_ih,
        sm_floor_violation,
        cg_envelope_violation,
        shortfall_pct,
        wing_area_limit_violation,
        wing_loading_violation,
        transport_constraints_active,
        geometric_body_alpha_deg,
        &w,
    )
}
