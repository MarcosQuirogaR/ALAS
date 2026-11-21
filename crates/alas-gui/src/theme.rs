// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Theme palette adapter mapping ALAS design system palettes to `egui::Visuals`.

use alas_fonts::{
    MATH_FONT_BYTES, MATH_FONT_KEY, MONOSPACE_FONT_BYTES, MONOSPACE_FONT_KEY,
    PROPORTIONAL_FONT_BYTES, PROPORTIONAL_FONT_KEY,
};
use alas_report::theme::{get_palette, Palette};
use egui::{
    vec2, Align2, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    Response, Rounding, Sense, Stroke, TextStyle, Ui, Visuals,
};

pub use crate::lock_icon::lock_icon;

mod card;
pub mod figure;
pub mod mark;
pub mod space;
pub use card::{
    card_content_width, card_depth, card_frame, card_title, column_padding, header_label,
    header_row, record_column_end, section_title, show_header, title_at_depth, CARD_INNER_MARGIN_X,
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

/// One row of a selectable list (the add-ons list): as wide as the list and at
/// least `height` tall, its text left-aligned and centred in height whether or
/// not the row is selected, so the entries of a list line up at one edge.
pub fn list_row(
    ui: &mut Ui,
    selected: bool,
    text: impl Into<egui::WidgetText>,
    height: f32,
) -> Response {
    // `left_to_right` centres a widget's text along the row by default.
    let row = egui::Layout::left_to_right(egui::Align::Center)
        .with_main_align(egui::Align::Min)
        .with_main_justify(true)
        .with_cross_justify(true);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), row, |ui| {
        ui.add(egui::SelectableLabel::new(selected, text))
    })
    .inner
}

/// The status line of a background job (`text`) that has been running since
/// `since`: from its first second on it counts the seconds, so a slow job
/// reads as work in progress, not as a label left behind; the page replaces
/// the line with the job's result as soon as the job ends. Without a running
/// job (`since` is `None`) it is `text` alone.
pub fn in_progress(ctx: &Context, text: &str, since: Option<std::time::Instant>) -> String {
    let Some(since) = since else {
        return text.to_owned();
    };
    ctx.request_repaint_after(std::time::Duration::from_secs(1));
    match since.elapsed().as_secs() {
        0 => text.to_owned(),
        seconds => format!("{text} ({seconds} s)"),
    }
}

/// The destructive-action red shared by [`danger_button`] and the dialogs.
const DANGER_RGB: (u8, u8, u8) = (229, 83, 75);

/// A subtle red button for destructive actions (discard, delete, exit
/// without saving): a faint red fill and outline at rest, which strengthen on
/// hover. Use it only when the action loses something; ordinary actions keep
/// the plain button.
pub fn danger_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    let (r, g, b) = DANGER_RGB;
    let text_color = Color32::from_rgb(0xff, 0x9a, 0x93);
    let strong = Color32::from_rgb(r, g, b);
    let button = egui::Button::new(egui::RichText::new(text.into().text()).color(text_color))
        .rounding(Rounding::same(SELECTABLE_CONTROL_RADIUS));
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        let states = [
            (
                &mut widgets.inactive,
                0.10_f32,
                Color32::from_rgba_unmultiplied(r, g, b, 115),
            ),
            (&mut widgets.hovered, 0.22, strong),
            (&mut widgets.active, 0.30, strong),
        ];
        for (state, fill_alpha, stroke_color) in states {
            let fill = Color32::from_rgba_unmultiplied(r, g, b, (fill_alpha * 255.0) as u8);
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0_f32, stroke_color);
        }
        ui.add(button)
    })
    .inner
}

/// The highlighted main action of a dialog: the selection fill, rounded like
/// every other control.
pub fn primary_button(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    selectable_button(text, true)
}

/// Paint the push-pin of the navigation toggle centred on `center`: an
/// outline when the navigation floats, a filled pin when it is pinned.
pub fn paint_pin_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32, filled: bool) {
    // A push-pin drawn upright (head up, needle down) and turned 45 degrees
    // clockwise, so the head sits upper right and the needle points lower left.
    let (sin, cos) = std::f32::consts::FRAC_PI_4.sin_cos();
    let at = |x: f32, y: f32| center + vec2(x * cos - y * sin, x * sin + y * cos) + vec2(-0.5, 0.5);
    let stroke = Stroke::new(1.5_f32, color);
    let fill = if filled { color } else { Color32::TRANSPARENT };
    let head = vec![at(-2.6, -9.0), at(2.6, -9.0), at(2.6, -3.0), at(-2.6, -3.0)];
    painter.add(egui::Shape::convex_polygon(head, fill, stroke));
    let flange = vec![at(-5.5, -3.0), at(5.5, -3.0), at(5.5, -1.0), at(-5.5, -1.0)];
    painter.add(egui::Shape::convex_polygon(flange, fill, stroke));
    painter.line_segment([at(0.0, -1.0), at(0.0, 9.0)], stroke);
}

