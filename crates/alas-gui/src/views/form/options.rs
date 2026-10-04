// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Schema option resolution and numeric form helpers.
//!
//! The schema records which subsystem owns a string option, while this GUI
//! crate is the first layer that can see all of those registries together.
//! Keeping the adapters here leaves the renderer focused on layout and keeps
//! a missing source from degrading silently into a free-text editor.

use alas_config::{Entry, Field, Number, OptionSource, ZeroMeaning};
use serde_json::Value;

/// Resolve the option sources the schema names at the GUI boundary.
pub(super) fn resolved_options(field: &Field, values: &Value) -> Option<Vec<String>> {
    let source = match &field.entry {
        Entry::Leaf(leaf) => leaf.options,
        Entry::Node(_) => None,
    };

    let mut options = match source {
        Some(OptionSource::Airfoil) => {
            alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils()
                .into_iter()
                .map(str::to_owned)
                .collect()
        }
        Some(OptionSource::Engine) => alas_config::engines::available()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        Some(OptionSource::Material) => alas_config::materials::available()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        Some(OptionSource::StrutMaterial) => vec![
            "auto".to_owned(),
            "300M high-strength steel".to_owned(),
            "300M high-strength steel (titanium truck beam)".to_owned(),
            "7075-T6 aluminium".to_owned(),
        ],
        Some(OptionSource::TireClass) => std::iter::once("auto".to_owned())
            .chain(
                alas_perf::landing_gear::TIRE_DATABASE
                    .iter()
                    .map(|tire| tire.code.to_owned()),
            )
            .collect(),
        Some(OptionSource::MainDeckUld) => alas_payload::cargo::ULD_DATABASE
            .iter()
            .map(|uld| uld.key.to_owned())
            .collect(),
        Some(OptionSource::LowerDeckUld) => std::iter::once("AUTO".to_owned())
            .chain(
                alas_payload::cargo::ULD_DATABASE
                    .iter()
                    .map(|uld| uld.key.to_owned()),
            )
            .collect(),
        Some(OptionSource::CabinPreset) => Vec::new(),
        Some(source) => source
            .options()
            .map(|options| options.iter().map(|option| (*option).to_owned()).collect())?,
        None => return None,
    };

    if field.name == "cabin_preset" {
        let aircraft_type = values
            .get("aircraft_type")
            .and_then(Value::as_str)
            .unwrap_or("passenger");
        let presets = if aircraft_type == "cargo" {
            &["Max payload", "Dense payload", "Custom"][..]
        } else {
            &["Ryanair", "Iberia", "Emirates", "Custom"][..]
        };
        options = presets.iter().copied().map(str::to_owned).collect();
    }
    Some(options)
}

/// Resolve a readonly-unless condition against the current form level and
/// then its ancestors, matching the nested form contract.
pub(super) fn readonly_unless(
    values: &Value,
    ancestors: &[Value],
    condition: alas_config::ReadonlyUnless,
) -> bool {
    value_at_path(values, condition.field)
        .or_else(|| {
            ancestors
                .iter()
                .find_map(|ancestor| value_at_path(ancestor, condition.field))
        })
        .and_then(value_as_str)
        .map(|value| value != condition.value)
        .unwrap_or(true)
}

fn value_at_path<'a>(values: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(values, |current, segment| current.get(segment))
}

pub(super) fn is_editable(field: &Field) -> bool {
    if let Entry::Leaf(leaf) = &field.entry {
        return leaf.options.map(OptionSource::editable).unwrap_or(false);
    }
    false
}

pub(super) fn bounds(field: &Field) -> (Option<f64>, Option<f64>) {
    if let Entry::Leaf(leaf) = &field.entry {
        (leaf.min.map(number_to_f64), leaf.max.map(number_to_f64))
    } else {
        (None, None)
    }
}

pub(super) fn leaf_decimals(field: &Field) -> Option<u32> {
    if let Entry::Leaf(leaf) = &field.entry {
        return leaf.decimals;
    }
    None
}

