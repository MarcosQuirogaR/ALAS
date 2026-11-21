// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `characteristics` block of the design database: the datasheet
//! quantities of a delivered design that the run's other blocks do not state
//! as numbers.
//!
//! Nothing here is a new model. Each entry calls the routine behind the
//! report figure or finding it mirrors and records the result with its
//! conditions in the key names:
//!
//! - `tail_volume_coefficients`: `alas_stab::trim::tail_volume_coefficients`
//!   on the built planforms, the numbers the control-surface figure prints;
//! - `field_isa_sea_level`: one
//!   [`crate::field_reference::IsaSeaLevelFieldReference`] on a synthetic ISA
//!   sea-level aerodrome (computed `VMC`, geometry-scaled `CLmax`,
//!   improved-climb `V2`), at the configured MTOW and the design landing mass;
//! - `cruise_and_ceiling`: the initial cruise altitude capability of the
//!   charted design point's trip (with the sizing closure's own top-of-climb
//!   finding beside it) and the Quick Analysis thrust-limited service ceiling;
//! - `design_point_range`: the preset's charted design point (its payload,
//!   at the aircraft's takeoff-mass limit) priced by the payload-range corner
//!   rule ([`crate::design_point::fly_design_point`], the
//!   routine the acceptance matrix's design-mission grade flies) under the
//!   reserve rule the chart states (`reference.design_point.reserve`, the
//!   configured policy for every term it leaves unstated) and at the cruise
//!   level and Mach it states (`reference.design_point.cruise`, or the
//!   source-typical level when the chart states none; the configured
//!   schedule without one): the model's like-for-like value of the
//!   published point;
//! - `maximum_operating_altitude_m`: the certified limit declared with the
//!   preset (`mass_model.flops_structure.maximum_operating_altitude_m`), a
//!   certification datum and not a computed quantity.
//!
//! A section the model cannot produce is `{"unavailable": reason}`, never a
//! substituted number. SI throughout unless a key says otherwise: masses kg,
//! speeds m/s, lengths m.

use alas_config::AlasConfig;
use alas_opt::mdo::top_of_climb;
use serde_json::{json, Value};

use crate::design_point::{fly_design_point, DesignPointFlight};
use crate::field_reference::{
    report_isa_sea_level_field_reference, IsaSeaLevelFieldReference, VREF_OVER_VS1G,
};
use crate::full_analysis::AnalysisReport;
use crate::quick_analysis::corners::{chart_corner_masses, corner_ranges_at, payload_point_range};
use crate::quick_analysis::{CruiseSolve, SERVICE_CEILING_CLIMB_RATE_M_S};

/// Metres per nautical mile.
const M_PER_NMI: f64 = 1_852.0;

fn unavailable(reason: impl Into<String>) -> Value {
    json!({ "unavailable": reason.into() })
}

/// The `characteristics` block of the design `report` describes under `config`.
pub(super) fn database(report: &AnalysisReport, config: &AlasConfig) -> Value {
    let design_point = design_point_json(report, config);
    json!({
        "note": "Datasheet quantities of this design computed by the report's own routines (see the module documentation of alas-pipeline export/characteristics.rs). SI units.",
        "tail_volume_coefficients": tail_volumes(report),
        "field_isa_sea_level": field_json(report, config),
        "cruise_and_ceiling": cruise_json(report, config, &design_point),
        "design_point_range": design_point,
        "maximum_operating_altitude_m": config
            .mass_model
            .flops_structure
            .maximum_operating_altitude_m,
        "maximum_operating_altitude_basis": "certified limit declared with the preset (type certificate data sheet cited at alas-config preset_flops/structure.rs); null when the aircraft has none declared",
    })
}

fn tail_volumes(report: &AnalysisReport) -> Value {
    let (horizontal, vertical) = alas_stab::trim::tail_volume_coefficients(&report.airplane);
    json!({
        "horizontal": horizontal,
        "vertical": vertical,
        "definition": "Vh = Sh Lh / (S c_bar), Vv = Sv Lv / (S b) on the built planforms (alas_stab::trim::tail_volume_coefficients), as printed on the control-surface figure",
    })
}

