// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit symbols understood by the score language and by the `unit_si`
//! strings of the mission outputs and preset parameters.
//!
//! Every entry is `(symbol, factor to SI, dimension)`. Conversion constants
//! are the exact definitions (inch, pound, foot) or the standard values
//! (g0 = 9.80665 m/s^2). Customary units appear because the rulebooks quote
//! them (SAE: lb, in, ft, fl oz; DBF: ft, lb, Wh).

use super::quantity::{Dim, Quantity};

const L: Dim = Dim::LENGTH;
const M: Dim = Dim::MASS;
const T: Dim = Dim::TIME;
const I: Dim = Dim::CURRENT;

fn force() -> Dim {
    M.times(L).over(T.powi(2))
}
fn energy() -> Dim {
    force().times(L)
}
fn power() -> Dim {
    energy().over(T)
}
fn voltage() -> Dim {
    power().over(I)
}
fn charge() -> Dim {
    I.times(T)
}

/// Look up a unit or named constant by symbol.
pub fn lookup(symbol: &str) -> Option<Quantity> {
    let q = |factor: f64, dim: Dim| Some(Quantity::new(factor, dim));
    match symbol {
        "m" => q(1.0, L),
        "km" => q(1e3, L),
        "cm" => q(1e-2, L),
        "mm" => q(1e-3, L),
        "in" => q(0.0254, L),
        "ft" => q(0.3048, L),
        "kg" => q(1.0, M),
        "g" => q(1e-3, M),
        "lb" => q(0.453_592_37, M),
        "oz" => q(0.028_349_523_125, M),
        "s" => q(1.0, T),
        "ms" => q(1e-3, T),
        "min" => q(60.0, T),
        "h" => q(3600.0, T),
        "Hz" => q(1.0, T.powi(-1)),
        "A" => q(1.0, I),
        "mA" => q(1e-3, I),
        "C" => q(1.0, charge()),
        "mAh" => q(3.6, charge()),
        "Ah" => q(3600.0, charge()),
        "K" => q(1.0, Dim::TEMPERATURE),
        "N" => q(1.0, force()),
        "lbf" => q(4.448_221_615_260_5, force()),
        "J" => q(1.0, energy()),
        "Wh" => q(3600.0, energy()),
        "kWh" => q(3.6e6, energy()),
        "W" => q(1.0, power()),
        "kW" => q(1e3, power()),
        "V" => q(1.0, voltage()),
        "L" => q(1e-3, L.powi(3)),
        "floz" => q(2.957_352_956_25e-5, L.powi(3)),
        "Pa" => q(1.0, force().over(L.powi(2))),
        "rad" => q(1.0, Dim::ONE),
        "deg" => q(std::f64::consts::PI / 180.0, Dim::ONE),
        "percent" => q(0.01, Dim::ONE),
        "EUR" => q(1.0, Dim::COST),
        "pi" => q(std::f64::consts::PI, Dim::ONE),
        // Standard gravity (CGPM 1901).
        "g0" => q(9.806_65, L.over(T.powi(2))),
        _ => None,
    }
}

/// Parse an SI-or-customary unit string such as `A*s`, `J/m`, `m3`, `kg*m2`
/// or `1` into its factor and dimension.
pub fn parse_unit(text: &str) -> Result<Quantity, String> {
    let text = text.trim();
    if text.is_empty() || text == "1" {
        return Ok(Quantity::number(1.0));
    }
    let mut acc = Quantity::number(1.0);
    let mut divide = false;
    let mut token = String::new();
    let flush = |token: &mut String, divide: bool, acc: &mut Quantity| -> Result<(), String> {
        if token.is_empty() {
            return Ok(());
        }
        let f = factor(token)?;
        let f = if divide {
            Quantity::new(1.0 / f.value, Dim::ONE.over(f.dim))
        } else {
            f
        };
        *acc = Quantity::new(acc.value * f.value, acc.dim.times(f.dim));
        token.clear();
        Ok(())
    };
    for c in text.chars() {
        match c {
            '*' => flush(&mut token, divide, &mut acc)?,
            '/' => {
                flush(&mut token, divide, &mut acc)?;
                divide = true;
            }
            _ => token.push(c),
        }
    }
    flush(&mut token, divide, &mut acc)?;
    Ok(acc)
}

/// One factor: `1`, a symbol, or a symbol with an exponent (`m3`, `s^2`).
fn factor(token: &str) -> Result<Quantity, String> {
    if token == "1" {
        return Ok(Quantity::number(1.0));
    }
    if let Some(q) = lookup(token) {
        return Ok(q);
    }
    // `s^0.5`, `m^2`, `m3`: the exponent follows a caret or trails the symbol.
    let (name, exponent) = match token.split_once('^') {
        Some((n, e)) => (n, e),
        None => {
            let split = token.trim_end_matches(|c: char| c.is_ascii_digit());
            (split, &token[split.len()..])
        }
    };
    let power = exponent.parse::<f64>().ok();
    match (lookup(name), power) {
        (Some(q), Some(p)) => {
            let dim = q
                .dim
                .powf(p)
                .ok_or_else(|| format!("unit '{token}' has a non-representable dimension"))?;
            Ok(Quantity::new(q.value.powf(p), dim))
        }
        _ => Err(format!("unknown unit '{token}'")),
    }
}
