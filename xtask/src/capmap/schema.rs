// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The configuration schema, read from `#[derive(ConfigNode)]` structs.

use super::sources::{doc_line, SourceFile};
use std::collections::BTreeMap;

const ROOT_TYPE: &str = "AlasConfig";

/// One `#[config(...)]` field as written in source.
#[derive(Clone, Default)]
pub struct RawField {
    pub name: String,
    pub rust_type: String,
    pub nested_type: Option<String>,
    pub skip: bool,
    pub hidden: bool,
    pub advanced: bool,
    pub label: String,
    pub unit: String,
    pub help: String,
    pub min: String,
    pub max: String,
}

pub struct RawStruct {
    pub file: String,
    pub fields: Vec<RawField>,
}

/// A leaf (non-nested) configuration field at its place in the tree.
#[derive(Clone)]
pub struct Leaf {
    /// Dotted path from the root, for example `geometry.wing.span`.
    pub path: String,
    pub field: RawField,
    /// Declaring file, relative to the repository root.
    pub file: String,
    /// The field or an ancestor group is shown only with advanced settings.
    pub advanced: bool,
}

pub fn collect_structs(files: &[SourceFile]) -> BTreeMap<String, RawStruct> {
    let mut out = BTreeMap::new();
    for file in files {
        let mut found = Vec::new();
        gather(&file.ast.items, &mut found);
        for item in found {
            out.insert(
                item.ident.to_string(),
                RawStruct {
                    file: file.path.clone(),
                    fields: struct_fields(item),
                },
            );
        }
    }
    out
}

fn gather<'a>(items: &'a [syn::Item], out: &mut Vec<&'a syn::ItemStruct>) {
    for item in items {
        match item {
            syn::Item::Struct(s) if derives_config_node(&s.attrs) => out.push(s),
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    gather(inner, out);
                }
            }
            _ => {}
        }
    }
}

fn derives_config_node(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string().contains("ConfigNode"))
    })
}

fn struct_fields(item: &syn::ItemStruct) -> Vec<RawField> {
    let syn::Fields::Named(named) = &item.fields else {
        return Vec::new();
    };
    named.named.iter().filter_map(raw_field).collect()
}

fn raw_field(field: &syn::Field) -> Option<RawField> {
    let ident = field.ident.as_ref()?;
    let mut raw = RawField {
        name: ident.to_string(),
        rust_type: type_text(&field.ty),
        help: doc_line(&field.attrs),
        ..RawField::default()
    };
    for attr in &field.attrs {
        if !attr.path().is_ident("config") {
            continue;
        }
        // Unknown keys cannot occur: the derive macro rejects them at build time.
        let _ = attr.parse_nested_meta(|meta| {
            let key = meta.path.get_ident().map(ToString::to_string);
            match key.as_deref() {
                Some("skip") => raw.skip = true,
                Some("hidden") => raw.hidden = true,
                Some("advanced") => raw.advanced = true,
                Some("nested") => raw.nested_type = Some(type_text(&field.ty)),
                Some("label") => raw.label = lit_text(&meta)?,
                Some("unit") => raw.unit = lit_text(&meta)?,
                Some("help") => raw.help = lit_text(&meta)?,
                Some("min") => raw.min = lit_text(&meta)?,
                Some("max") => raw.max = lit_text(&meta)?,
                Some("readonly_unless") => {
                    let _ = meta.parse_nested_meta(|inner| {
                        let _: syn::Lit = inner.value()?.parse()?;
                        Ok(())
                    });
                }
                _ => {
                    if meta.input.peek(syn::Token![=]) {
                        let _: syn::Expr = meta.value()?.parse()?;
                    }
                }
            }
            Ok(())
        });
    }
    Some(raw)
}

fn lit_text(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    let lit: syn::Lit = meta.value()?.parse()?;
    Ok(match lit {
        syn::Lit::Str(s) => s.value(),
        syn::Lit::Int(i) => i.base10_digits().to_owned(),
        syn::Lit::Float(f) => f.base10_digits().to_owned(),
        _ => String::new(),
    })
}

fn type_text(ty: &syn::Type) -> String {
    quote::ToTokens::to_token_stream(ty)
        .to_string()
        .replace(" ", "")
}

/// Flatten the tree under [`ROOT_TYPE`] into leaves, in declaration order.
pub fn leaves(structs: &BTreeMap<String, RawStruct>) -> Result<Vec<Leaf>, String> {
    if !structs.contains_key(ROOT_TYPE) {
        return Err(format!("no ConfigNode struct named {ROOT_TYPE} found"));
    }
    let mut out = Vec::new();
    walk(structs, ROOT_TYPE, "", false, 0, &mut out)?;
    Ok(out)
}

fn walk(
    structs: &BTreeMap<String, RawStruct>,
    ty: &str,
    prefix: &str,
    advanced: bool,
    depth: usize,
    out: &mut Vec<Leaf>,
) -> Result<(), String> {
    if depth > 12 {
        return Err(format!("configuration nesting under {prefix} is too deep"));
    }
    let node = structs
        .get(ty)
        .ok_or_else(|| format!("nested ConfigNode type {ty} (at {prefix}) was not found"))?;
    for field in &node.fields {
        if field.skip {
            continue;
        }
        let path = if prefix.is_empty() {
            field.name.clone()
        } else {
            format!("{prefix}.{}", field.name)
        };
        let adv = advanced || field.advanced;
        if let Some(nested) = &field.nested_type {
            walk(structs, nested, &path, adv, depth + 1, out)?;
        } else {
            out.push(Leaf {
                path,
                field: field.clone(),
                file: node.file.clone(),
                advanced: adv,
            });
        }
    }
    Ok(())
}