fn field_json(report: &AnalysisReport, config: &AlasConfig) -> Value {
    let mtow_kg = config.requirements.mtow_kg;
    // The design landing mass, else the published MLW of the preset, else the
    // mass model's own landing fraction of the take-off mass (a notional
    // aircraft such as the AVE publishes no MLW).
    let mlw_kg = report
        .design_landing_mass_kg()
        .or_else(|| {
            alas_config::presets::get(&config.preset)
                .ok()
                .and_then(|preset| preset.reference.mlw_kg)
        })
        .or(Some(config.mass_model.mlw_fraction_mtow * mtow_kg))
        .filter(|mass| mass.is_finite() && *mass > 0.0);
    let Some(mlw_kg) = mlw_kg else {
        return unavailable(
            "no design landing mass, no preset MLW and no mass-model landing fraction",
        );
    };
    match report_isa_sea_level_field_reference(
        config,
        report,
        report.airplane.s_ref,
        mtow_kg,
        mlw_kg,
    ) {
        Ok(reference) => reference_json(&reference),
        Err(reason) => unavailable(reason),
    }
}

fn reference_json(reference: &IsaSeaLevelFieldReference) -> Value {
    let speeds = &reference.speeds;
    // The integrated jet field carries a balanced V1 and the selected VR; the
    // propeller method and the replay correlations integrate none, and their
    // decision and rotation speeds are the schedule of the speed model.
    let (v1, vr, v1_vr_basis) = match &reference.takeoff_field {
        Some(field) => (
            field.v1_m_s,
            field.vr_m_s,
            "CS 25.113 integration: balanced V1 (CS 25.109 accelerate-stop equal to the one-engine continued take-off distance) and the VR searched between the schedule minimum and V2 for the shortest field",
        ),
        None => (
            speeds.v1_ms,
            speeds.v_r_ms,
            "speed-model schedule, not an integrated field: VR = max(VR/VMC factor x VMC, VR/VSR factor x VSR) (CS 25.107(e) floors); V1 is the conceptual decision-speed proxy floored at VMC (no VEF, VMCG or engine-out acceleration for this method)",
        ),
    };
    // Climb-limited: the gradient evidence is complete and misses the CS
    // 25.121(b) minimum even at the speed and setting the selection chose.
    let climb_limited = reference.oei.available_gradient.is_some()
        && reference.oei.required_gradient.is_some()
        && !reference.oei.meets_required_gradient();
    json!({
        "condition": "ISA sea level, still air, dry level runway; take-off speeds and field at the configured MTOW, landing speeds and field at the design landing mass; speed model of the report (computed VMC, geometry-scaled CLmax, improved-climb V2)",
        "takeoff_mass_kg": reference.takeoff_mass_kg,
        "landing_mass_kg": reference.landing_mass_kg,
        "v1_m_s": v1,
        "vr_m_s": vr,
        "v1_vr_basis": v1_vr_basis,
        "v2_m_s": reference.v2_m_s,
        "vsr_takeoff_m_s": reference.v_stall_takeoff_m_s,
        "vmc_m_s": speeds.v_mc_ms,
        "vref_m_s": reference.vref_m_s,
        "vref_basis": format!("VREF = {VREF_OVER_VS1G} VS1g (14 CFR 25.125(b)(2)(i)) at the landing mass"),
        "vapp_m_s": speeds.v_app_ms,
        "vapp_basis": "approach speed of the speed model, still air: the configured approach multiple of the landing-configuration 1 g stall speed (1.23 for every preset that declares a landing reference); no wind or gust additive",
        "vs1g_landing_m_s": reference.vs1g_landing_m_s,
        "tofl_m": (!climb_limited).then_some(reference.takeoff_field_length_m),
        "tofl_basis": reference.takeoff_field_length_basis,
        "takeoff_climb_limited": climb_limited,
        "takeoff_climb_limit_basis": "CS 25.121(b) second-segment gradient not met at any V2 between the schedule and the overspeed limit V2/VS maxi (Airbus, Getting to Grips with Aircraft Performance, 2002, p. 194) in any offered take-off configuration: no take-off field length exists at this mass, so tofl_m is null and the field flown at the best speed within the limit is tofl_at_climb_limit_m",
        "tofl_at_climb_limit_m": climb_limited.then_some(reference.takeoff_field_length_m),
        "takeoff_flap_deg": reference.takeoff_flap_deg,
        "takeoff_flap_selection": reference.takeoff_flap_selection,
        "v2_over_vsr": reference.oei.v2_over_vsr,
        "oei_available_gradient": reference.oei.available_gradient,
        "oei_required_gradient": reference.oei.required_gradient,
        "balanced_field_length_m": reference.model_balanced_field_length_m,
        "lfl_m": reference.landing_field_length_m,
        "lfl_basis": reference.landing_field_length_basis,
        "landing_distance_unfactored_m": reference.landing_distance_m,
        "static_tw": reference.static_tw,
        "static_thrust_n": reference.static_thrust_n,
        "static_thrust_basis": reference.static_thrust_basis,
    })
}

