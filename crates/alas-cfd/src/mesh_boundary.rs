// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{BoundaryPatch, BoundaryPatchReport, MeshError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
/// The physical patch/type contract emitted by this module.
pub fn required_boundary_types() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("airfoil".to_owned(), "wall".to_owned()),
        ("farField".to_owned(), "patch".to_owned()),
        ("frontAndBack".to_owned(), "empty".to_owned()),
        ("inlet".to_owned(), "patch".to_owned()),
        ("outlet".to_owned(), "patch".to_owned()),
    ])
}

/// Inspect converted OpenFOAM patch names and types without changing the file.
pub fn inspect_boundary_patch_types(
    boundary_path: &Path,
) -> Result<BoundaryPatchReport, MeshError> {
    let contents = fs::read_to_string(boundary_path).map_err(|error| {
        MeshError::Io(format!("cannot read {}: {error}", boundary_path.display()))
    })?;
    parse_boundary_contents(&contents, false)
}

/// Enforce the physical patch contract in a converted OpenFOAM boundary file.
///
/// Only the `type` value inside the five required patch dictionaries is
/// updated.  Missing or duplicate dictionaries are errors.  A zero-face
/// unknown dictionary is retained and reported, while a nonzero unknown
/// dictionary is rejected because it means converted geometry escaped the
/// physical-group contract.
pub fn ensure_boundary_patch_types(boundary_path: &Path) -> Result<BoundaryPatchReport, MeshError> {
    let contents = fs::read_to_string(boundary_path).map_err(|error| {
        MeshError::Io(format!("cannot read {}: {error}", boundary_path.display()))
    })?;
    let mut tokens = lex_boundary(&contents)?;
    let required = required_boundary_types();
    let mut replacements = Vec::new();
    for (name, expected_type) in &required {
        let block = find_named_block(&tokens, name)?;
        let type_token = find_field_value(&tokens, block, "type")
            .ok_or_else(|| MeshError::Parse(format!("patch '{name}' has no type entry")))?;
        if tokens[type_token].text != *expected_type {
            replacements.push((
                tokens[type_token].start,
                tokens[type_token].end,
                expected_type.clone(),
            ));
        }
    }
    let updated = !replacements.is_empty();
    let mut rewritten = contents;
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in replacements {
        rewritten.replace_range(start..end, &replacement);
    }
    if updated {
        fs::write(boundary_path, &rewritten).map_err(|error| {
            MeshError::Io(format!(
                "cannot rewrite {}: {error}",
                boundary_path.display()
            ))
        })?;
    }
    tokens = lex_boundary(&rewritten_after_write(boundary_path, &rewritten, updated)?)?;
    let mut report = parse_boundary_tokens(&tokens, &required)?;
    validate_required_patch_faces(&report)?;
    reject_nonzero_unknown_patches(&tokens, &report)?;
    report.updated = updated;
    Ok(report)
}

fn rewritten_after_write(path: &Path, rewritten: &str, updated: bool) -> Result<String, MeshError> {
    if updated {
        Ok(rewritten.to_owned())
    } else {
        fs::read_to_string(path)
            .map_err(|error| MeshError::Io(format!("cannot reread {}: {error}", path.display())))
    }
}
#[derive(Debug, Clone, Copy)]
struct NamedBlock {
    open: usize,
    close: usize,
}

#[derive(Debug, Clone)]
struct BoundaryToken {
    text: String,
    start: usize,
    end: usize,
}
fn lex_boundary(contents: &str) -> Result<Vec<BoundaryToken>, MeshError> {
    let bytes = contents.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            let start = index;
            index += 2;
            let mut closed = false;
            while index + 1 < bytes.len() {
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(MeshError::Parse(format!(
                    "unterminated block comment beginning at byte {start}"
                )));
            }
            continue;
        }
        let start = index;
        if bytes[index] == b'"' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            tokens.push(BoundaryToken {
                text: contents[start..index].to_owned(),
                start,
                end: index,
            });
            continue;
        }
        if matches!(bytes[index], b'{' | b'}' | b'(' | b')' | b';' | b'[' | b']') {
            index += 1;
            tokens.push(BoundaryToken {
                text: contents[start..index].to_owned(),
                start,
                end: index,
            });
            continue;
        }
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'{' | b'}' | b'(' | b')' | b';' | b'[' | b']')
        {
            if bytes[index] == b'/'
                && index + 1 < bytes.len()
                && matches!(bytes[index + 1], b'/' | b'*')
            {
                break;
            }
            index += 1;
        }
        if start == index {
            return Err(MeshError::Parse(format!(
                "cannot lex boundary file near byte {index}"
            )));
        }
        tokens.push(BoundaryToken {
            text: contents[start..index].to_owned(),
            start,
            end: index,
        });
    }
    Ok(tokens)
}

