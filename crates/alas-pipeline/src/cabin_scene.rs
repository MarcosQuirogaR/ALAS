// SPDX-License-Identifier: AGPL-3.0-or-later
//! Versioned, renderer-neutral cabin scene produced from a completed analysis.

use std::{fs::File, io, path::Path};

use alas_config::AlasConfig;
use alas_payload::geometry::CabinGeometry;
use serde::{Deserialize, Serialize};

use crate::AnalysisReport;

mod build;

/// Major-versioned interchange identifier.
pub const CABIN_SCENE_SCHEMA_VERSION: &str = "alas.cabin-scene/v2";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CabinScene {
    pub schema_version: String,
    pub units: SceneUnits,
    pub frame: CoordinateFrame,
    pub provenance: SceneProvenance,
    pub stations: Vec<SectionStation>,
    pub recommended_sections: Vec<RecommendedSection>,
    pub decks: Vec<ResolvedDeck>,
    pub seat_rows: Vec<SeatRow>,
    pub seats: Vec<Seat>,
    pub windows: NominalWindows,
    pub overhead: OverheadSystem,
    pub cargo: CargoSystem,
    pub missing_inputs: Vec<MissingInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SceneUnits {
    pub length: String,
    pub mass: String,
    pub angle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CoordinateFrame {
    pub origin: String,
    pub x: String,
    pub y: String,
    pub z: String,
    pub handedness: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SceneProvenance {
    pub producer: String,
    pub source: String,
    pub aircraft_preset: Option<String>,
    pub cabin_preset: String,
    pub optimized_design: alas_config::DesignVector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Point2 {
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SourcedContour {
    pub points_yz_m: Vec<Point2>,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SectionStation {
    pub id: String,
    pub x_m: f64,
    pub outer: SourcedContour,
    pub inner: Option<SourcedContour>,
    pub liner: Option<SourcedContour>,
    pub hold: Option<SourcedContour>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct RecommendedSection {
    pub station_id: String,
    pub purpose: String,
    pub deck_id: String,
    pub x_m: f64,
    pub intersects: Vec<String>,
    pub selection: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ResolvedDeck {
    pub id: String,
    pub floor_z_m: f64,
    pub ceiling_z_m: f64,
    pub usable_width_m: f64,
    pub station_x_m: f64,
    pub passenger: bool,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Box3 {
    pub center_x_m: f64,
    pub center_y_m: f64,
    pub center_z_m: f64,
    pub length_m: f64,
    pub width_m: f64,
    pub height_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SeatRow {
    pub id: String,
    pub deck_id: String,
    pub envelope: Box3,
    pub class: String,
    pub abreast: i64,
    pub filled: i64,
    pub blocks: Vec<i64>,
    pub aisle_width_m: f64,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Seat {
    pub id: String,
    pub row_id: String,
    pub deck_id: String,
    pub center_x_m: f64,
    pub center_y_m: f64,
    pub center_z_m: f64,
    pub width_m: f64,
    pub occupied: Option<bool>,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct NominalWindows {
    pub apertures: Vec<WindowAperture>,
    pub status: String,
    pub source: String,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct WindowAperture {
    pub id: String,
    pub deck_id: String,
    pub x_m: f64,
    pub center_yz_m: Point2,
    pub outward_normal_yz: Point2,
    pub width_m: f64,
    pub height_m: f64,
    pub fidelity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct OverheadSystem {
    pub runs: Vec<OverheadRun>,
    pub topology: Vec<OverheadTopology>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct OverheadRun {
    pub id: String,
    pub deck_id: String,
    pub kind: String,
    pub envelope: Box3,
    pub profile_yz_m: Vec<Point2>,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct OverheadTopology {
    pub run_id: String,
    pub rail_ids: Vec<String>,
    pub valance_ids: Vec<String>,
    pub psu_ids: Vec<String>,
    pub attachment_ids: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CargoSystem {
    pub slots: Vec<CargoSlot>,
    pub items: Vec<CargoItem>,
    pub empty_slot_inventory: CargoInventoryStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CargoSlot {
    pub id: String,
    pub deck_id: String,
    pub envelope: Box3,
    pub occupied_by: String,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CargoItem {
    pub id: String,
    pub slot_id: String,
    pub envelope: Box3,
    pub mass_kg: f64,
    pub fill_fraction: Option<f64>,
    pub net_load_kg: Option<f64>,
    pub uld: Option<UldDefinition>,
    pub orientation: OrientationStatus,
    pub fidelity: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct UldDefinition {
    pub key: String,
    pub code: String,
    pub name: String,
    pub dimensions_m: [f64; 3],
    pub normalized_contour_yz: Vec<Point2>,
    pub contour_source: String,
    pub contour_fidelity: String,
    pub mirrorable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct OrientationStatus {
    pub value: Option<String>,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CargoInventoryStatus {
    pub available: bool,
    pub reason: String,
    pub required_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct MissingInput {
    pub field: String,
    pub reason: String,
    pub required_source: String,
}

/// The completed-analysis fields a cabin scene actually reads.
///
/// [`AnalysisReport`] carries a polar sweep, a trim solution and a mass
/// breakdown; the scene reads three of its fields. Naming that subset lets a
/// caller that already holds resolved geometry and a payload layout -- a
/// figure gallery, a renderer regression test -- build the same scene without
/// first running an analysis it does not use, and keeps the two paths building
/// it through one function rather than two that can drift.
#[derive(Debug, Clone, Copy)]
pub struct CabinSceneInputs<'a> {
    /// Design vector the geometry and layout were resolved at.
    pub design: alas_config::DesignVector,
    /// Built aircraft geometry the section contours come from.
    pub airplane: &'a alas_geom::aircraft::airplane::Airplane,
    /// Resolved cabin and cargo layout.
    pub layout: &'a alas_payload::layout::PayloadLayout,
    /// What produced these inputs, recorded verbatim in the scene provenance.
    ///
    /// A scene resolved from a preset design vector and one resolved from an
    /// optimized run are not the same evidence, and the difference has to
    /// survive into the exported file.
    pub source: &'static str,
}

impl CabinScene {
    /// Resolve a scene exclusively from live configuration and analysis data.
    pub fn from_run(config: &AlasConfig, report: &AnalysisReport) -> Result<Self, String> {
        let layout = report
            .payload_layout
            .as_ref()
            .ok_or("analysis has no resolved payload layout")?;
        Self::from_parts(
            config,
            CabinSceneInputs {
                design: report.design,
                airplane: &report.airplane,
                layout,
                source: "live AnalysisReport + effective AlasConfig",
            },
        )
    }

    /// Resolve a scene from geometry and a payload layout that already exist.
    pub fn from_parts(config: &AlasConfig, inputs: CabinSceneInputs<'_>) -> Result<Self, String> {
        let cabin = CabinGeometry::new(
            inputs.airplane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .map_err(|error| format!("cabin geometry: {error}"))?;
        build::build_scene(config, inputs, &cabin)
    }
}

/// Write a stable renderer input beside the run's other artifacts.
pub fn export_cabin_scene(
    config: &AlasConfig,
    report: &AnalysisReport,
    path: &Path,
) -> io::Result<CabinScene> {
    let scene = CabinScene::from_run(config, report)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_writer_pretty(File::create(path)?, &scene)?;
    Ok(scene)
}
