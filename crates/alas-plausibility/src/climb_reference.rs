// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Published times to climb (`data/climb_references.tsv`).

const FT: f64 = 0.3048;
const TABLE: &str = include_str!("../data/climb_references.tsv");

/// A published time to climb of a type: the altitude it is quoted to, at the
/// weight it is quoted for, and the sea-level rate of climb of the same source.
#[derive(Debug, Clone, PartialEq)]
pub struct ClimbReference {
    /// Altitude the time is quoted to (above sea level), m.
    pub altitude_m: f64,
    /// Weight it is quoted for, kg.
    pub mass_kg: f64,
    /// Time from sea level, s.
    pub time_s: f64,
    /// Rate of climb at sea level, m/s.
    pub sea_level_roc_m_s: f64,
    /// Where it comes from.
    pub source: String,
    /// A class reference (compared by mean rate) rather than a type's own time.
    pub class: bool,
}

/// The reference of `aircraft` (the registered preset name), when it has one.
pub fn reference_for(aircraft: &str) -> Option<ClimbReference> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .find_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 6 || fields[0] != aircraft {
                return None;
            }
            let number = |i: usize| fields[i].trim().parse::<f64>().ok();
            Some(ClimbReference {
                altitude_m: number(1)? * FT,
                mass_kg: number(2)?,
                time_s: number(3)? * 60.0,
                sea_level_roc_m_s: number(4)? * FT / 60.0,
                source: fields[5].trim().to_owned(),
                class: fields[0].starts_with("CLASS:"),
            })
        })
}

impl ClimbReference {
    /// Reference time, s, to climb from `field_m` to the quoted altitude at
    /// weight `mass_kg`: the quoted time less the time to climb from sea level
    /// to the field at the sea-level rate (the rate falls with altitude, so
    /// this takes off too little and errs on the lenient side), scaled with
    /// the weight (the climb rate is the excess power over the weight, so the
    /// time grows in proportion to it to first order).
    pub fn time_from_field_s(&self, field_m: f64, mass_kg: f64) -> f64 {
        let below_field_s = field_m.max(0.0) / self.sea_level_roc_m_s;
        (self.time_s - below_field_s) * mass_kg / self.mass_kg
    }
}

// Tests assert on a table row they just read, so a failed expect is the assertion
// failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_atr_factsheet_is_stored_with_its_source() {
        let reference = reference_for("ATR72-600").expect("the ATR 72-600 has a reference");
        assert!((reference.altitude_m - 17_000.0 * FT).abs() < 1e-9);
        assert_eq!(reference.time_s, 17.5 * 60.0);
        assert!(reference.source.contains("factsheet 2020 p.2"));
        assert!(reference_for("A320-200").is_none());
        // At MTOW from a 2,001 ft field the reference is shorter than from sea level.
        let from_field = reference.time_from_field_s(2_001.0 * FT, 22_800.0);
        assert!(from_field < 17.5 * 60.0 && from_field > 15.5 * 60.0);
    }
}
