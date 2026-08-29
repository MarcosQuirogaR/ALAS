// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shape of `golden/aero/vorlax.json`, and the geometry the port is
//! handed.
//!
//! The geometry is read *out of* the fixture rather than rebuilt from the
//! vehicle request. Rebuilding it would be checking two vehicle builders
//! against each other -- `vehicle_builder.py` and a Rust reimplementation of
//! it that this row does not own -- and `alas-aero::drag_buildup`'s ledger
//! entry records what that costs: a parity test that re-derives its inputs
//! reports the difference between two models as a disagreement in the one
//! being tested.

use std::collections::HashMap;

use alas_aero::vorlax::{VlmCondition, VlmGeometry, VlmSettings, VlmWing};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub number_spanwise_vortices: usize,
    pub number_chordwise_vortices: usize,
    pub spanwise_cosine_spacing: bool,
    pub model_fuselage: bool,
    pub model_nacelle: bool,
    pub discretize_control_surfaces: bool,
    pub propeller_wake_model: bool,
    #[serde(rename = "use_VORLAX_matrix_calculation")]
    pub use_vorlax_matrix_calculation: bool,
    pub leading_edge_suction_multiplier: f64,
    pub use_surrogate: bool,
    pub floating_point_precision: String,
}

#[derive(Debug, Deserialize)]
pub struct WingFixture {
    pub tag: String,
    pub symmetric: bool,
    pub vertical: bool,
    pub vortex_lift: bool,
    pub span_projected_m: f64,
    pub chord_root_m: f64,
    pub chord_tip_m: f64,
    pub chord_mean_aerodynamic_m: f64,
    pub taper: f64,
    pub aspect_ratio: f64,
    pub sweep_quarter_chord_rad: f64,
    /// `null` is the normal case, and is what makes the leading-edge sweep a
    /// derived quantity rather than a copied one.
    pub sweep_leading_edge_rad: Option<f64>,
    pub twist_root_rad: f64,
    pub twist_tip_rad: f64,
    pub dihedral_rad: f64,
    pub thickness_to_chord: f64,
    pub area_reference_m2: f64,
    pub origin_m: [f64; 3],
    pub n_segments: usize,
    pub n_control_surfaces: usize,
    pub aerodynamic_center_m: [f64; 3],
}

#[derive(Debug, Deserialize)]
pub struct GeometryFixture {
    pub reference_area_m2: f64,
    pub center_of_gravity_m: [f64; 3],
    pub wings: Vec<WingFixture>,
}

#[derive(Debug, Deserialize)]
pub struct PanelsFixture {
    #[serde(rename = "XAH")]
    pub xah: Vec<f64>,
    #[serde(rename = "YAH")]
    pub yah: Vec<f64>,
    #[serde(rename = "ZAH")]
    pub zah: Vec<f64>,
    #[serde(rename = "XBH")]
    pub xbh: Vec<f64>,
    #[serde(rename = "YBH")]
    pub ybh: Vec<f64>,
    #[serde(rename = "ZBH")]
    pub zbh: Vec<f64>,
    #[serde(rename = "XCH")]
    pub xch: Vec<f64>,
    #[serde(rename = "YCH")]
    pub ych: Vec<f64>,
    #[serde(rename = "ZCH")]
    pub zch: Vec<f64>,
    #[serde(rename = "XA1")]
    pub xa1: Vec<f64>,
    #[serde(rename = "YA1")]
    pub ya1: Vec<f64>,
    #[serde(rename = "ZA1")]
    pub za1: Vec<f64>,
    #[serde(rename = "XA2")]
    pub xa2: Vec<f64>,
    #[serde(rename = "YA2")]
    pub ya2: Vec<f64>,
    #[serde(rename = "ZA2")]
    pub za2: Vec<f64>,
    #[serde(rename = "XB1")]
    pub xb1: Vec<f64>,
    #[serde(rename = "YB1")]
    pub yb1: Vec<f64>,
    #[serde(rename = "ZB1")]
    pub zb1: Vec<f64>,
    #[serde(rename = "XB2")]
    pub xb2: Vec<f64>,
    #[serde(rename = "YB2")]
    pub yb2: Vec<f64>,
    #[serde(rename = "ZB2")]
    pub zb2: Vec<f64>,
    #[serde(rename = "XAC")]
    pub xac: Vec<f64>,
    #[serde(rename = "YAC")]
    pub yac: Vec<f64>,
    #[serde(rename = "ZAC")]
    pub zac: Vec<f64>,
    #[serde(rename = "XBC")]
    pub xbc: Vec<f64>,
    #[serde(rename = "YBC")]
    pub ybc: Vec<f64>,
    #[serde(rename = "ZBC")]
    pub zbc: Vec<f64>,
    #[serde(rename = "XC")]
    pub xc: Vec<f64>,
    #[serde(rename = "YC")]
    pub yc: Vec<f64>,
    #[serde(rename = "ZC")]
    pub zc: Vec<f64>,
    #[serde(rename = "XA_TE")]
    pub xa_te: Vec<f64>,
    #[serde(rename = "YA_TE")]
    pub ya_te: Vec<f64>,
    #[serde(rename = "ZA_TE")]
    pub za_te: Vec<f64>,
    #[serde(rename = "XB_TE")]
    pub xb_te: Vec<f64>,
    #[serde(rename = "YB_TE")]
    pub yb_te: Vec<f64>,
    #[serde(rename = "ZB_TE")]
    pub zb_te: Vec<f64>,
}

