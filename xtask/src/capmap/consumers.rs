// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Who reads a configuration field.
//!
//! A read is a field access (`x.name`), a destructuring pattern, a no-argument
//! method of the same name, or a string literal spelling the field's full path.
//! Assignments to a field and struct literals are not reads. Matching is by
//! field name, so a name shared by several configuration structs is attributed
//! to all of them; the map flags those fields as `shared_name`.
//!
//! `alas-config` holds real model code (resolved shapes, speed caps, cabin
//! shares) beside its loading and validation code. A read inside a function of
//! `alas-config` counts as use only when the function is not validation or
//! loading and its name is called, directly or through other `alas-config`
//! functions, from a model or pipeline crate. This is name-based, so it can
//! over-approximate; it cannot see a method that nothing calls by name.

use super::sources::{is_test_item, item_attrs, SourceFile};
use proc_macro2::{TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

/// What kind of code a reading file belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Role {
    /// Models and pipeline stages: the code that turns configuration into results.
    Compute,
    /// `alas-gui`: editing and display.
    Gui,
    /// `alas-config` itself: validation, presets, loading, resolved shapes.
    Config,
    /// Acceptance and test support crates; never counts as use.
    Harness,
}

impl Role {
    pub fn of_crate(krate: &str) -> Self {
        match krate {
            "alas-gui" | "alas-i18n" | "alas-viz" | "alas-fonts" => Self::Gui,
            "alas-config" | "alas-config-derive" => Self::Config,
            "alas-acceptance" | "alas-testkit" => Self::Harness,
            _ => Self::Compute,
        }
    }
}

/// Reads keyed by field name or full path: key -> role -> reading files.
pub type Reads = BTreeMap<String, BTreeMap<Role, BTreeSet<String>>>;

/// Everything the scan learned about field use.
#[derive(Default)]
pub struct Scan {
    pub reads: Reads,
    pub path_reads: Reads,
    /// Field name -> `alas-config` functions that read it outside validation and loading.
    pub model_fns: BTreeMap<String, BTreeSet<String>>,
    /// `alas-config` function -> names it calls.
    pub config_calls: BTreeMap<String, BTreeSet<String>>,
    /// Names called from model and pipeline crates.
    pub compute_calls: BTreeSet<String>,
}

impl Scan {
    /// Functions of `alas-config` reachable from model or pipeline code, by name.
    pub fn reached_fns(&self) -> BTreeSet<String> {
        let mut reached = self.compute_calls.clone();
        let mut queue: Vec<String> = reached.iter().cloned().collect();
        while let Some(name) = queue.pop() {
            for callee in self.config_calls.get(&name).into_iter().flatten() {
                if reached.insert(callee.clone()) {
                    queue.push(callee.clone());
                }
            }
        }
        reached
    }
}

/// Files of `alas-config` whose reads are loading, validation or preset application.
fn is_loading_file(path: &str) -> bool {
    [
        "/presets/",
        "/preset_",
        "validation",
        "overlay",
        "schema",
        "retired_keys",
        "settings_load_notes",
    ]
    .iter()
    .any(|part| path.contains(part))
}

