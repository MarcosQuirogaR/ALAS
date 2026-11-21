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

/// Rows a [`LimitTable`] holds: enough for the type certificate charts
/// (the Learjet 60 VMO needs five).
pub const LIMIT_TABLE_ROWS: usize = 8;

/// An operating limit that varies with pressure altitude: `(pressure altitude
/// m, value)` rows in ascending altitude, a straight line between rows, the end
/// value held beyond either end, and a step as two rows at one altitude (the
/// lower value holds at the step). Empty means "not declared".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LimitTable {
    rows: [(f64, f64); LIMIT_TABLE_ROWS],
    len: usize,
}

impl LimitTable {
    /// The table of `rows`; `None` when there are more than
    /// [`LIMIT_TABLE_ROWS`], a row is not finite or not positive in value, or
    /// the altitudes descend.
    #[must_use]
    pub fn from_rows(rows: &[(f64, f64)]) -> Option<Self> {
        let ordered = rows.windows(2).all(|pair| pair[1].0 >= pair[0].0);
        let valid = rows
            .iter()
            .all(|(altitude, value)| altitude.is_finite() && value.is_finite() && *value > 0.0);
        if rows.len() > LIMIT_TABLE_ROWS || !ordered || !valid {
            return None;
        }
        let mut table = Self::default();
        table.rows[..rows.len()].copy_from_slice(rows);
        table.len = rows.len();
        Some(table)
    }

    /// Whether no row is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The limit at `pressure_altitude_m`; `None` for an empty table.
    #[must_use]
    pub fn at(&self, pressure_altitude_m: f64) -> Option<f64> {
        let rows = &self.rows[..self.len];
        let (first, last) = (rows.first()?, rows.last()?);
        if pressure_altitude_m <= first.0 {
            return Some(first.1);
        }
        if pressure_altitude_m >= last.0 {
            return Some(last.1);
        }
        let mut value = f64::INFINITY;
        for pair in rows.windows(2) {
            let ((h0, v0), (h1, v1)) = (pair[0], pair[1]);
            if pressure_altitude_m < h0 || pressure_altitude_m > h1 {
                continue;
            }
            let here = if h1 > h0 {
                v0 + (v1 - v0) * (pressure_altitude_m - h0) / (h1 - h0)
            } else {
                v0.min(v1)
            };
            value = value.min(here);
        }
        Some(value)
    }

    /// The smallest limit between `low_m` and `high_m` (the rows inside the
    /// band and its two ends); `None` for an empty table.
    #[must_use]
    pub fn min_between(&self, low_m: f64, high_m: f64) -> Option<f64> {
        let inside = self.rows[..self.len]
            .iter()
            .filter(|(altitude, _)| *altitude > low_m && *altitude < high_m)
            .filter_map(|(altitude, _)| self.at(*altitude));
        [self.at(low_m)?, self.at(high_m)?]
            .into_iter()
            .chain(inside)
            .reduce(f64::min)
    }
}

/// Pressure altitude, m, of static pressure `pressure_pa`: the altitude of the
/// mission atmosphere ([`alas_atmo::us1976_compute_values`], standard day)
/// at which the standard pressure is `pressure_pa`. Type certificate limits
/// are stated against it. Bisection between 0 and 30 km (to about 1 mm); the
/// ends are returned outside that band. The steps compare the pressure alone
/// ([`alas_atmo::us1976_pressure_pa`], the same number as the full state),
/// evaluated only near the answer
/// ([`alas_atmo::us1976_bisect_pressure_altitude_m`], bit for bit the
/// bisection evaluating every step).
#[must_use]
pub fn pressure_altitude_m(pressure_pa: f64) -> f64 {
    const ITERATIONS: usize = 45;
    let pressure_at = alas_atmo::us1976_pressure_pa;
    let (low_m, high_m) = (0.0_f64, 30_000.0_f64);
    if pressure_pa >= pressure_at(low_m) {
        return low_m;
    }
    if pressure_pa <= pressure_at(high_m) {
        return high_m;
    }
    alas_atmo::us1976_bisect_pressure_altitude_m(pressure_pa, low_m, high_m, ITERATIONS)
}

