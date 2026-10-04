// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which workspace packages a set of changed files affects.
//!
//! The quick gate lints and tests only these packages. The computation is pure
//! (metadata text and file names in, package names out) so it is tested without
//! Git or Cargo.

use std::collections::BTreeMap;

/// A workspace member: its name, its directory relative to the workspace root
/// with forward slashes, and the workspace members it depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dir: String,
    pub deps: Vec<String>,
}

/// The outcome of the selection.
#[derive(Debug, PartialEq, Eq)]
pub enum Affected {
    /// A file that can change any package's build or behaviour changed.
    All(String),
    /// Package name to the reason it was selected.
    Some(BTreeMap<String, String>),
}

/// Files whose change can alter every package: the workspace manifest and
/// lock, the toolchain pin, Cargo configuration and the test-runner
/// configuration, which decides the test tiers.
fn is_global(file: &str) -> bool {
    matches!(
        file,
        "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | ".config/nextest.toml"
    ) || file.starts_with(".cargo/")
}

/// Files outside every package that no build or test reads: Markdown prose,
/// CI definitions and the top-level images. Any other file outside a package
/// may be read by a test through a relative path (fixtures under `golden/`,
/// `assets/`, the JSON manifests under `docs/benchmarks/`), so it selects
/// every package.
fn is_inert(file: &str) -> bool {
    let top_level = !file.contains('/');
    (file.ends_with(".md") && (top_level || file.starts_with("docs/")))
        || file.starts_with(".github/")
        || (top_level && (file.ends_with(".png") || file.ends_with(".ico")))
}

/// Reads workspace members from `cargo metadata --format-version 1 --no-deps`.
pub fn parse_metadata(json: &str) -> Result<Vec<Package>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("cannot parse cargo metadata: {e}"))?;
    let normalize = |p: &str| p.replace('\\', "/");
    let root = normalize(
        value["workspace_root"]
            .as_str()
            .ok_or("metadata has no workspace_root")?,
    );
    let members = value["packages"]
        .as_array()
        .ok_or("metadata has no packages")?;

    let names: Vec<&str> = members.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for member in members {
        let name = member["name"].as_str().ok_or("package without a name")?;
        let manifest = normalize(
            member["manifest_path"]
                .as_str()
                .ok_or("package without a manifest_path")?,
        );
        let dir = manifest
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir)
            .strip_prefix(root.as_str())
            .ok_or_else(|| format!("{name} lies outside the workspace root"))?
            .trim_matches('/')
            .to_owned();
        let mut deps: Vec<String> = member["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["name"].as_str())
            .filter(|d| names.contains(d))
            .map(str::to_owned)
            .collect();
        deps.sort();
        deps.dedup();
        out.push(Package {
            name: name.to_owned(),
            dir,
            deps,
        });
    }
    Ok(out)
}

/// The package owning `file`; the deepest directory wins so a nested package
/// is not attributed to its parent.
fn owner<'a>(packages: &'a [Package], file: &str) -> Option<&'a Package> {
    packages
        .iter()
        .filter(|p| !p.dir.is_empty() && file.starts_with(&format!("{}/", p.dir)))
        .max_by_key(|p| p.dir.len())
}

