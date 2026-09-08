// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Count production source including textual splices, with explicit legacy budgets.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::{spanned::Spanned, visit::Visit};

const LIMIT: usize = 500;

#[derive(Default)]
struct Source {
    lines: usize,
    includes: Vec<PathBuf>,
    test_includes: Vec<PathBuf>,
}

#[derive(Default)]
struct Items {
    excluded: BTreeSet<usize>,
    includes: Vec<String>,
    test_includes: Vec<String>,
    in_test: bool,
    errors: Vec<String>,
}

impl<'ast> Visit<'ast> for Items {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let was_test = self.in_test;
        self.in_test |= item_attrs(item).iter().any(test_only);
        if self.in_test {
            self.excluded
                .extend(item.span().start().line..=item.span().end().line);
        }
        syn::visit::visit_item(self, item);
        self.in_test = was_test;
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node.path.is_ident("include") {
            self.record_include(node.tokens.clone());
        } else {
            self.scan_tokens(node.tokens.clone());
        }
    }
}

impl Items {
    fn record_include(&mut self, tokens: proc_macro2::TokenStream) {
        match syn::parse2::<syn::LitStr>(tokens) {
            Ok(path) if self.in_test => self.test_includes.push(path.value()),
            Ok(path) => self.includes.push(path.value()),
            Err(_) if self.in_test => {}
            Err(_) => self
                .errors
                .push("include! requires a literal path for source accounting".into()),
        }
    }

    // Macro bodies are opaque to syn's AST visitor; a literal include inside
    // a macro expansion must still contribute to the containing module.
    fn scan_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        use proc_macro2::TokenTree;
        let tokens: Vec<_> = tokens.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            if let [TokenTree::Ident(name), TokenTree::Punct(bang), TokenTree::Group(group), ..] =
                &tokens[index..]
            {
                if name == "include" && bang.as_char() == '!' {
                    self.record_include(group.stream());
                    index += 3;
                    continue;
                }
            }
            if let TokenTree::Group(group) = &tokens[index] {
                self.scan_tokens(group.stream());
            }
            index += 1;
        }
    }
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

// Evaluate only what is known when `test` is false. An unrelated feature is
// unknown, so `any(test, feature = "x")` remains production-accounted.
fn without_test(meta: &syn::Meta) -> Option<bool> {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Some(false),
        syn::Meta::List(list) => {
            let args = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            let values: Vec<_> = args.iter().map(without_test).collect();
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|value| *value == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|value| *value == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|value| !value)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn test_only(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("test")
        || (attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| without_test(&meta) == Some(false)))
}

fn parse(text: &str, path: &Path) -> Result<Source, String> {
    let file = syn::parse_file(text).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut items = Items {
        in_test: file.attrs.iter().any(test_only),
        ..Items::default()
    };
    if items.in_test {
        items.excluded.extend(1..=text.lines().count());
    }
    items.visit_file(&file);
    if !items.errors.is_empty() {
        return Err(format!("{}: {}", path.display(), items.errors.join("; ")));
    }
    Ok(Source {
        lines: (1..=text.lines().count())
            .filter(|line| !items.excluded.contains(line))
            .count(),
        test_includes: items
            .test_includes
            .into_iter()
            .map(|include| normalize(&path.parent().unwrap_or(Path::new("")).join(include)))
            .collect(),
        includes: items
            .includes
            .into_iter()
            .map(|include| normalize(&path.parent().unwrap_or(Path::new("")).join(include)))
            .collect(),
    })
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            normalized.pop();
        } else if component != std::path::Component::CurDir {
            normalized.push(component);
        }
    }
    normalized
}

fn assembled(
    path: &Path,
    sources: &BTreeMap<PathBuf, Source>,
    stack: &mut Vec<PathBuf>,
) -> Result<usize, String> {
    if stack.iter().any(|parent| parent == path) {
        return Err(format!("{}: cyclic include!", path.display()));
    }
    let source = sources
        .get(path)
        .ok_or_else(|| format!("{}: included source is missing", path.display()))?;
    stack.push(path.to_owned());
    let mut count = source.lines;
    for include in &source.includes {
        count += assembled(include, sources, stack)?;
    }
    stack.pop();
    Ok(count)
}

fn budgets(root: &Path) -> Result<BTreeMap<String, usize>, String> {
    let path = root.join("docs/source-size-budgets.tsv");
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut budgets = BTreeMap::new();
    for line in text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.splitn(3, '\t').collect();
        if fields.len() != 3 || fields[2].trim().is_empty() {
            return Err(format!(
                "{}: budget requires path, limit, and rationale",
                path.display()
            ));
        }
        let limit = fields[1]
            .parse::<usize>()
            .map_err(|_| format!("invalid budget: {line}"))?;
        if limit <= LIMIT || budgets.insert(fields[0].into(), limit).is_some() {
            return Err(format!("invalid or duplicate budget: {line}"));
        }
    }
    Ok(budgets)
}

