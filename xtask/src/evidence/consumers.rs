// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Discovery of Rust code that consumes checked-in golden artifacts.

use super::{fixture_id, json_string_at, matching_delimiter, FixtureConsumer};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub(super) fn fixture_consumers(root: &Path) -> Result<BTreeSet<FixtureConsumer>, String> {
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Ok(BTreeSet::new());
    }
    let mut consumers = BTreeSet::new();
    let mut directories = vec![crates];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
        {
            let path = entry
                .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
                .path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                collect_file_consumers(root, &path, &mut consumers)?;
            }
        }
    }
    Ok(consumers)
}

fn collect_file_consumers(
    root: &Path,
    path: &Path,
    consumers: &mut BTreeSet<FixtureConsumer>,
) -> Result<(), String> {
    let source =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let source_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/");
    for (family, name, offset) in literal_fixture_calls(&source) {
        consumers.insert(FixtureConsumer {
            fixture: fixture_id(&family, &format!("{name}.json")),
            source_path: source_path.clone(),
            source_line: source_line(&source, offset),
        });
    }
    for (golden_path, offset) in included_golden_paths(&source) {
        record_golden_target(
            root,
            &golden_path,
            &source_path,
            source_line(&source, offset),
            consumers,
        )?;
    }
    for (golden_path, offset) in joined_golden_paths(&source) {
        record_golden_target(
            root,
            &golden_path,
            &source_path,
            source_line(&source, offset),
            consumers,
        )?;
    }
    Ok(())
}

fn record_golden_target(
    root: &Path,
    golden_path: &str,
    source_path: &str,
    source_line: usize,
    consumers: &mut BTreeSet<FixtureConsumer>,
) -> Result<(), String> {
    let Some((family, file)) = golden_path.split_once('/') else {
        return Ok(());
    };
    if family.is_empty() || file.is_empty() {
        return Ok(());
    }
    let target = root.join("golden").join(family).join(file);
    if target.is_dir() {
        let mut directories = vec![target];
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(&directory)
                .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
            {
                let path = entry
                    .map_err(|e| format!("cannot read {}: {e}", directory.display()))?
                    .path();
                if path.is_dir() {
                    directories.push(path);
                } else if path.file_name().and_then(|name| name.to_str()) != Some("manifest.json") {
                    let relative = path
                        .strip_prefix(root.join("golden").join(family))
                        .map_err(|e| format!("cannot classify {}: {e}", path.display()))?
                        .display()
                        .to_string()
                        .replace('\\', "/");
                    consumers.insert(FixtureConsumer {
                        fixture: fixture_id(family, &relative),
                        source_path: source_path.to_owned(),
                        source_line,
                    });
                }
            }
        }
    } else if !file.ends_with('/') {
        consumers.insert(FixtureConsumer {
            fixture: fixture_id(family, file),
            source_path: source_path.to_owned(),
            source_line,
        });
    }
    Ok(())
}

fn literal_fixture_calls(text: &str) -> Vec<(String, String, usize)> {
    let mut calls = Vec::new();
    for function in ["load_json", "load"] {
        let mut rest_start = 0;
        while let Some(relative) = text[rest_start..].find(function) {
            let start = rest_start + relative;
            rest_start = start + function.len();
            if start > 0 && is_rust_identifier(text.as_bytes()[start - 1]) {
                continue;
            }
            let Some(mut index) = next_non_whitespace(text, rest_start) else {
                continue;
            };
            if text.as_bytes().get(index) != Some(&b'(') {
                continue;
            }
            let Some(next) = next_non_whitespace(text, index + 1) else {
                continue;
            };
            index = next;
            let Some((family, after_family)) = json_string_at(text, index) else {
                continue;
            };
            let Some(comma) = next_non_whitespace(text, after_family) else {
                continue;
            };
            if text.as_bytes().get(comma) != Some(&b',') {
                continue;
            }
            let Some(name_start) = next_non_whitespace(text, comma + 1) else {
                continue;
            };
            if let Some((name, _)) = json_string_at(text, name_start) {
                calls.push((family, name, start));
            }
        }
    }
    calls
}

fn included_golden_paths(text: &str) -> Vec<(String, usize)> {
    let mut paths = Vec::new();
    for macro_name in ["include_str!", "include_bytes!"] {
        let mut rest_start = 0;
        while let Some(relative) = text[rest_start..].find(macro_name) {
            let start = rest_start + relative;
            rest_start = start + macro_name.len();
            let Some(open_relative) = text[rest_start..].find('(') else {
                break;
            };
            let open = rest_start + open_relative;
            let Some(close) = matching_delimiter(text, open, b'(', b')') else {
                continue;
            };
            let mut literal_text = String::new();
            let mut index = open + 1;
            while index < close {
                if text.as_bytes()[index] == b'"' {
                    if let Some((value, after)) = json_string_at(text, index) {
                        literal_text.push_str(&value);
                        index = after;
                        continue;
                    }
                }
                index += 1;
            }
            let normalized = literal_text.replace('\\', "/");
            if let Some(golden) = normalized.find("golden/") {
                paths.push((normalized[golden + "golden/".len()..].to_owned(), start));
            }
        }
    }
    paths
}

fn joined_golden_paths(text: &str) -> Vec<(String, usize)> {
    let mut paths = Vec::new();
    let mut rest_start = 0;
    while let Some(relative) = text[rest_start..].find("golden_dir()") {
        let start = rest_start + relative;
        let mut index = start + "golden_dir()".len();
        rest_start = index;
        let mut components = Vec::new();
        while let Some(join) = next_non_whitespace(text, index) {
            if !text[join..].starts_with(".join") {
                break;
            }
            let Some(open) = next_non_whitespace(text, join + ".join".len()) else {
                break;
            };
            if text.as_bytes().get(open) != Some(&b'(') {
                break;
            }
            let Some(value_start) = next_non_whitespace(text, open + 1) else {
                break;
            };
            let Some((value, after_value)) = json_string_at(text, value_start) else {
                break;
            };
            let Some(close) = next_non_whitespace(text, after_value) else {
                break;
            };
            if text.as_bytes().get(close) != Some(&b')') {
                break;
            }
            components.push(value);
            index = close + 1;
        }
        if components.len() >= 2 {
            paths.push((components.join("/"), start));
        }
    }
    paths
}

fn next_non_whitespace(text: &str, mut index: usize) -> Option<usize> {
    while text
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    (index < text.len()).then_some(index)
}

fn is_rust_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn source_line(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}
