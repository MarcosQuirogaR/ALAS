// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical invariants and error bounds of the cruise-level, climb-revision
//! and frozen-plan numerics on registered presets.

use alas_config::design_variables::DesignVector;
use alas_config::mission::CruiseAltitudePolicy;
use alas_config::AlasConfig;
use alas_mass::dispatch::{solve_dispatch, DispatchLimits, DispatchSolution, DispatchStatus};
use alas_units::FOOT;

use super::frozen_plan::RICHARDSON_TOLERANCE;
use super::*;
use crate::mdo::build::build_geometry;
use crate::mdo::propulsion::max_climb_rate_ft_min;

const NMI: f64 = 1_852.0;
const LB: f64 = 0.453_592_37;

/// A preset model. Jets fly their design altitude at the design Mach on
/// every cruise rung (the declared rungs belong to the preset's own short
/// route level), unless `operational` asks for the preset's declared route
/// level and speeds; the ATR always flies its declared schedule. Jets carry a
/// representative transport polar (CD0 0.018, k 0.045, CD_wave 0.002 at the
/// cruise Mach); the ATR carries its report-derived terms. The invariants
/// below hold for any physical polar; the values are fixture inputs, not
/// references.
fn preset_model(preset: &str) -> (SegmentMissionModel, AlasConfig) {
    preset_model_at(preset, false)
}

fn preset_model_at(preset: &str, operational: bool) -> (SegmentMissionModel, AlasConfig) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let declared = alas_config::presets::get(preset)
        .unwrap_or_else(|error| panic!("{preset}: {error}"))
        .operational_mission_defaults()
        .cruise_altitude_m;
    let mut flown_altitude_m = config.requirements.cruise_altitude_m;
    if operational && declared > 0.0 {
        flown_altitude_m = declared;
    } else if preset != "ATR72-600" {
        let req = &config.requirements;
        let tas = req.cruise_mach
            * alas_atmo::us1976_compute_values(req.cruise_altitude_m, 0.0).speed_of_sound_m_s;
        let p = &mut config.mission.profile;
        p.cruise_1_air_speed_m_s = tas;
        p.cruise_2_air_speed_m_s = tas;
        p.cruise_3_air_speed_m_s = tas;
    }
    let design: DesignVector = alas_config::presets::get(preset)
        .unwrap_or_else(|error| panic!("{preset}: {error}"))
        .design_vector;
    let (config, _, plane) = build_geometry(&config, &design.to_array())
        .unwrap_or_else(|failure| panic!("{preset} geometry: {}", failure.reason));
    let req = &config.requirements;
    let deck = PropulsionDeck::from_engine(
        &super::fixture_engine(&config.geometry.engine),
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )
    .unwrap_or_else(|error| panic!("{preset} deck: {error}"));
    let (cd0, k) = if preset == "ATR72-600" {
        (0.024_600_009_989_746_922, 0.030_377_093_904_043_483)
    } else {
        (0.018, 0.045)
    };
    let model = SegmentMissionModel::new(
        config.mission.profile.clone(),
        req.cruise_mach,
        flown_altitude_m,
        0.0,
        0.0,
        plane.s_ref,
        std::sync::Arc::new(super::ParabolicPolar::new(cd0, k, 0.002, req.cruise_mach)),
        req.gravity_m_s2,
        1_500.0 * FOOT,
        PhaseAeroLimits::from_config(&config),
        deck,
    )
    .unwrap_or_else(|error| panic!("{preset} model: {error}"));
    (model, config)
}

/// The A320-200 fixture with its MMO/VMO gate removed.
///
/// The frozen-plan pins below were written when the A320 ladder flew 300 KCAS
/// all the way to the 11,278 m design level (M 0.87 at FL350, 0.91 at FL370),
/// where the climb ran out of thrust above about 62 t and a plan frozen at
/// 50 t had to be re-frozen. With the type certificate MMO (0.82) holding the
/// Mach number the same aircraft climbs to the design level at every mass up
/// to MTOW (measured: no re-freeze between 60 and 78 t), which is the physical
/// correction, not a regression. The re-freeze mechanism is a property of the
/// frozen-plan code, so it is still exercised on the pre-gate schedule as a
/// stress fixture; it is not a statement about the aircraft.
fn without_operating_limits(mut model: SegmentMissionModel, preset: &str) -> SegmentMissionModel {
    if preset == "A320-200" {
        model.profile.max_operating_mach = 0.0;
        model.profile.max_operating_cas_m_s = 0.0;
    }
    model
}

fn dispatch(
    model: &SegmentMissionModel,
    config: &AlasConfig,
    zfw_kg: f64,
    range_m: f64,
) -> DispatchSolution {
    solve_dispatch(
        zfw_kg,
        range_m,
        &config.fuel_policy,
        model,
        &DispatchLimits {
            mtow_kg: 1.0e9,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        },
        80,
        1.0,
    )
}

