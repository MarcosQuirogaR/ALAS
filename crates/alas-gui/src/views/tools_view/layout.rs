// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout primitives of the External Tools page: grouped cards in a masonry of
//! two columns, tool blocks (title row with status chip and documentation
//! link), and aligned label/value rows.

use egui::{vec2, Align, Id, Layout, RichText, Rounding, Stroke, TextEdit, Ui};

use crate::path_picker::ToolPathTarget;
use crate::state::AppState;
use crate::theme::space::ROW_GAP;
use crate::views::{tr, tr_label};

use super::field_row::{display_path, field_width, grid_row, path_elide};
use super::open_in_file_explorer;
use super::status::{chip_of, Chip, Tone};

/// Page width from which the cards sit in two columns.
pub(super) const TWO_COLUMN_WIDTH: f32 = 1100.0;
/// A card of the page: renders into the column it is given.
pub(super) type CardFn = fn(&mut AppState, &mut Ui);

/// A framed card holding `body`, as wide as its column.
pub(super) fn framed(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        body(ui);
    });
}

/// A card that groups tools of one discipline: a heading, then its blocks,
/// the forms' header-to-body gap apart.
pub(super) fn group(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    framed(ui, |ui| {
        crate::theme::header_label(ui, RichText::new(tr_label(title)).strong().size(16.0));
        body(ui);
    });
}

/// The gap and rule between two tool blocks of a group card.
pub(super) fn block_gap(ui: &mut Ui) {
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
}

/// One tool card: framed, with the block header and `body`.
pub(super) fn card(
    ui: &mut Ui,
    title: &str,
    link: &str,
    help: &[&str],
    body: impl FnOnce(&mut Ui),
) {
    framed(ui, |ui| tool_block(ui, None, title, link, help, body));
}

/// One tool inside a card: the title row (name, then the documentation link
/// and the status chip at the right) and its rows, the forms' header-to-body
/// gap under the title row. `help` is the title's tooltip.
pub(super) fn tool_block(
    ui: &mut Ui,
    key: Option<&str>,
    title: &str,
    link: &str,
    help: &[&str],
    body: impl FnOnce(&mut Ui),
) {
    let chip = key.and_then(|key| chip_of(ui.ctx(), key));
    // The title at the left, the chip and the documentation link flush right.
    // The gap is computed from the widths, not a right-to-left layout, which
    // collapses inside the page's column layout.
    let small = egui::TextStyle::Small.resolve(ui.style());
    let width_of = |ui: &Ui, text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), small.clone(), ui.visuals().text_color())
            .size()
            .x
    };
    let link_text = tr("Documentation");
    // A block without a documentation link keeps the link's room, so the chip
    // of every block sits at one x.
    let link_width = width_of(ui, &link_text) + 4.0;
    let chip_width = if chip.is_some() { chip_width(ui) } else { 0.0 };
    let spacing = ui.spacing().item_spacing.x;
    let title_row = |ui: &mut Ui| {
        let name = ui.label(RichText::new(tr(title)).strong().size(15.0));
        if !help.is_empty() {
            name.on_hover_text(
                help.iter()
                    .map(|paragraph| tr(paragraph))
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            );
        }
        let items = 1.0 + f32::from(u8::from(chip.is_some()));
        let right = link_width + chip_width + spacing * items;
        let gap = ui.available_width() - right;
        if gap > 0.0 {
            ui.add_space(gap);
        }
        if let Some(chip) = &chip {
            chip_label(ui, chip);
        }
        if !link.is_empty() {
            ui.hyperlink_to(RichText::new(link_text.as_str()).small(), link)
                .on_hover_text(link);
        }
    };
    crate::theme::header_row(ui, |ui| ui.horizontal(title_row));
    body(ui);
}

/// Every chip text the page can show; the badge is as wide as the widest.
const CHIP_TEXTS: &[&str] = &[
    "Ready",
    "Ready (found)",
    "Ready (set)",
    "Not found",
    "Check path",
    "Not set",
    "Configured",
];
/// Horizontal inner margin of a chip, per side, pt.
const CHIP_MARGIN: f32 = 7.0;

/// Outer width of every status chip of the page, pt: one width, so the
/// badges of all rows and headers line up.
pub(super) fn chip_width(ui: &Ui) -> f32 {
    let small = egui::TextStyle::Small.resolve(ui.style());
    let widest = CHIP_TEXTS
        .iter()
        .map(|text| {
            ui.painter()
                .layout_no_wrap(tr(text), small.clone(), ui.visuals().text_color())
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    (widest + 2.0 * CHIP_MARGIN + 2.0).ceil()
}

/// A status chip of the page's fixed chip width: the tone outlines it and
/// colours its centred text.
pub(super) fn chip_label(ui: &mut Ui, chip: &Chip) {
    let color = match chip.tone {
        Tone::Ready => crate::theme::success_color(ui.visuals()),
        Tone::Warn => ui.visuals().warn_fg_color,
        Tone::Missing | Tone::Neutral => ui.visuals().weak_text_color(),
    };
    let inner = chip_width(ui) - 2.0 * CHIP_MARGIN - 2.0;
    let line = ui.text_style_height(&egui::TextStyle::Small);
    egui::Frame::none()
        .stroke(Stroke::new(1.0_f32, color))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(CHIP_MARGIN, 1.0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                vec2(inner, line),
                Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| ui.label(RichText::new(chip.text.as_str()).color(color).small()),
            );
        });
}

