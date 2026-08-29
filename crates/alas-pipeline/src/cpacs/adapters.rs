// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Non-GUI CPACS geometry boundary for external analysis adapters.
//!
//! The CPACS document is the aircraft-data authority for a product run. The
//! external tools still need their native representations, so this module
//! records the derivation from that aircraft data without moving solver deck
//! writers or numerical models into the CPACS layer. The pipeline binds this
//! contract only after its exported document has been read back and converted
//! to the native geometry type.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use serde::{Deserialize, Serialize};

use crate::cpacs::CpacsExportResult;
use crate::full_analysis::AnalysisReport;

/// Version of the retained non-GUI adapter contract.
pub const CPACS_ADAPTER_MANIFEST_VERSION: &str = "1";

/// Identity of the CPACS aircraft document used as the source of adapter data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpacsSource {
    /// Path to the CPACS aircraft document.
    pub path: PathBuf,
    /// CPACS schema version declared by the document.
    pub cpacs_version: String,
    /// Stable aircraft-model UID in the document.
    pub aircraft_model_uid: String,
    /// Stable engine UID in the document.
    pub engine_uid: String,
}

impl From<&CpacsExportResult> for CpacsSource {
    fn from(export: &CpacsExportResult) -> Self {
        Self {
            path: export.path.clone(),
            cpacs_version: export.cpacs_version.clone(),
            aircraft_model_uid: export.aircraft_model_uid.clone(),
            engine_uid: export.engine_uid.clone(),
        }
    }
}

/// Downstream non-GUI tool represented by the CPACS boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpacsAdapterTool {
    /// OpenVSP geometry materialization.
    #[serde(rename = "openvsp")]
    OpenVsp,
    /// OpenVSP's VSPAERO lifting-surface solver.
    #[serde(rename = "vspaero")]
    Vspaero,
    /// Athena Vortex Lattice.
    #[serde(rename = "avl")]
    Avl,
    /// MSES two-dimensional section solver.
    #[serde(rename = "mses")]
    Mses,
    /// MSC or compatible NASTRAN structural solver.
    #[serde(rename = "nastran")]
    Nastran,
    /// FLOWUnsteady external adapter.
    #[serde(rename = "flowunsteady")]
    FlowUnsteady,
}

impl CpacsAdapterTool {
    /// Every supported downstream boundary in manifest order.
    pub const ALL: [Self; 6] = [
        Self::OpenVsp,
        Self::Vspaero,
        Self::Avl,
        Self::Mses,
        Self::Nastran,
        Self::FlowUnsteady,
    ];

    /// Stable manifest spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenVsp => "openvsp",
            Self::Vspaero => "vspaero",
            Self::Avl => "avl",
            Self::Mses => "mses",
            Self::Nastran => "nastran",
            Self::FlowUnsteady => "flowunsteady",
        }
    }

    /// Native representation and derivation contract for this tool.
    pub const fn contract(self) -> CpacsAdapterContract {
        match self {
            Self::OpenVsp => CpacsAdapterContract {
                tool: Self::OpenVsp,
                geometry_scope: CpacsGeometryScope::WholeAircraft,
                representation: CpacsNativeRepresentation::OpenVspScript,
                derivation: CpacsGeometryDerivation::DirectFromCpacsAircraft,
            },
            Self::Vspaero => CpacsAdapterContract {
                tool: Self::Vspaero,
                geometry_scope: CpacsGeometryScope::LiftingSurfaces,
                representation: CpacsNativeRepresentation::OpenVspLiftingSurfaceMesh,
                derivation: CpacsGeometryDerivation::FromOpenVspGeometry,
            },
            Self::Avl => CpacsAdapterContract {
                tool: Self::Avl,
                geometry_scope: CpacsGeometryScope::LiftingSurfaces,
                representation: CpacsNativeRepresentation::AvlDeck,
                derivation: CpacsGeometryDerivation::FromCpacsLiftingSurfaces,
            },
            Self::Mses => CpacsAdapterContract {
                tool: Self::Mses,
                geometry_scope: CpacsGeometryScope::RootAirfoilSection,
                representation: CpacsNativeRepresentation::MsesAirfoilDat,
                derivation: CpacsGeometryDerivation::FromCpacsRootAirfoil,
            },
            Self::Nastran => CpacsAdapterContract {
                tool: Self::Nastran,
                geometry_scope: CpacsGeometryScope::Wingbox,
                representation: CpacsNativeRepresentation::NastranBdf,
                derivation: CpacsGeometryDerivation::FromCpacsWingboxModel,
            },
            Self::FlowUnsteady => CpacsAdapterContract {
                tool: Self::FlowUnsteady,
                geometry_scope: CpacsGeometryScope::LiftingSurfaces,
                representation: CpacsNativeRepresentation::FlowUnsteadyV2Request,
                derivation: CpacsGeometryDerivation::FromCpacsLiftingSurfaces,
            },
        }
    }
}