fn dispatch_tow(
    model: &SegmentMissionModel,
    config: &AlasConfig,
    zfw_kg: f64,
    range_m: f64,
) -> f64 {
    let solution = dispatch(model, config, zfw_kg, range_m);
    assert!(
        matches!(solution.status, DispatchStatus::Converged),
        "dispatch at {zfw_kg} kg over {:.0} nmi: {:?}",
        range_m / NMI,
        solution.status
    );
    solution.takeoff_mass_kg
}

/// Takeoff mass must rise with range at fixed payload: a heavier start may
/// only fly lower and burn more, never less. Sweeps cover each preset's
/// payload-range span; the B787-9 sweep crosses the 2,200 nmi region where
/// its payload-range used to collapse and ends at the chart's 5,300 nmi
/// maximum-zero-fuel-mass corner. Beyond it this fixture polar needs over
/// 320 t (126 % of chart MTOW), where the maximum-climb rating cannot pay
/// the 170 to 250 m/s climb acceleration even at the 0.5 m/s floor rate, so
/// no flyable mission exists to dispatch.
#[test]
fn takeoff_mass_rises_strictly_with_range() {
    for (preset, zfw_kg, ranges_nmi) in [
        (
            "A320-200",
            62_500.0,
            vec![500.0, 1_000.0, 1_500.0, 2_000.0, 2_500.0],
        ),
        (
            "B787-9",
            400_000.0 * LB,
            vec![
                1_500.0, 2_000.0, 2_200.0, 2_400.0, 3_000.0, 4_000.0, 5_300.0,
            ],
        ),
        (
            "ATR72-600",
            20_290.0,
            vec![200.0, 350.0, 500.0, 650.0, 800.0],
        ),
    ] {
        let (model, config) = preset_model(preset);
        let tows: Vec<f64> = ranges_nmi
            .iter()
            .map(|range| dispatch_tow(&model, &config, zfw_kg, range * NMI))
            .collect();
        for (pair, ranges) in tows.windows(2).zip(ranges_nmi.windows(2)) {
            assert!(
                pair[1] > pair[0],
                "{preset}: TOW {:.0} kg at {} nmi is not above {:.0} kg at {} nmi",
                pair[1],
                ranges[1],
                pair[0],
                ranges[0]
            );
        }
    }
}

/// The B787-9 dispatch closes at the payload-range corners of its airport
/// planning chart: maximum zero-fuel mass at the 5,300 nmi corner (chart
/// MZFW 400,000 lb) and maximum takeoff mass with full tanks (chart MTOW
/// 561,500 lb, 126,372 L of Jet A at 0.8 kg/L) at 7,600 nmi, near the
/// published 7,635 nmi range.
#[test]
fn b787_dispatch_closes_at_the_payload_range_corners() {
    let (model, config) = preset_model("B787-9");
    let full_tanks_zfw_kg = 561_500.0 * LB - 126_372.0 * 0.8;
    for (zfw_kg, range_nmi) in [(400_000.0 * LB, 5_300.0), (full_tanks_zfw_kg, 7_600.0)] {
        let tow = dispatch_tow(&model, &config, zfw_kg, range_nmi * NMI);
        assert!(tow > zfw_kg);
    }
}

/// The frozen step count meets its own Richardson bound, and the frozen
/// trip is within that bound of the same choices integrated at 64 steps.
#[test]
fn the_frozen_step_count_meets_the_richardson_bound() {
    let (model, _) = preset_model("B787-9");
    let tow = 250_000.0;
    let range = 5_300.0 * NMI;
    let plan = model
        .freeze_plan(tow, range, &CruiseAltitudePolicy::OptimumStep)
        .unwrap_or_else(|error| panic!("freeze: {error}"));
    assert!(plan.richardson_error_kg <= RICHARDSON_TOLERANCE * plan.trip_fuel_kg);
    let reference = model
        .clone()
        .with_frozen_plan(FrozenMissionPlan {
            steps_per_segment: 64,
            ..plan.clone()
        })
        .fly_trip(tow, range)
        .unwrap_or_else(|error| panic!("reference: {error}"))
        .leg
        .fuel_kg;
    let frozen = model
        .with_frozen_plan(plan)
        .fly_trip(tow, range)
        .unwrap_or_else(|error| panic!("frozen: {error}"))
        .leg
        .fuel_kg;
    assert!(
        (frozen - reference).abs() <= RICHARDSON_TOLERANCE * reference,
        "frozen {frozen} kg against the 64-step {reference} kg"
    );
}

