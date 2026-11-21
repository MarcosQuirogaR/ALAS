// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `deserialize_with` helpers for `&'static str` fields of stored report
//! types (`alas --only`'s report cache): the text read back is leaked to give
//! it the `'static` lifetime. A cache read happens once per process, so the
//! leak is bounded by the size of one report.

use serde::{Deserialize, Deserializer};

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// A `&'static str` field.
///
/// # Errors
///
/// The deserializer's error when the value is not a string.
pub fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<&'static str, D::Error> {
    String::deserialize(deserializer).map(leak)
}

/// An `Option<&'static str>` field.
///
/// # Errors
///
/// The deserializer's error when the value is not a string or null.
pub fn optional<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'static str>, D::Error> {
    Option::<String>::deserialize(deserializer).map(|text| text.map(leak))
}

/// A `Vec<(&'static str, T)>` field.
///
/// # Errors
///
/// The deserializer's error when the value is not a list of pairs.
pub fn pairs<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<(&'static str, T)>, D::Error> {
    Vec::<(String, T)>::deserialize(deserializer).map(|pairs| {
        pairs
            .into_iter()
            .map(|(text, value)| (leak(text), value))
            .collect()
    })
}

/// `&'static str` under a name serde's derive does not treat as a borrowed
/// field (a literal `&'static str` field makes the derived impl require
/// `'de: 'static`, even with `deserialize_with`).
pub type Text = &'static str;
