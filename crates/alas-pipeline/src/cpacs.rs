// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 aircraft-data interchange.
//!
//! A CPACS consumer needs the computed outer mould line, not a hand-written
//! approximation assembled from configuration defaults. This module exports
//! the geometry after optimization and full analysis have built it. It
//! deliberately omits discipline inputs the physics core does not model rather
//! than inventing them.

mod aircraft;
mod model;
mod reader;
mod render;
mod validation;
mod wing_roles;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_config::AlasConfig;
use alas_mission::MissionResult;
use serde::{Deserialize, Serialize};

use crate::feasibility::FeasibilityReport;
use crate::full_analysis::AnalysisReport;

pub use aircraft::CpacsAircraftError;
pub use model::{
    CpacsAircraft, CpacsDocument, CpacsEngine, CpacsEnginePosition, CpacsFuselage,
    CpacsFuselageElement, CpacsFuselageProfile, CpacsFuselageSection, CpacsHeader, CpacsReference,
    CpacsSegment, CpacsTransformation, CpacsVersionInfo, CpacsWing, CpacsWingAirfoil,
    CpacsWingElement, CpacsWingSection, CPACS_35_VERSION,
};
pub use reader::{read_cpacs, read_cpacs_file, CpacsReadError};
pub use render::render_cpacs_v35;

/// Official CPACS 3.5 schema used to validate generated documents.
pub const CPACS_V35_SCHEMA_URL: &str = "https://www.cpacs.de/schema/v3_5/cpacs_schema.xsd";

pub(super) const AIRCRAFT_MODEL_UID: &str = "alas-aircraft-model";
pub(super) const ENGINE_UID: &str = "alas-engine";
pub(super) const FUSELAGE_PROFILE_UID: &str = "alas-fuselage-profile-unit-ellipse";

/// Files and stable identifiers produced by a CPACS export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpacsExportResult {
    /// Schema-validatable CPACS 3.5 document.
    pub path: PathBuf,
    /// UID of the exported aircraft model.
    pub aircraft_model_uid: String,
    /// UID of the shared engine definition referenced by all installations.
    pub engine_uid: String,
    /// Number of lifting surfaces exported.
    pub wing_count: usize,
    /// Number of fuselage-shaped bodies exported, including nacelles.
    pub fuselage_count: usize,
    /// Number of engine installations represented by nacelle bodies.
    pub engine_position_count: usize,
    /// CPACS version declared in the artifact.
    pub cpacs_version: String,
}

/// Final run manifest that keeps CPACS as the aircraft authority while
/// retaining the status and paths of non-CPACS execution adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpacsRunManifest {
    /// CPACS document consumed by downstream adapters.
    pub cpacs_input: String,
    /// CPACS schema version declared by the document.
    pub cpacs_version: String,
    /// Stable aircraft model UID shared by all adapters.
    pub aircraft_model_uid: String,
    /// Stable engine UID shared by all adapters.
    pub engine_uid: String,
    /// Stage statuses, with `completed` reserved for observed completion.
    pub stages: BTreeMap<String, String>,
    /// Retained adapter artifacts relative to the run output directory.
    pub artifacts: BTreeMap<String, String>,
}

/// Why a CPACS export could not represent the computed geometry faithfully.
#[derive(Debug, thiserror::Error)]
pub enum CpacsExportError {
    /// The target document could not be written.
    #[error("could not write CPACS export: {0}")]
    Io(#[from] io::Error),
    /// CPACS aircraft data requires an aircraft with lifting geometry.
    #[error("CPACS export requires at least one wing")]
    NoWings,
    /// A CPACS wing needs two or more sections.
    #[error("wing {name:?} has {count} sections; CPACS requires at least two")]
    TooFewWingSections {
        /// Source wing name.
        name: String,
        /// Source section count.
        count: usize,
    },
    /// A CPACS fuselage needs two or more sections.
    #[error("fuselage {name:?} has {count} sections; CPACS requires at least two")]
    TooFewFuselageSections {
        /// Source fuselage name.
        name: String,
        /// Source section count.
        count: usize,
    },
    /// A CPACS airfoil profile needs a closed or open contour of three points.
    #[error("airfoil {name:?} has {count} coordinates; CPACS export requires at least three")]
    TooFewAirfoilPoints {
        /// Source airfoil name.
        name: String,
        /// Source point count.
        count: usize,
    },
    /// A source value cannot be represented as an XML Schema double.
    #[error("non-finite value in {context}")]
    NonFinite {
        /// Human-readable source field.
        context: String,
    },
    /// Technology tag and typed engine payload do not form a valid binding.
    #[error("invalid propulsion binding: {0}")]
    InvalidEngineBinding(String),
    /// A section has no projected span direction, so its local orientation is undefined.
    #[error("wing {name:?} section {section} has no YZ span direction")]
    DegenerateWingStation {
        /// Source wing name.
        name: String,
        /// Source section index.
        section: usize,
    },
}

/// Export computed geometry as a CPACS 3.5 aircraft document.
pub fn export_cpacs(
    report: &AnalysisReport,
    config: &AlasConfig,
    path: &Path,
) -> Result<CpacsExportResult, CpacsExportError> {
    export_cpacs_document(report, config, path).map(|(export, _)| export)
}

/// Export computed geometry as a CPACS 3.5 aircraft document and return the
/// XML that was written.
///
/// A caller that reads the aircraft back should parse the returned XML rather
/// than the file: another run writing to the same output directory can
/// replace the file between this write and a later read.
pub fn export_cpacs_document(
    report: &AnalysisReport,
    config: &AlasConfig,
    path: &Path,
) -> Result<(CpacsExportResult, String), CpacsExportError> {
    let document = render::render_cpacs_v35_with_analysis(
        report,
        config,
        None,
        None,
        &current_timestamp_utc(),
    )?;
    let export = write_export(report, path, &document)?;
    Ok((export, document))
}

/// Export a CPACS 3.5 aircraft document and the standard analyses available
/// from the completed non-GUI pipeline.
pub fn export_cpacs_with_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
    path: &Path,
) -> Result<CpacsExportResult, CpacsExportError> {
    let document = render::render_cpacs_v35_with_analysis(
        report,
        config,
        feasibility,
        mission,
        &current_timestamp_utc(),
    )?;
    write_export(report, path, &document)
}