fn number_to_f64(n: Number) -> f64 {
    match n {
        Number::Integer(i) => i as f64,
        Number::Real(r) => r,
    }
}

/// Descriptive, airline-independent labels for the cabin-preset
/// identifiers. The identifier itself is the serialized `cabin_preset` value
/// (also the key `alas-payload/src/build/presets.rs` matches on to write
/// preset geometry) and must not change; this only substitutes what the combo
/// box displays for it. Names describe each preset's actual seat mix
/// (`passenger_preset_mix` in that module): Ryanair is a single, all-economy
/// class at a tight pitch; Iberia writes Business and Economy shares only;
/// Emirates is the only preset that also populates First.
fn cabin_preset_display_name(value: &str) -> Option<&'static str> {
    match value {
        "Ryanair" => Some("High-density single-class"),
        "Iberia" => Some("Two-class (Business/Economy)"),
        "Emirates" => Some("Three-class (First/Business/Economy)"),
        _ => None,
    }
}

/// Present fixed option values as sentence-cased UI text without changing the
/// serialized configuration value. This keeps values such as `passenger` and
/// `auto` readable while preserving their lower-case data contracts.
pub(super) fn display_option(field: &Field, value: &str) -> String {
    if let Some(kind) = objective_kind(field, value) {
        return crate::views::tr(
            alas_pipeline::optimizer_summary::objective::objective_label(kind),
        );
    }
    let humanized = humanize_identifier(value);
    let source = cabin_preset_display_name(value)
        .or(humanized.as_deref())
        .unwrap_or(value);
    let localized = alas_i18n::t(Some(source), None).into_owned();
    let mut chars = localized.chars();
    let Some(first) = chars.next() else {
        return localized;
    };
    format!("{}{}", first.to_uppercase(), chars.as_str())
}

/// The objective a value names, for a field whose options are the objective
/// list. Decoding the serialized token avoids duplicating the variant list.
pub(super) fn objective_kind(field: &Field, value: &str) -> Option<alas_config::ObjectiveKind> {
    let Entry::Leaf(leaf) = &field.entry else {
        return None;
    };
    if leaf.options != Some(OptionSource::ObjectiveKind) {
        return None;
    }
    serde_json::from_value(Value::String(value.to_owned())).ok()
}

/// Whether the schema declares that zero means every logical thread.
pub(super) fn zero_means_all_threads(field: &Field) -> bool {
    matches!(
        &field.entry,
        Entry::Leaf(leaf) if leaf.zero_means == Some(ZeroMeaning::AllThreads)
    )
}

/// Logical threads the machine reports, resolved once: the editor formats
/// its value every frame and the count does not change while running.
pub(super) fn all_threads() -> usize {
    static THREADS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *THREADS.get_or_init(|| alas_config::SolverSettings::default().resolved_workers())
}

/// Readable text for a snake_case enumeration identifier such as
/// `fuel_per_seat_kilometre` or `lth_civil_transport_v1`. Only identifier-like
/// values (lower-case ASCII, digits and underscores) are rewritten, so catalogue
/// names (engines, materials, ULD codes) pass through untouched. Known
/// acronyms keep their capitals.
fn humanize_identifier(value: &str) -> Option<String> {
    let identifier_like = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !identifier_like {
        return None;
    }
    let special = match value {
        "fuel_per_seat_kilometre" => Some("Fuel per seat-kilometre"),
        "short_medium_haul" => Some("Short/medium haul"),
        "min_pallets" => Some("Minimum pallets"),
        _ => None,
    };
    if let Some(text) = special {
        return Some(text.to_owned());
    }
    if !value.contains('_') {
        return None;
    }
    let words: Vec<String> = value
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| match word {
            "flops" | "lth" | "faa" | "easa" | "cg" | "de" | "uld" | "mtow" | "oew" => {
                word.to_ascii_uppercase()
            }
            _ => word.to_owned(),
        })
        .collect();
    // Sentence case before the catalogue lookup, whose keys are UI text.
    let text = words.join(" ");
    let mut chars = text.chars();
    let first = chars.next()?;
    Some(format!("{}{}", first.to_uppercase(), chars.as_str()))
}

