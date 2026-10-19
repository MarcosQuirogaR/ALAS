// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit text from axis labels and the conversion of display units to SI.

use serde::{Deserialize, Serialize};

/// The SI unit of a quantity and the factor taking plotted values to it.
///
/// `si_value = plotted_value * factor`. Only pure scale conversions are listed;
/// offset units (degC, degF) are left without an SI mapping on purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiConversion {
    /// SI unit symbol.
    pub unit: String,
    /// Multiplier from the plotted unit to `unit`.
    pub factor: f64,
}

/// Extract the unit from the end of an axis label: `"Range [nm]"` or `"Range (nm)"`.
pub fn unit_from_label(label: &str) -> Option<String> {
    let trimmed = label.trim_end();
    let (open, close) = if trimmed.ends_with(']') {
        ('[', ']')
    } else if trimmed.ends_with(')') {
        ('(', ')')
    } else {
        return None;
    };
    let start = trimmed.rfind(open)?;
    let inner = trimmed[start + 1..trimmed.len() - close.len_utf8()].trim();
    (!inner.is_empty()).then(|| inner.to_owned())
}

/// SI conversion for a plotted unit, when it is a known pure-scale unit.
pub fn si_conversion(unit: &str) -> Option<SiConversion> {
    let compact: String = unit.chars().filter(|c| !c.is_whitespace()).collect();
    let (si, factor): (&str, f64) = match compact.as_str() {
        "-" | "1" | "Mach" | "mach" | "ratio" => ("1", 1.0),
        "%" | "%MAC" | "% MAC" => ("1", 1e-2),
        "m" => ("m", 1.0),
        "mm" => ("m", 1e-3),
        "cm" => ("m", 1e-2),
        "km" => ("m", 1e3),
        "ft" => ("m", 0.3048),
        "in" => ("m", 0.0254),
        "nm" | "nmi" | "NM" => ("m", 1852.0),
        "mi" => ("m", 1609.344),
        "m2" | "m^2" => ("m^2", 1.0),
        "ft2" | "ft^2" => ("m^2", 0.3048 * 0.3048),
        "m3" | "m^3" => ("m^3", 1.0),
        "L" | "l" => ("m^3", 1e-3),
        "s" => ("s", 1.0),
        "ms" => ("s", 1e-3),
        "min" => ("s", 60.0),
        "h" | "hr" => ("s", 3600.0),
        "kg" => ("kg", 1.0),
        "g" => ("kg", 1e-3),
        "t" => ("kg", 1e3),
        "lb" | "lbm" => ("kg", 0.453_592_37),
        "N" => ("N", 1.0),
        "kN" => ("N", 1e3),
        "lbf" => ("N", 4.448_221_615_260_5),
        "Pa" => ("Pa", 1.0),
        "kPa" => ("Pa", 1e3),
        "MPa" => ("Pa", 1e6),
        "GPa" => ("Pa", 1e9),
        "hPa" => ("Pa", 1e2),
        "psi" => ("Pa", 6_894.757_293_168),
        "m/s" => ("m/s", 1.0),
        "km/h" => ("m/s", 1.0 / 3.6),
        "kt" | "kts" | "kn" => ("m/s", 1852.0 / 3600.0),
        "mph" => ("m/s", 0.447_04),
        "ft/min" | "fpm" => ("m/s", 0.3048 / 60.0),
        "m/s2" | "m/s^2" => ("m/s^2", 1.0),
        "rad" => ("rad", 1.0),
        "deg" | "\u{b0}" => ("rad", std::f64::consts::PI / 180.0),
        "rad/s" => ("rad/s", 1.0),
        "deg/s" => ("rad/s", std::f64::consts::PI / 180.0),
        "W" => ("W", 1.0),
        "kW" => ("W", 1e3),
        "MW" => ("W", 1e6),
        "hp" => ("W", 745.699_871_582_270_2),
        "K" => ("K", 1.0),
        "N/m" => ("N/m", 1.0),
        "N/m2" | "N/m^2" => ("Pa", 1.0),
        "kg/m3" | "kg/m^3" => ("kg/m^3", 1.0),
        "kg/s" => ("kg/s", 1.0),
        "kg/m" => ("kg/m", 1.0),
        "Hz" => ("Hz", 1.0),
        "1/s" => ("1/s", 1.0),
        "N*m" | "Nm" | "N.m" => ("N*m", 1.0),
        "kg*m2" | "kg*m^2" | "kgm2" => ("kg*m^2", 1.0),
        _ => return None,
    };
    Some(SiConversion {
        unit: si.to_owned(),
        factor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_are_read_from_either_bracket_style() {
        assert_eq!(unit_from_label("Range [nm]").as_deref(), Some("nm"));
        assert_eq!(unit_from_label("Payload (t)").as_deref(), Some("t"));
        assert_eq!(unit_from_label("Mach number"), None);
    }

    #[test]
    fn display_units_convert_to_si() {
        let nm = si_conversion("nm").unwrap();
        assert_eq!((nm.unit.as_str(), nm.factor), ("m", 1852.0));
        assert!(si_conversion("degC").is_none());
    }
}
