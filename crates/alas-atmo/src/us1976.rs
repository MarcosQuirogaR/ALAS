// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from SUAVE/Attributes/Atmospheres/Earth/US_Standard_1976.py,
// SUAVE/Analyses/Atmospheric/US_Standard_1976.py, SUAVE/Attributes/Gases/Air.py
// and SUAVE/Attributes/Planets/Earth.py
// Upstream: SUAVE 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The U.S. Standard Atmosphere (1976), as SUAVE's mission stack evaluates it.
//!
//! Every SUAVE mission segment attaches one of these to compute pressure,
//! temperature and the gas properties the segment solver needs at whatever
//! altitude it is currently integrating through. It is a different reference
//! implementation from [`crate::isa`] -- SUAVE's break-point table rather than
//! AeroSandbox's -- so the two are kept as separate submodules rather than
//! merged into one "the" atmosphere: nothing here assumes agreement with
//! `isa`, and a caller that needs SUAVE parity must use this one.
//!
//! # A different lapse-rate sign convention from `isa`
//!
//! [`crate::isa`]'s `lapse_rate_k_per_m` is positive where temperature
//! *rises* with altitude. This module's `alpha` is the opposite: positive
//! where temperature *falls*, because that is how
//! `Analyses/Atmospheric/US_Standard_1976.py` defines it
//! (`alpha = -(T[i+1] - T[i]) / (z[i+1] - z[i])`). The two are not unified,
//! because unifying them would stop this module's arithmetic from matching
//! SUAVE's own line for line, and a reader who assumes the sign is shared
//! between the two atmosphere models would get every adiabatic layer's
//! pressure wrong.

use std::sync::LazyLock;

/// Specific gas constant of air, in m^2/(s^2*K).
///
/// `SUAVE.Attributes.Gases.Air.gas_specific_constant`. Independent of
/// [`crate::isa::GAS_CONSTANT_AIR`], which AeroSandbox derives instead from
/// the universal gas constant and the molecular mass of air; the two agree to
/// five significant figures but are not the same constant and are not
/// unified here, for the same reason the lapse-rate sign is not unified (see
/// the module documentation).
pub const GAS_CONSTANT_AIR: f64 = 287.052_874_2;

/// Specific heat capacity of air at constant pressure, in J/(kg*K).
///
/// `SUAVE.Attributes.Gases.Air.specific_heat_capacity`. Used only to form the
/// Prandtl number.
const SPECIFIC_HEAT_CAPACITY_AIR: f64 = 1006.0;

/// Sutherland's law constant, in kg/(m*s*sqrt(K)).
///
/// `SUAVE.Attributes.Gases.Air.compute_absolute_viscosity`, cited upstream to
/// <https://www.cfd-online.com/Wiki/Sutherland's_law>. Numerically identical
/// to [`crate::isa`]'s private Sutherland constant, but kept as this module's
/// own copy rather than shared: the two upstream projects each define it
/// independently (`AeroSandbox/atmosphere.py` and `SUAVE/Attributes/Gases/
/// Air.py` do not share code either), so one copy per translated source file
/// is the more faithful port, not an oversight.
const SUTHERLAND_C1: f64 = 1.458e-6;

/// Sutherland's law reference temperature, in Kelvin. See [`SUTHERLAND_C1`].
const SUTHERLAND_S: f64 = 110.4;

/// Earth's mean radius, in metres. `SUAVE.Attributes.Planets.Earth.mean_radius`.
///
/// Used only to convert a geometric altitude into a geopotential one; SUAVE's
/// `compute_gravity` (a `g0 * (Re / (Re + H))^2` correction) is a separate,
/// unrelated method on the same class that this module does not call, because
/// `compute_values` itself never calls it either -- gravity is held at
/// [`SEA_LEVEL_GRAVITY`] throughout.
const MEAN_RADIUS_M: f64 = 6.371e6;

/// Standard sea-level gravity, in m/s^2.
/// `SUAVE.Attributes.Planets.Earth.sea_level_gravity`.
const SEA_LEVEL_GRAVITY: f64 = 9.80665;

