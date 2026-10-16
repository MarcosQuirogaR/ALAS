// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CAS/Mach climb and descent schedule and the operating-speed limits.
//!
//! Airliners climb and descend at a constant calibrated airspeed (CAS) up to
//! the *crossover altitude* and at a constant Mach number above it. At the
//! crossover the CAS and the Mach schedules describe the same flight
//! condition; above it a constant CAS would carry the Mach number beyond the
//! schedule (and, higher still, beyond MMO), so the Mach number is held
//! instead (Airbus, "Getting to Grips with Aircraft Performance", 2002,
//! climb and descent chapters). Equivalently, with `M_cas(h)` the Mach number
//! the scheduled CAS implies at pressure altitude `h` (which rises with `h`),
//! the Mach number flown is `min(M_cas(h), M_schedule, MMO)`, and the CAS is
//! additionally bounded by VMO.
//!
//! The CAS-to-Mach relation is the isentropic impact-pressure relation in
//! [`alas_atmo::airspeed`] (ICAO Doc 7488 / US Standard Atmosphere 1976 for
//! the ambient state). The true airspeed is `TAS = M a(T)` with `a` evaluated
//! at the actual static temperature, so a temperature offset changes the
//! speed of sound and nothing else here.
//!
//! One definition, read by the MDO mission model (`alas-opt`) and by the
//! native pseudospectral schedule (`alas-pipeline`, `alas-mission`), so the
//! two paths cannot disagree about where the crossover is.

use alas_atmo::airspeed::{
    mach_from_calibrated, true_from_calibrated, true_from_mach, AirspeedError,
};

use super::{MissionProfileConfig, SpeedReference};

/// The Mach hold and operating limits a climb or descent leg's speed is
/// resolved against. All three are optional; zero (the default) means "not
/// declared", and the resolution is then exactly the configured speed
/// reference.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpeedCaps {
    /// Mach flown above the crossover (the schedule's Mach).
    pub mach_hold: f64,
    /// Maximum operating Mach number, MMO.
    pub max_mach: f64,
    /// Maximum operating calibrated airspeed, VMO, m/s.
    pub max_cas_m_s: f64,
}

impl SpeedCaps {
    /// The binding Mach limit, `INFINITY` when none is declared.
    fn mach_limit(&self) -> f64 {
        [self.mach_hold, self.max_mach]
            .into_iter()
            .filter(|mach| mach.is_finite() && *mach > 0.0)
            .fold(f64::INFINITY, f64::min)
    }

    /// True airspeed to fly for a configured leg speed `configured_m_s`,
    /// stated per `reference`, at ambient pressure `ambient_pressure_pa` and
    /// temperature `ambient_temperature_k`.
    ///
    /// With no cap declared this is exactly
    /// [`crate::resolve_true_airspeed_m_s`].
    ///
    /// # Errors
    ///
    /// [`AirspeedError`] when the speed or ambient state is invalid or
    /// supersonic.
    pub fn true_airspeed_m_s(
        &self,
        reference: SpeedReference,
        configured_m_s: f64,
        ambient_pressure_pa: f64,
        ambient_temperature_k: f64,
    ) -> Result<f64, AirspeedError> {
        let limit = self.mach_limit();
        match reference {
            SpeedReference::TrueAirspeed => {
                if limit.is_finite() {
                    let ceiling = true_from_mach(limit, ambient_temperature_k)?;
                    Ok(configured_m_s.min(ceiling))
                } else {
                    Ok(configured_m_s)
                }
            }
            SpeedReference::CalibratedAirspeed => {
                let cas = if self.max_cas_m_s > 0.0 {
                    configured_m_s.min(self.max_cas_m_s)
                } else {
                    configured_m_s
                };
                if limit.is_finite() {
                    // A CAS whose Mach number would be supersonic at this
                    // pressure is simply held to the Mach limit.
                    let mach = match mach_from_calibrated(cas, ambient_pressure_pa) {
                        Ok(mach) => mach,
                        Err(AirspeedError::Supersonic { .. }) => f64::INFINITY,
                        Err(error) => return Err(error),
                    };
                    true_from_mach(mach.min(limit), ambient_temperature_k)
                } else {
                    true_from_calibrated(cas, ambient_pressure_pa, ambient_temperature_k)
                }
            }
        }
    }
}

impl MissionProfileConfig {
    /// The caps for a climb (and take-off) leg.
    pub fn climb_caps(&self) -> SpeedCaps {
        SpeedCaps {
            mach_hold: self.climb_mach,
            max_mach: self.max_operating_mach,
            max_cas_m_s: self.max_operating_cas_m_s,
        }
    }

    /// The caps for a descent (and landing) leg.
    pub fn descent_caps(&self) -> SpeedCaps {
        SpeedCaps {
            mach_hold: self.descent_mach,
            ..self.climb_caps()
        }
    }
}

