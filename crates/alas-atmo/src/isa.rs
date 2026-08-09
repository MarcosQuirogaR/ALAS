// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from aerosandbox/atmosphere/_isa_atmo_functions.py
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The closed-form International Standard Atmosphere: pressure and
//! temperature, from a table of eight layers and the barometric formula.
//!
//! This is one of the two altitude models [`crate::Atmosphere`] selects
//! between, and the one a caller gets only by asking for it. Upstream's
//! default is the fitted [`crate::differentiable`] model, and the two
//! disagree by about a per cent in temperature, so where a module in this
//! port evaluates the ISA it is because its Python counterpart passed
//! `method="isa"` -- not because the closed form is the natural reading of
//! "the standard atmosphere".
//!
//! [`BAROMETRIC_GRAVITY`] (9.81 m/s^2) is the constant this module's upstream
//! defines locally and reads as `g`. It is deliberately not the 9.80665
//! m/s^2 that `atmosphere.py`'s density-altitude formula uses; see
//! [`crate::atmosphere`] for why the two are kept apart.

// A test asserts on a `Result` it just constructed, so a failed unwrap there
// is the assertion failing, not a panic escaping into a caller.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// The ISA layer table is built once, since every altitude query walks it and
// rebuilding it per call would be pure waste for eight rows that never change.
use std::sync::LazyLock;

/// Universal gas constant, in J/(mol*K).
///
/// From `_isa_atmo_functions.py` and `atmosphere.py`, which both define it
/// with this value (a pre-1986 CODATA figure AeroSandbox carries rather than
/// the current 8.31446 J/(mol*K); reproduced as written).
pub const GAS_CONSTANT_UNIVERSAL: f64 = 8.31432;

/// Molecular mass of air, in kg/mol.
pub const MOLECULAR_MASS_AIR: f64 = 28.9644e-3;

/// Specific gas constant of air, in J/(kg*K).
pub const GAS_CONSTANT_AIR: f64 = GAS_CONSTANT_UNIVERSAL / MOLECULAR_MASS_AIR;

/// Gravitational acceleration used by the ISA barometric formula, in m/s^2.
///
/// This is `_isa_atmo_functions.py`'s local `g`, not standard gravity; see
/// the module documentation for why the two are kept apart.
pub const BAROMETRIC_GRAVITY: f64 = 9.81;

/// Kelvin at zero degrees Celsius, for converting the ISA table's base
/// temperatures.
const CELSIUS_TO_KELVIN: f64 = 273.15;

/// Sea-level pressure, in pascals: the base condition the layer table's
/// chained barometric-formula evaluation starts from.
const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;

/// The ISA table's eight layers, exactly as
/// `aerosandbox/atmosphere/isa_data/isa_table.csv` lists them: base
/// altitude in metres, lapse rate in K/km, base temperature in Celsius.
///
/// Troposphere through the first mesospheric layer, per ICAO Doc 7488.
///
/// Each row's base temperature is consistent with linearly extrapolating the
/// row below it, except the last: -86.28 C at 84852 m is half a bit cooler
/// than the -2 K/km lapse from layer 6 would predict there. `temperature_isa`
/// always evaluates the layer below a boundary (see [`layer_index`]), so
/// querying exactly 84852 m returns the extrapolated value, not this row's
/// own number -- a published-table rounding artifact reproduced as-is, not a
/// discontinuity this crate introduces.
const RAW_TABLE: [(f64, f64, f64); 8] = [
    (0.0, -6.5, 15.0),
    (11_000.0, 0.0, -56.5),
    (20_000.0, 1.0, -56.5),
    (32_000.0, 2.8, -44.5),
    (47_000.0, 0.0, -2.5),
    (51_000.0, -2.8, -2.5),
    (71_000.0, -2.0, -58.5),
    (84_852.0, 0.0, -86.28),
];

