// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ledger-derived loading-state basis and the registered-MRW lookup, both
//! consumed by `assess_model_cg_envelope_from_states`'s callers.
//!
//! Private items of `envelope.rs` (e.g.
//! `operational_loading_states_with_z`, `assess_model_cg_envelope_from_states`)
//! stay visible here because Rust privacy extends to descendant modules.

use alas_config::{AlasConfig, MacFrame};
use alas_geom::aircraft::airplane::Airplane;

use super::support::{
    horizontal_tail_ac_x_m, tail_area_ratio, tail_ground_effect_factor, ROTATION_CM_AC_WB_TAKEOFF,
};
use super::*;

/// The OEW/analyzed-ZFW/analyzed-TOW points the item-level mass ledger
/// placed, in the shape [`super::assess_model_cg_envelope_with_ledger`]
/// needs to anchor the same five named states the shared loading-state
/// builder produces. Mid-mission/reserve are not carried here: the ledger
/// has no named state for them (see [`Self::payload_and_fuel`]'s doc
/// comment).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgerLoadingBasis {
    /// Ledger operating-empty mass, kg.
    pub oew_mass_kg: f64,
    /// Ledger operating-empty longitudinal CG, m.
    pub oew_cg_x_m: f64,
    /// Ledger operating-empty vertical CG, m.
    pub oew_cg_z_m: f64,
    /// Ledger zero-fuel mass, kg.
    pub zero_fuel_mass_kg: f64,
    /// Ledger zero-fuel longitudinal CG, m.
    pub zero_fuel_cg_x_m: f64,
    /// Ledger zero-fuel vertical CG, m.
    pub zero_fuel_cg_z_m: f64,
    /// Ledger analyzed-takeoff mass, kg.
    pub takeoff_mass_kg: f64,
    /// Ledger analyzed-takeoff longitudinal CG, m.
    pub takeoff_cg_x_m: f64,
    /// Ledger analyzed-takeoff vertical CG, m.
    pub takeoff_cg_z_m: f64,
}

impl LedgerLoadingBasis {
    /// Back-solve the payload and fuel mass/CG deltas the shared
    /// operational-loading-state builder needs from this basis's three
    /// ledger points, so feeding them back in reproduces the ledger's own
    /// OEW/ZFW/TOW points exactly (mass and moment both close over the
    /// subtraction by construction) while still deriving mid-mission/
    /// reserve the same way the lumped path always did: the ledger itself
    /// carries no named mid-mission/reserve state, only the four
    /// `LoadState` points (`OperatingEmpty`, `ZeroFuel`, `Takeoff`,
    /// `Landing`), and `Landing` is not one of this envelope's five named
    /// states.
    ///
    /// Returns `(payload_mass_kg, payload_cg_x_m, payload_cg_z_m,
    /// fuel_mass_kg, fuel_cg_x_m, fuel_cg_z_m)`.
    #[must_use]
    pub fn payload_and_fuel(self) -> (f64, f64, f64, f64, f64, f64) {
        let payload_mass = (self.zero_fuel_mass_kg - self.oew_mass_kg).max(0.0);
        let (payload_cg_x, payload_cg_z) = if payload_mass > 0.0 {
            (
                (self.zero_fuel_mass_kg * self.zero_fuel_cg_x_m
                    - self.oew_mass_kg * self.oew_cg_x_m)
                    / payload_mass,
                (self.zero_fuel_mass_kg * self.zero_fuel_cg_z_m
                    - self.oew_mass_kg * self.oew_cg_z_m)
                    / payload_mass,
            )
        } else {
            (self.oew_cg_x_m, self.oew_cg_z_m)
        };
        let fuel_mass = (self.takeoff_mass_kg - self.zero_fuel_mass_kg).max(0.0);
        let (fuel_cg_x, fuel_cg_z) = if fuel_mass > 0.0 {
            (
                (self.takeoff_mass_kg * self.takeoff_cg_x_m
                    - self.zero_fuel_mass_kg * self.zero_fuel_cg_x_m)
                    / fuel_mass,
                (self.takeoff_mass_kg * self.takeoff_cg_z_m
                    - self.zero_fuel_mass_kg * self.zero_fuel_cg_z_m)
                    / fuel_mass,
            )
        } else {
            (self.zero_fuel_cg_x_m, self.zero_fuel_cg_z_m)
        };
        (
            payload_mass,
            payload_cg_x,
            payload_cg_z,
            fuel_mass,
            fuel_cg_x,
            fuel_cg_z,
        )
    }
}

