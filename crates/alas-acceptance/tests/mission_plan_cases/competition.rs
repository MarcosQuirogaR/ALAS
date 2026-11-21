// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! C1: the competition mission LPA-VDE-LPA with a diversion (the
//! competition brief's Fig. 2.1), flown with its stages on a regional turboprop (ATR 72-600).

use alas_config::airports::get as get_airport;
use alas_config::{AlasConfig, MissionPlan, StageKind, TakeoffField};
use alas_route::route::Route;

use super::support::*;

/// Nautical miles in kilometres.
const KM_PER_NMI: f64 = 1.852;

/// ATR 72-600 take-off distance at the maximum take-off mass of the preset
/// (23,000 kg, "Option 1"), ISA, sea level, m (4,278 ft): ATR, ATR 72-600
/// factsheet (PW127M/N edition, 2020), page 2, "Airfield performance, Take-off
/// distance, Option 1 - MTOW - ISA - SL 1,304 m". Read from the text layer of the
/// page (the same sheet lists 1,279 m at the basic 22,800 kg, 1,156 m
/// at the 300 NM maximum-passenger weight and 1,383 m at 3,000 ft ISA+10).
/// The sheet does not define "Take-off distance": whether it is the CS 25.113
/// take-off distance or a balanced field length is to be settled by card
/// FIELD-OEI (see [`TOFL_RATIO_BAND`]).
const PUBLISHED_TOFL_MTOW_M: f64 = 1_304.0;

/// The factsheet (PW127M/N, 2020, p.2) labels the 1,304 m figure "Take-off
/// distance (MTOW, ISA, SL)", not a balanced field length; the current factsheet
/// page gives 1,315 m. Read as a take-off distance, the published figure is
/// compared with the model's turboprop field check (max of 1.15 AEO and the
/// Torenbeek balanced field, which contains a balanced term), a quantity that
/// is not defined to equal a take-off distance. The
/// unfactored all-engine distance is not exposed by `FieldPerformance` yet, so
/// the take-off-distance comparison is not evaluated here (ATR-FIELD, partial).
///
/// The band the model's field quantity over the published length must lie in
/// is NOT fixed here: it depends on the ATR field-length evidence (the ATR AFM
/// OEI gradient is not public, so the 1.20 vs 1.13 V2 choice stays flagged).
/// Two outcomes, each with its own band:
///
/// - Published figure is the CS 25.113 take-off distance and the model compares
///   the continued one-engine-out distance: the certified distance is the
///   greatest of the continued distance, 1.15 times the all-engines distance,
///   and (for the field) the accelerate-stop distance (CS 25.113, CS 25.109, as
///   summarised in the original FIELD-OEI brief; paragraph-level citation not
///   re-read on the page), so the continued distance lies in `[1 / 1.15, 1.0]`
///   of the published figure. The lower edge holds only for this definition.
/// - Published figure is a balanced field length, or the model's quantity is
///   the turboprop proxy (`StageFieldCheck::method` = "Torenbeek propeller
///   estimate": max of 1.15 times the all-engines distance and the Torenbeek
///   Eq. 5-89 balanced field, FIELD-OEI finding 1): the proxy contains a
///   balanced-field term and is not a continued one-engine-out distance, so no
///   band derived from CS 25.113 applies. The upper edge 1.0 is only a
///   statement about the quantity this test compares and is not to be read as
///   a certification band for the proxy.
///
/// Until ATR-FIELD closes, the test keeps the open-card ratchet below and does
/// not claim either band. The constant is kept for the closed branch.
const TOFL_RATIO_BAND: [f64; 2] = [1.0 / 1.15, 1.0];