/// Lay `cards` out in two columns balanced by height (each card as tall as its
/// own content), or in one column on a narrow page. Cards keep their order
/// within a column; each goes to the currently shorter column, using the
/// height it had on the previous frame.
pub(super) fn masonry(ui: &mut Ui, state: &mut AppState, cards: &[(&'static str, CardFn)]) {
    let ctx = ui.ctx().clone();
    let height_id = |name: &str| Id::new(("external_tool_card_height", name));
    let remembered = |name: &str| {
        ctx.data(|data| data.get_temp::<f32>(height_id(name)))
            .unwrap_or(400.0)
    };
    let columns = if ui.available_width() >= TWO_COLUMN_WIDTH {
        2
    } else {
        1
    };
    let mut assigned = vec![Vec::new(); columns];
    let mut filled = vec![0.0_f32; columns];
    for (index, (name, _)) in cards.iter().enumerate() {
        let shortest = (0..columns)
            .min_by(|a, b| filled[*a].total_cmp(&filled[*b]))
            .unwrap_or(0);
        assigned[shortest].push(index);
        filled[shortest] += remembered(name) + ui.spacing().item_spacing.y;
    }
    let mut draw = |ui: &mut Ui, index: usize| {
        let (name, card) = cards[index];
        // Cards stand one item spacing apart, as the cards of a form do.
        let height = ui.scope(|ui| card(state, ui)).response.rect.height();
        ctx.data_mut(|data| data.insert_temp(height_id(name), height));
    };
    if columns == 1 {
        for index in &assigned[0] {
            draw(ui, *index);
        }
        return;
    }
    ui.columns(columns, |slots| {
        for (slot, indices) in slots.iter_mut().zip(&assigned) {
            for index in indices {
                draw(slot, *index);
            }
        }
    });
}

/// A sub-heading inside a card.
pub(super) fn sub_heading(ui: &mut Ui, title: &str) -> egui::Response {
    ui.label(RichText::new(tr(title)).strong())
}

/// Width of a button with `text`, pt.
fn button_width(ui: &Ui, text: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::TextStyle::Button.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}

/// One editable path or value on the page's label/field grid (see
/// `field_row`): the label column is shared by every card, the field fills the
/// rest of the row, and the Browse and Open folder buttons have one width.
pub(super) fn text_row(
    state: &mut AppState,
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    directory: bool,
    picker_target: Option<ToolPathTarget>,
) -> bool {
    let help = text_row_help(label);
    let height = crate::theme::control_height(ui);
    let changed = grid_row(ui, label, help, |ui| {
        value_box(state, ui, value, directory, picker_target, height)
    });
    // The form grid's gap from a row's boxes to the next row's label.
    ui.add_space((ROW_GAP - ui.spacing().item_spacing.y).max(0.0));
    changed
}

/// Width shared by the Browse and Open folder buttons, pt.
pub(super) fn picker_button_width(ui: &Ui) -> f32 {
    button_width(ui, &tr("Browse..."))
        .max(button_width(ui, &tr("Open")))
        .ceil()
}

/// The location a configured path stands for: the text itself when it is
/// empty or absolute, else where discovery found the tool, or the
/// application folder joined with the text (the first place a relative path
/// is looked for).
fn resolved_path(text: &str, found: Option<&str>, app_root: &std::path::Path) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || std::path::Path::new(trimmed).is_absolute() {
        return text.to_owned();
    }
    found.map_or_else(
        || app_root.join(trimmed).display().to_string(),
        str::to_owned,
    )
}

/// The value box and its buttons, on the current horizontal row.
fn value_box(
    state: &mut AppState,
    ui: &mut Ui,
    value: &mut String,
    directory: bool,
    picker_target: Option<ToolPathTarget>,
    height: f32,
) -> bool {
    let browse = tr("Browse...");
    let open = tr("Open");
    let found = picker_target.and_then(|target| {
        let key = match target {
            ToolPathTarget::NastranExecutable => "nastran",
            ToolPathTarget::NastranSolver => "nastran_solver",
            ToolPathTarget::PatranExecutable => "patran",
            ToolPathTarget::MsesDirectory => "mses",
            ToolPathTarget::AvlExecutable => "avl",
            _ => return None,
        };
        super::status::found_path(ui.ctx(), key)
    });
    let spacing = ui.spacing().item_spacing.x;
    let button_w = picker_button_width(ui);
    // Rows without buttons reserve the same room, so every box has one width.
    let buttons = 2.0 * button_w + 2.0 * spacing;
    let margin = egui::Margin::symmetric(4.0, 2.0);
    let width = field_width(ui, buttons, margin.sum().x);
    let id = ui.auto_id_with("tool_value_box");
    let editing = ui.memory(|memory| memory.has_focus(id));
    let inner = width - margin.sum().x;
    let font = egui::TextStyle::Body.resolve(ui.style());
    let app_root = state.tool_locator.app_root().to_path_buf();
    let is_path = picker_target.is_some();
    // While the box is not being edited a path shows resolved (absolute, like
    // every other path of the page) and, when long, middle-elided; the text
    // itself is untouched and shows as entered as soon as the box has focus.
    let mut layouter = |ui: &Ui, text: &str, _wrap: f32| {
        let color = ui.visuals().text_color();
        let shown = if editing {
            text.to_owned()
        } else {
            let resolved = if is_path {
                resolved_path(text, found.as_deref(), &app_root)
            } else {
                text.to_owned()
            };
            path_elide(&display_path(&resolved), |candidate| {
                ui.painter()
                    .layout_no_wrap(candidate.to_owned(), font.clone(), color)
                    .size()
                    .x
                    <= inner
            })
        };
        ui.fonts(|fonts| {
            fonts.layout_job(egui::text::LayoutJob::simple_singleline(
                shown,
                font.clone(),
                color,
            ))
        })
    };
    // An empty box shows where discovery found the tool, elided like a value
    // instead of cut at the box edge.
    let weak = ui.visuals().weak_text_color();
    let hint = found.as_deref().map_or_else(String::new, |found| {
        path_elide(&display_path(found), |candidate| {
            ui.painter()
                .layout_no_wrap(candidate.to_owned(), font.clone(), weak)
                .size()
                .x
                <= inner
        })
    });
    let response = ui.add(
        TextEdit::singleline(value)
            .id(id)
            .hint_text(RichText::new(hint).weak())
            .margin(margin)
            .desired_width(width)
            .min_size(vec2(0.0, height))
            .vertical_align(Align::Center)
            .layouter(&mut layouter),
    );
    // A long path shows elided in the box; the tooltip shows all of it, and
    // the found location when the box is empty.
    let full = if value.is_empty() {
        found.as_deref().map(display_path)
    } else {
        Some(value.clone())
    };
    if let Some(full) = full {
        response.clone().on_hover_text(full);
    }
    let changed = response.changed();
    if let Some(target) = picker_target {
        let pending = state.path_picker_pending(target);
        if ui
            .add_enabled(
                !pending,
                egui::Button::new(browse).min_size(vec2(button_w, height)),
            )
            .on_hover_text(tr(
                "Choose the directory or executable with the native file picker. The selected location is saved in this user's preferences.",
            ))
            .clicked()
        {
            state.begin_path_picker(target, directory);
        }
        if ui
            .add_enabled(
                !value.trim().is_empty() || found.is_some(),
                egui::Button::new(open).min_size(vec2(button_w, height)),
            )
            .on_hover_text(tr("Open folder"))
            .clicked()
        {
            let target = if value.trim().is_empty() {
                found.clone().unwrap_or_default()
            } else {
                resolved_path(value, found.as_deref(), state.tool_locator.app_root())
            };
            if let Err(error) = open_in_file_explorer(&target, directory) {
                state.log(error, crate::state::LogKind::Warn);
            }
        }
    }
    changed
}

fn text_row_help(label: &str) -> Option<&'static str> {
    match label {
        "MSC solver override" => Some(
            "For the split MSC Student Edition, select Patran/.../analysis.exe here. Leave blank when the launcher finds its own solver.",
        ),
        "Short RF staging directory" => Some(
            "Set these three paths once to run local SOL 101/SOL 103 beside MSC in every desktop launch. The RF directory must be an absolute path shorter than 38 bytes (for example C:/nas-rf on Windows or /tmp/nas-rf on Linux).",
        ),
        "Open-core words (OCMEM)" => Some(
            "Leave OCMEM blank to use the capacity compiled into the NASTRAN-95 executable. This local build records its limit beside the executable; rebuild it with a larger open-core array only if the full mesh exceeds that recorded allocation.",
        ),
        "WSL environment launcher" => Some(
            "The launcher receives each utility and its arguments inside WSL2, for example openfoam2306 from the openfoam.com Ubuntu packages. Without it, WSL runs the utilities without the OpenFOAM environment and a bin directory alone does not load their libraries.",
        ),
        _ => None,
    }
}
