// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shared per-state assessment body behind every envelope entry point,
//! and the registered-MRW lookup it sizes the gear with.
//!
//! Private items of `envelope.rs` (e.g.
//! `operational_loading_states_with_z`, `assess_on_one_ground_plane`)
//! stay visible here because Rust privacy extends to descendant modules.

use alas_config::{AlasConfig, MacFrame};
use alas_geom::aircraft::airplane::Airplane;

use super::rotation::{
    pitch_radius_of_gyration_m, rotation_pitch_acceleration_deg_s2, rotation_tail_lift,
};
use super::rotation_thrust::{
    rotation_lift_coefficient, rotation_lift_coefficient_at, rotation_speed_m_s,
    rotation_vmc_floor_m_s, RotationThrustModel,
};
use super::support::{
    horizontal_tail_ac_x_m, landing_cm_ac_wb, tail_area_ratio, tail_efficiency,
};
use super::ths_trim::takeoff_trim_model;
use super::*;

/// The registered preset's maximum ramp weight (CS 25.733 /
/// Currey: MRW, not MTOW, governs the gear design load). `None` for a
/// clean-sheet or unregistered-MRW preset, the ordinary "size at MTOW
/// alone" case.
pub(super) fn registered_mrw_kg(config: &AlasConfig) -> Option<f64> {
    alas_config::presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.mrw_kg)
}

/// The registered preset's published stabiliser ice protection, `None` for a
/// clean-sheet or unregistered preset (checked as unprotected).
pub(super) fn registered_tail_ice_protection(
    config: &AlasConfig,
) -> Option<alas_config::TailIceProtection> {
    alas_config::presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.horizontal_tail_ice_protection)
        .map(|sourced| sourced.protection)
}

/// The MLW that scopes each state and sets the landing-trim speeds, kg: the
/// mode-aware [`AlasConfig::landing_mass_limit_kg`] the mass model uses, at
/// the candidate MTOW (the analysed take-off mass only if that is unusable),
/// so a lighter loading keeps the aircraft's MLW and landing speeds; its clip
/// to the candidate only bites on an MLW above the MTOW, which no landing
/// reaches.
pub(super) fn maximum_landing_mass_kg(config: &AlasConfig, takeoff_mass_kg: f64) -> f64 {
    let candidate_kg = Some(config.requirements.mtow_kg).filter(|m| m.is_finite() && *m > 0.0);
    config.landing_mass_limit_kg(candidate_kg.unwrap_or(takeoff_mass_kg))
}

/// The stability margin kept between the critical neutral point and the aft
/// limit, as a fraction of the MAC: Torenbeek's minimum for an inherently
/// stable transport, `0.02 l_h` (*Advanced Aircraft Design*, Wiley, 2013,
/// sec. 6.3.3, p. 168: "Minimum stability margin: Delta x_sm = 0.02 l_h"),
/// `l_h` the tail arm from the wing-body aerodynamic centre, and never less
/// than the configured floor (`requirements.min_physical_static_margin`).
/// Without a tail station the configured floor alone.
pub(super) fn minimum_stability_margin(configured: f64, tail_arm_m: f64, mac_m: f64) -> f64 {
    if tail_arm_m.is_finite() && tail_arm_m > 0.0 && mac_m > 0.0 {
        configured.max(0.02 * tail_arm_m / mac_m)
    } else {
        configured
    }
}

