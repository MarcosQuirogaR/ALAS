// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-field provenance wrapper and tabulated curves.
//!
//! Every number in a component row is a [`Field`]: the SI value, its unit, the
//! raw vendor text, the [`crate::Source`] it came from, when it was retrieved,
//! how it was obtained, under which test conditions it holds, and a confidence
//! in `[0, 1]`. A value that is unknown is `None` in the row, never a default.

use serde::{Deserialize, Serialize};

use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::SourceId;

/// How a value was obtained; sets the default confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Measured on a bench, in a tunnel or in flight.
    Measured,
    /// Stated by the manufacturer (datasheet or manufacturer web page).
    Datasheet,
    /// Computed by software (for example APC PER3 tables) or derived.
    Computed,
    /// Stated by a retailer page.
    Retailer,
    /// Entered by the user.
    User,
}

impl Method {
    /// Default confidence: measured 0.9, manufacturer 0.7, user 0.8,
    /// computed 0.6, retailer 0.5 (component-data.md section 4; the user
    /// value is this crate's choice and is documented in the schema notes).
    pub fn default_confidence(self) -> f64 {
        match self {
            Self::Measured => 0.9,
            Self::User => 0.8,
            Self::Datasheet => 0.7,
            Self::Computed => 0.6,
            Self::Retailer => 0.5,
        }
    }
}

/// Conditions under which a value was stated (all SI).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TestConditions {
    /// Supply or test voltage, V.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_v: Option<f64>,
    /// Temperature, K.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_k: Option<f64>,
    /// Free-text load condition (for example "static" or "30 A discharge").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<String>,
}

/// Where, when, how and how reliably a value is known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The citing [`crate::Source`].
    pub source_id: SourceId,
    /// Retrieval date; `None` means unknown and caps confidence at 0.5.
    pub retrieved_at: Option<IsoDate>,
    /// How the value was obtained.
    pub method: Method,
    /// The raw vendor text the value was parsed from, unit included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// Conditions the value holds under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestConditions>,
    /// Confidence in `[0, 1]`.
    pub confidence: f64,
}

impl Provenance {
    /// A provenance record with the method's default confidence.
    pub fn new(source_id: SourceId, retrieved_at: Option<IsoDate>, method: Method) -> Self {
        let mut confidence = method.default_confidence();
        if retrieved_at.is_none() {
            confidence = confidence.min(0.5);
        }
        Self {
            source_id,
            retrieved_at,
            method,
            raw: None,
            test: None,
            confidence,
        }
    }

    /// Check ranges; the source reference is resolved by the database.
    pub fn validate(&self, what: &str) -> DbResult<()> {
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(DbError::new(format!("{what}: confidence outside [0, 1]")));
        }
        if let Some(test) = &self.test {
            for (name, value) in [("voltage_v", test.voltage_v), ("temp_k", test.temp_k)] {
                if value.is_some_and(|v| !v.is_finite() || v <= 0.0) {
                    return Err(DbError::new(format!("{what}: test {name} must be > 0")));
                }
            }
        }
        Ok(())
    }
}

/// A value with its unit and provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field<T> {
    /// The value in `unit_si`.
    pub value: T,
    /// The SI unit symbol of `value` (see `units`).
    pub unit_si: String,
    /// Provenance, flattened beside the value in the serialized form.
    #[serde(flatten)]
    pub prov: Provenance,
}

impl<T> Field<T> {
    /// Build a field with default confidence for `method`.
    pub fn new(
        value: T,
        unit_si: &str,
        source_id: SourceId,
        retrieved_at: Option<IsoDate>,
        method: Method,
    ) -> Self {
        Self {
            value,
            unit_si: unit_si.to_owned(),
            prov: Provenance::new(source_id, retrieved_at, method),
        }
    }

    /// Attach the raw vendor text.
    #[must_use]
    pub fn with_raw(mut self, raw: impl Into<String>) -> Self {
        self.prov.raw = Some(raw.into());
        self
    }

    /// Attach test conditions.
    #[must_use]
    pub fn with_test(mut self, test: TestConditions) -> Self {
        self.prov.test = Some(test);
        self
    }

    /// Override the default confidence.
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.prov.confidence = confidence;
        self
    }
}

/// A numeric field in SI.
pub type Num = Field<f64>;

