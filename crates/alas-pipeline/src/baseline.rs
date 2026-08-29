// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/pipeline.py (`BaselineReport`, `_baseline_analysis`).
// Reference: alas @ rust-port-baseline.

//! Fast weight & balance and longitudinal stability check for the initial design.
//!
//! [`analyze_baseline`] builds the nominal design, sizes the lumped/detailed
//! interior layout, anchors the moment reference to the physical CG, and
//! computes the neutral point and static margin without performing an expensive
//! polar sweep or optimizer run.

use std::collections::HashMap;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{MassBreakdown, MassCoordinateModel, MassCoordinates};
use alas_payload::build::build_payload_layout;
use alas_payload::layout::PayloadLayout;
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::neutral_point;

/// Summary report of the initial baseline aircraft before optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineReport {
    /// The input design vector evaluated.
    pub design: DesignVector,
    /// The built aircraft geometry, if construction succeeded.
    pub airplane: Option<Airplane>,
    /// Component masses by name, in kg.
    pub component_masses: HashMap<String, f64>,
    /// Component centroids by name, in meters `[x, y, z]`.
    pub mass_coordinates: HashMap<String, [f64; 3]>,
    /// Global physical CG coordinates `[x, y, z]`, in meters.
    pub physical_cg: [f64; 3],
    /// Aerodynamic static margin `(x_np - x_cg) / c_ref`.
    pub static_margin: f64,
    /// Mean aerodynamic chord length, in meters.
    pub mac: f64,
    /// Longitudinal neutral point location, in meters.
    pub x_neutral_point: f64,
    /// CG location as a percentage of MAC.
    pub cg_pct_mac: f64,
    /// Neutral point location as a percentage of MAC.
    pub np_pct_mac: f64,
    /// Detailed cabin and cargo layout, if successfully constructed.
    pub payload_layout: Option<PayloadLayout>,
    /// Execution status (`"ok"` or `"error"`).
    pub status: String,
    /// Failure message when status is `"error"`.
    pub error: Option<String>,
}

