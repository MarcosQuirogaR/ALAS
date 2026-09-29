// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strict NASTRAN-95 printed CQUAD4/CTRIA3 centroid stresses. The final
//! printed column is maximum shear, so von Mises is derived from the actual
//! printed plane-stress tensor, never read from that unrelated column.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::loads::LoadCase;
use crate::mesh::Deck;
use crate::nastran::{
    StaticCaseIdentity, StaticShellStressCase, StaticShellStressResponse, StaticShellStressSample,
};

/// Read every submitted shell's two centroid fibers in every explicit case.
/// A missing, repeated, unknown or nonfinite row fails the complete result.
pub fn read_static_shell_stress_print(
    print: &str,
    deck: &Deck,
    cases: &[LoadCase],
) -> StaticShellStressResponse {
    match extract(print, deck, cases) {
        Ok(cases) => StaticShellStressResponse {
            cases,
            error: None,
            case_identity: StaticCaseIdentity::ExplicitSubcaseIds,
        },
        Err(error) => StaticShellStressResponse {
            cases: Vec::new(),
            error: Some(error),
            case_identity: StaticCaseIdentity::Unresolved,
        },
    }
}

/// Plane-stress invariant; hypot scaling avoids squaring overflow.
fn von_mises(sx: f64, sy: f64, txy: f64) -> f64 {
    (sx - 0.5 * sy)
        .hypot(0.75_f64.sqrt() * sy)
        .hypot(3.0_f64.sqrt() * txy)
}

fn extract(
    print: &str,
    deck: &Deck,
    cases: &[LoadCase],
) -> Result<Vec<StaticShellStressCase>, String> {
    let materials: HashSet<_> = deck.materials.iter().map(|m| m.mid).collect();
    let properties: HashMap<_, _> = deck.shell_properties.iter().map(|p| (p.pid, p)).collect();
    let elements: HashMap<_, _> = deck
        .quads
        .iter()
        .chain(&deck.trias)
        .map(|e| (e.eid, e))
        .collect();
    if elements.is_empty()
        || elements.len() != deck.quads.len() + deck.trias.len()
        || properties.len() != deck.shell_properties.len()
        || materials.len() != deck.materials.len()
        || cases.is_empty()
        || cases.iter().map(|c| c.name).collect::<HashSet<_>>().len() != cases.len()
    {
        return Err("invalid/empty shell deck or requested static cases".into());
    }
    let mut output: BTreeMap<i64, Vec<StaticShellStressSample>> = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut pending_case = None;
    let mut active = None;
    let mut pending_fiber: Option<(i64, f64)> = None;
    for line in crate::nastran::text::splitlines(print) {
        if let Some((_, tail)) = line.rsplit_once("SUBCASE") {
            if let Some(id) = tail
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<i64>().ok())
            {
                pending_case = Some(id);
            }
        }
        if line.contains("S T R E S S E S") {
            if pending_fiber.is_some() {
                return Err("shell element is missing its second fiber row".into());
            }
            let nodes = if line.contains("C Q U A D 4") {
                Some(4)
            } else if line.contains("C T R I A 3") {
                Some(3)
            } else {
                None
            };
            active = match nodes {
                Some(nodes) => {
                    let sid = pending_case
                        .take()
                        .ok_or("shell stress page lacks explicit SUBCASE identity")?;
                    if sid < 1 || sid > cases.len() as i64 {
                        return Err("unexpected shell stress SUBCASE".into());
                    }
                    Some((sid, nodes))
                }
                None => None,
            };
            continue;
        }
        let Some((sid, nodes)) = active else {
            continue;
        };
        let tokens: Vec<_> = line.split_whitespace().collect();
        let first = tokens.first().copied().unwrap_or("");
        let (eid, numeric) =
            if first == "0" && tokens.get(1).is_some_and(|v| v.parse::<i64>().is_ok()) {
                if tokens.len() != 10 || pending_fiber.is_some() {
                    return Err("malformed or incomplete shell stress pair".into());
                }
                let eid: i64 = tokens[1].parse().map_err(|_| "invalid shell EID")?;
                if !seen.insert((sid, eid)) {
                    return Err("repeated shell stress element in one case".into());
                }
                (eid, &tokens[2..])
            } else if first.parse::<f64>().is_ok() && tokens.len() == 8 {
                let (eid, _) = pending_fiber.ok_or("unidentified second shell fiber row")?;
                (eid, tokens.as_slice())
            } else {
                continue;
            };
        let values: Vec<f64> = numeric
            .iter()
            .map(|v| v.parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| "unreadable shell stress tensor")?;
        if values.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite shell stress tensor".into());
        }
        let element = elements.get(&eid).ok_or("unknown shell stress element")?;
        let property = properties
            .get(&element.pid)
            .ok_or("missing shell property")?;
        if element.nodes.len() != nodes
            || !materials.contains(&property.mid1)
            || property.mid1 != property.mid2
        {
            return Err(
                "shell stress element/material attribution unsupported or inconsistent".into(),
            );
        }
        if let Some((_, previous)) = pending_fiber.take() {
            if previous * values[0] >= 0.0 {
                return Err("shell fiber pair lacks distinct top/bottom locations".into());
            }
        } else {
            pending_fiber = Some((eid, values[0]));
        }
        let vm = von_mises(values[1], values[2], values[3]);
        if !vm.is_finite() {
            return Err("nonfinite derived shell stress invariant".into());
        }
        output
            .entry(sid)
            .or_default()
            .push(StaticShellStressSample {
                element_id: eid,
                property_id: element.pid,
                material_id: property.mid1,
                fiber_distance_m: values[0],
                normal_x_pa: values[1],
                normal_y_pa: values[2],
                shear_xy_pa: values[3],
                von_mises_pa: vm,
            });
    }
    if pending_fiber.is_some()
        || output.len() != cases.len()
        || output.values().any(|v| v.len() != 2 * elements.len())
        || seen.len() != elements.len() * cases.len()
    {
        return Err("missing static cases or incomplete shell stress output".into());
    }
    Ok(cases
        .iter()
        .enumerate()
        .map(|(i, case)| StaticShellStressCase {
            subcase_id: i as i64 + 1,
            name: case.name,
            samples: output.remove(&(i as i64 + 1)).unwrap_or_default(),
        })
        .collect())
}

#[cfg(test)]
#[path = "static_stress_tests.rs"]
mod tests;
