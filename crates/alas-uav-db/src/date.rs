// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A calendar date in ISO 8601 `YYYY-MM-DD` form.
//!
//! Provenance needs dates (retrieval, validity intervals, price observation)
//! but not times of day or time zones, so the crate carries a validated string
//! instead of a date-time dependency. Lexicographic order equals chronological
//! order for four-digit years, which makes interval checks plain comparisons.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::DbError;

/// A validated calendar date (proleptic Gregorian, years 1900-2999).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IsoDate(String);

impl IsoDate {
    /// Parse `YYYY-MM-DD`, checking month length and leap years.
    pub fn parse(text: &str) -> Result<Self, DbError> {
        let bytes = text.as_bytes();
        let shape_ok = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
        if !shape_ok {
            return Err(DbError::new(format!("'{text}' is not a YYYY-MM-DD date")));
        }
        let year: u32 = text[0..4].parse().unwrap_or(0);
        let month: u32 = text[5..7].parse().unwrap_or(0);
        let day: u32 = text[8..10].parse().unwrap_or(0);
        if !(1900..=2999).contains(&year) || !(1..=12).contains(&month) {
            return Err(DbError::new(format!(
                "'{text}' has an invalid year or month"
            )));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(DbError::new(format!("'{text}' has an invalid day")));
        }
        Ok(Self(text.to_owned()))
    }

    /// The canonical text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    }
}

impl Serialize for IsoDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for IsoDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_dates_and_rejects_impossible_ones() {
        assert!(IsoDate::parse("2026-10-12").is_ok());
        assert!(IsoDate::parse("2028-02-29").is_ok());
        assert!(IsoDate::parse("2026-02-29").is_err());
        assert!(IsoDate::parse("2026-13-01").is_err());
        assert!(IsoDate::parse("2026-1-01").is_err());
        assert!(IsoDate::parse("12/10/2026").is_err());
    }

    #[test]
    fn text_order_is_chronological() {
        let a = IsoDate::parse("2026-01-31").unwrap_or_else(|e| panic!("{e}"));
        let b = IsoDate::parse("2026-02-01").unwrap_or_else(|e| panic!("{e}"));
        assert!(a < b);
    }
}