/// Portion of the aircraft geometry required by a downstream tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpacsGeometryScope {
    /// Complete exported aircraft, including fuselage-shaped bodies.
    #[serde(rename = "whole_aircraft")]
    WholeAircraft,
    /// Wing sections and their reference quantities.
    #[serde(rename = "lifting_surfaces")]
    LiftingSurfaces,
    /// The first section of the first lifting surface.
    #[serde(rename = "root_airfoil_section")]
    RootAirfoilSection,
    /// Structural wingbox model derived from the aircraft geometry.
    #[serde(rename = "wingbox")]
    Wingbox,
}

/// Native file or request format handed to a downstream tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpacsNativeRepresentation {
    /// OpenVSP AngelScript input.
    #[serde(rename = "openvsp_script")]
    OpenVspScript,
    /// OpenVSP lifting-surface mesh consumed by VSPAERO.
    #[serde(rename = "openvsp_lifting_surface_mesh")]
    OpenVspLiftingSurfaceMesh,
    /// AVL lifting-surface text deck.
    #[serde(rename = "avl_deck")]
    AvlDeck,
    /// MSES Selig-like section input.
    #[serde(rename = "mses_airfoil_dat")]
    MsesAirfoilDat,
    /// NASTRAN bulk-data wingbox mesh.
    #[serde(rename = "nastran_bdf")]
    NastranBdf,
    /// FLOWUnsteady V2 adapter request.
    #[serde(rename = "flowunsteady_v2_request")]
    FlowUnsteadyV2Request,
}

/// Transformation between the CPACS aircraft and a native tool input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpacsGeometryDerivation {
    /// The tool's geometry writer reads the exported aircraft surface model.
    #[serde(rename = "direct_from_cpacs_aircraft")]
    DirectFromCpacsAircraft,
    /// OpenVSP materializes the lifting-surface mesh before VSPAERO runs.
    #[serde(rename = "from_openvsp_geometry")]
    FromOpenVspGeometry,
    /// The tool receives the CPACS root section as a two-dimensional contour.
    #[serde(rename = "from_cpacs_root_airfoil")]
    FromCpacsRootAirfoil,
    /// The structural mesh adds wingbox sizing and finite-element topology.
    #[serde(rename = "from_cpacs_wingbox_model")]
    FromCpacsWingboxModel,
    /// The tool receives a lifting-surface request assembled from CPACS sections.
    #[serde(rename = "from_cpacs_lifting_surfaces")]
    FromCpacsLiftingSurfaces,
}

/// Contract for one native representation of the CPACS aircraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpacsAdapterContract {
    /// Downstream tool owning the representation.
    pub tool: CpacsAdapterTool,
    /// Geometry scope admitted at this boundary.
    pub geometry_scope: CpacsGeometryScope,
    /// Native input representation.
    pub representation: CpacsNativeRepresentation,
    /// How the native input is derived from the CPACS aircraft.
    pub derivation: CpacsGeometryDerivation,
}

