// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed data retained by the CPACS 3.5 import boundary.
//!
//! The importer keeps CPACS identifiers, transformations, reference values and
//! point lists in their document order. It does not turn CPACS geometry into an
//! internal aircraft approximation: doing so here would discard information
//! such as symmetry declarations and UID relationships before a downstream
//! consumer had a chance to inspect it.

use alas_geom::aircraft::wing::{PlanformDatum, WingReference};

/// The only CPACS schema version accepted by the current reader.
pub const CPACS_35_VERSION: &str = "3.5";

/// A document-level CPACS 3.5 aircraft dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsDocument {
    /// CPACS schema version declared by the document.
    pub cpacs_version: String,
    /// Header metadata, including the version history used to select the
    /// supported schema version.
    pub header: CpacsHeader,
    /// The aircraft model contained in the document.
    pub aircraft: CpacsAircraft,
    /// Engine definitions available to engine installations.
    pub engines: Vec<CpacsEngine>,
    /// Fuselage profiles available to fuselage elements.
    pub fuselage_profiles: Vec<CpacsFuselageProfile>,
    /// Wing airfoil definitions available to wing elements.
    pub wing_airfoils: Vec<CpacsWingAirfoil>,
}

/// CPACS header metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpacsHeader {
    /// Human-readable document name, when present.
    pub name: Option<String>,
    /// Human-readable document description, when present.
    pub description: Option<String>,
    /// Producer or document version, when present.
    pub version: Option<String>,
    /// Version history entries in document order.
    pub version_infos: Vec<CpacsVersionInfo>,
}

/// One CPACS version-history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpacsVersionInfo {
    /// Producer version key, when the `version` attribute is present.
    pub version: Option<String>,
    /// CPACS schema version named by this entry, when present.
    pub cpacs_version: Option<String>,
    /// Entry description, when present.
    pub description: Option<String>,
    /// Entry timestamp, when present.
    pub timestamp: Option<String>,
    /// Entry creator, when present.
    pub creator: Option<String>,
}

/// Aircraft model geometry and its local reference data.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsAircraft {
    /// Globally unique model identifier.
    pub uid: String,
    /// Aircraft model name.
    pub name: String,
    /// Aircraft model description, when present.
    pub description: Option<String>,
    /// Reference quantities, when the model contains a `reference` element.
    pub reference: Option<CpacsReference>,
    /// Fuselages and other fuselage-shaped bodies in document order.
    pub fuselages: Vec<CpacsFuselage>,
    /// Wings in document order.
    pub wings: Vec<CpacsWing>,
    /// Engine installations in document order.
    pub engine_positions: Vec<CpacsEnginePosition>,
    /// ALAS toolspecific records of the tail reference planforms. A document not
    /// exported by ALAS has none.
    pub tail_planform_records: Vec<CpacsTailPlanformRecord>,
    /// ALAS toolspecific records of the bodies as the analyses read them. A document
    /// not exported by ALAS has none.
    pub body_records: Vec<CpacsBodyRecord>,
    /// ALAS toolspecific record of the main wing, when present. A document not
    /// exported by ALAS has none.
    pub main_wing_record: Option<CpacsMainWingRecord>,
    /// ALAS toolspecific record of the design vector the exported aircraft
    /// was built from, when present. A document not exported by ALAS has none.
    pub design_vector: Option<alas_config::DesignVector>,
}

/// The straight reference planform of a tail surface whose built edges leave it,
/// which CPACS 3.5 cannot encode, and the sections it was recorded against. See the
/// exporter's `tail_planform` module.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsTailPlanformRecord {
    /// The surface's name in the document.
    pub name: String,
    /// The reference planform's sections, root to tip.
    pub reference: Vec<CpacsRecordedSection>,
    /// The sections of the exported surface, root to tip.
    pub sections: Vec<CpacsRecordedSection>,
}

/// A body as the analyses read it, which the document's closed sections are not. See
/// the exporter's `body_record` module.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsBodyRecord {
    /// The body's name in the document.
    pub name: String,
    /// The stations, nose to tail, open ends open and with their shape exponents.
    pub stations: Vec<alas_geom::aircraft::fuselage::FuselageXSec>,
    /// The scarfed inlet, if any.
    pub lip_scarf: Option<alas_geom::aircraft::fuselage::LipScarf>,
    /// What the lofted surface is built from, when the body has one.
    pub loft: Option<CpacsBodyLoft>,
    /// The stations of the separate APU cone, when the body has one.
    pub apu_cone: Option<Vec<alas_geom::aircraft::fuselage::FuselageXSec>>,
}