pub(super) fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Render a schema unit the way the desktop displays it.
///
/// The schema states a unit per field, and four different spellings of a
/// dimensionless quantity had reached the screen at once: a literal hyphen
/// (`10.000 -`), the word `fraction` (`0.027 fraction`), a phrase
/// (`0.020 fraction of critical`) and nothing at all. This collapses them to
/// one convention: a dimensionless quantity is a bare number, and a unit that
/// names the reference a fraction is taken of keeps only that reference
/// (`0.080 of semi-span`). Physical units are untouched apart from typography:
/// `^2`/`^3` become real superscripts and the ASCII `.` that separates
/// multiplied unit symbols becomes a middle dot, so `kg/(kgf.hr)` reads
/// `kg/(kgf.h)` with the conventional separator instead of a full stop.
///
/// No value is converted here; this is presentation only.
pub(crate) fn display_unit(unit: &str) -> String {
    let unit = unit.trim();
    let dimensionless = [
        "-",
        "--",
        "1",
        "0-1",
        "fraction",
        "dimensionless",
        "ratio",
        "x/c",
        "chord fraction",
        "root chord ratio",
    ];
    if unit.is_empty() || dimensionless.contains(&unit) {
        return String::new();
    }
    // A fraction taken of a named reference keeps the reference only.
    for prefix in ["fraction of ", "0-1 of "] {
        if let Some(reference) = unit.strip_prefix(prefix) {
            let reference = match reference {
                "semispan" => "semi-span",
                other => other,
            };
            // The phrase is translated whole: Spanish needs an article that
            // depends on the reference (`de la MAC`, `de semienvergadura`).
            return crate::views::tr(&format!("of {reference}"));
        }
    }
    typeset_unit(unit)
}

/// Superscripts and the middle dot of a physical unit symbol.
fn typeset_unit(unit: &str) -> String {
    // The UAV pages spell squares and cubes without a caret. Map only these
    // exact symbols; a general "digit after a letter" rule would also rewrite
    // chemical and model names.
    let unit = match unit {
        "m2" => "m^2",
        "m3" => "m^3",
        "kg/m2" => "kg/m^2",
        "kg/m3" => "kg/m^3",
        "m/s2" => "m/s^2",
        other => other,
    };
    let mut text = unit.replace("^2", "\u{b2}").replace("^3", "\u{b3}");
    // `hr` is not the SI symbol for the hour and only ever appears as a
    // denominator token here.
    text = text.replace("kgf.hr", "kgf.h");
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, character) in bytes.iter().enumerate() {
        let multiplied = *character == '.'
            && index > 0
            && index + 1 < bytes.len()
            && bytes[index - 1].is_alphanumeric()
            && bytes[index + 1].is_alphabetic();
        out.push(if multiplied { '\u{b7}' } else { *character });
    }
    out
}

/// The word a declared sentinel should read as, when the field currently
/// holds it.
///
/// Several fields declare a magic value in their own schema text - three
/// landing-gear counts say `(0 = auto)` in the label, the design range says
/// "Zero uses the great-circle distance", the span and approach-speed limits
/// say "Zero disables", MSES says "1.0 = free (natural) transition" - and all
/// of them still rendered as an ordinary number, indistinguishable from a
/// measured value. The same panel already solves this for Tire class and Strut
/// material, whose dropdowns read "Auto". Only these declared patterns are
/// matched: "Zero adds", "Zero cost" and "Enter zero explicitly" describe real
/// values, not sentinels, and must keep rendering as numbers.
pub(super) fn sentinel_word(field: &Field, value: f64) -> Option<&'static str> {
    if value == 0.0 {
        if field.label.contains("(0 = auto)") || field.help.contains("0 = auto") {
            return Some("Auto");
        }
        if field.help.contains("Zero uses the great-circle distance") {
            return Some("From route");
        }
        if field.help.contains("Zero disables ") {
            return Some("No limit");
        }
    }
    if value == 1.0 && field.help.contains("1.0 = free") {
        return Some("Free");
    }
    None
}