/// Shared body of `super::assess_model_cg_envelope` and
/// [`super::assess_model_cg_envelope_with_ledger_and_landing`]: everything
/// downstream of the named `(state, cg_x, cg_z, mass_kg)` tuples, whichever
/// basis built them. The list must contain an
/// [`ModelCgLoadingState::AnalyzedTakeoff`] state and may carry an
/// [`ModelCgLoadingState::AnalyzedLanding`] state.
///
/// `pitch_inertia` is the mass ledger whose tensors give each state's own
/// pitch moment of inertia about its own centre of gravity
/// ([`super::LedgerLoadingBasis::pitch_inertia_about_kg_m2`]); `None` when
/// no ledger exists, which selects Raymer's radius of gyration
/// (`rotation::pitch_radius_of_gyration_m`). Each state's rotation balance
/// takes its own radius of gyration, so a state's line does not depend on
/// which basis built the list; only the takeoff state is gated by rotation,
/// so the other states' rotation boundaries are diagnostics.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_on_one_ground_plane(
    plane: &Airplane,
    states: Vec<(ModelCgLoadingState, f64, f64, f64)>,
    pitch_inertia: Option<&super::LedgerLoadingBasis>,
    cg_x: f64,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let wing = plane
        .wings
        .first()
        .ok_or(ModelCgEnvelopeError::MissingMainWing)?;
    let fuselage = plane
        .fuselages
        .first()
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;
    let fus_start_x = fuselage
        .xsecs
        .first()
        .map(|section| section.xyz_c[0])
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;
    let fus_end_x = fuselage
        .xsecs
        .last()
        .map(|section| section.xyz_c[0])
        .ok_or(ModelCgEnvelopeError::MissingFuselage)?;

    let values_are_finite = [
        cg_x,
        x_np,
        critical_x_np,
        mac,
        fus_start_x,
        fus_end_x,
        config.requirements.min_physical_static_margin,
        config.requirements.target_static_margin,
        config.requirements.cg_range_pct_mac,
        config.mass_model.pct_load_nlg_min,
        config.mass_model.pct_load_nlg_max_handling,
    ]
    .into_iter()
    .all(f64::is_finite)
        && states
            .iter()
            .all(|(_, state_cg_x, _, mass_kg)| state_cg_x.is_finite() && mass_kg.is_finite());
    if !values_are_finite
        || mac <= 0.0
        || states.iter().any(|(_, _, _, mass_kg)| *mass_kg <= 0.0)
        || fus_end_x <= fus_start_x
    {
        return Err(ModelCgEnvelopeError::InvalidInput);
    }

    let x_wing_ac = wing.carried_box().aerodynamic_center(0.25)[0];
    let x_mac_le = plane
        .mac_frame()
        .ok_or(ModelCgEnvelopeError::MissingMainWing)?
        .x_lemac_m;
    // One declared MAC reference in the primary aircraft body frame (x aft
    // from the fuselage nose tip, metres; percent MAC dimensionless), used by
    // every percent-MAC number this function reports so a station and a limit
    // cannot end up referred to different chords.
    let mac_frame = MacFrame::new(x_mac_le, mac).ok_or(ModelCgEnvelopeError::InvalidInput)?;
    let to_pct_mac = move |x_m: f64| mac_frame.pct_mac(x_m).unwrap_or(f64::NAN);

    let fuselage_length_m = fus_end_x - fus_start_x;
    let fallback_x_nose_gear = fus_start_x + fuselage_length_m * config.mass_model.nlg_x_fraction;
    let fallback_x_main_gear = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac;
    let gear_stations = config.landing_gear.resolved_station_positions(
        fallback_x_nose_gear,
        fallback_x_main_gear,
        fus_start_x,
        fuselage_length_m,
    );
    // `fallback_x_main_gear` above is the wing-mounted rule, admissible only
    // on the layouts `alas_mass::stations` states it for. Refuse the whole
    // assessment when this aircraft is outside that domain and registers no
    // station anchor: the wheelbase, both gear-strength capacities and
    // `min_nose_gear_load` below are all moments about that station, so a
    // refused station cannot be allowed to produce reported reactions.
    if let Some(error) = unmeasured_main_gear_station(plane, config, &gear_stations) {
        return Err(ModelCgEnvelopeError::MainGearStationNotMeasured(error));
    }
    let x_nose_gear = gear_stations.x_nlg_m;
    let x_main_gear = gear_stations.x_mlg_m;
    // Tail scrape, tip-back and lift-off pitch about the aft-most main station.
    let pivot_x_m = gear_stations.rotation_pivot_x_m();
    let wheelbase_m = x_main_gear - x_nose_gear;
    if !x_wing_ac.is_finite()
        || !x_mac_le.is_finite()
        || !x_nose_gear.is_finite()
        || !x_main_gear.is_finite()
        || wheelbase_m <= 0.0
    {
        return Err(ModelCgEnvelopeError::InvalidInput);
    }

    // The shared ground plane and each state's loaded CG height above
    // it, replacing the `1.1 * fuselage_diameter` placeholder with the mass
    // model's own per-state vertical CG.
    let ground_z_m = ground_z_m(fuselage, config);
    // The analyzed takeoff state, found by name: the state list may carry a
    // landing state after it.
    let (_, mtow_state_x_m, mtow_state_z_m, mtow_mass_kg) = states
        .iter()
        .copied()
        .find(|(state, _, _, _)| *state == ModelCgLoadingState::AnalyzedTakeoff)
        .ok_or(ModelCgEnvelopeError::InvalidInput)?;
    let mtow_h_cg_m = mtow_state_z_m - ground_z_m;
    let scrape_points = lower_contour_aft_of(plane, fuselage, pivot_x_m);
    let scrape_angle_deg = alas_perf::landing_gear::geometry::tail_scrape_angle_deg(
        &scrape_points,
        pivot_x_m,
        ground_z_m,
    );
    // The tail-scrape gate compares like with like: a gear-extended (or
    // state-unspecified) tail-strike attitude is met by the contour on the
    // ground plane lowered by the lift-off oleo extension, a static one by the
    // static plane. Tip-back keeps the static angle.
    let rotation_basis = config.landing_gear.required_rotation_basis;
    let scrape_gate_deg = if rotation_basis.gear_extended() {
        alas_perf::landing_gear::geometry::tail_scrape_angle_deg(
            &scrape_points,
            x_main_gear,
            ground_z_m - super::liftoff_oleo_extension_m(),
        )
    } else {
        scrape_angle_deg
    };
    let required_tip_back_deg = alas_perf::landing_gear::geometry::required_tip_back_deg(
        config.landing_gear.min_tip_back_deg,
        scrape_angle_deg,
    );
    let fuselage_diameter_m = config.geometry.fuselage.diameter_m;
    // Size at the governing design state (most-aft of the aero limit
    // and the analyzed takeoff) rather than the aero limit alone.
    let static_margin_floor = minimum_stability_margin(
        config.requirements.min_physical_static_margin,
        horizontal_tail_ac_x_m(plane)
            - (x_mac_le + super::support::wing_body_ac_frac(plane, config) * mac),
        mac,
    );
    let aerodynamic_aft_limit_pct_mac = to_pct_mac(critical_x_np) - 100.0 * static_margin_floor;
    let physical_aft_x_m = x_mac_le + aerodynamic_aft_limit_pct_mac / 100.0 * mac;
    let s_ref = wing.reference_area();
    // The envelope-wide forward limit (gear sizing and the top-level
    // report) is evaluated once here at the analyzed takeoff state: the
    // landing-trim and max-nose-load mechanisms are state-independent, and
    // the rotation one depends on the state only through its thrust-to-weight
    // and CG height. This call's aft outputs are discarded; every per-state
    // call below recomputes both sides with that state's own height and
    // thrust.
    let x_h_ac_m = horizontal_tail_ac_x_m(plane);
    let tail_area_ratio_val = tail_area_ratio(plane, s_ref);
    // The landing-trim balance and the iced-tail check are candidates of every
    // state and of the envelope-wide limit, so they are evaluated once, at the
    // maximum landing mass (the speeds they are set by). The same MLW gates
    // the phase scoping of each state (`PhaseLimits::for_state_at_mass`): an
    // en-route state above it cannot land and drops landing trim. A landing
    // state's presence moves no envelope-wide value and no other state's
    // verdict.
    let design_landing_mass_kg = maximum_landing_mass_kg(config, mtow_mass_kg);
    let landing_trim_cl_h = super::iced_tail::landing_trim_cl_h(
        plane,
        config,
        s_ref,
        x_mac_le + super::support::wing_body_ac_frac(plane, config) * mac,
        x_h_ac_m,
        design_landing_mass_kg,
        super::iced_tail::IceAccretion::of(registered_tail_ice_protection(config)),
    );
    let landing_authority =
        super::landing_download::landing_authority(plane, config, s_ref, design_landing_mass_kg);
    let cl_ground_attitude = super::support::takeoff_ground_lift(plane, config, s_ref);
    let cl_r_rotation = rotation_lift_coefficient(&config.performance);
    // Each state's own `V_R`: the stall branch, floored by 1.05 of the computed
    // minimum control speed when the gate's configuration carries one.
    let vr_floor_m_s = rotation_vmc_floor_m_s(config);
    let cl_r_at = |mass_kg: f64| {
        rotation_lift_coefficient_at(
            mass_kg * config.requirements.gravity_m_s2,
            s_ref,
            cl_r_rotation,
            vr_floor_m_s,
        )
    };
    // The analysed takeoff state's rotation speed sets the Mach and Reynolds
    // numbers of the tail's stall limit.
    let rotation_speed_m_s = rotation_speed_m_s(
        mtow_mass_kg * config.requirements.gravity_m_s2,
        s_ref,
        cl_r_at(mtow_mass_kg),
    );
    let rotation_tail = rotation_tail_lift(
        plane,
        config,
        ground_z_m,
        cl_ground_attitude,
        rotation_speed_m_s,
    );
    let rotation_tail_lift_coefficient =
        rotation_tail.map_or(f64::NAN, |tail| tail.lift_coefficient);
    // A reversible elevator holds only what the pilot's pull and the trim tab
    // allow (`super::elevator_force`): at rotation the two-hand pull at the
    // state's own `q_R = W / (S CL_R)`, at landing the trimmed approach and the
    // one-hand flare at the maximum landing mass, like the rest of the landing
    // balance.
    let gravity = config.requirements.gravity_m_s2;
    let eta_tail = tail_efficiency(plane);
    let reversible_elevator =
        super::elevator_force::reversible_elevator(plane, config, rotation_speed_m_s);
    let rotation_cl_h_at = |mass_kg: f64| -> f64 {
        match (reversible_elevator, rotation_tail) {
            (Some(elevator), Some(tail)) => {
                let q_h = eta_tail * mass_kg * gravity / (s_ref * cl_r_at(mass_kg));
                let alpha_h = tail.incidence_rad - tail.downwash_rad;
                let held = tail.lift_curve_slope_per_rad
                    * (alpha_h
                        + elevator.elevator_term_rad(
                            q_h,
                            alpha_h,
                            super::elevator_force::ROTATION_PULL_FORCE_N,
                        ));
                held.max(-tail.section_cap).max(rotation_tail_lift_coefficient)
            }
            _ => rotation_tail_lift_coefficient,
        }
    };
    let landing_authority = match (landing_authority, reversible_elevator) {
        (Some(authority), Some(elevator)) => Some(authority.with_reversible_elevator(
            &elevator,
            design_landing_mass_kg * gravity,
            s_ref,
            eta_tail,
        )),
        (authority, _) => authority,
    };
    let thrust_model = RotationThrustModel::new(plane, config, ground_z_m, cl_r_rotation);
    let takeoff_thrust = thrust_model.at_mass(mtow_mass_kg);
    let takeoff_trim = takeoff_trim_model(
        plane,
        config,
        rotation_tail,
        s_ref,
        mac,
        x_mac_le,
        x_h_ac_m,
        tail_area_ratio_val,
        cl_ground_attitude,
        takeoff_thrust.thrust_to_weight,
        mtow_h_cg_m - takeoff_thrust.thrust_line_height_m,
    );
    let radius_of_gyration_at = |mass_kg: f64, cg_x_m: f64, cg_z_m: f64| {
        pitch_radius_of_gyration_m(
            config.landing_gear.pitch_radius_of_gyration_frac_mac,
            mac,
            pitch_inertia.map_or(f64::NAN, |ledger| {
                ledger.pitch_inertia_about_kg_m2(mass_kg, cg_x_m, cg_z_m)
            }),
            mass_kg,
            plane.b_ref,
            fuselage_length_m,
        )
    };
    let pitch_radius_of_gyration_m =
        radius_of_gyration_at(mtow_mass_kg, mtow_state_x_m, mtow_state_z_m);
    let forward_mechanism_input = PhysicalCgLimitsInput {
        mac_frame,
        critical_np_x_m: critical_x_np,
        clean_np_x_m: x_np,
        min_physical_static_margin: static_margin_floor,
        x_main_gear_m: x_main_gear,
        x_mlg_aft_axle_m: pivot_x_m,
        wheelbase_m,
        // The analyzed takeoff state's height and thrust, so the forward
        // limit used for gear sizing is the takeoff rotation boundary.
        h_cg_m: mtow_h_cg_m,
        min_tip_back_deg: config.landing_gear.min_tip_back_deg,
        scrape_angle_deg: None,
        pct_load_nlg_min: config.mass_model.pct_load_nlg_min,
        pct_load_nlg_max_handling: super::support::nose_handling_fraction(config, mtow_mass_kg),
        x_ac_wb_frac: super::support::wing_body_ac_frac(plane, config),
        cm_ac_wb_landing: landing_cm_ac_wb(plane, config, s_ref),
        cl_max_landing: config.performance.cl_max_land,
        eta: tail_efficiency(plane),
        tail_volume_coefficient: tail_volume_coefficient(
            plane,
            s_ref,
            mac,
            x_mac_le + super::support::wing_body_ac_frac(plane, config) * mac,
        ),
        cl_h_max: SCISSOR_CL_H_MAX,
        landing_authority,
        cg_range_pct_mac: config.requirements.cg_range_pct_mac,
        x_h_ac_m,
        tail_area_ratio: tail_area_ratio_val,
        landing_trim_cl_h,
        rotation_tail_lift_coefficient: rotation_cl_h_at(mtow_mass_kg),
        cl_ground_attitude,
        cl_r_rotation: cl_r_at(mtow_mass_kg),
        cm_ac_wb_takeoff: super::support::takeoff_cm_ac_wb(plane, config, s_ref),
        pitch_radius_of_gyration_m,
        rotation_angular_accel_deg_s2: rotation_pitch_acceleration_deg_s2(config),
        gravity_m_s2: config.requirements.gravity_m_s2,
        rotation_thrust_to_weight: takeoff_thrust.thrust_to_weight,
        rotation_thrust_line_height_m: takeoff_thrust.thrust_line_height_m,
        rotation_rolling_friction_coefficient: config
            .landing_gear
            .rotation_rolling_friction_coefficient,
        takeoff_trim,
        clean_trim: super::clean_trim::clean_trim_model(
            plane,
            config,
            s_ref,
            mac,
            x_mac_le,
            x_h_ac_m,
            tail_area_ratio_val,
            landing_authority,
            rotation_tail,
        ),
    };
    let forward_limit_pct_mac = physical_cg_limits(&forward_mechanism_input).fwd_limit_pct_mac;
    let physical_forward_x_m = x_mac_le + forward_limit_pct_mac / 100.0 * mac;
    let mrw_kg = registered_mrw_kg(config);
    let gear_design_weight_kg = mrw_kg.map_or(mtow_mass_kg, |mrw| mrw.max(mtow_mass_kg));
    let gear = size_landing_gear_at_design_state(
        mtow_mass_kg,
        mrw_kg,
        x_nose_gear,
        x_main_gear,
        physical_forward_x_m,
        physical_aft_x_m,
        Some(mtow_state_x_m),
        // The gear is sized at its design weight with the class minimum; the
        // per-state ground limit below reads the same value, so sizing and
        // envelope agree.
        config.mass_model.pct_load_nlg_min,
        alas_perf::landing_gear::geometry::TipBackBasis {
            most_aft_ground_cg_x_m: None,
            scrape_angle_deg,
            cg_height_m: None,
        },
        fuselage_diameter_m,
        mtow_h_cg_m,
        &gear_stations.main_gear_x_m,
        &config.landing_gear,
    );
    let nose_gear_capacity_kg = mtow_mass_kg * gear.pct_load_nlg_max;
    let main_gear_capacity_kg = mtow_mass_kg * gear.pct_load_mlg_max;

    let mut loading_assessments = Vec::with_capacity(states.len());
    let mut worst_aft_limit_pct_mac = f64::INFINITY;
    let mut worst_aft_limit_governance = AftCgLimitGovernance::NotEvaluated;
    let mut top_level_limits: Option<PhysicalCgLimits> = None;
    let usable_range = super::usable_range::UsableRangeRequirement::of(config);
    for (state, state_cg_x, state_cg_z, mass_kg) in states {
        let nose_gear_load_kg = mass_kg * (x_main_gear - state_cg_x) / wheelbase_m;
        let main_gear_load_kg = mass_kg - nose_gear_load_kg;
        let h_cg_m = state_cg_z - ground_z_m;
        // Reuses `forward_mechanism_input`'s state-independent fields
        // verbatim; only the per-state fields (gear axle, this state's own CG
        // height and rotation thrust, tail-scrape) are overridden.
        let state_thrust = if state == ModelCgLoadingState::AnalyzedTakeoff {
            takeoff_thrust
        } else {
            thrust_model.at_mass(mass_kg)
        };
        let min_nose_fraction = config.mass_model.pct_load_nlg_min;
        let unscoped_limits = physical_cg_limits(&PhysicalCgLimitsInput {
            x_mlg_aft_axle_m: pivot_x_m,
            h_cg_m,
            scrape_angle_deg,
            pct_load_nlg_min: min_nose_fraction,
            pct_load_nlg_max_handling: super::support::nose_handling_fraction(config, mass_kg),
            rotation_thrust_to_weight: state_thrust.thrust_to_weight,
            rotation_thrust_line_height_m: state_thrust.thrust_line_height_m,
            rotation_tail_lift_coefficient: rotation_cl_h_at(mass_kg),
            cl_r_rotation: cl_r_at(mass_kg),
            pitch_radius_of_gyration_m: radius_of_gyration_at(mass_kg, state_cg_x, state_cg_z),
            ..forward_mechanism_input
        });
        let scope = PhaseLimits::for_state_at_mass(state, mass_kg, design_landing_mass_kg);
        let physical_limits = unscoped_limits.scoped(scope);
        let tip_back_angle_deg =
            alas_perf::landing_gear::geometry::tip_back_angle_deg(pivot_x_m, state_cg_x, h_cg_m);
        if physical_limits.aft_limit_pct_mac.is_finite()
            && physical_limits.aft_limit_pct_mac < worst_aft_limit_pct_mac
        {
            worst_aft_limit_pct_mac = physical_limits.aft_limit_pct_mac;
            worst_aft_limit_governance = match physical_limits.aft_limit_governance {
                AftLimitGovernance::Aerodynamic => AftCgLimitGovernance::Aerodynamic,
                AftLimitGovernance::GroundMinimumNoseLoad => {
                    AftCgLimitGovernance::GroundMinimumNoseLoad {
                        margin_pct_mac: (physical_limits.aerodynamic_aft_pct_mac
                            - physical_limits.aft_limit_pct_mac)
                            .max(0.0),
                    }
                }
                AftLimitGovernance::TipBack => AftCgLimitGovernance::TipBack {
                    margin_pct_mac: (physical_limits
                        .aerodynamic_aft_pct_mac
                        .min(physical_limits.ground_aft_pct_mac)
                        - physical_limits.aft_limit_pct_mac)
                        .max(0.0),
                },
            };
        }
        if state == ModelCgLoadingState::AnalyzedTakeoff {
            // Envelope-wide top-level values keep every mechanism.
            top_level_limits = Some(unscoped_limits);
        }
        let assessment = assess_loading_constraints(LoadingConstraintInputs {
            state,
            scope,
            mass_kg,
            cg_x_m: state_cg_x,
            h_cg_m,
            cg_pct_mac: to_pct_mac(state_cg_x),
            static_margin: (x_np - state_cg_x) / mac,
            critical_static_margin: (critical_x_np - state_cg_x) / mac,
            static_margin_floor,
            physical_limits,
            nose_gear_load_kg,
            nose_gear_capacity_kg,
            main_gear_load_kg,
            main_gear_capacity_kg,
            capacity_basis_declared: gear.capacity_basis_declared,
            minimum_nose_gear_load_fraction: min_nose_fraction,
            maximum_nose_gear_load_fraction: super::support::nose_handling_fraction(
                config, mass_kg,
            ),
            tip_back_angle_deg,
            required_tip_back_deg,
            scrape_angle_deg: scrape_gate_deg,
            required_rotation_angle_deg: config.landing_gear.required_rotation_angle_deg,
            tail_scrape_rejects: rotation_basis.rejects(),
            cg_range_pct_mac: usable_range.pct_of_model_mac(mass_kg, mac),
        });
        loading_assessments.push(assessment);
    }
    let mtow_static_margin = loading_assessments
        .iter()
        .find(|assessment| assessment.state == ModelCgLoadingState::AnalyzedTakeoff)
        .map(|assessment| assessment.static_margin)
        .ok_or(ModelCgEnvelopeError::InvalidInput)?;
    let top_level_limits = top_level_limits.ok_or(ModelCgEnvelopeError::InvalidInput)?;

    let ground_aft_limit_pct_mac = top_level_limits.ground_aft_pct_mac;
    let main_gear_station_pct_mac = to_pct_mac(x_main_gear);
    if !worst_aft_limit_pct_mac.is_finite() {
        worst_aft_limit_pct_mac = f64::NAN;
    }

    Ok(ModelCgEnvelopeAssessment {
        loading_states: loading_assessments,
        design_landing_mass_kg,
        mac_m: mac,
        minimum_physical_static_margin: static_margin_floor,
        aerodynamic_aft_limit_pct_mac: top_level_limits.aerodynamic_aft_pct_mac,
        clean_np_pct_mac: top_level_limits.clean_np_pct_mac,
        configured_forward_limit_pct_mac: top_level_limits.fwd_limit_pct_mac,
        max_nose_load_fwd_limit_pct_mac: top_level_limits.max_nose_load_fwd_pct_mac,
        landing_trim_fwd_limit_pct_mac: top_level_limits.landing_trim_fwd_pct_mac,
        scissor_plot_fwd_limit_pct_mac: top_level_limits.scissor_plot_fwd_pct_mac,
        main_gear_station_pct_mac,
        ground_aft_limit_pct_mac,
        aft_limit_governance: worst_aft_limit_governance,
        worst_aft_limit_pct_mac,
        capacity_basis_declared: gear.capacity_basis_declared,
        tire_overloaded: gear.tire_overloaded,
        gear_design_weight_kg,
        gear_design_main_reaction_kg: gear.r_mlg_total_design_kg,
        gear_aero_aft_limit_pulled_back: gear.aero_aft_limit_pulled_back,
        gear_raw_aft_nose_reaction_kg: gear.r_nlg_aft_raw_kg,
        gear_raw_aft_cg_is_takeoff_state: mtow_state_x_m > physical_aft_x_m,
        required_tip_back_deg,
        scrape_angle_deg,
        target_static_margin: StaticMarginPreferenceAssessment {
            actual: mtow_static_margin,
            target: config.requirements.target_static_margin,
            deviation: mtow_static_margin - config.requirements.target_static_margin,
        },
        takeoff_trim,
    })
}