/// A frozen plan stores the fuel of its own fixed choices, which must
/// replay at the planning mass and stay within the same error bound on a
/// finer integration. The fixture polar is independent of optimizer output.
// Coverage diagnostics keep explicitly rejected fixture loads visible.
#[allow(clippy::print_stderr)]
#[test]
fn frozen_plans_replay_their_declared_fuel_and_converge_with_fixed_choices() {
    for preset in alas_config::presets::registry() {
        let (model, config) = preset_model(preset.name);
        let range_m = 1_500_000.0;
        let mut replayed = Vec::new();
        for mass_fraction in [0.6, 0.75, 0.9] {
            let mass_kg = mass_fraction * config.requirements.mtow_kg;
            let plan = match model.freeze_plan(mass_kg, range_m, &CruiseAltitudePolicy::OptimumStep)
            {
                Ok(plan) => plan,
                Err(FreezeError::Fuel(FuelModelError::NotConverged(reason)))
                    if reason.contains("energy deficit")
                        || reason.contains("speed schedule not attained") =>
                {
                    continue
                }
                Err(error) => panic!("{} at {mass_kg} kg freeze: {error}", preset.name),
            };
            assert!(plan.richardson_error_kg <= RICHARDSON_TOLERANCE * plan.trip_fuel_kg);
            let frozen = model.clone().with_frozen_plan(plan.clone());
            let flown = frozen
                .fly_trip(mass_kg, range_m)
                .unwrap_or_else(|error| panic!("{} replay: {error}", preset.name));
            assert_eq!(flown.leg.fuel_kg.to_bits(), plan.trip_fuel_kg.to_bits());
            assert_eq!(flown.step_positions.len(), plan.step_positions.len());
            let reference = model
                .clone()
                .with_frozen_plan(FrozenMissionPlan {
                    steps_per_segment: 128,
                    ..plan
                })
                .fly_trip(mass_kg, range_m)
                .unwrap_or_else(|error| panic!("{} reference: {error}", preset.name));
            assert!(
                (flown.leg.fuel_kg - reference.leg.fuel_kg).abs()
                    <= RICHARDSON_TOLERANCE * reference.leg.fuel_kg,
                "{} frozen {} kg against fixed-choice reference {} kg",
                preset.name,
                flown.leg.fuel_kg,
                reference.leg.fuel_kg
            );
            replayed.push(mass_fraction);
        }
        assert!(
            !replayed.is_empty(),
            "{} has no flyable fixture load",
            preset.name
        );
        eprintln!(
            "{} validated fixed-plan mass fractions {replayed:?}",
            preset.name
        );
    }
}

/// A frozen plan re-frozen twice at its own closure prices the dispatch
/// within 0.1 % of fuel of the converged reference: the closure whose plan
/// is re-frozen at its own takeoff mass until that mass stops moving. Each
/// re-freeze contracts the takeoff-mass error about tenfold; once is not
/// always enough from the unstepped closure (B787-9 with the taxi-in budget
/// carried: 0.109 % after one, 0.009 % after two).
///
/// The B787-9 range is 5,350 nmi (was 5,300; re-pinned by lane field lengths,
/// 2026-10-17, by probe): with the capped BPR 8 take-off chart the 5,300 nmi
/// closure sits on the optimum-step policy's switch from three to four step
/// climbs at about 279 t (plans frozen at 278,476 and 279,092 kg differ by a
/// step and 229 kg of trip fuel), so the re-freeze iteration cycles between the
/// two plans (278,477 -> 279,092 -> 278,536 -> 278,481 -> 278,476 -> 279,092 kg)
/// and has no converged reference. At 5,350 nmi the three re-freezes land within
/// 0.017 % of fuel of the reference (5,250 nmi 0.067 %, 5,450 nmi 0.020 %).
#[test]
fn a_refrozen_plan_prices_dispatch_within_a_tenth_of_a_percent() {
    for (preset, zfw_kg, range_nmi) in [
        ("A320-200", 62_500.0, 2_120.0),
        ("B787-9", 400_000.0 * LB, 5_350.0),
        ("ATR72-600", 20_290.0, 758.0),
    ] {
        let (model, config) = preset_model(preset);
        let range = range_nmi * NMI;
        let policy = model.profile.cruise_altitude_policy;
        let frozen_at = |tow: f64| {
            let plan = model
                .freeze_plan(tow, range, &policy)
                .unwrap_or_else(|error| panic!("{preset} freeze: {error}"));
            dispatch_tow(
                &model.clone().with_frozen_plan(plan),
                &config,
                zfw_kg,
                range,
            )
        };
        // The first plan is frozen at the unstepped closure, 4.4 % of
        // takeoff mass away on the B787-9; two re-freezes at their own
        // closures follow it.
        let refrozen = frozen_at(frozen_at(frozen_at(dispatch_tow(
            &model, &config, zfw_kg, range,
        ))));
        let mut reference = refrozen;
        for _ in 0..10 {
            let next = frozen_at(reference);
            let settled = (next - reference).abs() <= 1.0;
            reference = next;
            if settled {
                break;
            }
        }
        let fuel_kg = reference - zfw_kg;
        assert!(
            (refrozen - reference).abs() <= 1.0e-3 * fuel_kg,
            "{preset}: re-frozen TOW {refrozen} kg against converged {reference} kg"
        );
    }
}

