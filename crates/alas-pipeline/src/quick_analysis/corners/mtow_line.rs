// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Points flown on the takeoff-mass line between payload-range corners B and C.
//!
//! From B to C every point carries the takeoff-mass limit: payload is traded
//! for fuel kilogram for kilogram until the tank is full at C. At a fixed
//! takeoff mass the range grows faster than the fuel (the Breguet logarithm:
//! fuel burned late in the flight is carried by a lighter aircraft), so the
//! straight B-C chord lies above the flown curve and a payload read off the
//! chord reads long. Each point here is flown with the corners' own rule
//! ([`super::corner_ranges_at`]): the report's mission model flown as a range
//! search at the same cruise level and speed, the configured fuel policy and
//! taxi fuel, every trip on the plan frozen for its own takeoff mass and range.
//!
//! [`MTOW_LINE_INTERIOR_POINTS`] points split B-C into equal payload steps. A
//! caller that knows the zero-fuel mass a source quotes its range at adds that
//! point too ([`MtowLinePoint::stated`]); a stated mass off the line is an
//! error, never moved onto it.

use alas_config::{AlasConfig, FuelPolicyConfig};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use alas_opt::mdo::PlannedTrips;

use super::{
    corner_loads, range_search_model, CornerLoad, CornerMasses, CORNER_TOLERANCE_M,
    SECONDS_PER_MINUTE,
};
use crate::full_analysis::AnalysisReport;

/// Interior points flown between B and C (engineering choice). Each costs one
/// corner range search. With four, the chord between neighbouring flown points
/// stays within 0.1 % of the curve on a long-range freighter's B-C segment
/// (Breguet curve through the corners of a 747-400F run: 4.8 nmi at most,
/// against 100 nmi for the bare B-C chord).
pub const MTOW_LINE_INTERIOR_POINTS: usize = 4;

/// Payload difference below which corners B and C are one point, kg.
const SAME_POINT_KG: f64 = 1.0;

/// One point flown on the takeoff-mass line, all masses kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MtowLinePoint {
    /// Payload.
    pub payload_kg: f64,
    /// Zero-fuel mass: operating empty mass plus payload.
    pub zero_fuel_mass_kg: f64,
    /// Fuel at brake release.
    pub fuel_kg: f64,
    /// Mass at brake release: the takeoff-mass limit.
    pub tow_kg: f64,
    /// Still-air range, m.
    pub range_m: f64,
    /// Reserve fuel the plan holds back.
    pub reserve_fuel_kg: f64,
    /// Whether this is the zero-fuel mass the caller stated rather than an
    /// equal-step point.
    pub stated: bool,
}

/// The loads strictly between B and C in equal payload steps, with the load
/// of `stated_zfw_kg` when given, in falling payload; each flags whether it is
/// the stated one. Empty when B and C coincide (a tank-limited B) and nothing
/// is stated.
///
/// # Errors
///
/// The reason when a stated zero-fuel mass is not on the line from B to C.
fn mtow_line_loads(
    masses: &CornerMasses,
    taxi_kg: f64,
    interior: usize,
    stated_zfw_kg: Option<f64>,
) -> Result<Vec<(CornerLoad, bool)>, String> {
    let [b, c, _] = corner_loads(masses, taxi_kg);
    let on_line = b.fuel_kg > 0.0 && b.payload_kg - c.payload_kg > SAME_POINT_KG;
    // The corner rule (`corner_loads`) applied to one payload.
    let budget_kg = (masses.mtow_kg - masses.oew_kg).max(0.0);
    let tank_kg = (masses.tank_capacity_kg - taxi_kg).max(0.0);
    let load = |payload_kg: f64| {
        let fuel_kg = tank_kg.min(budget_kg - payload_kg).max(0.0);
        CornerLoad {
            payload_kg,
            fuel_kg,
            tow_kg: masses.oew_kg + payload_kg + fuel_kg,
        }
    };
    let mut loads = Vec::with_capacity(interior + 1);
    if on_line {
        let step_kg = (b.payload_kg - c.payload_kg) / (interior + 1) as f64;
        for index in 1..=interior {
            loads.push((load(b.payload_kg - step_kg * index as f64), false));
        }
    }
    if let Some(zfw_kg) = stated_zfw_kg {
        let payload_kg = zfw_kg - masses.oew_kg;
        let inside = on_line && payload_kg <= b.payload_kg && payload_kg >= c.payload_kg;
        if !inside {
            return Err(format!(
                "stated zero-fuel mass {zfw_kg:.0} kg is not on the takeoff-mass line (zero-fuel mass {:.0} to {:.0} kg)",
                masses.oew_kg + c.payload_kg,
                masses.oew_kg + b.payload_kg,
            ));
        }
        loads.push((load(payload_kg), true));
        loads.sort_by(|left, right| right.0.payload_kg.total_cmp(&left.0.payload_kg));
    }
    Ok(loads)
}

