// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What `parity_mesh.rs` reads: the shape of `golden/struct/mesh.json`, and
//! the construction that has to match the generator's.
//!
//! The deck records are tuple structs because that is the shape the fixture
//! has -- a card is a row of fields, and naming each position in JSON would
//! have doubled a file that is already the largest in `golden/`. Each one's
//! documentation says what the positions are.

#![allow(clippy::unwrap_used, clippy::expect_used)]
// Two test binaries compile this module, and each reads the part of the fixture
// its own row is about; an item the other one uses is not dead.
#![allow(dead_code)]

pub mod deck;

use alas_config::materials::{self, MaterialSpec};
use alas_config::{DesignVector, StructuresConfig, WingConfig};
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_geom::wing_structure::WingStructureGeometry;
use serde::Deserialize;
use serde_json::{Map, Value};

/// `[nid, x, y, z]`.
#[derive(Debug, Deserialize)]
pub struct GridRow(pub i64, pub f64, pub f64, pub f64);

/// `[mid, E, G, nu, rho]`.
#[derive(Debug, Deserialize)]
pub struct Mat1Row(pub i64, pub f64, pub f64, pub f64, pub f64);

/// `[pid, mid1, t, mid2]`.
#[derive(Debug, Deserialize)]
pub struct PshellRow(pub i64, pub i64, pub f64, pub i64);

/// `[pid, mid, section, [dimensions]]`.
#[derive(Debug, Deserialize)]
pub struct PbarlRow(pub i64, pub i64, pub String, pub Vec<f64>);

/// `[eid, pid, ga, gb, [orientation], offt]`.
#[derive(Debug, Deserialize)]
pub struct CbarRow(pub i64, pub i64, pub i64, pub i64, pub [f64; 3], pub String);

/// `[eid, nid, cid, mass, [offset]]`.
#[derive(Debug, Deserialize)]
pub struct Conm2Row(pub i64, pub i64, pub i64, pub f64, pub [f64; 3]);

/// `[eid, refgrid, refc, weight, comp, [independent grids]]`.
#[derive(Debug, Deserialize)]
pub struct Rbe3Row(
    pub i64,
    pub i64,
    pub String,
    pub f64,
    pub String,
    pub Vec<i64>,
);

/// `[sid, components, [grids]]`.
#[derive(Debug, Deserialize)]
pub struct Spc1Row(pub i64, pub String, pub Vec<i64>);

/// Every card in one case's deck, grouped by type and sorted by identifier.
#[derive(Debug, Deserialize)]
pub struct DeckRecord {
    /// `[name, [values]]`, each value a string or a number.
    pub params: Vec<(String, Vec<Value>)>,
    pub grids: Vec<GridRow>,
    pub mat1: Vec<Mat1Row>,
    pub pshell: Vec<PshellRow>,
    pub pbarl: Vec<PbarlRow>,
    /// `[eid, pid, n1, n2, n3, n4]`, flat because every field is an integer.
    pub cquad4: Vec<Vec<i64>>,
    /// `[eid, pid, n1, n2, n3]`.
    pub ctria3: Vec<Vec<i64>>,
    pub cbar: Vec<CbarRow>,
    pub conm2: Vec<Conm2Row>,
    pub rbe3: Vec<Rbe3Row>,
    pub spc1: Vec<Spc1Row>,
}

/// The mesh health report as the reference produced it.
#[derive(Debug, Deserialize)]
pub struct HealthRecord {
    pub n_nodes: usize,
    pub n_elements: usize,
    pub n_perp_warnings: usize,
    pub n_warping_bad: usize,
    pub warping_max: f64,
    pub warping_mean: f64,
    pub n_cquad4: usize,
    pub n_ctria3: usize,
    pub triangle_ratio: f64,
    pub n_spar_straightness_warnings: usize,
    pub spar_straightness_max_dev_m: Vec<(f64, f64)>,
    pub rbe3_count: usize,
    pub warnings: Vec<String>,
    pub ok: bool,
}

