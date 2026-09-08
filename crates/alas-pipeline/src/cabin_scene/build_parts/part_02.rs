// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn resolve_cargo(items: &[DeckItem]) -> CargoSystem {
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

fn nominal_windows(c: &CabinGeometry, items: &[DeckItem]) -> NominalWindows {
    let mut apertures = Vec::new();
    for (index, row) in items
        .iter()
        .filter(|i| i.kind == ItemKind::SeatRow)
        .step_by(3)
        .enumerate()
    {
        let Some(deck) = c.passenger_decks.iter().find(|d| d.name == row.deck) else {
            continue;
        };
        let z = c.floor_z(deck, row.x) + 0.95;
        let Some((a, b)) = c.inner_semi_axes(row.x) else {
            continue;
        };
        let nz = ((z - c.zc_at(row.x)) / b).clamp(-0.98, 0.98);
        let y_abs = a * (1.0 - nz * nz).sqrt();
        for sign in [-1.0, 1.0] {
            let y = sign * y_abs;
            let ny = y / (a * a);
            let nnz = (z - c.zc_at(row.x)) / (b * b);
            let norm = ny.hypot(nnz);
            apertures.push(WindowAperture {
                id: format!(
                    "nominal-window-{index}-{}",
                    if sign < 0.0 { "port" } else { "starboard" }
                ),
                deck_id: row.deck.into(),
                x_m: row.x,
                center_yz_m: Point2 { y, z },
                outward_normal_yz: Point2 {
                    y: ny / norm,
                    z: nnz / norm,
                },
                width_m: 0.30,
                height_m: 0.45,
                fidelity: "nominal_fallback_not_for_clearance".into(),
            });
        }
    }
    NominalWindows {
        apertures,
        status: "nominal_fallback".into(),
        source: "row-sampled visualization fallback; not manufacturer data".into(),
        missing: vec![
            "window longitudinal stations and pitch".into(),
            "deck-specific belt datum and aperture profile".into(),
            "pane curvature, reveal depth and door exclusion zones".into(),
        ],
    }
}

fn box3(item: &DeckItem) -> Box3 {
    Box3 {
        center_x_m: item.x,
        center_y_m: item.y,
        center_z_m: item.z,
        length_m: item.length,
        width_m: item.width,
        height_m: item.height,
    }
}

fn missing_input(field: &str, reason: &str, source: &str) -> MissingInput {
    MissingInput {
        field: field.into(),
        reason: reason.into(),
        required_source: source.into(),
    }
}

fn base_missing() -> Vec<MissingInput> {
    vec![
        missing_input(
            "stations[].liner",
            "current model has no independent cabin liner mould line",
            "station-indexed aircraft CAD/ACAP/WBM contour",
        ),
        missing_input(
            "windows.authoritative_apertures",
            "window fallback must not drive physical clearance",
            "aircraft window schedule and aperture geometry",
        ),
        missing_input(
            "overhead.topology",
            "layout retains bin envelopes but not supplier installation topology",
            "bin supplier/CAD rails, attachments, valances, PSUs and kinematics",
        ),
        missing_input(
            "cargo.empty_slots",
            "resolved layout discards empty positions",
            "cargo solver slot inventory before loading",
        ),
        missing_input(
            "cargo.items[].orientation",
            "DeckItem does not retain orientation/mirror state",
            "cargo placement result slot ID and orientation",
        ),
    ]
}