/// Climb time to the initial level against the EUROCONTROL Aircraft
/// Performance Database A320 climb rates (2,500 ft/min to FL50, 2,000 to
/// FL150, 1,400 to FL240, 1,000 ft/min above), within 25 %, and takeoff-plus-
/// climb fuel between Raymer's climb fraction (0.985) and Roskam's
/// takeoff-times-climb fraction (0.995 x 0.980) of the takeoff mass, on the
/// A320 short-sector loading (42 t OEW, 150 passengers at 95 kg, 536 nmi).
#[test]
fn a320_climb_time_and_fuel_lie_in_their_sourced_bands() {
    let (model, _) = preset_model_at("A320-200", true);
    let tow = 42_000.0 + 150.0 * 95.0 + 6_000.0;
    let flown = model
        .fly_trip(tow, 536.0 * NMI)
        .unwrap_or_else(|error| panic!("A320 short sector: {error}"));
    let level_ft = flown.cruise_altitude_m / FOOT;
    let segment =
        |low: f64, high: f64, rate_ft_min: f64| (level_ft.min(high) - low).max(0.0) / rate_ft_min;
    let reference_min = segment(0.0, 5_000.0, 2_500.0)
        + segment(5_000.0, 15_000.0, 2_000.0)
        + segment(15_000.0, 24_000.0, 1_400.0)
        + segment(24_000.0, f64::INFINITY, 1_000.0);
    let climb_min = flown.climb_time_s / 60.0;
    assert!(
        (climb_min - reference_min).abs() <= 0.25 * reference_min,
        "climb to FL{:.0}: {climb_min:.1} min against {reference_min:.1} min",
        level_ft / 100.0
    );
    let fraction = flown.climb_fuel_kg / tow;
    assert!(
        (1.0 - 0.985..=1.0 - 0.995 * 0.980).contains(&fraction),
        "climb fuel {:.0} kg is {fraction:.4} of the takeoff mass",
        flown.climb_fuel_kg
    );
}

/// The initial level is the *highest* altitude below the cap that keeps the
/// 300 ft/min residual climb at the estimated top-of-climb mass, and it never
/// rises with mass. The default widebody fixture's residual climb at cruise
/// speed is not monotone in altitude (a dip near the 8,000 m crossover from
/// the calibrated-airspeed limit to the Mach limit), which is where a
/// bisection over the whole band picked a low root: 7,565 m at 95 % MTOW
/// while 10,000 m qualified.
#[test]
fn the_initial_level_is_the_highest_qualifying_altitude_and_never_rises_with_mass() {
    use super::cruise_levels::{CLIMB_MASS_FRACTION, LEVEL_SCAN_STEP_M, RESIDUAL_CLIMB_M_S};
    let config = AlasConfig::default();
    let (config, _, plane) = build_geometry(&config, &DesignVector::default().to_array())
        .unwrap_or_else(|failure| panic!("geometry: {}", failure.reason));
    let req = &config.requirements;
    let deck = PropulsionDeck::from_engine(
        &super::fixture_engine(&config.geometry.engine),
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )
    .expect("deck");
    let model = SegmentMissionModel::new(
        config.mission.profile.clone(),
        req.cruise_mach,
        req.cruise_altitude_m,
        0.0,
        0.0,
        plane.s_ref,
        std::sync::Arc::new(super::ParabolicPolar::new(
            0.018,
            0.045,
            0.002,
            req.cruise_mach,
        )),
        req.gravity_m_s2,
        457.2,
        PhaseAeroLimits::from_config(&config),
        deck,
    )
    .expect("model");
    let geometry = model.geometry();
    let range_m = 3.0e6;
    let cap_m = geometry
        .plan(LegKind::Trip, range_m)
        .expect("plan")
        .planned_cruise_m;
    let margin = |mass_kg: f64, cruise_m: f64| {
        let plan = geometry
            .plan_at(LegKind::Trip, range_m, cruise_m)
            .expect("plan");
        let cruise = plan.cruise_rungs[0].0;
        let climb = plan.climb.last().expect("climb").tas_m_s;
        let toc_kg = mass_kg * CLIMB_MASS_FRACTION;
        let rate = |tas| {
            model
                .residual_climb_m_s(toc_kg, plan.cruise_altitude_m, tas)
                .expect("residual")
        };
        rate(cruise).min(rate(climb)) - RESIDUAL_CLIMB_M_S
    };
    let mut previous_m = f64::INFINITY;
    for step in 0..=20 {
        let mass_kg = (0.8 + 0.01 * f64::from(step)) * req.mtow_kg;
        let level_m =
            model.initial_cruise_level_m(&geometry, LegKind::Trip, mass_kg, range_m, cap_m);
        assert!(
            margin(mass_kg, level_m - 1.0) >= 0.0,
            "{mass_kg} kg: {level_m} m fails"
        );
        let mut above_m = cap_m;
        while above_m > level_m + 1.0 {
            assert!(
                margin(mass_kg, above_m) < 0.0,
                "{mass_kg} kg: {above_m} m qualifies above the chosen {level_m} m"
            );
            above_m -= LEVEL_SCAN_STEP_M;
        }
        assert!(
            level_m <= previous_m + 1.0,
            "{mass_kg} kg flies higher: {level_m} m"
        );
        previous_m = level_m;
    }
}

