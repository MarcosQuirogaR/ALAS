// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The maximum operating speed condition of the en-route envelope.

use crate::AlasConfig;

impl AlasConfig {
    /// `(Mach, altitude m)` where the maximum operating airspeed meets the
    /// maximum operating Mach number, the highest dynamic pressure of the
    /// en-route envelope: the declared V_MO and M_MO of the mission profile
    /// (their lowest-altitude rows where they are tabulated), else the design
    /// cruising speed implied by the design dive speed through CS 25.335(b)
    /// ([`crate::design_speeds::design_cruise_speed_eas_m_s`]; `V_MO` may not
    /// exceed `V_C`, CS 25.1505), taken as calibrated, at the cruise Mach number.
    /// `None` without a positive speed and a subsonic Mach number.
    #[must_use]
    pub fn max_operating_condition(&self) -> Option<(f64, f64)> {
        let profile = &self.mission.profile;
        let declared_cas = profile
            .vmo_schedule
            .first()
            .map_or(profile.max_operating_cas_m_s, |row| row.1);
        let declared_mach = profile
            .mmo_schedule
            .first()
            .map_or(profile.max_operating_mach, |row| row.1);
        let cas = if declared_cas > 0.0 {
            declared_cas
        } else {
            self.requirements.design_cruise_speed_eas_m_s()
        };
        let mach = if declared_mach > 0.0 {
            declared_mach
        } else {
            self.requirements.cruise_mach
        };
        if !(cas.is_finite() && cas > 0.0 && mach.is_finite() && mach > 0.0 && mach < 1.0) {
            return None;
        }
        let altitude_m = crate::mission::crossover_altitude_m(cas, mach).ok()?;
        altitude_m
            .is_finite()
            .then_some((mach, altitude_m.max(0.0)))
    }
}

#[cfg(test)]
mod tests {
    use crate::AlasConfig;

    /// The jets cross over below their cruise level; the ATR 72-212A (VMO 250 kt, MMO 0.55,
    /// FAA TCDS A53EU) crosses over at 20,293 ft, above its 17,000 ft cruise. Either way the
    /// condition carries a dynamic pressure no lower than the cruise point's.
    #[test]
    fn the_condition_has_a_higher_dynamic_pressure_than_the_cruise_point() {
        let q = |mach: f64, altitude_m: f64| {
            0.7 * alas_atmo::us1976_compute_values(altitude_m, 0.0).pressure_pa * mach * mach
        };
        for name in ["A320-200", "B787-9", "ATR72-600"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let (mach, altitude_m) = config
                .max_operating_condition()
                .unwrap_or_else(|| panic!("{name}: no condition"));
            assert!(mach > 0.3 && mach < 1.0, "{name}: Mach {mach}");
            assert!((0.0..15_000.0).contains(&altitude_m), "{name}: {altitude_m} m");
            let requirements = &config.requirements;
            let cruise = q(requirements.cruise_mach, requirements.cruise_altitude_m);
            let condition = q(mach, altitude_m);
            assert!(condition >= cruise, "{name}: {condition} Pa against {cruise} Pa");
        }
    }
}