/// Compact geometry identity retained with an adapter manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpacsGeometrySummary {
    /// Aircraft name from the computed geometry.
    pub name: String,
    /// Number of lifting surfaces.
    pub wing_count: usize,
    /// Number of fuselage-shaped bodies, including nacelles.
    pub fuselage_count: usize,
    /// Reference area in square meters.
    pub reference_area_m2: f64,
    /// Reference chord in meters.
    pub reference_chord_m: f64,
    /// Reference span in meters.
    pub reference_span_m: f64,
    /// Moment-reference point in meters.
    pub moment_reference_m: [f64; 3],
}

/// Retained declaration of every non-GUI CPACS adapter boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpacsAdapterManifest {
    /// Contract version for consumers of this sidecar.
    pub manifest_version: String,
    /// CPACS aircraft document identity shared by every adapter.
    pub source: CpacsSource,
    /// Geometry identity shared by every adapter.
    pub geometry: CpacsGeometrySummary,
    /// Native representations derived from the source aircraft.
    pub adapters: Vec<CpacsAdapterContract>,
}

impl CpacsAdapterManifest {
    /// Build a manifest from one CPACS-bound aircraft snapshot.
    pub fn from_aircraft(aircraft: &CpacsAircraftData<'_>) -> Self {
        Self {
            manifest_version: CPACS_ADAPTER_MANIFEST_VERSION.to_owned(),
            source: aircraft.source.clone(),
            geometry: CpacsGeometrySummary::from(aircraft.geometry),
            adapters: CpacsAdapterTool::ALL
                .into_iter()
                .map(CpacsAdapterTool::contract)
                .collect(),
        }
    }

    /// Write the manifest as a retained JSON sidecar.
    pub fn write_json(&self, path: &Path) -> io::Result<PathBuf> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut document = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        document.push(b'\n');
        fs::write(path, document)?;
        Ok(path.to_path_buf())
    }
}

/// In-memory CPACS aircraft snapshot shared by non-GUI adapter callers.
#[derive(Debug, Clone)]
pub struct CpacsAircraftData<'a> {
    /// CPACS document identity.
    pub source: CpacsSource,
    /// Computed aircraft geometry represented by the CPACS document.
    pub geometry: &'a Airplane,
}

impl<'a> CpacsAircraftData<'a> {
    /// Bind an analysis report to its exported CPACS identity.
    pub fn from_report(report: &'a AnalysisReport, export: &CpacsExportResult) -> Self {
        Self::from_geometry(&report.airplane, export)
    }

    /// Bind an already selected aircraft geometry to its exported identity.
    pub fn from_geometry(geometry: &'a Airplane, export: &CpacsExportResult) -> Self {
        Self {
            source: CpacsSource::from(export),
            geometry,
        }
    }

    /// Describe how one downstream tool is allowed to consume the snapshot.
    pub fn adapter_request(&self, tool: CpacsAdapterTool) -> CpacsAdapterRequest<'a> {
        CpacsAdapterRequest {
            source: self.source.clone(),
            geometry: self.geometry,
            contract: tool.contract(),
        }
    }

    /// Build the retained declaration for this aircraft snapshot.
    pub fn manifest(&self) -> CpacsAdapterManifest {
        CpacsAdapterManifest::from_aircraft(self)
    }
}

/// Borrowed native-adapter request with an explicit CPACS source.
#[derive(Debug, Clone)]
pub struct CpacsAdapterRequest<'a> {
    /// CPACS source shared by the request.
    pub source: CpacsSource,
    /// Computed geometry represented by the source document.
    pub geometry: &'a Airplane,
    /// Scope and representation contract for the selected tool.
    pub contract: CpacsAdapterContract,
}

impl CpacsAdapterRequest<'_> {
    /// Return the root airfoil when the selected geometry contains one.
    pub fn root_airfoil(&self) -> Option<&Airfoil> {
        self.geometry
            .wings
            .first()
            .and_then(|wing| wing.xsecs.first())
            .map(|section| &section.airfoil)
    }
}

