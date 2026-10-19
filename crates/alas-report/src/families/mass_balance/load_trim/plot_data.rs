// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The numbers behind the load-and-trim sheet, in physical coordinates.
//!
//! The sheet is drawn in balance-index space, which is a linear display
//! transform of (mass, CG). The exported data uses the physical pair: CG in
//! %MAC and aircraft mass in kg.

use super::{LimitVertex, LoadTrimSheetData};
use crate::plot_data::{CustomSeries, SeriesKind};
use crate::scene::Scene;

const KEY: &str = "load_trim";
const X_LABEL: &str = "CG position [%MAC]";
const Y_LABEL: &str = "Aircraft mass [kg]";

fn record(
    scene: &mut Scene,
    name: &str,
    kind: SeriesKind,
    points: Vec<(f64, f64)>,
    labels: Vec<String>,
) {
    if points.is_empty() {
        return;
    }
    scene.data_trace.note_custom(CustomSeries {
        key: KEY,
        x_label: X_LABEL,
        y_label: Y_LABEL,
        name,
        kind,
        x: points.iter().map(|p| p.0).collect(),
        y: points.iter().map(|p| p.1).collect(),
        labels,
    });
}

fn limit_edges(scene: &mut Scene, name: &str, limits: &[LimitVertex]) {
    let edge = |pick: fn(&LimitVertex) -> f64| -> Vec<(f64, f64)> {
        limits.iter().map(|v| (pick(v), v.mass_kg)).collect()
    };
    record(
        scene,
        &format!("{name} forward"),
        SeriesKind::Line,
        edge(|v| v.fwd_pct_mac),
        Vec::new(),
    );
    record(
        scene,
        &format!("{name} aft"),
        SeriesKind::Line,
        edge(|v| v.aft_pct_mac),
        Vec::new(),
    );
}

/// Record the limit sets, boarding potato, fuel curve, loading orders and the
/// worked loading steps of the sheet on the scene's data record.
pub(super) fn record_sheet(scene: &mut Scene, data: &LoadTrimSheetData) {
    if !crate::plot_data::is_active() {
        return;
    }
    limit_edges(scene, "ground CG limit", &data.ground_limits);
    limit_edges(scene, "takeoff CG limit", &data.takeoff_limits);
    limit_edges(scene, "flight CG limit", &data.flight_limits);
    limit_edges(scene, "landing CG limit", &data.landing_limits);
    limit_edges(scene, "zero-fuel CG limit", &data.zfw_limits);
    let potato = |pick: fn(&super::PotatoLevel) -> f64| -> Vec<(f64, f64)> {
        data.potato.iter().map(|l| (pick(l), l.mass_kg)).collect()
    };
    record(
        scene,
        "boarding potato forward",
        SeriesKind::Line,
        potato(|l| l.fwd_pct_mac),
        Vec::new(),
    );
    record(
        scene,
        "boarding potato aft",
        SeriesKind::Line,
        potato(|l| l.aft_pct_mac),
        Vec::new(),
    );
    record(
        scene,
        "fuel curve",
        SeriesKind::Line,
        data.fuel_curve
            .iter()
            .map(|&(mass, pct)| (pct, mass))
            .collect(),
        Vec::new(),
    );
    for path in &data.sequences {
        record(
            scene,
            &format!("loading order: {}", path.name),
            SeriesKind::Line,
            path.points.iter().map(|&(mass, pct)| (pct, mass)).collect(),
            Vec::new(),
        );
    }
    record(
        scene,
        "loading steps",
        SeriesKind::Points,
        data.steps.iter().map(|s| (s.pct_mac, s.mass_kg)).collect(),
        data.steps
            .iter()
            .map(|s| format!("{}: {}", s.state, s.item))
            .collect(),
    );
}
