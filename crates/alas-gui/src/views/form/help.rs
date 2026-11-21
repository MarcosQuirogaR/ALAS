// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field help tooltips: at most two lines, with a "Docs" link to the full text
//! when the field has one. The help strings themselves live in `alas-config`:
//! `help` is the short summary a tooltip shows and `long_help`, when present,
//! the full explanation; this only decides how much of each is shown where.

use egui::Response;

use crate::views::tr;

/// The longest help a tooltip shows whole: about two lines of the tooltip.
pub(crate) const HELP_MAX_CHARS: usize = 180;

/// `help` clamped to [`HELP_MAX_CHARS`] at a word boundary, with an ellipsis,
/// and whether anything was cut.
pub(crate) fn clamp_help(help: &str) -> (String, bool) {
    let help = help.trim();
    if help.chars().count() <= HELP_MAX_CHARS {
        return (help.to_owned(), false);
    }
    let cut: String = help.chars().take(HELP_MAX_CHARS).collect();
    let cut = match cut.rfind(char::is_whitespace) {
        Some(index) if index > HELP_MAX_CHARS / 2 => &cut[..index],
        _ => cut.as_str(),
    };
    (
        format!(
            "{}\u{2026}",
            cut.trim_end_matches([' ', ',', ';', ':', '.'])
        ),
        true,
    )
}

/// `response` with `hovered` decided by where the pointer is rather than by
/// egui's hit test. egui hovers only the widgets at or above the topmost
/// interactive one under the pointer, in the order they were registered in the
/// previous pass, so a cell-wide overlay that is registered below a value box
/// (or any later click or drag widget over the cell) never counts as hovered
/// and its tooltip never opens. The cell owns its help whenever the pointer is
/// inside it on this layer and no button is held (a drag or a click is not a
/// hover).
pub(crate) fn hovered_by_pointer(mut response: Response) -> Response {
    if !response.hovered
        && response.enabled
        && response
            .ctx
            .rect_contains_pointer(response.layer_id, response.interact_rect)
        && !response.ctx.input(|input| input.pointer.any_down())
    {
        response.hovered = true;
    }
    response
}

/// Attach the help tooltip to `response`. `long_help` is the field's full
/// explanation, or empty when `help` already says everything: the tooltip then
/// ends in a "Docs" link that opens the online documentation in the system
/// browser. The tooltip never repeats the full text in the app.
pub(crate) fn help_on_hover(
    response: Response,
    _title: &str,
    help: &str,
    _long_help: &str,
) -> Response {
    if help.trim().is_empty() {
        return response;
    }
    let (short, _) = clamp_help(help);
    response.on_hover_ui(|ui| {
        ui.set_min_width(HELP_MIN_WIDTH.min(ui.ctx().screen_rect().width() * 0.5));
        crate::notation::label(ui, &short);
        if ui.link(format!("{} \u{2197}", tr("Docs"))).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DOCUMENTATION_URL));
        }
    })
}

/// The narrowest a field-help tooltip gets, so a short help line never wraps
/// to a word per row.
const HELP_MIN_WIDTH: f32 = 260.0;

/// The online documentation, opened by every "Docs" link.
pub(crate) const DOCUMENTATION_URL: &str = "https://alas.uvigo.es/docs/";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_help_is_shown_whole() {
        let (text, clamped) = clamp_help("Lift-dependent viscous drag fraction.");
        assert_eq!(text, "Lift-dependent viscous drag fraction.");
        assert!(!clamped);
    }

    #[test]
    fn long_help_is_cut_at_a_word_with_an_ellipsis() {
        let long = "word ".repeat(100);
        let (text, clamped) = clamp_help(&long);
        assert!(clamped);
        assert!(text.chars().count() <= HELP_MAX_CHARS + 1);
        assert!(text.ends_with('\u{2026}'));
        assert!(!text.contains("wor\u{2026}"));
    }
}
