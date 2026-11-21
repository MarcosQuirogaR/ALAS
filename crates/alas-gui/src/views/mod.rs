// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! View components for every navigable page and shell panel.

pub mod aircraft_start;
pub mod airport_window;
pub mod analyses_view;
pub mod cfd_view;
pub mod control_bar;
pub mod design_space_view;
pub(crate) mod edit_draft;
pub mod external_tool_catalog;
pub mod files_page;
pub(crate) mod files_thumbnail;
pub mod flow_pages;
pub(crate) mod focus_card;
pub mod form;
pub mod form_page;
mod inputs_baseline;
pub(crate) mod inputs_custom;
pub(crate) mod inputs_mtow;
#[cfg(test)]
mod inputs_mtow_tests;
pub(crate) mod inputs_selectors;
pub mod inputs_view;
mod label_fixups;
pub(crate) mod mission_phase_focus;
pub(crate) mod mission_profile_inputs;
pub(crate) mod mission_profile_preview;
pub(crate) mod mission_stage_editor;
pub mod notices;
pub mod overlays;
pub(crate) mod payload_editor;
pub(crate) mod plot_data_menu;
pub(crate) mod plot_data_sources;
pub mod preview_dock;
#[cfg(test)]
mod r36_layout_tests;
#[cfg(test)]
mod r39_layout_tests;
pub(crate) mod result_3d;
pub mod results_view;
pub(crate) mod route_override;
pub mod run_log;
pub mod screening_view;
pub(crate) mod screening_window;
pub mod search_overlay;
pub mod tool_intro;
pub mod tools_view;
pub mod tour_data;
pub mod uav_view;
pub mod wing_analysis_view;

pub use analyses_view::show_analyses_view;
pub use control_bar::show_control_bar;
pub use design_space_view::show_design_space_view;
pub use form_page::show_form_page;
pub use inputs_view::show_inputs_view;
pub use preview_dock::{page_shows_preview_dock, show_preview_dock};
pub use results_view::show_results_view;
pub use run_log::show_run_log;
pub use screening_view::show_screening_view;
pub use tools_view::show_tools_view;
pub use uav_view::show_uav_view;

/// The text for the active language. A short English label-like text is put in
/// sentence case, so a source literal written in Title Case reads like the rest.
pub(crate) fn tr(text: &str) -> String {
    tr_label(text)
}

/// A UI label: translated, and in English put in sentence case. Sentences,
/// tooltips, templates and every Spanish string keep the catalogue's wording.
pub(crate) fn tr_label(text: &str) -> String {
    let translated = alas_i18n::t(Some(text), None);
    if matches!(translated, std::borrow::Cow::Borrowed(_)) && is_label(text) {
        label_case(text)
    } else {
        translated.into_owned()
    }
}

/// Whether `text` reads as a label rather than a sentence or a template.
fn is_label(text: &str) -> bool {
    let text = text.trim_end_matches("...");
    let words = text.split_whitespace().count();
    (1..=6).contains(&words) && !text.contains(['.', ',', ';', '?', '!', '{', '}', '\n'])
}

pub(crate) fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    alas_i18n::fill_fields(&tr(template), fields)
}

/// Capitalised words a label keeps capitalised: names of people, programs,
/// languages and units.
const PROPER_WORDS: &[&str] = &[
    "Mach",
    "Reynolds",
    "Breguet",
    "Raymer",
    "Torenbeek",
    "Roskam",
    "Nastran",
    "Patran",
    "Kelvin",
    "Celsius",
    "Newton",
    "Oswald",
    "Goethert",
    "Prandtl",
    "Glauert",
    "Korn",
    "English",
    "Spanish",
    "Espa\u{f1}ol",
    "Windows",
    "Explorer",
    "Airbus",
    "Boeing",
    "Embraer",
    "Bombardier",
    "Ctrl",
    "Shift",
    "Alt",
    "Enter",
    "Escape",
    "Docs",
    "I",
    "Schrenk",
    "Hoerner",
    "Lanchester",
    "Sears",
    "Haack",
    "Whitcomb",
    "Weissinger",
    "Jones",
    "Harris",
    "Datcom",
    "Sandbox",
    "Lock",
    "Jenkinson",
    "Helmbold",
    "Multhopp",
    "Munk",
    "Diederich",
    "Gothert",
    "Spence",
    "Hess",
    "Smith",
    "Selig",
    "Theodorsen",
    "Sutherland",
    "Newmark",
    "Euler",
    "Python",
    "Jacobi",
];

