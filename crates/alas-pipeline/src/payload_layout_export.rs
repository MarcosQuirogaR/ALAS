// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stable, renderer-facing snapshot of the payload layout selected by a run.
//!
//! This is deliberately separate from `design_database.json`: renderers need
//! every placed item, including zero-mass monuments and overhead bins, while
//! the design database is an aircraft-performance compatibility artifact.

use std::fs::File;
use std::io;
use std::path::Path;

use alas_config::{AlasConfig, DesignVector};
use alas_payload::cargo::{uld, uld_by_code, ContourFidelity, UldType};
use alas_payload::layout::{DeckItem, ItemMeta, LayoutSummary, PassengerSummary, PayloadLayout};
use serde::{Deserialize, Serialize};

/// Current wire-format revision. Consumers must reject unknown major versions.
pub const PAYLOAD_LAYOUT_SCHEMA_VERSION: &str = "alas.payload-layout-render/v1";

/// Complete input needed to reproduce payload-layout figures from a real run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct PayloadLayoutArtifact {
    pub schema_version: String,
    pub provenance: PayloadLayoutProvenance,
    pub optimized_design: DesignVector,
    pub effective_config: AlasConfig,
    pub layout: RenderLayout,
    pub fidelity: RenderFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct PayloadLayoutProvenance {
    pub producer: String,
    pub source: String,
    pub aircraft_preset: Option<String>,
    pub cabin_preset: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct RenderLayout {
    pub mode: String,
    pub items: Vec<RenderItem>,
    pub total_mass_kg: f64,
    pub cg_x_m: f64,
    pub cg_y_m: f64,
    pub summary: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct RenderItem {
    pub kind: String,
    pub deck: String,
    pub geometry_m: RenderItemGeometry,
    pub mass_kg: f64,
    pub label: String,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct RenderItemGeometry {
    /// Center coordinate [m], positive aft from the aircraft origin.
    pub center_x: f64,
    /// Center coordinate [m], positive starboard from the aircraft origin.
    pub center_y: f64,
    /// Center coordinate [m], positive up from the aircraft origin.
    pub center_z: f64,
    /// Longitudinal extent [m].
    pub length: f64,
    /// Lateral extent [m].
    pub width: f64,
    /// Vertical extent [m].
    pub height: f64,
}

/// What is authoritative in the snapshot and what still needs richer inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Serialized fields form the documented interchange schema.
#[allow(missing_docs)]
pub struct RenderFidelity {
    pub authoritative: Vec<String>,
    pub derived_or_nominal: Vec<String>,
    pub missing_for_manufacturer_fidelity: Vec<String>,
}

impl PayloadLayoutArtifact {
    /// Capture an optimized design and its exact placed payload layout.
    pub fn from_run(config: &AlasConfig, design: DesignVector, layout: &PayloadLayout) -> Self {
        Self {
            schema_version: PAYLOAD_LAYOUT_SCHEMA_VERSION.to_owned(),
            provenance: PayloadLayoutProvenance {
                producer: "ALAS optimized pipeline".to_owned(),
                source: "optimized_report.payload_layout".to_owned(),
                aircraft_preset: (!config.preset.is_empty()).then(|| config.preset.clone()),
                cabin_preset: config.requirements.cabin_preset.clone(),
            },
            optimized_design: design,
            effective_config: config.clone(),
            layout: RenderLayout {
                mode: layout.mode.as_str().to_owned(),
                items: layout.items.iter().map(render_item).collect(),
                total_mass_kg: layout.total_mass,
                cg_x_m: layout.cg_x,
                cg_y_m: layout.cg_y,
                summary: summary_json(&layout.summary),
            },
            fidelity: RenderFidelity {
                authoritative: vec![
                    "effective configuration and optimized design vector".to_owned(),
                    "item centres, envelopes, masses, labels, deck and placement order".to_owned(),
                    "ULD code, fill fraction, net load when modeled, and payload mass/CG"
                        .to_owned(),
                ],
                derived_or_nominal: vec![
                    "fuselage and deck geometry from the configured aircraft model".to_owned(),
                    "seat, monument and overhead-bin envelopes from layout rules".to_owned(),
                    "ULD contours resolved by code from the ALAS IATA-type database".to_owned(),
                ],
                missing_for_manufacturer_fidelity: vec![
                    "station-indexed structural inner mould line and cabin-liner contour"
                        .to_owned(),
                    "window station, reveal depth, glazing curvature and frame profile".to_owned(),
                    "supplier-specific overhead-bin shell, rail and attachment geometry".to_owned(),
                    "ULD serial/orientation/restraint details and measured load bulge".to_owned(),
                ],
            },
        }
    }
}

/// Persist the live optimized layout beside the ordinary design database.
pub fn export_payload_layout_artifact(
    config: &AlasConfig,
    design: DesignVector,
    layout: &PayloadLayout,
    path: &Path,
) -> io::Result<PayloadLayoutArtifact> {
    let artifact = PayloadLayoutArtifact::from_run(config, design, layout);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_writer_pretty(File::create(path)?, &artifact)?;
    Ok(artifact)
}

fn render_item(item: &DeckItem) -> RenderItem {
    RenderItem {
        kind: item.kind.as_str().to_owned(),
        deck: item.deck.to_owned(),
        geometry_m: RenderItemGeometry {
            center_x: item.x,
            center_y: item.y,
            center_z: item.z,
            length: item.length,
            width: item.width,
            height: item.height,
        },
        mass_kg: item.mass,
        label: item.label.clone(),
        meta: meta_json(&item.meta),
    }
}

fn meta_json(meta: &ItemMeta) -> serde_json::Value {
    match meta {
        ItemMeta::None => serde_json::json!({}),
        ItemMeta::Seat(seat) => serde_json::json!({
            "class": seat.cls, "abreast": seat.abreast, "filled": seat.filled,
            "deck": seat.deck, "aisles": seat.aisles, "blocks": seat.blocks,
            "seat_width_m": seat.seat_w, "aisle_width_m": seat.aisle_w,
        }),
        ItemMeta::Exit(exit) => serde_json::json!({
            "exit_type": exit.exit_type, "door_width_m": exit.door_w,
            "door_height_m": exit.door_h,
        }),
        ItemMeta::Container(container) => {
            let mut value = serde_json::json!({
                "uld_code": container.uld, "fill_fraction": container.fill,
                "color": container.color, "net_load_kg": container.net,
            });
            if let Some(uld) = uld_by_code(container.uld) {
                merge_contour_metadata(&mut value, uld);
            }
            value
        }
        ItemMeta::OverheadBin(bin) => {
            serde_json::json!({ "bin_type": bin.bin_type.as_str() })
        }
        ItemMeta::BulkBag => {
            let mut value = serde_json::json!({ "loading": "loose_bulk" });
            if let Some(uld) = uld("BLK") {
                merge_contour_metadata(&mut value, uld);
            }
            value
        }
    }
}

fn merge_contour_metadata(value: &mut serde_json::Value, uld: &UldType) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.insert(
        "normalized_contour_yz".to_owned(),
        serde_json::json!(uld.contour.vertices),
    );
    object.insert(
        "contour_source".to_owned(),
        serde_json::Value::String(uld.contour.source.to_owned()),
    );
    object.insert(
        "contour_fidelity".to_owned(),
        serde_json::Value::String(
            match uld.contour.fidelity {
                ContourFidelity::Authoritative => "authoritative",
                ContourFidelity::ConservativeEnvelope => "conservative_envelope",
                ContourFidelity::VisualizationOnly => "visualization_only",
            }
            .to_owned(),
        ),
    );
    object.insert(
        "contour_mirrorable".to_owned(),
        serde_json::Value::Bool(uld.contour.mirrorable),
    );
}

fn summary_json(summary: &LayoutSummary) -> serde_json::Value {
    match summary {
        LayoutSummary::Passenger(summary) => passenger_summary_json(summary),
        LayoutSummary::Cargo(summary) => serde_json::json!({
            "type": "cargo", "payload_t": summary.payload_t,
            "requested_net_payload_t": summary.requested_net_payload_t,
            "loaded_net_payload_t": summary.loaded_net_payload_t,
            "tare_mass_t": summary.tare_mass_t, "n_ulds": summary.n_ulds,
            "n_main_deck": summary.n_main_deck, "n_lower_deck": summary.n_lower_deck,
            "n_slots": summary.n_slots, "capacity_t": summary.capacity_t,
            "fill_pct": summary.fill_pct, "volume_m3": summary.volume_m3,
            "lower_uld": summary.lower_uld,
            "target_cg_pct_mac": summary.target_cg_pct_mac,
            "achieved_cg_pct_mac": summary.achieved_cg_pct_mac,
            "strategy": summary.strategy,
        }),
    }
}

fn passenger_summary_json(summary: &PassengerSummary) -> serde_json::Value {
    serde_json::json!({
        "type": "passenger", "total_pax": summary.total_pax,
        "seated_pax": summary.seated_pax, "unseated_pax": summary.unseated_pax,
        "classes": summary.classes, "lavatories": summary.lavatories,
        "galleys": summary.galleys, "accessible_lavatories": summary.accessible_lavatories,
        "wheelchair_stowages": summary.wheelchair_stowages,
        "exit_type": summary.exit_type, "exit_pairs": summary.exit_pairs,
        "exit_capacity": summary.exit_capacity,
        "max_certifiable_capacity": summary.max_certifiable_capacity,
        "payload_t": summary.payload_t, "seat_mass_t": summary.seat_mass_t,
        "bag_mass_t": summary.bag_mass_t, "belly_cargo_t": summary.belly_cargo_t,
        "hold_capacity_t": summary.hold_capacity_t, "hold_used_t": summary.hold_used_t,
        "hold_ulds": summary.hold_ulds, "aisle_width_m": summary.aisle_width_m,
        "max_abreast": summary.max_abreast, "n_aisles": summary.n_aisles,
        "deck_utilization": summary.deck_utilization, "cg_pct_mac": summary.cg_pct_mac,
        "double_deck": summary.double_deck,
    })
}

#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alas_payload::layout::{CargoSummary, ContainerMeta, DeckItem, ItemKind, Mode, LOWER};

    #[test]
    fn artifact_keeps_individual_uld_identity_geometry_and_load() {
        let layout = PayloadLayout {
            mode: Mode::Cargo,
            items: vec![DeckItem {
                kind: ItemKind::Uld,
                deck: LOWER,
                x: 14.0,
                y: -0.8,
                z: -1.1,
                length: 1.534,
                width: 1.562,
                height: 1.626,
                mass: 1_200.0,
                label: "AKE 1200kg".to_owned(),
                meta: ItemMeta::Container(ContainerMeta {
                    uld: "AKE",
                    fill: 0.75,
                    color: "#4477aa",
                    net: Some(1_120.0),
                }),
            }],
            total_mass: 1_200.0,
            cg_x: 14.0,
            cg_y: -0.8,
            summary: LayoutSummary::Cargo(Box::new(CargoSummary {
                payload_t: 1.2,
                requested_net_payload_t: 1.12,
                loaded_net_payload_t: 1.12,
                tare_mass_t: 0.08,
                n_ulds: 1,
                n_main_deck: 0,
                n_lower_deck: 1,
                n_slots: 1,
                capacity_t: 1.5,
                fill_pct: 75.0,
                volume_m3: 4.3,
                lower_uld: "AKE",
                target_cg_pct_mac: 25.0,
                achieved_cg_pct_mac: 25.1,
                strategy: "balanced".to_owned(),
            })),
        };
        let artifact = PayloadLayoutArtifact::from_run(
            &AlasConfig::default(),
            DesignVector::default(),
            &layout,
        );
        let encoded = serde_json::to_string(&artifact).expect("serialize artifact");
        let decoded: PayloadLayoutArtifact =
            serde_json::from_str(&encoded).expect("deserialize artifact");
        assert_eq!(decoded.schema_version, PAYLOAD_LAYOUT_SCHEMA_VERSION);
        assert_eq!(decoded.layout.items[0].meta["uld_code"], "AKE");
        assert_eq!(decoded.layout.items[0].meta["net_load_kg"], 1_120.0);
        assert_eq!(
            decoded.layout.items[0].meta["contour_fidelity"],
            "visualization_only"
        );
        assert_eq!(
            decoded.layout.items[0].meta["normalized_contour_yz"]
                .as_array()
                .expect("ULD contour is serialized")
                .len(),
            8
        );
        assert_eq!(decoded.layout.items[0].geometry_m.center_y, -0.8);
        assert!(decoded
            .fidelity
            .missing_for_manufacturer_fidelity
            .iter()
            .any(|entry| entry.contains("window station")));
    }
}