/// The registered preset's maximum ramp weight (CS 25.733 /
/// Currey: MRW, not MTOW, governs the gear design load). `None` for a
/// clean-sheet or unregistered-MRW preset, the ordinary "size at MTOW
/// alone" case.
pub(super) fn registered_mrw_kg(config: &AlasConfig) -> Option<f64> {
    alas_config::presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.mrw_kg)
}

/// The same hard gate as `super::assess_model_cg_envelope`,
/// but its OEW/ZFW/TOW states come verbatim from the item-level mass
/// ledger (`ledger`) instead of the lumped model's centroids -- which
/// removes the `MassModelDisagreement` disagreement. Mid-mission/reserve stay
/// the same linear fuel-fraction mix, anchored to the ledger's own
/// endpoints instead of the lumped ones (the ledger carries no named
/// state for them). The lumped path remains the fallback with no ledger.
pub fn assess_model_cg_envelope_with_ledger(
    plane: &Airplane,
    ledger: LedgerLoadingBasis,
    x_np: f64,
    critical_x_np: f64,
    mac: f64,
    config: &AlasConfig,
) -> Result<ModelCgEnvelopeAssessment, ModelCgEnvelopeError> {
    let (payload_mass, payload_cg_x, payload_cg_z, fuel_mass, fuel_cg_x, fuel_cg_z) =
        ledger.payload_and_fuel();
    let states = super::operational_loading_states_with_z(
        ledger.oew_mass_kg,
        ledger.oew_cg_x_m,
        ledger.oew_cg_z_m,
        payload_mass,
        payload_cg_x,
        payload_cg_z,
        fuel_mass,
        fuel_cg_x,
        fuel_cg_z,
        ledger.takeoff_cg_x_m,
    );
    assess_model_cg_envelope_from_states(
        plane,
        states,
        ledger.takeoff_cg_x_m,
        x_np,
        critical_x_np,
        mac,
        config,
    )
}

