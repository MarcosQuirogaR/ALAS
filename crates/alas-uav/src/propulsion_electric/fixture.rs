// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reviewed dimensionless propeller data and bounded interpolation.
//!
//! The legacy project carries the 436 official APC `PER3` performance files.
//! Repeating their fixed-width text in the executable would obscure the data
//! with headers and duplicate columns, so the fixture generator stores their
//! exact displayed decimal samples in a compact binary bundle.  The bundle is
//! deliberately decoded here rather than fitted or extrapolated: a requested
//! speed must still be bracketed by source samples.

use std::sync::OnceLock;

use serde::Deserialize;

use super::ElectricPropulsionError;

const APC_12X6E_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/prop_elec/apc_12x6e_fixture.json"
));
const APC_TABLE_BUNDLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/prop_elec/apc_performance_tables.bin"
));
const APC_TABLE_MAGIC: &[u8; 8] = b"ALASAPC1";
const MPH_TO_M_S: f64 = 0.447_04;
const INCH_TO_M: f64 = 0.0254;

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
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RpmCurve {
    rpm: f64,
    points: Vec<PropellerSample>,
}

/// A source-bounded propeller Ct/Cp table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropellerPerformanceMap {
    propeller_id: String,
    model: String,
    source_file: String,
    diameter_m: f64,
    curves: Vec<RpmCurve>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: u32,
    source: FixtureSource,
    curves: Vec<RpmCurve>,
}

#[derive(Debug, Deserialize)]
struct FixtureSource {
    propeller_id: String,
    diameter_m: f64,
}

impl PropellerPerformanceMap {
    /// Stable catalogue identifier for this table.
    pub fn propeller_id(&self) -> &str {
        &self.propeller_id
    }

    /// Printed APC model designation associated with the source table.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Original APC performance filename retained by the generated bundle.
    pub fn source_file(&self) -> &str {
        &self.source_file
    }

