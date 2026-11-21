// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A schema `help` string is a tooltip summary: it must fit the two lines the
//! tooltip shows. Anything longer belongs in `long_help`, which carries the full
//! explanation (sources, derivations, ranges) and is shown on request.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use alas_config::{AlasConfig, ConfigNode, Entry, Field};

/// The most characters a `help` summary may have. The settings interface
/// clamps a tooltip at the same length (`alas-gui`, `views::form::help`).
const HELP_MAX_CHARS: usize = 180;

fn walk(fields: &[Field], path: &str, violations: &mut Vec<String>, long_helps: &mut usize) {
    for field in fields {
        let here = format!("{path}.{}", field.name);
        let length = field.help.chars().count();
        if length > HELP_MAX_CHARS {
            violations.push(format!("{here}: help is {length} characters"));
        }
        if !field.long_help.is_empty() {
            *long_helps += 1;
            if length == 0 {
                violations.push(format!("{here}: long_help without a help summary"));
            }
            if field.long_help == field.help {
                violations.push(format!("{here}: long_help repeats help"));
            }
        }
        if let Entry::Node(node) = &field.entry {
            walk(&node.fields, &here, violations, long_helps);
        }
    }
}

#[test]
fn every_schema_help_fits_a_two_line_tooltip() {
    let schema = AlasConfig::default().schema();
    let mut violations = Vec::new();
    let mut long_helps = 0;
    walk(&schema.fields, "", &mut violations, &mut long_helps);
    assert!(
        violations.is_empty(),
        "help must be a summary of at most {HELP_MAX_CHARS} characters; move the rest to \
         `long_help`:\n{}",
        violations.join("\n")
    );
    assert!(long_helps > 0, "the long-help split is exercised");
}

#[test]
fn the_serialized_schema_carries_the_full_text() {
    let schema = AlasConfig::default().schema();
    let field = schema
        .field("analysis")
        .and_then(|field| match &field.entry {
            Entry::Node(node) => node.field("fine_chordwise_resolution"),
            Entry::Leaf(_) => None,
        })
        .expect("analysis.fine_chordwise_resolution");
    assert!(!field.long_help.is_empty());
    let json = serde_json::to_value(field).expect("serializes");
    assert_eq!(json["help"], field.long_help);
}
