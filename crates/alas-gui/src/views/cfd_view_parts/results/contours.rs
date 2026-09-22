// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native OpenFOAM contour artifacts rendered outside the result parser.

use super::super::widgets::{card, WIDE_ROW_WIDTH};
use super::tr;
use egui::{RichText, Ui};

/// Display contour artifacts rendered from the native OpenFOAM fields.  The
/// renderer writes these files outside the result parser (usually through the
/// bundled ParaView batch script), so a missing image stays visibly
/// unavailable rather than becoming a synthetic scalar plot.
pub(super) fn show_contour_images(
    textures: &mut std::collections::BTreeMap<String, egui::TextureHandle>,
    result: &alas_cfd::CfdResults,
    ui: &mut Ui,
) {
    let figure_root = result
        .case_dir
        .join("postProcessing")
        .join("alas-field-figures");
    let figures = [
        ("Mach contour", "mach-contour.png", "Mach [-]"),
        (
            "Pressure contour",
            "pressure-contour.png",
            "Gauge pressure [Pa]",
        ),
    ]
    .into_iter()
    .filter_map(|(title, filename, unit)| {
        let path = figure_root.join(filename);
        path.is_file().then_some((title, path, unit))
    })
    .collect::<Vec<_>>();
    if figures.is_empty() {
        card(ui, "Mach and pressure contours", "", |ui| {
            ui.label(
                RichText::new(tr("Unavailable: no native ParaView contour artifacts were rendered for this case. The OpenFOAM p and U fields remain available in the field list and ParaView handoff."))
                    .weak()
                    .small(),
            );
        });
        return;
    }
    let mut show_figure = |ui: &mut Ui, title: &str, path: &std::path::Path, unit: &str| {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr(title)).strong());
                ui.label(RichText::new(tr(unit)).weak().small());
                ui.label(RichText::new("(i)").weak().small())
                    .on_hover_text(format!(
                        "{}\n{}",
                        tr("Mach uses |U|/a at the recorded static temperature. Incompressible cases show gauge rho*p; compressible cases show solved absolute p minus the recorded static reference. Both images retain the exact case folder and solved write time."),
                        path.display()
                    ));
            });
            let key = path.to_string_lossy().to_string();
            let texture = if let Some(texture) = textures.get(&key) {
                Some(texture.clone())
            } else {
                let bytes = std::fs::read(path).ok();
                let decoded = bytes
                    .as_deref()
                    .and_then(|bytes| eframe::icon_data::from_png_bytes(bytes).ok());
                decoded.map(|icon| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [icon.width as usize, icon.height as usize],
                        &icon.rgba,
                    );
                    let texture = ui.ctx().load_texture(
                        format!("airfoil-cfd-contour:{key}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    textures.insert(key.clone(), texture.clone());
                    texture
                })
            };
            if let Some(texture) = texture {
                let width = ui.available_width().max(200.0);
                let height = width * texture.size_vec2().y / texture.size_vec2().x;
                ui.add(
                    egui::Image::from_texture(&texture)
                        .fit_to_exact_size(egui::vec2(width, height.min(460.0))),
                );
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Contour image could not be decoded from the recorded case artifact."),
                );
            }
        });
    };
    if figures.len() == 2 && ui.available_width() >= WIDE_ROW_WIDTH {
        ui.columns(2, |columns| {
            for (column, (title, path, unit)) in columns.iter_mut().zip(figures.iter()) {
                show_figure(column, title, path, unit);
            }
        });
    } else {
        for (index, (title, path, unit)) in figures.iter().enumerate() {
            if index > 0 {
                ui.add_space(8.0);
            }
            show_figure(ui, title, path, unit);
        }
    }
}