/// Shared body of `super::assess_model_cg_envelope` and
/// [`assess_model_cg_envelope_with_ledger`]: everything downstream of the
/// five named `(state, cg_x, cg_z, mass_kg)` tuples, whichever basis built
/// them.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_model_cg_envelope_from_states(
    plane: &Airplane,
    states: Vec<(ModelCgLoadingState, f64, f64, f64)>,
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
    let mtow_mass_kg = states
        .last()
        .map(|(_, _, _, mass_kg)| *mass_kg)
        .unwrap_or_default();
    let mtow_state_x_m = states
        .last()
        .map(|(_, state_cg_x, _, _)| *state_cg_x)
        .unwrap_or(cg_x);
    let mtow_h_cg_m = states
        .last()
        .map(|(_, _, state_cg_z, _)| *state_cg_z - ground_z_m)
        .unwrap_or(f64::NAN);
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
    // The rotation/landing-trim/max-nose-load forward mechanisms are
    // all state-independent (no per-loading-state mass or CG dependence),
    // so they are computed once here from the shared physical helper rather
    // than duplicated as a standalone scissor-only estimate. `h_cg_m: NAN`
    // only disables this throwaway call's tip-back/aft outputs, which are
    // discarded; every per-state call below recomputes the aft side with
    // this state's own `h_cg_m` and reuses these same forward inputs.
    let x_h_ac_m = horizontal_tail_ac_x_m(plane);
    let tail_area_ratio_val = tail_area_ratio(plane, s_ref);
    let tail_ground_effect_factor_val = tail_ground_effect_factor(plane, ground_z_m);
    let cl_ground_attitude =
        config.performance.cl_max_to * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
    let cl_r_rotation =
        config.performance.cl_max_to / config.performance.vr_vstall_factor.powi(2).max(1.0e-6);
    let forward_mechanism_input = PhysicalCgLimitsInput {
        mac_frame,
        critical_np_x_m: critical_x_np,
        clean_np_x_m: x_np,
        min_physical_static_margin: config.requirements.min_physical_static_margin,
        x_main_gear_m: x_main_gear,
        x_mlg_aft_axle_m: x_main_gear,
        wheelbase_m,
        h_cg_m: f64::NAN,
        min_tip_back_deg: config.landing_gear.min_tip_back_deg,
        scrape_angle_deg: None,
        pct_load_nlg_min: config.mass_model.pct_load_nlg_min,
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
        cl_ground_attitude,
        cl_r_rotation,
        cm_ac_wb_takeoff: ROTATION_CM_AC_WB_TAKEOFF,
        pitch_radius_of_gyration_frac_mac: config.landing_gear.pitch_radius_of_gyration_frac_mac,
        rotation_angular_accel_deg_s2: config.landing_gear.rotation_pitch_acceleration_deg_s2,
        gravity_m_s2: config.requirements.gravity_m_s2,
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
        // Reuses `forward_mechanism_input`'s state-independent forward
        // fields verbatim; only the aft-side/per-state fields (gear axle,
        // this state's own CG height, tail-scrape) are overridden.
        let physical_limits = physical_cg_limits(&PhysicalCgLimitsInput {
            x_mlg_aft_axle_m: gear.x_mlg_aft_axle_m,
            h_cg_m,
            scrape_angle_deg,
            ..forward_mechanism_input
        });
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
            top_level_limits = Some(physical_limits);
        }
        let mut assessment = assess_loading_constraints(LoadingConstraintInputs {
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
            minimum_nose_gear_load_fraction: config.mass_model.pct_load_nlg_min,
            maximum_nose_gear_load_fraction: config.mass_model.pct_load_nlg_max_handling,
            tip_back_angle_deg,
            required_tip_back_deg,
            scrape_angle_deg,
            required_rotation_angle_deg: config.landing_gear.required_rotation_angle_deg,
            cg_range_pct_mac: config.requirements.cg_range_pct_mac,
        });
        if !is_flight_eligible_state(state) {
            // This model's applicability contract treats bare OEW as a
            // ground-only reference condition, not a loaded flight/dispatch
            // case: keep only the ground-reaction constraints and drop the
            // flight CG-range/static-margin verdicts that do not apply to it.
            assessment
                .constraints
                .retain(|constraint| ground_reaction_constraint(constraint.constraint));
        }
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
    use super::super::assess_model_cg_envelope;
    use super::LedgerLoadingBasis;
    use alas_config::AlasConfig;
    use alas_geom::builder::AircraftBuilder;
    use alas_mass::breakdown::run_mass_analysis;

    /// The governing forward limit (most aft of rotation, landing
    /// trim, max nose load) against each manufacturer's own figure,
    /// converted to that manufacturer's MAC frame (LEMAC m aft of nose, MAC
    /// m): A380-800 34.65-37.8 %MAC at MRW (Airbus AC section 7 pavement
    /// loads; LEMAC 28.765 m, MAC 12.295 m from two-point statics), A220-300
    /// 12.0-18.6 %MAC (registered planning envelope; LEMAC 16.535 m, MAC
    /// 3.781 m), A320-200 17 %MAC most-forward CG (Airbus AC Fig.
    /// 7-3-0-991-010; LEMAC 15.26 m, MAC 4.1935 m). Tolerance 3 %MAC past
    /// the published band, except the A320 at 7 %MAC: its conceptual
    /// rotation estimate sits about 6.5 %MAC aft of the published value, a
    /// known residual of the conceptual rotation estimate.
    #[test]
    // Printing the compared values is deliberate: they are quoted in the report.
    #[allow(
        clippy::print_stdout,
        reason = "prints the compared values for the report"
    )]
    fn the_governing_forward_limit_matches_published_forward_limits() {
        let cases: [(&str, f64, f64, f64, f64, f64); 3] = [
            ("A380-800", 28.765, 12.295, 34.65, 37.8, 3.0),
            ("A220-300", 16.535, 3.781, 12.0, 18.6, 3.0),
            ("A320-200", 15.26, 4.1935, 17.0, 17.0, 7.0),
        ];
        for (preset, lemac_m, mac_m, lo, hi, tolerance) in cases {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
                .unwrap_or_else(|error| panic!("{preset}: {error}"));
            let registered = alas_config::presets::get(preset)
                .unwrap_or_else(|error| panic!("{preset}: {error}"));
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .unwrap_or_else(|error| panic!("{preset}: {error}"));
            let (masses, coordinates, cg) = run_mass_analysis(
                &plane,
                &config.requirements,
                &config.geometry,
                Some(&config.mass_model),
                None,
            );
            let x_np = cg[0] + 0.10 * plane.c_ref;
            let assessment = assess_model_cg_envelope(
                &plane,
                &masses,
                &coordinates,
                cg[0],
                x_np,
                x_np,
                plane.c_ref,
                &config,
            )
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
            let frame = plane
                .mac_frame()
                .unwrap_or_else(|| panic!("{preset}: no MAC frame"));
            let x_fwd_m = frame.x_at_pct(assessment.configured_forward_limit_pct_mac);
            let published_frame_pct = (x_fwd_m - lemac_m) / mac_m * 100.0;
            println!(
                "{preset}: governing forward limit {:.2} %MAC model frame = {:.2} %MAC manufacturer frame (published {lo}-{hi})",
                assessment.configured_forward_limit_pct_mac, published_frame_pct
            );
            assert!(
                published_frame_pct >= lo - tolerance && published_frame_pct <= hi + tolerance,
                "{preset}: {published_frame_pct:.2} %MAC outside published {lo}-{hi} +- {tolerance}"
            );
        }
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