fn cruise_json(report: &AnalysisReport, config: &AlasConfig, design_point: &Value) -> Value {
    let requirements = &config.requirements;
    // The plan the sizing closure froze: the route's own trip for a baseline
    // (closed on its route at the route's operational level, e.g. the
    // A320-200 on LEMD-LEPA under FL280), the design mission for a sized
    // mode. Reported for the closure; not a datasheet initial cruise altitude.
    let closure_plan = report
        .fuel
        .artifacts(config, &report.design)
        .and_then(|artifacts| top_of_climb(config, &artifacts))
        .map(|toc| {
            json!({
                "condition": "the sizing closure's frozen plan (the route's trip for a baseline, the design mission for a sized mode): residual climb at maximum-climb thrust at its initial cruise level less 30 m, top-of-climb mass (the report's 300 ft/min finding)",
                "initial_cruise_level_m": toc.altitude_m,
                "residual_climb_rate_m_s": toc.rate_m_s,
            })
        })
        .unwrap_or_else(unavailable);
    // The datasheet quantity: the initial cruise altitude capability of the
    // charted design point's trip (brake release at the takeoff-mass limit,
    // charted payload, its range), the definition the published values carry
    // (Jenkinson, Simpkin and Rhodes, "Civil Jet Aircraft Design", 1999,
    // Ch. 10, p. 268: 300 ft/min at maximum-climb rating at the top-of-climb
    // mass), searched up to the certified maximum operating altitude.
    // The level is also held under the 1.3 g buffet margin where the model
    // carries an onset lift (Airbus 2002, "Maximum altitude").
    //
    // A run without a charted point (a freighter flown on its passenger
    // donor's preset, a preset that declares none) is read on its own design
    // point: the design payload at the same takeoff-mass limit, configured
    // reserves. A trip that was flown always reports a level: where no level
    // of the climb geometry holds the criterion, the level the trip flew,
    // marked as such.
    let charted = (design_point["flight"].is_object()).then(|| TripCapability {
        basis: "charted",
        flight: design_point["flight"].clone(),
        takeoff_mass_kg: design_point["takeoff_mass_kg"].clone(),
        range_m: design_point["range_m"].clone(),
        payload_kg: design_point["payload_kg"].clone(),
    });
    let trip = charted.or_else(|| design_payload_trip(report, config));
    let initial_cruise = match trip {
        Some(trip) => {
            let found = trip.flight["initial_cruise_capability_m"].as_f64();
            let flown = trip.flight["initial_cruise_level_m"].as_f64();
            let condition = match trip.basis {
                "charted" => "charted design point's trip (design_point_range: takeoff-mass limit, charted payload, its range)",
                _ => "design-payload trip (no charted point for this configuration: the configured design payload at the payload-range takeoff-mass limit, fuel = min(tank less taxi, limit less OEW less payload), configured reserves, its range)",
            };
            match found.or(flown) {
                Some(level_m) => json!({
                    "condition": format!(
                        "{condition}: highest level, up to the certified maximum operating altitude (the design cruise altitude where none is declared), at which maximum-climb thrust leaves 300 ft/min at the cruise speed at the top-of-climb mass its own climb reaches (Jenkinson et al. 1999 p. 268) and the wing keeps 1.3 g to buffet onset at the cruise speed (Airbus 2002, Maximum altitude); a propeller aircraft's rate is its best rate of climb (cruise ceiling, Sadraey 2013 Sec. 4.3.6) and its cruise speed must be held on maximum-cruise power"
                    ),
                    "basis": trip.basis,
                    "initial_cruise_level_m": level_m,
                    "capability_found": found.is_some(),
                    "level_basis": if found.is_some() {
                        "the capability search"
                    } else {
                        "no level of the climb geometry holds the criterion at the top-of-climb mass: the initial cruise level the trip flew"
                    },
                    "search_ceiling_m": trip.flight["capability_ceiling_m"],
                    "flown_initial_cruise_level_m": trip.flight["initial_cruise_level_m"],
                    "takeoff_mass_kg": trip.takeoff_mass_kg,
                    "payload_kg": trip.payload_kg,
                    "range_m": trip.range_m,
                    "closure_plan": closure_plan,
                }),
                None => json!({
                    "unavailable": "the design point's trip could not be re-flown",
                    "closure_plan": closure_plan,
                }),
            }
        }
        None => json!({
            "unavailable": "neither the charted design point nor the design-payload point could be priced and flown",
            "closure_plan": closure_plan,
        }),
    };
    let mass_kg = report.analysis_takeoff_mass_kg(requirements.mtow_kg);
    let searched =
        CruiseSolve::new(config, report, mass_kg).and_then(|solve| solve.service_ceiling());
    let ceiling = searched.as_ref().map_or_else(
        |error| unavailable(error.clone()),
        |found| json!(found.altitude_m),
    );
    // True when the climb rate still meets the criterion at the upper bound
    // of the altitude scan: `service_ceiling_m` is then a lower bound.
    let at_scan_bound = searched
        .as_ref()
        .map_or(Value::Null, |found| json!(found.at_scan_bound));
    json!({
        "top_of_climb_initial_cruise_level": initial_cruise,
        "thrust_limited": {
            "condition": "standard day, report drag table and propulsion deck at maximum-climb rating, at the analysed take-off mass (conservative for the end of cruise)",
            "mass_kg": mass_kg,
            "service_ceiling_m": ceiling,
            "service_ceiling_is_scan_bound": at_scan_bound,
            "service_ceiling_criterion_climb_rate_m_s": SERVICE_CEILING_CLIMB_RATE_M_S,
        },
    })
}

