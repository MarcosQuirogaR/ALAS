// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! GUI pages and windows, read from the navigation model in `alas-gui`.

use super::sources::{str_lit, SourceFile};
use std::collections::BTreeMap;
use syn::visit::Visit;

const NAV_FILE: &str = "crates/alas-gui/src/nav.rs";

#[derive(Clone, Default)]
pub struct PageInfo {
    pub id: String,
    pub title: String,
    pub kind: String,
    /// The configuration group a form page edits (top-level schema key).
    pub group: String,
    /// `all`, `modeling` or `advanced`.
    pub surface: String,
    /// Where the page is reachable: design flow, workspace, a page section or a standalone window.
    pub place: String,
    /// Extra cards: (node path, field names).
    pub extras: Vec<(String, Vec<String>)>,
}

pub fn pages(files: &[SourceFile]) -> Vec<PageInfo> {
    let Some(nav) = files.iter().find(|f| f.path == NAV_FILE) else {
        return Vec::new();
    };
    let mut visitor = Nav::default();
    for item in &nav.ast.items {
        visitor.visit_item(item);
    }
    let Nav {
        mut pages, extras, ..
    } = visitor;
    for page in &mut pages {
        if let Some(resolved) = extras.get(&page.place_extra) {
            page.info.extras.clone_from(resolved);
        }
    }
    pages.into_iter().map(|p| p.info).collect()
}

#[derive(Default)]
struct Found {
    info: PageInfo,
    place_extra: String,
}

#[derive(Default)]
struct Nav {
    pages: Vec<Found>,
    extras: BTreeMap<String, Vec<(String, Vec<String>)>>,
    current_const: String,
}

fn last_ident(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn place_of(const_name: &str) -> &'static str {
    match const_name {
        "ADVANCED_SETTINGS_PAGES" => "page section (Advanced) and sandbox window",
        "MODELING_PAGES" => "page section (Basic and Advanced)",
        "WORKSPACE_PAGES" => "workspace",
        "STANDALONE_PAGES" => "standalone window",
        _ => "design flow",
    }
}

fn base_page(call: &syn::ExprCall) -> Option<PageInfo> {
    let syn::Expr::Path(func) = &*call.func else {
        return None;
    };
    let segments: Vec<String> = func
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    if segments.len() != 2 || segments[0] != "Page" {
        return None;
    }
    let args: Vec<Option<String>> = call.args.iter().map(str_lit).collect();
    let arg = |i: usize| args.get(i).cloned().flatten().unwrap_or_default();
    let mut info = PageInfo {
        id: arg(0),
        title: arg(1),
        surface: "all".to_owned(),
        ..PageInfo::default()
    };
    match segments[1].as_str() {
        "form" => {
            "Form".clone_into(&mut info.kind);
            info.group = arg(2);
        }
        "screen" => {
            info.kind = call.args.get(2).map(last_ident).unwrap_or_default();
        }
        _ => return None,
    }
    Some(info)
}

impl Nav {
    fn push(&mut self, mut info: PageInfo, place_extra: String) {
        info.place = place_of(&self.current_const).to_owned();
        self.pages.push(Found { info, place_extra });
    }
}

impl<'ast> Visit<'ast> for Nav {
    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.current_const = node.ident.to_string();
        if let Some(sections) = extra_sections(&node.expr) {
            self.extras.insert(self.current_const.clone(), sections);
        }
        syn::visit::visit_item_const(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        if let Some(syn::Expr::Call(call)) = node.rest.as_deref() {
            if let Some(mut info) = base_page(call) {
                let mut extra = String::new();
                for field in &node.fields {
                    let syn::Member::Named(name) = &field.member else {
                        continue;
                    };
                    match name.to_string().as_str() {
                        "surface" => info.surface = last_ident(&field.expr).to_lowercase(),
                        "extra" => extra = last_ident(&field.expr),
                        _ => {}
                    }
                }
                self.push(info, extra);
                return;
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Some(info) = base_page(node) {
            self.push(info, String::new());
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn extra_sections(expr: &syn::Expr) -> Option<Vec<(String, Vec<String>)>> {
    let syn::Expr::Reference(reference) = expr else {
        return None;
    };
    let syn::Expr::Array(array) = &*reference.expr else {
        return None;
    };
    let mut out = Vec::new();
    for element in &array.elems {
        let syn::Expr::Struct(section) = element else {
            return None;
        };
        let mut path = String::new();
        let mut names = Vec::new();
        for field in &section.fields {
            let syn::Member::Named(name) = &field.member else {
                continue;
            };
            match name.to_string().as_str() {
                "path" => path = str_lit(&field.expr)?,
                "names" => {
                    if let syn::Expr::Reference(r) = &field.expr {
                        if let syn::Expr::Array(a) = &*r.expr {
                            names = a.elems.iter().filter_map(str_lit).collect();
                        }
                    }
                }
                _ => {}
            }
        }
        out.push((path, names));
    }
    Some(out)
}

/// The pages on which a field is reachable: (page id, window, level).
pub fn locations(pages: &[PageInfo], path: &str, advanced: bool) -> Vec<(String, String)> {
    let mut parts = path.splitn(2, '.');
    let group = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    let leaf = path.rsplit('.').next().unwrap_or_default();
    let node_pointer = format!(
        "/{}",
        path.rsplit_once('.')
            .map_or("", |(p, _)| p)
            .replace('.', "/")
    );
    let _ = rest;
    let mut out = Vec::new();
    for page in pages {
        let on_form = page.kind == "Form"
            && page.group == group
            && match page.surface.as_str() {
                "modeling" => !advanced,
                "advanced" => advanced,
                _ => true,
            };
        let on_extra = page
            .extras
            .iter()
            .any(|(p, names)| *p == node_pointer && names.iter().any(|n| n == leaf));
        if on_form || on_extra {
            out.push((page.id.clone(), page.place.clone()));
        }
    }
    out
}