/// The Mach hold and operating limits a climb or descent leg's speed is
/// resolved against. All are optional; zero or an empty table (the default)
/// means "not declared", and the resolution is then exactly the configured
/// speed reference. A declared table governs over the single value beside it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpeedCaps {
    /// Mach flown above the crossover (the schedule's Mach).
    pub mach_hold: f64,
    /// Maximum operating Mach number, MMO, where a single value holds at every altitude.
    pub max_mach: f64,
    /// Maximum operating calibrated airspeed, VMO, m/s, where a single value
    /// holds at every altitude.
    pub max_cas_m_s: f64,
    /// MMO against pressure altitude, where it varies.
    pub mmo_table: LimitTable,
    /// VMO (calibrated, m/s) against pressure altitude, where it varies.
    pub vmo_table: LimitTable,
}

impl SpeedCaps {
    /// Whether any operating limit varies with altitude.
    fn tabulated(&self) -> bool {
        !(self.mmo_table.is_empty() && self.vmo_table.is_empty())
    }

    /// MMO at static pressure `pressure_pa`, 0 when none is declared.
    #[must_use]
    pub fn max_mach_at(&self, pressure_pa: f64) -> f64 {
        if self.mmo_table.is_empty() {
            self.max_mach
        } else {
            self.mmo_table
                .at(pressure_altitude_m(pressure_pa))
                .unwrap_or(0.0)
        }
    }

    /// VMO, m/s calibrated, at static pressure `pressure_pa`, 0 when none is declared.
    #[must_use]
    pub fn max_cas_m_s_at(&self, pressure_pa: f64) -> f64 {
        if self.vmo_table.is_empty() {
            self.max_cas_m_s
        } else {
            self.vmo_table
                .at(pressure_altitude_m(pressure_pa))
                .unwrap_or(0.0)
        }
    }

    /// Whether a certified limit (VMO or MMO, single or tabulated) is declared.
    #[must_use]
    pub fn declares_limits(&self) -> bool {
        self.max_mach > 0.0 || self.max_cas_m_s > 0.0 || self.tabulated()
    }

    /// MMO and VMO (m/s calibrated) at static pressure `pressure_pa`, each 0
    /// when none is declared: [`Self::max_mach_at`] and
    /// [`Self::max_cas_m_s_at`] with the pressure altitude solved once for
    /// both tables.
    #[must_use]
    pub fn limits_at(&self, pressure_pa: f64) -> (f64, f64) {
        if !self.tabulated() {
            return (self.max_mach, self.max_cas_m_s);
        }
        let altitude_m = pressure_altitude_m(pressure_pa);
        let read = |table: &LimitTable, single: f64| {
            if table.is_empty() {
                single
            } else {
                table.at(altitude_m).unwrap_or(0.0)
            }
        };
        (
            read(&self.mmo_table, self.max_mach),
            read(&self.vmo_table, self.max_cas_m_s),
        )
    }

