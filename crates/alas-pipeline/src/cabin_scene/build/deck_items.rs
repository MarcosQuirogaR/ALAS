// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Seat, overhead and cargo systems resolved from the laid-out deck items.

use super::*;

pub(super) fn resolve_seats(items: &[DeckItem]) -> (Vec<SeatRow>, Vec<Seat>) {
    let mut rows = Vec::new();
    let mut seats = Vec::new();
    for (index, item) in items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .enumerate()
    {
        let ItemMeta::Seat(meta) = &item.meta else {
            continue;
        };
        let row_id = format!("seat-row-{index}");
        rows.push(SeatRow {
            id: row_id.clone(),
            deck_id: item.deck.into(),
            envelope: box3(item),
            class: meta.cls.into(),
            abreast: meta.abreast,
            filled: meta.filled,
            blocks: meta.blocks.clone(),
            aisle_width_m: meta.aisle_w,
            fidelity: "solver_resolved_row".into(),
            source: "PayloadLayout DeckItem and SeatMeta".into(),
        });
        let total_width = meta.abreast as f64 * meta.seat_w + meta.aisles as f64 * meta.aisle_w;
        let mut y = -total_width * 0.5 + meta.seat_w * 0.5;
        let mut ordinal = 0_i64;
        for (block_index, count) in meta.blocks.iter().enumerate() {
            for _ in 0..*count {
                seats.push(Seat {
                    id: format!("{row_id}-seat-{ordinal}"),
                    row_id: row_id.clone(),
                    deck_id: item.deck.into(),
                    center_x_m: item.x,
                    center_y_m: y,
                    center_z_m: item.z,
                    width_m: meta.seat_w,
                    occupied: if meta.filled == meta.abreast {
                        Some(true)
                    } else if meta.filled == 0 {
                        Some(false)
                    } else {
                        None
                    },
                    fidelity: "derived_from_solver_row".into(),
                    source: "SeatMeta blocks, seat width and aisle width".into(),
                });
                ordinal += 1;
                y += meta.seat_w;
            }
            if block_index + 1 < meta.blocks.len() {
                y += meta.aisle_w;
            }
        }
    }
    (rows, seats)
}

pub(super) fn resolve_overhead(items: &[DeckItem]) -> OverheadSystem {
    let mut runs = Vec::new();
    let mut topology = Vec::new();
    for (index, item) in items
        .iter()
        .filter(|item| item.kind == ItemKind::OverheadBin)
        .enumerate()
    {
        let kind = match item.meta {
            ItemMeta::OverheadBin(meta) => meta.bin_type.as_str(),
            _ => "unknown",
        };
        let id = format!("overhead-run-{index}");
        let y0 = item.y - item.width * 0.5;
        let y1 = item.y + item.width * 0.5;
        let z0 = item.z - item.height * 0.5;
        let z1 = item.z + item.height * 0.5;
        runs.push(OverheadRun {
            id: id.clone(),
            deck_id: item.deck.into(),
            kind: kind.into(),
            envelope: box3(item),
            profile_yz_m: vec![
                Point2 { y: y0, z: z0 },
                Point2 { y: y1, z: z0 },
                Point2 { y: y1, z: z1 },
                Point2 { y: y0, z: z1 },
            ],
            fidelity: "solver_envelope".into(),
            source: "PayloadLayout overhead-bin DeckItem".into(),
        });
        topology.push(OverheadTopology {
            run_id: id,
            rail_ids: vec![],
            valance_ids: vec![],
            psu_ids: vec![],
            attachment_ids: vec![],
            status: "missing_supplier_topology".into(),
        });
    }
    OverheadSystem {
        runs,
        topology,
        missing: vec![
            "rail/strongback identifiers and cross-sections".into(),
            "continuous valance and ceiling transition geometry".into(),
            "PSU runs, bin doors, hinges, opening envelopes and structural attachments".into(),
        ],
    }
}

pub(super) fn resolve_cargo(items: &[DeckItem]) -> CargoSystem {
    let mut slots = Vec::new();
    let mut cargo_items = Vec::new();
    for (index, item) in items
        .iter()
        .filter(|item| matches!(item.kind, ItemKind::Uld | ItemKind::Bag))
        .enumerate()
    {
        let slot_id = format!("occupied-slot-{index}");
        let item_id = format!("cargo-item-{index}");
        let (uld_definition, fill, net) = match &item.meta {
            ItemMeta::Container(meta) => (
                cargo::uld_by_code(meta.uld).map(uld_definition),
                Some(meta.fill),
                meta.net,
            ),
            // Loose overflow is not a certified container, but it still uses
            // the shared bulk envelope for section/3D visualization. Keeping
            // the BLK definition in the interchange prevents the Python
            // renderer from silently falling back to a rectangular cuboid.
            ItemMeta::BulkBag => (cargo::uld("BLK").map(uld_definition), None, None),
            _ => (None, None, None),
        };
        slots.push(CargoSlot {
            id: slot_id.clone(),
            deck_id: item.deck.into(),
            envelope: box3(item),
            occupied_by: item_id.clone(),
            fidelity: "occupied_item_proxy".into(),
            source: "PayloadLayout does not preserve independent slot geometry".into(),
        });
        cargo_items.push(CargoItem {
            id: item_id,
            slot_id,
            envelope: box3(item),
            mass_kg: item.mass,
            fill_fraction: fill,
            net_load_kg: net,
            uld: uld_definition,
            orientation: OrientationStatus {
                value: None,
                status: "missing".into(),
                reason:
                    "orientation and mirroring are discarded when the cargo slot becomes a DeckItem"
                        .into(),
            },
            fidelity: "solver_resolved_envelope".into(),
            source: "PayloadLayout cargo DeckItem".into(),
        });
    }
    CargoSystem {
        slots,
        items: cargo_items,
        empty_slot_inventory: CargoInventoryStatus {
            available: false,
            reason: "PayloadLayout retains occupied items only".into(),
            required_input:
                "solver cargo-slot inventory including empty positions, IDs and orientations".into(),
        },
    }
}

fn uld_definition(uld: &'static cargo::UldType) -> UldDefinition {
    UldDefinition {
        key: uld.key.into(),
        code: uld.code.into(),
        name: uld.name.into(),
        dimensions_m: [uld.length, uld.width, uld.height],
        normalized_contour_yz: uld
            .contour
            .vertices
            .iter()
            .map(|p| Point2 { y: p[0], z: p[1] })
            .collect(),
        contour_source: uld.contour.source.into(),
        contour_fidelity: match uld.contour.fidelity {
            ContourFidelity::Authoritative => "authoritative",
            ContourFidelity::ConservativeEnvelope => "conservative_envelope",
            ContourFidelity::VisualizationOnly => "visualization_only",
        }
        .into(),
        mirrorable: uld.contour.mirrorable,
    }
}