    /// Propeller diameter used by the dimensionless source table.
    pub fn diameter_m(&self) -> f64 {
        self.diameter_m
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

/// Load every supplied APC performance table, including non-electric variants.
///
/// A table being available means that thrust and torque can be evaluated only
/// within its documented speed/RPM envelope.  It does not assert that a
/// particular motor, installation, or airframe is verified.
pub fn apc_performance_maps() -> Result<&'static [PropellerPerformanceMap], ElectricPropulsionError>
{
    static TABLES: OnceLock<Result<Vec<PropellerPerformanceMap>, ElectricPropulsionError>> =
        OnceLock::new();
    TABLES
        .get_or_init(parse_apc_table_bundle)
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

/// Resolve one APC source table by its stable catalogue identifier.
pub fn apc_performance_map(
    propeller_id: &str,
) -> Result<&'static PropellerPerformanceMap, ElectricPropulsionError> {
    if propeller_id == "apc-12x6e" {
        return apc_12x6e_performance_map();
    }
    // The manufacturer catalogue uses an `E` suffix while the generated APC
    // tables retain the normalized PER3 identifier without that suffix.
    let table_id = match propeller_id.strip_suffix('e') {
        Some(id) if id.starts_with("apc-") => id,
        _ => propeller_id,
    };
    apc_performance_maps()?
        .iter()
        .find(|table| table.propeller_id == table_id)
        .ok_or_else(|| ElectricPropulsionError::UnsupportedPropeller {
            id: propeller_id.to_owned(),
        })
}

/// Load the normalized user-supplied APC 12x6E performance table.
pub fn apc_12x6e_performance_map(
) -> Result<&'static PropellerPerformanceMap, ElectricPropulsionError> {
    static TABLE: OnceLock<Result<PropellerPerformanceMap, ElectricPropulsionError>> =
        OnceLock::new();
    TABLE
        .get_or_init(parse_apc_12x6e)
        .as_ref()
        .map_err(Clone::clone)
}

fn parse_apc_12x6e() -> Result<PropellerPerformanceMap, ElectricPropulsionError> {
    let fixture: Fixture = serde_json::from_str(APC_12X6E_FIXTURE).map_err(|error| {
        ElectricPropulsionError::InvalidInput(format!(
            "embedded APC 12x6E fixture is invalid JSON: {error}"
        ))
    })?;
    if fixture.schema_version != 1 {
        return Err(ElectricPropulsionError::InvalidInput(
            "embedded APC 12x6E fixture has an unsupported schema or incomplete table".to_owned(),
        ));
    }
    let table = PropellerPerformanceMap {
        propeller_id: fixture.source.propeller_id,
        model: "12x6E".to_owned(),
        source_file: "PER3_12x6E.dat".to_owned(),
        diameter_m: fixture.source.diameter_m,
        curves: fixture.curves,
    };
    validate_table(&table, "embedded APC 12x6E fixture")?;
    Ok(table)
}

fn parse_apc_table_bundle() -> Result<Vec<PropellerPerformanceMap>, ElectricPropulsionError> {
    let mut reader = BundleReader::new(APC_TABLE_BUNDLE);
    if reader.take(APC_TABLE_MAGIC.len())? != APC_TABLE_MAGIC {
        return Err(bundle_error("has an invalid magic header"));
    }
    let schema_version = reader.u16()?;
    if schema_version != 1 {
        return Err(bundle_error("has an unsupported schema version"));
    }
    let table_count = usize::from(reader.u16()?);
    if table_count == 0 {
        return Err(bundle_error("contains no propeller tables"));
    }
    let mut tables = Vec::with_capacity(table_count);
    for _ in 0..table_count {
        let propeller_id = reader.ascii_string()?;
        let model = reader.ascii_string()?;
        let source_file = reader.ascii_string()?;
        let diameter_m = f64::from(reader.u32()?) * 1.0e-4 * INCH_TO_M;
        let _pitch_m = f64::from(reader.u32()?) * 1.0e-4 * INCH_TO_M;
        let curve_count = usize::from(reader.u16()?);
        let mut curves = Vec::with_capacity(curve_count);
        for _ in 0..curve_count {
            let rpm = f64::from(reader.u32()?);
            let sample_count = usize::from(reader.u16()?);
            let mut points = Vec::with_capacity(sample_count);
            for _ in 0..sample_count {
                points.push(PropellerSample {
                    speed_m_s: f64::from(reader.u16()?) * 0.01 * MPH_TO_M_S,
                    thrust_coefficient: f64::from(reader.i16()?) * 1.0e-4,
                    power_coefficient: f64::from(reader.i16()?) * 1.0e-4,
                });
            }
            curves.push(RpmCurve { rpm, points });
        }
        let table = PropellerPerformanceMap {
            propeller_id,
            model,
            source_file,
            diameter_m,
            curves,
        };
        validate_table(&table, "embedded APC performance bundle")?;
        tables.push(table);
    }
    if reader.remaining() != 0 {
        return Err(bundle_error("has trailing bytes"));
    }
    tables.sort_by(|left, right| left.propeller_id.cmp(&right.propeller_id));
    if tables
        .windows(2)
        .any(|pair| pair[0].propeller_id == pair[1].propeller_id)
    {
        return Err(bundle_error("contains duplicate propeller identifiers"));
    }
    Ok(tables)
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

struct BundleReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> BundleReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ElectricPropulsionError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| bundle_error("overflows"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| bundle_error("ends before a complete table"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u16(&mut self) -> Result<u16, ElectricPropulsionError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, ElectricPropulsionError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i16(&mut self) -> Result<i16, ElectricPropulsionError> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn ascii_string(&mut self) -> Result<String, ElectricPropulsionError> {
        let count = usize::from(self.take(1)?[0]);
        let bytes = self.take(count)?;
        if !bytes.is_ascii() {
            return Err(bundle_error("contains non-ASCII text"));
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| bundle_error("contains invalid text"))
    }
}

fn bundle_error(detail: &str) -> ElectricPropulsionError {
    ElectricPropulsionError::InvalidInput(format!("embedded APC performance bundle {detail}"))
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

fn interpolate(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + fraction * (upper - lower)
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
