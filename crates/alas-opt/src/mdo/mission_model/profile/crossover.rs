// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CAS/Mach climb and descent legs, the Mach ceiling (MMO) on literal
//! speeds, and the standard-day cruise speed on the actual day.
//!
//! The speed definition itself lives in [`alas_config::mission::SpeedCaps`],
//! shared with the native pseudospectral schedule. This module applies it to
//! the MDO model's sub-rungs and bounds the literal-speed legs.

use alas_config::mission::{SpeedCaps, SpeedReference};
use alas_mass::fuel_plan::FuelModelError;

use super::{ProfileGeometry, Segment, SegmentKind};

impl ProfileGeometry<'_> {
    /// A climbing or descending leg from `start_m` to `end_m` flown at the
    /// calibrated airspeed schedule `cas_m_s`, switching to the profile's
    /// Mach hold above the CAS/Mach crossover and bounded by MMO and VMO:
    /// `subdivisions` equal-altitude segments, each with the true airspeed
    /// `TAS = M a(T)` resolved at its own midpoint altitude's ambient state
    /// ([`SpeedCaps::true_airspeed_m_s`]). A *discretized* approximation of
    /// the continuous schedule (each sub-rung flies one constant true
    /// airspeed); every production call site uses
    /// [`alas_config::mission::CAS_SPEED_SUBDIVISIONS`], the count the native
    /// schedule uses, so both paths apply one rule. The real atmosphere at
    /// whatever band is flown is what is sampled, so no equivalent-airspeed
    /// adaptation is needed when the cruise altitude is lowered.
    pub(super) fn cas_vertical_segments(
        &self,
        kind: SegmentKind,
        start_m: f64,
        end_m: f64,
        cas_m_s: f64,
        rate_m_s: f64,
        subdivisions: usize,
    ) -> Result<Vec<Segment>, FuelModelError> {
        if (end_m - start_m).abs() <= super::MINIMUM_SEGMENT_ALTITUDE_M {
            return Ok(Vec::new());
        }
        let caps = self.caps_for(kind);
        let subdivisions = subdivisions.max(1);
        let mut segments = Vec::with_capacity(subdivisions);
        let step_m = (end_m - start_m) / subdivisions as f64;
        let mut altitude_m = start_m;
        for _ in 0..subdivisions {
            let next_m = altitude_m + step_m;
            let mid_m = 0.5 * (altitude_m + next_m);
            let atmosphere = self.ambient(mid_m, "calibrated-airspeed")?;
            let tas_m_s = caps
                .true_airspeed_m_s(
                    SpeedReference::CalibratedAirspeed,
                    cas_m_s,
                    atmosphere.pressure_pa,
                    atmosphere.temperature_k,
                )
                .map_err(|error| {
                    FuelModelError::InvalidModel(format!(
                        "{kind:?} calibrated airspeed {cas_m_s} m/s at {mid_m} m: {error}"
                    ))
                })?;
            segments.extend(Segment::vertical(
                kind, altitude_m, next_m, tas_m_s, rate_m_s,
            )?);
            altitude_m = next_m;
        }
        Ok(segments)
    }

    /// The Mach hold and operating limits of a leg of `kind`.
    fn caps_for(&self, kind: SegmentKind) -> SpeedCaps {
        match kind {
            SegmentKind::Descent | SegmentKind::Landing => self.profile.descent_caps(),
            _ => self.profile.climb_caps(),
        }
    }

    /// The factor, at most `1`, that brings the literal-speed legs and the
    /// cruise rungs of a plan under MMO (`max_operating_mach`).
    ///
    /// Calibrated-airspeed legs resolve their own MMO bound per sub-rung
    /// ([`Self::cas_vertical_segments`]) and are skipped here. A literal
    /// true airspeed is a larger Mach number the higher it is flown (the
    /// speed of sound falls with altitude to the tropopause), so each rung is
    /// held to `MMO a` at the colder end of its band. `1.0` when MMO is not
    /// declared.
    pub(super) fn mach_scale(
        &self,
        climb: &[Segment],
        descent: &[Segment],
        cruise_rungs: &[(f64, f64)],
        cruise_m: f64,
    ) -> Result<f64, FuelModelError> {
        let mmo = self.profile.max_operating_mach;
        if !(mmo.is_finite() && mmo > 0.0) {
            return Ok(1.0);
        }
        let ceiling = |low_m: f64, high_m: f64| -> Result<f64, FuelModelError> {
            let cold = self
                .ambient(low_m, "MMO")?
                .speed_of_sound_m_s
                .min(self.ambient(high_m, "MMO")?.speed_of_sound_m_s);
            Ok(mmo * cold)
        };
        let mut scale: f64 = 1.0;
        if self.profile.climb_descent_speed_reference == SpeedReference::TrueAirspeed {
            for segment in climb.iter().chain(descent.iter()) {
                if segment.tas_m_s.is_finite() && segment.tas_m_s > 0.0 {
                    let limit = ceiling(segment.start_altitude_m, segment.end_altitude_m)?;
                    scale = scale.min(limit / segment.tas_m_s);
                }
            }
        }
        let cruise_limit = ceiling(cruise_m, cruise_m)?;
        for (tas_m_s, _) in cruise_rungs {
            if tas_m_s.is_finite() && *tas_m_s > 0.0 {
                scale = scale.min(cruise_limit / tas_m_s);
            }
        }
        Ok(scale.clamp(0.0, 1.0))
    }

    /// The declared cruise true airspeed `declared_m_s` is a standard-day
    /// number stated for the configured level (`TAS = M a_ISA`); on a day
    /// `isa_deviation_c` off standard the same Mach number is a different
    /// true airspeed, `M a(T_ISA + dT)`. The ratio at `level_m`, exactly `1`
    /// on a standard day.
    pub(super) fn standard_day_speed_ratio(&self, level_m: f64) -> Result<f64, FuelModelError> {
        if self.isa_deviation_c == 0.0 {
            return Ok(1.0);
        }
        let standard = alas_atmo::us1976_try_compute_values(level_m, 0.0).map_err(|error| {
            FuelModelError::InvalidModel(format!("standard-day cruise atmosphere: {error}"))
        })?;
        let actual = self.ambient(level_m, "cruise")?;
        Ok(actual.speed_of_sound_m_s / standard.speed_of_sound_m_s)
    }
}