#[derive(Debug, Deserialize)]
pub struct DistributionFixture {
    pub n_w: usize,
    pub n_cp: usize,
    pub n_sw: Vec<usize>,
    pub n_cw: Vec<usize>,
    pub chordwise_breaks: Vec<usize>,
    pub spanwise_breaks: Vec<usize>,
    pub symmetric_wings: Vec<i64>,
    pub leading_edge_indices: Vec<i64>,
    pub trailing_edge_indices: Vec<i64>,
    pub panels_per_strip: Vec<usize>,
    pub chordwise_panel_number: Vec<usize>,
    pub exposed_leading_edge_flag: Vec<i64>,
    pub vortex_lift: Vec<bool>,
    pub wing_areas_m2: Vec<f64>,
    pub chord_lengths_m: Vec<f64>,
    pub tangent_incidence_angle: Vec<f64>,
    pub panel_areas_m2: Vec<f64>,
    pub normals: Vec<[f64; 3]>,
    #[serde(rename = "SLOPE")]
    pub slope: Vec<f64>,
    #[serde(rename = "SLE")]
    pub sle: Vec<f64>,
    #[serde(rename = "D")]
    pub d: Vec<f64>,
    pub panels: PanelsFixture,
}

#[derive(Debug, Deserialize)]
pub struct CaseResults {
    #[serde(rename = "CL")]
    pub cl: f64,
    #[serde(rename = "CDi")]
    pub cdi: f64,
    #[serde(rename = "CM")]
    pub cm: f64,
    #[serde(rename = "CYTOT")]
    pub cytot: f64,
    #[serde(rename = "CRTOT")]
    pub crtot: f64,
    #[serde(rename = "CRMTOT")]
    pub crmtot: f64,
    #[serde(rename = "CNTOT")]
    pub cntot: f64,
    #[serde(rename = "CYMTOT")]
    pub cymtot: f64,
    #[serde(rename = "CL_wing")]
    pub cl_wing: Vec<f64>,
    #[serde(rename = "CDi_wing")]
    pub cdi_wing: Vec<f64>,
    pub cl_y: Vec<f64>,
    pub cdi_y: Vec<f64>,
    #[serde(rename = "CP")]
    pub cp: Vec<f64>,
    pub gamma: Vec<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub tag: String,
    pub angle_of_attack_deg: f64,
    pub mach: f64,
    pub side_slip_angle_deg: f64,
    pub pitch_rate_rad_s: f64,
    pub roll_rate_rad_s: f64,
    pub yaw_rate_rad_s: f64,
    pub velocity_m_s: f64,
    pub results: CaseResults,
}