/// A speed change a climb rung leaves unpaid is rejected when the next rung
/// opens its own target, and it must be revised on the rung that held the
/// budget. The B787-9 fixture at 319 t accelerates from 170 to 250 m/s on
/// the second climb rung, 680 m tall at its 3 m/s commanded rate: blamed on
/// the third rung, whose gentler rate cannot pay it, the leg was rejected;
/// a gentler second rung buys the time and the leg flies with a closed
/// energy ledger.
#[test]
fn an_unpaid_climb_acceleration_is_revised_on_the_rung_that_owns_it() {
    let (model, _) = preset_model("B787-9");
    let flown = model
        .fly_trip(319_000.0, 7_000.0 * NMI)
        .unwrap_or_else(|error| panic!("319 t over 7,000 nmi: {error}"));
    assert!(flown.adapted);
    let l = flown.ledger;
    let residual = l.propulsive_work_j - l.drag_work_j - l.potential_energy_j - l.kinetic_energy_j;
    assert!(residual.abs() <= 1.0e-8 * l.propulsive_work_j);
}

/// A frozen plan is a fixed function of mass, as the exact dispatch memo
/// keyed by mass requires: an A320-200 plan frozen at 50 t that cannot climb
/// to its level at 92 t flies that trip adapted and records the mass, and prices
/// the 50 t trip bit-identically before and after. (The plan re-frozen in
/// place at the unflyable mass priced the same 50 t trip 1.5 % higher
/// afterwards. The fixture engine is uprated by 25 %, see `fixture_engine`,
/// which moved the unflyable mass from 66 t to 92 t.)
#[test]
fn a_frozen_plan_answers_a_mass_the_same_after_an_unflyable_trip() {
    let (model, _) = preset_model("A320-200");
    let model = without_operating_limits(model, "A320-200");
    let range = 2_120.0 * NMI;
    let policy = model.profile.cruise_altitude_policy;
    let plan = model
        .freeze_plan(50_000.0, range, &policy)
        .unwrap_or_else(|error| panic!("freeze: {error}"));
    let frozen = model.with_frozen_plan(plan.clone());
    let trip = |mass_kg: f64| {
        frozen
            .fly_trip(mass_kg, range)
            .map(|flown| flown.leg.fuel_kg)
    };
    let before = trip(50_000.0).unwrap_or_else(|error| panic!("50 t: {error}"));
    let heavy = trip(92_000.0).unwrap_or_else(|error| panic!("92 t: {error}"));
    assert!(heavy > before);
    assert_eq!(frozen.refreeze_request_kg(), Some(92_000.0));
    let after = trip(50_000.0).unwrap_or_else(|error| panic!("50 t: {error}"));
    assert_eq!(before.to_bits(), after.to_bits());
    assert_eq!(frozen.frozen_plan(), Some(plan));
}

/// A dispatch whose first plan (frozen far below the closure) had to be
/// re-frozen replays bit-identically on the plan it carries out: every mass
/// the closure priced came from that one plan, so report trip equals
/// closure trip.
#[test]
fn a_re_frozen_dispatch_replays_exactly_on_its_carried_plan() {
    for (preset, first_kg, zfw_kg, range_nmi) in [
        ("A320-200", 50_000.0, 62_500.0, 2_120.0),
        ("B787-9", 170_000.0, 400_000.0 * LB, 5_300.0),
        ("ATR72-600", 16_000.0, 20_290.0, 758.0),
    ] {
        let (model, config) = preset_model(preset);
        let model = without_operating_limits(model, preset);
        let range = range_nmi * NMI;
        let policy = model.profile.cruise_altitude_policy;
        let solve = |frozen: &SegmentMissionModel| dispatch(frozen, &config, zfw_kg, range);
        let (carried, closure) = model
            .solve_on_frozen_plans(first_kg, range, &policy, solve)
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
        let plan = carried
            .frozen_plan()
            .unwrap_or_else(|| panic!("{preset}: no plan"));
        assert_ne!(plan.takeoff_mass_kg, first_kg, "{preset}: never re-frozen");
        assert!(
            matches!(closure.status, DispatchStatus::Converged),
            "{preset}"
        );
        let replay = dispatch(
            &model.clone().with_frozen_plan(plan),
            &config,
            zfw_kg,
            range,
        );
        assert_eq!(
            closure.takeoff_mass_kg.to_bits(),
            replay.takeoff_mass_kg.to_bits(),
            "{preset}: closure {} kg, replay {} kg",
            closure.takeoff_mass_kg,
            replay.takeoff_mass_kg
        );
    }
}