/// What an empty optional field means, from the schema's own instruction.
///
/// An optional editor rendered a blank box with no placeholder, no unit and no
/// hint, so "derived", "random" and "never set" all looked the same.
pub(super) fn optional_hint(field: &Field) -> &'static str {
    let help = field.help;
    if help.contains("different search each run") {
        return "Random";
    }
    if help.contains("Leave blank to let the app determine") || help.contains("auto-derived") {
        return "Auto";
    }
    "Not set"
}

/// The localized sentinel word an editor should render for `value`.
pub(super) fn sentinel_text(field: &Field, value: f64) -> Option<String> {
    if value == 0.0 && zero_means_all_threads(field) {
        return Some(crate::views::tr_fields(
            "Automatic (all {count} threads)",
            &[("count", all_threads().to_string())],
        ));
    }
    sentinel_word(field, value).map(|word| alas_i18n::t(Some(word), None).into_owned())
}

/// Parse an editor's text, accepting any sentinel word this field renders in
/// either language, so the value it displays can also be typed back.
pub(super) fn parse_number_or_sentinel(text: &str, field: &Field) -> Option<f64> {
    let trimmed = text.trim();
    if zero_means_all_threads(field) && sentinel_text(field, 0.0).as_deref() == Some(trimmed) {
        return Some(0.0);
    }
    for candidate in [0.0_f64, 1.0] {
        if let Some(word) = sentinel_word(field, candidate) {
            let localized = alas_i18n::t(Some(word), None);
            if trimmed.eq_ignore_ascii_case(word)
                || trimmed.eq_ignore_ascii_case(localized.as_ref())
            {
                return Some(candidate);
            }
        }
    }
    trimmed.parse::<f64>().ok()
}

/// The shortest decimal rendering of `value` that still round-trips within
/// `max_decimals` places.
///
/// `egui` derives a `DragValue`'s minimum decimals from its drag speed and the
/// pointer aim radius, which is scaled by the display's points-per-pixel: on a
/// 2x display that rendered whole-number counts as `16.0` and `0.0`, and the
/// same quantity at two magnitudes with different decimals (`-4.00` beside
/// `10.0`). Formatting explicitly makes the rendering a property of the value
/// and the field, not of the monitor.
pub(super) fn format_number(value: f64, max_decimals: usize) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    for decimals in 0..max_decimals {
        let text = format!("{value:.decimals$}");
        let parsed: f64 = text.parse().unwrap_or(f64::NAN);
        if (parsed - value).abs() <= value.abs() * 1e-9 + 1e-12 {
            return text;
        }
    }
    format!("{value:.max_decimals$}")
}

/// Decimals and drag step from a value's magnitude.
pub(super) fn number_step(value: f64, unit: &str, decimals_override: Option<u32>) -> (usize, f64) {
    if let Some(d) = decimals_override {
        return (d as usize, 10f64.powi(-(d as i32)));
    }
    let av = if value != 0.0 { value.abs() } else { 1.0 };
    if unit == "kg" || unit == "Pa" {
        if av >= 100_000.0 {
            return (0, 10_000.0);
        }
        if av >= 10_000.0 {
            return (0, 1_000.0);
        }
        if av >= 1_000.0 {
            return (0, 100.0);
        }
        return (1, 10.0);
    }
    if av >= 10_000.0 {
        return (0, 500.0);
    }
    if av >= 1_000.0 {
        return (0, 50.0);
    }
    if av >= 100.0 {
        return (1, 5.0);
    }
    if av >= 10.0 {
        return (2, 1.0);
    }
    if av >= 1.0 {
        return (3, 0.1);
    }
    if av >= 0.01 {
        return (3, 0.01);
    }
    (5, 0.001)
}
