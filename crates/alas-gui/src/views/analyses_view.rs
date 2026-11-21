// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Analyses: one place to choose which analysis disciplines a Run
//! performs. The
//! toggles write the same configuration flags the analysis stages own.

use egui::{vec2, Frame, RichText, ScrollArea, TextStyle, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::{tr, tr_label};

/// Keep a whole analysis group readable in each column, including the long
/// structural-stage labels. Two columns balance the structural detail against
/// the core and downstream groups; smaller windows stack them vertically.
const MIN_ANALYSIS_COLUMN_WIDTH: f32 = 480.0;
/// Page width per column from which the three-column layout is used.
const WIDE_ANALYSIS_COLUMN_WIDTH: f32 = 620.0;

/// Render the Analyses page.
pub fn show_analyses_view(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| show_analyses_body(state, ui));
}

/// The Analyses page content without its own scroll area, for the flow page frame.
pub(crate) fn show_analyses_body(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr_label("Analyses")).on_hover_text(tr_label(
        "Choose which analysis disciplines a Run performs. Everything is on by default; \
         disabling an optional discipline skips its pipeline stage. Fine-grained settings \
         are in the Advanced sections of each page; external-tool paths are on the External Tools page.",
    ));
    ui.add_space(6.0);
    // The check box and the lock of a fixed row are one width, so every row's
    // label starts at the same x.
    ui.spacing_mut().icon_width = 16.0;
    let width = ui.available_width();
    if width >= 3.0 * WIDE_ANALYSIS_COLUMN_WIDTH {
        // Very wide windows: Core | Downstream tools on the left two thirds
        // with the run options under them, Optional (the tall group) on the
        // right, so no column leaves a hole under it.
        let total = ui.available_width();
        let left = (total * 2.0 / 3.0).round();
        let right = total - left - 14.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.allocate_ui_with_layout(
                vec2(left, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    ui.columns(2, |columns| {
                        core_section(&mut columns[0]);
                        downstream_section(state, &mut columns[1], 0.0);
                    });
                    ui.add_space(8.0);
                    crate::views::inputs_view::show_run_options_card(state, ui);
                },
            );
            ui.allocate_ui_with_layout(
                vec2(right, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(right);
                    optional_section(state, ui, 0.0);
                },
            );
        });
        return;
    } else if width >= 2.0 * MIN_ANALYSIS_COLUMN_WIDTH {
        // The two columns end at one bottom: the last row of the shorter
        // column stretches to the longer one.
        let set = egui::Id::new("analyses_columns");
        ui.columns(2, |columns| {
            let pads =
                [0, 1].map(|column| crate::theme::column_padding(&columns[column], set, column));
            let top = columns[0].min_rect().top();
            core_section(&mut columns[0]);
            columns[0].add_space(8.0);
            downstream_section(state, &mut columns[0], pads[0]);
            optional_section(state, &mut columns[1], pads[1]);
            for (column, pad) in pads.into_iter().enumerate() {
                let height = columns[column].min_rect().bottom() - top;
                crate::theme::record_column_end(&columns[column], set, column, height, pad);
            }
        });
    } else {
        core_section(ui);
        ui.add_space(8.0);
        optional_section(state, ui, 0.0);
        ui.add_space(8.0);
        downstream_section(state, ui, 0.0);
    }
    ui.add_space(8.0);
    crate::views::inputs_view::show_run_options_card(state, ui);
}

fn core_section(ui: &mut Ui) {
    crate::views::focus_card::group_heading(ui, "Core (every run)");
    locked_row(
        ui,
        "Aerodynamics: VLM + drag build-up",
        "Native vortex-lattice analysis, drag polar, span loading, V-n envelope and \
         dynamic modes of the optimized design. The pipeline's backbone; cannot be skipped.",
    );
    locked_row(
        ui,
        "Weight & Balance / Stability",
        "Selected mass method (compatibility fractions or architecture-dependent NASA \
         FLOPS), CG solve and envelope, static margin, landing-gear placement and \
         cabin/payload layout.",
    );
    locked_row(
        ui,
        "Propulsion cycle",
        "On-design turbofan cycle analysis of the selected engine: cycle summary, carpet \
         plot, efficiency decomposition and sweeps on the Propulsion results tab.",
    );
    locked_row(
        ui,
        "Field performance",
        "V-speeds, balanced field length and landing distances for the selected departure/ \
         arrival airports: the Matching Chart and Landing & Take-Off results tabs.",
    );
}

