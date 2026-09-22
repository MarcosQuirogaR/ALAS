// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spanish desktop catalog coverage and rendered-scene integration tests.

use alas_config::ConfigNode;
use alas_gui::scene::build_page_preview;
use alas_gui::state::AppState;

#[test]
fn every_registered_figure_has_spanish_title_description_and_category_provenance() {
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    for descriptor in alas_report::PREVIEW_FIGURES
        .iter()
        .chain(alas_report::RESULT_FIGURES)
        .chain(alas_report::SCREENING_FIGURES)
    {
        for (kind, text) in [
            ("title", descriptor.title),
            ("description", descriptor.description),
            ("category", descriptor.category),
        ] {
            assert!(
                base.contains_key(text) || desktop.contains_key(text),
                "figure {} {kind} has no Spanish catalog provenance: {text}",
                descriptor.id
            );
        }
    }
}

#[test]
fn every_schema_label_and_help_sentence_has_spanish_catalog_provenance() {
    fn check_fields(
        fields: &[alas_config::Field],
        base: &std::collections::HashMap<String, String>,
        desktop: &std::collections::HashMap<String, String>,
        missing: &mut Vec<String>,
    ) {
        for field in fields {
            for (kind, text) in [("label", field.label), ("help", field.help)] {
                if !text.is_empty() && !base.contains_key(text) && !desktop.contains_key(text) {
                    missing.push(format!("{} {kind}: {text}", field.name));
                }
            }
            if let alas_config::Entry::Node(node) = &field.entry {
                check_fields(&node.fields, base, desktop, missing);
            }
        }
    }

    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    let mut missing = Vec::new();
    check_fields(
        &alas_config::AlasConfig::default().schema().fields,
        base,
        desktop,
        &mut missing,
    );
    assert!(
        missing.is_empty(),
        "schema strings without Spanish provenance:\n{}",
        missing.join("\n")
    );
}

#[test]
fn spanish_scene_dispatch_localizes_static_report_text_without_changing_identifiers() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let state = AppState::default();
    let scene = build_page_preview(&state, "geometry").expect("localized geometry preview");
    assert_eq!(scene.title.as_deref(), Some("Dibujo en tres vistas"));
    assert!(scene.elements.iter().any(|element| matches!(
        element,
        alas_report::scene::SceneElement::Text { text, .. }
            if text != "Three-View Drawing" && text.contains("Vista")
    )));
    alas_i18n::set_language(Some("en"));
}

#[test]
fn every_literal_sent_to_the_gui_translation_boundary_has_catalog_provenance() {
    let mut files = Vec::new();
    collect_rs_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    for path in files {
        let source = std::fs::read_to_string(&path).expect("ASCII GUI source");
        for function in ["tr", "tr_fields"] {
            for key in literal_arguments(&source, function) {
                assert!(
                    base.contains_key(&key) || desktop.contains_key(&key),
                    "{} sends an uncatalogued literal through {function}: {key:?}",
                    path.display()
                );
            }
        }
    }
}

