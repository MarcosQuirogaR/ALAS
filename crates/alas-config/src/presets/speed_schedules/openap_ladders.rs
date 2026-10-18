// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CAS/Mach climb and descent ladders for the registered presets that used to
//! inherit the shared literal-true-airspeed ladder (physics audit MIS-02,
//! MIS-08).
//!
//! # Sources
//!
//! Speeds and vertical rates are the modal ("opt") values of the OpenAP
//! kinematic (WRAP) tables, transcribed here as cited numbers; the OpenAP data
//! files are NOT vendored (GPL-3 `openap/data/LICENSE`). Retrieved 2026-10-11
//! from `https://github.com/TUDelft-CNS-ATM/openap`, `openap/data/wrap/<type>.txt`
//! (units SI: m/s, Mach). Method: Sun, Ellerbroek, Hoekstra, "WRAP: An
//! open-source kinematic aircraft performance model", Transportation Research
//! Part C 98 (2019) 118-138, doi 10.1016/j.trc.2018.11.009. They are
//! statistics of observed ADS-B/Mode-S flights, i.e. fleet operating
//! behaviour, not certified or manufacturer-scheduled speeds.
//!
//! MMO and VMO are from the type certificate data sheets where the sheet
//! states them (EASA TCDS), and are flagged where only secondary sources were
//! found. Nothing here is tuned to fuel burn or block time.
//!
//! # Type mapping (the owner decision D4: nearest documented type, flagged)
//!
//! A220-300, C919, E195-E2, DC-10 and AVE have no WRAP file of their own and
//! use the proxy named in [`Ladder::proxy_note`]; the proxy is a different
//! aeroplane and its speeds are a plausible class schedule, not data for the
//! preset.
//!
//! # Structure of the ladder
//!
//! The climb rungs are: take-off at the 250 kt climb-out speed to 1,500 ft
//! AGL; to 10,000 ft at 250 kt CAS (the speed limit below 10,000 ft, 14 CFR
//! 91.117(a)); then the type's CAS to the CAS/Mach crossover altitude; then
//! the type's Mach to the cruise level. Rates map the WRAP phases: initial
//! climb, pre-constant-CAS, constant-CAS and constant-Mach mean climb rates.
//! The descent mirrors it: Mach down to the crossover, CAS to 10,000 ft,
//! 250 kt CAS below, 210 kt to 3,000 ft (an unsourced intermediate approach
//! speed, as in the A320-200 ladder) and the WRAP final-approach speed and
//! rate. Descent rates follow a [`DESCENT_PATH_DEG`] path at each rung's own
//! true airspeed; where the idle-thrust path is shallower the mission deck
//! flies idle and the leg takes longer (it never flies below idle).

use crate::{AircraftPreset, MissionProfileConfig, SpeedReference};

/// Flight-path angle of the planned descent, degrees below the horizon. The
/// usual 3-degree descent gradient (ICAO PANS-OPS / ICAO Doc 8168 final
/// approach and the 300 ft/NM rule of thumb), against the 1.2-1.9 degrees the
/// old powered 4.5-5 m/s rates gave (audit MIS-08).
const DESCENT_PATH_DEG: f64 = 3.0;
/// International knot, m/s.
const KNOT_M_S: f64 = 1852.0 / 3600.0;
/// Foot, m.
const FOOT_M: f64 = 0.3048;
/// 14 CFR 91.117(a): 250 kt indicated below 10,000 ft MSL.
const SPEED_LIMIT_CAS_M_S: f64 = 250.0 * KNOT_M_S;
/// Altitude of that speed limit, m.
const SPEED_LIMIT_ALTITUDE_M: f64 = 10_000.0 * FOOT_M;
/// Intermediate approach speed between 5,000 and 3,000 ft (unsourced, the
/// A320-200 ladder's value).
const APPROACH_INTERMEDIATE_CAS_M_S: f64 = 210.0 * KNOT_M_S;
/// Representative climb rate the engine deck's thrust correlation is built
/// at, m/s (the shared default profile's initial-climb rate).
const DECK_REFERENCE_CLIMB_RATE_M_S: f64 = 12.0;