/// Price `loads` with the corner range search under `policy` on `model`.
///
/// # Errors
///
/// The reason when the model cannot price a point.
fn fly(
    loads: &[(CornerLoad, bool)],
    oew_kg: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
) -> Result<Vec<MtowLinePoint>, String> {
    loads
        .iter()
        .map(|(load, stated)| {
            let found = max_range_with_reserves(
                load.tow_kg,
                load.fuel_kg,
                policy,
                model,
                CORNER_TOLERANCE_M,
            );
            if let RangeStatus::ModelFailed(message) = found.status {
                return Err(message);
            }
            Ok(MtowLinePoint {
                payload_kg: load.payload_kg,
                zero_fuel_mass_kg: oew_kg + load.payload_kg,
                fuel_kg: load.fuel_kg,
                tow_kg: load.tow_kg,
                range_m: found.range_m,
                reserve_fuel_kg: found.reserve_fuel_kg,
                stated: *stated,
            })
        })
        .collect()
}

/// The points flown on the takeoff-mass line of `masses` between corners B and
/// C, in falling payload: [`MTOW_LINE_INTERIOR_POINTS`] equal payload steps
/// and the stated zero-fuel mass when given, priced as
/// [`super::corner_ranges_at`] prices the corners with the same `config` and
/// `cruise_level_m`.
///
/// # Errors
///
/// The reason when the fuel model cannot be built, a point cannot be priced,
/// or the stated zero-fuel mass is not on the line.
pub fn mtow_line_points(
    config: &AlasConfig,
    report: &AnalysisReport,
    masses: &CornerMasses,
    cruise_level_m: Option<f64>,
    stated_zfw_kg: Option<f64>,
) -> Result<Vec<MtowLinePoint>, String> {
    let model = range_search_model(config, report, cruise_level_m)?;
    let policy = &config.fuel_policy;
    let taxi_kg = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| format!("taxi fuel flow is unavailable: {error}"))?
        * policy.taxi_time_min
        * SECONDS_PER_MINUTE;
    let loads = mtow_line_loads(masses, taxi_kg, MTOW_LINE_INTERIOR_POINTS, stated_zfw_kg)?;
    fly(&loads, masses.oew_kg, policy, &PlannedTrips(&model))
}