fn collect_rs_files(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("GUI source directory") {
        let path = entry.expect("GUI source entry").path();
        if path.is_dir() {
            collect_rs_files(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn literal_arguments(source: &str, function: &str) -> Vec<String> {
    let mut found = Vec::new();
    let marker = format!("{function}(");
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find(&marker) {
        let marker_start = cursor + relative;
        let preceded_by_identifier = marker_start > 0 && {
            let byte = source.as_bytes()[marker_start - 1];
            byte.is_ascii_alphanumeric() || byte == b'_'
        };
        if preceded_by_identifier {
            cursor = marker_start + marker.len();
            continue;
        }
        let mut index = marker_start + marker.len();
        while source
            .as_bytes()
            .get(index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            index += 1;
        }
        if source.as_bytes().get(index) != Some(&b'"') {
            cursor = index;
            continue;
        }
        index += 1;
        let mut value = String::new();
        while let Some(&byte) = source.as_bytes().get(index) {
            if byte == b'"' {
                found.push(value);
                index += 1;
                break;
            }
            if byte != b'\\' {
                value.push(byte as char);
                index += 1;
                continue;
            }
            index += 1;
            match source.as_bytes().get(index).copied() {
                Some(b'n') => value.push('\n'),
                Some(b'r') => value.push('\r'),
                Some(b't') => value.push('\t'),
                Some(b'"') => value.push('"'),
                Some(b'\\') => value.push('\\'),
                // A backslash before the line break continues the literal on
                // the next line; the break is LF or, on a CRLF checkout, CRLF.
                Some(b'\n') | Some(b'\r') => {
                    while source
                        .as_bytes()
                        .get(index)
                        .is_some_and(u8::is_ascii_whitespace)
                    {
                        index += 1;
                    }
                    continue;
                }
                Some(other) => value.push(other as char),
                None => break,
            }
            index += 1;
        }
        cursor = index;
    }
    found
}

/// The two optimizer policy groups Q11 and D02 surface in Advanced Settings.
///
/// The whole-schema test above is the standing contract, but it fails on
/// unrelated groups whose catalogue entries other work still owes, and a
/// failure there would hide a regression here. This one is scoped to the
/// groups this surface owns: the validity domain, its fourteen windows, and
/// the relaxation policy, label and help sentence alike.
#[test]
fn the_optimizer_policy_groups_have_spanish_catalog_provenance() {
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    let schema = alas_config::AlasConfig::default().schema();
    let optimizer = schema
        .fields
        .iter()
        .find(|field| field.name == "optimizer")
        .expect("the optimizer group reaches the form");
    let alas_config::Entry::Node(optimizer) = &optimizer.entry else {
        panic!("the optimizer group is a node");
    };

    let mut checked = 0;
    let mut missing = Vec::new();
    for group in ["plausibility", "relaxation"] {
        let field = optimizer
            .fields
            .iter()
            .find(|field| field.name == group)
            .unwrap_or_else(|| panic!("{group} reaches the form rather than being skipped"));
        let mut pending = vec![field];
        while let Some(field) = pending.pop() {
            for (kind, text) in [("label", field.label), ("help", field.help)] {
                if text.is_empty() {
                    missing.push(format!("{} has no {kind}", field.name));
                    continue;
                }
                checked += 1;
                if !base.contains_key(text) && !desktop.contains_key(text) {
                    missing.push(format!("{} {kind}: {text}", field.name));
                }
            }
            if let alas_config::Entry::Node(node) = &field.entry {
                pending.extend(node.fields.iter());
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
    // Two groups, fourteen plausibility windows and two relaxation controls,
    // each with a label and a help sentence.
    assert_eq!(checked, 2 * (2 + 14 + 2));
}

/// The two surfaces this work added: Q12's search-diagnostics block and
/// D01-D03's Inputs constraint-policy card.
///
/// Scoped for the same reason as the test above: the whole-source contract
/// stops at the first uncatalogued literal anywhere in the crate, so a
/// regression in these two files would be masked by an unrelated one.
#[test]
fn the_search_diagnostics_and_constraint_policy_views_have_spanish_provenance() {
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut total = 0;
    for relative in [
        "src/views/run_log/search_diagnostics.rs",
        "src/views/inputs_relaxation.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).expect("the view source");
        let mut keys = literal_arguments(&source, "tr");
        keys.extend(literal_arguments(&source, "tr_fields"));
        assert!(
            keys.len() >= 7,
            "{relative} sends almost nothing through the boundary, so this would pass vacuously: {keys:?}"
        );
        total += keys.len();
        for key in keys {
            assert!(
                base.contains_key(&key) || desktop.contains_key(&key),
                "{relative} sends an uncatalogued literal: {key:?}"
            );
        }
    }
    assert!(
        total >= 27,
        "expected every field label and note, got {total}"
    );
}

/// The surfaces the 2026-09-17 High GUI corrections added: the responsive
/// landing layout's notice, the reason a run is blocked, the schema form's
/// modification marker and the sandbox component context.
///
/// Scoped for the same reason as the two tests above: the whole-source
/// contract stops at the first uncatalogued literal anywhere in the crate,
/// so a regression in these files would be masked by an unrelated one.
#[test]
fn the_narrow_layout_run_gate_and_sandbox_context_views_have_spanish_provenance() {
    let base = alas_i18n::es::catalog();
    let desktop = alas_i18n::es::desktop_catalog();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut total = 0;
    for relative in [
        "src/app_parts/part_01.rs",
        "src/views/notices.rs",
        "src/views/form_feedback.rs",
        "src/views/form_parts/part_01.rs",
        "src/views/form_parts/part_02.rs",
        "src/views/control_bar.rs",
        "src/sandbox/panel.rs",
        "src/sandbox/viewport.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).expect("the view source");
        let mut keys = literal_arguments(&source, "tr");
        keys.extend(literal_arguments(&source, "tr_fields"));
        total += keys.len();
        for key in keys {
            assert!(
                base.contains_key(&key) || desktop.contains_key(&key),
                "{relative} sends an uncatalogued literal: {key:?}"
            );
        }
    }
    assert!(
        total >= 30,
        "expected every corrected surface's prose, got {total}"
    );
}