/// One row of the break-point table: SUAVE's `self.breaks`, arrays indexed
/// together.
#[derive(Debug, Clone, Copy)]
struct Break {
    altitude_m: f64,
    temperature_k: f64,
    pressure_pa: f64,
    /// Published kg/m^3 at this break. `compute_values` never reads this --
    /// it recomputes density from `p` and `T` through the ideal gas law
    /// instead -- so this field exists only for fidelity to the source table
    /// and is not consumed by anything in this module.
    #[allow(dead_code)] // carried for fidelity to the upstream table; see above
    density_kg_m3: f64,
}

/// `SUAVE.Attributes.Atmospheres.Earth.US_Standard_1976.__defaults__`'s
/// `self.breaks`: geopotential altitude, temperature, pressure and density at
/// nine standard breakpoints, from -2 km to 84.852 km.
const BREAKS: [Break; 9] = [
    Break {
        altitude_m: -2000.0,
        temperature_k: 301.15,
        pressure_pa: 127_774.0,
        density_kg_m3: 1.478_08,
    },
    Break {
        altitude_m: 0.0,
        temperature_k: 288.15,
        pressure_pa: 101_325.0,
        density_kg_m3: 1.2250,
    },
    Break {
        altitude_m: 11_000.0,
        temperature_k: 216.65,
        pressure_pa: 22_632.1,
        density_kg_m3: 0.363_918,
    },
    Break {
        altitude_m: 20_000.0,
        temperature_k: 216.65,
        pressure_pa: 5474.89,
        density_kg_m3: 0.088_034_9,
    },
    Break {
        altitude_m: 32_000.0,
        temperature_k: 228.65,
        pressure_pa: 868.019,
        density_kg_m3: 0.013_225_0,
    },
    Break {
        altitude_m: 47_000.0,
        temperature_k: 270.65,
        pressure_pa: 110.906,
        density_kg_m3: 0.001_427_53,
    },
    Break {
        altitude_m: 51_000.0,
        temperature_k: 270.65,
        pressure_pa: 66.9389,
        density_kg_m3: 0.000_861_606,
    },
    Break {
        altitude_m: 71_000.0,
        temperature_k: 214.65,
        pressure_pa: 3.956_42,
        density_kg_m3: 0.000_064_209_9,
    },
    Break {
        altitude_m: 84_852.0,
        temperature_k: 186.95,
        pressure_pa: 0.3734,
        density_kg_m3: 0.000_006_957_92,
    },
];

/// One resolved segment between two consecutive breaks: the lower break's
/// base condition, plus `alpha`, the segment's own lapse rate (see the module
/// documentation for its sign convention).
#[derive(Debug, Clone, Copy)]
struct Segment {
    base_altitude_m: f64,
    // Read only by test failure messages (a segment's own span makes a
    // mis-selected segment easier to spot); production logic only ever needs
    // `base_altitude_m`, since `values_at_geopotential_altitude` clamps its
    // input to the table's overall range before `segment_for` ever runs.
    #[allow(dead_code)]
    top_altitude_m: f64,
    base_temperature_k: f64,
    base_pressure_pa: f64,
    alpha_k_per_m: f64,
}

/// The eight segments the nine breaks bound, resolved once.
static SEGMENTS: LazyLock<[Segment; BREAKS.len() - 1]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let lower = BREAKS[i];
        let upper = BREAKS[i + 1];
        Segment {
            base_altitude_m: lower.altitude_m,
            top_altitude_m: upper.altitude_m,
            base_temperature_k: lower.temperature_k,
            base_pressure_pa: lower.pressure_pa,
            alpha_k_per_m: -(upper.temperature_k - lower.temperature_k)
                / (upper.altitude_m - lower.altitude_m),
        }
    })
});

