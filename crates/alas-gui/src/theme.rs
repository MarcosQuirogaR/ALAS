// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Theme palette adapter mapping ALAS design system palettes to `egui::Visuals`.

use alas_report::theme::{get_palette, Palette};
use egui::{
    vec2, Align2, Color32, Context, FontId, Frame, Margin, Response, Rounding, Sense, Stroke,
    TextStyle, Ui, Visuals,
};

/// Available UI themes for the desktop application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    /// High-contrast sleek dark engineering palette.
    #[default]
    Dark,
    /// Clean high-readability light presentation palette.
    Light,
    /// Neutral slate grey workstation palette.
    Grey,
}

impl AppTheme {
    /// Human-readable display label.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Grey => "Grey",
        }
    }

    /// Retrieve the corresponding `alas-report` palette specification.
    pub fn palette(&self) -> &'static Palette {
        match self {
            Self::Dark => get_palette(Some("dark-accessible")),
            Self::Light => get_palette(Some("light")),
            Self::Grey => get_palette(Some("grey-accessible")),
        }
    }

    /// Report palette identifier used by live scenes and exported figures.
    pub fn figure_theme_name(&self) -> &'static str {
        self.palette().name
    }
}

/// Convert a hex color code string (e.g. `"#1e293b"`) to an `egui::Color32`.
pub fn hex_to_color32(hex: &str) -> Color32 {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        Color32::from_rgb(r, g, b)
    } else {
        Color32::WHITE
    }
}

/// Status color for successful checks, selected for readable normal text.
pub fn success_color(visuals: &Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(126, 231, 165)
    } else {
        Color32::from_rgb(20, 105, 57)
    }
}

/// Return the shared elevated surface used for cards, navigation groups, and
/// other bounded work areas.
///
/// A distinct fill plus a single ordered corner radius makes grouping visible
/// without relying on the indentation rule that competes with form content.
pub const CARD_INNER_MARGIN_X: f32 = 14.0;

/// Corner radius shared by ordinary and selected navigation controls.
///
/// `egui::Button::selected` deliberately defaults to square corners. Keeping
/// the radius here prevents a selected workflow or results tab from breaking
/// the rounded-control language used everywhere else in the desktop shell.
pub const SELECTABLE_CONTROL_RADIUS: f32 = 6.0;

/// Return a selectable button that retains the desktop's rounded shape when
/// its selection fill is active.
pub fn selectable_button(
    text: impl Into<egui::WidgetText>,
    selected: bool,
) -> egui::Button<'static> {
    egui::Button::new(text)
        .selected(selected)
        .rounding(Rounding::same(SELECTABLE_CONTROL_RADIUS))
}

pub fn card_frame(ui: &Ui) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .inner_margin(Margin::symmetric(CARD_INNER_MARGIN_X, 10.0))
        .rounding(Rounding::same(12.0))
}

/// Return the content width that produces the requested outer card width.
///
/// A frame's horizontal inner margin belongs to the painted card, not to the
/// child UI that receives a requested width. Accounting for it here keeps a
/// row of cards inside its measured gallery width instead of growing each card
/// by the frame padding.
pub fn card_content_width(outer_width: f32) -> f32 {
    (outer_width - 2.0 * CARD_INNER_MARGIN_X).max(1.0)
}

