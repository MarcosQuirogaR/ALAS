// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Catalogue records for onboard equipment beyond the propulsion chain.

use serde::{Deserialize, Serialize};

use crate::catalog::{validate_optional_positive, CatalogError, Dimensions};

/// Radio receiver fields retained only where a source publishes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiverSpec {
    /// Maximum independently addressable output channels.
    pub channel_count: Option<u16>,
    /// Minimum supply voltage.
    pub min_voltage_v: Option<f64>,
    /// Maximum supply voltage.
    pub max_voltage_v: Option<f64>,
    /// Installed receiver mass.
    pub mass_kg: Option<f64>,
    /// Receiver outer dimensions.
    pub dimensions: Option<Dimensions>,
    /// Radio/control protocols printed by the source.
    #[serde(default)]
    pub protocols: Vec<String>,
    /// Whether the source explicitly identifies telemetry support.
    pub telemetry: Option<bool>,
}

impl ReceiverSpec {
    pub(crate) fn validate(&self, id: &str) -> Result<(), CatalogError> {
        if self.channel_count == Some(0) {
            return Err(CatalogError::new(format!(
                "receiver '{id}' has zero channels"
            )));
        }
        validate_optional_positive(id, "minimum voltage", self.min_voltage_v)?;
        validate_optional_positive(id, "maximum voltage", self.max_voltage_v)?;
        validate_optional_positive(id, "mass", self.mass_kg)?;
        if matches!((self.min_voltage_v, self.max_voltage_v), (Some(a), Some(b)) if a > b) {
            return Err(CatalogError::new(format!(
                "receiver '{id}' has an inverted voltage range"
            )));
        }
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}

/// Avionics or telemetry equipment without invented electrical loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectronicsSpec {
    /// Functional role printed by the source, such as `gps_sensor` or `bec`.
    pub role: String,
    /// Minimum supply voltage.
    pub min_voltage_v: Option<f64>,
    /// Maximum supply voltage.
    pub max_voltage_v: Option<f64>,
    /// Maximum current only when explicitly stated.
    pub max_current_a: Option<f64>,
    /// Installed mass.
    pub mass_kg: Option<f64>,
    /// Equipment outer dimensions.
    pub dimensions: Option<Dimensions>,
    /// Data or control protocols printed by the source.
    #[serde(default)]
    pub protocols: Vec<String>,
}

impl ElectronicsSpec {
    pub(crate) fn validate(&self, id: &str) -> Result<(), CatalogError> {
        if self.role.trim().is_empty() {
            return Err(CatalogError::new(format!(
                "electronics component '{id}' has no role"
            )));
        }
        validate_optional_positive(id, "minimum voltage", self.min_voltage_v)?;
        validate_optional_positive(id, "maximum voltage", self.max_voltage_v)?;
        validate_optional_positive(id, "maximum current", self.max_current_a)?;
        validate_optional_positive(id, "mass", self.mass_kg)?;
        if matches!((self.min_voltage_v, self.max_voltage_v), (Some(a), Some(b)) if a > b) {
            return Err(CatalogError::new(format!(
                "electronics component '{id}' has an inverted voltage range"
            )));
        }
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}

/// Landing-gear hardware described by a retail source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandingGearSpec {
    /// Retail form, such as `fixed_skid`, `retract_mechanism`, or `wheel`.
    pub form: String,
    /// Assembly mass only when explicitly published.
    pub mass_kg: Option<f64>,
    /// Maximum supported aircraft mass only when explicitly published.
    pub max_aircraft_mass_kg: Option<f64>,
    /// Assembly outer dimensions.
    pub dimensions: Option<Dimensions>,
    /// Nominal mechanism voltage where applicable.
    pub nominal_voltage_v: Option<f64>,
}

impl LandingGearSpec {
    pub(crate) fn validate(&self, id: &str) -> Result<(), CatalogError> {
        if self.form.trim().is_empty() {
            return Err(CatalogError::new(format!(
                "landing gear '{id}' has no form"
            )));
        }
        validate_optional_positive(id, "mass", self.mass_kg)?;
        validate_optional_positive(id, "maximum aircraft mass", self.max_aircraft_mass_kg)?;
        validate_optional_positive(id, "nominal voltage", self.nominal_voltage_v)?;
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}
