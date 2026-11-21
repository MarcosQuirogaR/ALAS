// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The search index: one entry per configuration field the guided pages draw,
//! from the live schema, with the page, section and detail level that host it.

use alas_config::{Entry, Field, Node};

use super::{implied_condition, Condition, SearchEntry};
use crate::nav::{self, PageKind};
use crate::search_concepts::CONCEPTS;
use crate::views::form_page::placement;

/// Where a family of fields is drawn.
#[derive(Clone, Copy)]
struct Host {
    page: &'static str,
    section: Option<&'static str>,
    /// Whether the host itself is drawn only at the Advanced level.
    advanced: bool,
    /// Whether the host draws a collapsing header for each schema group, so a
    /// group is a target of its own.
    nodes: bool,
    /// Whether the host honours the schema's per-field `advanced` flag.
    flags: bool,
}

impl Host {
    fn card(page: &'static str) -> Self {
        Self {
            page,
            section: None,
            advanced: false,
            nodes: false,
            flags: true,
        }
    }
}

/// What a field inherits from the groups above it.
#[derive(Clone, Default)]
struct Chain {
    advanced: bool,
    conditions: Vec<Condition>,
}

/// Groups whose page draws them with a bespoke editor instead of the generic
/// form, so they have no schema header to land on.
const BESPOKE_GROUPS: &[&str] = &["cabin"];

/// Add the entry for `field` and, for a group, the entries below it. `prefix`
/// is the dotted parent path.
fn collect(field: &Field, prefix: &str, host: Host, inherited: &Chain, out: &mut Vec<SearchEntry>) {
    let path = if prefix.is_empty() {
        field.name.to_owned()
    } else {
        format!("{prefix}.{}", field.name)
    };
    let mut chain = inherited.clone();
    if host.flags {
        chain.advanced |= field.advanced;
    }
    if let Some(condition) = field.visible_if {
        chain
            .conditions
            .push((condition.path, condition.any_of, false));
    }
    if let Some(condition) = implied_condition(&path) {
        chain.conditions.push(condition);
    }
    let entry = |out: &mut Vec<SearchEntry>| {
        let label = alas_i18n::t(Some(field.label), None).into_owned();
        let haystack = format!(
            "{} {} {} {} {}",
            field.label,
            label,
            field.name.replace('_', " "),
            field.unit,
            field.full_help()
        )
        .to_lowercase();
        out.push(SearchEntry {
            title: field.label.to_owned(),
            unit: field.unit.to_owned(),
            page: host.page,
            section: host.section,
            field: Some(path.clone()),
            preset: None,
            is_page: false,
            advanced: chain.advanced,
            visible_if: chain.conditions.clone(),
            haystack,
            summary: field.help.to_lowercase(),
        });
    };
    match &field.entry {
        Entry::Node(node) => {
            if host.nodes {
                entry(out);
            }
            for child in &node.fields {
                collect(child, &path, host, &chain, out);
            }
        }
        Entry::Leaf(_) => entry(out),
    }
}

/// Add the fields of `fields`, the children of the group `group`, hosted by
/// `host`.
fn collect_group(group: &str, fields: &[Field], host: Host, out: &mut Vec<SearchEntry>) {
    let chain = Chain {
        advanced: host.advanced,
        conditions: Vec::new(),
    };
    for field in fields {
        collect(field, group, host, &chain, out);
    }
}

/// Add the `requirements` leaves: the routine ones sit in the Aircraft page's
/// requirements card; the schema's advanced ones are the Design Space page's
/// design constraints, except the freighter's cargo objective, which the
/// Aircraft card draws at the Advanced level.
fn collect_requirements(group: &Field, aircraft: &'static str, out: &mut Vec<SearchEntry>) {
    let Entry::Node(node) = &group.entry else {
        return;
    };
    let chain = Chain::default();
    for field in &node.fields {
        let constraint =
            field.advanced && !matches!(field.name, "cargo_objective_kg" | "gravity_m_s2");
        let host = if constraint {
            // The Design Space card clears the flag: it is the section itself.
            Host {
                flags: false,
                ..Host::card("design_space")
            }
        } else {
            Host::card(aircraft)
        };
        collect(field, group.name, host, &chain, out);
    }
}

/// Add one bespoke-card root (a group or a leaf) without a header entry of its
/// own.
fn collect_root(field: &Field, host: Host, out: &mut Vec<SearchEntry>) {
    let chain = Chain::default();
    match &field.entry {
        Entry::Node(node) => {
            for child in &node.fields {
                collect(child, field.name, host, &chain, out);
            }
        }
        Entry::Leaf(_) => collect(field, "", host, &chain, out),
    }
}