/// One resolved ISA layer: the raw table row converted to SI, plus the
/// pressure at its base.
#[derive(Debug, Clone, Copy)]
struct Layer {
    base_altitude_m: f64,
    lapse_rate_k_per_m: f64,
    base_temperature_k: f64,
    base_pressure_pa: f64,
}

/// The resolved layer table, built once.
///
/// `isa_pressure` in `_isa_atmo_functions.py` is a running list: each layer's
/// base pressure comes from evaluating the *previous* layer's barometric
/// formula at its own top, so this has to be built in order rather than row
/// by row independently.
static LAYERS: LazyLock<[Layer; RAW_TABLE.len()]> = LazyLock::new(|| {
    let mut layers = RAW_TABLE.map(
        |(base_altitude_m, lapse_rate_k_per_km, base_temperature_c)| {
            Layer {
                base_altitude_m,
                lapse_rate_k_per_m: lapse_rate_k_per_km / 1000.0,
                base_temperature_k: base_temperature_c + CELSIUS_TO_KELVIN,
                base_pressure_pa: 0.0, // resolved below, once the previous layer exists
            }
        },
    );

    layers[0].base_pressure_pa = SEA_LEVEL_PRESSURE_PA;
    for i in 0..layers.len() - 1 {
        layers[i + 1].base_pressure_pa = barometric_formula(
            layers[i].base_pressure_pa,
            layers[i].base_temperature_k,
            layers[i].lapse_rate_k_per_m,
            layers[i + 1].base_altitude_m,
            layers[i].base_altitude_m,
        );
    }
    layers
});

/// The barometric pressure equation, with a linear temperature lapse.
///
/// <https://en.wikipedia.org/wiki/Barometric_formula>.
fn barometric_formula(
    base_pressure_pa: f64,
    base_temperature_k: f64,
    lapse_rate_k_per_m: f64,
    altitude_m: f64,
    base_altitude_m: f64,
) -> f64 {
    let temperature_k = base_temperature_k + lapse_rate_k_per_m * (altitude_m - base_altitude_m);
    // Keeps the layer temperature positive no matter what altitude is asked
    // for; ported from `np.fmax(T, 1)`.
    let temperature_k = temperature_k.max(1.0);

    if lapse_rate_k_per_m != 0.0 {
        base_pressure_pa
            * (temperature_k / base_temperature_k)
                .powf(-BAROMETRIC_GRAVITY / (GAS_CONSTANT_AIR * lapse_rate_k_per_m))
    } else {
        let exponent = (-BAROMETRIC_GRAVITY * (altitude_m - base_altitude_m)
            / (GAS_CONSTANT_AIR * base_temperature_k))
            .clamp(-500.0, 500.0);
        base_pressure_pa * exponent.exp()
    }
}

/// The layer whose formula applies at `altitude_m`.
///
/// `pressure_isa`/`temperature_isa` walk every table row and let the last
/// matching one win (`altitude > isa_base_altitude[i]`, evaluated for every
/// `i` with `np.where`), with an explicit fallback to the first layer when
/// `altitude` is at or below its base. Both amount to picking the highest
/// index whose base altitude is strictly below `altitude_m`, defaulting to
/// the first layer, which is what this does directly.
fn layer_index(altitude_m: f64) -> usize {
    let layers = &*LAYERS;
    let mut index = 0;
    for (i, layer) in layers.iter().enumerate() {
        if altitude_m > layer.base_altitude_m {
            index = i;
        }
    }
    index
}

/// Pressure at `altitude_m` (geopotential, in metres) under the ISA, in
/// pascals.
pub fn pressure_isa(altitude_m: f64) -> f64 {
    let layer = &LAYERS[layer_index(altitude_m)];
    barometric_formula(
        layer.base_pressure_pa,
        layer.base_temperature_k,
        layer.lapse_rate_k_per_m,
        altitude_m,
        layer.base_altitude_m,
    )
}