/// The optional analyses; `pad` lengthens the last row (see
/// [`crate::theme::column_padding`]).
fn optional_section(state: &mut AppState, ui: &mut Ui, pad: f32) {
    crate::views::focus_card::group_heading(ui, "Optional (toggle per run)");
    toggle_row(
        state,
        ui,
        "mission",
        "Mission analysis",
        "Flies the full route natively: fuel burn, flight profile, aero coefficient \
         histories and the 3D route globe. Degrades gracefully when the environment is \
         missing.",
        None,
    );
    toggle_row(
        state,
        ui,
        "mses",
        "2-D airfoil analysis (MSES)",
        "High-fidelity viscous/transonic polar and pressure/Mach-contour analysis of the \
         optimized root section. Sweep settings are in the MSES Analysis section of Advanced.",
        Some(airfoil_cfd_button),
    );
    toggle_field_row_with(
        state,
        ui,
        ("structures", "enabled"),
        "Structures (wingbox FEM)",
        "Sizes a generic wingbox from strength requirements, always available \
         analytically; add a NASTRAN path on the External Tools page for a real solve.",
        Some(structures_details),
        pad,
    );
}

/// The standalone 2-D OpenFOAM study belongs with the other 2-D airfoil
/// analysis, so it opens from that row instead of from the page title.
fn airfoil_cfd_button(state: &mut AppState, ui: &mut Ui) {
    if ui
        .add(
            egui::Button::new(tr("Open airfoil CFD\u{2026}"))
                .min_size(egui::vec2(0.0, crate::theme::control_height(ui))),
        )
        .on_hover_text(tr(
            "Open the standalone 2-D OpenFOAM study for any database airfoil.",
        ))
        .clicked()
    {
        crate::native_viewport::raise(ui.ctx(), "airfoil_cfd");
        state.cfd.window_open = true;
        state.cfd.tab = crate::cfd::CfdTab::Study;
    }
}

/// The structural solver cases and external stages, inside the Structures row
/// and indented to its label.
fn structures_details(state: &mut AppState, ui: &mut Ui) {
    ui.indent("structures_details", |ui| {
        structures_case_rows(state, ui);
        structures_external_rows(state, ui);
    });
}

