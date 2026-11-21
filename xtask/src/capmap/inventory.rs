// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Inventories of the capability families that are not configuration fields.

use super::sources::{doc_line, is_test_item, str_lit, SourceFile};
use proc_macro2::{TokenStream, TokenTree};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

fn file<'a>(files: &'a [SourceFile], path: &str) -> Option<&'a SourceFile> {
    files.iter().find(|f| f.path == path)
}

/// Pipeline stages, from the `Stage N/M: text` progress reports.
pub fn pipeline_stages(files: &[SourceFile]) -> Vec<Value> {
    let path = "crates/alas-pipeline/src/pipeline.rs";
    let Some(source) = file(files, path) else {
        return Vec::new();
    };
    let mut lits = LitCollector::default();
    for item in &source.ast.items {
        lits.visit_item(item);
    }
    let mut stages: BTreeMap<u32, String> = BTreeMap::new();
    for text in &lits.lits {
        let Some(rest) = text.strip_prefix("Stage ") else {
            continue;
        };
        let Some((number, tail)) = rest.split_once('/') else {
            continue;
        };
        let Ok(number) = number.parse::<u32>() else {
            continue;
        };
        let label = tail.split_once(": ").map_or(tail, |(_, l)| l);
        stages.entry(number).or_insert_with(|| label.to_owned());
    }
    stages
        .into_iter()
        .map(|(n, label)| json!({"stage": n, "label": label, "source": path}))
        .collect()
}

#[derive(Default)]
struct LitCollector {
    lits: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for LitCollector {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if matches!(item, syn::Item::Mod(m) if is_test_item(&m.attrs)) {
            return;
        }
        syn::visit::visit_item(self, item);
    }
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.lits.insert(node.value());
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        walk_tokens(node.tokens.clone(), &mut |text| {
            self.lits.insert(text);
        });
    }
}

fn walk_tokens(tokens: TokenStream, sink: &mut dyn FnMut(String)) {
    for tree in tokens {
        match tree {
            TokenTree::Group(g) => walk_tokens(g.stream(), sink),
            TokenTree::Literal(l) => {
                if let Ok(lit) = syn::parse_str::<syn::LitStr>(&l.to_string()) {
                    sink(lit.value());
                }
            }
            _ => {}
        }
    }
}

/// Report figures (also the plot-data export ids): every `descriptor(id, title, category, ..)`.
pub fn figures(files: &[SourceFile]) -> Vec<Value> {
    let path = "crates/alas-report/src/registry/figures.rs";
    let Some(source) = file(files, path) else {
        return Vec::new();
    };
    let mut visitor = Figures::default();
    for item in &source.ast.items {
        visitor.visit_item(item);
    }
    visitor.out
}

#[derive(Default)]
struct Figures {
    table: String,
    out: Vec<Value>,
}