/// Probe of fix lane C1 (not an assertion): trip fuel and time of each
/// registered preset on a representative sector with (a) the retired shared
/// literal-TAS ladder, (b) the preset's CAS/Mach ladder without the ground
/// roll, and (c) the full C1 model. The fixture polar is representative, not
/// the preset's own, so the numbers size the change and are not references.
/// Run with `--ignored --nocapture`.
#[test]
#[ignore = "probe: prints the before/after of the C1 speed ladders"]
// A probe prints its numbers for the lane report; it asserts nothing.
#[allow(clippy::print_stdout)]
fn probe_c1_ladders() {
    for (preset, mass_fraction, range_nmi) in [
        ("A220-300", 1.0, 800.0),
        ("A320-200", 1.0, 1_000.0),
        ("A340-300", 1.0, 4_000.0),
        ("A380-800", 1.0, 5_000.0),
        ("B787-9", 1.0, 5_000.0),
        ("B747-400", 1.0, 5_000.0),
        ("C919", 1.0, 1_000.0),
        ("E195-E2", 1.0, 800.0),
        ("DC-10", 1.0, 4_000.0),
        ("AVE", 1.0, 5_000.0),
        ("ATR72-600", 1.0, 300.0),
    ] {
        let (model, config) = preset_model_at(preset, true);
        let mass_kg = mass_fraction * config.requirements.mtow_kg;
        let range_m = range_nmi * NMI;
        let fly = |model: &SegmentMissionModel| {
            model
                .fly_trip(mass_kg, range_m)
                .map(|flown| (flown.leg.fuel_kg, flown.leg.time_s / 3600.0))
        };
        let mut no_roll = model.clone();
        no_roll.profile.takeoff_roll_time_s = 0.0;
        let mut old = no_roll.clone();
        let cruise = old.profile.cruise_1_air_speed_m_s;
        old.profile = alas_config::mission::MissionProfileConfig {
            cruise_1_air_speed_m_s: cruise,
            cruise_2_air_speed_m_s: cruise,
            cruise_3_air_speed_m_s: cruise,
            takeoff_roll_time_s: 0.0,
            ..alas_config::mission::MissionProfileConfig::default()
        };
        if matches!(preset, "A320-200" | "ATR72-600") {
            old.profile = no_roll.profile.clone();
            old.profile.max_operating_mach = 0.0;
            old.profile.max_operating_cas_m_s = 0.0;
        }
        println!(
            "C1 PROBE {preset:10} {range_nmi:6.0} nmi  old {:?}  ladder {:?}  full {:?}",
            fly(&old),
            fly(&no_roll),
            fly(&model)
        );
    }
}

/// Probe (not an assertion): the lightest A320-200 mass at which a plan
/// frozen at 50 t can no longer climb to its level and is re-frozen.
#[test]
#[ignore = "probe: finds the re-freeze threshold mass used by the frozen-plan pins"]
// A probe prints its numbers for the lane report; it asserts nothing.
#[allow(clippy::print_stdout)]
fn probe_a320_refreeze_threshold() {
    let (model, _) = preset_model("A320-200");
    let range = 2_120.0 * NMI;
    let policy = model.profile.cruise_altitude_policy;
    let plan = model.freeze_plan(50_000.0, range, &policy).unwrap();
    for mass_kg in (60..=100).step_by(4).map(|t| f64::from(t) * 1_000.0) {
        let frozen = model.clone().with_frozen_plan(plan.clone());
        let result = frozen.fly_trip(mass_kg, range).map(|f| f.leg.fuel_kg);
        println!(
            "C1 PROBE refreeze {mass_kg:8.0} kg -> {:?} request {:?}",
            result,
            frozen.refreeze_request_kg()
        );
    }
}

/// The take-off segment carries the take-off flap increment (gear up, the
/// second-segment configuration) and the landing segment the landing flap
/// plus gear increments of the per-phase configuration drag (lane A1 fields).
#[test]
fn the_phase_drag_increments_come_from_the_per_phase_configuration_drag() {
    let mut config = AlasConfig::default();
    config.performance.oei_climb_delta_cd = 0.015;
    config.performance.landing_flap_delta_cd = 0.040;
    config.performance.landing_gear_delta_cd = 0.020;
    let limits = PhaseAeroLimits::from_config(&config);
    assert_eq!(limits.high_lift_delta_cd, 0.015);
    assert!((limits.landing_delta_cd - 0.060).abs() < 1.0e-15);
}

