// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/translations/es.py
// Reference: alas @ rust-port-baseline.

//! The Spanish string catalog: every English source string the reference
//! implementation's UI shows, mapped to its Spanish translation.
//!
//! The table itself is not Rust source. `alas/translations/es.py` is a
//! roughly 1,100-line dict literal, and hand-formatting that as ~760 `match`
//! arms would both blow past the 500-line file limit several times over and
//! turn a data file into code nobody reviews as code. It lives instead as
//! `data/es_catalog.json`, checked into this crate, generated once from the
//! Python source (`golden/generators/gen_i18n.py`) and parsed the first time
//! [`install`] runs.
//!
//! That embedded copy is deliberately distinct from `golden/i18n/es_catalog.json`,
//! the parity fixture -- a shipped binary must not need `golden/` to exist on
//! disk, so this crate carries its own copy rather than reading the test
//! fixture at runtime. `tests/parity_es.rs` is what keeps the two from
//! drifting apart.

use std::collections::HashMap;
use std::sync::OnceLock;

// `env!("CARGO_MANIFEST_DIR")` at compile time is what makes this path
// resolve the same way regardless of the caller's working directory --
// `include_str!` itself only accepts a path relative to this source file, so
// the manifest-dir prefix is for readers, not for the compiler.
const CATALOG_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/es_catalog.json"));

/// The parsed embedded catalog: every English key mapped to its Spanish
/// value, exactly as `data/es_catalog.json` holds it.
///
/// Exposed mainly for `tests/parity_es.rs`, which checks this against
/// `golden/i18n/es_catalog.json` in both directions; [`install`] is the
/// entry point anything else should use.
pub fn catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        // `alas.i18n._catalog` treats a broken or absent Spanish catalog as
        // empty rather than letting the exception propagate ("a broken/absent
        // catalog must never break the app"). This file is generated and
        // checked in, so a parse failure here should never happen, but the
        // same fallback -- log it, then behave as if no catalog were
        // installed -- is what keeps this a reported condition rather than a
        // panic if it ever does.
        serde_json::from_str(CATALOG_JSON).unwrap_or_else(|error| {
            tracing::error!(%error, "crates/alas-i18n/data/es_catalog.json failed to parse");
            HashMap::new()
        })
    })
}

/// Register the Spanish catalog with [`crate::register_catalog`].
///
/// Nothing in this crate calls this automatically -- see the crate's module
/// doc. Whatever assembles the running application calls it once, at
/// startup, if Spanish support is wanted; a headless build that never calls
/// it behaves exactly as if Spanish had no catalog at all, which is the safe
/// default [`crate::t`] already falls back to.
pub fn install() {
    let entries = catalog()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()));
    crate::register_catalog("es", entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_makes_a_known_label_translate_to_its_spanish_text() {
        install();
        assert_eq!(crate::t(Some("Wing"), Some("es")), "Ala");
        assert_eq!(crate::t(Some("Fuselage"), Some("es")), "Fuselaje");
        assert_eq!(crate::t(Some("Enabled"), Some("es")), "Activado");
    }

    #[test]
    fn install_leaves_an_uncatalogued_string_in_english() {
        install();
        assert_eq!(
            crate::t(Some("Not a real catalog entry"), Some("es")),
            "Not a real catalog entry"
        );
    }

    #[test]
    fn the_embedded_catalog_parses_into_a_nonempty_table() {
        assert!(catalog().len() > 700);
    }
}