/// The initial cruise altitude capability is searched up to the certified
/// maximum operating altitude: the design cruise altitude is a dispatch
/// choice, not a limit of the aircraft.
fn capability_ceiling_m(config: &AlasConfig) -> f64 {
    config
        .mass_model
        .flops_structure
        .maximum_operating_altitude_m
        .unwrap_or(config.requirements.cruise_altitude_m)
}

/// The trip an initial cruise altitude capability is read on.
struct TripCapability {
    /// `charted` or `design_payload`.
    basis: &'static str,
    /// The trip's flight block (levels, capability, search ceiling).
    flight: Value,
    takeoff_mass_kg: Value,
    range_m: Value,
    payload_kg: Value,
}

/// The design payload at the payload-range takeoff-mass limit under the
/// configured reserves: the trip of a configuration without a charted point.
fn design_payload_trip(report: &AnalysisReport, config: &AlasConfig) -> Option<TripCapability> {
    let chart = chart_corner_masses(report, config)?;
    let payload_kg = config
        .design_payload_kg()
        .0
        .min(chart.masses.max_payload_kg);
    let ceiling_m = capability_ceiling_m(config);
    let found = payload_point_range(
        config,
        report,
        &chart.masses,
        payload_kg,
        &config.fuel_policy,
        None,
        Some(ceiling_m),
    )
    .ok()?;
    let flight = found.flight?;
    Some(TripCapability {
        basis: "design_payload",
        flight: json!({
            "initial_cruise_level_m": flight.initial_cruise_level_m,
            "initial_cruise_capability_m": flight.initial_cruise_capability_m,
            "capability_ceiling_m": ceiling_m,
        }),
        takeoff_mass_kg: json!(found.tow_kg),
        range_m: json!(found.range_m),
        payload_kg: json!(payload_kg),
    })
}