/// The top-of-climb rate the `toc_rate_of_climb` residual reports is the one
/// the level search holds: judged at the speeds the plan flies at that level
/// (the first cruise rung and the top climb rung, the lower of the two), not
/// at the constant-Mach speed `M a(h)`. Below the configured level the rung is
/// capped at `VC = VD / 1.25` in equivalent airspeed, so the plan is slower
/// than `M a(h)` there and the constant-Mach rate understates what it flies
/// (A220-300 over 251 nmi at FL250: 0.66 m/s at 241.6 m/s, 3.5 m/s at the
/// 209 m/s the plan flies).
#[test]
fn the_top_of_climb_rate_is_judged_at_the_speeds_the_plan_flies() {
    use super::cruise_levels::{CLIMB_MASS_FRACTION, RESIDUAL_CLIMB_M_S};
    let (model, config) = preset_model("A320-200");
    let geometry = model.geometry();
    let range_m = 1_500.0 * NMI;
    let mass_kg = 0.9 * config.requirements.mtow_kg;
    let altitude_m = 7_620.0;
    assert!(altitude_m < config.requirements.cruise_altitude_m - 1_000.0);
    let plan = geometry
        .plan_at(LegKind::Trip, range_m, altitude_m)
        .expect("plan");
    let cruise = plan.cruise_rungs[0].0;
    let climb = plan.climb.last().expect("climb").tas_m_s;
    let mach_tas = model
        .cruise_true_airspeed_m_s(plan.cruise_altitude_m)
        .expect("Mach speed");
    assert!(
        cruise < mach_tas - 5.0,
        "the plan flies {cruise} m/s below the configured level, constant Mach {mach_tas} m/s"
    );
    let toc_kg = mass_kg * CLIMB_MASS_FRACTION;
    let rate = |tas| {
        model
            .residual_climb_m_s(toc_kg, plan.cruise_altitude_m, tas)
            .expect("residual")
    };
    let reported = model
        .top_of_climb_rate_m_s(mass_kg, altitude_m, range_m, [1.0; 3])
        .expect("rate");
    assert_eq!(reported.to_bits(), rate(cruise).min(rate(climb)).to_bits());
    assert!(
        reported > rate(mach_tas) + 0.5,
        "{reported} m/s at the plan speeds against {} m/s at constant Mach",
        rate(mach_tas)
    );
    // The search's margin is this rate less the criterion.
    let level_m =
        model.initial_cruise_level_m(&geometry, LegKind::Trip, mass_kg, range_m, altitude_m);
    assert!(
        model
            .top_of_climb_rate_m_s(mass_kg, level_m - 1.0, range_m, [1.0; 3])
            .expect("rate at the level")
            >= RESIDUAL_CLIMB_M_S
    );
}

/// The warm-started climb revision is the full search's answer: a model
/// flying a mass sweep up and down (its memory warm with the previous rung
/// scales) prices every trip to the bit, and fails the same way, as a model
/// that has never revised a rung. The B787-9 fixture at 319 t over 7,000 nmi
/// revises its second climb rung (`an_unpaid_climb_acceleration_is_revised_on_the_rung_that_owns_it`).
#[test]
fn a_warm_started_climb_revision_prices_every_trip_as_a_cold_one_does() {
    let (warm, _) = preset_model("B787-9");
    let range = 7_000.0 * NMI;
    let cold_trip = |mass_kg: f64| {
        let mut cold = warm.clone();
        cold.memory = super::adapt_memory::AdaptationMemory::default();
        cold.fly_trip(mass_kg, range)
            .map(|flown| {
                (
                    flown.leg.fuel_kg.to_bits(),
                    flown.leg.time_s.to_bits(),
                    flown.adapted,
                )
            })
            .map_err(|error| error.to_string())
    };
    let masses: Vec<f64> = (0..=28)
        .map(|step| 312_000.0 + 500.0 * f64::from(step))
        .collect();
    let mut checked = 0;
    for order in [
        masses.clone(),
        masses.iter().rev().copied().collect::<Vec<_>>(),
    ] {
        for mass_kg in order {
            let warm_trip = warm
                .fly_trip(mass_kg, range)
                .map(|flown| {
                    (
                        flown.leg.fuel_kg.to_bits(),
                        flown.leg.time_s.to_bits(),
                        flown.adapted,
                    )
                })
                .map_err(|error| error.to_string());
            assert_eq!(warm_trip, cold_trip(mass_kg), "{mass_kg} kg");
            checked += 1;
        }
    }
    assert_eq!(checked, 58);
}

