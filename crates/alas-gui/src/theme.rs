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
    let accent = hex_to_color32(pal.accent);
    let border = widget_border(theme, pal);
    let field_fill = input_fill(theme);
    let field_border = input_border(theme);

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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, accent);
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
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = Stroke::new(1.0_f32, selection_foreground(accent));
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
