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
                Some(b'\n') => {
                    index += 1;
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