/// The external downstream tools; `pad` lengthens the last row (see
/// [`crate::theme::column_padding`]).
fn downstream_section(state: &mut AppState, ui: &mut Ui, pad: f32) {
    crate::views::focus_card::group_heading(ui, "External downstream tools");
    toggle_field_row(
        state,
        ui,
        "downstream",
        "openvsp",
        "OpenVSP geometry export",
        "Writes an inspectable OpenVSP script and, when OpenVSP is configured, materializes the \
         .vsp3 geometry and CAD preview. This export is required by VSPAERO.",
    );
    let openvsp_enabled = state
        .config_values
        .get("downstream")
        .and_then(|g| g.get("openvsp"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ui.add_enabled_ui(openvsp_enabled, |ui| {
        toggle_field_row(
            state,
            ui,
            "downstream",
            "vspaero",
            "VSPAERO analysis",
            "Runs OpenVSP's independent 3-D vortex-lattice comparison from the exported geometry. \
             Requires a configured VSPAERO executable.",
        );
    });
    toggle_field_row(
        state,
        ui,
        "downstream",
        "avl",
        "AVL comparison",
        "Runs Athena Vortex Lattice's take-off sweep and retains its SI deck for comparison. \
         The Aerodynamic results selector in the Advanced run options must include AVL.",
    );
    toggle_field_row_with(
        state,
        ui,
        ("downstream", "flowunsteady"),
        "FLOWUnsteady analysis",
        "Runs the FLOWUnsteady adapter and retains its request, result and solver logs. Set \
         the FLOWUnsteady executable under External tools before enabling a real external solve.",
        None,
        pad,
    );
}

/// Solver-case selection for the structural analysis, kept on Analyses next
/// to the discipline toggle; model settings stay under Modeling and Advanced
/// Settings > Structures.
fn structures_case_rows(state: &mut AppState, ui: &mut Ui) {
    let enabled = state
        .config_values
        .get("structures")
        .and_then(|g| g.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(RichText::new(tr_label("Structural solver cases")).strong());
        for (name, label) in crate::views::form_page::placement::STRUCTURES_CASES {
            let mut checked = state
                .config_values
                .get("structures")
                .and_then(|g| g.get(name))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if crate::checkbox::checkbox(ui, &mut checked, tr_label(label)).changed() {
                if let Some(values) = state.config_values.get_mut("structures") {
                    crate::views::form_page::placement::toggle_bool(values, name, checked);
                }
                state.on_config_modified();
            }
        }
    });
}

/// External structural stages whose prerequisites are selected alongside the
/// structural solver cases rather than hidden on the tool-path page.
fn structures_external_rows(state: &mut AppState, ui: &mut Ui) {
    let structures_enabled = state
        .config_values
        .get("structures")
        .and_then(|g| g.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ui.add_enabled_ui(structures_enabled, |ui| {
        ui.label(RichText::new(tr_label("Structural external stages")).strong());
        let mut run_nastran = structure_bool(state, "run_nastran");
        if crate::checkbox::checkbox(
            ui,
            &mut run_nastran,
            tr_label("NASTRAN solve / NASTRAN-95 comparison"),
        )
        .changed()
        {
            set_structure_bool(state, "run_nastran", run_nastran);
        }
        let run_static = structure_bool(state, "run_sol_static");
        ui.add_enabled_ui(run_nastran && run_static, |ui| {
            let mut run_patran = structure_bool(state, "run_patran_export");
            if crate::checkbox::checkbox(ui, &mut run_patran, tr_label("Patran deformation export"))
                .changed()
            {
                set_structure_bool(state, "run_patran_export", run_patran);
            }
        });
    });
}

fn structure_bool(state: &AppState, name: &str) -> bool {
    state
        .config_values
        .get("structures")
        .and_then(|group| group.get(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn set_structure_bool(state: &mut AppState, name: &str, enabled: bool) {
    if let Some(values) = state.config_values.get_mut("structures") {
        crate::views::form_page::placement::toggle_bool(values, name, enabled);
    }
    state.on_config_modified();
}

/// The frame of one analysis row: the card fill and border of the cards on
/// the page (Run options, the settings sections), at row size.
fn row_frame(ui: &Ui) -> Frame {
    crate::theme::card_frame(ui)
        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
        .rounding(egui::Rounding::same(8.0))
}

/// A row that cannot be switched off: the same card, check box and label as a
/// toggle row, so every row of a section sits on one grid.
fn locked_row(ui: &mut Ui, title: &str, desc: &str) {
    row_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        row_line(ui, |ui| {
            // A fixed-on row, not a disabled control: a lock, the label in the
            // normal text colour and the weak tag. The lock sits where a check
            // box would and the label has a check box label's font; the tag
            // stands at the row's right edge, where a toggle row's button does.
            ui.spacing_mut().item_spacing.x = ui.spacing().icon_spacing;
            crate::lock_icon::lock_icon(ui, &tr_label("always runs"));
            ui.label(
                RichText::new(tr_label(title))
                    .strong()
                    .text_style(TextStyle::Button),
            )
            .on_hover_text(tr_label(desc));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(tr_label("always runs")).weak());
            });
        });
    });
}

/// The line of an analysis row, one height for every row: a button's height
/// (the tallest control a row holds), its content centred in it.
fn row_line(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let size = vec2(ui.available_width(), crate::theme::control_height(ui));
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            add(ui);
        },
    );
}

/// Extra content of a row: drawn right-aligned beside the check box, or below
/// it, by the row function that takes it.
type RowExtra = fn(&mut AppState, &mut Ui);

fn toggle_row(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    title: &str,
    desc: &str,
    extra: Option<RowExtra>,
) {
    toggle_field_row_with(state, ui, (group, "enabled"), title, desc, extra, 0.0);
}

fn toggle_field_row(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    field: &str,
    title: &str,
    desc: &str,
) {
    toggle_field_row_with(state, ui, (group, field), title, desc, None, 0.0);
}

