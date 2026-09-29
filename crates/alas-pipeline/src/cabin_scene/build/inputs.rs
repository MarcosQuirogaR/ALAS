// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Nominal window apertures, item boxes and the inputs the scene records as missing.

use super::*;

pub(super) fn nominal_windows(c: &CabinGeometry, items: &[DeckItem]) -> NominalWindows {
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

pub(super) fn box3(item: &DeckItem) -> Box3 {
    Box3 {
        center_x_m: item.x,
        center_y_m: item.y,
        center_z_m: item.z,
        length_m: item.length,
        width_m: item.width,
        height_m: item.height,
    }
}

pub(super) fn missing_input(field: &str, reason: &str, source: &str) -> MissingInput {
    MissingInput {
        field: field.into(),
        reason: reason.into(),
        required_source: source.into(),
    }
}

pub(super) fn base_missing() -> Vec<MissingInput> {
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