/// The one label style of a parameter, shared by the schema forms and the
/// bespoke Aircraft controls: body size, regular weight, normal text colour
/// while the value can be edited and the weak colour only when it cannot, so
/// the difference tells the user something.
pub fn field_label(text: &str, editable: bool) -> egui::RichText {
    let label = egui::RichText::new(text);
    if editable {
        label
    } else {
        label.weak()
    }
}

/// The interaction states of a field (a value box, a choice, a framed
/// checkbox cell and the controls on its caption): the field fill in every
/// state, the hover ring while the pointer is over it, the wider ring while
/// it is pressed, open or has keyboard focus. egui fills a hovered or focused
/// button-like widget with the accent tint, so the field that a resting
/// pointer happened to be over after a scroll or a click read as selected;
/// a field now shows only its ring, which leaves with the pointer or focus.
pub fn field_states(visuals: &mut Visuals) {
    let fill = visuals.widgets.inactive.weak_bg_fill;
    let ring = visuals.widgets.active.bg_stroke;
    let widgets = &mut visuals.widgets;
    for state in [&mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
        state.weak_bg_fill = fill;
        state.bg_fill = fill;
    }
    widgets.open.bg_stroke = ring;
}

/// Outer height of a button: the text plus the button padding, never below
/// the interaction height. A text input beside a button takes this height so a
/// row of controls shares top and bottom edges.
pub fn control_height(ui: &Ui) -> f32 {
    let spacing = ui.spacing();
    spacing
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Button) + 2.0 * spacing.button_padding.y)
}