/// A row with a check box for the flag `field` of `group`; `pad` lengthens
/// the row at its bottom (the last row of a column, see
/// [`crate::theme::column_padding`]).
fn toggle_field_row_with(
    state: &mut AppState,
    ui: &mut Ui,
    (group, field): (&str, &str),
    title: &str,
    desc: &str,
    extra: Option<RowExtra>,
    pad: f32,
) {
    row_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let mut enabled = state
            .config_values
            .get(group)
            .and_then(|g| g.get(field))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut toggled = false;
        row_line(ui, |ui| {
            toggled = crate::checkbox::checkbox(
                ui,
                &mut enabled,
                RichText::new(tr_label(title)).strong(),
            )
            .on_hover_text(tr_label(desc))
            .changed();
            // The button of a 2-D airfoil row sits at the right edge; the
            // structures details are a block below the row.
            if let (Some(extra), "mses") = (extra, group) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    extra(state, ui);
                });
            }
        });
        if toggled {
            if let Some(obj) = state
                .config_values
                .get_mut(group)
                .and_then(Value::as_object_mut)
            {
                obj.insert(field.to_owned(), Value::Bool(enabled));
            }
            state.on_config_modified();
        }
        if let (Some(extra), "structures") = (extra, group) {
            extra(state, ui);
        }
        if pad > 0.0 {
            // Exactly `pad`: the spacing a cursor advance adds would not count.
            ui.expand_to_include_y(ui.min_rect().bottom() + pad);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::show_analyses_view;
    use crate::state::AppState;

    fn text_positions<const N: usize>(width: f32, labels: [&str; N]) -> [egui::Pos2; N] {
        let context = egui::Context::default();
        let mut state = AppState::default();
        let mut output = None;
        for _ in 0..2 {
            output = Some(context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1_400.0),
                    )),
                    ..egui::RawInput::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show_analyses_view(&mut state, ui);
                    });
                },
            ));
        }
        let output = output.expect("rendered analyses page");
        labels.map(|label| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == crate::views::tr_label(label) =>
                    {
                        Some(text.pos)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing section: {label}"))
        })
    }

    #[test]
    fn analysis_groups_use_the_available_width_and_stack_on_small_windows() {
        let [core, optional, downstream, patran] = text_positions(
            1_500.0,
            [
                "Core (every run)",
                "Optional (toggle per run)",
                "External downstream tools",
                "Patran deformation export",
            ],
        );
        assert!(optional.x > core.x + 400.0);
        assert!((core.x - downstream.x).abs() < 1.0);
        assert!((core.y - optional.y).abs() < 1.0);
        assert!(downstream.y > core.y);
        assert!(patran.y > core.y);

        // Very wide: Core | Downstream tools | Optional on one row.
        let [core, optional, downstream] = text_positions(
            2_600.0,
            [
                "Core (every run)",
                "Optional (toggle per run)",
                "External downstream tools",
            ],
        );
        assert!(downstream.x > core.x + 400.0);
        assert!(optional.x > downstream.x + 400.0);
        assert!((core.y - downstream.y).abs() < 1.0);
        assert!((core.y - optional.y).abs() < 1.0);

        let [core, optional, downstream] = text_positions(
            700.0,
            [
                "Core (every run)",
                "Optional (toggle per run)",
                "External downstream tools",
            ],
        );
        assert!((core.x - optional.x).abs() < 1.0);
        assert!((optional.x - downstream.x).abs() < 1.0);
        assert!(core.y < optional.y && optional.y < downstream.y);
    }

    /// Every row label of the page, fixed or switchable, starts at one x and
    /// is set in one font size.
    #[test]
    fn core_and_optional_rows_share_one_label_font_and_indent() {
        let context = egui::Context::default();
        let mut state = AppState::default();
        let mut output = None;
        for _ in 0..2 {
            output = Some(context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 1_800.0),
                    )),
                    ..egui::RawInput::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show_analyses_view(&mut state, ui);
                    });
                },
            ));
        }
        let output = output.expect("rendered analyses page");
        let find = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == crate::views::tr_label(label) =>
                    {
                        let size = text.galley.job.sections[0].format.font_id.size;
                        Some((text.pos.x, size))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing row: {label}"))
        };
        let core = find("Field performance");
        let optional = find("Mission analysis");
        let external = find("OpenVSP geometry export");
        assert!((core.0 - optional.0).abs() < 1.0, "{core:?} {optional:?}");
        assert!((core.0 - external.0).abs() < 1.0, "{core:?} {external:?}");
        assert!((core.1 - optional.1).abs() < 0.01, "{core:?} {optional:?}");
        assert!((core.1 - external.1).abs() < 0.01, "{core:?} {external:?}");
    }

    #[test]
    fn optional_panel_lists_every_external_downstream_stage() {
        let context = egui::Context::default();
        let mut state = AppState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 1_400.0),
                )),
                ..egui::RawInput::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_analyses_view(&mut state, ui);
                });
            },
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        for label in [
            "OpenVSP geometry export",
            "VSPAERO analysis",
            "AVL comparison",
            "FLOWUnsteady analysis",
            "NASTRAN solve / NASTRAN-95 comparison",
            "Patran deformation export",
        ] {
            assert!(
                labels.contains(&crate::views::tr_label(label).as_str()),
                "missing optional stage: {label}"
            );
        }
    }
}
