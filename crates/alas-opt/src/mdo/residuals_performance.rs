// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airworthiness performance family: engine-out second-segment climb,
//! cruise and takeoff thrust margin, landing field length and approach speed.

use crate::mdo::ResidualRole;

use alas_config::airports::Airport;
use alas_config::{AlasConfig, DesignRequirements, ObjectiveConfig, PerformanceConfig};
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, dry_landing_share_for, far25_oei_gradient,
    landing_runway_share, oei_cl_at_v2, ws_landing_limit_for, OeiClimbStatus,
};
use alas_units::KNOT;

/// Certification climb checks beyond the second segment (CS 25.121(c),(d),
/// 25.119) and the CS 25.115 net-path margin.
pub mod certification;
/// The cruise-altitude thrust requirements from the engine deck.
pub mod cruise_deck;
/// The en-route OEI obstacle rows beyond the end of the take-off path (CAT.POL.A.215).
pub mod en_route_rows;
/// Whether the fixed engine rating still matches the closed take-off mass.
pub mod engine_match;
/// Improved-climb V2 resolution and the top-of-climb margin at the selected
/// initial cruise level.
pub mod improved_climb;
/// The initial cruise altitude requirement and the achievable-ICA report.
pub mod initial_cruise;
/// The net take-off flight path and its obstacle clearance.
pub mod net_path;
/// The shared engine-out evidence builder (deck thrust ratio, failed-engine
/// and asymmetric trim drag).
pub mod oei_deck;
pub mod planned_cruise;
/// The CS 25.113 jet take-off field length shared by the constraint and the reports.
pub mod takeoff_field;
/// The scatter-aware verdict of the cruise-thrust and top-of-climb checks.
pub mod thrust_uncertainty;
/// The take-off weight the departure aerodrome allows (WAT, climb and obstacle limit).
pub mod wat_limit;

use certification::{certification_residuals, CertificationInputs};
use oei_deck::{second_segment_evidence, EngineOutModel, YawAirframe};
use takeoff_field::{field_on_planes, ResidualInputs};

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// The performance residuals of a candidate, with the climb and obstacle
/// checks on the sea-level ISA certification basis and the take-off weight the
/// departure aerodrome allows reported beside them (see [`wat_limit`]).
pub(super) fn performance_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    wat_limit::performance_residuals_with_limit(outcome, config, role)
}