/// A tabulated relation `y(x)` with one provenance, optionally at a fixed
/// parameter (for example internal resistance versus state of charge at 298 K).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// Name of the abscissa (for example "soc", "voltage", "omega").
    pub x_name: String,
    /// SI unit of the abscissa.
    pub x_unit_si: String,
    /// SI unit of the ordinate.
    pub y_unit_si: String,
    /// Strictly increasing abscissa values.
    pub x: Vec<f64>,
    /// Ordinate values, same length as `x`.
    pub y: Vec<f64>,
    /// Fixed parameter the curve holds at, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<CurveParam>,
    /// Provenance of the whole table.
    #[serde(flatten)]
    pub prov: Provenance,
}

/// A named fixed parameter of a curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveParam {
    /// Parameter name (for example "temp").
    pub name: String,
    /// SI unit of the parameter.
    pub unit_si: String,
    /// Parameter value in SI.
    pub value: f64,
}

impl Curve {
    /// Check lengths, finiteness and monotone abscissa.
    pub fn validate(&self, what: &str) -> DbResult<()> {
        self.prov.validate(what)?;
        if self.x.len() != self.y.len() || self.x.is_empty() {
            return Err(DbError::new(format!(
                "{what}: curve needs matching, non-empty x and y"
            )));
        }
        if self.x.iter().chain(&self.y).any(|v| !v.is_finite()) {
            return Err(DbError::new(format!(
                "{what}: curve has a non-finite value"
            )));
        }
        if self.x.windows(2).any(|w| w[1] <= w[0]) {
            return Err(DbError::new(format!(
                "{what}: curve abscissa is not strictly increasing"
            )));
        }
        Ok(())
    }

    /// Piecewise-linear value at `x`, `None` outside the tabulated range
    /// (no extrapolation, matching the database rule that unknown stays unknown).
    pub fn at(&self, x: f64) -> Option<f64> {
        let first = *self.x.first()?;
        let last = *self.x.last()?;
        if !(first..=last).contains(&x) {
            return None;
        }
        if self.x.len() == 1 {
            return self.y.first().copied();
        }
        let upper = self.x.partition_point(|&v| v < x).max(1);
        let (x0, x1) = (self.x[upper - 1], self.x[upper]);
        let (y0, y1) = (self.y[upper - 1], self.y[upper]);
        Some(y0 + (y1 - y0) * (x - x0) / (x1 - x0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov() -> Result<Provenance, DbError> {
        Ok(Provenance::new(
            SourceId::new("s")?,
            None,
            Method::Datasheet,
        ))
    }

    #[test]
    fn unknown_retrieval_date_caps_confidence() -> Result<(), DbError> {
        assert_eq!(prov()?.confidence, 0.5);
        let dated = Provenance::new(
            SourceId::new("s")?,
            Some(IsoDate::parse("2026-10-12")?),
            Method::Datasheet,
        );
        assert_eq!(dated.confidence, 0.7);
        Ok(())
    }

    #[test]
    fn field_round_trips_through_json_with_flat_provenance(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let field = Field::new(0.1106, "m", SourceId::new("s")?, None, Method::Retailer)
            .with_raw("110.6 mm");
        let json = serde_json::to_value(&field)?;
        assert_eq!(json["unit_si"], "m");
        assert_eq!(json["source_id"], "s");
        assert!(json["retrieved_at"].is_null());
        let back: Field<f64> = serde_json::from_value(json)?;
        assert_eq!(back, field);
        Ok(())
    }

    #[test]
    fn curve_interpolates_inside_and_refuses_outside() -> Result<(), DbError> {
        let curve = Curve {
            x_name: "soc".into(),
            x_unit_si: "1".into(),
            y_unit_si: "Ohm".into(),
            x: vec![0.0, 0.5, 1.0],
            y: vec![0.03, 0.02, 0.01],
            param: None,
            prov: prov()?,
        };
        curve.validate("r")?;
        assert_eq!(curve.at(0.25), Some(0.025));
        assert_eq!(curve.at(1.0), Some(0.01));
        assert_eq!(curve.at(1.1), None);
        Ok(())
    }

    #[test]
    fn curve_rejects_unsorted_abscissa() -> Result<(), DbError> {
        let curve = Curve {
            x_name: "v".into(),
            x_unit_si: "V".into(),
            y_unit_si: "N*m".into(),
            x: vec![6.0, 4.8],
            y: vec![1.0, 0.8],
            param: None,
            prov: prov()?,
        };
        assert!(curve.validate("t").is_err());
        Ok(())
    }
}