/// Altitude, m, of the CAS/Mach crossover on a standard day: where a
/// constant `cas_m_s` implies Mach `mach`, in the altitude coordinate of the
/// mission atmosphere ([`alas_atmo::us1976_compute_values`]).
///
/// Found by bisection on the US Standard Atmosphere 1976 static pressure
/// (the Mach number implied by a fixed CAS increases monotonically with
/// altitude). The crossover depends on static pressure only, so it is a
/// pressure altitude and does not move with a temperature offset.
///
/// # Errors
///
/// [`AirspeedError`] for an invalid speed, an unreachable Mach number
/// (`mach` at or below the sea-level Mach implied by `cas_m_s`, or at or
/// above one) or an altitude outside the standard-atmosphere table.
pub fn crossover_altitude_m(cas_m_s: f64, mach: f64) -> Result<f64, AirspeedError> {
    const BISECTION_ITERATIONS: usize = 80;
    let mach_at = |altitude_m: f64| -> Result<f64, AirspeedError> {
        let ambient = alas_atmo::us1976_compute_values(altitude_m, 0.0);
        mach_from_calibrated(cas_m_s, ambient.pressure_pa)
    };
    let (mut low_m, mut high_m) = (0.0_f64, 25_000.0_f64);
    if mach_at(low_m)? >= mach {
        return Ok(low_m);
    }
    for _ in 0..BISECTION_ITERATIONS {
        let mid_m = 0.5 * (low_m + high_m);
        match mach_at(mid_m) {
            Ok(m) if m < mach => low_m = mid_m,
            _ => high_m = mid_m,
        }
    }
    Ok(0.5 * (low_m + high_m))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const KNOT_M_S: f64 = 1852.0 / 3600.0;
    const FOOT_M: f64 = 0.3048;

    fn ambient(altitude_m: f64) -> (f64, f64) {
        let values = alas_atmo::us1976_compute_values(altitude_m, 0.0);
        (values.pressure_pa, values.temperature_k)
    }

    #[test]
    fn crossover_of_300_kcas_and_mach_0_78_is_flight_level_293() {
        // Hand case of the physics audit (MIS-02): 300 KCAS reaches M 0.78 at
        // 29.3 kft. Tolerance 150 ft covers the rounding of the quoted value.
        let altitude_m = crossover_altitude_m(300.0 * KNOT_M_S, 0.78).unwrap();
        let flight_level = altitude_m / FOOT_M / 100.0;
        assert!(
            (flight_level - 293.0).abs() < 1.5,
            "crossover at FL{flight_level:.1}"
        );
        // And the Mach number there is the schedule's, by construction.
        let (pressure_pa, _) = ambient(altitude_m);
        let mach = mach_from_calibrated(300.0 * KNOT_M_S, pressure_pa).unwrap();
        assert!((mach - 0.78).abs() < 1.0e-9, "Mach at the crossover {mach}");
    }

    #[test]
    fn mach_hold_takes_over_above_the_crossover_and_cas_below_it() {
        let cas = 300.0 * KNOT_M_S;
        let caps = SpeedCaps {
            mach_hold: 0.78,
            ..SpeedCaps::default()
        };
        let crossover_m = crossover_altitude_m(cas, 0.78).unwrap();
        for (altitude_m, expect_mach_hold) in [
            (crossover_m - 3_000.0, false),
            (crossover_m + 1_000.0, true),
            (crossover_m + 3_000.0, true),
        ] {
            let (p, t) = ambient(altitude_m);
            let tas = caps
                .true_airspeed_m_s(SpeedReference::CalibratedAirspeed, cas, p, t)
                .unwrap();
            let cas_tas = true_from_calibrated(cas, p, t).unwrap();
            let mach_tas = true_from_mach(0.78, t).unwrap();
            let expected = if expect_mach_hold { mach_tas } else { cas_tas };
            assert!((tas - expected).abs() < 1.0e-9, "{altitude_m} m: {tas}");
        }
    }

    #[test]
    fn no_caps_is_the_plain_speed_reference_bit_for_bit() {
        let (p, t) = ambient(9_000.0);
        let caps = SpeedCaps::default();
        assert_eq!(
            caps.true_airspeed_m_s(SpeedReference::CalibratedAirspeed, 150.0, p, t)
                .unwrap(),
            true_from_calibrated(150.0, p, t).unwrap()
        );
        assert_eq!(
            caps.true_airspeed_m_s(SpeedReference::TrueAirspeed, 250.0, p, t)
                .unwrap(),
            250.0
        );
    }

    #[test]
    fn mmo_and_vmo_bound_every_resolved_speed() {
        let caps = SpeedCaps {
            mach_hold: 0.0,
            max_mach: 0.82,
            max_cas_m_s: 350.0 * KNOT_M_S,
        };
        for altitude_m in (0..=13_000).step_by(500) {
            let (p, t) = ambient(f64::from(altitude_m));
            // A deliberately excessive request: 400 KCAS, and 330 m/s TAS.
            let cas_tas = caps
                .true_airspeed_m_s(SpeedReference::CalibratedAirspeed, 400.0 * KNOT_M_S, p, t)
                .unwrap();
            let literal = caps
                .true_airspeed_m_s(SpeedReference::TrueAirspeed, 330.0, p, t)
                .unwrap();
            let a = 2.0 * true_from_mach(0.5, t).unwrap();
            assert!(
                cas_tas / a <= 0.82 + 1.0e-12,
                "{altitude_m} m: M {}",
                cas_tas / a
            );
            assert!(literal / a <= 0.82 + 1.0e-12);
            // 350 KCAS is supersonic at the top of the range; VMO is only a
            // meaningful bound where it is subsonic.
            if let Ok(vmo) = true_from_calibrated(350.0 * KNOT_M_S, p, t) {
                assert!(cas_tas <= vmo + 1.0e-9, "{altitude_m} m exceeds VMO");
            }
        }
    }

    #[test]
    fn a_temperature_offset_moves_true_airspeed_but_not_the_crossover() {
        let cas = 300.0 * KNOT_M_S;
        let caps = SpeedCaps {
            mach_hold: 0.78,
            ..SpeedCaps::default()
        };
        let altitude_m = 11_300.0;
        let (p, t) = ambient(altitude_m);
        let standard = caps
            .true_airspeed_m_s(SpeedReference::CalibratedAirspeed, cas, p, t)
            .unwrap();
        let hot = caps
            .true_airspeed_m_s(SpeedReference::CalibratedAirspeed, cas, p, t + 15.0)
            .unwrap();
        assert!((hot / standard - ((t + 15.0) / t).sqrt()).abs() < 1.0e-12);
    }
}