/// The inputs of a body's lofted surface, in the frame of the record's stations.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsBodyLoft {
    /// The loft data of the body, geometry only: no fit statistics or provenance.
    pub spec: alas_geom::loft::LoftSpec,
    /// How that data was instantiated.
    pub params: alas_geom::loft::LoftParams,
}

/// The main wing's reference wing and design planform, which CPACS 3.5 cannot
/// encode, and what they were recorded against. See the exporter's `main_wing` module.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsMainWingRecord {
    /// The reference wing, when the exported wing had one.
    pub reference: Option<WingReference>,
    /// The design planform, when the exported wing had one.
    pub datum: Option<PlanformDatum>,
    /// The wingtip device's flare in the last section's chord of a wing with no datum,
    /// m; 0 otherwise.
    pub tip_te_flare_m: f64,
    /// The sections of the exported wing, root to tip. Empty in a record from an
    /// export older than this list, which cannot be checked against its document.
    pub sections: Vec<CpacsRecordedSection>,
    /// The document's own `<reference>` area and length at the export, m^2 and m.
    pub document_reference: Option<(f64, f64)>,
}

/// One recorded section. Metres and degrees, aircraft axes, x aft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpacsRecordedSection {
    /// Leading edge.
    pub le_m: [f64; 3],
    /// Chord.
    pub chord_m: f64,
    /// Twist.
    pub twist_deg: f64,
}

/// Aircraft reference quantities expressed in CPACS SI geometry coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsReference {
    /// Reference area in square metres, when present.
    pub area: Option<f64>,
    /// Reference length in metres, when present.
    pub length: Option<f64>,
    /// Reference point in metres in CPACS coordinates, when present.
    pub point: Option<[f64; 3]>,
}

/// A CPACS fuselage or other fuselage-shaped body.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsFuselage {
    /// Globally unique fuselage identifier.
    pub uid: String,
    /// Fuselage name.
    pub name: String,
    /// Fuselage description, when present.
    pub description: Option<String>,
    /// Parent geometry identifier, when present.
    pub parent_uid: Option<String>,
    /// Body transformation, when present.
    pub transformation: Option<CpacsTransformation>,
    /// Fuselage sections in document order.
    pub sections: Vec<CpacsFuselageSection>,
    /// Fuselage segments in document order.
    pub segments: Vec<CpacsSegment>,
}

/// A CPACS fuselage section.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsFuselageSection {
    /// Globally unique section identifier.
    pub uid: String,
    /// Section name.
    pub name: String,
    /// Section transformation, when present.
    pub transformation: Option<CpacsTransformation>,
    /// Section elements in document order.
    pub elements: Vec<CpacsFuselageElement>,
}

/// A CPACS fuselage section element.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsFuselageElement {
    /// Globally unique element identifier.
    pub uid: String,
    /// Element name.
    pub name: String,
    /// Referenced fuselage profile identifier.
    pub profile_uid: String,
    /// Element transformation, when present.
    pub transformation: Option<CpacsTransformation>,
}

/// A CPACS wing.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsWing {
    /// Globally unique wing identifier.
    pub uid: String,
    /// Wing name.
    pub name: String,
    /// Wing description, when present.
    pub description: Option<String>,
    /// Parent geometry identifier, when present.
    pub parent_uid: Option<String>,
    /// Symmetry declaration, preserved exactly when present.
    pub symmetry: Option<String>,
    /// Wing transformation, when present.
    pub transformation: Option<CpacsTransformation>,
    /// Wing sections in document order.
    pub sections: Vec<CpacsWingSection>,
    /// Wing segments in document order.
    pub segments: Vec<CpacsSegment>,
}

/// A CPACS wing section.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsWingSection {
    /// Globally unique section identifier.
    pub uid: String,
    /// Section name.
    pub name: String,
    /// Section transformation, when present.
    pub transformation: Option<CpacsTransformation>,
    /// Section elements in document order.
    pub elements: Vec<CpacsWingElement>,
}