// Tests assert on `Result`s they just constructed, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::mission::{MissionProfileConfig, CAS_SPEED_SUBDIVISIONS};

    const KNOT_M_S: f64 = 1852.0 / 3600.0;
    /// The airspeed module and the US1976 table carry gas constants that
    /// differ in the seventh digit (287.05287 against 287.0528742 J/(kg K)).
    const MACH_TOLERANCE: f64 = 1.0e-6;

    fn geometry(profile: &MissionProfileConfig, isa_deviation_c: f64) -> ProfileGeometry<'_> {
        ProfileGeometry {
            profile,
            departure_elevation_m: 0.0,
            arrival_elevation_m: 0.0,
            cruise_altitude_m: 11_300.0,
            cas_subdivisions: 64,
            isa_deviation_c,
            dive_speed_eas_m_s: 0.0,
            constant_mach_cruise: true,
        }
    }

    fn cas_mach_profile() -> MissionProfileConfig {
        MissionProfileConfig {
            climb_descent_speed_reference: SpeedReference::CalibratedAirspeed,
            climb_mach: 0.78,
            descent_mach: 0.78,
            max_operating_mach: 0.82,
            max_operating_cas_m_s: 350.0 * KNOT_M_S,
            ..MissionProfileConfig::default()
        }
    }

    #[test]
    fn a_300_kcas_m078_climb_switches_from_cas_to_mach_at_flight_level_293() {
        let profile = cas_mach_profile();
        let geometry = geometry(&profile, 0.0);
        let segments = geometry
            .cas_vertical_segments(SegmentKind::Climb, 0.0, 11_300.0, 300.0 * KNOT_M_S, 8.0, 64)
            .unwrap();
        let crossover_m =
            alas_config::mission::crossover_altitude_m(300.0 * KNOT_M_S, 0.78).unwrap();
        for segment in &segments {
            let mid_m = 0.5 * (segment.start_altitude_m + segment.end_altitude_m);
            let ambient = alas_atmo::us1976_compute_values(mid_m, 0.0);
            let mach = segment.tas_m_s / ambient.speed_of_sound_m_s;
            assert!(mach <= 0.78 + MACH_TOLERANCE, "M {mach} at {mid_m} m");
            if mid_m > crossover_m + 200.0 {
                // Above the crossover the Mach number is the schedule's.
                assert!(
                    (mach - 0.78).abs() < MACH_TOLERANCE,
                    "M {mach} at {mid_m} m"
                );
            }
            if mid_m < crossover_m - 200.0 {
                // Below it the calibrated airspeed is: Mach is lower.
                assert!(mach < 0.78 - 1.0e-4, "M {mach} at {mid_m} m");
            }
        }
        assert!((crossover_m / 0.3048 / 100.0 - 293.0).abs() < 1.5);
    }

    #[test]
    fn a_literal_ladder_is_scaled_under_mmo_and_a_cas_ladder_is_not() {
        let mut profile = cas_mach_profile();
        profile.climb_descent_speed_reference = SpeedReference::TrueAirspeed;
        let geometry = geometry(&profile, 0.0);
        let cruise_m = 11_300.0;
        let ambient = alas_atmo::us1976_compute_values(cruise_m, 0.0);
        // 250 m/s at FL370 is M 0.847, above MMO 0.82.
        let rungs = [(250.0, 1.0)];
        let scale = geometry.mach_scale(&[], &[], &rungs, cruise_m).unwrap();
        assert!((scale - 0.82 * ambient.speed_of_sound_m_s / 250.0).abs() < 1.0e-12);
        // The scaled speed is exactly MMO.
        assert!((250.0 * scale / ambient.speed_of_sound_m_s - 0.82).abs() < 1.0e-12);
        // A speed already under MMO is left alone, as is an undeclared MMO.
        assert_eq!(
            geometry
                .mach_scale(&[], &[], &[(230.0, 1.0)], cruise_m)
                .unwrap(),
            1.0
        );
        let mut undeclared = cas_mach_profile();
        undeclared.max_operating_mach = 0.0;
        assert_eq!(
            super::ProfileGeometry {
                profile: &undeclared,
                ..geometry
            }
            .mach_scale(&[], &[], &rungs, cruise_m)
            .unwrap(),
            1.0
        );
    }

    #[test]
    fn the_declared_cruise_speed_is_a_standard_day_mach_on_the_flown_day() {
        let profile = cas_mach_profile();
        let declared = 0.78 * alas_atmo::us1976_compute_values(11_300.0, 0.0).speed_of_sound_m_s;
        // Standard day: bit-unchanged.
        assert_eq!(
            geometry(&profile, 0.0)
                .cruise_tas_at(11_300.0, declared)
                .unwrap(),
            declared
        );
        // ISA+15: the same Mach is M a(T + 15): FL370, 295.07 -> 305.11 m/s.
        let hot = geometry(&profile, 15.0)
            .cruise_tas_at(11_300.0, declared)
            .unwrap();
        let expected = 0.78 * (1.4_f64 * 287.052_874_2 * (216.65 + 15.0)).sqrt();
        assert!((hot - expected).abs() < 1.0e-9, "{hot} vs {expected}");
        // A fixed standard-day TAS would be M 0.7543 on that day.
        assert!((declared / (expected / 0.78) - 0.7543).abs() < 1.0e-3);
    }

    #[test]
    fn the_cas_subdivision_count_default_is_shared_with_the_native_path() {
        assert_eq!(CAS_SPEED_SUBDIVISIONS, 8);
    }
}