/// English sentence case for a label: the first letter is capitalised and
/// every later word written as a plain capitalised word ("Aircraft") is put in
/// lower case. Acronyms, mixed-case names (OpenVSP), words with digits or
/// symbols, and [`PROPER_WORDS`] are untouched. Each part of a hyphenated word
/// is treated as a word ("Fixed-Wing" reads "Fixed-wing").
pub(crate) fn label_case(text: &str) -> String {
    fn plain_capitalised(word: &str) -> bool {
        let mut chars = word.chars();
        chars.next().is_some_and(char::is_uppercase)
            && chars.clone().count() > 0
            && chars.all(char::is_lowercase)
            && !PROPER_WORDS.contains(&word)
    }
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for (index, word) in text.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        for (part_index, part) in word.split('-').enumerate() {
            if part_index > 0 {
                out.push('-');
            }
            if first && part.chars().any(char::is_alphabetic) {
                let mut chars = part.chars();
                if let Some(initial) = chars.next() {
                    out.extend(initial.to_uppercase());
                    out.push_str(chars.as_str());
                }
                first = false;
            } else if plain_capitalised(part) {
                out.push_str(&part.to_lowercase());
            } else {
                out.push_str(part);
            }
        }
    }
    out
}

/// Schema labels the form words differently from the schema (the schema, its goldens and the
/// catalogue key stay as they are): the requirement value, and the mass the MTOW card is titled by.
const RELABELED: &[(&str, &str)] = &[
    ("Cruise altitude", "Design cruise altitude"),
    ("Target passenger share [%]", "Target passenger share (%)"),
    ("Time limit [s]", "Time limit"),
    (
        "MTOW threshold for body (centreline) main gear",
        "MTOW threshold for body main gear",
    ),
    ("Max take-off weight (MTOW)", "Maximum take-off mass (MTOW)"),
    (
        "Maximum cruise CL (stall guard)",
        "Maximum cruise C_L (stall guard)",
    ),
];

/// A schema field label for the active language: sentence case in English.
pub(crate) fn form_label(label: &'static str, lang: Option<&str>) -> String {
    let label = RELABELED
        .iter()
        .find_map(|(raw, shown)| (*raw == label).then_some(*shown))
        .unwrap_or(label);
    let translated = alas_i18n::t(Some(label), lang).into_owned();
    let shown = if translated == label && is_label(label) {
        label_case(&label_fixups::english_label(&translated))
    } else {
        translated
    };
    label_fixups::without_symbol_subscripts(&shown)
}

#[cfg(test)]
mod label_case_tests {
    use super::label_case;

    #[test]
    fn capitalises_the_first_word_and_lowers_the_rest() {
        assert_eq!(
            label_case("take-off distance available"),
            "Take-off distance available"
        );
        assert_eq!(label_case("Reference Aircraft"), "Reference aircraft");
        assert_eq!(label_case("No Results Yet"), "No results yet");
        assert_eq!(label_case("Fixed-Wing UAV"), "Fixed-wing UAV");
        assert_eq!(label_case("Load & Trim Sheet"), "Load & trim sheet");
    }

    #[test]
    fn leaves_acronyms_names_digits_and_symbols_alone() {
        assert_eq!(label_case("MTOW sizing mode"), "MTOW sizing mode");
        assert_eq!(label_case("SOL 101 Static"), "SOL 101 static");
        assert_eq!(label_case("cruise L/D ratio"), "Cruise L/D ratio");
        assert_eq!(label_case("Cruise Mach Number"), "Cruise Mach number");
        assert_eq!(label_case("Open OpenVSP Geometry"), "Open OpenVSP geometry");
        assert_eq!(label_case("Search Ctrl+K"), "Search Ctrl+K");
        assert_eq!(label_case("a - b"), "A - b");
    }
}
