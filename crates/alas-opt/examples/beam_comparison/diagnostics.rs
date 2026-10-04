// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Read-only material accounting and analytical section evidence for the probe.

use alas_config::materials::MaterialSpec;
use alas_struct::analytical::StructuralAnalysisReport;
use alas_struct::{mesh::Deck, sizing::WingboxSizing};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

fn area(xyz: &[[f64; 3]]) -> Option<f64> {
    match xyz {
        [a, b, c] => Some(
            0.5 * norm(cross(
                std::array::from_fn(|i| b[i] - a[i]),
                std::array::from_fn(|i| c[i] - a[i]),
            )),
        ),
        [_, _, _, _] => {
            // NASTRAN quadrilateral area: 2x2 bilinear Jacobian quadrature.
            let mut total = 0.0;
            let gauss = 1.0 / 3.0_f64.sqrt();
            for xi in [-gauss, gauss] {
                for eta in [-gauss, gauss] {
                    let dx = [
                        -0.25 * (1.0 - eta),
                        0.25 * (1.0 - eta),
                        0.25 * (1.0 + eta),
                        -0.25 * (1.0 + eta),
                    ];
                    let de = [
                        -0.25 * (1.0 - xi),
                        -0.25 * (1.0 + xi),
                        0.25 * (1.0 + xi),
                        0.25 * (1.0 - xi),
                    ];
                    let a =
                        std::array::from_fn(|i| xyz.iter().zip(dx).map(|(p, d)| p[i] * d).sum());
                    let b =
                        std::array::from_fn(|i| xyz.iter().zip(de).map(|(p, d)| p[i] * d).sum());
                    total += norm(cross(a, b));
                }
            }
            Some(total)
        }
        _ => None,
    }
}

/// Full-wing shell families, with cap bars from total minus all shells.
/// MAT1 ids 1, 2 and 4 identify covers, webs and ribs in the product builder.
pub fn fe_inventory(deck: &Deck, full_wing_total_kg: f64) -> Option<Value> {
    let mut by_material = BTreeMap::<i64, f64>::new();
    for shell in deck.quads().iter().chain(deck.trias()) {
        let property = deck
            .shell_properties()
            .iter()
            .find(|p| p.pid == shell.pid)?;
        let material = deck.materials().iter().find(|m| m.mid == property.mid1)?;
        let xyz: Option<Vec<_>> = shell.nodes.iter().map(|id| deck.grid_xyz(*id)).collect();
        let mass = 2.0 * area(&xyz?)? * property.t * material.rho;
        *by_material.entry(material.mid).or_default() += mass;
    }
    let shell_total: f64 = by_material.values().sum();
    Some(json!({
        "caps": full_wing_total_kg - shell_total,
        "covers": by_material.get(&1), "webs": by_material.get(&2),
        "ribs_and_te_strips": by_material.get(&4),
        "all_shells": shell_total, "total": full_wing_total_kg,
    }))
}

/// Analytical first-order cap, cover and web EI, in N m^2, at the root.
/// Cover depth is linear between spars; caps include their own rectangle I.
pub fn root_ei(
    sizing: &WingboxSizing,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> Value {
    let caps: f64 = sizing
        .spars
        .iter()
        .map(|s| {
            let h = s.h[0];
            let t = s.t_cap[0];
            cap.e_pa * 2.0 * s.a_cap[0] * ((0.5 * (h - t)).powi(2) + t * t / 12.0)
        })
        .sum();
    let webs: f64 = sizing
        .spars
        .iter()
        .map(|s| web.e_pa * s.t_web * s.h[0].powi(3) / 12.0)
        .sum();
    let covers: f64 = sizing
        .spars
        .windows(2)
        .map(|s| {
            let a = s[0].h[0];
            let b = s[1].h[0];
            let width = (s[1].chord_fraction - s[0].chord_fraction) * sizing.chord[0];
            skin.e_pa * sizing.t_skin * width * (a * a + a * b + b * b) / 6.0
        })
        .sum();
    json!({"caps": caps, "covers": covers, "webs": webs, "total": caps+covers+webs})
}

/// Independent segment force and moment equilibrium, including exact point
/// mass jumps. Across a point, integrate its shear step at its declared Y;
/// otherwise the ordinary trapezoidal cell balance is exact for this model.
pub fn beam_equilibrium(
    response: &StructuralAnalysisReport,
    gravity_m_s2: f64,
    point_masses_kg: &[(f64, f64)],
) -> Value {
    json!(response
        .load_cases
        .iter()
        .map(|case| {
            let mut shear_error = 0.0_f64;
            let mut moment_error = 0.0_f64;
            for i in 0..case.y.len().saturating_sub(1) {
                let (left, right) = (case.y[i], case.y[i + 1]);
                let length = right - left;
                let midpoint = 0.5 * (left + right);
                let mut segment_force = 0.5 * (case.q_net[i] + case.q_net[i + 1]) * length;
                let mut segment_moment = 0.5 * (case.shear_n[i] + case.shear_n[i + 1]) * length;
                for &(position, mass) in point_masses_kg {
                    if left <= position && position < right {
                        let force = case.load_factor * gravity_m_s2 * mass;
                        segment_force -= force;
                        segment_moment += force * (midpoint - position);
                    }
                }
                shear_error =
                    shear_error.max((case.shear_n[i] - case.shear_n[i + 1] - segment_force).abs());
                moment_error = moment_error
                    .max((case.moment_nm[i] - case.moment_nm[i + 1] - segment_moment).abs());
            }
            let shear_scale = case
                .shear_n
                .iter()
                .map(|value| value.abs())
                .fold(1.0_f64, f64::max);
            let moment_scale = case
                .moment_nm
                .iter()
                .map(|value| value.abs())
                .fold(1.0_f64, f64::max);
            json!({"case": case.name,
                "maximum_force_balance_error_n": shear_error,
                "maximum_moment_balance_error_nm": moment_error,
                "relative_force_balance_error": shear_error / shear_scale,
                "relative_moment_balance_error": moment_error / moment_scale,
            })
        })
        .collect::<Vec<_>>())
}