/// Run weight & balance and stability estimation on `design`.
pub fn analyze_baseline(config: &AlasConfig, design: &DesignVector) -> BaselineReport {
    let mut effective_config = config.clone();
    effective_config
        .geometry
        .engine
        .apply_engine_spec_if_uninitialized();
    let req = &effective_config.requirements;
    let builder = AircraftBuilder::new(Some(effective_config.geometry.clone()));
    let mut plane = match builder.build(Some(design), true) {
        Ok(p) => p,
        Err(e) => {
            return BaselineReport {
                design: *design,
                airplane: None,
                component_masses: HashMap::new(),
                mass_coordinates: HashMap::new(),
                physical_cg: [0.0, 0.0, 0.0],
                static_margin: f64::NAN,
                mac: 0.0,
                x_neutral_point: 0.0,
                cg_pct_mac: 0.0,
                np_pct_mac: 0.0,
                payload_layout: None,
                status: "error".to_owned(),
                error: Some(format!("failed to build aircraft geometry: {e:?}")),
            };
        }
    };

    let coordinate_model = MassCoordinateModel::StructuralWingbox(&effective_config.structures);
    let (masses_init, coords_init, _) =
        match alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
            &plane,
            req,
            &effective_config.geometry,
            &effective_config.cabin,
            &effective_config.control_surfaces,
            Some(&effective_config.mass_model),
            None,
            coordinate_model,
            &effective_config.landing_gear,
        ) {
            Ok(result) => result,
            Err(error) => {
                return BaselineReport {
                    design: *design,
                    airplane: Some(plane),
                    component_masses: HashMap::new(),
                    mass_coordinates: HashMap::new(),
                    physical_cg: [0.0, 0.0, 0.0],
                    static_margin: f64::NAN,
                    mac: 0.0,
                    x_neutral_point: 0.0,
                    cg_pct_mac: 0.0,
                    np_pct_mac: 0.0,
                    payload_layout: None,
                    status: "error".to_owned(),
                    error: Some(format!("failed to compute mass coordinates: {error}")),
                };
            }
        };

    let (oew, x_oew) = oew_and_cg(&masses_init, &coords_init);
    let payload_layout = match build_payload_layout(&plane, &effective_config, oew, x_oew) {
        Ok(layout) => layout,
        Err(error) => {
            return BaselineReport {
                design: *design,
                airplane: Some(plane),
                // The first pass is a lumped sizing estimate.  Do not expose
                // it as if it were a resolved baseline after detailed layout
                // construction failed.
                component_masses: HashMap::new(),
                mass_coordinates: HashMap::new(),
                physical_cg: [f64::NAN; 3],
                static_margin: f64::NAN,
                mac: 0.0,
                x_neutral_point: 0.0,
                cg_pct_mac: 0.0,
                np_pct_mac: 0.0,
                payload_layout: None,
                status: "error".to_owned(),
                error: Some(format!("payload layout error: {error}")),
            };
        }
    };
    let layout_summary = Some(alas_mass::breakdown::PayloadLayoutSummary {
        total_mass: payload_layout.total_mass,
        cg_x: payload_layout.cg_x,
        cg_y: payload_layout.cg_y,
    });

    let (masses, coords, cg) =
        match alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
            &plane,
            req,
            &effective_config.geometry,
            &effective_config.cabin,
            &effective_config.control_surfaces,
            Some(&effective_config.mass_model),
            layout_summary.as_ref(),
            coordinate_model,
            &effective_config.landing_gear,
        ) {
            Ok(result) => result,
            Err(error) => {
                return BaselineReport {
                    design: *design,
                    airplane: Some(plane),
                    component_masses: HashMap::new(),
                    mass_coordinates: HashMap::new(),
                    physical_cg: [0.0, 0.0, 0.0],
                    static_margin: f64::NAN,
                    mac: 0.0,
                    x_neutral_point: 0.0,
                    cg_pct_mac: 0.0,
                    np_pct_mac: 0.0,
                    payload_layout: Some(payload_layout),
                    status: "error".to_owned(),
                    error: Some(format!("failed to compute mass coordinates: {error}")),
                };
            }
        };

    plane.xyz_ref[0] = cg[0];

    let (x_np, sm, _) = match neutral_point(&plane, &effective_config.analysis) {
        Ok(res) => res,
        Err(e) => {
            return BaselineReport {
                design: *design,
                airplane: Some(plane),
                component_masses: breakdown_to_map(&masses),
                mass_coordinates: coordinates_to_map(&coords),
                physical_cg: cg,
                static_margin: f64::NAN,
                mac: 0.0,
                x_neutral_point: 0.0,
                cg_pct_mac: 0.0,
                np_pct_mac: 0.0,
                payload_layout: Some(payload_layout),
                status: "error".to_owned(),
                error: Some(format!("failed to compute neutral point: {e:?}")),
            };
        }
    };

    let mac = plane.c_ref;
    let x_wing_ac = if !plane.wings.is_empty() {
        plane.wings[0].aerodynamic_center(0.25)[0]
    } else {
        0.0
    };
    let x_mac_le = x_wing_ac - 0.25 * mac;

    let to_pct = |x_val: f64| -> f64 {
        if mac > 0.001 {
            ((x_val - x_mac_le) / mac) * 100.0
        } else {
            0.0
        }
    };

    BaselineReport {
        design: *design,
        airplane: Some(plane),
        component_masses: breakdown_to_map(&masses),
        mass_coordinates: coordinates_to_map(&coords),
        physical_cg: cg,
        static_margin: sm,
        mac,
        x_neutral_point: x_np,
        cg_pct_mac: to_pct(cg[0]),
        np_pct_mac: to_pct(x_np),
        payload_layout: Some(payload_layout),
        status: "ok".to_owned(),
        error: None,
    }
}

fn breakdown_to_map(mb: &MassBreakdown) -> HashMap<String, f64> {
    mb.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn coordinates_to_map(mc: &MassCoordinates) -> HashMap<String, [f64; 3]> {
    mc.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
