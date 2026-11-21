// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `cargo xtask capability-map`: a generated inventory of everything ALAS can do.
//!
//! The map is derived from the source tree alone (it never builds or runs the
//! application), is deterministic, and is checked in under `docs/architecture/`.
//! `checks` regenerates it in memory and fails when the committed copy is stale,
//! when a configuration field is read by no computation and is not exempted, and
//! when a capability family has no smoke test.

mod cli_reference;
mod config_schema;
mod consumers;
mod gui;
mod inventory;
mod render;
mod schema;
mod sources;
#[cfg(test)]
mod tests;

use consumers::Role;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const MAP_JSON: &str = "docs/architecture/capability-map.json";
pub const MAP_MD: &str = "docs/architecture/capability-map.md";
pub const CONFIG_SCHEMA: &str = "docs/schemas/alas.config.v1.schema.json";
pub const CLI_REFERENCE: &str = "docs/architecture/cli-reference.md";
const EXEMPTIONS: &str = "docs/architecture/field-exemptions.txt";

/// Capability families; each needs at least one `capability_smoke_<family>_*` test.
pub const FAMILIES: &[&str] = &[
    "config",
    "pipeline",
    "report",
    "tools",
    "gui",
    "cli",
    "addons",
    "filetypes",
];

const EXEMPTION_KINDS: &[&str] = &["display_only", "deprecated"];

struct Exemption {
    kind: String,
    reason: String,
}

fn read_exemptions(root: &Path) -> Result<BTreeMap<String, Exemption>, String> {
    let path = root.join(EXEMPTIONS);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').map(str::trim).collect();
        if parts.len() != 3 || !EXEMPTION_KINDS.contains(&parts[1]) || parts[2].is_empty() {
            return Err(format!(
                "{EXEMPTIONS}:{}: expected `path | display_only|deprecated | reason`",
                number + 1
            ));
        }
        out.insert(
            parts[0].to_owned(),
            Exemption {
                kind: parts[1].to_owned(),
                reason: parts[2].to_owned(),
            },
        );
    }
    Ok(out)
}

/// The regenerated map plus the findings the checks report.
pub struct Generated {
    pub json: String,
    pub markdown: String,
    pub schema: String,
    pub cli_reference: String,
    pub findings: Vec<String>,
}

pub fn generate(root: &Path) -> Result<Generated, String> {
    let (files, unparsed) = sources::load(root)?;
    let structs = schema::collect_structs(&files);
    let leaves = schema::leaves(&structs)?;
    let exemptions = read_exemptions(root)?;

    let names: BTreeSet<String> = leaves.iter().map(|l| l.field.name.clone()).collect();
    let paths: BTreeSet<String> = leaves.iter().map(|l| l.path.clone()).collect();
    let scan = consumers::scan(&files, &names, &paths);
    let reached = scan.reached_fns();
    let mut name_owners: BTreeMap<&str, usize> = BTreeMap::new();
    for leaf in &leaves {
        *name_owners.entry(leaf.field.name.as_str()).or_default() += 1;
    }
    let pages = gui::pages(&files);

    let mut findings = Vec::new();
    let mut fields = Vec::new();
    let mut seen_exemptions = BTreeSet::new();
    for leaf in &leaves {
        let mut by_role: BTreeMap<Role, BTreeSet<String>> = BTreeMap::new();
        for source in [
            scan.reads.get(&leaf.field.name),
            scan.path_reads.get(&leaf.path),
        ]
        .into_iter()
        .flatten()
        {
            for (role, set) in source {
                by_role
                    .entry(*role)
                    .or_default()
                    .extend(set.iter().cloned());
            }
        }
        let crate_of = |path: &String| path.split('/').nth(1).unwrap_or_default().to_owned();
        let compute: BTreeSet<String> = by_role
            .get(&Role::Compute)
            .map(|s| s.iter().map(crate_of).collect())
            .unwrap_or_default();
        let gui_crates: BTreeSet<String> = by_role
            .get(&Role::Gui)
            .map(|s| s.iter().map(crate_of).collect())
            .unwrap_or_default();
        let config_files = by_role.get(&Role::Config).map_or(0, BTreeSet::len);
        let model_methods: Vec<&String> = scan
            .model_fns
            .get(&leaf.field.name)
            .into_iter()
            .flatten()
            .filter(|f| reached.contains(*f))
            .collect();
        let status = if !compute.is_empty() {
            "computed"
        } else if !model_methods.is_empty() {
            "config_method"
        } else if !gui_crates.is_empty() {
            "gui_only"
        } else if config_files > 0 {
            "config_only"
        } else {
            "unread"
        };
        let exemption = exemptions.get(&leaf.path);
        if exemption.is_some() {
            seen_exemptions.insert(leaf.path.clone());
        }
        let used = status == "computed" || status == "config_method";
        if !used && exemption.is_none() {
            findings.push(format!(
                "{}: configuration field `{}` is {status}: no computation reads it; wire it, or \
                 list it in {EXEMPTIONS} as display_only or deprecated (retire it through retired_keys.rs); an unread field is never accepted as a known gap",
                leaf.file, leaf.path
            ));
        }
        if used && exemption.is_some() {
            findings.push(format!(
                "{EXEMPTIONS}: `{}` is read by model code now ({status}); delete its exemption",
                leaf.path
            ));
        }
        let location = gui::locations(&pages, &leaf.path, leaf.advanced);
        fields.push(json!({
            "path": leaf.path,
            "type": leaf.field.rust_type,
            "unit": leaf.field.unit,
            "label": leaf.field.label,
            "help": leaf.field.help,
            "min": leaf.field.min,
            "max": leaf.field.max,
            "level": if leaf.advanced { "advanced" } else { "routine" },
            "hidden": leaf.field.hidden,
            "declared_in": leaf.file,
            "status": status,
            "shared_name": name_owners[leaf.field.name.as_str()] > 1,
            "readers": {
                "compute": compute, "gui": gui_crates, "config_files": config_files,
                "config_methods": model_methods,
            },
            "gui": location.iter()
                .map(|(page, window)| json!({"page": page, "window": window}))
                .collect::<Vec<_>>(),
            "exemption": exemption.map(|e| json!({"kind": e.kind, "reason": e.reason})),
        }));
    }
    for path in exemptions.keys() {
        if !seen_exemptions.contains(path) {
            findings.push(format!(
                "{EXEMPTIONS}: `{path}` is not a configuration field"
            ));
        }
    }

    let smoke = inventory::smoke_tests(&files);
    let families_with_smoke: BTreeSet<&str> =
        smoke.iter().filter_map(|t| t["family"].as_str()).collect();
    for family in FAMILIES {
        if !families_with_smoke.contains(family) {
            findings.push(format!(
                "capability family `{family}` has no `capability_smoke_{family}_*` test"
            ));
        }
    }

    let map = json!({
        "schema_version": 1,
        "families": FAMILIES,
        "config": {
            "root": "AlasConfig",
            "field_count": fields.len(),
            "status_note": "computed = read by a model or pipeline crate; config_method = read by an alas-config function (outside validation and loading) whose name model code calls; gui_only / config_only / unread need an exemption. Matching is by field name (shared_name marks ambiguous names) plus full-path string literals.",
            "fields": fields,
        },
        "pipeline_stages": inventory::pipeline_stages(&files),
        "report_figures": inventory::figures(&files),
        "external_tools": inventory::external_tools(&files),
        "gui_pages": pages.iter().map(page_json).collect::<Vec<_>>(),
        "cli_flags": inventory::cli_flags(&files),
        "cli_help": cli_reference::cli_help(&files),
        "addons": inventory::addons(&files),
        "file_types": inventory::file_types(&files),
        "smoke_tests": smoke,
        "unparsed_sources": unparsed,
    });
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())? + "\n";
    let markdown = render::markdown(&map);
    let cli_reference = cli_reference::markdown(&map);
    let schema = serde_json::to_string_pretty(&config_schema::config_schema(&map)?)
        .map_err(|e| e.to_string())?
        + "