impl Case {
    /// The flight condition, in the units the port takes.
    pub fn condition(&self) -> VlmCondition {
        VlmCondition {
            angle_of_attack_rad: self.angle_of_attack_deg.to_radians(),
            mach: self.mach,
            side_slip_angle_rad: self.side_slip_angle_deg.to_radians(),
            pitch_rate_rad_s: self.pitch_rate_rad_s,
            roll_rate_rad_s: self.roll_rate_rad_s,
            yaw_rate_rad_s: self.yaw_rate_rad_s,
            velocity_m_s: self.velocity_m_s,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Training {
    pub angle_of_attack_rad: Vec<f64>,
    pub mach: Vec<f64>,
    pub angle_of_attack_flat_rad: Vec<f64>,
    pub mach_flat: Vec<f64>,
    #[serde(rename = "CL")]
    pub cl: Vec<f64>,
    #[serde(rename = "CDi")]
    pub cdi: Vec<f64>,
    pub wing_tags: Vec<String>,
    #[serde(rename = "wing_CL")]
    pub wing_cl: HashMap<String, Vec<f64>>,
    #[serde(rename = "wing_CDi")]
    pub wing_cdi: HashMap<String, Vec<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub settings: Settings,
    pub geometry: GeometryFixture,
    pub vortex_distribution: DistributionFixture,
    pub cases: Vec<Case>,
    pub training: Training,
}

impl Fixture {
    /// The port's settings, as the analysis held them.
    pub fn vlm_settings(&self) -> VlmSettings {
        VlmSettings {
            number_spanwise_vortices: self.settings.number_spanwise_vortices,
            number_chordwise_vortices: self.settings.number_chordwise_vortices,
            spanwise_cosine_spacing: self.settings.spanwise_cosine_spacing,
            leading_edge_suction_multiplier: self.settings.leading_edge_suction_multiplier,
        }
    }

    /// The vehicle, as the analysis held it.
    ///
    /// The moment reference is VORLAX's own choice: it takes the centre of
    /// gravity when the aircraft has one, and the main wing's aerodynamic
    /// centre -- offset by the wing origin -- when the `x` component is
    /// exactly zero, which is every vehicle the mission runner builds since
    /// nothing sets a centre of gravity on it.
    pub fn vlm_geometry(&self) -> VlmGeometry {
        let main = self
            .geometry
            .wings
            .iter()
            .find(|wing| wing.tag == "main_wing")
            .expect("the fixture's vehicle has a main wing");

        let cg = self.geometry.center_of_gravity_m;
        let moment_reference = if cg[0] == 0.0 {
            [
                main.aerodynamic_center_m[0] + main.origin_m[0],
                main.aerodynamic_center_m[2] + main.origin_m[2],
            ]
        } else {
            [cg[0], cg[2]]
        };

        VlmGeometry {
            reference_area_m2: self.geometry.reference_area_m2,
            center_of_gravity_m: cg,
            mean_aerodynamic_chord_m: main.chord_mean_aerodynamic_m,
            reference_span_m: main.span_projected_m,
            moment_reference_m: moment_reference,
            wings: self.geometry.wings.iter().map(wing).collect(),
        }
    }

    /// Every single-condition case, in fixture order.
    pub fn conditions(&self) -> Vec<VlmCondition> {
        self.cases.iter().map(Case::condition).collect()
    }
}

fn wing(fixture: &WingFixture) -> VlmWing {
    VlmWing {
        tag: fixture.tag.clone(),
        symmetric: fixture.symmetric,
        vertical: fixture.vertical,
        vortex_lift: fixture.vortex_lift,
        span_projected_m: fixture.span_projected_m,
        chord_root_m: fixture.chord_root_m,
        chord_tip_m: fixture.chord_tip_m,
        taper: fixture.taper,
        aspect_ratio: fixture.aspect_ratio,
        sweep_quarter_chord_rad: fixture.sweep_quarter_chord_rad,
        sweep_leading_edge_rad: fixture.sweep_leading_edge_rad,
        twist_root_rad: fixture.twist_root_rad,
        twist_tip_rad: fixture.twist_tip_rad,
        dihedral_rad: fixture.dihedral_rad,
        area_reference_m2: fixture.area_reference_m2,
        origin_m: fixture.origin_m,
    }
}

/// One `f32` array widened for comparison.
pub fn widen(values: &[f32]) -> Vec<f64> {
    values.iter().map(|&v| f64::from(v)).collect()
}

/// One `usize` array as the integers the fixture holds.
pub fn as_i64(values: &[usize]) -> Vec<i64> {
    values.iter().map(|&v| v as i64).collect()
}

/// A boolean mask as the 0/1 integers the fixture holds.
pub fn as_flags(values: &[bool]) -> Vec<i64> {
    values.iter().map(|&v| i64::from(v)).collect()
}