    /// The binding Mach limit under an MMO of `max_mach` (0 when none is
    /// declared), `INFINITY` when neither it nor a Mach hold is declared.
    fn mach_limit(&self, max_mach: f64) -> f64 {
        [self.mach_hold, max_mach]
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
        let (max_mach, vmo_m_s) = self.limits_at(ambient_pressure_pa);
        let limit = self.mach_limit(max_mach);
        match reference {
            SpeedReference::TrueAirspeed => {
                let mut speed_m_s = configured_m_s;
                if limit.is_finite() {
                    speed_m_s = speed_m_s.min(true_from_mach(limit, ambient_temperature_k)?);
                }
                // VMO bounds a literal true airspeed too; high above the
                // VMO/MMO crossover VMO is supersonic and MMO governs.
                if vmo_m_s > 0.0 {
                    match true_from_calibrated(vmo_m_s, ambient_pressure_pa, ambient_temperature_k)
                    {
                        Ok(vmo_m_s) => speed_m_s = speed_m_s.min(vmo_m_s),
                        Err(AirspeedError::Supersonic { .. }) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(speed_m_s)
            }
            SpeedReference::CalibratedAirspeed => {
                let cas = if vmo_m_s > 0.0 {
                    configured_m_s.min(vmo_m_s)
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
            mmo_table: LimitTable::from_rows(&self.mmo_schedule).unwrap_or_default(),
            vmo_table: LimitTable::from_rows(&self.vmo_schedule).unwrap_or_default(),
        }
    }

    /// Hold the approach rung (descent 4) and the landing to the declared
    /// landing-flap VFE, so the speed change into the landing configuration
    /// starts at or below it. Exact for a calibrated-airspeed reference; for a
    /// true-airspeed one it is conservative, a true airspeed equal to a
    /// calibrated one being a smaller calibrated airspeed at every altitude
    /// above sea level. A no-op when VFE is not declared.
    pub fn hold_approach_to_vfe(&mut self) {
        let vfe = self.vfe_landing_cas_m_s;
        if vfe.is_finite() && vfe > 0.0 {
            self.descent_4_air_speed_m_s = self.descent_4_air_speed_m_s.min(vfe);
            self.landing_air_speed_m_s = self.landing_air_speed_m_s.min(vfe);
        }
    }

    /// The certified limits alone (no Mach hold): what bounds a level leg.
    pub fn certified_caps(&self) -> SpeedCaps {
        SpeedCaps {
            mach_hold: 0.0,
            ..self.climb_caps()
        }
    }

    /// VMO, m/s calibrated, at pressure altitude `pressure_altitude_m`: the
    /// tabulated value where the profile declares a table, otherwise the
    /// single value; 0 when none is declared.
    pub fn vmo_cas_m_s_at_altitude(&self, pressure_altitude_m: f64) -> f64 {
        LimitTable::from_rows(&self.vmo_schedule)
            .and_then(|table| table.at(pressure_altitude_m))
            .unwrap_or(self.max_operating_cas_m_s)
    }

    /// MMO at pressure altitude `pressure_altitude_m`, as
    /// [`Self::vmo_cas_m_s_at_altitude`]; 0 when none is declared.
    pub fn mmo_at_altitude(&self, pressure_altitude_m: f64) -> f64 {
        LimitTable::from_rows(&self.mmo_schedule)
            .and_then(|table| table.at(pressure_altitude_m))
            .unwrap_or(self.max_operating_mach)
    }

    /// The smallest VMO, m/s calibrated, between two pressure altitudes; 0 when none is declared.
    pub fn min_vmo_cas_m_s_between(&self, low_m: f64, high_m: f64) -> f64 {
        LimitTable::from_rows(&self.vmo_schedule)
            .and_then(|table| table.min_between(low_m, high_m))
            .unwrap_or(self.max_operating_cas_m_s)
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
            ..SpeedCaps::default()
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
                assert!(
                    literal <= vmo + 1.0e-9,
                    "{altitude_m} m: literal TAS exceeds VMO"
                );
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

    #[test]
    fn both_limits_at_one_pressure_are_the_two_single_reads_bit_for_bit() {
        // The Learjet 60 chart: a VMO step and slope, an MMO slope.
        let vmo = LimitTable::from_rows(&[
            (0.0, 300.0 * KNOT_M_S),
            (8_000.0 * FOOT_M, 300.0 * KNOT_M_S),
            (8_000.0 * FOOT_M, 340.0 * KNOT_M_S),
            (20_800.0 * FOOT_M, 340.0 * KNOT_M_S),
            (23_300.0 * FOOT_M, 330.0 * KNOT_M_S),
        ])
        .unwrap();
        let mmo =
            LimitTable::from_rows(&[(37_000.0 * FOOT_M, 0.81), (43_000.0 * FOOT_M, 0.78)]).unwrap();
        let single = SpeedCaps {
            max_mach: 0.82,
            max_cas_m_s: 350.0 * KNOT_M_S,
            ..SpeedCaps::default()
        };
        let tables = [
            single,
            SpeedCaps {
                vmo_table: vmo,
                ..single
            },
            SpeedCaps {
                mmo_table: mmo,
                ..single
            },
            SpeedCaps {
                vmo_table: vmo,
                mmo_table: mmo,
                ..SpeedCaps::default()
            },
        ];
        for caps in tables {
            for altitude_m in (-10..=320).map(|step| f64::from(step) * 157.3 * FOOT_M) {
                let (p, _) = ambient(altitude_m);
                let (max_mach, max_cas_m_s) = caps.limits_at(p);
                assert_eq!(
                    max_mach.to_bits(),
                    caps.max_mach_at(p).to_bits(),
                    "{altitude_m} m"
                );
                assert_eq!(max_cas_m_s.to_bits(), caps.max_cas_m_s_at(p).to_bits());
            }
        }
    }
}
