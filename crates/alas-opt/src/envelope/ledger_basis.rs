// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shared per-state assessment body behind every envelope entry point,
//! and the registered-MRW lookup it sizes the gear with.
//!
//! Private items of `envelope.rs` (e.g.
//! `operational_loading_states_with_z`, `assess_model_cg_envelope_from_states`)
//! stay visible here because Rust privacy extends to descendant modules.

use alas_config::{AlasConfig, MacFrame};
use alas_geom::aircraft::airplane::Airplane;

use super::rotation::{
    pitch_radius_of_gyration_m, rotation_pitch_acceleration_deg_s2, rotation_tail_lift,
};
use super::rotation_thrust::{rotation_lift_coefficient, RotationThrustModel};
use super::support::{
    horizontal_tail_ac_x_m, tail_area_ratio, tail_ground_effect_factor, ROTATION_CM_AC_WB_TAKEOFF,
};
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

/// The minimum static nose-gear load fraction at `mass_kg`, the state's own
/// weight: the registered preset's published aft-CG nose share against weight
/// ([`alas_config::PublishedAftCgNoseLoad::minimum_nose_gear_fraction`]) when
/// it has one, else the class default `mass_model.pct_load_nlg_min`. A
/// clean-sheet preset has no published split and keeps the class default.
pub(super) fn minimum_nose_gear_fraction(
    config: &AlasConfig,
    published: Option<alas_config::PublishedAftCgNoseLoad>,
    mass_kg: f64,
) -> f64 {
    let class_minimum = config.mass_model.pct_load_nlg_min;
    published.map_or(class_minimum, |split| {
        split.minimum_nose_gear_fraction(class_minimum, mass_kg)
    })
}

/// The registered preset's published aft-CG gear-load split, `None` for a
/// clean-sheet or unregistered preset.
pub(super) fn registered_aft_cg_nose_load(
    config: &AlasConfig,
) -> Option<alas_config::PublishedAftCgNoseLoad> {
    alas_config::presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.aft_cg_nose_load)
}

