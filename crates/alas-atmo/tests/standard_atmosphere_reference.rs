// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The atmosphere model against the printed tables of the standard it
//! implements, and its two entry points against each other.
//!
//! Reference: U.S. Standard Atmosphere, 1976 (NOAA, NASA, USAF; NOAA-S/T
//! 76-1562), Part 4, Table I (temperature, pressure and density as a
//! function of geopotential and geometric altitude, SI units) and Table III
//! (speed of sound and viscosity). The same rows by geopotential altitude
//! are those of the ICAO Standard Atmosphere (ICAO Doc 7488, 3rd edition),
//! which is identical to the U.S. standard below 32 km.
//!
//! The numbers below are transcribed from the tables at their printed
//! precision (five significant figures in pressure and density, 0.001 K,
//! 0.001 m/s); they are not produced by this crate.

use alas_atmo::{
    geopotential_from_geometric, us1976_compute_values, us1976_values_at_geometric, Atmosphere,
    EFFECTIVE_EARTH_RADIUS_M, GAS_CONSTANT_AIR,
};

/// One printed table row: altitude, m; temperature, K; pressure, Pa;
/// density, kg/m^3; speed of sound, m/s.
struct Row {
    altitude_m: f64,
    temperature_k: f64,
    pressure_pa: f64,
    density_kg_m3: f64,
    speed_of_sound_m_s: f64,
}

/// Rows entered by geopotential altitude `H`.
const BY_GEOPOTENTIAL: [Row; 5] = [
    Row {
        altitude_m: 0.0,
        temperature_k: 288.150,
        pressure_pa: 1.01325e5,
        density_kg_m3: 1.2250,
        speed_of_sound_m_s: 340.294,
    },
    Row {
        altitude_m: 5_000.0,
        temperature_k: 255.650,
        pressure_pa: 5.4020e4,
        density_kg_m3: 7.3612e-1,
        speed_of_sound_m_s: 320.529,
    },
    Row {
        altitude_m: 11_000.0,
        temperature_k: 216.650,
        pressure_pa: 2.2632e4,
        density_kg_m3: 3.6392e-1,
        speed_of_sound_m_s: 295.070,
    },
    Row {
        altitude_m: 15_000.0,
        temperature_k: 216.650,
        pressure_pa: 1.2045e4,
        density_kg_m3: 1.9367e-1,
        speed_of_sound_m_s: 295.070,
    },
    Row {
        altitude_m: 20_000.0,
        temperature_k: 216.650,
        pressure_pa: 5.4749e3,
        density_kg_m3: 8.8035e-2,
        speed_of_sound_m_s: 295.070,
    },
];

/// Rows entered by geometric altitude `Z`.
const BY_GEOMETRIC: [Row; 5] = [
    Row {
        altitude_m: 0.0,
        temperature_k: 288.150,
        pressure_pa: 1.01325e5,
        density_kg_m3: 1.2250,
        speed_of_sound_m_s: 340.294,
    },
    Row {
        altitude_m: 5_000.0,
        temperature_k: 255.676,
        pressure_pa: 5.4048e4,
        density_kg_m3: 7.3643e-1,
        speed_of_sound_m_s: 320.545,
    },
    Row {
        altitude_m: 11_000.0,
        temperature_k: 216.774,
        pressure_pa: 2.2700e4,
        density_kg_m3: 3.6480e-1,
        speed_of_sound_m_s: 295.154,
    },
    Row {
        altitude_m: 15_000.0,
        temperature_k: 216.650,
        pressure_pa: 1.2112e4,
        density_kg_m3: 1.9475e-1,
        speed_of_sound_m_s: 295.070,
    },
    Row {
        altitude_m: 20_000.0,
        temperature_k: 216.650,
        pressure_pa: 5.5293e3,
        density_kg_m3: 8.8910e-2,
        speed_of_sound_m_s: 295.070,
    },
];

/// Half a unit of the last printed digit of a five-figure mantissa (5e-5
/// for a mantissa of 1.0000) with a margin for the table's own rounding of
/// its inputs. The requirement on the model is 1e-4 (0.01 %) in density.
const RELATIVE_TOLERANCE: f64 = 5.5e-5;
/// Kelvin: half the last printed digit.
const TEMPERATURE_TOLERANCE_K: f64 = 6.0e-4;
/// m/s: the printed speed of sound of the isothermal layer is 295.069 or
/// 295.070 depending on the table, one unit of the last digit.
const SPEED_OF_SOUND_TOLERANCE_M_S: f64 = 1.5e-3;

fn assert_row(label: &str, atmosphere: Atmosphere, row: &Row) {
    let relative = |value: f64, reference: f64| ((value - reference) / reference).abs();
    assert!(
        (atmosphere.temperature() - row.temperature_k).abs() < TEMPERATURE_TOLERANCE_K,
        "{label} {} m: T {} K against {} K",
        row.altitude_m,
        atmosphere.temperature(),
        row.temperature_k
    );
    assert!(
        relative(atmosphere.pressure(), row.pressure_pa) < RELATIVE_TOLERANCE,
        "{label} {} m: p {} Pa against {} Pa",
        row.altitude_m,
        atmosphere.pressure(),
        row.pressure_pa
    );
    assert!(
        relative(atmosphere.density(), row.density_kg_m3) < RELATIVE_TOLERANCE,
        "{label} {} m: rho {} kg/m^3 against {} kg/m^3",
        row.altitude_m,
        atmosphere.density(),
        row.density_kg_m3
    );
    assert!(
        (atmosphere.speed_of_sound() - row.speed_of_sound_m_s).abs() < SPEED_OF_SOUND_TOLERANCE_M_S,
        "{label} {} m: a {} m/s against {} m/s",
        row.altitude_m,
        atmosphere.speed_of_sound(),
        row.speed_of_sound_m_s
    );
}

