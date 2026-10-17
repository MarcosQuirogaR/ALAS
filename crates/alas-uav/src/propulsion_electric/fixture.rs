// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dimensionless propeller data and bounded interpolation.
//!
//! Tables come from the user's local propeller data directory (see
//! [`super::prop_data`]); nothing is embedded in the executable.  They are
//! used as published: a requested speed and RPM must be bracketed by source
//! samples, and nothing is fitted or extrapolated.

use serde::Deserialize;

use super::prop_data::{loaded_prop_tables, unavailable_reason};
use super::ElectricPropulsionError;

/// One Ct/Cp sample at one RPM and true airspeed.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PropellerSample {
    /// True airspeed in metres per second.
    pub speed_m_s: f64,
    /// Thrust coefficient, `T / (rho n^2 D^4)`.
    pub thrust_coefficient: f64,
    /// Power coefficient, `P / (rho n^3 D^5)`.
    pub power_coefficient: f64,
}

/// Propeller samples at one shaft speed.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RpmCurve {
    pub(super) rpm: f64,
    pub(super) points: Vec<PropellerSample>,
}

/// A source-bounded propeller Ct/Cp table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropellerPerformanceMap {
    propeller_id: String,
    model: String,
    source_file: String,
    diameter_m: f64,
    pitch_m: f64,
    curves: Vec<RpmCurve>,
}

impl PropellerPerformanceMap {
    pub(super) fn from_parts(
        propeller_id: String,
        model: String,
        source_file: String,
        diameter_m: f64,
        pitch_m: f64,
        curves: Vec<RpmCurve>,
    ) -> Self {
        Self {
            propeller_id,
            model,
            source_file,
            diameter_m,
            pitch_m,
            curves,
        }
    }

    /// Stable catalogue identifier for this table.
    pub fn propeller_id(&self) -> &str {
        &self.propeller_id
    }

    /// Printed model designation associated with the source table.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Original performance filename of the imported table.
    pub fn source_file(&self) -> &str {
        &self.source_file
    }

    /// Propeller diameter used by the dimensionless source table.
    pub fn diameter_m(&self) -> f64 {
        self.diameter_m
    }

    /// Propeller pitch in metres, from the table title.
    pub fn pitch_m(&self) -> f64 {
        self.pitch_m
    }

    /// Inclusive RPM bounds that have data at a requested flight speed.
    pub fn rpm_bounds_at_speed(&self, speed_m_s: f64) -> Option<(f64, f64)> {
        let curves: Vec<&RpmCurve> = self
            .curves
            .iter()
            .filter(|curve| sample_at_speed(curve, speed_m_s).is_some())
            .collect();
        Some((curves.first()?.rpm, curves.last()?.rpm))
    }

    /// Bilinearly interpolated Ct/Cp values, without extrapolating the table.
    pub fn coefficients_at(&self, rpm: f64, speed_m_s: f64) -> Option<PropellerSample> {
        let available: Vec<(f64, PropellerSample)> = self
            .curves
            .iter()
            .filter_map(|curve| sample_at_speed(curve, speed_m_s).map(|sample| (curve.rpm, sample)))
            .collect();
        let (minimum_rpm, _) = *available.first()?;
        let (maximum_rpm, _) = *available.last()?;
        if !rpm.is_finite() || rpm < minimum_rpm || rpm > maximum_rpm {
            return None;
        }
        for window in available.windows(2) {
            let (lower_rpm, lower) = window[0];
            let (upper_rpm, upper) = window[1];
            if rpm >= lower_rpm && rpm <= upper_rpm {
                let fraction = (rpm - lower_rpm) / (upper_rpm - lower_rpm);
                return Some(PropellerSample {
                    speed_m_s,
                    thrust_coefficient: interpolate(
                        lower.thrust_coefficient,
                        upper.thrust_coefficient,
                        fraction,
                    ),
                    power_coefficient: interpolate(
                        lower.power_coefficient,
                        upper.power_coefficient,
                        fraction,
                    ),
                });
            }
        }
        None
    }
}

/// Every table found in the user's propeller data directory.
///
/// A table being available means that thrust and torque can be evaluated only
/// within its documented speed/RPM envelope.  It does not assert that a
/// particular motor, installation, or airframe is verified.  Fails with
/// [`ElectricPropulsionError::PropellerDataUnavailable`] when no table is
/// installed.
pub fn apc_performance_maps() -> Result<&'static [PropellerPerformanceMap], ElectricPropulsionError>
{
    let loaded = loaded_prop_tables();
    if loaded.tables.is_empty() {
        return Err(unavailable("any propeller", loaded));
    }
    Ok(loaded.tables.as_slice())
}