/// The engine-out climb, cruise and takeoff thrust-margin residuals, plus
/// (when the arrival aerodrome resolves) landing field length and approach
/// speed.
pub(super) fn performance_residuals_at(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
    survey_elevation_m: Option<f64>,
) -> Vec<ConstraintResidual> {
    // The computed minimum control speed replaces the declared estimate in
    // every speed schedule below (CS 25.107(b), (e)); see `residuals_vmc`.
    let vmc = super::residuals_vmc::evaluate_candidate(outcome, config);
    let resolved = super::residuals_vmc::performance_with_vmc(config, &vmc);
    let config = &*resolved;
    // Before the improved-climb resolution below: the field row re-resolves it
    // at its own mass.
    let base_config = config;
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
                aspect_ratio: net_path::climb_out_aspect_ratio(
                    config,
                    outcome.plane.b_ref,
                    outcome.plane.s_ref,
                ),
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
    // `oei_cl_at_v2`). Its drag is the V2 climb-out locus of the take-off
    // configuration (`net_path::V2ClimbOutPolar`), which contains the flaps
    // and, where the source states it, the failed engine. Without a resolved
    // departure the legacy `oei_climb_cl` is the lift coefficient.
    let oei_cl = oei_condition
        .and_then(|condition| oei_cl_at_v2(perf.cl_max_to, condition.v2_over_vstall))
        .unwrap_or(perf.oei_climb_cl);
    let oei_assessment = net_path::V2ClimbOutPolar::of(
        config,
        net_path::climb_out_aspect_ratio(config, outcome.plane.b_ref, outcome.plane.s_ref),
    )
    .assess(
        outcome.n_engines,
        certified_oei_gradient.unwrap_or(perf.oei_gradient),
        oei_cl,
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
    let certification_inputs = engine_out.as_ref().map(|model| CertificationInputs {
        config,
        drag,
        engine_out: model,
        s_ref_m2: s_ref,
        takeoff_mass_kg: sized.takeoff_mass_kg,
        landing_mass_kg: sized.design_landing_mass_kg,
        departure: outcome.departure,
        arrival: outcome.arrival,
    });
    if let Some(inputs) = certification_inputs.as_ref() {
        residuals.extend(certification_residuals(inputs, available_tw, role));
    }
    match oei_assessment.status {
        OeiClimbStatus::NotApplicable | OeiClimbStatus::SlsEquivalent => {}
        // CS 25.121(b) states a gradient for two, three and four engines only,
        // so a configuration outside that schedule has no demonstrated
        // certification basis: a failed check, not a skipped one.
        OeiClimbStatus::UnsupportedEngineCount => residuals.push(ConstraintResidual::direct(
            "oei_part25_engine_count_unsupported",
            Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
        )),
        OeiClimbStatus::ConceptualInflight | OeiClimbStatus::EvidenceGap => {
            // An engine-out climb that cannot be evaluated (no departure
            // condition, deck or fin evidence) is not demonstrated: a failed
            // check, never a silently skipped one.
            residuals.push(ConstraintResidual::direct(
                "oei_second_segment_evidence_gap",
                Performance,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                role,
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
    residuals.extend(initial_cruise::initial_cruise_rows(
        config,
        &sized.fuel_artifacts,
        sized.takeoff_mass_kg,
        role,
    ));
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
    let mut screen = None;
    if let Some(departure) = outcome.departure {
        // The field is certified at the design gross mass, so its speeds are the
        // schedule at that mass (improved-climb V2 resolved there), not the
        // dispatch mass the second-segment rows above use.
        let design_gap = (sized.design_gross_mass_kg - sized.takeoff_mass_kg).abs();
        let field_speeds = if design_gap > 1e-6 * sized.design_gross_mass_kg {
            takeoff_field::design_mass_speeds(
                base_config,
                outcome,
                departure,
                s_ref,
                req.gravity_m_s2,
            )
            .or(departure_speeds)
        } else {
            departure_speeds
        };
        let inputs = ResidualInputs {
            perf,
            engine_out: engine_out.as_ref(),
            drag: &sized.fuel_artifacts.drag,
            departure,
            speeds: field_speeds,
            s_ref_m2: s_ref,
            mass_kg: sized.design_gross_mass_kg,
            static_thrust_n: outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0,
            gravity_m_s2: req.gravity_m_s2,
            role,
            climb_out: Some(net_path::V2ClimbOutPolar::of(
                config,
                net_path::climb_out_aspect_ratio(config, outcome.plane.b_ref, outcome.plane.s_ref),
            )),
        };
        let (field_row, screen_point) = field_on_planes(&inputs, &outcome.plane, config);
        residuals.push(field_row);
        screen = screen_point;
    }
    // CS 25.115(b) net path gradient guard and CAT.POL.A.210 obstacle clearance.
    residuals.extend(net_path::net_path_residuals(
        outcome,
        &net_path::NetPathTerms {
            perf,
            drag,
            departure: outcome.departure,
            required_gradient: certified_oei_gradient,
            required_inflight_tw: oei_assessment.required_inflight_tw,
            thrust_ratio: oei_condition
                .and_then(|condition| condition.condition_to_sls_thrust_ratio),
            available_tw,
            speeds: departure_speeds,
            takeoff_distance_m: screen.map(|point| point.distance_m),
            airborne_to_screen_s: screen.and_then(|point| point.airborne_time_s),
            final_segment_net_gradient: certification_inputs
                .as_ref()
                .and_then(|inputs| certification::final_segment_net_gradient(inputs, available_tw)),
            survey_elevation_m,
            departure_track: req.departure_track.as_deref(),
            en_route_role: if config.optimizer.design_space.mode
                == alas_config::DesignMode::BaselineSandbox
            {
                ResidualRole::Diagnostic
            } else {
                role
            },
        },
        role,
    ));

    if let Some(arrival) = outcome.arrival {
        // The landing model of the field stage: a turboprop outside the translated
        // replay flies the propeller segments and their runway share.
        let propeller = !perf.legacy_field_correlations
            && matches!(
                config.geometry.engine.active_model(),
                Ok(alas_config::ActiveEngineModel::Turboprop(_))
            );
        residuals.extend(landing_and_approach_residuals(
            outcome, req, perf, propeller, objective, arrival, s_ref, role,
        ));
    }

    residuals.extend(super::residuals_vmc::vmc_residuals(&vmc, role));
    residuals
}

/// The landing field-length residual, plus the approach-speed residual when
/// a limit is configured. Both are evaluated at the design landing mass
/// (MLW), the mass the landing distance and approach speed are certified at.
#[allow(clippy::too_many_arguments)] // one named physical input per residual; a struct would only rename them once
fn landing_and_approach_residuals(
    outcome: &SizingOutcome,
    req: &DesignRequirements,
    perf: &PerformanceConfig,
    propeller: bool,
    objective: &ObjectiveConfig,
    arrival: &Airport,
    s_ref: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let sized = &outcome.sized;
    let sigma = density_ratio(arrival.elevation_m, arrival.isa_deviation_c);
    // Same definition as the delivery gate and the matching chart: the actual
    // distance of the landing segment model may use the dry share of the LDA
    // (0.6 jets, the configured 0.7 turboprops).
    let ws_land_limit_pa = ws_landing_limit_for(
        perf,
        propeller,
        arrival.lda_m,
        landing_runway_share(
            dry_landing_share_for(perf, propeller),
            perf.wet_landing_runway,
        ),
        sigma,
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