#[test]
fn the_model_reproduces_the_printed_table_by_geopotential_altitude() {
    for row in &BY_GEOPOTENTIAL {
        assert_row("geopotential", Atmosphere::new(row.altitude_m), row);
        assert_eq!(
            Atmosphere::new(row.altitude_m),
            Atmosphere::from_geopotential(row.altitude_m)
        );
    }
}

#[test]
fn the_model_reproduces_the_printed_table_by_geometric_altitude() {
    for row in &BY_GEOMETRIC {
        assert_row("geometric", Atmosphere::from_geometric(row.altitude_m), row);
    }
}

#[test]
fn sea_level_viscosity_matches_the_printed_value() {
    // Table III sea-level row: mu = 1.7894e-5 N s/m^2, nu = 1.4607e-5 m^2/s.
    let sea_level = Atmosphere::new(0.0);
    assert!((sea_level.dynamic_viscosity() / 1.7894e-5 - 1.0).abs() < 5e-5);
    assert!((sea_level.kinematic_viscosity() / 1.4607e-5 - 1.0).abs() < 5e-5);
}

#[test]
fn the_object_and_function_entry_points_are_one_model() {
    // Every 250 m from -1 km to 25 km, on a standard, a hot and a cold day:
    // not close, identical.
    for step in 0..=104 {
        let altitude_m = -1_000.0 + f64::from(step) * 250.0;
        for deviation_k in [0.0, 20.0, -15.0] {
            let object = Atmosphere::new(altitude_m).with_temperature_deviation(deviation_k);
            let function = us1976_compute_values(altitude_m, deviation_k);
            assert_eq!(object.pressure(), function.pressure_pa);
            assert_eq!(object.temperature(), function.temperature_k);
            assert_eq!(object.density(), function.density_kg_m3);
            assert_eq!(object.speed_of_sound(), function.speed_of_sound_m_s);
            assert_eq!(object.dynamic_viscosity(), function.dynamic_viscosity_pa_s);
            assert_eq!(
                object.kinematic_viscosity(),
                function.kinematic_viscosity_m2_s
            );
        }
    }
}

#[test]
fn a_true_airspeed_converts_back_to_the_mach_number_that_formed_it() {
    // Mach to true airspeed with the object form, back to Mach with the
    // function form and with the airspeed helpers, at cruise altitudes
    // either side of the tropopause.
    for altitude_m in [
        3_000.0, 7_620.0, 9_600.0, 10_668.0, 11_000.0, 11_278.0, 12_497.0,
    ] {
        for mach in [0.45, 0.78, 0.85] {
            let aero_side = Atmosphere::new(altitude_m);
            let tas_m_s = mach * aero_side.speed_of_sound();
            let mission_side = us1976_compute_values(altitude_m, 0.0);
            let recovered = tas_m_s / mission_side.speed_of_sound_m_s;
            assert!(
                (recovered - mach).abs() < 1e-15,
                "{altitude_m} m: Mach {mach} came back as {recovered}"
            );
            let helper_tas_m_s =
                alas_atmo::true_from_mach(mach, mission_side.temperature_k).unwrap_or(f64::NAN);
            assert!(
                (helper_tas_m_s / tas_m_s - 1.0).abs() < 1e-14,
                "{altitude_m} m: {helper_tas_m_s} m/s against {tas_m_s} m/s"
            );
            // Same dynamic pressure on both sides, so the same lift
            // coefficient for the same weight.
            let q_aero = 0.5 * aero_side.density() * tas_m_s * tas_m_s;
            let q_mission = 0.5 * mission_side.density_kg_m3 * tas_m_s * tas_m_s;
            assert_eq!(q_aero, q_mission);
        }
    }
}

#[test]
fn the_geopotential_conversion_is_the_stated_one() {
    // H = r0 Z / (r0 + Z) with the standard's effective Earth radius
    // 6 356 766 m (equation 18). Table I prints H = 10 981 m beside
    // Z = 11 000 m and H = 19 937 m beside Z = 20 000 m.
    assert_eq!(EFFECTIVE_EARTH_RADIUS_M, 6_356_766.0);
    let geopotential_m = geopotential_from_geometric(11_000.0);
    assert!((geopotential_m - 6_356_766.0 * 11_000.0 / 6_367_766.0).abs() < 1e-9);
    assert!((geopotential_m - 10_981.0).abs() < 0.5);
    assert!((geopotential_from_geometric(20_000.0) - 19_937.0).abs() < 0.5);
    // The function form at a geometric altitude is the same conversion.
    assert_eq!(
        us1976_values_at_geometric(11_000.0, 0.0),
        us1976_compute_values(geopotential_m, 0.0)
    );
    // The gas constant is the published 287.052 87 J/(kg K), R*/M0 with the
    // molar mass 28.964 420 kg/kmol, at its printed precision.
    assert!((GAS_CONSTANT_AIR - 287.052_87).abs() < 5e-6);
    assert!((GAS_CONSTANT_AIR - 8_314.32 / 28.964_420).abs() < 5e-6);
}
