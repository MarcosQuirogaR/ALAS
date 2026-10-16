// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural validation of a `UavSpec`.

use std::collections::BTreeSet;

use super::inventory::{param, scalars};
use super::master::EmpennageDriving;
use super::topology::{FuselageKind, MotorLayout, TailKind, WingKind};
use super::{UavSpec, SPEC_SCHEMA_VERSION};

pub(super) fn validate(spec: &UavSpec) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    if spec.schema_version != SPEC_SCHEMA_VERSION {
        problems.push(format!("schema_version must be {SPEC_SCHEMA_VERSION}"));
    }
    ranges(spec, &mut problems);
    configuration(spec, &mut problems);
    tree(spec, &mut problems);
    if spec.mission.preset.id.trim().is_empty() {
        problems.push("mission.preset.id is empty".into());
    }
    let d = &spec.mission.decisions;
    if d.announced_takeoff_m
        .is_some_and(|v| !(v.is_finite() && v > 0.0))
    {
        problems.push("decisions.announced_takeoff_m must be > 0".into());
    }
    if d.takeoff_confidence.is_some_and(|v| !(v > 0.0 && v < 1.0)) {
        problems.push("decisions.takeoff_confidence must be in (0, 1)".into());
    }
    for key in spec.origins.keys() {
        if !is_known_path(spec, key) {
            problems.push(format!(
                "origins: '{key}' is not a parameter of this design"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

fn is_known_path(spec: &UavSpec, key: &str) -> bool {
    scalars(&spec.master).iter().any(|(path, _, _)| path == key)
}

fn ranges(spec: &UavSpec, problems: &mut Vec<String>) {
    let m = &spec.master;
    if !(1..=4).contains(&m.panels.len()) {
        problems.push("master.panels must hold 1 to 4 panels".into());
    }
    for (path, pattern, value) in scalars(m) {
        let Some(info) = param(pattern) else {
            problems.push(format!("{path}: no inventory row for '{pattern}'"));
            continue;
        };
        if !value.is_finite() || value < info.min || value > info.max {
            problems.push(format!(
                "{path} = {value} is outside [{}, {}] {}",
                info.min, info.max, info.unit
            ));
        }
    }
    for (i, c) in m.controls.iter().enumerate() {
        if c.span_end <= c.span_start {
            problems.push(format!("controls[{i}]: span_end must exceed span_start"));
        }
    }
    for (name, value) in &spec.tolerances_m {
        if !(value.is_finite() && *value >= 0.0 && *value < 0.01) {
            problems.push(format!(
                "tolerances_m.{name} = {value} must be in [0, 0.01) m"
            ));
        }
    }
}

fn configuration(spec: &UavSpec, problems: &mut Vec<String>) {
    let (t, m) = (&spec.topology, &spec.master);
    let flying = matches!(t.wing, WingKind::Flying { .. });
    if flying != matches!(t.tail, TailKind::None) {
        problems.push("a flying wing has no empennage and every other wing needs one".into());
    }
    if t.fuselage == FuselageKind::Boom && m.tail_boom.is_none() {
        problems.push("fuselage kind 'boom' needs master.tail_boom".into());
    }
    if t.fuselage != FuselageKind::Boom && m.tail_boom.is_some() {
        problems.push("master.tail_boom is set but the fuselage kind is not 'boom'".into());
    }
    if let MotorLayout::Twin { span_fraction, .. } = t.motors {
        if !(span_fraction > 0.0 && span_fraction < 1.0) {
            problems.push("twin motor span_fraction must be in (0, 1)".into());
        }
    }
    if let EmpennageDriving::Explicit {
        horizontal,
        vertical,
    } = &m.empennage
    {
        let needs_vertical = matches!(
            t.tail,
            TailKind::Conventional | TailKind::T | TailKind::TwinBoom
        );
        let has_horizontal = !matches!(t.tail, TailKind::None);
        if has_horizontal != horizontal.is_some() {
            problems.push(
                "explicit empennage: horizontal surface presence must match the tail kind".into(),
            );
        }
        if needs_vertical != vertical.is_some() {
            problems.push(
                "explicit empennage: vertical surface presence must match the tail kind".into(),
            );
        }
    }
    if flying && matches!(m.empennage, EmpennageDriving::Driven { .. }) {
        problems.push("driven empennage sizing does not apply to a flying wing".into());
    }
    super::validate_layout::layout(spec, problems);
}

fn tree(spec: &UavSpec, problems: &mut Vec<String>) {
    let mut publications = BTreeSet::new();
    for (skeleton, list) in &spec.subskeletons {
        for p in list {
            if p.owner != *skeleton {
                problems.push(format!(
                    "publication '{}' is filed under the wrong skeleton",
                    p.name
                ));
            }
            if !publications.insert(p.name.as_str()) {
                problems.push(format!("duplicate publication '{}'", p.name));
            }
        }
    }
    let names: BTreeSet<&str> = spec.tree.iter().map(|n| n.name.as_str()).collect();
    if names.len() != spec.tree.len() {
        problems.push("tree node names must be unique".into());
    }
    for node in &spec.tree {
        let n = &node.name;
        if node
            .parent
            .as_deref()
            .is_some_and(|p| !names.contains(p) || p == n)
        {
            problems.push(format!("node '{n}': unknown or self parent"));
        }
        for reference in [&node.parent_frame, &node.cg_point].into_iter().flatten() {
            if !publications.contains(reference.as_str()) {
                problems.push(format!("node '{n}': unknown publication '{reference}'"));
            }
        }
        let bad_mass = |v: f64| !(v.is_finite() && v >= 0.0);
        if node.mass_override_kg.is_some_and(bad_mass) {
            problems.push(format!("node '{n}': mass_override_kg must be >= 0"));
        }
        if node
            .offset_m
            .iter()
            .chain(&node.euler_deg)
            .any(|v| !v.is_finite())
        {
            problems.push(format!("node '{n}': non-finite offset or angle"));
        }
    }
    // Every parent chain must terminate (no cycles).
    for node in &spec.tree {
        let mut current = node.parent.as_deref();
        for _ in 0..=spec.tree.len() {
            let Some(name) = current else { break };
            current = spec
                .tree
                .iter()
                .find(|n| n.name == name)
                .and_then(|n| n.parent.as_deref());
        }
        if current.is_some() {
            problems.push(format!("node '{}': parent chain has a cycle", node.name));
        }
    }
}
