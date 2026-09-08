// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl EscSpec {
    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        for (field, value) in [
            ("continuous current", self.continuous_current_a),
            ("BEC minimum voltage", self.bec_min_voltage_v),
            ("BEC maximum voltage", self.bec_max_voltage_v),
            ("BEC continuous current", self.bec_continuous_current_a),
            ("BEC peak current", self.bec_peak_current_a),
            ("mass", self.mass_kg),
        ] {
            validate_optional_positive(id, field, value)?;
        }
        validate_cell_range(id, self.min_series_cells, self.max_series_cells)?;
        if let (Some(minimum), Some(maximum)) = (self.bec_min_voltage_v, self.bec_max_voltage_v) {
            if minimum > maximum {
                return Err(CatalogError::new(format!(
                    "ESC '{id}' has an inverted BEC voltage range"
                )));
            }
        }
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}

/// Servo performance at a stated supply voltage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ServoOperatingPoint {
    /// Supply voltage.
    pub voltage_v: f64,
    /// Stall torque in newton metres.
    pub stall_torque_nm: f64,
    /// Transit time for 60 degrees, in seconds.
    pub seconds_per_60_deg: Option<f64>,
    /// Stall current when published.
    pub stall_current_a: Option<f64>,
}

/// Servo mass, dimensions, and voltage-indexed performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServoSpec {
    /// Servo mass.
    pub mass_kg: Option<f64>,
    /// Servo outer dimensions.
    pub dimensions: Option<Dimensions>,
    /// Published performance points ordered by voltage.
    pub operating_points: Vec<ServoOperatingPoint>,
}

impl ServoSpec {
    /// Conservative linear torque interpolation inside the published range.
    pub fn stall_torque_at_voltage(&self, voltage_v: f64) -> Option<f64> {
        interpolate_points(&self.operating_points, voltage_v, |point| {
            point.stall_torque_nm
        })
    }

    /// Conservative linear stall-current interpolation when both endpoints exist.
    pub fn stall_current_at_voltage(&self, voltage_v: f64) -> Option<f64> {
        interpolate_optional_points(&self.operating_points, voltage_v, |point| {
            point.stall_current_a
        })
    }

    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        validate_optional_positive(id, "mass", self.mass_kg)?;
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        let mut last_voltage = 0.0;
        for point in &self.operating_points {
            if !positive(point.voltage_v)
                || !positive(point.stall_torque_nm)
                || point.voltage_v <= last_voltage
                || point
                    .seconds_per_60_deg
                    .is_some_and(|value| !positive(value))
                || point.stall_current_a.is_some_and(|value| !positive(value))
            {
                return Err(CatalogError::new(format!(
                    "servo '{id}' has an invalid operating point"
                )));
            }
            last_voltage = point.voltage_v;
        }
        Ok(())
    }
}

/// Geometric propeller data without an invented performance map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropellerSpec {
    /// Propeller diameter.
    pub diameter_m: Option<f64>,
    /// Geometric pitch.
    pub pitch_m: Option<f64>,
    /// Blade count.
    pub blade_count: Option<u16>,
    /// Propeller mass.
    pub mass_kg: Option<f64>,
    /// Nominal bore diameter before any adapter.
    pub bore_diameter_m: Option<f64>,
    /// Whether the source explicitly permits electric propulsion.
    pub electric_compatible: Option<bool>,
}

impl PropellerSpec {
    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        for (field, value) in [
            ("diameter", self.diameter_m),
            ("pitch", self.pitch_m),
            ("mass", self.mass_kg),
            ("bore diameter", self.bore_diameter_m),
        ] {
            validate_optional_positive(id, field, value)?;
        }
        if self.blade_count == Some(0) {
            return Err(CatalogError::new(format!(
                "propeller '{id}' has zero blades"
            )));
        }
        Ok(())
    }
}

/// Purchased material stock; strength remains absent unless published.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialStockSpec {
    /// Retail form, such as `sheet` or `tube`.
    pub form: String,
    /// Stock dimensions.
    pub dimensions: Option<Dimensions>,
    /// Purchased stock mass.
    pub mass_kg: Option<f64>,
    /// Young's modulus only when supplied by a traceable data sheet.
    pub youngs_modulus_pa: Option<f64>,
    /// Strength allowable only when supplied by a traceable data sheet.
    pub allowable_stress_pa: Option<f64>,
}

impl MaterialStockSpec {
    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        if self.form.trim().is_empty() {
            return Err(CatalogError::new(format!(
                "material stock '{id}' has no form"
            )));
        }
        for (field, value) in [
            ("mass", self.mass_kg),
            ("Young's modulus", self.youngs_modulus_pa),
            ("allowable stress", self.allowable_stress_pa),
        ] {
            validate_optional_positive(id, field, value)?;
        }
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}

fn interpolate_points(
    points: &[ServoOperatingPoint],
    voltage_v: f64,
    value: impl Fn(&ServoOperatingPoint) -> f64,
) -> Option<f64> {
    let exact = points
        .iter()
        .find(|point| point.voltage_v == voltage_v)
        .map(&value);
    if exact.is_some() {
        return exact;
    }
    points.windows(2).find_map(|window| {
        let lower = &window[0];
        let upper = &window[1];
        (voltage_v > lower.voltage_v && voltage_v < upper.voltage_v).then(|| {
            let fraction = (voltage_v - lower.voltage_v) / (upper.voltage_v - lower.voltage_v);
            value(lower) + fraction * (value(upper) - value(lower))
        })
    })
}

fn interpolate_optional_points(
    points: &[ServoOperatingPoint],
    voltage_v: f64,
    value: impl Fn(&ServoOperatingPoint) -> Option<f64>,
) -> Option<f64> {
    if let Some(point) = points.iter().find(|point| point.voltage_v == voltage_v) {
        return value(point);
    }
    points.windows(2).find_map(|window| {
        let lower = &window[0];
        let upper = &window[1];
        if voltage_v <= lower.voltage_v || voltage_v >= upper.voltage_v {
            return None;
        }
        let lower_value = value(lower)?;
        let upper_value = value(upper)?;
        let fraction = (voltage_v - lower.voltage_v) / (upper.voltage_v - lower.voltage_v);
        Some(lower_value + fraction * (upper_value - lower_value))
    })
}

fn validate_cell_range(
    id: &str,
    minimum: Option<u16>,
    maximum: Option<u16>,
) -> Result<(), CatalogError> {
    if minimum == Some(0)
        || maximum == Some(0)
        || matches!((minimum, maximum), (Some(a), Some(b)) if a > b)
    {
        Err(CatalogError::new(format!(
            "component '{id}' has an invalid cell-count range"
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_optional_positive(
    id: &str,
    field: &str,
    value: Option<f64>,
) -> Result<(), CatalogError> {
    if value.is_some_and(|value| !positive(value)) {
        Err(CatalogError::new(format!(
            "component '{id}' has invalid {field}"
        )))
    } else {
        Ok(())
    }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

