// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airworthiness performance family: engine-out second-segment climb,
//! cruise and takeoff thrust margin, landing field length and approach speed.

use crate::mdo::ResidualRole;

use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignRequirements, ObjectiveConfig, PerformanceConfig};
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, cs25_115_net_path_reduction, density_ratio,
    far25_oei_gradient, landing_runway_share, oei_cl_at_v2, tw_takeoff_constraint,
    ws_landing_field_limit, OeiClimbStatus, JET_DRY_LANDING_SHARE,
};
use alas_units::KNOT;

/// Certification climb checks beyond the second segment (CS 25.121(c),(d),
/// 25.119) and the CS 25.115 net-path diagnostic.
pub mod certification;
/// The cruise-altitude thrust requirements from the engine deck.
pub mod cruise_deck;
/// Whether the fixed engine rating still matches the closed take-off mass.
pub mod engine_match;
/// Improved-climb V2 resolution and the top-of-climb margin at the selected
/// initial cruise level.
pub mod improved_climb;
/// The shared engine-out evidence builder (deck thrust ratio, failed-engine
/// and asymmetric trim drag).
pub mod oei_deck;
pub mod planned_cruise;
/// The scatter-aware verdict of the cruise-thrust and top-of-climb checks.
pub mod thrust_uncertainty;

use certification::{certification_residuals, net_path_gradient_margin, CertificationInputs};
use oei_deck::{second_segment_evidence, EngineOutModel, YawAirframe};