/// A solve stopped at its re-freeze request (the answer would be discarded
/// for the re-frozen plan) leaves the answer of the solve exactly as the
/// unstopped algorithm gave it: every plan and result equals a reference run
/// of freeze, solve, re-freeze at the first request, solve again. The model
/// handed back flies an unflyable mass adapted, not stopped.
#[test]
fn stopping_a_solve_at_its_refreeze_request_leaves_the_answer_exact() {
    for (preset, first_kg, zfw_kg, range_nmi) in [
        ("A320-200", 50_000.0, 62_500.0, 2_120.0),
        ("B787-9", 170_000.0, 400_000.0 * LB, 5_300.0),
        ("ATR72-600", 16_000.0, 20_290.0, 758.0),
    ] {
        let (model, config) = preset_model(preset);
        let model = without_operating_limits(model, preset);
        let range = range_nmi * NMI;
        let policy = model.profile.cruise_altitude_policy;
        let solve = |frozen: &SegmentMissionModel| dispatch(frozen, &config, zfw_kg, range);
        let (carried, closure) = model
            .solve_on_frozen_plans(first_kg, range, &policy, solve)
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
        let plan = model
            .freeze_plan(first_kg, range, &policy)
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
        let mut reference = model.clone().with_frozen_plan(plan);
        let mut reference_closure = solve(&reference);
        for _ in 0..frozen_plan::MAX_REFREEZES {
            let Some(mass_kg) = reference.refreeze_request_kg() else {
                break;
            };
            let Ok(plan) = model.freeze_plan(mass_kg, range, &policy) else {
                break;
            };
            reference = model.clone().with_frozen_plan(plan);
            reference_closure = solve(&reference);
        }
        assert_eq!(closure, reference_closure, "{preset}");
        assert_eq!(carried.frozen_plan(), reference.frozen_plan(), "{preset}");
        // Disarmed: a trip the carried plan cannot fly is flown adapted.
        let heavy_kg = 3.0 * closure.takeoff_mass_kg;
        let _ = carried.fly_trip(heavy_kg, range);
        let adapted = carried
            .fly_trip(closure.takeoff_mass_kg * 1.01, range)
            .map(|flown| flown.leg.fuel_kg);
        assert!(
            !matches!(adapted, Err(FuelModelError::Cancelled)),
            "{preset}: the returned model is still armed"
        );
    }
}

/// Where no plan can be made at the takeoff-mass ceiling the solve is run on
/// a plan made at a lighter mass, starting its dispatch there; where one can,
/// the answer is that of `solve_on_frozen_plans` to the bit.
#[test]
fn a_ceiling_no_plan_can_be_made_at_is_planned_at_a_lighter_mass() {
    let (model, config) = preset_model("A320-200");
    let model = without_operating_limits(model, "A320-200");
    let range = 2_120.0 * NMI;
    let policy = model.profile.cruise_altitude_policy;
    let zfw_kg = 62_500.0;
    let ceiling_kg = 130_000.0;
    assert!(model.freeze_plan(ceiling_kg, range, &policy).is_err());
    let started_at = std::cell::Cell::new(f64::NAN);
    let (carried, closure, planned_at_kg) = model
        .solve_on_descending_plans(ceiling_kg, zfw_kg, range, &policy, |frozen, start_kg| {
            started_at.set(start_kg);
            dispatch(frozen, &config, zfw_kg, range)
        })
        .unwrap_or_else(|error| panic!("no lighter plan: {error}"));
    assert!(planned_at_kg < ceiling_kg && planned_at_kg > zfw_kg);
    assert_eq!(started_at.get().to_bits(), planned_at_kg.to_bits());
    assert!(matches!(closure.status, DispatchStatus::Converged));
    assert!(carried.frozen_plan().is_some());

    let solve = |frozen: &SegmentMissionModel| dispatch(frozen, &config, zfw_kg, range);
    let (direct, direct_closure) = model
        .solve_on_frozen_plans(50_000.0, range, &policy, solve)
        .unwrap_or_else(|error| panic!("direct: {error}"));
    let (planned, planned_closure, at_kg) = model
        .solve_on_descending_plans(50_000.0, zfw_kg, range, &policy, |frozen, _| solve(frozen))
        .unwrap_or_else(|error| panic!("descending: {error}"));
    assert_eq!(at_kg.to_bits(), 50_000.0_f64.to_bits());
    assert_eq!(planned_closure, direct_closure);
    assert_eq!(planned.frozen_plan(), direct.frozen_plan());
}

/// A route ladder's first level is held to the planner's flyability rule: the
/// heavy B747-400 starts lower (and the dispatch then closes on the route),
/// the light one keeps the level the ladder configured, and a canonical
/// profile (no ladder) is never touched.
#[test]
fn a_route_ladders_first_level_starts_lower_when_heavy() {
    let (mut model, config) = preset_model("B747-400");
    let range = 3_000.0 * NMI;
    let cruise_m = config.requirements.cruise_altitude_m;
    let canonical = model.clone();
    alas_mission::configure_cruise_legs(&mut model.profile, 2, cruise_m, 0.0);
    let fraction = |model: &SegmentMissionModel, mass_kg: f64| {
        let geometry = model.geometry();
        model.limited_initial_fraction(&geometry, LegKind::Trip, mass_kg, range, cruise_m)
    };
    let heavy = fraction(&model, 396_000.0).expect("the heavy aircraft starts lower");
    assert!(heavy < model.profile.initial_climb_altitude_fraction);
    assert!(fraction(&model, 250_000.0).is_none_or(|f| f > heavy));
    assert_eq!(fraction(&canonical, 396_000.0), None);
    let flown = model.fly_trip(396_000.0, range);
    assert!(flown.is_ok(), "{flown:?}");
}
