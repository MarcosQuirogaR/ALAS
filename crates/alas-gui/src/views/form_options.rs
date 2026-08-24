// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Schema option resolution and numeric form helpers.
//!
//! The schema records which subsystem owns a string option, while this GUI
//! crate is the first layer that can see all of those registries together.
//! Keeping the adapters here leaves the renderer focused on layout and keeps
//! a missing source from degrading silently into a free-text editor.

use alas_config::{Entry, Field, Number, OptionSource};
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
        Some(OptionSource::CabinPreset) => Vec::new(),
        Some(source) => source
            .options()
            .map(|options| options.iter().map(|option| (*option).to_owned()).collect())?,
        None if field.name == "class_mix_mode" => {
            vec!["percent".to_owned(), "count".to_owned()]
        }
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
    values
        .get(condition.field)
        .or_else(|| {
            ancestors
                .iter()
                .find_map(|ancestor| ancestor.get(condition.field))
        })
        .and_then(value_as_str)
        .map(|value| value != condition.value)
        .unwrap_or(true)
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

/// Present fixed option values as sentence-cased UI text without changing the
/// serialized configuration value. This keeps values such as `passenger` and
/// `auto` readable while preserving their lower-case data contracts.
pub(super) fn display_option(value: &str) -> String {
    let localized = alas_i18n::t(Some(value), None).into_owned();
    let mut chars = localized.chars();
    let Some(first) = chars.next() else {
        return localized;
    };
    format!("{}{}", first.to_uppercase(), chars.as_str())
}

pub(super) fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Decimals and drag step from a value's magnitude, ported from the
/// reference's numberStep.
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