/// Ratchet on the field-performance card (ATR-FIELD): the model's field check
/// (for a turboprop the Torenbeek propeller estimate, not a continued
/// one-engine-out distance) at the maximum take-off mass is about 15 % over the
/// published length (ratio 1.159 with the V1 viscous margin, 1.154 before it;
/// above the band). While `true` the test asserts that excess
/// (ratio above the band's upper edge, at most 1.17: today's 1.159, measured on
/// the tree with the V1 viscous margin, 1.154 before it, may not get worse
/// unnoticed; the margin raised the ATR's viscous drag by 7.5 % and the
/// distance by 0.5 %) and the present outcome of
/// the VDE full-cabin take-off; when the card closes it is set to `false`, and
/// the ratio must be inside the band and a full cabin whose scaled published length
/// fits the field must be feasible.
const OEI_TOD_LONG_OPEN: bool = true;

/// Payload the operator boards for the payload-limited dispatch, kg: 50
/// passengers at the load case's 100 kg.
const LIMITED_PAYLOAD_KG: f64 = 5_000.0;

/// The stages that move the aircraft from LPA to VDE, and from VDE back to the
/// decision height at LPA (the holding leg inside `atc-hold` is not one of them,
/// the descent into the hold is).
const OUT_STAGES: &[&str] = &[
    "takeoff-lpa",
    "climb-lpa",
    "cruise-lpa-vde",
    "descent-vde",
    "landing-vde",
];
const BACK_STAGES: &[&str] = &[
    "critical-takeoff-vde",
    "climb-vde",
    "cruise-vde-lpa",
    "atc-hold",
    "descent-lpa",
    "approach-lpa",
];

/// Ground distance flown by the legs of the stages `ids`, km, without the legs
/// of a hold (`loiter`), from the stage number in each leg's tag.
fn route_km(result: &alas_pipeline::PipelineResult, ids: &[&str]) -> f64 {
    let stages = &result.config.mission.plan.stages;
    let in_ids = |tag: &str| {
        let stage = tag
            .rsplit_once('@')
            .and_then(|(_, tail)| {
                tail.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<usize>()
                    .ok()
            })
            .and_then(|index| stages.get(index));
        stage.is_some_and(|stage| ids.contains(&stage.id.as_str()))
    };
    result
        .mission_result
        .as_ref()
        .unwrap()
        .segments
        .iter()
        .filter(|segment| in_ids(&segment.spec.tag) && !segment.spec.tag.starts_with("loiter"))
        .map(|segment| {
            let range = &segment.conditions.aircraft_range_m;
            range.last().copied().unwrap_or(0.0) - range.first().copied().unwrap_or(0.0)
        })
        .sum::<f64>()
        / 1000.0
}

/// The three aerodromes of the case as a student enters them: position, field
/// elevation and runway length from the Spanish AIP (AENA), rounded; the
/// standard atmosphere. These are inputs of the case, not model data.
fn register_competition_airports() {
    let airport = |icao: &str, name: &str, lat: f64, lon: f64, elevation_m: f64, runway_m: f64| {
        let record = alas_config::airport_io::CustomAirport {
            icao: icao.to_owned(),
            name: name.to_owned(),
            latitude_deg: lat,
            longitude_deg: lon,
            isa_delta_c: 0.0,
            altitude_m: elevation_m,
            runway_lengths_m: vec![runway_m],
            declared_toda_m: Some(runway_m),
            declared_lda_m: Some(runway_m),
            // No obstacle survey entered: the CAT.POL.A.210 check reports its evidence gap.
            departure_surveys: Vec::new(),
            provenance: alas_config::airport_io::AirportProvenance {
                kind: alas_config::airport_io::AirportProvenanceKind::UserEntered,
                source: Some("AENA, Spanish AIP, rounded (case input)".to_owned()),
                note: None,
            },
        };
        // A second call (the case is configured twice) finds the airports registered.
        let _ = alas_config::airport_io::register_custom_airport(record);
        // Only an already registered airport may make the call fail: the record
        // in the database must be the one of the case.
        assert!(
            get_airport(icao).is_ok_and(|a| (a.elevation_m - elevation_m).abs() < 1.0e-6),
            "{icao} is not registered as entered"
        );
    };
    airport("GCLP", "Gran Canaria", 27.932, -15.387, 24.0, 3_100.0);
    airport("GCHI", "El Hierro", 27.815, -17.887, 32.0, 1_250.0);
    airport("GCFV", "Fuerteventura", 28.453, -13.864, 26.0, 3_400.0);
}

