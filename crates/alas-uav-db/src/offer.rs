// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Offers (price and stock observations) and propeller performance sets.
//!
//! Both are separate tables from the spec rows: a price is a time series
//! observation about a component, never part of its specification, and a
//! performance map is evidence about a propeller from one source.

use serde::{Deserialize, Serialize};

use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::{ComponentId, SourceId};

/// What one unit of the quoted price buys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceBasis {
    /// One piece.
    Each,
    /// One kilogram of stock.
    PerKg,
    /// One metre of stock.
    PerMetre,
    /// One square metre of stock.
    PerSquareMetre,
}

/// One price and stock observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    /// The component the offer is for.
    pub component_id: ComponentId,
    /// Vendor name.
    pub vendor: String,
    /// Product page URL.
    pub url: String,
    /// Quoted price in `currency` per `basis`.
    pub price_amount: f64,
    /// What the price is per.
    pub basis: PriceBasis,
    /// ISO 4217 code, upper case.
    pub currency: String,
    /// Whether taxes are included in the price.
    pub taxes_included: bool,
    /// Units in stock when stated.
    #[serde(default)]
    pub stock: Option<u32>,
    /// Two-letter ISO 3166 country the item ships from, when stated.
    #[serde(default)]
    pub ship_from: Option<String>,
    /// Observation date.
    pub observed_at: IsoDate,
    /// The page or feed the observation came from.
    pub source_id: SourceId,
}

impl Offer {
    /// Check price, currency and country codes.
    pub fn validate(&self) -> DbResult<()> {
        let what = self.component_id.as_str();
        if !self.price_amount.is_finite() || self.price_amount <= 0.0 {
            return Err(DbError::new(format!(
                "offer for '{what}': price must be > 0"
            )));
        }
        let upper3 = |s: &str| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase());
        if !upper3(&self.currency) {
            return Err(DbError::new(format!(
                "offer for '{what}': currency must be a 3-letter upper-case ISO 4217 code"
            )));
        }
        if let Some(country) = &self.ship_from {
            if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_uppercase()) {
                return Err(DbError::new(format!(
                    "offer for '{what}': ship_from must be a 2-letter upper-case ISO 3166 code"
                )));
            }
        }
        if self.vendor.trim().is_empty() || !self.url.starts_with("https://") {
            return Err(DbError::new(format!(
                "offer for '{what}': vendor and an https url are required"
            )));
        }
        Ok(())
    }
}

/// Whether a performance map was measured or computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfKind {
    /// Wind-tunnel or thrust-stand data.
    Measured,
    /// Output of a proprietary or in-house model (for example APC PER3).
    Computed,
}

/// One operating point of a propeller map.
///
/// `j = V / (n D)` with `n` in revolutions per second and `D` the diameter, so
/// `j` is dimensionless and independent of the unit of rotation speed stored in
/// `omega_rad_s`. `ct = T / (rho n^2 D^4)`, `cp = P / (rho n^3 D^5)`, and
/// `eta = j ct / cp`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PropPerfRow {
    /// Shaft speed, rad/s.
    pub omega_rad_s: f64,
    /// Advance ratio (1), 0 at static.
    pub j: f64,
    /// Chord Reynolds number at 75 % radius when stated.
    #[serde(default)]
    pub re: Option<f64>,
    /// Thrust coefficient.
    pub ct: f64,
    /// Power coefficient.
    pub cp: f64,
    /// Propulsive efficiency when stated.
    #[serde(default)]
    pub eta: Option<f64>,
}

/// A propeller map from one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropPerfSet {
    /// The propeller this map belongs to.
    pub prop_id: ComponentId,
    /// Citing source.
    pub source_id: SourceId,
    /// Measured or computed.
    pub kind: PerfKind,
    /// Retrieval date, if known.
    pub retrieved_at: Option<IsoDate>,
    /// Operating points.
    pub rows: Vec<PropPerfRow>,
}

impl PropPerfSet {
    /// Check ranges of every row.
    pub fn validate(&self) -> DbResult<()> {
        let what = self.prop_id.as_str();
        if self.rows.is_empty() {
            return Err(DbError::new(format!("prop map for '{what}' has no rows")));
        }
        for (i, row) in self.rows.iter().enumerate() {
            let bad = !row.omega_rad_s.is_finite()
                || row.omega_rad_s <= 0.0
                || !row.j.is_finite()
                || row.j < 0.0
                || !row.ct.is_finite()
                || !row.cp.is_finite()
                || row.cp < 0.0
                || row.re.is_some_and(|r| !r.is_finite() || r <= 0.0)
                || row
                    .eta
                    .is_some_and(|e| !e.is_finite() || !(-1.0..=1.0).contains(&e));
            if bad {
                return Err(DbError::new(format!(
                    "prop map for '{what}': row {i} is outside omega > 0, j >= 0, cp >= 0, |eta| <= 1"
                )));
            }
        }
        Ok(())
    }
}