impl From<&Airplane> for CpacsGeometrySummary {
    fn from(airplane: &Airplane) -> Self {
        Self {
            name: airplane.name.clone(),
            wing_count: airplane.wings.len(),
            fuselage_count: airplane.fuselages.len(),
            reference_area_m2: airplane.s_ref,
            reference_chord_m: airplane.c_ref,
            reference_span_m: airplane.b_ref,
            moment_reference_m: airplane.xyz_ref,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn export() -> CpacsExportResult {
        CpacsExportResult {
            path: PathBuf::from("outputs/cpacs/aircraft.xml"),
            aircraft_model_uid: "aircraft".to_owned(),
            engine_uid: "engine".to_owned(),
            wing_count: 1,
            fuselage_count: 0,
            engine_position_count: 0,
            cpacs_version: "3.5".to_owned(),
        }
    }

    fn airplane() -> Airplane {
        Airplane {
            name: "adapter-test".to_owned(),
            xyz_ref: [1.0, 2.0, 3.0],
            wings: vec![Wing::new(
                "Main Wing",
                vec![WingXSec::new(
                    [0.0, 0.0, 0.0],
                    2.0,
                    0.0,
                    Airfoil::from_coordinates("root", vec![(1.0, 0.0), (0.0, 0.1), (1.0, 0.0)]),
                )],
                false,
            )],
            fuselages: Vec::new(),
            s_ref: 10.0,
            c_ref: 2.0,
            b_ref: 5.0,
        }
    }

    #[test]
    fn every_supported_tool_has_one_explicit_native_contract() {
        let contracts: Vec<CpacsAdapterContract> = CpacsAdapterTool::ALL
            .into_iter()
            .map(CpacsAdapterTool::contract)
            .collect();

        assert_eq!(contracts.len(), 6);
        assert_eq!(contracts[0].tool, CpacsAdapterTool::OpenVsp);
        assert_eq!(
            contracts[1].representation,
            CpacsNativeRepresentation::OpenVspLiftingSurfaceMesh
        );
        assert_eq!(
            contracts[2].representation,
            CpacsNativeRepresentation::AvlDeck
        );
        assert_eq!(
            contracts[3].geometry_scope,
            CpacsGeometryScope::RootAirfoilSection
        );
        assert_eq!(
            contracts[4].derivation,
            CpacsGeometryDerivation::FromCpacsWingboxModel
        );
        assert_eq!(contracts[5].tool, CpacsAdapterTool::FlowUnsteady);
    }

    #[test]
    fn a_request_keeps_the_cpacs_identity_and_root_section_geometry_together() {
        let geometry = airplane();
        let data = CpacsAircraftData::from_geometry(&geometry, &export());
        let request = data.adapter_request(CpacsAdapterTool::Mses);

        assert_eq!(request.source.cpacs_version, "3.5");
        assert_eq!(request.source.aircraft_model_uid, "aircraft");
        assert_eq!(request.geometry.s_ref, 10.0);
        assert_eq!(
            request.root_airfoil().map(|airfoil| airfoil.name.as_str()),
            Some("root")
        );
    }

    #[test]
    fn the_manifest_serializes_the_shared_source_and_all_adapter_names() {
        let geometry = airplane();
        let manifest = CpacsAircraftData::from_geometry(&geometry, &export()).manifest();
        let text =
            serde_json::to_string(&manifest).unwrap_or_else(|error| panic!("manifest: {error}"));

        assert!(text.contains("\"manifest_version\":\"1\""));
        assert!(text.contains("\"path\":\"outputs/cpacs/aircraft.xml\""));
        assert!(text.contains("\"tool\":\"openvsp\""));
        assert!(text.contains("\"tool\":\"nastran\""));
        assert!(text.contains("\"tool\":\"flowunsteady\""));
    }
}