/// Shared body of `super::assess_model_cg_envelope` and
/// [`super::assess_model_cg_envelope_with_ledger_and_landing`]: everything
/// downstream of the named `(state, cg_x, cg_z, mass_kg)` tuples, whichever
/// basis built them. The list must contain an
/// [`ModelCgLoadingState::AnalyzedTakeoff`] state and may carry an
/// [`ModelCgLoadingState::AnalyzedLanding`] state.
///
/// `takeoff_pitch_inertia_kg_m2` is the mass ledger's pitch moment of
/// inertia about the analyzed-takeoff centre of gravity, kg m^2; `NaN` when
/// no ledger exists, which selects Raymer's radius of gyration
/// (`rotation::pitch_radius_of_gyration_m`). Every state uses the takeoff
/// radius of gyration with its own mass; only the takeoff state is gated by
/// rotation, so the other states' rotation boundaries are diagnostics.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_model_cg_envelope_from_states(
    plane: &Airplane,
    states: Vec<(ModelCgLoadingState, f64, f64, f64)>,
    takeoff_pitch_inertia_kg_m2: f64,
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

    let x_wing_ac = wing.aerodynamic_center(0.25)[0];
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
    let scrape_points = fuselage_lower_points_aft_of(fuselage, x_main_gear);
    let scrape_angle_deg = alas_perf::landing_gear::geometry::tail_scrape_angle_deg(
        &scrape_points,
        x_main_gear,
        ground_z_m,
    );
    let required_tip_back_deg = config.landing_gear.min_tip_back_deg.max(
        scrape_angle_deg
            .filter(|angle| angle.is_finite())
            .unwrap_or(0.0),
    );
    let fuselage_diameter_m = config.geometry.fuselage.diameter_m;
    // Size at the governing design state (most-aft of the aero limit
    // and the analyzed takeoff) rather than the aero limit alone.
    let aerodynamic_aft_limit_pct_mac =
        to_pct_mac(critical_x_np) - 100.0 * config.requirements.min_physical_static_margin;
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
    let tail_ground_effect_factor_val = tail_ground_effect_factor(plane, ground_z_m);
    let cl_ground_attitude =
        config.performance.cl_max_to * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
    let rotation_tail_lift_coefficient =
        rotation_tail_lift(plane, config, ground_z_m, cl_ground_attitude)
            .map_or(f64::NAN, |tail| tail.lift_coefficient);
    let pitch_radius_of_gyration_m = pitch_radius_of_gyration_m(
        config.landing_gear.pitch_radius_of_gyration_frac_mac,
        mac,
        takeoff_pitch_inertia_kg_m2,
        mtow_mass_kg,
        plane.b_ref,
        fuselage_length_m,
    );
    let cl_r_rotation = rotation_lift_coefficient(&config.performance);
    let thrust_model = RotationThrustModel::new(plane, config, ground_z_m, cl_r_rotation);
    let takeoff_thrust = thrust_model.at_mass(mtow_mass_kg);
    let published_nose_load = registered_aft_cg_nose_load(config);
    let forward_mechanism_input = PhysicalCgLimitsInput {
        mac_frame,
        critical_np_x_m: critical_x_np,
        clean_np_x_m: x_np,
        min_physical_static_margin: config.requirements.min_physical_static_margin,
        x_main_gear_m: x_main_gear,
        x_mlg_aft_axle_m: x_main_gear,
        wheelbase_m,
        // The analyzed takeoff state's height and thrust, so the forward
        // limit used for gear sizing is the takeoff rotation boundary.
        h_cg_m: mtow_h_cg_m,
        min_tip_back_deg: config.landing_gear.min_tip_back_deg,
        scrape_angle_deg: None,
        pct_load_nlg_min: minimum_nose_gear_fraction(config, published_nose_load, mtow_mass_kg),
        pct_load_nlg_max_handling: config.mass_model.pct_load_nlg_max_handling,
        x_ac_wb_frac: SCISSOR_X_AC_WB_FRAC,
        cm_ac_wb_landing: SCISSOR_CM_AC_WB_LANDING,
        cl_max_landing: config.performance.cl_max_land,
        eta: SCISSOR_ETA,
        tail_volume_coefficient: tail_volume_coefficient(plane, s_ref, mac),
        cl_h_max: SCISSOR_CL_H_MAX,
        cg_range_pct_mac: config.requirements.cg_range_pct_mac,
        x_h_ac_m,
        tail_area_ratio: tail_area_ratio_val,
        tail_ground_effect_factor: tail_ground_effect_factor_val,
        rotation_tail_lift_coefficient,
        cl_ground_attitude,
        cl_r_rotation,
        cm_ac_wb_takeoff: ROTATION_CM_AC_WB_TAKEOFF,
        pitch_radius_of_gyration_m,
        rotation_angular_accel_deg_s2: rotation_pitch_acceleration_deg_s2(config),
        gravity_m_s2: config.requirements.gravity_m_s2,
        rotation_thrust_to_weight: takeoff_thrust.thrust_to_weight,
        rotation_thrust_line_height_m: takeoff_thrust.thrust_line_height_m,
        rotation_rolling_friction_coefficient: config
            .landing_gear
            .rotation_rolling_friction_coefficient,
    };
    let forward_limit_pct_mac = physical_cg_limits(&forward_mechanism_input).fwd_limit_pct_mac;
    let physical_forward_x_m = x_mac_le + forward_limit_pct_mac / 100.0 * mac;
    let mrw_kg = registered_mrw_kg(config);
    let gear = size_landing_gear_at_design_state(
        mtow_mass_kg,
        mrw_kg,
        x_nose_gear,
        x_main_gear,
        physical_forward_x_m,
        physical_aft_x_m,
        Some(mtow_state_x_m),
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
        let min_nose_fraction = minimum_nose_gear_fraction(config, published_nose_load, mass_kg);
        let unscoped_limits = physical_cg_limits(&PhysicalCgLimitsInput {
            x_mlg_aft_axle_m: gear.x_mlg_aft_axle_m,
            h_cg_m,
            scrape_angle_deg,
            pct_load_nlg_min: min_nose_fraction,
            rotation_thrust_to_weight: state_thrust.thrust_to_weight,
            rotation_thrust_line_height_m: state_thrust.thrust_line_height_m,
            ..forward_mechanism_input
        });
        let physical_limits = unscoped_limits.scoped(PhaseLimits::for_state(state));
        let tip_back_angle_deg = alas_perf::landing_gear::geometry::tip_back_angle_deg(
            gear.x_mlg_aft_axle_m,
            state_cg_x,
            h_cg_m,
        );
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
            mass_kg,
            cg_x_m: state_cg_x,
            cg_pct_mac: to_pct_mac(state_cg_x),
            static_margin: (x_np - state_cg_x) / mac,
            critical_static_margin: (critical_x_np - state_cg_x) / mac,
            static_margin_floor: config.requirements.min_physical_static_margin,
            physical_limits,
            nose_gear_load_kg,
            nose_gear_capacity_kg,
            main_gear_load_kg,
            main_gear_capacity_kg,
            capacity_basis_declared: gear.capacity_basis_declared,
            minimum_nose_gear_load_fraction: min_nose_fraction,
            maximum_nose_gear_load_fraction: config.mass_model.pct_load_nlg_max_handling,
            tip_back_angle_deg,
            required_tip_back_deg,
            scrape_angle_deg,
            required_rotation_angle_deg: config.landing_gear.required_rotation_angle_deg,
            cg_range_pct_mac: config.requirements.cg_range_pct_mac,
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
        minimum_physical_static_margin: config.requirements.min_physical_static_margin,
        aerodynamic_aft_limit_pct_mac: top_level_limits.aerodynamic_aft_pct_mac,
        clean_np_pct_mac: top_level_limits.clean_np_pct_mac,
        configured_forward_limit_pct_mac: top_level_limits.fwd_limit_pct_mac,
        max_nose_load_fwd_limit_pct_mac: top_level_limits.max_nose_load_fwd_pct_mac,
        scissor_plot_fwd_limit_pct_mac: top_level_limits.scissor_plot_fwd_pct_mac,
        main_gear_station_pct_mac,
        ground_aft_limit_pct_mac,
        aft_limit_governance: worst_aft_limit_governance,
        worst_aft_limit_pct_mac,
        capacity_basis_declared: gear.capacity_basis_declared,
        tire_overloaded: gear.tire_overloaded,
        required_tip_back_deg,
        scrape_angle_deg,
        target_static_margin: StaticMarginPreferenceAssessment {
            actual: mtow_static_margin,
            target: config.requirements.target_static_margin,
            deviation: mtow_static_margin - config.requirements.target_static_margin,
        },
    })
}