/// The segment covering `geopotential_altitude_m`, already clamped to
/// `[BREAKS[0].altitude_m, BREAKS[last].altitude_m]` by the caller.
///
/// `compute_values` walks every segment and lets the last matching one win
/// (`(zs >= breaks.altitude[i]) & (zs <= breaks.altitude[i+1])`, applied in
/// order for every `i`), so at an exact break altitude the higher-indexed
/// segment's overwrite is what survives. Both segments meeting at a break
/// agree on `p` and `T` there by construction of the table, so which one
/// "wins" does not change the result; this picks the segment directly rather
/// than reproducing the overwrite loop.
fn segment_for(geopotential_altitude_m: f64) -> &'static Segment {
    let segments = &*SEGMENTS;
    let mut index = 0;
    for (i, segment) in segments.iter().enumerate() {
        if geopotential_altitude_m >= segment.base_altitude_m {
            index = i;
        }
    }
    &segments[index]
}

/// Every quantity `compute_values` reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Values {
    /// Pressure, in pascals.
    pub pressure_pa: f64,
    /// Temperature, in Kelvin, including any `temperature_deviation_k`.
    pub temperature_k: f64,
    /// Density, in kg/m^3, from the ideal gas law.
    pub density_kg_m3: f64,
    /// Speed of sound, in m/s, at a constant ratio of specific heats of 1.4.
    ///
    /// SUAVE's `compute_speed_of_sound` also supports a temperature-varying
    /// `gamma` (`var_gamma=True`, a cubic fit in `Air.compute_gamma`), but no
    /// caller in SUAVE's own mission stack ever passes it, so only the
    /// constant-gamma path is translated here.
    pub speed_of_sound_m_s: f64,
    /// Dynamic viscosity, in kg/(m*s), from Sutherland's law.
    pub dynamic_viscosity_pa_s: f64,
    /// Kinematic viscosity, in m^2/s: dynamic viscosity over density.
    pub kinematic_viscosity_m2_s: f64,
    /// Thermal conductivity, in W/(m*K).
    ///
    /// Cited upstream to
    /// <https://www.engineeringtoolbox.com/air-properties-viscosity-conductivity-heat-capacity-d_1509.html>,
    /// "properties computed at 1 bar (14.5 psia)".
    pub thermal_conductivity_w_m_k: f64,
    /// Prandtl number, dimensionless.
    pub prandtl_number: f64,
}

/// Compute atmospheric values at `altitude_m` (geometric altitude above mean
/// sea level, in metres), with an optional temperature deviation.
///
/// `altitude_m` is converted from geometric to geopotential internally
/// (`z / (1 + z / R_earth)`, matching `compute_values`) and then clamped to
/// the table's range, `[-2000, 84852]` m; a request outside that range is
/// logged and clamped rather than extrapolated, matching upstream's
/// `warnings.warn`-and-clamp behaviour (translated as [`tracing::warn!`],
/// since library code here reports through `tracing` rather than printing --
/// see `CONTRIBUTING.md`).
pub fn compute_values(altitude_m: f64, temperature_deviation_k: f64) -> Values {
    let geopotential_altitude_m = altitude_m / (1.0 + altitude_m / MEAN_RADIUS_M);
    values_at_geopotential_altitude(geopotential_altitude_m, temperature_deviation_k)
}

