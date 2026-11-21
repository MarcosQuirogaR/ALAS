// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `gui`: every page of the navigation model is
//! reachable, and every form page edits a group that exists in the settings
//! schema, so no page is a dead link.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use alas_config::{AlasConfig, ConfigNode};
use alas_gui::nav::{all_pages, PageKind};

#[test]
fn capability_smoke_gui_pages_resolve_to_schema_groups() {
    let schema = AlasConfig::default().schema();
    let groups: BTreeSet<&str> = schema.fields.iter().map(|field| field.name).collect();
    let mut ids = BTreeSet::new();
    for page in all_pages() {
        assert!(ids.insert(page.id), "duplicate page id {}", page.id);
        if page.kind == PageKind::Form {
            let group = page.group.expect("a form page names its group");
            assert!(groups.contains(group), "{}: unknown group {group}", page.id);
        }
        for section in page.extra {
            let top = section
                .path
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap();
            assert!(
                groups.contains(top),
                "{}: unknown card node {}",
                page.id,
                section.path
            );
        }
    }
    assert!(ids.len() >= 15, "{}", ids.len());
}