// A test asserts on values it constructed here, so a failed assertion is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::LedgerLoadingBasis;
    use super::{minimum_nose_gear_fraction, registered_aft_cg_nose_load};

    /// Every published point that prints its most-aft CG must land the
    /// model ground aft boundary, at that mass, on that CG. The tolerance is
    /// the model-frame residual: the model MAC and LEMAC are fitted, not the
    /// manufacturer's (A320 36.8 [S] -> 36.9 [M], A340 38.0 -> 38.4, A380
    /// 43.0 -> 42.0).
    #[test]
    fn published_aft_cg_gear_splits_return_the_tabulated_aft_cg() {
        use alas_config::{presets, AlasConfig};
        use alas_geom::builder::AircraftBuilder;
        let mut checked = 0;
        for name in presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let Some(split) = registered_aft_cg_nose_load(&config) else {
                continue;
            };
            let registered = presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let frame = plane.mac_frame().expect("a main wing");
            let fuselage = &plane.fuselages[0];
            let start = fuselage.xsecs[0].xyz_c[0];
            let length = fuselage.xsecs[fuselage.xsecs.len() - 1].xyz_c[0] - start;
            let stations = config.landing_gear.resolved_station_positions(
                start + length * config.mass_model.nlg_x_fraction,
                frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
                start,
                length,
            );
            let wheelbase_m = stations.x_mlg_m - stations.x_nlg_m;
            for point in split.points {
                let Some(published_pct_mac) = point.aft_cg_pct_mac else {
                    continue;
                };
                let fraction = minimum_nose_gear_fraction(&config, Some(split), point.mass_kg);
                let aft_x_m = stations.x_mlg_m - fraction * wheelbase_m;
                let model_pct_mac = frame.pct_mac(aft_x_m);
                assert!(
                    (model_pct_mac - published_pct_mac).abs() < 1.5,
                    "{name}: model ground aft {model_pct_mac:.2} % MAC against the published \
                     {published_pct_mac} % MAC at {} kg",
                    point.mass_kg
                );
                checked += 1;
            }
        }
        assert!(checked >= 18, "published points checked: {checked}");
    }

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
