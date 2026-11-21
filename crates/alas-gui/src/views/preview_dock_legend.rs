// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cabin legend of the live preview: a screen-space key built from the
//! item kinds the cabin actually contains (seat classes, galleys, lavatories,
//! exits, floor and cargo zones, ULDs, point masses), so it follows the
//! aircraft type and the declared items instead of a fixed class list.

use alas_payload::layout::{ItemKind, ItemMeta, PayloadLayout};
use egui::{vec2, Color32, FontId, Frame, Id, RichText, Ui};

use super::{
    centered_row, preview_overlay_rects, text_size, PREVIEW_LEGEND_GAP, PREVIEW_OVERLAY_MARGIN,
};
use super::{LEGEND_INNER_MARGIN, LEGEND_ROW_GAP};
use crate::state::AppState;
use crate::views::tr_label as tl;

/// Entries per legend row, and the most rows the card draws.
const PER_ROW: usize = 3;
const MAX_ROWS: usize = 3;

/// One swatch and its (untranslated) label.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LegendEntry {
    pub color: Color32,
    pub label: String,
}

fn entry(hex: &str, label: &str) -> LegendEntry {
    LegendEntry {
        color: crate::theme::hex_to_color32(hex),
        label: label.to_owned(),
    }
}

/// The class layout key shown before a layout is known.
pub(super) fn default_entries() -> Vec<LegendEntry> {
    vec![
        entry("#8e44ad", "First class"),
        entry("#2980b9", "Business class"),
        entry("#27ae60", "Economy class"),
        entry("#e67e22", "Galley"),
        entry("#26c6da", "Lavatory"),
        entry("#e74c3c", "Exit"),
    ]
}

fn push_unique(entries: &mut Vec<LegendEntry>, next: LegendEntry) {
    if !entries.contains(&next) {
        entries.push(next);
    }
}

/// The legend of the item kinds present in `layout`, in cabin order. The
/// swatch colours are the ones the 3-D cabin figure paints the kinds in.
pub(super) fn entries_for(layout: &PayloadLayout) -> Vec<LegendEntry> {
    let present = |kind: ItemKind| layout.items.iter().any(|item| item.kind == kind);
    let mut entries = Vec::new();
    for class in ["First", "Business", "Premium", "Economy"] {
        let seats = layout.items.iter().any(|item| {
            item.kind == ItemKind::SeatRow
                && matches!(&item.meta, ItemMeta::Seat(meta) if class_of(meta.cls) == class)
        });
        if seats {
            entries.push(match class {
                "First" => entry("#8e44ad", "First class"),
                "Business" => entry("#2980b9", "Business class"),
                "Premium" => entry("#16a085", "Premium class"),
                _ => entry("#27ae60", "Economy class"),
            });
        }
    }
    for (kind, hex, label) in [
        (ItemKind::Galley, "#e67e22", "Galley"),
        (ItemKind::Lav, "#26c6da", "Lavatory"),
        (ItemKind::AccessibleLav, "#ec407a", "Accessible lavatory"),
        (ItemKind::WheelchairStowage, "#f4d03f", "Wheelchair stowage"),
        (ItemKind::OverheadBin, "#566573", "Overhead bins"),
        (ItemKind::Exit, "#e74c3c", "Exit"),
    ] {
        if present(kind) {
            entries.push(entry(hex, label));
        }
    }
    let zone_kinds: Vec<bool> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Zone)
        .map(|item| {
            matches!(
                &item.meta,
                ItemMeta::Declared(meta) if meta.zone_kind == Some(alas_config::ZoneKind::Cargo)
            )
        })
        .collect();
    if !zone_kinds.is_empty() {
        let label = match (
            zone_kinds.iter().any(|c| *c),
            zone_kinds.iter().any(|c| !*c),
        ) {
            (true, false) => "Cargo zone",
            (true, true) => "Cabin or cargo zone",
            _ => "Cabin zone",
        };
        entries.push(entry("#d7bde2", label));
    }
    if present(ItemKind::Bag) {
        entries.push(entry("#95a5a6", "Hold baggage and freight"));
    }
    let mut ulds: Vec<(&str, &str)> = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            ItemMeta::Container(meta) if item.kind == ItemKind::Uld => Some((meta.uld, meta.color)),
            _ => None,
        })
        .collect();
    ulds.sort_unstable();
    ulds.dedup();
    match ulds.as_slice() {
        [] => {}
        [(_, hex)] => entries.push(entry(hex, "Cargo container (ULD)")),
        many => {
            for (code, hex) in many.iter().take(2) {
                push_unique(
                    &mut entries,
                    LegendEntry {
                        color: crate::theme::hex_to_color32(hex),
                        label: format!("ULD {code}"),
                    },
                );
            }
        }
    }
    if present(ItemKind::PointMass) {
        entries.push(entry("#c0392b", "Point mass"));
    }
    entries.truncate(PER_ROW * MAX_ROWS);
    entries
}