// A test asserts on values it constructed here, so a failed assertion is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::LedgerLoadingBasis;

    /// Feeding `payload_and_fuel`'s back-solved deltas into the same
    /// mass-weighted mixing the shared loading-state builder uses must
    /// reproduce the ledger's own ZFW and TOW mass/CG exactly (both are
    /// linear in mass and moment, so the subtraction and remixing round
    /// trip to machine precision).
    #[test]
    fn payload_and_fuel_round_trips_to_the_ledgers_own_zfw_and_tow_points() {
        let basis = LedgerLoadingBasis {
            oew_mass_kg: 40_000.0,
            oew_cg_x_m: 17.0,
            oew_cg_z_m: 1.0,
            zero_fuel_mass_kg: 55_000.0,
            zero_fuel_cg_x_m: 17.5,
            zero_fuel_cg_z_m: 1.1,
            takeoff_mass_kg: 70_000.0,
            takeoff_cg_x_m: 17.8,
            takeoff_cg_z_m: 1.2,
            takeoff_pitch_inertia_kg_m2: f64::NAN,
            oew_pitch_inertia_kg_m2: f64::NAN,
            zero_fuel_pitch_inertia_kg_m2: f64::NAN,
        };
        let (payload_mass, payload_cg_x, payload_cg_z, fuel_mass, fuel_cg_x, fuel_cg_z) =
            basis.payload_and_fuel();

        let zfw_mass = basis.oew_mass_kg + payload_mass;
        let zfw_cg_x =
            (basis.oew_mass_kg * basis.oew_cg_x_m + payload_mass * payload_cg_x) / zfw_mass;
        let zfw_cg_z =
            (basis.oew_mass_kg * basis.oew_cg_z_m + payload_mass * payload_cg_z) / zfw_mass;
        assert!((zfw_mass - basis.zero_fuel_mass_kg).abs() < 1.0e-9);
        assert!((zfw_cg_x - basis.zero_fuel_cg_x_m).abs() < 1.0e-9);
        assert!((zfw_cg_z - basis.zero_fuel_cg_z_m).abs() < 1.0e-9);

        let tow_mass = zfw_mass + fuel_mass;
        let tow_cg_x = (zfw_mass * zfw_cg_x + fuel_mass * fuel_cg_x) / tow_mass;
        let tow_cg_z = (zfw_mass * zfw_cg_z + fuel_mass * fuel_cg_z) / tow_mass;
        assert!((tow_mass - basis.takeoff_mass_kg).abs() < 1.0e-9);
        assert!((tow_cg_x - basis.takeoff_cg_x_m).abs() < 1.0e-9);
        assert!((tow_cg_z - basis.takeoff_cg_z_m).abs() < 1.0e-9);
    }

    /// A ledger with equal OEW and ZFW mass (no payload) must not divide by
    /// zero: the payload CG falls back to the OEW point.
    #[test]
    fn a_zero_payload_delta_falls_back_to_the_oew_point_without_dividing_by_zero() {
        let basis = LedgerLoadingBasis {
            oew_mass_kg: 40_000.0,
            oew_cg_x_m: 17.0,
            oew_cg_z_m: 1.0,
            zero_fuel_mass_kg: 40_000.0,
            zero_fuel_cg_x_m: 17.0,
            zero_fuel_cg_z_m: 1.0,
            takeoff_mass_kg: 40_000.0,
            takeoff_cg_x_m: 17.0,
            takeoff_cg_z_m: 1.0,
            takeoff_pitch_inertia_kg_m2: f64::NAN,
            oew_pitch_inertia_kg_m2: f64::NAN,
            zero_fuel_pitch_inertia_kg_m2: f64::NAN,
        };
        let (payload_mass, payload_cg_x, payload_cg_z, fuel_mass, fuel_cg_x, fuel_cg_z) =
            basis.payload_and_fuel();
        assert_eq!(payload_mass, 0.0);
        assert_eq!(payload_cg_x, basis.oew_cg_x_m);
        assert_eq!(payload_cg_z, basis.oew_cg_z_m);
        assert_eq!(fuel_mass, 0.0);
        assert_eq!(fuel_cg_x, basis.zero_fuel_cg_x_m);
        assert_eq!(fuel_cg_z, basis.zero_fuel_cg_z_m);
    }
}