pub fn check(root: &Path, paths: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut sources = BTreeMap::new();
    for path in paths {
        if path.components().any(|part| part.as_os_str() == "tests") {
            continue;
        }
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        sources.insert(normalize(path), parse(&text, path)?);
    }
    let included: BTreeSet<_> = sources
        .values()
        .flat_map(|source| source.includes.iter().chain(&source.test_includes).cloned())
        .collect();
    // Test fragments must not become independent production roots. Traverse
    // their descendants too, since an included file inherits its caller's cfg.
    let mut test_reachable = BTreeSet::new();
    let mut pending: Vec<_> = sources
        .values()
        .flat_map(|source| source.test_includes.iter().cloned())
        .collect();
    while let Some(path) = pending.pop() {
        if test_reachable.insert(path.clone()) {
            if let Some(source) = sources.get(&path) {
                pending.extend(source.includes.iter().chain(&source.test_includes).cloned());
            }
        }
    }
    // An include cycle has no root. Validate non-test components before root
    // selection so a cycle cannot hide every file from the size check.
    for path in sources
        .keys()
        .filter(|path| !test_reachable.contains(*path))
    {
        assembled(path, &sources, &mut Vec::new())?;
    }
    let mut budgets = budgets(root)?;
    let mut findings = Vec::new();
    for path in sources.keys().filter(|path| !included.contains(*path)) {
        let display = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let count = assembled(path, &sources, &mut Vec::new())?;
        let budget = budgets.remove(&display);
        let limit = budget.unwrap_or(LIMIT);
        if count > limit {
            findings.push(format!(
                "{display}: {count} assembled production lines, limit is {limit}"
            ));
        } else if budget.is_some() && count <= LIMIT {
            findings.push(format!("{display}: remove obsolete source-size budget"));
        }
    }
    for path in budgets.keys() {
        findings.push(format!("{path}: source-size budget has no production root"));
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_code_after_tests_and_ignores_test_includes() -> Result<(), String> {
        let source = parse("#[cfg(test)]\nmod tests { include!(\"tests.rs\"); }\nfn production() {}\ninclude!(\"body.rs\");\n", Path::new("root.rs"))?;
        assert_eq!(source.lines, 2);
        assert_eq!(source.includes, [PathBuf::from("body.rs")]);
        Ok(())
    }

    #[test]
    fn follows_nested_includes_and_rejects_cycles_and_missing_files() -> Result<(), String> {
        let mut sources = BTreeMap::new();
        for (name, text) in [
            ("root.rs", "include!(\"part.rs\");\n"),
            ("part.rs", "include!(\"leaf.rs\");\n"),
            ("leaf.rs", "fn f() {}\n"),
        ] {
            sources.insert(PathBuf::from(name), parse(text, Path::new(name))?);
        }
        assert_eq!(
            assembled(Path::new("root.rs"), &sources, &mut Vec::new())?,
            3
        );
        sources.insert(
            PathBuf::from("leaf.rs"),
            parse("include!(\"root.rs\");", Path::new("leaf.rs"))?,
        );
        assert!(assembled(Path::new("root.rs"), &sources, &mut Vec::new()).is_err());
        sources.remove(Path::new("leaf.rs"));
        assert!(assembled(Path::new("root.rs"), &sources, &mut Vec::new()).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Result<Self, String> {
            let root = std::env::temp_dir().join(format!(
                "alas-source-size-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("docs")).map_err(|e| e.to_string())?;
            std::fs::write(
                root.join("docs/source-size-budgets.tsv"),
                "# test budgets\n",
            )
            .map_err(|e| e.to_string())?;
            Ok(Self(root))
        }
        fn source(&self, name: &str, content: &str) -> Result<PathBuf, String> {
            let path = self.0.join(name);
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            Ok(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_fragments_and_their_descendants_are_not_independent_roots() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let paths = [
            fixture.source(
                "root.rs",
                "#[cfg(test)]\nmod tests { include!(\"test_body.rs\"); }\nfn production() {}\n",
            )?,
            fixture.source("test_body.rs", "include!(\"test_leaf.rs\");\n")?,
            fixture.source("test_leaf.rs", &"// test fixture\n".repeat(700))?,
        ];
        assert!(check(&fixture.0, &paths)?.is_empty());
        Ok(())
    }

    #[test]
    fn shared_test_and_production_fragment_is_still_counted() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let paths = [
            fixture.source(
                "root.rs",
                "#[cfg(test)]\nmod tests { include!(\"shared.rs\"); }\ninclude!(\"shared.rs\");\n",
            )?,
            fixture.source("shared.rs", &"// production\n".repeat(501))?,
        ];
        assert!(check(&fixture.0, &paths)?
            .iter()
            .any(|finding| finding.contains("502 assembled")));
        Ok(())
    }

    #[test]
    fn a_cycle_without_a_root_cannot_escape_the_check() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let paths = [
            fixture.source("a.rs", "include!(\"b.rs\");")?,
            fixture.source("b.rs", "include!(\"a.rs\");")?,
        ];
        assert!(check(&fixture.0, &paths).is_err());
        Ok(())
    }

    #[test]
    fn recognizes_test_only_macro_items_and_compound_cfg_without_hiding_features(
    ) -> Result<(), String> {
        let source = parse("#[cfg(all(test, feature = \"x\"))]\ninclude!(\"test.rs\");\n#[cfg(any(test, feature = \"x\"))]\ninclude!(\"production.rs\");\n", Path::new("root.rs"))?;
        assert_eq!(source.test_includes, [PathBuf::from("test.rs")]);
        assert_eq!(source.includes, [PathBuf::from("production.rs")]);
        assert_eq!(source.lines, 2);
        Ok(())
    }

    #[test]
    fn macro_expansion_includes_are_accounted_and_dynamic_production_paths_fail_closed(
    ) -> Result<(), String> {
        let source = parse(
            "macro_rules! body { () => { include!(\"part.rs\"); } }",
            Path::new("root.rs"),
        )?;
        assert_eq!(source.includes, [PathBuf::from("part.rs")]);
        assert!(parse(
            "include!(concat!(\"part\", \".rs\"));",
            Path::new("root.rs")
        )
        .is_err());
        Ok(())
    }
}