/// One type's transcribed ladder numbers. The two provenance strings are data
/// carried with the numbers and read by the tests.
#[allow(dead_code)]
struct Ladder {
    /// Where the numbers come from: the WRAP type file and, for a proxy, why
    /// it stands in.
    proxy_note: &'static str,
    /// Constant CAS of the climb (WRAP `cl_v_cas_const`), m/s.
    climb_cas_m_s: f64,
    /// Constant Mach of the climb (`cl_v_mach_const`).
    climb_mach: f64,
    /// Mean climb rate of the initial climb (`ic_vs_avg`), m/s.
    initial_climb_rate_m_s: f64,
    /// Mean climb rate before the constant-CAS phase (`cl_vs_avg_pre_cas`).
    pre_cas_rate_m_s: f64,
    /// Mean climb rate at constant CAS (`cl_vs_avg_cas_const`).
    cas_rate_m_s: f64,
    /// Mean climb rate at constant Mach (`cl_vs_avg_mach_const`).
    mach_rate_m_s: f64,
    /// Constant CAS of the descent (`de_v_cas_const`), m/s.
    descent_cas_m_s: f64,
    /// Constant Mach of the descent (`de_v_mach_const`).
    descent_mach: f64,
    /// Final-approach airspeed (`fa_va_avg`), m/s.
    approach_speed_m_s: f64,
    /// Final-approach descent rate magnitude (`fa_vs_avg`), m/s.
    approach_rate_m_s: f64,
    /// MMO, 0 when not found.
    mmo: f64,
    /// VMO, kt (calibrated; the TCDS wording is quoted per type), 0 when not found.
    vmo_kt: f64,
    /// Provenance of `mmo` / `vmo_kt`.
    limits_note: &'static str,
}

// WRAP `a343.txt`, A340-300.
const A343: Ladder = Ladder {
    proxy_note: "OpenAP wrap a343 (A340-300), own type",
    climb_cas_m_s: 155.0,
    climb_mach: 0.78,
    initial_climb_rate_m_s: 6.44,
    pre_cas_rate_m_s: 6.54,
    cas_rate_m_s: 6.59,
    mach_rate_m_s: 4.08,
    descent_cas_m_s: 154.0,
    descent_mach: 0.81,
    approach_speed_m_s: 74.0,
    approach_rate_m_s: 3.64,
    mmo: 0.86,
    vmo_kt: 0.0,
    limits_note: "MMO 0.86: LOW confidence, not read from EASA TCDS A.015 (incident summary and OpenAP a343.yml); VMO not found",
};

// WRAP `a388.txt`, A380-800.
const A388: Ladder = Ladder {
    proxy_note: "OpenAP wrap a388 (A380-800), own type",
    climb_cas_m_s: 163.0,
    climb_mach: 0.84,
    initial_climb_rate_m_s: 5.65,
    pre_cas_rate_m_s: 7.85,
    cas_rate_m_s: 7.51,
    mach_rate_m_s: 5.56,
    descent_cas_m_s: 154.0,
    descent_mach: 0.83,
    approach_speed_m_s: 73.0,
    approach_rate_m_s: 3.71,
    mmo: 0.89,
    vmo_kt: 0.0,
    limits_note: "MMO 0.89: LOW confidence, EASA TCDS A.110 refers to the AFM; value from OpenAP a388.yml and secondary pages; VMO not found",
};

