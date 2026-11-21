// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Acceptance sample: global search finds thirty-three concepts a person
//! would type, each on the page (and section) that edits it.

use super::{build_index, search};

/// (what a person types, the page that must be among the first hits, the
/// Advanced section when it sits in one).
const SAMPLES: [(&str, &str, Option<&str>); 33] = [
    ("mtow", "inputs", None),
    ("range", "inputs", None),
    ("mach", "inputs", None),
    ("altitude", "inputs", None),
    ("passengers", "inputs", None),
    ("departure", "inputs", None),
    ("arrival", "inputs", None),
    ("toda", "inputs", None),
    ("runway", "inputs", None),
    ("preset", "inputs", None),
    ("clean sheet", "inputs", None),
    ("optimizer", "inputs", Some("Optimizer")),
    ("aspect ratio", "inputs", Some("Geometry")),
    ("empennage", "inputs", Some("Geometry")),
    ("bypass ratio", "inputs", Some("Propulsion")),
    ("flops", "inputs", Some("Mass")),
    ("wave drag", "inputs", Some("Aerodynamics")),
    ("root airfoil", "inputs", Some("Aerodynamics")),
    ("nose gear", "inputs", Some("Landing Gear")),
    ("aileron", "inputs", Some("Control Surfaces")),
    ("seat pitch", "cabin", None),
    ("galley", "cabin", None),
    ("climb", "mission", None),
    ("alternate", "mission", None),
    ("clmax", "mission", Some("Performance")),
    ("simbrief", "mission", Some("Mission Analysis")),
    ("cargo", "payload", None),
    ("nastran", "setup_analyses", Some("Structures")),
    ("vlm", "setup_analyses", Some("Analysis Fidelity")),
    ("mses", "setup_analyses", Some("MSES Analysis")),
    ("install", "setup_tools", None),
    ("export", "files", None),
    ("bounds", "design_space", None),
];

#[test]
fn thirty_three_sample_concepts_are_found_on_the_page_that_edits_them() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let mut misses = Vec::new();
    for (query, page, section) in SAMPLES {
        let hits = search(&index, query, 12);
        let found = hits
            .iter()
            .any(|entry| entry.page == page && (section.is_none() || entry.section == section));
        if !found {
            misses.push(format!(
                "{query:?} -> {page}/{section:?}; got {:?}",
                hits.iter()
                    .map(|entry| (entry.title.as_str(), entry.page, entry.section))
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

#[test]
fn every_concept_title_has_a_spanish_catalogue_entry() {
    let catalog = alas_i18n::es::desktop_catalog();
    for concept in crate::search_concepts::CONCEPTS {
        assert!(
            catalog.contains_key(concept.title),
            "concept without a Spanish entry: {}",
            concept.title
        );
        for text in concept.section.into_iter() {
            assert!(
                catalog.contains_key(text),
                "section without an entry: {text}"
            );
        }
    }
}

#[test]
fn a_word_in_a_title_or_field_name_ranks_above_a_help_only_match() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let hits = search(&index, "dihedral", 12);
    assert!(!hits.is_empty());
    let named = |entry: &crate::search::SearchEntry| {
        entry.title.to_lowercase().contains("dihedral")
            || entry
                .field
                .as_deref()
                .is_some_and(|path| path.to_lowercase().contains("dihedral"))
    };
    // Every hit that names dihedral comes before the first one that only
    // mentions it in its help text.
    let first_help_only = hits.iter().position(|entry| !named(entry));
    let last_named = hits.iter().rposition(|entry| named(entry));
    if let (Some(help), Some(named)) = (first_help_only, last_named) {
        assert!(
            named < help,
            "{:?}",
            hits.iter().map(|e| &e.title).collect::<Vec<_>>()
        );
    }
    assert!(
        named(hits[0]) || hits[0].summary.contains("dihedral"),
        "first hit: {}",
        hits[0].title
    );
    assert_ne!(hits[0].title, "Main-gear oleo length");
}

#[test]
fn cg_gate_opens_the_advanced_mass_page_where_the_sheet_is_drawn() {
    let state = crate::state::AppState::default();
    let index = build_index(&state.schema);
    let hits = search(&index, "CG gate", 12);
    let first = hits.first().expect("a hit for the CG gate");
    assert_eq!(first.title, "CG gate");
    assert_eq!(first.page, "mass_model");
    assert!(first.advanced);
}

#[test]
fn the_legacy_vibration_psd_is_not_searchable() {
    let state = crate::state::AppState::default();
    let index = crate::search::build_index(&state.schema);
    assert!(index
        .iter()
        .all(|entry| entry.field.as_deref() != Some("structures.psd_base_g2_per_hz")));
}