use super::sizing::SizingOutcome;
use super::types::CandidateDrag;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// The engine-out climb, cruise and takeoff thrust-margin residuals, plus
/// (when the arrival aerodrome resolves) landing field length and approach
/// speed.
pub(super) fn performance_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    // The computed minimum control speed replaces the declared estimate in
    // every speed schedule below (CS 25.107(b), (e)); see `residuals_vmc`.
    let vmc = super::residuals_vmc::evaluate_candidate(outcome, config);
    let resolved = super::residuals_vmc::performance_with_vmc(config, &vmc);
    let config = &*resolved;
    let req = &config.requirements;
    let objective = &config.optimizer.objective;
    let sized = &outcome.sized;
    let available_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0
        / (sized.takeoff_mass_kg * req.gravity_m_s2);
    let s_ref = outcome.plane.s_ref.max(1e-9);

    let mut residuals = Vec::new();

    let certified_oei_gradient = far25_oei_gradient(outcome.n_engines);
    // Engine deck and airframe geometry for the engine-out evidence. A
    // failure leaves the evidence terms unset (an evidence gap), never a
    // placeholder value.
    let engine_out = EngineOutModel::new(
        config,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()
    .map(|model| model.with_weight_kg(sized.takeoff_mass_kg));
    // Improved climb: when the schedule V2 misses the second-segment gradient,
    // V2 is raised to the lowest speed that meets it (up to the best-climb
    // speed) and the longer field is charged. Every speed below reads it.
    let (improved_config, improved) = match (outcome.departure, engine_out.as_ref()) {
        (Some(departure), Some(model)) => improved_climb::resolve_improved_climb(
            config,
            &improved_climb::ImprovedClimbInputs {
                model,
                departure,
                mass_kg: sized.takeoff_mass_kg,
                s_ref_m2: s_ref,
                drag: &sized.fuel_artifacts.drag,
                installed_tw_sls: available_tw,
            },
        ),
        _ => (std::borrow::Cow::Borrowed(config), None),
    };
    let config = &*improved_config;
    let perf = &config.performance;
    if let Some(selection) = improved.filter(|selection| selection.improved) {
        residuals.push(ConstraintResidual::direct(
            "improved_climb_v2_over_vsr",
            Performance,
            selection.v2_over_vsr,
            selection.schedule_v2_over_vsr,
            "V2/VSR",
            selection.v2_over_vsr - selection.schedule_v2_over_vsr,
            0.0,
            ResidualRole::Diagnostic,
        ));
    }
    let departure_speeds = outcome.departure.map(|departure| {
        compute_v_speeds_at_masses(
            sized.takeoff_mass_kg,
            sized.takeoff_mass_kg,
            s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        )
    });
    let drag = &sized.fuel_artifacts.drag;
    let oei_evidence = outcome
        .departure
        .zip(departure_speeds)
        .map(|(departure, speeds)| {
            second_segment_evidence(
                engine_out.as_ref(),
                perf,
                departure,
                &speeds,
                &|mach, altitude_m, isa_deviation_c| {
                    drag.cd_at_atmosphere(0.0, mach, altitude_m, isa_deviation_c)
                },
            )
        });
    let oei_condition = oei_evidence.map(|evidence| evidence.condition);
    // The engine-out climb is flown at the V2 lift coefficient
    // `CL_V2 = CLmax_TO / (V2/VS)^2` (14 CFR 25.107(c): V2 >= 1.13 VSR, so
    // CL_V2 <= CLmax_TO / 1.13^2; the assessor uses the same relation,
    // `oei_cl_at_v2`), at the airport elevation and the V2 Mach there. The
    // drag table is read at exactly that point, not at the cruise design CL
    // and reference altitude. Without a resolved departure the legacy
    // `oei_climb_cl` is the lift coefficient and the field is sea level.
    // The table is clean: the takeoff flap/slat increment enters through
    // `oei_climb_delta_cd`. The projection is used only at this selected CL.
    let oei_cl = oei_condition
        .and_then(|condition| oei_cl_at_v2(perf.cl_max_to, condition.v2_over_vstall))
        .unwrap_or(perf.oei_climb_cl);
    let (oei_altitude_m, oei_isa_deviation_c) = outcome.departure.map_or((0.0, 0.0), |airport| {
        (airport.elevation_m, airport.isa_deviation_c)
    });
    let departure_atmosphere =
        alas_atmo::us1976_compute_values(oei_altitude_m, oei_isa_deviation_c);
    let oei_speed_m_s = departure_speeds.map_or_else(
        || {
            (2.0 * sized.takeoff_mass_kg * req.gravity_m_s2
                / (departure_atmosphere.density_kg_m3 * s_ref * oei_cl))
                .sqrt()
        },
        |speeds| speeds.v2_ms,
    );
    let oei_mach = oei_speed_m_s / departure_atmosphere.speed_of_sound_m_s;
    let (oei_cd0, oei_k) =
        departure_polar(drag, oei_cl, oei_mach, oei_altitude_m, oei_isa_deviation_c);
    let oei_assessment = assess_oei_climb(
        oei_cd0,
        oei_k,
        outcome.n_engines,
        certified_oei_gradient.unwrap_or(perf.oei_gradient),
        oei_cl,
        perf.oei_climb_delta_cd,
        perf.cl_max_to,
        oei_condition,
    );
    // A conceptual in-flight estimate remains useful as a soft ranking
    // signal, but only the shared assessor's SLS-equivalent result may become
    // a hard residual. In particular, unsupported engine counts never fall
    // back to a made-up Part 25 requirement.
    if let Some(required_oei_tw) = oei_assessment
        .required_sls_tw
        .or(oei_assessment.required_inflight_tw)
    {
        let oei_policy = if oei_assessment.status == OeiClimbStatus::SlsEquivalent {
            role
        } else {
            ResidualRole::Preference
        };
        // The take-off rating carries the take-off band when the requirement
        // is the SLS-equivalent one; a conceptual in-flight estimate does not.
        let band = if oei_assessment.status == OeiClimbStatus::SlsEquivalent {
            thrust_uncertainty::takeoff_thrust_scatter(config)
        } else {
            0.0
        };
        residuals.extend(thrust_uncertainty::takeoff_banded_rows(
            "oei_second_segment",
            "oei_second_segment_within_takeoff_band",
            "CS 25.121(b) second segment (static T/W)",
            available_tw,
            required_oei_tw,
            band,
            oei_policy,
        ));
    }
    // CS 25.115(b): the net path reduces the gross gradient by 0.8/0.9/1.0 %.
    // Without an obstacle model this is a Diagnostic margin, not a constraint.
    if let (Some(inflight_tw), Some(ratio), Some(gradient), Some(reduction)) = (
        oei_assessment.required_inflight_tw,
        oei_condition.and_then(|condition| condition.condition_to_sls_thrust_ratio),
        certified_oei_gradient,
        cs25_115_net_path_reduction(outcome.n_engines),
    ) {
        if let Some(margin) = net_path_gradient_margin(
            outcome.n_engines,
            gradient,
            inflight_tw,
            available_tw,
            ratio,
        ) {
            residuals.push(ConstraintResidual::direct(
                "takeoff_net_path_gradient",
                Performance,
                margin,
                0.0,
                "gradient",
                -margin,
                (-margin / reduction).max(0.0),
                ResidualRole::Diagnostic,
            ));
        }
    }
    if let Some(model) = engine_out.as_ref() {
        residuals.extend(certification_residuals(
            &CertificationInputs {
                config,
                drag,
                engine_out: model,
                s_ref_m2: s_ref,
                takeoff_mass_kg: sized.takeoff_mass_kg,
                landing_mass_kg: sized.design_landing_mass_kg,
                departure: outcome.departure,
                arrival: outcome.arrival,
            },
            available_tw,
            role,
        ));
    }
    match oei_assessment.status {
        OeiClimbStatus::NotApplicable | OeiClimbStatus::SlsEquivalent => {}
        OeiClimbStatus::UnsupportedEngineCount => residuals.push(ConstraintResidual::direct(
            "oei_part25_engine_count_unsupported",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ResidualRole::Diagnostic,
        )),
        OeiClimbStatus::ConceptualInflight | OeiClimbStatus::EvidenceGap => {
            residuals.push(ConstraintResidual::direct(
                "oei_second_segment_evidence_gap",
                Performance,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                ResidualRole::Diagnostic,
            ));
        }
    }

    // Diagnostic only: a fixed engine rating that no longer matches the
    // closed take-off mass (outside the configured band) is a warning, not a
    // constraint.
    if let Some(matched) = engine_match::engine_mass_match(config, sized.takeoff_mass_kg) {
        residuals.push(ConstraintResidual::direct(
            "engine_mass_match",
            Performance,
            matched.ratio,
            1.0,
            "ratio",
            matched.ratio - 1.0,
            matched.normalized_excess(),
            ResidualRole::Diagnostic,
        ));
    }

    residuals.extend(cruise_deck::toc_rows(config, &sized.fuel_artifacts, role));
    residuals.extend(cruise_deck::cruise_rows(
        engine_out.as_ref(),
        drag,
        config,
        (sized.takeoff_mass_kg, sized.mid_cruise_mass_kg()),
        (s_ref, available_tw),
        &sized.fuel_artifacts,
        role,
    ));
    if !outcome.airport_records_resolved {
        residuals.push(ConstraintResidual::direct(
            "airport_unknown",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    } else if !outcome.declared_airport_data_complete {
        residuals.push(ConstraintResidual::direct(
            "airport_declared_distance_unavailable",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    }

    if !outcome.mission_distance_known {
        residuals.push(ConstraintResidual::direct(
            "mission_distance_unavailable",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        ));
    }

    if outcome.minimum_profile_range_m.is_finite() && outcome.minimum_profile_range_m > 0.0 {
        residuals.push(ConstraintResidual::scaled(
            "mission_profile_range",
            Performance,
            sized.design_range_m,
            outcome.minimum_profile_range_m,
            "m",
            outcome.minimum_profile_range_m - sized.design_range_m,
            role,
        ));
    }

    // The field lengths a runway has to offer are certified at the maximum
    // masses, not at the mass a short city pair happens to dispatch: takeoff
    // at the design gross mass (MTOW) and landing at the design landing mass
    // (MLW). Both are the sizing basis's own values
    // (`alas_config::MassSizingBasis`): the declared weights of a fixed
    // aircraft, the closure of a coupled design.
    if let Some(departure) = outcome.departure {
        residuals.push(takeoff_field_residual(
            sized.design_gross_mass_kg,
            outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0,
            s_ref,
            req.gravity_m_s2,
            departure,
            // An improved-climb V2 lengthens the field (same T/W correlation, lower
            // effective CLmax).
            perf.takeoff_field_cl_max(perf.cl_max_to),
            role,
        ));
    }

    if let Some(arrival) = outcome.arrival {
        residuals.extend(landing_and_approach_residuals(
            outcome, req, perf, objective, arrival, s_ref, role,
        ));
    }

    residuals.extend(super::residuals_vmc::vmc_residuals(&vmc, role));
    residuals
}

/// The takeoff field-length residual at `mass_kg`, as a thrust-to-weight
/// requirement.
///
/// Raymer's empirical takeoff relation (`alas_perf::performance::
/// tw_takeoff_constraint`) gives the required `T/W` from the wing loading at
/// `mass_kg`; the available `T/W` is the installed static thrust over the same
/// weight. Both sides move with the mass, so a field that a light dispatch
/// clears can be missed at the maximum takeoff mass.
fn takeoff_field_residual(
    mass_kg: f64,
    static_thrust_n: f64,
    s_ref: f64,
    gravity_m_s2: f64,
    departure: &Airport,
    cl_max_to: f64,
    role: ResidualRole,
) -> ConstraintResidual {
    let weight_n = mass_kg * gravity_m_s2;
    let available_tw = static_thrust_n / weight_n;
    let sigma = density_ratio(departure.elevation_m, departure.isa_deviation_c);
    let required_tw =
        tw_takeoff_constraint(&[weight_n / s_ref], departure.toda_m, sigma, cl_max_to)[0];
    ConstraintResidual::scaled(
        "takeoff_field",
        Performance,
        available_tw,
        required_tw,
        "T/W",
        required_tw - available_tw,
        role,
    )
}

/// Single-point clean-drag projection for the engine-out assessor. Both
/// scoring branches evaluate this selected CL; `(CD_at_CL, 0)` preserves
/// the drag with its actual atmospheric Reynolds number exactly.
fn departure_polar(
    drag: &CandidateDrag,
    cl: f64,
    mach: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> (f64, f64) {
    (
        drag.cd_airport_base_at_atmosphere(cl, mach, altitude_m, isa_deviation_c),
        0.0,
    )
}

/// The landing field-length residual, plus the approach-speed residual when
/// a limit is configured. Both are evaluated at the design landing mass
/// (MLW), the mass the landing distance and approach speed are certified at.
#[allow(clippy::too_many_arguments)] // one named physical input per residual; a struct would only rename them once
fn landing_and_approach_residuals(
    outcome: &SizingOutcome,
    req: &DesignRequirements,
    perf: &PerformanceConfig,
    objective: &ObjectiveConfig,
    arrival: &Airport,
    s_ref: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let sized = &outcome.sized;
    let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
    // Same definition as the delivery gate and the matching chart: the actual
    // distance k_land (W/S)/(sigma CLmax) may use 0.6 of the LDA.
    let ws_land_limit_pa = ws_landing_field_limit(
        arrival.lda_m,
        landing_runway_share(JET_DRY_LANDING_SHARE, perf.wet_landing_runway),
        sigma,
        perf.cl_max_land,
        perf.k_land,
    );
    let landing_ws_pa = sized.design_landing_mass_kg * req.gravity_m_s2 / s_ref;

    let mut residuals = vec![ConstraintResidual::scaled(
        "landing_field",
        Performance,
        landing_ws_pa,
        ws_land_limit_pa,
        "Pa",
        landing_ws_pa - ws_land_limit_pa,
        role,
    )];

    if objective.max_approach_speed_kt > 0.0 {
        let v_speeds = compute_v_speeds_at_masses(
            sized.design_gross_mass_kg,
            sized.design_landing_mass_kg,
            s_ref,
            arrival,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let v_app_kt = v_speeds.v_app_ms / KNOT;
        residuals.push(ConstraintResidual::scaled(
            "approach_speed",
            Performance,
            v_app_kt,
            objective.max_approach_speed_kt,
            "kt",
            v_app_kt - objective.max_approach_speed_kt,
            role,
        ));
    }
    residuals
}

#[cfg(test)]
mod deck_altitude_probe;
#[cfg(test)]
mod deck_takeoff_probe;
#[cfg(test)]
mod gradient_probe;
#[cfg(test)]
mod tests;
