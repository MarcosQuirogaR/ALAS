// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parsed workspace sources shared by every capability-map scanner.

use std::path::Path;

/// One parsed Rust source file of a workspace crate.
pub struct SourceFile {
    /// Path relative to the repository root, with forward slashes.
    pub path: String,
    /// The crate directory name under `crates/`.
    pub krate: String,
    /// Whether the whole file is test support (a `tests` directory or module file).
    pub test_file: bool,
    pub ast: syn::File,
}

/// Parse every `crates/**/*.rs` file; files that are not valid Rust are listed, not fatal.
pub fn load(root: &Path) -> Result<(Vec<SourceFile>, Vec<String>), String> {
    let mut paths = Vec::new();
    collect(&root.join("crates"), &mut paths)?;
    paths.sort();
    let mut files = Vec::new();
    let mut unparsed = Vec::new();
    for path in paths {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {rel}: {e}"))?;
        match syn::parse_file(&text) {
            Ok(ast) => {
                let krate = rel.split('/').nth(1).unwrap_or_default().to_owned();
                let test_file = is_test_file(&rel);
                files.push(SourceFile {
                    path: rel,
                    krate,
                    test_file,
                    ast,
                });
            }
            Err(_) => unparsed.push(rel),
        }
    }
    Ok((files, unparsed))
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or_default();
    rel.contains("/tests/")
        || rel.contains("/benches/")
        || rel.contains("/examples/")
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.starts_with("tests_")
        || rel.starts_with("crates/alas-testkit/")
}

/// Whether the attributes mark an item as compiled only for tests.
pub fn is_test_item(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let text = match &attr.meta {
            syn::Meta::List(list) => list.tokens.to_string(),
            _ => String::new(),
        };
        let words: Vec<&str> = text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .collect();
        words.contains(&"test") && !words.contains(&"not")
    })
}

pub fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// The text of a string literal expression, if it is one.
pub fn str_lit(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Some(s.value()),
        syn::Expr::Reference(r) => str_lit(&r.expr),
        _ => None,
    }
}

/// The first line of a field's or item's doc comment.
pub fn doc_line(attrs: &[syn::Attribute]) -> String {
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let Some(text) = str_lit(&nv.value) {
                let text = text.trim();
                if !text.is_empty() {
                    return text.to_owned();
                }
            }
        }
    }
    String::new()
}