/// Write `document` to `path` whole: it is written to a sibling file unique to
/// this process and write, then renamed over the target, so a concurrent
/// reader never sees a truncated document.
fn write_document_atomically(path: &Path, document: &str) -> io::Result<()> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut staging = path.as_os_str().to_owned();
    staging.push(format!(".{}-{sequence}.partial", std::process::id()));
    let staging = PathBuf::from(staging);
    if let Err(error) = fs::write(&staging, document).and_then(|()| fs::rename(&staging, path)) {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    Ok(())
}

fn write_export(
    report: &AnalysisReport,
    path: &Path,
    document: &str,
) -> Result<CpacsExportResult, CpacsExportError> {
    write_document_atomically(path, document)?;

    Ok(CpacsExportResult {
        path: path.to_path_buf(),
        aircraft_model_uid: AIRCRAFT_MODEL_UID.to_owned(),
        engine_uid: ENGINE_UID.to_owned(),
        wing_count: report.airplane.wings.len(),
        fuselage_count: report.airplane.fuselages.len(),
        engine_position_count: report
            .airplane
            .fuselages
            .iter()
            .filter(|fuselage| render::is_nacelle(fuselage))
            .count(),
        cpacs_version: "3.5".to_owned(),
    })
}

/// Write the CPACS-centered manifest after the optional analysis stages finish.
pub fn write_cpacs_run_manifest(
    path: &Path,
    export: &CpacsExportResult,
    stages: BTreeMap<String, String>,
    artifacts: BTreeMap<String, String>,
) -> io::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let manifest = CpacsRunManifest {
        cpacs_input: export.path.display().to_string(),
        cpacs_version: export.cpacs_version.clone(),
        aircraft_model_uid: export.aircraft_model_uid.clone(),
        engine_uid: export.engine_uid.clone(),
        stages,
        artifacts,
    };
    let file = fs::File::create(path)?;
    serde_json::to_writer_pretty(file, &manifest).map_err(io::Error::other)?;
    Ok(path.to_path_buf())
}

fn current_timestamp_utc() -> String {
    let elapsed = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed,
        Err(_) => std::time::Duration::ZERO,
    };
    let seconds = elapsed.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_unix_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    // A test asserts on values it constructed, so a failed unwrap is the
    // assertion failing.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Concurrent writers to one path never let a reader see a partial
    /// document: every read is one writer's complete text.
    #[test]
    fn concurrent_writes_to_one_path_are_never_read_partially() {
        let dir = std::env::temp_dir().join(format!(
            "alas-cpacs-atomic-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let path = dir.join("cpacs/aircraft.cpacs.xml");
        let documents: Vec<String> = (0..4)
            .map(|writer| format!("<cpacs>{}</cpacs>", format!("{writer}").repeat(200_000)))
            .collect();
        write_document_atomically(&path, &documents[0]).unwrap();
        std::thread::scope(|scope| {
            for document in &documents {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..20 {
                        write_document_atomically(path, document).unwrap();
                    }
                });
            }
            for _ in 0..200 {
                if let Ok(text) = fs::read_to_string(&path) {
                    assert!(
                        documents.contains(&text),
                        "read a partial document of {} bytes",
                        text.len()
                    );
                }
            }
        });
        let _ = fs::remove_dir_all(&dir);
    }
}