/// Resolve one table by its stable catalogue identifier.
///
/// The manufacturer catalogue spells electric models with an `E` suffix while
/// a table title may omit it (or the reverse), so the exact key is tried
/// first, then the key without and with a trailing `e`.
pub fn apc_performance_map(
    propeller_id: &str,
) -> Result<&'static PropellerPerformanceMap, ElectricPropulsionError> {
    let loaded = loaded_prop_tables();
    if loaded.tables.is_empty() {
        return Err(unavailable(propeller_id, loaded));
    }
    let appended = format!("{propeller_id}e");
    let candidates = [
        Some(propeller_id),
        propeller_id.strip_suffix('e'),
        Some(&appended),
    ];
    let tables: &'static [PropellerPerformanceMap] = loaded.tables.as_slice();
    let found = candidates
        .into_iter()
        .flatten()
        .find_map(|key| tables.iter().find(|table| table.propeller_id == key));
    found.ok_or_else(|| ElectricPropulsionError::UnsupportedPropeller {
        id: propeller_id.to_owned(),
    })
}

/// The table for the 12x6E propeller (`apc-12x6e`), when installed.
pub fn apc_12x6e_performance_map(
) -> Result<&'static PropellerPerformanceMap, ElectricPropulsionError> {
    apc_performance_map("apc-12x6e")
}

fn unavailable(id: &str, loaded: &super::prop_data::LoadedTables) -> ElectricPropulsionError {
    ElectricPropulsionError::PropellerDataUnavailable {
        id: id.to_owned(),
        reason: unavailable_reason(loaded),
    }
}

impl PropellerPerformanceMap {
    pub(super) fn validate(&self, source_name: &str) -> Result<(), ElectricPropulsionError> {
        validate_table(self, source_name)
    }
}

fn validate_table(
    table: &PropellerPerformanceMap,
    source_name: &str,
) -> Result<(), ElectricPropulsionError> {
    if !positive(table.diameter_m) || table.propeller_id.is_empty() || table.curves.len() < 2 {
        return Err(ElectricPropulsionError::InvalidInput(format!(
            "{source_name} has an unsupported schema or incomplete table"
        )));
    }
    let mut previous_rpm = 0.0;
    for curve in &table.curves {
        if !positive(curve.rpm) || curve.rpm <= previous_rpm || curve.points.len() < 2 {
            return Err(ElectricPropulsionError::InvalidInput(format!(
                "{source_name} has invalid RPM curves"
            )));
        }
        let mut previous_speed = -1.0;
        for sample in &curve.points {
            // High-advance-ratio propeller samples may be windmilling, so a
            // signed Ct or Cp is source data rather than a malformed table.
            if !sample.speed_m_s.is_finite()
                || sample.speed_m_s < 0.0
                || sample.speed_m_s <= previous_speed
                || !sample.thrust_coefficient.is_finite()
                || !sample.power_coefficient.is_finite()
            {
                return Err(ElectricPropulsionError::InvalidInput(format!(
                    "{source_name} has invalid Ct/Cp samples"
                )));
            }
            previous_speed = sample.speed_m_s;
        }
        previous_rpm = curve.rpm;
    }
    Ok(())
}

fn sample_at_speed(curve: &RpmCurve, speed_m_s: f64) -> Option<PropellerSample> {
    let first = curve.points.first()?;
    let last = curve.points.last()?;
    if !speed_m_s.is_finite() || speed_m_s < first.speed_m_s || speed_m_s > last.speed_m_s {
        return None;
    }
    if speed_m_s == first.speed_m_s {
        return Some(*first);
    }
    if speed_m_s == last.speed_m_s {
        return Some(*last);
    }
    if let Some(sample) = curve
        .points
        .iter()
        .find(|sample| sample.speed_m_s == speed_m_s)
    {
        return Some(*sample);
    }
    curve.points.windows(2).find_map(|window| {
        let lower = window[0];
        let upper = window[1];
        (speed_m_s > lower.speed_m_s && speed_m_s < upper.speed_m_s).then(|| {
            let fraction = (speed_m_s - lower.speed_m_s) / (upper.speed_m_s - lower.speed_m_s);
            PropellerSample {
                speed_m_s,
                thrust_coefficient: interpolate(
                    lower.thrust_coefficient,
                    upper.thrust_coefficient,
                    fraction,
                ),
                power_coefficient: interpolate(
                    lower.power_coefficient,
                    upper.power_coefficient,
                    fraction,
                ),
            }
        })
    })
}

pub(super) fn interpolate(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + fraction * (upper - lower)
}

pub(super) fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