/// The competition mission of C1:
/// 1 taxi LPA; 2 take-off + climb LPA; 3 cruise FL100 to VDE; 4 descent +
/// landing VDE; 5 taxi VDE; 6 critical take-off VDE, runway 1,250 m, OEI at V1,
/// continued; 7 cruise FL100 back to LPA; 8 ATC hold 15 min at the destination;
/// 9 descent + missed approach at the decision height; 10 climb to the alternate;
/// 11 cruise to FUE, 86 NM (160 km); 12 final reserve hold 30 min; 13 descent,
/// landing, taxi FUE. Eighteen plan stages, the climb and descent of a stage
/// being stages of their own.
///
/// The cruise of a leg is a distance, so `climb_descent_km` (out, back) is what
/// all the other legs that move the aircraft along each route leg cover: the first
/// flight of the case measures it (with a guess), the second is closed to the
/// route with it.
fn c1_configuration(climb_descent_km: [f64; 2]) -> AlasConfig {
    register_competition_airports();
    let mut config = config_for_preset("ATR72-600");
    config.departure_airport = "GCLP".to_owned();
    config.arrival_airport = "GCHI".to_owned();
    let lpa = get_airport("GCLP").unwrap();
    let vde = get_airport("GCHI").unwrap();
    let lpa_field = TakeoffField {
        name: "Gran Canaria".to_owned(),
        elevation_ft: lpa.elevation_m / 0.3048,
        toda_m: 3_100.0,
        isa_deviation_k: lpa.isa_deviation_c,
    };
    let leg_km = Route::great_circle(lpa, vde, 2).total_distance_m() / 1000.0;
    let cruise = [leg_km - climb_descent_km[0], leg_km - climb_descent_km[1]];
    println!(
        "LPA-VDE {leg_km:.1} km, cruise {:.1} and {:.1} km",
        cruise[0], cruise[1]
    );
    let field = TakeoffField {
        name: "El Hierro".to_owned(),
        elevation_ft: vde.elevation_m / 0.3048,
        toda_m: 1_250.0,
        isa_deviation_k: vde.isa_deviation_c,
    };
    config.mission.plan = MissionPlan {
        name: "C1 competition mission".to_owned(),
        stages: vec![
            stage("taxi-lpa", StageKind::Taxi { time_min: 8.0 }),
            stage(
                "takeoff-lpa",
                StageKind::Takeoff {
                    field: Some(lpa_field),
                },
            ),
            climb("climb-lpa", 10_000.0),
            stage("cruise-lpa-vde", cruise_km(cruise[0])),
            descent("descent-vde", 1_500.0, true),
            stage(
                "landing-vde",
                StageKind::Landing {
                    field_elevation_ft: Some(vde.elevation_m / 0.3048),
                },
            ),
            stage("taxi-vde", StageKind::Taxi { time_min: 5.0 }),
            stage(
                "critical-takeoff-vde",
                StageKind::Takeoff { field: Some(field) },
            ),
            climb("climb-vde", 10_000.0),
            stage("cruise-vde-lpa", cruise_km(cruise[1])),
            stage("atc-hold", loiter_min(15.0, Some(5_000.0))),
            descent("descent-lpa", 3_000.0, false),
            stage(
                "approach-lpa",
                StageKind::Approach {
                    decision_height_ft: alas_config::CATEGORY_I_DECISION_HEIGHT_FT,
                    field_elevation_ft: None,
                },
            ),
            stage(
                "missed-approach",
                StageKind::GoAround {
                    altitude_ft: config.mission.profile.descent_4_altitude_ft,
                    agl: true,
                },
            )
            .as_reserve(),
            stage(
                "divert-fue",
                StageKind::Diversion {
                    distance_km: 86.0 * KM_PER_NMI,
                    cruise_altitude_ft: 10_000.0,
                    field_elevation_ft: None,
                },
            )
            .as_reserve(),
            stage("final-hold", loiter_min(30.0, Some(1_500.0))).as_reserve(),
            stage(
                "landing-fue",
                StageKind::Landing {
                    field_elevation_ft: None,
                },
            )
            .as_reserve(),
            stage("taxi-fue", StageKind::Taxi { time_min: 5.0 }).as_reserve(),
        ],
        ..MissionPlan::default()
    };
    config
}