// WRAP `b744.txt`, B747-400.
const B744: Ladder = Ladder {
    proxy_note: "OpenAP wrap b744 (B747-400), own type",
    climb_cas_m_s: 168.0,
    climb_mach: 0.84,
    initial_climb_rate_m_s: 9.24,
    pre_cas_rate_m_s: 8.66,
    cas_rate_m_s: 8.77,
    mach_rate_m_s: 6.72,
    descent_cas_m_s: 152.0,
    descent_mach: 0.83,
    approach_speed_m_s: 79.0,
    approach_rate_m_s: 4.03,
    mmo: 0.92,
    vmo_kt: 375.0,
    limits_note: "EASA TCDS IM.A.196 Issue 19, 747-400 Air Speeds: 375/0.92 (the sheet labels it KEAS in one place and KCAS in others)",
};

// WRAP `b789.txt`, B787-9.
const B789: Ladder = Ladder {
    proxy_note: "OpenAP wrap b789 (B787-9), own type",
    climb_cas_m_s: 163.0,
    climb_mach: 0.84,
    initial_climb_rate_m_s: 10.6,
    pre_cas_rate_m_s: 9.46,
    cas_rate_m_s: 8.68,
    mach_rate_m_s: 6.05,
    descent_cas_m_s: 154.0,
    descent_mach: 0.84,
    approach_speed_m_s: 77.0,
    approach_rate_m_s: 4.15,
    mmo: 0.90,
    vmo_kt: 350.0,
    limits_note: "EASA TCDS IM.A.115 Issue 30, 787-9: VMO/MMO = 350 KEAS / 0.90 M (taken as 350 kt CAS, equal to EAS at low altitude)",
};

// WRAP `e190.txt`.
const E190: Ladder = Ladder {
    proxy_note: "OpenAP wrap e190 (first-generation E190) as the nearest documented regional jet; NOT data for this type",
    climb_cas_m_s: 140.0,
    climb_mach: 0.75,
    initial_climb_rate_m_s: 11.19,
    pre_cas_rate_m_s: 10.44,
    cas_rate_m_s: 8.93,
    mach_rate_m_s: 4.82,
    descent_cas_m_s: 148.0,
    descent_mach: 0.77,
    approach_speed_m_s: 70.0,
    approach_rate_m_s: 3.57,
    mmo: 0.0,
    vmo_kt: 0.0,
    limits_note: "MMO/VMO not in the EASA TCDS (see Airplane Flight Manual): undeclared",
};

// WRAP `a320.txt`.
const A320_PROXY: Ladder = Ladder {
    proxy_note:
        "OpenAP wrap a320 as the nearest documented 150-seat narrowbody; NOT data for this type",
    climb_cas_m_s: 151.0,
    climb_mach: 0.78,
    initial_climb_rate_m_s: 12.59,
    pre_cas_rate_m_s: 10.25,
    cas_rate_m_s: 8.43,
    mach_rate_m_s: 5.28,
    descent_cas_m_s: 144.0,
    descent_mach: 0.77,
    approach_speed_m_s: 72.0,
    approach_rate_m_s: 3.55,
    mmo: 0.0,
    vmo_kt: 0.0,
    limits_note: "no public C919 type data sheet found: MMO/VMO undeclared",
};

// WRAP `a332.txt`.
const A332_PROXY: Ladder = Ladder {
    proxy_note: "OpenAP wrap a332 (A330-200) as the nearest documented widebody twin for a tri-jet; NOT data for this type",
    climb_cas_m_s: 152.0,
    climb_mach: 0.81,
    initial_climb_rate_m_s: 12.48,
    pre_cas_rate_m_s: 9.53,
    cas_rate_m_s: 7.98,
    mach_rate_m_s: 5.23,
    descent_cas_m_s: 152.0,
    descent_mach: 0.81,
    approach_speed_m_s: 72.0,
    approach_rate_m_s: 3.62,
    mmo: 0.88,
    vmo_kt: 0.0,
    limits_note: "MMO 0.88: LOW confidence, a search snippet of FAA TCDS A22WE Rev 5 (DC-10 and MD-11 not separated), A22WE itself not opened; VMO not found",
};