impl<'ast> Visit<'ast> for Figures {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.table = node.ident.to_string();
        syn::visit::visit_item_static(self, node);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let name = match &*node.func {
            syn::Expr::Path(p) => p.path.get_ident().map(ToString::to_string),
            _ => None,
        };
        if matches!(name.as_deref(), Some("descriptor" | "preview_descriptor")) {
            let args: Vec<String> = node.args.iter().filter_map(str_lit).collect();
            if args.len() >= 4 {
                self.out.push(json!({
                    "id": args[0], "title": args[1], "category": args[2],
                    "table": self.table, "preview_only": name.as_deref() == Some("preview_descriptor"),
                }));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

/// External tools: the resolved-run environment, user preferences and `ALAS_*` overrides.
pub fn external_tools(files: &[SourceFile]) -> Value {
    let types = "crates/alas-exec/src/tools/types.rs";
    let mut environment = Vec::new();
    let mut preferences = Vec::new();
    if let Some(source) = file(files, types) {
        for item in &source.ast.items {
            if let syn::Item::Struct(s) = item {
                let target = match s.ident.to_string().as_str() {
                    "RunEnvironment" => &mut environment,
                    "ToolPreferences" => &mut preferences,
                    _ => continue,
                };
                if let syn::Fields::Named(named) = &s.fields {
                    for field in &named.named {
                        if let Some(ident) = &field.ident {
                            target.push(
                                json!({"name": ident.to_string(), "doc": doc_line(&field.attrs)}),
                            );
                        }
                    }
                }
            }
        }
    }
    let mut env_vars: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for source in files
        .iter()
        .filter(|f| f.krate == "alas-exec" && !f.test_file)
    {
        let mut lits = LitCollector::default();
        for item in &source.ast.items {
            lits.visit_item(item);
        }
        for text in lits.lits {
            if text.starts_with("ALAS_")
                && text.len() > 5
                && text
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                env_vars
                    .entry(text)
                    .or_default()
                    .insert(source.path.clone());
            }
        }
    }
    json!({
        "run_environment": environment,
        "preferences": preferences,
        "environment_variables": env_vars.into_iter()
            .map(|(name, files)| json!({"name": name, "files": files}))
            .collect::<Vec<_>>(),
        "source": types,
    })
}

/// Command-line flags, from the argument parser's `match` arms.
pub fn cli_flags(files: &[SourceFile]) -> Vec<Value> {
    let path = "crates/alas-app/src/cli/args.rs";
    let Some(source) = file(files, path) else {
        return Vec::new();
    };
    let mut visitor = Flags::default();
    for item in &source.ast.items {
        visitor.visit_item(item);
    }
    visitor.out
}

#[derive(Default)]
struct Flags {
    out: Vec<Value>,
}

impl<'ast> Visit<'ast> for Flags {
    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        for arm in &node.arms {
            let mut names = Vec::new();
            collect_flag_names(&arm.pat, &mut names);
            if names.is_empty() {
                continue;
            }
            let mut body = Lits::default();
            body.visit_expr(&arm.body);
            let takes_value = body.calls_flag_value;
            self.out
                .push(json!({"flags": names, "takes_value": takes_value}));
        }
        syn::visit::visit_expr_match(self, node);
    }
}

#[derive(Default)]
struct Lits {
    calls_flag_value: bool,
}

impl<'ast> Visit<'ast> for Lits {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if matches!(&*node.func, syn::Expr::Path(p) if p.path.is_ident("flag_value")) {
            self.calls_flag_value = true;
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn collect_flag_names(pat: &syn::Pat, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Or(or) => or.cases.iter().for_each(|p| collect_flag_names(p, out)),
        syn::Pat::Lit(lit) => {
            if let syn::Lit::Str(s) = &lit.lit {
                if s.value().starts_with('-') {
                    out.push(s.value());
                }
            }
        }
        _ => {}
    }
}

/// Add-on manifest surface: kinds, manifest keys and the documented target grammar.
pub fn addons(files: &[SourceFile]) -> Value {
    let path = "crates/alas-addons/src/manifest.rs";
    let mut keys = Vec::new();
    let mut kinds = Vec::new();
    if let Some(source) = file(files, path) {
        for item in &source.ast.items {
            match item {
                syn::Item::Struct(s) if s.ident == "AddonManifest" => {
                    if let syn::Fields::Named(named) = &s.fields {
                        for field in &named.named {
                            if let Some(ident) = &field.ident {
                                keys.push(json!({"key": ident.to_string(), "doc": doc_line(&field.attrs)}));
                            }
                        }
                    }
                }
                syn::Item::Enum(e) if e.ident == "AddonKind" => {
                    kinds = e.variants.iter().map(|v| v.ident.to_string()).collect();
                }
                _ => {}
            }
        }
    }
    json!({"manifest_keys": keys, "kinds": kinds, "source": path})
}

/// File types the app reads or writes: dialog filters and named file literals by extension.
pub fn file_types(files: &[SourceFile]) -> Value {
    const EXTENSIONS: &[&str] = &[
        "json", "yaml", "yml", "csv", "svg", "pdf", "png", "xml", "txt", "md", "html", "toml",
        "kml", "vsp3", "dat", "bdf", "obj", "stl", "zip", "cpacs", "rhai", "py", "geo", "avl",
    ];
    let mut by_extension: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for source in files.iter().filter(|f| !f.test_file) {
        if !matches!(
            source.krate.as_str(),
            "alas-app"
                | "alas-gui"
                | "alas-pipeline"
                | "alas-exec"
                | "alas-addons"
                | "alas-report"
                | "alas-route"
                | "alas-config"
        ) {
            continue;
        }
        let mut lits = LitCollector::default();
        for item in &source.ast.items {
            lits.visit_item(item);
        }
        for text in lits.lits {
            let Some((stem, ext)) = text.rsplit_once('.') else {
                continue;
            };
            if stem.is_empty()
                || !EXTENSIONS.contains(&ext)
                || !stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                continue;
            }
            let entry = by_extension.entry(ext.to_owned()).or_default();
            entry.0.insert(text);
            entry.1.insert(source.krate.clone());
        }
    }
    let entries: Vec<Value> = by_extension
        .into_iter()
        .map(|(ext, (names, crates))| {
            let sample: Vec<String> = names.iter().take(8).cloned().collect();
            json!({"extension": ext, "named_files": names.len(), "examples": sample, "crates": crates})
        })
        .collect();
    json!({"by_extension": entries, "config_dialog_filter": ["json", "yaml", "yml"]})
}

/// Capability smoke tests: `#[test]` functions named `capability_smoke_<family>_...`.
pub fn smoke_tests(files: &[SourceFile]) -> Vec<Value> {
    let mut visitor = Smokes::default();
    for source in files {
        visitor.file = source.path.clone();
        visitor.krate = source.krate.clone();
        syn::visit::visit_file(&mut visitor, &source.ast);
    }
    visitor.out.sort_by_key(|a| a.to_string());
    visitor.out
}

#[derive(Default)]
struct Smokes {
    file: String,
    krate: String,
    out: Vec<Value>,
}

impl<'ast> Visit<'ast> for Smokes {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        if let Some(rest) = name.strip_prefix("capability_smoke_") {
            let family = rest.split('_').next().unwrap_or_default().to_owned();
            self.out.push(json!({
                "test": name, "family": family, "crate": self.krate, "file": self.file,
            }));
        }
        syn::visit::visit_item_fn(self, node);
    }
}
