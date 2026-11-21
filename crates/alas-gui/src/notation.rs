// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Engineering notation in egui text: `S_ref` draws a subscript, `m^2` a
//! superscript, `c/4` reads "¼-chord". The parsing lives in
//! `alas_units::notation`, shared with the SVG figures; this module only turns
//! the runs into an egui `LayoutJob`.

use alas_units::notation::{parse, Placement};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, FontId, TextStyle, Ui, WidgetText};

/// Size of a sub- or superscript run relative to the surrounding text.
const SCRIPT_SCALE: f32 = 0.72;

/// A layout job for `text` in `font` and `color`, with real sub- and
/// superscript runs.
#[must_use]
pub(crate) fn job(text: &str, font: &FontId, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in parse(text) {
        let (size, valign) = match span.placement {
            Placement::Normal => (font.size, Align::BOTTOM),
            Placement::Sub => (font.size * SCRIPT_SCALE, Align::BOTTOM),
            Placement::Sup => (font.size * SCRIPT_SCALE, Align::TOP),
        };
        job.append(
            &span.text,
            0.0,
            TextFormat {
                font_id: FontId::new(size, font.family.clone()),
                color,
                valign,
                line_height: (span.placement == Placement::Sub)
                    .then_some(font.size * alas_units::notation::SUB_LINE_HEIGHT_EM),
                ..TextFormat::default()
            },
        );
    }
    job
}

/// `text` as widget text in the body style of `ui`, with sub- and
/// superscripts drawn. Text without notation stays plain text, so the common
/// case costs nothing.
#[must_use]
pub(crate) fn rich(ui: &Ui, text: &str) -> WidgetText {
    rich_in(ui, text, &TextStyle::Body, None)
}

/// As [`rich`], in the given text style and, when given, colour.
#[must_use]
pub(crate) fn rich_in(
    ui: &Ui,
    text: &str,
    style: &TextStyle,
    color: Option<Color32>,
) -> WidgetText {
    if !alas_units::notation::has_notation(text) {
        return WidgetText::from(text);
    }
    let font = style.resolve(ui.style());
    let color = color.unwrap_or_else(|| ui.visuals().text_color());
    WidgetText::from(job(text, &font, color))
}

/// A styled `RichText` (weak, small, coloured, strong...) with notation
/// drawn: the text keeps every property of its single run and only the
/// sub- and superscript runs change size and baseline. Text without notation
/// stays a plain `RichText`, so the common case costs nothing.
#[must_use]
pub(crate) fn styled(ui: &Ui, text: egui::RichText) -> WidgetText {
    if !alas_units::notation::has_notation(text.text()) {
        return WidgetText::from(text);
    }
    let job = WidgetText::from(text).into_layout_job(
        ui.style(),
        egui::FontSelection::Default,
        Align::Center,
    );
    let Some(base) = job.sections.first().map(|section| section.format.clone()) else {
        return WidgetText::from(job);
    };
    let mut out = LayoutJob::default();
    for span in parse(&job.text) {
        let mut format = base.clone();
        match span.placement {
            Placement::Normal => {}
            Placement::Sub => {
                format.line_height =
                    Some(format.font_id.size * alas_units::notation::SUB_LINE_HEIGHT_EM);
                format.font_id.size *= SCRIPT_SCALE;
                format.valign = Align::BOTTOM;
            }
            Placement::Sup => {
                format.font_id.size *= SCRIPT_SCALE;
                format.valign = Align::TOP;
            }
        }
        out.append(&span.text, 0.0, format);
    }
    WidgetText::from(out)
}

/// Any widget text with notation drawn: plain and rich text are converted,
/// laid-out jobs and galleys are assumed to be styled already.
#[must_use]
pub(crate) fn widget(ui: &Ui, text: WidgetText) -> WidgetText {
    match text {
        WidgetText::RichText(rich) => styled(ui, rich),
        other => other,
    }
}

/// Hover text for `response` with notation drawn. Text without notation is
/// the plain egui tooltip.
pub(crate) fn hover(response: egui::Response, text: impl AsRef<str>) -> egui::Response {
    let text = text.as_ref();
    if !alas_units::notation::has_notation(text) {
        return response.on_hover_text(text.to_owned());
    }
    response.on_hover_ui(|ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        label(ui, text);
    })
}

/// Add a label for `text` with notation drawn.
pub(crate) fn label(ui: &mut Ui, text: &str) -> egui::Response {
    let widget = rich(ui, text);
    ui.label(widget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscript_becomes_a_smaller_lowered_run() {
        let font = FontId::proportional(14.0);
        let job = job("S_ref 5 m^2", &font, Color32::WHITE);
        let sections: Vec<_> = job
            .sections
            .iter()
            .map(|section| {
                (
                    job.text[section.byte_range.clone()].to_owned(),
                    section.format.font_id.size,
                    section.format.valign,
                )
            })
            .collect();
        assert_eq!(sections[0], ("S".to_owned(), 14.0, Align::BOTTOM));
        assert_eq!(sections[1].0, "ref");
        assert!(sections[1].1 < 14.0 && sections[1].2 == Align::BOTTOM);
        assert_eq!(sections[3].0, "2");
        assert_eq!(sections[3].2, Align::TOP);
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::test_raster::{render, write_in};

    /// Every notation form drawn in the dark theme, for a look.
    #[test]
    fn notation_renders_for_a_look() {
        let samples = [
            "S_ref 525.2 m^2",
            "L_fus 76.72 m",
            "c/4 sweep 29.7 deg",
            "Landing distance factor (k_land)",
            "VMC / VS_TO",
            "Bending stiffness EI [GN.m^2]",
            "CD = CD0 + k*CL^2, kg/m^3, W_PYLON = F_PYL (W_ENG + W_NAC)^0.736",
            "break_chord_m stays raw",
        ];
        let shot = render(520.0, 260.0, 2, 0, |ui| {
            for sample in samples {
                label(ui, sample);
            }
            ui.label(styled_sample(ui));
        });
        write_in(&crate::test_raster::render_dir(), "c3-notation.png", &shot);
        assert!(shot.texts.iter().any(|(text, _)| text.contains("ref")));
    }

    fn styled_sample(ui: &Ui) -> WidgetText {
        styled(
            ui,
            egui::RichText::new("Weak small x_cg, V_dive, m^2")
                .weak()
                .small(),
        )
    }
}
