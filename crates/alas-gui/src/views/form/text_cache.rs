// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Translated field labels and help texts, kept between frames.
//!
//! A form re-translated (and title-cased) every label and help string of every
//! visible field on every frame. Both are pure functions of the schema string
//! and the language, and the schema strings are `'static`, so the result is
//! cached against the string's address and length and dropped when the
//! language changes. A call without an explicit language reads the thread's
//! language and is not cached.

use std::cell::RefCell;
use std::collections::HashMap;

type Key = (usize, usize);

#[derive(Default)]
struct TextCache {
    language: String,
    labels: HashMap<Key, String>,
    helps: HashMap<Key, String>,
}

thread_local! {
    static CACHE: RefCell<TextCache> = RefCell::new(TextCache::default());
}

fn key(text: &'static str) -> Key {
    (text.as_ptr() as usize, text.len())
}

fn lookup(
    text: &'static str,
    lang: Option<&str>,
    pick: fn(&mut TextCache) -> &mut HashMap<Key, String>,
    compute: impl FnOnce() -> String,
) -> String {
    let Some(lang) = lang else {
        return compute();
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.language != lang {
            cache.language = lang.to_owned();
            cache.labels.clear();
            cache.helps.clear();
        }
        pick(&mut cache)
            .entry(key(text))
            .or_insert_with(compute)
            .clone()
    })
}

/// The field label for `lang` (see [`crate::views::form_label`]).
pub(super) fn label(text: &'static str, lang: Option<&str>) -> String {
    lookup(
        text,
        lang,
        |cache| &mut cache.labels,
        || crate::views::form_label(text, lang),
    )
}

/// The field help text for `lang`.
pub(super) fn help(text: &'static str, lang: Option<&str>) -> String {
    lookup(
        text,
        lang,
        |cache| &mut cache.helps,
        || alas_i18n::t(Some(text), lang).into_owned(),
    )
}