/// Fields edited by the bespoke cards of the Aircraft and Mission pages, as
/// (group, page id): their leaves are indexed under that page without a
/// section.
const CARD_GROUPS: &[(&str, &str)] = &[
    ("requirements", "inputs"),
    ("departure_airport", "inputs"),
    ("arrival_airport", "inputs"),
    ("mission", "mission"),
];

/// Add the `mission.profile` leaves the Mission page edits in its phase
/// editors (see `mission_profile_inputs::PHASES`).
fn collect_phase_fields(mission: &Field, host: Host, out: &mut Vec<SearchEntry>) {
    let Entry::Node(node) = &mission.entry else {
        return;
    };
    let Some(Entry::Node(profile)) = node.field("profile").map(|f| &f.entry) else {
        return;
    };
    let chain = Chain::default();
    for field in &profile.fields {
        if crate::views::mission_phase_focus::phase_of_field(field.name).is_some() {
            collect(field, "mission.profile", host, &chain, out);
        }
    }
}

/// Add the Mesh resolution fields the Analysis fidelity page draws itself.
fn collect_mesh_fields(schema: &Node, out: &mut Vec<SearchEntry>) {
    let Some(page) = nav::page("setup_analyses") else {
        return;
    };
    let section = page
        .advanced
        .iter()
        .find(|section| section.pages.contains(&"analysis"))
        .map(|section| section.title);
    // The Analysis fidelity page draws these itself, whatever their flags.
    let host = Host {
        page: page.id,
        section,
        advanced: nav::page("analysis")
            .is_some_and(|form| !crate::views::flow_pages::form_shown_at_basic(schema, form)),
        nodes: false,
        flags: false,
    };
    let chain = Chain {
        advanced: host.advanced,
        conditions: Vec::new(),
    };
    for (node, prefix, name) in crate::views::form_page::mesh_resolution::MESH_FIELDS {
        let fields = placement::node_fields_at(schema, node).unwrap_or_default();
        for field in fields.iter().filter(|field| field.name == *name) {
            collect(field, prefix, host, &chain, out);
        }
    }
}

/// Whether a flow page's Advanced section titled `title` has rows at Basic.
fn section_shown_at_basic(schema: &Node, page: &nav::Page, title: &str) -> bool {
    page.advanced
        .iter()
        .find(|section| section.title == title)
        .is_some_and(|section| {
            section
                .pages
                .iter()
                .filter_map(|id| nav::page(id))
                .any(|form| crate::views::flow_pages::form_shown_at_basic(schema, form))
        })
}

/// Build the index from the live schema, without the preset entries.
pub fn build_index(schema: &Node) -> Vec<SearchEntry> {
    build_index_with_presets(schema, &[])
}

/// Aliases a person types for a page whose sidebar title is another word.
fn page_aliases(id: &str) -> &'static str {
    match id {
        "inputs" => "inputs input configuration",
        "setup_analyses" => "analyses analysis run settings",
        _ => "",
    }
}

/// One entry per page of the navigation tree and per registered preset, so the
/// palette finds where to go and what to load, not only fields.
fn page_and_preset_entries(presets: &[(String, String)], out: &mut Vec<SearchEntry>) {
    for page in nav::NAV
        .iter()
        .flat_map(|group| group.subgroups.iter())
        .flat_map(|sub| sub.pages.iter())
    {
        out.push(SearchEntry {
            title: page.title.to_owned(),
            unit: String::new(),
            page: page.id,
            section: None,
            field: None,
            preset: None,
            is_page: true,
            advanced: false,
            visible_if: Vec::new(),
            haystack: format!(
                "{} {} {} {}",
                page.title,
                alas_i18n::t(Some(page.title), None),
                page.id.replace('_', " "),
                page_aliases(page.id)
            )
            .to_lowercase(),
            summary: String::new(),
        });
    }
    for (key, display) in presets {
        out.push(SearchEntry {
            title: display.clone(),
            unit: String::new(),
            page: "inputs",
            section: Some("Preset"),
            field: None,
            preset: Some(key.clone()),
            is_page: false,
            advanced: false,
            visible_if: Vec::new(),
            haystack: format!("{display} {key} preset").to_lowercase(),
            summary: String::new(),
        });
    }
}

