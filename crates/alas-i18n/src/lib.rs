// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/i18n.py
// Reference: alas @ rust-port-baseline.

//! Runtime language selection and the boundary where English source text
//! becomes a translation.
//!
//! **Design: English stays canonical in the source.** Configuration field
//! labels and help text, figure titles and axis labels are written inline in
//! English exactly as everywhere else in this program; translation happens
//! only where text is handed to a user, by looking the English string up in a
//! catalog for the active language. Nothing in the physics or configuration
//! crates has to know a second language exists, there is no duplicated prose
//! to keep in sync, and an untranslated string degrades to English rather
//! than to a missing-key placeholder. [`t`] is that lookup; everything else
//! here is what it needs to decide which language to use.
//!
//! # A thread-local in place of Python's `ContextVar`
//!
//! The reference implementation keeps the active language in a
//! `contextvars.ContextVar`. Its sidecar serves HTTP requests concurrently on
//! worker threads and renders figures from a thread pool, and a `ContextVar`
//! gives each request's call graph its own value -- inherited by whatever it
//! spawns -- without every request racing over one process-wide setting.
//!
//! This workspace has no request, or worker-thread pool to route a value
//! through yet: the desktop interface and the pipeline that would eventually
//! drive one are both still unbuilt (`docs/PORTING.md`). What carries over is
//! the reason a bare global was wrong upstream, not the mechanism -- two units
//! of concurrent work must not see each other's language. [`std::thread_local!`]
//! gives every OS thread its own cell, which is that same guarantee restated
//! for the unit of concurrency this program has today (a thread) instead of
//! the one Python's sidecar had (an async task). Pulling in an async runtime
//! or a context-propagation crate to reproduce `ContextVar` exactly would be
//! adding a dependency to serve a request model that does not exist yet;
//! when one does, routing the language through it is a design question for
//! whatever carries the request's context then.
//!
//! # Catalogs degrade to English, never to an error
//!
//! A catalog is one language's string table: English source text mapped to
//! its translation. [`register_catalog`] is the only way one is installed,
//! and nothing in this crate calls it -- the Spanish catalog (ported from
//! `alas/translations/es.py` as `alas_i18n::es`, tracked separately in
//! `docs/PORTING.md`) is expected to call it once, when it exists. Until it
//! does, or for any language a catalog does not cover an entry for, [`t`]
//! returns the original English text. A missing or partial catalog is
//! therefore always the safe case, never an error: a headless build that
//! never registers anything behaves exactly as if every lookup missed.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub mod es;

/// The language every lookup falls back to.
pub const DEFAULT_LANGUAGE: &str = "en";

/// Every language code this program recognizes. Anything else normalizes to
/// [`DEFAULT_LANGUAGE`].
pub const SUPPORTED_LANGUAGES: [&str; 2] = ["en", "es"];

/// Map anything a caller might send -- `"es-ES"`, `"ES"`, `"es_MX"`, absent --
/// onto a supported code, falling back to [`DEFAULT_LANGUAGE`] rather than
/// rejecting it.
///
/// Case is folded, underscores are treated as the hyphen IETF language tags
/// use, and only the primary subtag is read: a region or script suffix this
/// program has no catalog for (`"es-MX"`) still resolves to the language it
/// can serve (`"es"`).
pub fn normalize_language(lang: Option<&str>) -> String {
    let Some(lang) = lang else {
        return DEFAULT_LANGUAGE.to_string();
    };
    let folded = lang.trim().to_lowercase().replace('_', "-");
    let primary = folded.split('-').next().unwrap_or("");
    if SUPPORTED_LANGUAGES.contains(&primary) {
        primary.to_string()
    } else {
        DEFAULT_LANGUAGE.to_string()
    }
}

thread_local! {
    // See the module doc for why this is thread-scoped rather than a bare
    // global or a `ContextVar` equivalent.
    static CURRENT_LANGUAGE: RefCell<String> = RefCell::new(DEFAULT_LANGUAGE.to_string());
}

/// Set the calling thread's active language, normalizing it first, and
/// return the code that was actually set.
pub fn set_language(lang: Option<&str>) -> String {
    let normalized = normalize_language(lang);
    CURRENT_LANGUAGE.with(|cell| *cell.borrow_mut() = normalized.clone());
    normalized
}

/// The calling thread's active language: [`DEFAULT_LANGUAGE`] until
/// [`set_language`] has been called on this thread.
pub fn get_language() -> String {
    CURRENT_LANGUAGE.with(|cell| cell.borrow().clone())
}

fn registry() -> &'static Mutex<HashMap<String, HashMap<String, String>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, HashMap<String, String>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Install `entries` as the string table for `lang`, replacing whatever was
/// registered for it before.
///
/// This is the plug-in point a catalog module calls at startup -- `alas-i18n`
/// itself never calls it, which is what lets it build and pass its tests with
/// no Spanish catalog present at all. See the module doc.
pub fn register_catalog<I>(lang: &str, entries: I)
where
    I: IntoIterator<Item = (String, String)>,
{
    let table: HashMap<String, String> = entries.into_iter().collect();
    // A poisoned lock means an earlier registration panicked mid-insert; the
    // table it was writing is best treated as never having arrived rather
    // than propagating that panic into an unrelated caller.
    if let Ok(mut catalogs) = registry().lock() {
        catalogs.insert(lang.to_string(), table);
    }
}