/// The part of [`compute_values`] that operates purely in geopotential
/// altitude, split out so tests can probe the break-point table's own
/// behaviour (clamping, segment selection, continuity) without also going
/// through the geometric-to-geopotential conversion -- which shifts a
/// geometric input away from the geopotential value of the same number, so a
/// test that wants to land exactly on a break has to reason about both at
/// once unless they are separated like this.
fn values_at_geopotential_altitude(
    geopotential_altitude_m: f64,
    temperature_deviation_k: f64,
) -> Values {
    let z_min = BREAKS[0].altitude_m;
    let z_max = BREAKS[BREAKS.len() - 1].altitude_m;
    let clamped_altitude_m = if geopotential_altitude_m < z_min {
        tracing::warn!(
            requested_m = geopotential_altitude_m,
            floor_m = z_min,
            "altitude requested below the US Standard 1976 model's floor; clamping"
        );
        z_min
    } else if geopotential_altitude_m > z_max {
        tracing::warn!(
            requested_m = geopotential_altitude_m,
            ceiling_m = z_max,
            "altitude requested above the US Standard 1976 model's ceiling; clamping"
        );
        z_max
    } else {
        geopotential_altitude_m
    };

    let segment = segment_for(clamped_altitude_m);
    let dz = clamped_altitude_m - segment.base_altitude_m;

    let pressure_pa = if segment.alpha_k_per_m == 0.0 {
        segment.base_pressure_pa
            * (-dz * SEA_LEVEL_GRAVITY / (GAS_CONSTANT_AIR * segment.base_temperature_k)).exp()
    } else {
        segment.base_pressure_pa
            * (1.0 - segment.alpha_k_per_m * dz / segment.base_temperature_k)
                .powf(SEA_LEVEL_GRAVITY / (segment.alpha_k_per_m * GAS_CONSTANT_AIR))
    };

    let temperature_k =
        segment.base_temperature_k - dz * segment.alpha_k_per_m + temperature_deviation_k;

    let density_kg_m3 = pressure_pa / (GAS_CONSTANT_AIR * temperature_k);
    let speed_of_sound_m_s = (1.4 * GAS_CONSTANT_AIR * temperature_k).sqrt();
    let dynamic_viscosity_pa_s =
        SUTHERLAND_C1 * temperature_k.powf(1.5) / (temperature_k + SUTHERLAND_S);
    let kinematic_viscosity_m2_s = dynamic_viscosity_pa_s / density_kg_m3;
    let thermal_conductivity_w_m_k = 3.99e-4 + 9.89e-5 * temperature_k
        - 4.57e-8 * temperature_k.powi(2)
        + 1.4e-11 * temperature_k.powi(3);
    let prandtl_number =
        dynamic_viscosity_pa_s * SPECIFIC_HEAT_CAPACITY_AIR / thermal_conductivity_w_m_k;

    Values {
        pressure_pa,
        temperature_k,
        density_kg_m3,
        speed_of_sound_m_s,
        dynamic_viscosity_pa_s,
        kinematic_viscosity_m2_s,
        thermal_conductivity_w_m_k,
        prandtl_number,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_level_matches_the_table() {
        let values = compute_values(0.0, 0.0);
        assert_eq!(values.pressure_pa, 101_325.0);
        assert_eq!(values.temperature_k, 288.15);
    }

    #[test]
    fn pressure_and_temperature_nearly_agree_either_side_of_every_segment_boundary() {
        // Unlike `crate::isa`'s table, which chains the barometric formula
        // layer by layer, SUAVE's break-point table stores independently
        // published pressure and temperature constants at each break -- they
        // are not derived from each other, so evaluating the segment below a
        // boundary at its own top does not reproduce the segment above's
        // stored base value to floating-point precision. Confirmed against
        // the Python reference directly: at the geopotential altitude 0
        // boundary, `compute_values` itself shows a ~2e-6 relative jump
        // between a point a millimetre below and a millimetre above. This
        // checks that the jump stays of that small, table-rounding size (a
        // wrong segment or a sign error would be wrong by orders of
        // magnitude more, not a few parts per million), not that there is no
        // jump at all -- there genuinely is one, faithfully reproduced.
        for boundary in &BREAKS[1..BREAKS.len() - 1] {
            let below = values_at_geopotential_altitude(boundary.altitude_m - 1e-3, 0.0);
            let above = values_at_geopotential_altitude(boundary.altitude_m + 1e-3, 0.0);
            let pressure_relative_diff =
                (below.pressure_pa - above.pressure_pa).abs() / above.pressure_pa;
            assert!(
                pressure_relative_diff < 1e-4,
                "pressure jumps by a relative {pressure_relative_diff:e} at {} m, \
                 far more than table-rounding should cause",
                boundary.altitude_m
            );
            assert!(
                (below.temperature_k - above.temperature_k).abs() < 1e-3,
                "temperature discontinuous at {} m",
                boundary.altitude_m
            );
        }
    }

    #[test]
    fn the_segment_above_a_boundary_wins_the_tie_and_matches_its_own_stored_base_value() {
        // `segment_for`'s documentation claims the higher-indexed segment
        // wins at an exact break, matching SUAVE's mask-overwrite loop.
        // Confirmed against the Python reference directly: querying exactly
        // 11000 m geopotential returns 22632.1 Pa -- `BREAKS[2]`'s own
        // stored base pressure, not a value derived from segment 1's formula
        // extrapolated up to that point.
        for interior_break in &BREAKS[1..BREAKS.len() - 1] {
            let boundary_m = interior_break.altitude_m;
            let values = values_at_geopotential_altitude(boundary_m, 0.0);
            assert_eq!(
                values.pressure_pa, interior_break.pressure_pa,
                "at the exact boundary {boundary_m} m, the segment above should win \
                 and its dz=0 evaluation should equal the table's own stored pressure"
            );
        }
    }

    #[test]
    fn isothermal_segments_are_hit_where_the_table_says_they_should_be() {
        // 11-20 km and 47-51 km share an identical base temperature in the
        // table above, which is what makes `alpha` exactly zero there.
        assert_eq!(
            SEGMENTS[2].alpha_k_per_m, 0.0,
            "11-20 km should be isothermal"
        );
        assert_eq!(
            SEGMENTS[5].alpha_k_per_m, 0.0,
            "47-51 km should be isothermal"
        );
        for (i, segment) in SEGMENTS.iter().enumerate() {
            if i != 2 && i != 5 {
                assert_ne!(
                    segment.alpha_k_per_m, 0.0,
                    "segment {i} ({}-{} m) should not be isothermal",
                    segment.base_altitude_m, segment.top_altitude_m
                );
            }
        }
    }

    #[test]
    fn requests_outside_the_table_are_clamped_not_extrapolated() {
        // `BREAKS[0].altitude_m`/`BREAKS[last].altitude_m` are geopotential
        // values, not valid geometric input for `compute_values` -- the
        // conversion would shift them off the table's true edge (see
        // `values_at_geopotential_altitude`'s documentation) -- so the
        // reference values here are taken in geopotential space directly,
        // and only the clamping behaviour itself is exercised through the
        // public, geometric-altitude entry point.
        let z_min = BREAKS[0].altitude_m;
        let z_max = BREAKS[BREAKS.len() - 1].altitude_m;
        let floor = values_at_geopotential_altitude(z_min, 0.0);
        let ceiling = values_at_geopotential_altitude(z_max, 0.0);

        for below_floor_altitude_m in [-100_000.0, -500_000.0] {
            let below_floor = compute_values(below_floor_altitude_m, 0.0);
            assert_eq!(below_floor.pressure_pa, floor.pressure_pa);
            assert_eq!(below_floor.temperature_k, floor.temperature_k);
        }
        for above_ceiling_altitude_m in [200_000.0, 500_000.0] {
            let above_ceiling = compute_values(above_ceiling_altitude_m, 0.0);
            assert_eq!(above_ceiling.pressure_pa, ceiling.pressure_pa);
            assert_eq!(above_ceiling.temperature_k, ceiling.temperature_k);
        }
    }

    #[test]
    fn temperature_deviation_shifts_temperature_and_density_but_not_pressure() {
        let base = compute_values(5000.0, 0.0);
        let warmer = compute_values(5000.0, 10.0);

        assert_eq!(warmer.pressure_pa, base.pressure_pa);
        assert_eq!(warmer.temperature_k, base.temperature_k + 10.0);
        assert!(warmer.density_kg_m3 < base.density_kg_m3);
    }

    #[test]
    fn density_matches_the_ideal_gas_law() {
        let values = compute_values(3000.0, 0.0);
        let recovered_pressure = values.density_kg_m3 * GAS_CONSTANT_AIR * values.temperature_k;
        assert!((recovered_pressure - values.pressure_pa).abs() < 1e-9);
    }

    #[test]
    fn kinematic_viscosity_is_dynamic_viscosity_over_density() {
        let values = compute_values(8000.0, 0.0);
        assert_eq!(
            values.kinematic_viscosity_m2_s,
            values.dynamic_viscosity_pa_s / values.density_kg_m3
        );
    }
}
