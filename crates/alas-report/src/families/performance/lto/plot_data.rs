// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The required and available field distances drawn as bars, as plain data.

use crate::plot_data::{CustomSeries, SeriesKind};
use crate::scene::Scene;

/// Record required and available distances [m], one row per distance type.
pub(super) fn record_distances(scene: &mut Scene, labels: &[&str; 4], rows: [[f64; 4]; 2]) {
    for (name, distances) in ["required distance", "available distance"]
        .into_iter()
        .zip(rows)
    {
        scene.data_trace.note_custom(CustomSeries {
            key: "field_distances",
            x_label: "Distance type",
            y_label: "Distance [m]",
            name,
            kind: SeriesKind::Points,
            x: (0..labels.len()).map(|i| i as f64).collect(),
            y: distances.to_vec(),
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
        });
    }
}

/// Record the take-off decision speeds V1, VR and V2 in knots (the unit the
/// figure prints; `y_si` converts to m/s), one row per speed.
pub(super) fn record_v_speeds(scene: &mut Scene, speeds_ms: [(&str, f64); 3], ms_to_kt: f64) {
    scene.data_trace.note_custom(CustomSeries {
        key: "v_speeds",
        x_label: "Speed type",
        y_label: "Speed [kt]",
        name: "take-off speeds",
        kind: SeriesKind::Points,
        x: (0..speeds_ms.len()).map(|i| i as f64).collect(),
        y: speeds_ms.iter().map(|(_, v)| v * ms_to_kt).collect(),
        labels: speeds_ms.iter().map(|(n, _)| (*n).to_owned()).collect(),
    });
}
