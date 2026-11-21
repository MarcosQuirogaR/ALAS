// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A per-section cache of schema field lists, so a page that renders the same
//! sections every frame does not clone its schema subtree each time.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use alas_config::Field;

type SectionFieldsCache = HashMap<(String, String, usize), (usize, Rc<Vec<Field>>)>;

thread_local! {
    /// Per-section field lists derived from an immutable schema slice, keyed
    /// by (owner, section) and tagged with the slice they were built from.
    static SECTION_FIELDS: RefCell<SectionFieldsCache> = RefCell::new(HashMap::new());
}

/// A derived list of schema fields built once per `(owner, section)` and
/// shared afterwards. `identity` is the address and length of the schema
/// slice it derives from, so a different schema rebuilds instead of reusing a
/// stale list. Without it every section of every open page cloned its schema
/// subtree on every frame.
pub(super) fn cached_section_fields(
    owner: &str,
    section: &str,
    identity: (usize, usize),
    build: impl FnOnce() -> Vec<Field>,
) -> Rc<Vec<Field>> {
    SECTION_FIELDS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (owner.to_owned(), section.to_owned(), identity.0);
        if let Some((len, fields)) = cache.get(&key) {
            if *len == identity.1 {
                return Rc::clone(fields);
            }
        }
        let fields = Rc::new(build());
        cache.insert(key, (identity.1, Rc::clone(&fields)));
        fields
    })
}