/// A CPACS wing section element.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsWingElement {
    /// Globally unique element identifier.
    pub uid: String,
    /// Element name.
    pub name: String,
    /// Referenced wing airfoil identifier.
    pub airfoil_uid: String,
    /// Element transformation, when present.
    pub transformation: Option<CpacsTransformation>,
}

/// A CPACS segment connecting two elements in one lifting surface or body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpacsSegment {
    /// Globally unique segment identifier.
    pub uid: String,
    /// Segment name.
    pub name: String,
    /// Source element identifier.
    pub from_element_uid: String,
    /// Destination element identifier.
    pub to_element_uid: String,
}

/// A CPACS engine installation on an aircraft model.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsEnginePosition {
    /// Globally unique installation identifier.
    pub uid: String,
    /// Installation name.
    pub name: String,
    /// Referenced engine definition identifier.
    pub engine_uid: String,
    /// Parent wing or fuselage identifier.
    pub parent_uid: String,
    /// Installation transformation, when present.
    pub transformation: Option<CpacsTransformation>,
}

/// A reusable CPACS engine definition.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsEngine {
    /// Globally unique engine identifier.
    pub uid: String,
    /// Engine name.
    pub name: String,
    /// Engine description, when present.
    pub description: Option<String>,
    /// Engine geometry length in metres, when CPACS provides it.
    pub geometry_length_m: Option<f64>,
    /// Engine geometry diameter in metres, when CPACS provides it.
    pub geometry_diameter_m: Option<f64>,
    /// Take-off thrust in newtons, when CPACS provides `analysis/thrust00`.
    pub thrust00_n: Option<f64>,
    /// Take-off fan pressure ratio, when CPACS provides `analysis/fpr00`.
    pub fpr00: Option<f64>,
    /// Take-off bypass ratio, when CPACS provides `analysis/bpr00`.
    pub bpr00: Option<f64>,
    /// Take-off overall pressure ratio, when CPACS provides `analysis/opr00`.
    pub opr00: Option<f64>,
    /// ALAS toolspecific turboprop data, when present.
    pub turboprop: Option<CpacsTurboprop>,
}

/// Shaft-power propulsion data not representable by CPACS 3.5 jet fields.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsTurboprop {
    pub propeller_model: String,
    pub takeoff_shaft_power_kw: f64,
    pub maximum_reserve_shaft_power_kw: f64,
    pub maximum_continuous_shaft_power_kw: f64,
    pub maximum_climb_shaft_power_kw: f64,
    pub maximum_cruise_shaft_power_kw: f64,
    pub maximum_cruise_fuel_flow_kg_h: f64,
    pub propeller_diameter_m: f64,
    pub governed_propeller_speed_rpm: f64,
    pub reduction_ratio: f64,
}

/// A reusable CPACS fuselage profile point list.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsFuselageProfile {
    /// Globally unique profile identifier.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile description, when present.
    pub description: Option<String>,
    /// Point-list encoding, normally `vector` for the current exporter.
    pub map_type: Option<String>,
    /// Profile points in the order written by CPACS.
    pub points: Vec<[f64; 3]>,
}

/// A reusable CPACS wing airfoil point list.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsWingAirfoil {
    /// Globally unique airfoil identifier.
    pub uid: String,
    /// Airfoil name.
    pub name: String,
    /// Airfoil description, when present.
    pub description: Option<String>,
    /// Point-list encoding, normally `vector` for the current exporter.
    pub map_type: Option<String>,
    /// Airfoil points in the order written by CPACS.
    pub points: Vec<[f64; 3]>,
}

/// A CPACS transformation in document order-independent typed form.
#[derive(Debug, Clone, PartialEq)]
pub struct CpacsTransformation {
    /// Dimensionless scale factors along the CPACS axes, when present.
    pub scaling: Option<[f64; 3]>,
    /// Rotation values as authored by CPACS, when present.
    pub rotation: Option<[f64; 3]>,
    /// Translation in metres in CPACS coordinates, when present.
    pub translation: Option<[f64; 3]>,
    /// Translation reference frame attribute, when present.
    pub translation_reference: Option<String>,
}