fn find_named_block(tokens: &[BoundaryToken], name: &str) -> Result<NamedBlock, MeshError> {
    let mut found = None;
    for index in 0..tokens.len().saturating_sub(1) {
        if tokens[index].text == name && tokens[index + 1].text == "{" {
            if found.is_some() {
                return Err(MeshError::Parse(format!(
                    "boundary file contains duplicate patch dictionary '{name}'"
                )));
            }
            let mut depth = 1_usize;
            let mut close = None;
            for (offset, token) in tokens.iter().enumerate().skip(index + 2) {
                if token.text == "{" {
                    depth += 1;
                } else if token.text == "}" {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(offset);
                        break;
                    }
                }
            }
            let Some(close) = close else {
                return Err(MeshError::Parse(format!(
                    "patch '{name}' has an unterminated dictionary"
                )));
            };
            found = Some(NamedBlock {
                open: index + 1,
                close,
            });
        }
    }
    found.ok_or_else(|| MeshError::Parse(format!("required patch '{name}' is missing")))
}

fn find_field_value(
    tokens: &[BoundaryToken],
    block: NamedBlock,
    field_name: &str,
) -> Option<usize> {
    let mut index = block.open + 1;
    while index + 1 < block.close {
        if tokens[index].text == field_name {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

fn parse_boundary_contents(
    contents: &str,
    updated: bool,
) -> Result<BoundaryPatchReport, MeshError> {
    let tokens = lex_boundary(contents)?;
    let required = required_boundary_types();
    let mut report = parse_boundary_tokens(&tokens, &required)?;
    report.updated = updated;
    Ok(report)
}

fn parse_boundary_tokens(
    tokens: &[BoundaryToken],
    required: &BTreeMap<String, String>,
) -> Result<BoundaryPatchReport, MeshError> {
    let mut patches = Vec::new();
    for name in required.keys() {
        let block = find_named_block(tokens, name)?;
        let patch_type = find_field_value(tokens, block, "type")
            .map(|index| tokens[index].text.clone())
            .ok_or_else(|| MeshError::Parse(format!("patch '{name}' has no type entry")))?;
        let physical_type =
            find_field_value(tokens, block, "physicalType").map(|index| tokens[index].text.clone());
        let n_faces = find_field_value(tokens, block, "nFaces")
            .and_then(|index| tokens[index].text.parse::<u64>().ok());
        patches.push(BoundaryPatch {
            name: name.clone(),
            patch_type,
            physical_type,
            n_faces,
        });
    }
    let expected_names = required.keys().collect::<Vec<_>>();
    let mut unknown_patches = Vec::new();
    for index in 0..tokens.len().saturating_sub(1) {
        if tokens[index + 1].text != "{" || tokens[index].text == "FoamFile" {
            continue;
        }
        let name = tokens[index].text.clone();
        if !expected_names.iter().any(|expected| **expected == name) {
            unknown_patches.push(name);
        }
    }
    unknown_patches.sort();
    unknown_patches.dedup();
    Ok(BoundaryPatchReport {
        patches,
        required_types: required.clone(),
        updated: false,
        unknown_patches,
    })
}

fn validate_required_patch_faces(report: &BoundaryPatchReport) -> Result<(), MeshError> {
    for patch in &report.patches {
        match patch.n_faces {
            Some(count) if count > 0 => {}
            Some(_) => {
                return Err(MeshError::Parse(format!(
                    "required patch '{}' has zero nFaces",
                    patch.name
                )))
            }
            None => {
                return Err(MeshError::Parse(format!(
                    "required patch '{}' has no valid nFaces entry",
                    patch.name
                )))
            }
        }
    }
    Ok(())
}

fn reject_nonzero_unknown_patches(
    tokens: &[BoundaryToken],
    report: &BoundaryPatchReport,
) -> Result<(), MeshError> {
    for name in &report.unknown_patches {
        let block = find_named_block(tokens, name)?;
        let count = find_field_value(tokens, block, "nFaces")
            .ok_or_else(|| MeshError::Parse(format!("unexpected patch '{name}' has no nFaces")))?;
        let count = tokens[count].text.parse::<u64>().map_err(|_| {
            MeshError::Parse(format!("unexpected patch '{name}' has invalid nFaces"))
        })?;
        if count > 0 {
            return Err(MeshError::Parse(format!(
                "unexpected patch '{name}' has {count} faces"
            )));
        }
    }
    Ok(())
}