/// The load and monitor grid identifiers.
#[derive(Debug, Deserialize)]
pub struct NodeIndexRecord {
    pub root_nid: i64,
    pub tip_nid: i64,
    pub kink_nid: i64,
    pub spar_upper_nids: Vec<Vec<i64>>,
    pub spar_lower_nids: Vec<Vec<i64>>,
    pub engine_nids: Vec<i64>,
}

/// The four wingbox material names the case was built with.
#[derive(Debug, Deserialize)]
pub struct MaterialsRecord {
    pub skin: String,
    pub web: String,
    pub cap: String,
    pub rib: String,
}

/// One recorded case.
#[derive(Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub config: Map<String, Value>,
    pub spar_chord_fractions: Vec<f64>,
    pub spar_full_span: Vec<bool>,
    pub materials: MaterialsRecord,
    pub num_ribs: i64,
    pub health: HealthRecord,
    pub node_index: NodeIndexRecord,
    pub deck: DeckRecord,
}

/// `golden/struct/mesh.json`.
#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub cases: Vec<Case>,
}

/// Rebuild a `StructuresConfig` from a default plus the recorded overrides,
/// the same `setattr`-after-default the generator does.
pub fn structures_config_for(overrides: &Map<String, Value>) -> StructuresConfig {
    let mut cfg = StructuresConfig::default();
    for (key, value) in overrides {
        match key.as_str() {
            "num_ribs_override" => cfg.num_ribs_override = Some(value.as_i64().unwrap()),
            "mesh_chordwise_points" => cfg.mesh_chordwise_points = value.as_i64().unwrap(),
            "center_spar_enabled" => cfg.center_spar_enabled = value.as_bool().unwrap(),
            "te_rib_mode" => cfg.te_rib_mode = value.as_str().unwrap().to_string(),
            "additional_safety_factor" => {
                cfg.additional_safety_factor = value.as_f64().unwrap();
            }
            "n_modes" => cfg.n_modes = value.as_i64().unwrap(),
            "freq_sweep_max_hz" => cfg.freq_sweep_max_hz = value.as_f64().unwrap(),
            "freq_step_hz" => cfg.freq_step_hz = value.as_f64().unwrap(),
            "modal_damping_ratio" => cfg.modal_damping_ratio = value.as_f64().unwrap(),
            "psd_base_g2_per_hz" => cfg.psd_base_g2_per_hz = value.as_f64().unwrap(),
            other => panic!("fixture set an unhandled StructuresConfig field: {other}"),
        }
    }
    cfg
}

/// The wingbox geometry, built exactly as the green `parity_wing_structure.rs`
/// and `parity_sizing.rs` build it: default design and wing configuration, the
/// root section through `build_section`, the tip straight from the library, and
/// the case's own resolved spar list.
pub fn build_geometry(
    spar_chord_fractions: &[f64],
    spar_full_span: &[bool],
) -> WingStructureGeometry {
    let dv = DesignVector::default();
    let wing_cfg = WingConfig::default();
    let root_base =
        AirfoilLibrary::get(&wing_cfg.root_airfoil).expect("the configured root airfoil resolves");
    let root_section =
        build_section(&dv, &root_base.coordinates).expect("the root section repanels cleanly");
    let tip_airfoil =
        AirfoilLibrary::get(&wing_cfg.tip_airfoil).expect("the configured tip airfoil resolves");
    WingStructureGeometry::new(
        &dv,
        &wing_cfg,
        &root_section,
        &tip_airfoil,
        spar_chord_fractions,
        Some(spar_full_span),
    )
    .expect("the fixture's spar list is valid")
}

/// The four materials a case names.
pub fn materials_for(named: &MaterialsRecord) -> [&'static MaterialSpec; 4] {
    [
        materials::get(&named.skin).expect("skin material resolves"),
        materials::get(&named.web).expect("web material resolves"),
        materials::get(&named.cap).expect("cap material resolves"),
        materials::get(&named.rib).expect("rib material resolves"),
    ]
}