/// The class a seat row's `cls` belongs to for the legend.
fn class_of(cls: &str) -> &str {
    match cls {
        "First" | "Business" | "Premium" => cls,
        _ => "Economy",
    }
}

/// The legend entries of the case: those of the layout the live cabin figure
/// is built from, kept until the configuration or design changes. When the
/// aircraft cannot be built the fixed class key stands.
pub(super) fn cabin_entries(state: &AppState, ctx: &egui::Context) -> Vec<LegendEntry> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", state.design_values).hash(&mut hasher);
    let key = state.config_fingerprint_now() ^ hasher.finish().rotate_left(13);
    let id = Id::new("preview_dock_cabin_legend");
    if let Some((cached, entries)) = ctx.data(|data| data.get_temp::<(u64, Vec<LegendEntry>)>(id)) {
        if cached == key {
            return entries;
        }
    }
    let entries = layout_entries(state).unwrap_or_else(default_entries);
    ctx.data_mut(|data| data.insert_temp(id, (key, entries.clone())));
    entries
}

fn layout_entries(state: &AppState) -> Option<Vec<LegendEntry>> {
    let mut config = state.typed_config()?;
    let design = state.current_design().unwrap_or_default();
    if config.requirements.cabin_preset != "Custom" {
        alas_payload::apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    config.geometry.wing.n_subdivisions = config.geometry.wing.n_subdivisions.clamp(1, 2);
    config.geometry.empennage.n_subdivisions = config.geometry.empennage.n_subdivisions.clamp(1, 2);
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let layout = alas_payload::build::build_payload_layout(&airplane, &config, 0.0, 0.0).ok()?;
    let entries = entries_for(&layout);
    (!entries.is_empty()).then_some(entries)
}

/// The legend's text size: the app's small size (`TextStyle::Small`). A dock
/// too narrow for a row of three entries gets rows of two or one rather than
/// smaller text.
const LEGEND_FONT: f32 = 13.0;

fn legend_item(ui: &mut Ui, color: Color32, label: &str, font: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
    ui.label(RichText::new(tl(label)).size(font));
}

fn legend_item_width(ui: &Ui, label: &str, font: f32) -> f32 {
    10.0 + ui.spacing().item_spacing.x + text_size(ui, &tl(label), FontId::proportional(font)).x
}

fn legend_row_width(ui: &Ui, entries: &[LegendEntry], font: f32) -> f32 {
    entries
        .iter()
        .map(|entry| legend_item_width(ui, &entry.label, font))
        .sum::<f32>()
        + ui.spacing().item_spacing.x * entries.len().saturating_sub(1) as f32
}

/// The most entries per row (three, then two, then one) at which every row of
/// `entries` fits `inner`.
fn legend_per_row(ui: &Ui, entries: &[LegendEntry], inner: f32) -> usize {
    (1..=PER_ROW)
        .rev()
        .find(|per_row| {
            entries
                .chunks(*per_row)
                .all(|row| legend_row_width(ui, row, LEGEND_FONT) <= inner)
        })
        .unwrap_or(1)
}

/// Height of a legend row: the tallest translated label or the swatch.
fn legend_row_height(ui: &Ui, entries: &[LegendEntry], font: f32) -> f32 {
    entries
        .iter()
        .map(|entry| text_size(ui, &tl(&entry.label), FontId::proportional(font)).y)
        .fold(10.0_f32, f32::max)
}

/// Height of the legend card: title row, the entry rows, the vertical spacing
/// between them and the frame's inner margin.
fn legend_frame_height(ui: &Ui, entries: &[LegendEntry], font: f32, per_row: usize) -> f32 {
    let rows = entries.len().div_ceil(per_row).max(1) as f32;
    let title_height = text_size(ui, &tl("Cabin legend"), FontId::proportional(font)).y;
    2.0 * LEGEND_INNER_MARGIN
        + title_height
        + rows * legend_row_height(ui, entries, font)
        + rows * LEGEND_ROW_GAP
}

/// Entries per row and card height of the legend over `viewport`.
fn legend_metrics(ui: &Ui, viewport: egui::Rect, entries: &[LegendEntry]) -> (usize, f32) {
    let (controls, region) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);
    let room = region.bottom() - (controls.bottom() + PREVIEW_LEGEND_GAP);
    let per_row = legend_per_row(
        ui,
        entries,
        region.width() - 2.0 * LEGEND_INNER_MARGIN - 6.0,
    );
    let height =
        legend_frame_height(ui, entries, LEGEND_FONT, per_row).min(room.max(region.height()));
    (per_row, height)
}