/// Build the index from the live schema, with an entry for each of `presets`
/// (registered key, display name).
pub fn build_index_with_presets(schema: &Node, presets: &[(String, String)]) -> Vec<SearchEntry> {
    let mut out = Vec::new();
    // Cards first, so a shared name resolves to the routine location.
    for (group, page) in CARD_GROUPS {
        let Some(field) = schema.field(group) else {
            continue;
        };
        if let Some(host) = nav::page(page) {
            if *group == "requirements" {
                collect_requirements(field, host.id, &mut out);
            } else if *group == "mission" {
                // The page itself draws only the fields of the phase editors;
                // the rest of the group sits in its Advanced sections.
                collect_phase_fields(field, Host::card(host.id), &mut out);
            } else {
                collect_root(field, Host::card(host.id), &mut out);
            }
        }
    }
    // The field-length overrides sit on the case itself and render in the Route card.
    for name in ["toda_override_m", "lda_override_m"] {
        if let (Some(field), Some(host)) = (schema.field(name), nav::page("inputs")) {
            collect_root(field, Host::card(host.id), &mut out);
        }
    }
    collect_mesh_fields(schema, &mut out);
    for host in nav::NAV
        .iter()
        .flat_map(|group| group.subgroups.iter())
        .flat_map(|sub| sub.pages.iter())
    {
        if host.kind == PageKind::Form {
            if let (Some(group), Some(Entry::Node(node))) = (
                host.group,
                host.group.and_then(|g| schema.field(g)).map(|f| &f.entry),
            ) {
                let fields = placement::visible_fields(host, group, &node.fields);
                let page_host = Host {
                    nodes: !BESPOKE_GROUPS.contains(&group),
                    ..Host::card(host.id)
                };
                collect_group(group, &fields, page_host, &mut out);
            }
        }
        for section in host.advanced {
            for form_id in section.pages {
                let Some(form) = nav::page(form_id) else {
                    continue;
                };
                let Some(group) = form.group else { continue };
                let Some(Entry::Node(node)) = schema.field(group).map(|f| &f.entry) else {
                    continue;
                };
                let fields = placement::visible_fields(form, group, &node.fields);
                let section_host = Host {
                    page: host.id,
                    section: Some(section.title),
                    advanced: !crate::views::flow_pages::form_rows_drawn_at_basic(schema, form),
                    nodes: !BESPOKE_GROUPS.contains(&group),
                    flags: true,
                };
                collect_group(group, &fields, section_host, &mut out);
                // Cards of fields owned by other groups that this page renders.
                for extra in form.extra {
                    let prefix = extra
                        .path
                        .split('/')
                        .filter(|segment| !segment.is_empty())
                        .collect::<Vec<_>>()
                        .join(".");
                    let Some(fields) = placement::node_fields_at(schema, extra.path) else {
                        continue;
                    };
                    let extra_host = Host {
                        advanced: !crate::views::flow_pages::form_shown_at_basic(schema, form),
                        ..section_host
                    };
                    let chain = Chain {
                        advanced: extra_host.advanced,
                        conditions: Vec::new(),
                    };
                    for field in fields.iter().filter(|f| extra.names.contains(&f.name)) {
                        collect(field, &prefix, extra_host, &chain, &mut out);
                    }
                }
            }
        }
    }
    // First hit wins: drop later duplicates of the same field path.
    let mut seen = std::collections::HashSet::new();
    out.retain(|entry| match &entry.field {
        Some(path) => seen.insert(path.clone()),
        None => true,
    });
    page_and_preset_entries(presets, &mut out);
    for concept in CONCEPTS {
        // A concept that names a field lands where that field is drawn, so
        // its page and section can never drift from the forms.
        let host = concept.field.and_then(|path| {
            out.iter()
                .find(|entry| entry.field.as_deref() == Some(path) && entry.title != concept.title)
                .map(|entry| {
                    (
                        entry.page,
                        entry.section,
                        entry.advanced,
                        entry.visible_if.clone(),
                    )
                })
        });
        let (page, section, advanced, visible_if) = host.unwrap_or_else(|| {
            // A concept with no field lands on its page or Advanced section.
            let hidden = concept.section.is_some_and(|title| {
                nav::page(concept.page)
                    .is_none_or(|page| !section_shown_at_basic(schema, page, title))
            });
            let hidden = hidden
                || nav::page(concept.page)
                    .is_some_and(|page| page.surface == nav::Surface::Modeling);
            (concept.page, concept.section, hidden, Vec::new())
        });
        out.push(SearchEntry {
            title: concept.title.to_owned(),
            unit: String::new(),
            page,
            section,
            field: concept.field.map(str::to_owned),
            preset: None,
            is_page: false,
            advanced,
            visible_if,
            haystack: format!(
                "{} {} {}",
                concept.title,
                alas_i18n::t(Some(concept.title), None),
                concept.synonyms
            )
            .to_lowercase(),
            summary: String::new(),
        });
    }
    out
}
