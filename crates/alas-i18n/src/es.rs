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

// Ported from desktop/frontend/src/lib/translations.es.ts.
// Reference: alas @ rust-port-baseline.
const DESKTOP_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/es_desktop_catalog.json"
));

// Native Rust-shell additions have no Python or TypeScript source counterpart.
// They stay separate so source-backed parity is never inferred for them.
const NATIVE_DESKTOP_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/es_native_desktop_catalog.json"
));

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

/// Desktop-shell translations from the reference frontend catalog plus
/// explicitly native Rust-shell additions.
///
/// ASCII punctuation aliases are included for the Rust source-style rule;
/// their values are the same source translations as the Unicode keys.
pub fn desktop_catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut combined = desktop_source_catalog().clone();
        combined.extend(native_desktop_catalog().clone());
        combined
    })
}

/// Source-backed translations from the reference desktop frontend.
pub fn desktop_source_catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(DESKTOP_CATALOG_JSON).unwrap_or_else(|error| {
            tracing::error!(
                %error,
                "crates/alas-i18n/data/es_desktop_catalog.json failed to parse"
            );
            HashMap::new()
        })
    })
}

/// Spanish translations for strings introduced by the native Rust shell.
pub fn native_desktop_catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(NATIVE_DESKTOP_CATALOG_JSON).unwrap_or_else(|error| {
            tracing::error!(
                %error,
                "crates/alas-i18n/data/es_native_desktop_catalog.json failed to parse"
            );
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
    let desktop_entries = desktop_catalog()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()));
    crate::extend_catalog("es", desktop_entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_makes_a_known_label_translate_to_its_spanish_text() {
        let _registry_guard = crate::registry_test_guard();
        crate::reset_registry_for_test();
        install();
        assert_eq!(crate::t(Some("Wing"), Some("es")), "Ala");
        assert_eq!(crate::t(Some("Fuselage"), Some("es")), "Fuselaje");
        assert_eq!(crate::t(Some("Enabled"), Some("es")), "Activado");
    }

    #[test]
    fn install_leaves_an_uncatalogued_string_in_english() {
        let _registry_guard = crate::registry_test_guard();
        crate::reset_registry_for_test();
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

    #[test]
    fn desktop_catalog_preserves_known_reference_frontend_translations() {
        assert_eq!(
            desktop_source_catalog().get("File").map(String::as_str),
            Some("Archivo")
        );
        assert_eq!(
            desktop_source_catalog()
                .get("Advanced Walkthrough")
                .map(String::as_str),
            Some("Gu\u{00ed}a avanzada")
        );
        assert_eq!(
            desktop_source_catalog()
                .get("What ALAS does")
                .map(String::as_str),
            Some("Qu\u{00e9} hace ALAS")
        );
    }

    #[test]
    fn desktop_catalog_carries_source_equivalent_ascii_aliases() {
        assert_eq!(
            desktop_source_catalog().get("Next ->"),
            desktop_source_catalog().get("Next \u{2192}")
        );
        assert_eq!(
            desktop_source_catalog().get("<- Back"),
            desktop_source_catalog().get("\u{2190} Back")
        );
    }

    #[test]
    fn native_tour_translations_are_not_misattributed_to_the_reference_catalog() {
        assert!(!desktop_source_catalog().contains_key("Welcome to ALAS"));
        assert_eq!(
            native_desktop_catalog()
                .get("Welcome to ALAS")
                .map(String::as_str),
            Some("Bienvenido a ALAS")
        );
        assert_eq!(
            desktop_catalog().get("Welcome to ALAS").map(String::as_str),
            Some("Bienvenido a ALAS")
        );
    }

    #[test]
    fn native_desktop_entries_never_claim_reference_frontend_provenance() {
        for key in native_desktop_catalog().keys() {
            assert!(
                !desktop_source_catalog().contains_key(key),
                "native desktop key is also attributed to the reference catalog: {key}"
            );
        }
    }

    #[test]
    fn native_analysis_and_result_templates_translate_with_placeholders_intact() {
        assert_eq!(
            native_desktop_catalog()
                .get("Physical status")
                .map(String::as_str),
            Some("Estado f\u{00ed}sico")
        );
        assert_eq!(
            native_desktop_catalog()
                .get("Stage 2 (3-D wing): {done}/{total} re-simulated -- {ok} ok")
                .map(String::as_str),
            Some("Etapa 2 (ala 3-D): {done}/{total} resimulados; {ok} correctos")
        );
        assert_eq!(
            native_desktop_catalog()
                .get("Not available: {detail}")
                .map(String::as_str),
            Some("No disponible: {detail}")
        );
    }

    #[test]
    fn native_form_feedback_stays_localized_without_claiming_frontend_provenance() {
        assert_eq!(
            native_desktop_catalog().get("Modified").map(String::as_str),
            Some("Modificado")
        );
        assert!(!desktop_source_catalog().contains_key("Modified"));
        assert_eq!(
            native_desktop_catalog()
                .get("Automatic zoom")
                .map(String::as_str),
            Some("Zoom autom\u{e1}tico")
        );
    }
}
