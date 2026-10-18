// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit normalisation to the SI units stored at rest.
//!
//! Every stored number is SI. Importers read vendor units (`g`, `mm`, `mAh`,
//! `rpm/V`, `Wh`, `kg.cm`, ...) and convert here, keeping the raw text next to
//! the converted value. Conversions use an exact `value * mul / div` pair so
//! decimal prefixes (g to kg, mm to m, mAh to C) incur a single rounding, so
//! the result is within one ulp of the decimal value a human would type.
//!
//! One deliberate exception to "SI at rest": the vendor C rating is stored in
//! `1/h` (discharge current in amperes = C rating x capacity in ampere-hours),
//! because `1/s` would turn "45C" into 0.0125 and hide the number every
//! datasheet prints. Temperatures are kelvin; Celsius and Fahrenheit convert
//! with an offset.

use crate::error::DbError;

/// Physical quantity classes; a conversion only applies within one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    /// Mass, kg.
    Mass,
    /// Length, m.
    Length,
    /// Area, m2.
    Area,
    /// Time, s.
    Time,
    /// Electric current, A.
    Current,
    /// Electric potential, V.
    Voltage,
    /// Electrical resistance, Ohm.
    Resistance,
    /// Power, W.
    Power,
    /// Energy, J.
    Energy,
    /// Electric charge, C.
    Charge,
    /// Force, N.
    Force,
    /// Torque, N*m.
    Torque,
    /// Angular speed, rad/s.
    AngularSpeed,
    /// Motor speed constant, rad/s/V.
    SpeedConstant,
    /// Absolute temperature, K.
    Temperature,
    /// Mass density, kg/m3.
    Density,
    /// Pressure or stress, Pa.
    Pressure,
    /// Moment of inertia, kg*m2.
    Inertia,
    /// Thermal resistance, K/W.
    ThermalResistance,
    /// Areal mass, kg/m2.
    ArealMass,
    /// Vendor C rating, 1/h.
    CRate,
    /// Pure number.
    Dimensionless,
}

struct UnitDef {
    symbol: &'static str,
    quantity: Quantity,
    mul: f64,
    div: f64,
    offset: f64,
}

const fn u(symbol: &'static str, quantity: Quantity, mul: f64, div: f64) -> UnitDef {
    UnitDef {
        symbol,
        quantity,
        mul,
        div,
        offset: 0.0,
    }
}

const TAU_OVER_60: f64 = std::f64::consts::TAU / 60.0;

const UNITS: &[UnitDef] = &[
    u("kg", Quantity::Mass, 1.0, 1.0),
    u("g", Quantity::Mass, 1.0, 1000.0),
    u("mg", Quantity::Mass, 1.0, 1_000_000.0),
    u("oz", Quantity::Mass, 0.028_349_523_125, 1.0),
    u("lb", Quantity::Mass, 0.453_592_37, 1.0),
    u("m", Quantity::Length, 1.0, 1.0),
    u("mm", Quantity::Length, 1.0, 1000.0),
    u("cm", Quantity::Length, 1.0, 100.0),
    u("in", Quantity::Length, 0.0254, 1.0),
    u("ft", Quantity::Length, 0.3048, 1.0),
    u("m2", Quantity::Area, 1.0, 1.0),
    u("cm2", Quantity::Area, 1.0, 10_000.0),
    u("mm2", Quantity::Area, 1.0, 1_000_000.0),
    u("in2", Quantity::Area, 0.000_645_16, 1.0),
    u("s", Quantity::Time, 1.0, 1.0),
    u("ms", Quantity::Time, 1.0, 1000.0),
    u("min", Quantity::Time, 60.0, 1.0),
    u("h", Quantity::Time, 3600.0, 1.0),
    u("A", Quantity::Current, 1.0, 1.0),
    u("mA", Quantity::Current, 1.0, 1000.0),
    u("V", Quantity::Voltage, 1.0, 1.0),
    u("mV", Quantity::Voltage, 1.0, 1000.0),
    u("Ohm", Quantity::Resistance, 1.0, 1.0),
    u("mOhm", Quantity::Resistance, 1.0, 1000.0),
    u("W", Quantity::Power, 1.0, 1.0),
    u("kW", Quantity::Power, 1000.0, 1.0),
    u("J", Quantity::Energy, 1.0, 1.0),
    u("kJ", Quantity::Energy, 1000.0, 1.0),
    u("Wh", Quantity::Energy, 3600.0, 1.0),
    u("kWh", Quantity::Energy, 3_600_000.0, 1.0),
    u("C", Quantity::Charge, 1.0, 1.0),
    u("Ah", Quantity::Charge, 3600.0, 1.0),
    u("mAh", Quantity::Charge, 3600.0, 1000.0),
    u("N", Quantity::Force, 1.0, 1.0),
    u("gf", Quantity::Force, 0.980_665, 100.0),
    u("kgf", Quantity::Force, 9.806_65, 1.0),
    u("N*m", Quantity::Torque, 1.0, 1.0),
    u("kg.cm", Quantity::Torque, 9.806_65, 100.0),
    u("kgf.cm", Quantity::Torque, 9.806_65, 100.0),
    u("oz.in", Quantity::Torque, 0.007_061_551_814_5, 1.0),
    u("rad/s", Quantity::AngularSpeed, 1.0, 1.0),
    u("rpm", Quantity::AngularSpeed, TAU_OVER_60, 1.0),
    u("rad/s/V", Quantity::SpeedConstant, 1.0, 1.0),
    u("rpm/V", Quantity::SpeedConstant, TAU_OVER_60, 1.0),
    u("K", Quantity::Temperature, 1.0, 1.0),
    UnitDef {
        symbol: "degC",
        quantity: Quantity::Temperature,
        mul: 1.0,
        div: 1.0,
        offset: 273.15,
    },
    UnitDef {
        symbol: "degF",
        quantity: Quantity::Temperature,
        mul: 5.0,
        div: 9.0,
        offset: 255.372_222_222_222_2,
    },
    u("kg/m3", Quantity::Density, 1.0, 1.0),
    u("g/cm3", Quantity::Density, 1000.0, 1.0),
    u("Pa", Quantity::Pressure, 1.0, 1.0),
    u("MPa", Quantity::Pressure, 1_000_000.0, 1.0),
    u("GPa", Quantity::Pressure, 1_000_000_000.0, 1.0),
    u("psi", Quantity::Pressure, 6_894.757_293_168, 1.0),
    u("kg*m2", Quantity::Inertia, 1.0, 1.0),
    u("g*mm2", Quantity::Inertia, 1.0, 1_000_000_000.0),
    u("K/W", Quantity::ThermalResistance, 1.0, 1.0),
    u("kg/m2", Quantity::ArealMass, 1.0, 1.0),
    u("g/m2", Quantity::ArealMass, 1.0, 1000.0),
    u("1/h", Quantity::CRate, 1.0, 1.0),
    u("Crate", Quantity::CRate, 1.0, 1.0),
    u("1", Quantity::Dimensionless, 1.0, 1.0),
    u("%", Quantity::Dimensionless, 1.0, 100.0),
];