// WRAP `b789.txt` again, for the synthetic reference aircraft.
const AVE_PROXY: Ladder = Ladder {
    proxy_note: "OpenAP wrap b789 (B787-9) as the nearest documented M0.84-0.85 long-haul twin for the synthetic AVE; NOT data for this type",
    mmo: 0.0,
    vmo_kt: 0.0,
    limits_note: "synthetic aircraft: MMO/VMO undeclared",
    ..B789
};

/// A220-300, same WRAP type as E195-E2 but with the A220's EASA TCDS limits.
const A220_PROXY: Ladder = Ladder {
    mmo: 0.82,
    vmo_kt: 330.0,
    limits_note: "EASA TCDS IM.A.570 Issue 25: MMO 0.82 above 27,500 ft; VMO 330 kt from 10,000 to 27,500 ft (300 kt below 8,000 ft is not modelled)",
    ..E190
};

fn ladder_for(preset_name: &str) -> Option<&'static Ladder> {
    match preset_name {
        "A220-300" => Some(&A220_PROXY),
        "A340-300" => Some(&A343),
        "A380-800" => Some(&A388),
        "B787-9" => Some(&B789),
        "B747-400" => Some(&B744),
        "C919" => Some(&A320_PROXY),
        "E195-E2" => Some(&E190),
        "DC-10" => Some(&A332_PROXY),
        "AVE" => Some(&AVE_PROXY),
        _ => None,
    }
}

/// The source and caveats of a preset's ladder, for reports and tests; `None`
/// for a preset that declares its own hand-written schedule.
#[cfg(test)]
pub fn ladder_provenance(preset_name: &str) -> Option<(&'static str, &'static str)> {
    ladder_for(preset_name).map(|ladder| (ladder.proxy_note, ladder.limits_note))
}

/// Overwrite the speed schedule of `profile` with the preset's CAS/Mach
/// ladder, when it has one. A no-op for the presets with hand-written
/// schedules (A320-200, ATR72-600, A400M).
///
/// `cruise_altitude_m` is the preset's declared cruise level.
pub(in crate::presets) fn apply_registered_ladder(
    preset: &AircraftPreset,
    profile: &mut MissionProfileConfig,
    cruise_altitude_m: f64,
) {
    let Some(ladder) = ladder_for(preset.name) else {
        return;
    };
    apply_ladder(ladder, profile, cruise_altitude_m);
}