/// Return the attached surface for the compact navigation rail and its hover
/// expansion. The fill deliberately interpolates from the accent rail into
/// the navigation panel so opening it reads as one continuous surface.
pub fn navigation_overlay_frame(ui: &Ui, expansion: f32) -> Frame {
    let expansion = expansion.clamp(0.0, 1.0);
    let padding = 14.0 * expansion;
    Frame::group(ui.style())
        // At rest the rail is a quiet 8 pt strip: the accent only tints the
        // panel colour (60 %), because a saturated accent at the window edge
        // reads as a rendering defect on dark pages. It reaches the plain
        // panel fill as the navigation opens.
        .fill(blend_color(
            ui.visuals().hyperlink_color,
            ui.visuals().panel_fill,
            0.6 + 0.4 * expansion,
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

mod colour;
use colour::*;

/// Apply the selected theme palette to an `egui::Context`.
pub fn apply_theme(theme: AppTheme, ctx: &Context) {
    install_engineering_fonts(ctx);
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
    // egui fades every new popup (selector lists, menus, tooltips) in over
    // `animation_time`, multiplying the opacity of its frame and its text. The
    // form underneath then shows through the list for the first frames, which
    // on a page that takes a while to draw is what the user sees. Popups open
    // fully opaque; widgets that animate pass their own duration.
    style.animation_time = 0.0;
    style.spacing.item_spacing = vec2(12.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.button_padding = vec2(11.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    // Room between a checkbox or radio glyph and its label.
    style.spacing.icon_spacing = 8.0;
    // Plain hover tooltips wrap at a readable measure, never at a word.
    style.spacing.tooltip_width = 420.0;
    // A field hint shows after a short rest of the pointer, not half a second.
    style.interaction.tooltip_delay = 0.2;
    // Floating bars cover card strokes and the trailing part of input rows.
    // A solid bar reserves its own lane and leaves every card edge intact.
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.bar_outer_margin = 1.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(space::BODY_TEXT));
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

/// Configure every native text family with the bundled engineering typefaces.
///
/// `egui` resolves the family list per glyph in order. The Noto text faces are
/// therefore the ordinary application typography, while Noto Sans Math covers
/// Greek extensions, SI notation, operators and letter subscripts absent from
/// the framework defaults. Emoji remains last so existing icon-like labels
/// retain egui's color glyph fallback.
fn install_engineering_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        PROPORTIONAL_FONT_KEY.to_owned(),
        FontData::from_static(PROPORTIONAL_FONT_BYTES),
    );
    fonts.font_data.insert(
        MONOSPACE_FONT_KEY.to_owned(),
        FontData::from_static(MONOSPACE_FONT_BYTES),
    );
    fonts.font_data.insert(
        MATH_FONT_KEY.to_owned(),
        FontData::from_static(MATH_FONT_BYTES),
    );

    fonts.families.insert(
        FontFamily::Proportional,
        vec![
            PROPORTIONAL_FONT_KEY.to_owned(),
            MATH_FONT_KEY.to_owned(),
            "NotoEmoji-Regular".to_owned(),
            "emoji-icon-font".to_owned(),
        ],
    );
    fonts.families.insert(
        FontFamily::Monospace,
        vec![
            MONOSPACE_FONT_KEY.to_owned(),
            MATH_FONT_KEY.to_owned(),
            PROPORTIONAL_FONT_KEY.to_owned(),
            "NotoEmoji-Regular".to_owned(),
            "emoji-icon-font".to_owned(),
        ],
    );
    ctx.set_fonts(fonts);
}

/// Style `ui` so selectable buttons added next form one segmented control:
/// every segment (selected or not) has a filled, outlined body and the gap
/// between segments is tight.
pub fn segmented_style(ui: &mut Ui) {
    let visuals = ui.visuals().clone();
    let style = ui.style_mut();
    style.spacing.item_spacing.x = 2.0;
    let fill = visuals.faint_bg_color;
    let stroke = visuals.widgets.noninteractive.bg_stroke;
    style.visuals.widgets.inactive.weak_bg_fill = fill;
    style.visuals.widgets.inactive.bg_fill = fill;
    style.visuals.widgets.inactive.bg_stroke = stroke;
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINEERING_GLYPH_CORPUS: &str = concat!(
        // Greek alphabet, commonly used alternate forms, and accented Greek.
        "\u{0391}\u{0392}\u{0393}\u{0394}\u{0395}\u{0396}\u{0397}\u{0398}\u{0399}\u{039A}\u{039B}\u{039C}\u{039D}\u{039E}\u{039F}\u{03A0}\u{03A1}\u{03A3}\u{03A4}\u{03A5}\u{03A6}\u{03A7}\u{03A8}\u{03A9}",
        "\u{03B1}\u{03B2}\u{03B3}\u{03B4}\u{03B5}\u{03B6}\u{03B7}\u{03B8}\u{03B9}\u{03BA}\u{03BB}\u{03BC}\u{03BD}\u{03BE}\u{03BF}\u{03C0}\u{03C1}\u{03C2}\u{03C3}\u{03C4}\u{03C5}\u{03C6}\u{03C7}\u{03C8}\u{03C9}",
        "\u{03D1}\u{03D5}\u{03D6}\u{03F0}\u{03F1}\u{03F5}\u{03AC}\u{03AD}\u{03AE}\u{03AF}\u{03CC}\u{03CD}\u{03CE}",
        // Actual ALAS engineering labels, including the Latin dotted m.
        "\u{1E41}\u{2070}\u{00B9}\u{00B2}\u{00B3}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}\u{207A}\u{207B}\u{207C}\u{207D}\u{207E}\u{207F}",
        "\u{2080}\u{2081}\u{2082}\u{2083}\u{2084}\u{2085}\u{2086}\u{2087}\u{2088}\u{2089}\u{2090}\u{2091}\u{2092}\u{2093}\u{2094}\u{2095}\u{2096}\u{2097}\u{2098}\u{2099}\u{209A}\u{209B}\u{209C}",
        // SI typography, calculus, relations, arrows, delimiters, and common
        // mathematical-alphabet letters that should never resolve to a box.
        "\u{00B0}\u{00B1}\u{00B7}\u{00D7}\u{00F7}\u{2013}\u{2014}\u{2032}\u{2033}\u{2190}\u{2191}\u{2192}\u{2193}\u{2194}\u{21A6}\u{21D0}\u{21D2}\u{21D4}",
        "\u{2200}\u{2202}\u{2203}\u{2205}\u{2206}\u{2207}\u{2208}\u{2209}\u{220F}\u{2211}\u{2212}\u{2213}\u{2217}\u{2218}\u{221A}\u{221D}\u{221E}\u{2220}\u{2227}\u{2228}\u{2229}\u{222B}\u{2248}\u{2260}\u{2261}\u{2264}\u{2265}\u{2282}\u{2286}\u{2295}\u{2297}\u{22C5}\u{2308}\u{2309}\u{230A}\u{230B}\u{27E8}\u{27E9}",
        "\u{1D6A8}\u{1D6C2}\u{1D6E2}\u{1D6FC}\u{1D70E}\u{1D722}\u{1D734}\u{1D74E}\u{1D760}\u{1D7CA}\u{1D400}\u{1D41A}\u{1D434}\u{1D44E}\u{1D468}\u{1D482}\u{1D7D8}\u{1D7E2}"
    );

    #[test]
    fn bundled_font_stacks_cover_greek_and_engineering_notation() {
        for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::Grey] {
            let context = Context::default();
            apply_theme(theme, &context);
            let _ = context.run(egui::RawInput::default(), |_| {});
            for style in [TextStyle::Body, TextStyle::Monospace] {
                let font = style.resolve(&context.style());
                assert!(
                    context.fonts(|fonts| fonts.has_glyphs(&font, ENGINEERING_GLYPH_CORPUS)),
                    "{theme:?}: {style:?} stack misses an engineering glyph"
                );
            }
        }
    }

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
            assert!(
                contrast_ratio(hovered, panel) >= 1.15,
                "{theme:?}: hover tint invisible"
            );
            assert!(
                contrast_ratio(open, panel) >= 1.3,
                "{theme:?}: open tint invisible"
            );
            assert!(
                contrast_ratio(selected, panel) >= 1.7,
                "{theme:?}: selected fill invisible"
            );
            // The selected fill leans on its stroke for the 1.4.11 boundary.
            assert!(contrast_ratio(visuals.selection.stroke.color, panel) >= 3.0);
            // Focus stays separable from hover by its wider ring.
            assert!(
                visuals.widgets.active.bg_stroke.width > visuals.widgets.hovered.bg_stroke.width
            );
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