/// Packages containing a changed file, closed over workspace reverse
/// dependencies (a dependent must be re-tested when its dependency changes).
pub fn affected(packages: &[Package], files: &[String]) -> Affected {
    let mut selected = BTreeMap::new();
    for file in files {
        if is_global(file) {
            return Affected::All(format!("{file} changed"));
        }
        match owner(packages, file) {
            Some(p) => {
                selected
                    .entry(p.name.clone())
                    .or_insert_with(|| format!("{file} changed"));
            }
            None if is_inert(file) => {}
            None => return Affected::All(format!("{file} lies outside every package")),
        }
    }
    loop {
        let mut added = Vec::new();
        for p in packages {
            if selected.contains_key(&p.name) {
                continue;
            }
            if let Some(dep) = p.deps.iter().find(|d| selected.contains_key(*d)) {
                added.push((p.name.clone(), format!("depends on {dep}")));
            }
        }
        if added.is_empty() {
            return Affected::Some(selected);
        }
        selected.extend(added);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, deps: &[&str]) -> Package {
        Package {
            name: name.to_owned(),
            dir: format!("crates/{name}"),
            deps: deps.iter().map(|d| (*d).to_owned()).collect(),
        }
    }

    fn graph() -> Vec<Package> {
        vec![
            pkg("core", &[]),
            pkg("aero", &["core"]),
            pkg("gui", &["aero", "core"]),
            pkg("lone", &[]),
        ]
    }

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|f| (*f).to_owned()).collect()
    }

    fn subset(result: Affected) -> BTreeMap<String, String> {
        match result {
            Affected::Some(map) => map,
            Affected::All(why) => panic!("expected a subset, got all: {why}"),
        }
    }

    #[test]
    fn a_leaf_change_selects_only_the_leaf() {
        let map = subset(affected(&graph(), &files(&["crates/gui/src/lib.rs"])));
        assert_eq!(map.into_keys().collect::<Vec<_>>(), ["gui"]);
    }

    #[test]
    fn a_change_selects_transitive_dependents() {
        let map = subset(affected(&graph(), &files(&["crates/core/src/lib.rs"])));
        assert_eq!(
            map.keys().cloned().collect::<Vec<_>>(),
            ["aero", "core", "gui"]
        );
        assert_eq!(map["aero"], "depends on core");
    }

    #[test]
    fn documentation_and_ci_select_nothing() {
        let changed = files(&["docs/RUNNING.md", ".github/workflows/a.yml", "README.md"]);
        assert!(subset(affected(&graph(), &changed)).is_empty());
    }

    #[test]
    fn global_files_select_everything() {
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".config/nextest.toml",
            "golden/fixture.json",
            "assets/model.bin",
            "docs/benchmarks/q0007_open_benchmarks.json",
        ] {
            let result = affected(&graph(), &files(&[file]));
            assert!(matches!(result, Affected::All(_)), "{file}");
        }
    }

    #[test]
    fn the_task_runner_is_an_ordinary_package() {
        let mut packages = graph();
        packages.push(Package {
            name: "xtask".into(),
            dir: "xtask".into(),
            deps: vec![],
        });
        let map = subset(affected(&packages, &files(&["xtask/src/gate.rs"])));
        assert_eq!(map.into_keys().collect::<Vec<_>>(), ["xtask"]);
    }

    #[test]
    fn a_package_manifest_is_not_global() {
        let map = subset(affected(&graph(), &files(&["crates/lone/Cargo.toml"])));
        assert_eq!(map.into_keys().collect::<Vec<_>>(), ["lone"]);
    }

    #[test]
    fn the_deepest_directory_owns_a_nested_package() {
        let mut packages = graph();
        packages.push(Package {
            name: "inner".into(),
            dir: "crates/core/inner".into(),
            deps: vec![],
        });
        let map = subset(affected(
            &packages,
            &files(&["crates/core/inner/src/lib.rs"]),
        ));
        assert_eq!(map.into_keys().collect::<Vec<_>>(), ["inner"]);
    }

    #[test]
    fn metadata_yields_members_and_only_workspace_edges() {
        let json = r#"{"workspace_root":"C:\\repo","packages":[
            {"name":"a","manifest_path":"C:\\repo\\crates\\a\\Cargo.toml","dependencies":[{"name":"serde"}]},
            {"name":"b","manifest_path":"C:\\repo\\crates\\b\\Cargo.toml","dependencies":[{"name":"a"},{"name":"a"},{"name":"serde"}]}]}"#;
        let packages = parse_metadata(json).expect("valid metadata");
        assert_eq!(packages[0].dir, "crates/a");
        assert!(packages[0].deps.is_empty());
        assert_eq!(packages[1].dir, "crates/b");
        assert_eq!(packages[1].deps, ["a"]);
    }
}