/// The preset's charted design point flown by the model under the reserve
/// rule its source states, with the payload-range corners under that rule.
///
/// Only for a run of the preset as declared: an overlay that changes the
/// aircraft type or the passenger count (a freighter conversion flown on a
/// passenger preset) is not the aircraft the point was published for.
fn design_point_json(report: &AnalysisReport, config: &AlasConfig) -> Value {
    let capability_ceiling_m = capability_ceiling_m(config);
    let flight = match fly_design_point(report, config, Some(capability_ceiling_m)) {
        Ok(flight) => flight,
        Err(reason) => return unavailable(reason),
    };
    let DesignPointFlight {
        point,
        chart,
        policy,
        charted_payload_kg,
        payload_kg,
        cruise_level_m,
        flown_config: stated,
        long_range,
        range,
    } = flight;
    let long_range = long_range.map(|(source, found)| match found {
        Ok(found) => json!({
            "stated_by_source": source,
            "definition": "99 % of the maximum specific range, high-speed side (Airbus, Getting to grips with aircraft performance, 2002; Boeing AERO QTR_04.07 fuel conservation), on the mission model's own specific range, at the mean cruise mass and level of the trip",
            "tas_m_s": found.tas_m_s,
            "tas_kt": found.tas_m_s / 0.514_444,
            "mach": found.mach,
            "max_range_tas_m_s": found.max_range_tas_m_s,
            "specific_range_over_maximum": found.specific_range_ratio,
            "mean_cruise_mass_kg": found.mass_kg,
            "mean_cruise_level_m": found.altitude_m,
            "passes": found.passes,
        }),
        Err(reason) => unavailable(format!(
            "long-range cruise speed not found, the configured speed is flown: {reason}"
        )),
    });
    let corners = match corner_ranges_at(&stated, report, &chart.masses, cruise_level_m) {
        Ok(ranges) => json!({
            "range_nmi": ranges.range_m.map(|m| m / M_PER_NMI),
            "payload_kg": ranges.payload_kg,
            "reserve_fuel_kg": ranges.reserve_fuel_kg,
        }),
        Err(reason) => unavailable(reason),
    };
    let reserve = json!({
        "scheme": policy.scheme.as_str(),
        "contingency_trip_fraction": policy.contingency_trip_fraction,
        "alternate_distance_nmi": policy.alternate_distance_nmi,
        "final_reserve_hold_min": policy.final_reserve_hold_min,
        "stated_by_source": point.reserve.map(|reserve| reserve.source),
    });
    let cruise = point.cruise.map_or_else(
        || json!({"stated_by_source": null}),
        |cruise| {
            json!({
                "altitude_m": cruise.altitude_m,
                "altitude_ft": cruise.altitude_m / 0.3048,
                "mach": cruise.mach,
                "stated_by_source": cruise.source,
            })
        },
    );
    match range {
        Ok(found) => json!({
            "condition": "charted payload (capped at the model's maximum payload) at the payload-range takeoff-mass limit (fuel = min(tank less taxi, limit less OEW less payload)), still air, segment mission model flown as a range search, reserve rule stated by the chart (configured policy for every unstated term), cruise level and Mach stated by the chart or its source-typical level (the stated level is the ceiling of the optimum-step ladder; configured schedule when none); corners A-D of the payload-range chart under the same rule and cruise",
            "range_nmi": found.range_m / M_PER_NMI,
            "range_m": found.range_m,
            "payload_kg": payload_kg,
            "charted_payload_kg": charted_payload_kg,
            "takeoff_mass_kg": found.tow_kg,
            "takeoff_fuel_kg": found.fuel_kg,
            "flight": found.flight.map(|flight| json!({
                "initial_cruise_level_m": flight.initial_cruise_level_m,
                "final_cruise_level_m": flight.final_cruise_level_m,
                "climb_time_s": flight.climb_time_s,
                "climb_fuel_kg": flight.climb_fuel_kg,
                "initial_cruise_capability_m": flight.initial_cruise_capability_m,
                "capability_ceiling_m": capability_ceiling_m,
            })),
            "tank_limited": found.tank_limited,
            "long_range_cruise": long_range,
            "fuel_plan_kg": found.plan.map(|plan| json!({
                "trip": plan.trip_kg,
                "contingency": plan.contingency_kg,
                "alternate": plan.alternate_kg,
                "final_reserve": plan.final_reserve_kg,
                "taxi_in_budget_in_takeoff_fuel": plan.taxi_in_kg,
            })),
            "taxi_fuel_kg": found.taxi_kg,
            "reserve_fuel_kg": found.reserve_fuel_kg,
            "oew_kg": chart.masses.oew_kg,
            "mtow_limit_kg": chart.masses.mtow_kg,
            "max_payload_kg": chart.masses.max_payload_kg,
            "published_range_nmi": point.range_nmi,
            "point_source": point.source,
            "reserve": reserve,
            "cruise": cruise,
            "corners_stated_rule": corners,
        }),
        Err(reason) => json!({
            "unavailable": reason,
            "reserve": reserve,
            "cruise": cruise,
            "corners_stated_rule": corners,
        }),
    }
}