/// Return the attached surface for the compact navigation rail and its hover
/// expansion. The fill deliberately interpolates from the accent rail into
/// the navigation panel so opening it reads as one continuous surface.
pub fn navigation_overlay_frame(ui: &Ui, expansion: f32) -> Frame {
    let expansion = expansion.clamp(0.0, 1.0);
    let padding = 14.0 * expansion;
    Frame::group(ui.style())
        .fill(blend_color(
            ui.visuals().hyperlink_color,
            ui.visuals().panel_fill,
            expansion,
        ))
        .stroke(Stroke::new(
            1.0_f32,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .inner_margin(Margin::symmetric(padding, 10.0 * expansion))
        .rounding(Rounding {
            nw: 0.0,
            ne: 12.0,
            sw: 0.0,
            se: 12.0,
        })
}

/// Draw a compact close affordance without the default button container.
/// The normal text colour keeps it quietly visible; hover switches to the
/// semantic error colour so its destructive action remains unmistakable.
pub fn close_icon_button(ui: &mut Ui, hover_text: impl Into<egui::WidgetText>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let color = if response.hovered() {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "x",
        FontId::proportional(18.0),
        color,
    );
    response.on_hover_text(hover_text)
}

fn blend_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let mix = |start: u8, end: u8| {
        (f32::from(start) + (f32::from(end) - f32::from(start)) * amount).round() as u8
    };
    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

fn selection_foreground(accent: Color32) -> Color32 {
    if contrast_ratio(Color32::BLACK, accent) >= contrast_ratio(Color32::WHITE, accent) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// How far a hovered control's tint sits from the selected fill, toward the
/// panel it floats on. Selected, hovered, pressed/focused and open were all
/// drawn with one saturated accent, so an open menu, a hovered tab and the
/// actually selected tab were indistinguishable.
const HOVER_TINT_TOWARD_PANEL: f32 = 0.62;

/// The same blend for a pressed, keyboard-focused or open control: stronger
/// than hover, still clearly weaker than a selected fill, and additionally
/// carrying the wider `active` ring.
const PRESSED_TINT_TOWARD_PANEL: f32 = 0.40;

/// The fill behind a *selected* control.
///
/// `apply_theme` sets `override_text_color` to the palette title colour, and
/// egui resolves a plain or strong widget label through that override before
/// any per-widget fallback (`RichText::get_text_color`,
/// `Visuals::strong_text_color`). The foreground on a selected control is
/// therefore the theme's own text colour - white in Dark and Grey, black in
/// Light - and the fill has to clear WCAG 2.1 AA 4.5:1 against *that*, not
/// against white everywhere. The palette accents did not: `#4f8cff` with white
/// is 3.22:1 and `#6aa2ff` with white is 2.55:1.
fn accent_fill(theme: AppTheme) -> Color32 {
    match theme {
        // White label text: 5.17:1.
        AppTheme::Dark | AppTheme::Grey => Color32::from_rgb(0x25, 0x63, 0xEB),
        // Black label text: 6.50:1, and 3.23:1 against the white page, so the
        // filled control keeps a 1.4.11 boundary even without its stroke.
        AppTheme::Light => Color32::from_rgb(0x5B, 0x8D, 0xEF),
    }
}

/// The accent used for *text* and thin marks: section headings, the navigation
/// rail, sparkline strokes and editor guide lines.
///
/// egui's default `hyperlink_color` is `#009bff`, which is 2.94:1 on white, so
/// every accent section heading on the Light page failed AA. Each value below
/// clears 4.5:1 against both the page background and the card surface of its
/// own theme.
fn accent_text(theme: AppTheme) -> Color32 {
    match theme {
        // 6.85:1 on #1e1e1e, 5.92:1 on #262a31.
        AppTheme::Dark => Color32::from_rgb(0x5A, 0xAA, 0xFF),
        // 5.67:1 on #3a3a3a, 4.80:1 on #41454c.
        AppTheme::Grey => Color32::from_rgb(0x8F, 0xB8, 0xFF),
        // 5.17:1 on #ffffff, 4.74:1 on #f4f5f7.
        AppTheme::Light => Color32::from_rgb(0x25, 0x63, 0xEB),
    }
}

/// Blend `accent` `amount` of the way into `surface`, then keep stepping
/// toward the surface until `foreground` clears the AA body-text threshold on
/// the result. Hover and pressed states are tints of the selected fill rather
/// than copies of it, which is what makes the interaction states separable;
/// the loop guarantees the label on them stays readable.
fn accent_tint(accent: Color32, surface: Color32, foreground: Color32, amount: f32) -> Color32 {
    let mut step = amount.clamp(0.0, 1.0);
    let mut tint = blend_color(accent, surface, step);
    while contrast_ratio(foreground, tint) < 4.5 && step < 1.0 {
        step = (step + 0.02).min(1.0);
        tint = blend_color(accent, surface, step);
    }
    tint
}

fn widget_border(theme: AppTheme, pal: &Palette) -> Color32 {
    match theme {
        // The print palette's pale rule is intentionally subtle. Interactive
        // controls need a 3:1 boundary against the desktop panel instead.
        AppTheme::Light => Color32::from_rgb(128, 128, 128),
        AppTheme::Dark | AppTheme::Grey => hex_to_color32(pal.border),
    }
}

fn input_fill(theme: AppTheme) -> Color32 {
    match theme {
        // Keep numeric fields visibly separate from the panel without turning
        // the dense engineering forms into a collection of bright cards.
        AppTheme::Dark => Color32::from_rgb(48, 53, 62),
        AppTheme::Grey => Color32::from_rgb(54, 59, 68),
        AppTheme::Light => Color32::from_rgb(255, 255, 255),
    }
}

fn input_border(theme: AppTheme) -> Color32 {
    match theme {
        AppTheme::Dark => Color32::from_rgb(158, 166, 179),
        AppTheme::Grey => Color32::from_rgb(202, 208, 218),
        AppTheme::Light => Color32::from_rgb(107, 114, 128),
    }
}

fn disabled_text_color(theme: AppTheme) -> Color32 {
    match theme {
        // Disabled controls remain deliberately subdued, but still meet the
        // same text contrast threshold as small explanatory copy.
        AppTheme::Dark => Color32::from_rgb(190, 198, 210),
        AppTheme::Grey => Color32::from_rgb(222, 226, 232),
        AppTheme::Light => Color32::from_rgb(72, 81, 96),
    }
}

fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    fn luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
    let (lighter, darker) = if luminance(a) >= luminance(b) {
        (luminance(a), luminance(b))
    } else {
        (luminance(b), luminance(a))
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Apply the selected theme palette to an `egui::Context`.
pub fn apply_theme(theme: AppTheme, ctx: &Context) {
    let pal = theme.palette();
    let is_dark = theme != AppTheme::Light;

    let mut visuals = if is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    let bg_color = hex_to_color32(pal.bg);
    let panel_bg = hex_to_color32(pal.panel);
    let text_color = hex_to_color32(pal.title);
    let border = widget_border(theme, pal);
    let field_fill = input_fill(theme);
    let field_border = input_border(theme);
    // egui also uses active.fg_stroke for RichText::strong everywhere.
    // Keep that foreground readable on ordinary surfaces and adapt the
    // highlight fill to it, rather than making all emphasized labels black.
    let hover_foreground = text_color;
    // One accent fill for the selected state, and two tints of it for hover
    // and for pressed / keyboard-focused / open, so the four states no longer
    // share a single saturated fill.
    let selected_fill = accent_fill(theme);
    let hover_fill = accent_tint(
        selected_fill,
        panel_bg,
        hover_foreground,
        HOVER_TINT_TOWARD_PANEL,
    );
    let pressed_fill = accent_tint(
        selected_fill,
        panel_bg,
        hover_foreground,
        PRESSED_TINT_TOWARD_PANEL,
    );

    visuals.panel_fill = panel_bg;
    visuals.window_fill = bg_color;
    visuals.extreme_bg_color = field_fill;
    visuals.override_text_color = Some(text_color);

    visuals.window_stroke = Stroke::new(1.0_f32, border);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    visuals.widgets.inactive.bg_fill = field_fill;
    visuals.widgets.inactive.weak_bg_fill = field_fill;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, field_border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, disabled_text_color(theme));
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, border);
    // An open menu-bar button is drawn from `widgets.open.weak_bg_fill`
    // (egui `menu::stationary_menu_impl`). That was left at the framework
    // default, which sits within ~1.1:1 of these panels, so an open menu had
    // no persistent indicator and the only visible fill tracked the pointer.
    visuals.widgets.open.weak_bg_fill = pressed_fill;
    visuals.widgets.open.bg_fill = pressed_fill;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, hover_foreground);
    // Menu bars intentionally remove egui's default hover stroke, so the
    // optional button fill is the visible hover affordance there. A tint of
    // the selected fill keeps that affordance while leaving the saturated
    // accent to mean "selected" and nothing else.
    visuals.widgets.hovered.weak_bg_fill = hover_fill;
    visuals.widgets.hovered.bg_fill = hover_fill;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, hover_foreground);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, hover_foreground);
    // Pressed and keyboard-focused controls resolve to `widgets.active`
    // (`Widgets::style`), so this is also the focus treatment: a stronger tint
    // plus the wider ring below.
    visuals.widgets.active.weak_bg_fill = pressed_fill;
    visuals.widgets.active.bg_fill = pressed_fill;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0_f32, hover_foreground);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, hover_foreground);
    visuals.window_rounding = Rounding::same(10.0);
    visuals.menu_rounding = Rounding::same(8.0);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.rounding = Rounding::same(6.0);
    }
    visuals.selection.bg_fill = selected_fill;
    visuals.selection.stroke = Stroke::new(1.0_f32, selection_foreground(selected_fill));
    // Section headings, the navigation rail and every accent mark read this.
    visuals.hyperlink_color = accent_text(theme);
    // Nested configuration nodes are already enclosed by cards and headings.
    // The default left rule makes those sections look like unfinished boxes.
    visuals.indent_has_left_vline = false;
    if is_dark {
        visuals.error_fg_color = Color32::from_rgb(255, 180, 171);
        visuals.warn_fg_color = Color32::from_rgb(255, 209, 102);
    } else {
        visuals.error_fg_color = Color32::from_rgb(180, 35, 24);
        visuals.warn_fg_color = Color32::from_rgb(122, 78, 0);
    }

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = vec2(12.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.button_padding = vec2(11.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    // Floating bars cover card strokes and the trailing part of input rows.
    // A solid bar reserves its own lane and leaves every card edge intact.
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.bar_outer_margin = 1.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.5));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(12.5));
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitted_label_colors(context: &Context) -> Vec<(String, Color32)> {
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.heading("Preset");
                    for label in ["Parameter values", "Console", "Timings"] {
                        ui.label(egui::RichText::new(label).strong());
                    }
                    egui::CollapsingHeader::new(egui::RichText::new("Cruise sweep").strong())
                        .show(ui, |_| {});
                    ui.scope(|ui| {
                        card_frame(ui).show(ui, |ui| {
                            ui.label(egui::RichText::new("Nested status").strong());
                        });
                    });
                    ui.add(
                        egui::ProgressBar::new(0.0)
                            .text(egui::RichText::new("Screening status").strong()),
                    );
                    ui.add(
                        egui::Button::new("Highlighted control")
                            .fill(ui.visuals().widgets.hovered.weak_bg_fill),
                    );
                    ui.add(
                        egui::Button::new(egui::RichText::new("Active control").strong())
                            .fill(ui.visuals().widgets.active.weak_bg_fill),
                    );
                });
            });
        });
        let mut colors = Vec::new();
        for shape in &output.shapes {
            collect_text_colors(&shape.shape, &mut colors);
        }
        colors
    }

    /// Walk a paint shape and record every emitted (text, colour) pair.
    fn collect_text_colors(shape: &egui::Shape, colors: &mut Vec<(String, Color32)>) {
        match shape {
            egui::Shape::Text(text) => {
                for section in &text.galley.job.sections {
                    let color = text.override_text_color.unwrap_or_else(|| {
                        if section.format.color == Color32::PLACEHOLDER {
                            text.fallback_color
                        } else {
                            section.format.color
                        }
                    });
                    colors.push((
                        text.galley.job.text[section.byte_range.clone()].to_owned(),
                        color,
                    ));
                }
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_colors(shape, colors);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn rendered_theme_labels_keep_contrast_including_strong_and_nested_text() {
        for theme in [AppTheme::Dark, AppTheme::Grey, AppTheme::Light] {
            let context = Context::default();
            apply_theme(theme, &context);
            let visuals = context.style().visuals.clone();
            let colors = emitted_label_colors(&context);
            for label in [
                "Preset",
                "Parameter values",
                "Console",
                "Timings",
                "Cruise sweep",
                "Nested status",
                "Screening status",
            ] {
                let color = colors
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("{theme:?}: missing rendered {label}"))
                    .1;
                for background in [
                    visuals.panel_fill,
                    visuals.window_fill,
                    visuals.extreme_bg_color,
                ] {
                    assert!(
                        contrast_ratio(color, background) >= 4.5,
                        "{theme:?}: rendered {label} {color:?} unreadable on {background:?}"
                    );
                }
            }
            // Normal and strong button text resolve differently in egui;
            // both emitted colors must remain readable on active/hover fills.
            for label in ["Highlighted control", "Active control"] {
                let color = colors.iter().find(|(text, _)| text == label).unwrap().1;
                for fill in [
                    visuals.widgets.hovered.weak_bg_fill,
                    visuals.widgets.active.weak_bg_fill,
                ] {
                    assert!(contrast_ratio(color, fill) >= 4.5);
                }
            }
        }
    }

    #[test]
    fn rendered_theme_regression_detects_previous_black_strong_foreground() {
        let context = Context::default();
        apply_theme(AppTheme::Grey, &context);
        // Reproduce the previous foreground in this isolated context only.
        context.style_mut(|style| {
            style.visuals.widgets.active.fg_stroke.color =
                selection_foreground(hex_to_color32(AppTheme::Grey.palette().accent));
        });
        let colors = emitted_label_colors(&context);
        let panel = context.style().visuals.panel_fill;
        let strong = colors
            .iter()
            .find(|(label, _)| label == "Parameter values")
            .unwrap()
            .1;
        let heading = colors
            .iter()
            .find(|(label, _)| label == "Preset")
            .unwrap()
            .1;
        assert!(
            contrast_ratio(strong, panel) < 4.5,
            "old strong-color bug must be observable"
        );
        assert!(
            contrast_ratio(heading, panel) >= 4.5,
            "ordinary heading takes override_text_color"
        );
    }

    /// Emit the labels of controls drawn in their *selected* state.
    ///
    /// The measurement has to come from the rendered galley: egui resolves a
    /// plain or strong label through `Visuals::override_text_color` before the
    /// selected widget's own `fg_stroke`, so reading `selection.stroke` alone
    /// would measure a colour the user never sees.
    fn emitted_selected_label_colors(context: &Context) -> Vec<(String, Color32)> {
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(selectable_button("Selected page", true));
                ui.add(selectable_button(
                    egui::RichText::new("Selected strong").strong(),
                    true,
                ));
                let _ = ui.selectable_label(true, "Selected row");
            });
        });
        let mut colors = Vec::new();
        for shape in &output.shapes {
            collect_text_colors(&shape.shape, &mut colors);
        }
        colors
    }

    #[test]
    fn a_selected_control_keeps_its_label_readable_on_the_accent_fill() {
        for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
            let context = Context::default();
            apply_theme(theme, &context);
            let fill = context.style().visuals.selection.bg_fill;
            let colors = emitted_selected_label_colors(&context);
            for label in ["Selected page", "Selected strong", "Selected row"] {
                let color = colors
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("{theme:?}: missing rendered {label}"))
                    .1;
                let ratio = contrast_ratio(color, fill);
                assert!(
                    ratio >= 4.5,
                    "{theme:?}: selected {label} {color:?} on {fill:?} is {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn accent_heading_text_meets_aa_on_every_theme_surface() {
        // Regression: egui's default `#009bff` hyperlink colour measured
        // 2.94:1 on the Light page, so every accent section heading failed.
        for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
            let context = Context::default();
            apply_theme(theme, &context);
            let visuals = context.style().visuals.clone();
            for background in [visuals.panel_fill, visuals.window_fill] {
                let ratio = contrast_ratio(visuals.hyperlink_color, background);
                assert!(
                    ratio >= 4.5,
                    "{theme:?}: accent text {:?} on {background:?} is {ratio:.2}:1",
                    visuals.hyperlink_color
                );
            }
        }
    }

    #[test]
    fn selected_hovered_and_open_controls_are_three_distinct_fills() {
        for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
            let context = Context::default();
            apply_theme(theme, &context);
            let visuals = context.style().visuals.clone();
            let panel = visuals.panel_fill;
            let selected = visuals.selection.bg_fill;
            let hovered = visuals.widgets.hovered.weak_bg_fill;
            let open = visuals.widgets.open.weak_bg_fill;
            assert!(
                contrast_ratio(selected, hovered) >= 1.4,
                "{theme:?}: selected {selected:?} and hovered {hovered:?} are the same fill"
            );
            assert!(
                contrast_ratio(open, hovered) >= 1.1,
                "{theme:?}: an open menu must not look hovered"
            );
            assert!(contrast_ratio(hovered, panel) >= 1.15, "{theme:?}: hover tint invisible");
            assert!(contrast_ratio(open, panel) >= 1.3, "{theme:?}: open tint invisible");
            assert!(contrast_ratio(selected, panel) >= 1.7, "{theme:?}: selected fill invisible");
            // The selected fill leans on its stroke for the 1.4.11 boundary.
            assert!(contrast_ratio(visuals.selection.stroke.color, panel) >= 3.0);
            // Focus stays separable from hover by its wider ring.
            assert!(visuals.widgets.active.bg_stroke.width > visuals.widgets.hovered.bg_stroke.width);
        }
    }

    #[test]
    fn interactive_dark_themes_select_accessible_figure_palettes() {
        assert_eq!(AppTheme::Dark.figure_theme_name(), "dark-accessible");
        assert_eq!(AppTheme::Grey.figure_theme_name(), "grey-accessible");
        assert_eq!(AppTheme::Light.figure_theme_name(), "light");
    }

    #[test]
    fn interactive_theme_semantics_meet_text_and_boundary_contrast() {
        for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
            let context = Context::default();
            apply_theme(theme, &context);
            let visuals = &context.style().visuals;
            let panel = visuals.panel_fill;
            assert!(
                contrast_ratio(visuals.selection.stroke.color, visuals.selection.bg_fill) >= 4.5
            );
            assert!(contrast_ratio(visuals.error_fg_color, panel) >= 4.5);
            assert!(contrast_ratio(visuals.warn_fg_color, panel) >= 4.5);
            assert!(contrast_ratio(success_color(visuals), panel) >= 4.5);
            assert!(contrast_ratio(visuals.widgets.inactive.fg_stroke.color, panel) >= 4.5);
            assert!(
                contrast_ratio(
                    visuals.widgets.hovered.fg_stroke.color,
                    visuals.widgets.hovered.weak_bg_fill
                ) >= 4.5,
                "hovered controls need readable text on their highlight"
            );
            assert!(contrast_ratio(visuals.widgets.inactive.bg_stroke.color, panel) >= 3.0);
            assert!(
                contrast_ratio(
                    visuals.widgets.inactive.bg_stroke.color,
                    visuals.extreme_bg_color
                ) >= 3.0
            );
            assert_eq!(
                visuals.widgets.inactive.bg_fill, visuals.extreme_bg_color,
                "interactive fields should have one consistent fill"
            );
            assert!(contrast_ratio(visuals.text_color(), panel) >= 4.5);
        }
    }
}