/// Temperature at `altitude_m` (geopotential, in metres) under the ISA, in
/// Kelvin.
pub fn temperature_isa(altitude_m: f64) -> f64 {
    let layer = &LAYERS[layer_index(altitude_m)];
    (altitude_m - layer.base_altitude_m) * layer.lapse_rate_k_per_m + layer.base_temperature_k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_level_matches_the_isa_definition() {
        assert_eq!(pressure_isa(0.0), 101_325.0);
        assert_eq!(temperature_isa(0.0), 288.15);
    }

    #[test]
    fn pressure_and_temperature_fall_monotonically_through_the_troposphere() {
        let altitudes = [0.0, 1000.0, 5000.0, 8000.0, 11_000.0];
        for pair in altitudes.windows(2) {
            assert!(
                pressure_isa(pair[1]) < pressure_isa(pair[0]),
                "pressure should fall with altitude in the troposphere"
            );
            assert!(
                temperature_isa(pair[1]) < temperature_isa(pair[0]),
                "temperature should fall with altitude in the troposphere"
            );
        }
    }

    #[test]
    fn temperature_is_constant_through_the_isothermal_layer() {
        // 11 km to 20 km has zero lapse rate by construction of the table.
        assert_eq!(temperature_isa(11_000.0), temperature_isa(15_000.0));
        assert_eq!(temperature_isa(15_000.0), temperature_isa(20_000.0));
    }

    #[test]
    fn pressure_agrees_with_the_layer_below_at_every_boundary() {
        // Continuity is what makes a table of independent per-layer formulas
        // describe one atmosphere. At a boundary altitude, `layer_index`
        // strictly picks the layer below (its condition is `altitude >
        // base`, not `>=`), so this checks that evaluating the lower layer's
        // formula at its own top reproduces the upper layer's stored base
        // pressure exactly -- which is what makes the table's precomputed
        // chain trustworthy in the first place. Guaranteed by construction:
        // each `base_pressure_pa` *is* the lower layer's formula evaluated at
        // that altitude.
        for i in 1..LAYERS.len() {
            let boundary_m = LAYERS[i].base_altitude_m;
            assert_eq!(layer_index(boundary_m), i - 1);
            assert_eq!(pressure_isa(boundary_m), LAYERS[i].base_pressure_pa);
        }
    }

    #[test]
    fn temperature_agrees_with_the_layer_below_at_every_boundary_but_the_last() {
        // Unlike pressure, each row's base temperature is an independent
        // value out of the CSV rather than something computed from the row
        // below, so this equality is not guaranteed the way pressure's is --
        // it happens to hold because the published lapse rates and base
        // temperatures are mutually consistent, except at the very last
        // boundary. There, linearly extrapolating layer 6 (-2 K/km from
        // 214.65 K) to 84852 m gives 186.946 K, half a bit above the table's
        // own -86.28 C (186.87 K) for layer 7: the published constant is
        // rounded to two decimal places and does not round-trip exactly.
        // `temperature_isa` always evaluates the layer *below* a boundary, so
        // it returns the extrapolated 186.946 K here, not the table's stored
        // value -- reproduced faithfully, not treated as a bug to fix.
        for i in 1..LAYERS.len() - 1 {
            let boundary_m = LAYERS[i].base_altitude_m;
            assert_eq!(temperature_isa(boundary_m), LAYERS[i].base_temperature_k);
        }

        let last = LAYERS.len() - 1;
        let boundary_m = LAYERS[last].base_altitude_m;
        assert_ne!(temperature_isa(boundary_m), LAYERS[last].base_temperature_k);
        assert!((temperature_isa(boundary_m) - LAYERS[last].base_temperature_k).abs() < 0.1);
    }

    #[test]
    fn below_the_first_layer_extrapolates_rather_than_clamping() {
        // The lower-bound branch in `pressure_isa`/`temperature_isa` reuses
        // layer 0's formula rather than returning the sea-level constant, so
        // pressure and temperature should keep rising below sea level.
        assert!(pressure_isa(-1000.0) > pressure_isa(0.0));
        assert!(temperature_isa(-1000.0) > temperature_isa(0.0));
    }
}