/// C1 flown with the cabin full: the intermediate landing and re-take-off, the
/// 15-minute ATC hold, the missed approach at the decision height, the 160 km
/// diversion to FUE and the 30-minute final hold, per stage; and the critical
/// take-off at VDE. The model says the ATR 72-600 at its zero-fuel mass of 21,000 kg
/// plus the fuel cannot make 1,250 m (the field check gives the distance it needs),
/// but that outcome is held by the ratchet `OEI_TOD_LONG_OPEN`: the model's
/// continued one-engine-out distance is about 15 % over the published ATR length,
/// and the published length scaled to that mass (1,194 m) fits.
/// With 5,000 kg of payload (the operator's payload-limited dispatch) it fits in
/// both readings.
#[test]
fn c1_competition_mission_flies_all_its_stages() {
    // First flight with a guess of what the other legs of each route leg cover, to
    // measure it; the second flight closes the cruises to the route with it.
    let guess = run(c1_configuration([60.0, 60.0]));
    let climb_descent = [
        route_km(&guess, OUT_STAGES) - route_km(&guess, &["cruise-lpa-vde"]),
        route_km(&guess, BACK_STAGES) - route_km(&guess, &["cruise-vde-lpa"]),
    ];
    println!("the legs other than the cruise cover {climb_descent:.1?} km");
    let config = c1_configuration(climb_descent);
    let design_mach = config.requirements.cruise_mach;
    let result = run(config.clone());
    print_tables(&result);
    print_findings(&result);
    assert_complete_and_closed(&result);
    // The ATR 72-600's descents average 800-900 ft/min over a short descent from
    // FL100 (rungs of 1,155, 906 and 800 ft/min of its profile, cut to its flight-idle
    // boundary): the descent-rate rule fires on the plan as it does (MEDIUM, 944
    // ft/min) on the standard mission. With the propeller-class viscous margin
    // (V1, 1.075 for turboprops) the flown level is lower and the three descents
    // average 981, 1,013 and 925 ft/min: MEDIUM, above the 900 ft/min HIGH bound, so
    // the ratchet this comment covered (HIGH on this plan) no longer fires; with the
    // margin set back to 1.00 it fires. The defect (turboprop
    // descent schedule rates on the 900 ft/min bound) is not fixed and stays
    // listed in plausibility_known.tsv for the standard mission.
    assert_plausible("C1 ATR72-600", &result, design_mach, &[]);
    assert_level_stage_sfc(&result, true, &[]);
    let history = history_of(&result);
    assert_eq!(history.stages.len(), 18);

    // Fuel, time and mass per stage: every stage reports.
    for stage in &history.stages {
        assert!(stage.fuel_kg >= 0.0, "{}", stage.id);
    }
    let by_id = |id: &str| history.stages.iter().find(|s| s.id == id).unwrap();
    // The route is closed: every leg that moves the aircraft along it (take-off,
    // climb, cruise, the descent into the hold, descent, approach and landing; not
    // the holding leg itself) covers the great-circle distance of LPA-VDE.
    let leg_km = Route::great_circle(
        get_airport("GCLP").unwrap(),
        get_airport("GCHI").unwrap(),
        2,
    )
    .total_distance_m()
        / 1000.0;
    for (name, ids) in [("LPA-VDE", OUT_STAGES), ("VDE-LPA", BACK_STAGES)] {
        let km = route_km(&result, ids);
        println!("{name}: {km:.1} km flown against {leg_km:.1} km");
        assert!((km - leg_km).abs() < 0.03 * leg_km, "{name}: {km} km");
    }
    // The holds are held for their time: the loiter leg of each, not the legs
    // that bring the aircraft to the holding level.
    let hold_s = |tag: &str| {
        let c = &result
            .mission_result
            .as_ref()
            .unwrap()
            .segments
            .iter()
            .find(|segment| segment.spec.tag == tag)
            .unwrap()
            .conditions;
        c.time_s.last().unwrap() - c.time_s.first().unwrap()
    };
    assert!((hold_s("loiter@10") - 900.0).abs() < 1.0);
    assert!((hold_s("loiter@15") - 1_800.0).abs() < 1.0);
    // The diversion covers 160 km (86 NM) to the alternate.
    let divert = by_id("divert-fue");
    assert!(
        (divert.distance_m / 1000.0 - 86.0 * KM_PER_NMI).abs() < 1.0,
        "{}",
        divert.distance_m
    );
    // The intermediate landing and the re-take-off: the aircraft stands on the
    // ground between stages 6 and 8, burning only its taxi fuel.
    assert!(by_id("taxi-vde").fuel_kg > 0.0);
    assert!(by_id("taxi-vde").time_s == 0.0 && by_id("taxi-vde").distance_m == 0.0);
    // The missed approach follows the approach: a go-around climb after the descent.
    assert!(by_id("missed-approach").time_s > 0.0);
    assert!(by_id("approach-lpa").time_s > 0.0);

    // The critical take-off at VDE, judged at the mass the stage starts with.
    let full = by_id("critical-takeoff-vde")
        .field
        .as_ref()
        .expect("the field check");
    println!(
        "VDE take-off at {:.0} kg: needs {:.0} m, {:.0} m available ({})",
        full.mass_kg, full.required_m, full.available_m, full.method
    );
    // The finding of the gate (which resolves the speed basis on its own path) and
    // the history report the same distance for this second, lighter take-off.
    let finding_m = result
        .feasibility
        .findings
        .iter()
        .find(|f| f.code == alas_pipeline::FindingCode::FieldTakeoffDistanceViolation)
        .and_then(|f| f.actual)
        .expect("the field finding");
    assert!(
        (finding_m - full.required_m).abs() < 1.0e-9 * full.required_m,
        "finding {finding_m} m, history {} m",
        full.required_m
    );
    assert_eq!(full.available_m, 1_250.0);
    // The published length scaled to this mass with the take-off mass squared is
    // the yardstick of the outcome. Basis of the scaling: Raymer, Aircraft Design: A
    // Conceptual Approach, 6th ed., Sec. 5.4, printed pp. 129-130, defines the
    // take-off parameter TOP = (W/S) / (sigma CL_TO (BHP/W)) for a propeller
    // aircraft (Eq. 5.8; (T/W) for jets, Eq. 5.9) and reads the field length from
    // it in Fig. 5.4, so at the same wing, density, lift coefficient and power it
    // goes with W^2 / (S sigma CL_TO BHP) if the line of the figure is proportional
    // to TOP (not checked on the figure itself; the FAR lines there are drawn per
    // number of jet engines, the propeller line is another quantity). The check in
    // the sheet itself: 1,304 m x (22,800/23,000)^2 = 1,281 m against the sheet's
    // own 1,279 m at the basic 22,800 kg (+0.2 %). The step to this stage's mass
    // is a 4 % extrapolation of that local agreement.
    let scaled_m = PUBLISHED_TOFL_MTOW_M * (full.mass_kg / config.requirements.mtow_kg).powi(2);
    println!(
        "published length scaled to {:.0} kg: {scaled_m:.0} m",
        full.mass_kg
    );
    if OEI_TOD_LONG_OPEN {
        // While the model's distance is long, the full cabin does not fit.
        assert!(
            !full.feasible
                && has_finding(
                    &result,
                    alas_pipeline::FindingCode::FieldTakeoffDistanceViolation
                ),
            "a full cabin does not take off from 1,250 m while the distance is long"
        );
    } else {
        // One way only: the certified length is at least the continued one-engine-out
        // distance, so a scaled length that fits the field says the take-off is
        // feasible, while one that does not fit says nothing about infeasibility.
        if scaled_m <= full.available_m {
            assert!(
                full.feasible,
                "scaled published length {scaled_m:.0} m fits"
            );
        }
    }

    // Take-off at the maximum take-off mass (stage 2): the fuel is stated to bring
    // the aircraft to it, and the continued one-engine-out distance at LPA is
    // compared with the published field length of the type.
    let ramp_fuel_kg = case_of(&result).ramp_fuel_kg
        + (config.requirements.mtow_kg - case_of(&result).takeoff_mass_kg);
    let mut at_mtow = config.clone();
    at_mtow.mission.plan.takeoff_fuel_kg = Some(ramp_fuel_kg);
    let at_mtow = run(at_mtow);
    let mtow_case = case_of(&at_mtow);
    assert!(
        (mtow_case.takeoff_mass_kg - config.requirements.mtow_kg).abs() < 1.0,
        "brake release at {} kg",
        mtow_case.takeoff_mass_kg
    );
    let lpa_check = history_of(&at_mtow)
        .stages
        .iter()
        .find(|s| s.id == "takeoff-lpa")
        .and_then(|s| s.field.as_ref())
        .expect("the field check at LPA");
    let ratio = lpa_check.required_m / PUBLISHED_TOFL_MTOW_M;
    println!(
        "LPA take-off at {:.0} kg: {:.0} m field check (method per FIELD-OEI, not a continued one-engine-out distance for a turboprop); published field length {PUBLISHED_TOFL_MTOW_M:.0} m: ratio {ratio:.2}",
        lpa_check.mass_kg, lpa_check.required_m
    );
    if OEI_TOD_LONG_OPEN {
        assert!(
            ratio > TOFL_RATIO_BAND[1] && ratio <= 1.17,
            "ratio {ratio:.3}: the card is open, so the excess over the band must stay (up to 1.17)"
        );
    } else {
        assert!(
            (TOFL_RATIO_BAND[0]..=TOFL_RATIO_BAND[1]).contains(&ratio),
            "ratio {ratio:.3} outside {TOFL_RATIO_BAND:?}"
        );
    }

    // The envelope: the ATR 72-600's operating empty centre of gravity lies aft of
    // the model's aft limit (registered known line ATR72-600 CG-ENVELOPE, owned by
    // P2): while it stands the plan reports the violation at every stage; once it
    // is fixed the case asserts the centre of gravity stays inside.
    if known_defect("ATR72-600", "CG-ENVELOPE") {
        assert!(has_finding(
            &result,
            alas_pipeline::FindingCode::CgEnvelopeViolation
        ));
    } else {
        assert!(history.points.iter().all(|point| point.within_envelope));
    }

    // The payload-limited dispatch: the same plan with 5,000 kg of payload. The
    // mass history of that run reads the full cabin's ledger, so only the flight
    // and its field check are read here.
    let limited = run_with_payload(config, Some(LIMITED_PAYLOAD_KG));
    assert_flown_and_closed(&limited);
    let check = history_of(&limited)
        .stages
        .iter()
        .find(|s| s.id == "critical-takeoff-vde")
        .and_then(|s| s.field.as_ref())
        .expect("the field check");
    println!(
        "payload-limited: take-off at {:.0} kg needs {:.0} m of {:.0} m",
        check.mass_kg, check.required_m, check.available_m
    );
    assert!(
        check.feasible && check.required_m < check.available_m,
        "the continued take-off fits 1,250 m with {LIMITED_PAYLOAD_KG} kg of payload"
    );
    assert!(check.mass_kg < full.mass_kg);
    assert!(!has_finding(
        &limited,
        alas_pipeline::FindingCode::FieldTakeoffDistanceViolation
    ));
}