fn apply_ladder(ladder: &Ladder, profile: &mut MissionProfileConfig, cruise_altitude_m: f64) {
    profile.climb_descent_speed_reference = SpeedReference::CalibratedAirspeed;
    profile.climb_mach = ladder.climb_mach;
    profile.descent_mach = ladder.descent_mach;
    profile.max_operating_mach = ladder.mmo;
    profile.max_operating_cas_m_s = ladder.vmo_kt * KNOT_M_S;

    // Climb: 250 kt to 1,500 ft AGL and 10,000 ft, the type's CAS to the
    // crossover, the type's Mach above it.
    let crossover_m = crate::mission::crossover_altitude_m(ladder.climb_cas_m_s, ladder.climb_mach)
        .unwrap_or(SPEED_LIMIT_ALTITUDE_M);
    let speed_limit_fraction = (SPEED_LIMIT_ALTITUDE_M / cruise_altitude_m).clamp(0.05, 0.9);
    profile.takeoff_altitude_gain_m = 1_500.0 * FOOT_M;
    // The take-off segment is flown at the 250 kt climb-out speed rather than
    // V2: the integrator charges a speed change at a segment boundary only
    // after the first segment, and one CAS sub-rung (about 30 s) cannot pay
    // the V2 -> 250 kt acceleration of a 400 t aeroplane while the planned
    // climb rate keeps priority (B747-400: 1.9 GJ). The acceleration energy
    // is therefore NOT charged (about 30 kg of fuel-equivalent for a 70 t
    // aeroplane, about 150 kg for a 400 t one; recorded limitation).
    profile.takeoff_air_speed_m_s = SPEED_LIMIT_CAS_M_S;
    profile.takeoff_climb_rate_m_s = ladder.initial_climb_rate_m_s;
    profile.initial_climb_altitude_fraction = speed_limit_fraction;
    profile.initial_climb_air_speed_m_s = SPEED_LIMIT_CAS_M_S;
    // NOT the WRAP pre-CAS rate (`ladder.pre_cas_rate_m_s`): the engine
    // deck's maximum-climb thrust correlation takes its representative rate
    // from this field (`max_climb_rate_ft_min(initial_climb_rate_m_s)`), and
    // 8-10 m/s lowers the modelled climb thrust enough that the B747-400
    // cannot pay its acceleration to 250 kt CAS. The shared default (12 m/s)
    // is kept until the deck is decoupled from the flown rate; the
    // integrator caps a rate the rating cannot deliver.
    profile.initial_climb_rate_m_s = DECK_REFERENCE_CLIMB_RATE_M_S;
    // No cruise leg at the 10,000 ft speed-limit level: the native schedule
    // flies a cruise leg wherever the climb before it ends, and 28 % of the
    // distance at 10,000 ft exhausts the fuel of a long-haul mission. The
    // first leg's share moves to the other two in their default proportion;
    // the segment model flies its cruise at the top of the ladder either way.
    let (second, third) = (
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    );
    profile.cruise_1_distance_fraction = 0.0;
    profile.cruise_2_distance_fraction = second / (second + third);
    profile.cruise_3_distance_fraction = third / (second + third);
    profile.step_climb_1_altitude_fraction =
        (crossover_m / cruise_altitude_m).clamp(speed_limit_fraction + 0.02, 0.95);
    profile.step_climb_1_air_speed_m_s = ladder.climb_cas_m_s;
    profile.step_climb_1_rate_m_s = ladder.cas_rate_m_s;
    profile.step_climb_2_air_speed_m_s = ladder.climb_cas_m_s;
    profile.step_climb_2_rate_m_s = ladder.mach_rate_m_s;

    // Descent: Mach to the descent crossover, CAS to 10,000 ft, then the
    // speed limit, the intermediate approach speed, and the final approach.
    let descent_crossover_m =
        crate::mission::crossover_altitude_m(ladder.descent_cas_m_s, ladder.descent_mach)
            .unwrap_or(SPEED_LIMIT_ALTITUDE_M);
    let mut top_m = cruise_altitude_m;
    let rungs = [
        (descent_crossover_m / FOOT_M, ladder.descent_cas_m_s),
        (10_000.0, ladder.descent_cas_m_s),
        (5_000.0, SPEED_LIMIT_CAS_M_S),
        (3_000.0, APPROACH_INTERMEDIATE_CAS_M_S),
    ];
    let caps = profile.descent_caps();
    let mut resolved = [(0.0, 0.0, 0.0); 4];
    for (slot, (altitude_ft, cas_m_s)) in resolved.iter_mut().zip(rungs) {
        let bottom_m = (altitude_ft * FOOT_M).min(top_m);
        let mid_m = 0.5 * (top_m + bottom_m);
        let ambient = alas_atmo::us1976_compute_values(mid_m, 0.0);
        let tas_m_s = caps
            .true_airspeed_m_s(
                SpeedReference::CalibratedAirspeed,
                cas_m_s,
                ambient.pressure_pa,
                ambient.temperature_k,
            )
            .unwrap_or(cas_m_s);
        *slot = (
            altitude_ft,
            cas_m_s,
            tas_m_s * DESCENT_PATH_DEG.to_radians().sin(),
        );
        top_m = bottom_m;
    }
    profile.descent_1_altitude_ft = resolved[0].0;
    profile.descent_1_air_speed_m_s = resolved[0].1;
    profile.descent_1_rate_m_s = resolved[0].2;
    profile.descent_2_altitude_ft = resolved[1].0;
    profile.descent_2_air_speed_m_s = resolved[1].1;
    profile.descent_2_rate_m_s = resolved[1].2;
    profile.descent_3_altitude_ft = resolved[2].0;
    profile.descent_3_air_speed_m_s = resolved[2].1;
    profile.descent_3_rate_m_s = resolved[2].2;
    profile.descent_4_altitude_ft = resolved[3].0;
    profile.descent_4_air_speed_m_s = resolved[3].1;
    profile.descent_4_rate_m_s = resolved[3].2;
    profile.landing_air_speed_m_s = ladder.approach_speed_m_s;
    profile.landing_descent_rate_m_s = ladder.approach_rate_m_s;
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_atmo::airspeed::mach_from_calibrated;

    const LADDER_PRESETS: [&str; 9] = [
        "A220-300", "A340-300", "A380-800", "B787-9", "B747-400", "C919", "E195-E2", "DC-10", "AVE",
    ];

    fn defaults(name: &str) -> (MissionProfileConfig, f64, f64) {
        let preset = crate::presets::get(name).unwrap();
        let operational = preset.operational_mission_defaults();
        (
            operational.profile,
            operational.cruise_altitude_m,
            operational.cruise_mach,
        )
    }

    #[test]
    fn every_default_ladder_preset_declares_a_cas_mach_ladder_with_its_provenance() {
        for name in LADDER_PRESETS {
            let (profile, _, _) = defaults(name);
            assert_eq!(
                profile.climb_descent_speed_reference,
                SpeedReference::CalibratedAirspeed,
                "{name}"
            );
            assert!(
                profile.climb_mach > 0.5 && profile.descent_mach > 0.5,
                "{name}"
            );
            let (source, limits) = ladder_provenance(name).unwrap();
            assert!(source.starts_with("OpenAP wrap"), "{name}: {source}");
            assert!(!limits.is_empty());
        }
        // The five presets without an OpenAP file are flagged as proxies.
        for name in ["A220-300", "C919", "E195-E2", "DC-10", "AVE"] {
            assert!(
                ladder_provenance(name).unwrap().0.contains("NOT data"),
                "{name}"
            );
        }
        for name in ["A340-300", "A380-800", "B787-9", "B747-400"] {
            assert!(
                ladder_provenance(name).unwrap().0.contains("own type"),
                "{name}"
            );
        }
    }

    #[test]
    fn the_hand_written_ladders_are_left_alone() {
        for name in ["A320-200", "ATR72-600", "A400M"] {
            assert!(ladder_provenance(name).is_none(), "{name}");
        }
    }

    #[test]
    fn the_schedule_never_exceeds_mmo_or_vmo_and_cruise_sits_below_mmo() {
        for name in LADDER_PRESETS {
            let (profile, cruise_m, cruise_mach) = defaults(name);
            if profile.max_operating_mach > 0.0 {
                assert!(
                    cruise_mach < profile.max_operating_mach,
                    "{name}: cruise M {cruise_mach} against MMO {}",
                    profile.max_operating_mach
                );
                assert!(profile.climb_mach <= profile.max_operating_mach, "{name}");
                assert!(profile.descent_mach <= profile.max_operating_mach, "{name}");
            }
            // Every CAS rung, resolved through the caps over its altitude
            // range, stays at or below MMO and VMO.
            let rungs = [
                (profile.initial_climb_air_speed_m_s, profile.climb_caps()),
                (profile.step_climb_1_air_speed_m_s, profile.climb_caps()),
                (profile.step_climb_2_air_speed_m_s, profile.climb_caps()),
                (profile.descent_1_air_speed_m_s, profile.descent_caps()),
                (profile.descent_3_air_speed_m_s, profile.descent_caps()),
            ];
            for (cas_m_s, caps) in rungs {
                if caps.max_cas_m_s > 0.0 {
                    assert!(
                        cas_m_s <= caps.max_cas_m_s,
                        "{name}: {cas_m_s} m/s CAS above VMO"
                    );
                }
                for altitude_m in (0..=(cruise_m as i32)).step_by(250) {
                    let ambient = alas_atmo::us1976_compute_values(f64::from(altitude_m), 0.0);
                    let tas = caps
                        .true_airspeed_m_s(
                            SpeedReference::CalibratedAirspeed,
                            cas_m_s,
                            ambient.pressure_pa,
                            ambient.temperature_k,
                        )
                        .unwrap();
                    let mach = tas / ambient.speed_of_sound_m_s;
                    let limit = if caps.max_mach > 0.0 {
                        caps.max_mach
                    } else {
                        1.0
                    };
                    assert!(mach <= limit + 1.0e-9, "{name}: M {mach} at {altitude_m} m");
                }
            }
        }
    }

    #[test]
    fn the_climb_crossover_sits_between_the_two_rungs_and_the_ladder_is_ordered() {
        for name in LADDER_PRESETS {
            let (profile, cruise_m, _) = defaults(name);
            assert!(
                profile.initial_climb_altitude_fraction < profile.step_climb_1_altitude_fraction
            );
            assert!(profile.step_climb_1_altitude_fraction <= 0.95);
            let crossover_m = crate::mission::crossover_altitude_m(
                profile.step_climb_1_air_speed_m_s,
                profile.climb_mach,
            )
            .unwrap();
            let boundary_m = profile.step_climb_1_altitude_fraction * cruise_m;
            // Either the rung boundary is the crossover, or the crossover is
            // above the cruise level (the Mach hold never engages).
            assert!(
                (boundary_m - crossover_m).abs() < 1.0 || crossover_m > boundary_m,
                "{name}: boundary {boundary_m} m, crossover {crossover_m} m"
            );
            let alts = [
                profile.descent_1_altitude_ft,
                profile.descent_2_altitude_ft,
                profile.descent_3_altitude_ft,
                profile.descent_4_altitude_ft,
            ];
            assert!(
                alts.windows(2).all(|pair| pair[1] <= pair[0]),
                "{name}: {alts:?}"
            );
            // Mach at the CAS ceiling of the climb is below the schedule's
            // Mach at the first rung.
            let ambient = alas_atmo::us1976_compute_values(SPEED_LIMIT_ALTITUDE_M, 0.0);
            let mach = mach_from_calibrated(SPEED_LIMIT_CAS_M_S, ambient.pressure_pa).unwrap();
            assert!(mach < profile.climb_mach, "{name}");
        }
    }

    #[test]
    fn descent_rungs_follow_the_three_degree_path_at_their_own_true_airspeed() {
        for name in LADDER_PRESETS {
            let (profile, cruise_m, _) = defaults(name);
            // Rung 3 is wholly below the Mach crossover: CAS only, so its
            // rate over the rung midpoint TAS is sin(3 degrees) exactly.
            let mid_m =
                0.5 * (profile.descent_2_altitude_ft + profile.descent_3_altitude_ft) * FOOT_M;
            let ambient = alas_atmo::us1976_compute_values(mid_m, 0.0);
            let tas = profile
                .descent_caps()
                .true_airspeed_m_s(
                    SpeedReference::CalibratedAirspeed,
                    profile.descent_3_air_speed_m_s,
                    ambient.pressure_pa,
                    ambient.temperature_k,
                )
                .unwrap();
            let angle_deg = (profile.descent_3_rate_m_s / tas).asin().to_degrees();
            assert!(
                (angle_deg - DESCENT_PATH_DEG).abs() < 0.05,
                "{name}: {angle_deg}"
            );
            assert!(cruise_m > 5_000.0 * FOOT_M);
        }
    }

    #[test]
    fn take_off_and_approach_speeds_are_plausible_for_a_transport() {
        for name in LADDER_PRESETS {
            let (profile, _, _) = defaults(name);
            assert!(
                (60.0..140.0).contains(&profile.takeoff_air_speed_m_s),
                "{name}: V2 {} m/s",
                profile.takeoff_air_speed_m_s
            );
            assert!(
                (60.0..90.0).contains(&profile.landing_air_speed_m_s),
                "{name}"
            );
        }
    }
}