";
    Ok(Generated {
        json,
        markdown,
        schema,
        cli_reference,
        findings,
    })
}

fn page_json(page: &gui::PageInfo) -> Value {
    json!({
        "id": page.id, "title": page.title, "kind": page.kind, "group": page.group,
        "surface": page.surface, "window": page.place,
        "extra_cards": page.extras.iter()
            .map(|(path, names)| json!({"node": path, "fields": names}))
            .collect::<Vec<_>>(),
    })
}

/// `cargo xtask capability-map [--check]`: write the map, or only verify it.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let generated = generate(root)?;
    let check_only = args.iter().any(|a| a == "--check");
    for finding in &generated.findings {
        println!("{finding}");
    }
    if check_only {
        let stale = stale_files(root, &generated);
        for path in &stale {
            println!("{path}: out of date; run `cargo xtask capability-map` and commit it");
        }
        if generated.findings.is_empty() && stale.is_empty() {
            println!("capability-map: pass");
            return Ok(());
        }
        return Err("capability-map: check failed".to_owned());
    }
    write(root, MAP_JSON, &generated.json)?;
    write(root, MAP_MD, &generated.markdown)?;
    write(root, CONFIG_SCHEMA, &generated.schema)?;
    write(root, CLI_REFERENCE, &generated.cli_reference)?;
    println!("wrote {MAP_JSON} and {MAP_MD}");
    if generated.findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "capability-map: {} finding(s) (files written)",
            generated.findings.len()
        ))
    }
}

/// Findings for `cargo xtask checks`.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let generated = generate(root)?;
    let mut findings = generated.findings.clone();
    for path in stale_files(root, &generated) {
        findings.push(format!(
            "{path}: out of date; run `cargo xtask capability-map` and commit it"
        ));
    }
    Ok(findings)
}

fn stale_files(root: &Path, generated: &Generated) -> Vec<&'static str> {
    let mut stale = Vec::new();
    for (path, text) in [
        (MAP_JSON, &generated.json),
        (MAP_MD, &generated.markdown),
        (CONFIG_SCHEMA, &generated.schema),
        (CLI_REFERENCE, &generated.cli_reference),
    ] {
        let on_disk = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        if on_disk.replace("\r\n", "\n") != *text {
            stale.push(path);
        }
    }
    stale
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}