fn is_loading_fn(name: &str) -> bool {
    [
        "validate",
        "check",
        "default",
        "is_default",
        "from_",
        "apply",
        "retired",
        "migrate",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

pub fn scan(files: &[SourceFile], names: &BTreeSet<String>, paths: &BTreeSet<String>) -> Scan {
    let mut out = Scan::default();
    for file in files {
        if file.test_file {
            continue;
        }
        let role = Role::of_crate(&file.krate);
        if role == Role::Harness {
            continue;
        }
        let mut finder = Finder::new(names, paths);
        for item in &file.ast.items {
            if !is_test_item(item_attrs(item)) {
                finder.visit_item(item);
            }
        }
        if role == Role::Compute {
            out.compute_calls.extend(finder.all_calls.iter().cloned());
        }
        if role == Role::Config {
            for (caller, callees) in finder.calls {
                out.config_calls.entry(caller).or_default().extend(callees);
            }
            if !is_loading_file(&file.path) {
                for (field, fns) in finder.fn_reads {
                    let kept = fns.into_iter().filter(|f| !is_loading_fn(f));
                    out.model_fns.entry(field).or_default().extend(kept);
                }
            }
        }
        for name in finder.found {
            record(&mut out.reads, name, role, &file.path);
        }
        for path in finder.literals {
            record(&mut out.path_reads, path, role, &file.path);
        }
    }
    out
}

fn record(reads: &mut Reads, key: String, role: Role, file: &str) {
    reads
        .entry(key)
        .or_default()
        .entry(role)
        .or_default()
        .insert(file.to_owned());
}

struct Finder<'a> {
    names: &'a BTreeSet<String>,
    paths: &'a BTreeSet<String>,
    found: BTreeSet<String>,
    literals: BTreeSet<String>,
    fn_stack: Vec<String>,
    calls: BTreeMap<String, BTreeSet<String>>,
    all_calls: BTreeSet<String>,
    fn_reads: BTreeMap<String, BTreeSet<String>>,
}

impl<'a> Finder<'a> {
    fn new(names: &'a BTreeSet<String>, paths: &'a BTreeSet<String>) -> Self {
        Self {
            names,
            paths,
            found: BTreeSet::new(),
            literals: BTreeSet::new(),
            fn_stack: Vec::new(),
            calls: BTreeMap::new(),
            all_calls: BTreeSet::new(),
            fn_reads: BTreeMap::new(),
        }
    }

    fn note(&mut self, ident: &str) {
        if self.names.contains(ident) {
            self.found.insert(ident.to_owned());
            if let Some(current) = self.fn_stack.last() {
                self.fn_reads
                    .entry(ident.to_owned())
                    .or_default()
                    .insert(current.clone());
            }
        }
    }

    fn note_call(&mut self, name: &str) {
        self.all_calls.insert(name.to_owned());
        if let Some(current) = self.fn_stack.last() {
            self.calls
                .entry(current.clone())
                .or_default()
                .insert(name.to_owned());
        }
    }

    fn note_literal(&mut self, text: &str) {
        let dotted = text.trim_start_matches('/').replace('/', ".");
        if self.paths.contains(&dotted) {
            self.literals.insert(dotted);
        }
    }

    fn scan_tokens(&mut self, tokens: TokenStream) {
        let mut previous_dot = false;
        let mut iter = tokens.into_iter().peekable();
        while let Some(tree) = iter.next() {
            match tree {
                TokenTree::Punct(p) if p.as_char() == '.' => {
                    previous_dot = true;
                    continue;
                }
                TokenTree::Ident(ident) if previous_dot => {
                    let call = matches!(
                        iter.peek(),
                        Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis
                    );
                    if call {
                        self.note_call(&ident.to_string());
                    } else {
                        self.note(&ident.to_string());
                    }
                }
                TokenTree::Group(group) => self.scan_tokens(group.stream()),
                TokenTree::Literal(lit) => {
                    let text = lit.to_string();
                    if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                        self.note_literal(inner);
                    }
                }
                _ => {}
            }
            previous_dot = false;
        }
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if is_test_item(item_attrs(item)) {
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        if let syn::Member::Named(ident) = &node.member {
            self.note(&ident.to_string());
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        if let syn::Expr::Field(target) = &*node.left {
            self.visit_expr(&target.base);
        } else {
            self.visit_expr(&node.left);
        }
        self.visit_expr(&node.right);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.args.is_empty() {
            self.note(&node.method.to_string());
        }
        self.note_call(&node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            if let Some(last) = path.path.segments.last() {
                self.note_call(&last.ident.to_string());
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_field_pat(&mut self, node: &'ast syn::FieldPat) {
        if let syn::Member::Named(ident) = &node.member {
            self.note(&ident.to_string());
        }
        syn::visit::visit_field_pat(self, node);
    }

    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.note_literal(&node.value());
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.scan_tokens(node.tokens.clone());
    }
}