// Tests construct every fixture they assert on, so a failed unwrap is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::FuelScheme;
    use alas_mass::fuel_plan::{FuelModelError, LegEstimate};

    /// MTOW 100 t, OEW 50 t, maximum payload 20 t, tank 40 t.
    const MASSES: CornerMasses = CornerMasses {
        mtow_kg: 100_000.0,
        oew_kg: 50_000.0,
        max_payload_kg: 20_000.0,
        tank_capacity_kg: 40_000.0,
    };

    #[test]
    fn interior_points_split_the_line_in_equal_payload_steps_at_the_takeoff_limit() {
        // Taxi 200 kg: tank 39,800 kg at brake release, so C carries
        // 50,000 - 39,800 = 10,200 kg and the step is (20,000 - 10,200) / 5.
        let loads = mtow_line_loads(&MASSES, 200.0, 4, None).unwrap();
        let payloads: Vec<f64> = loads.iter().map(|(load, _)| load.payload_kg).collect();
        assert_eq!(payloads, [18_040.0, 16_080.0, 14_120.0, 12_160.0]);
        for (load, stated) in &loads {
            assert!(!stated);
            assert_eq!(load.tow_kg, 100_000.0);
            assert_eq!(load.fuel_kg, 50_000.0 - load.payload_kg);
        }
    }

    #[test]
    fn a_stated_zero_fuel_mass_is_flown_in_payload_order_and_never_moved_onto_the_line() {
        // ZFW 65,000 kg: payload 15,000 kg, fuel 35,000 kg, between 16,080 and 14,120 kg.
        let loads = mtow_line_loads(&MASSES, 200.0, 4, Some(65_000.0)).unwrap();
        let payloads: Vec<f64> = loads.iter().map(|(load, _)| load.payload_kg).collect();
        assert_eq!(payloads, [18_040.0, 16_080.0, 15_000.0, 14_120.0, 12_160.0]);
        assert!(loads[2].1);
        assert_eq!(loads[2].0.fuel_kg, 35_000.0);
        // ZFW 75,000 kg is above B's 70,000 kg; 59,000 kg is below C's 60,200 kg.
        assert!(mtow_line_loads(&MASSES, 200.0, 4, Some(75_000.0)).is_err());
        assert!(mtow_line_loads(&MASSES, 200.0, 4, Some(59_000.0)).is_err());
    }

    #[test]
    fn a_tank_limited_b_has_no_line() {
        // Tank 25,000 kg: B already carries the full tank (24,800 kg after
        // taxi) under the 30,000 kg budget left by the maximum payload, so C is B.
        let masses = CornerMasses {
            tank_capacity_kg: 25_000.0,
            ..MASSES
        };
        assert!(mtow_line_loads(&masses, 200.0, 4, None).unwrap().is_empty());
        assert!(mtow_line_loads(&masses, 200.0, 4, Some(65_000.0)).is_err());
    }

    /// Constant-speed Breguet cruise from brake release: trip fuel
    /// m0 (1 - exp(-R / K)) with K = V (L/D) / (g c) = 20,000 km; no taxi flow.
    struct Breguet;

    const BREGUET_K_M: f64 = 2.0e7;

    impl FuelBurnModel for Breguet {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: takeoff_mass_kg * (1.0 - (-range_m / BREGUET_K_M).exp()),
                time_s: range_m / 230.0,
            })
        }

        fn diversion(&self, _: f64, _: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.0,
                time_s: 0.0,
            })
        }

        fn holding_fuel_flow_kg_s(&self, _: f64, _: f64) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }

        fn cruise_fuel_flow_kg_s(&self, _: f64) -> Result<f64, FuelModelError> {
            Ok(1.0)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.0)
        }
    }

    #[test]
    fn flown_points_match_the_breguet_range_and_lie_below_the_chord() {
        // Trip fuel only, so the range is R = -K ln(1 - F / m0) at m0 = 100 t:
        // B (F 30 t) 7,133,498.9 m, C (F 40 t) 10,216,512.5 m; the stated
        // ZFW 65 t flies F 35 t to 8,615,658.3 m against the chord's
        // 8,675,005.7 m (the chord reads 59 km long).
        let policy = FuelPolicyConfig {
            scheme: FuelScheme::TripFuelOnly,
            ..Default::default()
        };
        let loads = mtow_line_loads(&MASSES, 0.0, 4, Some(65_000.0)).unwrap();
        let points = fly(&loads, MASSES.oew_kg, &policy, &Breguet).unwrap();
        let hand_m = [
            7_713_249.6,
            8_310_308.9,
            8_615_658.3,
            8_925_742.1,
            9_560_716.0,
        ];
        let (range_b_m, range_c_m) = (7_133_498.9, 10_216_512.5);
        for (point, hand_m) in points.iter().zip(hand_m) {
            assert!(
                point.range_m <= hand_m + 1.0 && point.range_m >= hand_m - CORNER_TOLERANCE_M,
                "{point:?} against {hand_m} m"
            );
            assert_eq!(point.zero_fuel_mass_kg, MASSES.oew_kg + point.payload_kg);
            let share = (20_000.0 - point.payload_kg) / 10_000.0;
            let chord_m = range_b_m + share * (range_c_m - range_b_m);
            assert!(
                point.range_m < chord_m - 20_000.0,
                "{point:?} chord {chord_m}"
            );
        }
        assert!(points[2].stated && points[2].payload_kg == 15_000.0);
        assert_eq!(points.iter().filter(|point| point.stated).count(), 1);
    }
}