/// What the Interior mode draws over a pane of `size` points: the top and bottom bands, points,
/// that the cabin model must stay out of (the controls row; the legend card with its margin and
/// gap), so the model is fitted into the area between them.
pub(super) fn reserved_bands(ui: &Ui, size: (f32, f32), entries: &[LegendEntry]) -> (f64, f64) {
    let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(size.0, size.1));
    let (controls, _) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);
    let (_, height) = legend_metrics(ui, viewport, entries);
    let top = controls.bottom() + PREVIEW_LEGEND_GAP;
    let bottom = height + PREVIEW_OVERLAY_MARGIN + PREVIEW_LEGEND_GAP;
    (f64::from(top), f64::from(bottom))
}

/// The fixed class key (tests and the case with no layout yet).
#[cfg(test)]
pub(super) fn show_cabin_legend(ui: &mut Ui, viewport: egui::Rect) {
    show_cabin_legend_for(ui, viewport, &default_entries());
}

/// Draw a readable screen-space key that never follows the 3-D camera.
///
/// The legend sits along the bottom edge of the viewport, away from the cabin
/// geometry, which the camera can otherwise place under a top-mounted card.
/// Its entries use rows of three so they stay inside the dock when translated
/// labels or a narrow dock need more room. The card is sized from its
/// measured content and anchored to the bottom of the legend region, so its
/// lower edge keeps the overlay margin from the viewport edge; a third row
/// grows upward as far as the controls allow.
pub(super) fn show_cabin_legend_for(ui: &mut Ui, viewport: egui::Rect, entries: &[LegendEntry]) {
    let (_, region) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);
    let font = LEGEND_FONT;
    let (per_row, frame_height) = legend_metrics(ui, viewport, entries);
    let rect = egui::Rect::from_min_size(
        egui::pos2(region.left(), region.bottom() - frame_height),
        vec2(region.width(), frame_height),
    );
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
        Frame::group(ui.style())
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(LEGEND_INNER_MARGIN))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // Keep the longest translated row inside the minimum
                    // 300-point dock while preserving a visible gap between
                    // each swatch/label pair.
                    ui.spacing_mut().item_spacing = vec2(4.0, LEGEND_ROW_GAP);
                    ui.label(RichText::new(tl("Cabin legend")).strong().size(font));
                    let row_height = legend_row_height(ui, entries, font);
                    for (index, row) in entries.chunks(per_row).enumerate() {
                        let row_id = Id::new(("cabin_legend_row", index));
                        let row_width = legend_row_width(ui, row, font);
                        centered_row(ui, row_id, row_width, row_height, |ui| {
                            for entry in row {
                                legend_item(ui, entry.color, &entry.label, font);
                            }
                        });
                    }
                    // Claim any sub-pixel remainder so the painted card ends
                    // exactly at the region's bottom edge.
                    let remainder = ui.available_size_before_wrap();
                    if remainder.y > 0.0 {
                        ui.allocate_space(vec2(0.0, remainder.y));
                    }
                });
            });
    });
}