// The registry lookup on its own, kept apart from `t` so a miss (no catalog,
// or no entry) is a plain `None` rather than something `t` has to unpack
// twice.
fn translated(lang: &str, text: &str) -> Option<String> {
    registry().lock().ok()?.get(lang)?.get(text).cloned()
}

/// Translate `text` into the active (or given) language.
///
/// `text` absent or empty returns empty without a lookup, matching Python's
/// treatment of `None`/`""` as nothing to translate. Otherwise the text comes
/// back unchanged for [`DEFAULT_LANGUAGE`], and for any other language it is
/// looked up in that language's catalog and returned as-is if the catalog is
/// absent or has no entry for it -- a partial catalog is therefore always
/// safe to ship, and a newly written English string shows up untranslated
/// instead of missing.
///
/// `lang` given explicitly is normalized and used as-is; `lang` absent reads
/// the calling thread's active language via [`get_language`]. Returns a
/// borrow of `text` whenever no translation happens, which is the common
/// case (the default language, or a catalog miss), so translating text that
/// never gets translated costs no allocation.
pub fn t<'a>(text: Option<&'a str>, lang: Option<&str>) -> Cow<'a, str> {
    let text = match text {
        Some(value) if !value.is_empty() => value,
        _ => return Cow::Borrowed(""),
    };
    let language = if lang.is_some() {
        normalize_language(lang)
    } else {
        get_language()
    };
    if language == DEFAULT_LANGUAGE {
        return Cow::Borrowed(text);
    }
    match translated(&language, text) {
        Some(value) => Cow::Owned(value),
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_language_folds_region_and_script_suffixes_to_the_base_language() {
        for input in ["es-ES", "ES", "es_MX"] {
            assert_eq!(normalize_language(Some(input)), "es", "input was {input}");
        }
    }

    #[test]
    fn normalize_language_maps_absent_or_empty_to_the_default() {
        assert_eq!(normalize_language(None), DEFAULT_LANGUAGE);
        assert_eq!(normalize_language(Some("")), DEFAULT_LANGUAGE);
    }

    #[test]
    fn normalize_language_maps_an_unsupported_code_to_the_default() {
        assert_eq!(normalize_language(Some("fr")), DEFAULT_LANGUAGE);
        assert_eq!(normalize_language(Some("xx-YY")), DEFAULT_LANGUAGE);
    }

    #[test]
    fn normalize_language_is_idempotent_on_already_normalized_codes() {
        for code in SUPPORTED_LANGUAGES {
            assert_eq!(normalize_language(Some(code)), code);
        }
    }

    #[test]
    fn t_returns_the_original_text_unchanged_for_the_default_language() {
        assert_eq!(t(Some("Wing area"), Some("en")), "Wing area");
    }

    #[test]
    fn t_treats_absent_or_empty_text_as_itself_rather_than_looking_it_up() {
        assert_eq!(t(None, Some("es")), "");
        assert_eq!(t(Some(""), Some("es")), "");
    }

    #[test]
    fn t_falls_back_to_english_until_a_catalog_is_registered_for_the_language() {
        // No test above or below this one registers a catalog for "es", so
        // this is the one place in the crate that owns that global state --
        // ordering everything through a single test avoids racing another
        // test that assumes the catalog is still absent.
        assert_eq!(t(Some("Wing area"), Some("es")), "Wing area");

        register_catalog(
            "es",
            [("Wing area".to_string(), "Superficie alar".to_string())],
        );

        assert_eq!(t(Some("Wing area"), Some("es")), "Superficie alar");
        // A key the catalog does not cover still falls back.
        assert_eq!(t(Some("Fuselage length"), Some("es")), "Fuselage length");
    }

    #[test]
    fn the_current_language_is_independent_per_thread() {
        // A fresh OS thread starts at the default; setting it on one thread
        // must not be visible from another, which is the property a bare
        // global would not have had.
        let es_thread = std::thread::spawn(|| {
            set_language(Some("es"));
            get_language()
        });
        let en_thread = std::thread::spawn(|| {
            set_language(Some("en"));
            get_language()
        });

        let es_result = es_thread.join();
        let en_result = en_thread.join();
        assert!(es_result.is_ok());
        assert!(en_result.is_ok());
        if let (Ok(es_lang), Ok(en_lang)) = (es_result, en_result) {
            assert_eq!(es_lang, "es");
            assert_eq!(en_lang, "en");
        }

        // Neither spawned thread's call touched this thread's own cell.
        assert_eq!(get_language(), DEFAULT_LANGUAGE);
    }
}