fn lookup(symbol: &str) -> Option<&'static UnitDef> {
    UNITS.iter().find(|def| def.symbol == symbol)
}

/// The quantity class of a unit symbol (SI or vendor), if known.
pub fn quantity_of(symbol: &str) -> Option<Quantity> {
    lookup(symbol).map(|def| def.quantity)
}

/// The SI unit symbol stored at rest for a quantity class.
pub fn si_symbol(quantity: Quantity) -> &'static str {
    match quantity {
        Quantity::Mass => "kg",
        Quantity::Length => "m",
        Quantity::Area => "m2",
        Quantity::Time => "s",
        Quantity::Current => "A",
        Quantity::Voltage => "V",
        Quantity::Resistance => "Ohm",
        Quantity::Power => "W",
        Quantity::Energy => "J",
        Quantity::Charge => "C",
        Quantity::Force => "N",
        Quantity::Torque => "N*m",
        Quantity::AngularSpeed => "rad/s",
        Quantity::SpeedConstant => "rad/s/V",
        Quantity::Temperature => "K",
        Quantity::Density => "kg/m3",
        Quantity::Pressure => "Pa",
        Quantity::Inertia => "kg*m2",
        Quantity::ThermalResistance => "K/W",
        Quantity::ArealMass => "kg/m2",
        Quantity::CRate => "1/h",
        Quantity::Dimensionless => "1",
    }
}

/// Convert `value` expressed in `unit` to the SI unit of the same quantity.
///
/// Returns the SI value and the quantity class. Non-finite input is rejected.
pub fn to_si(value: f64, unit: &str) -> Result<(f64, Quantity), DbError> {
    if !value.is_finite() {
        return Err(DbError::new(format!("non-finite value for unit '{unit}'")));
    }
    let def = lookup(unit).ok_or_else(|| DbError::new(format!("unknown unit '{unit}'")))?;
    Ok((value * def.mul / def.div + def.offset, def.quantity))
}

/// Convert `value` in `unit` to SI, requiring the unit to belong to `expected`.
pub fn to_si_as(value: f64, unit: &str, expected: Quantity) -> Result<f64, DbError> {
    let (si, quantity) = to_si(value, unit)?;
    if quantity == expected {
        Ok(si)
    } else {
        Err(DbError::new(format!(
            "unit '{unit}' is {quantity:?}, expected {expected:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_prefixes_are_exact_decimals() -> Result<(), DbError> {
        assert!((to_si(110.6, "mm")?.0 - 0.1106).abs() < 1e-16);
        assert_eq!(to_si(2600.0, "g")?.0, 2.6);
        assert_eq!(to_si(1800.0, "mAh")?.0, 6480.0);
        assert_eq!(to_si(1.8, "Ah")?.0, 6480.0);
        Ok(())
    }

    #[test]
    fn kv_converts_to_rad_per_second_per_volt() -> Result<(), DbError> {
        let (kv, quantity) = to_si(900.0, "rpm/V")?;
        assert_eq!(quantity, Quantity::SpeedConstant);
        assert!((kv - 94.247_779_607_693_8).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn temperature_and_torque_conversions() -> Result<(), DbError> {
        assert!((to_si(25.0, "degC")?.0 - 298.15).abs() < 1e-12);
        assert!((to_si(77.0, "degF")?.0 - 298.15).abs() < 1e-9);
        assert!((to_si(2.0, "kg.cm")?.0 - 0.196_133).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn wrong_class_and_unknown_units_fail() {
        assert!(to_si_as(1.0, "g", Quantity::Length).is_err());
        assert!(to_si(1.0, "furlong").is_err());
        assert!(to_si(f64::NAN, "g").is_err());
    }

    #[test]
    fn every_si_symbol_is_in_the_table_with_unit_factor() -> Result<(), DbError> {
        for def in UNITS {
            let si = si_symbol(def.quantity);
            let (value, quantity) = to_si(1.0, si)?;
            assert_eq!(quantity, def.quantity);
            assert_eq!(value, 1.0, "{si}");
        }
        Ok(())
    }
}
