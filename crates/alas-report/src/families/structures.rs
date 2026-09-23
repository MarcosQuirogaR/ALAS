// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Wingbox sizing preview, sizing summary and internal loads figures.
//!
//! Three of `visualization.py`'s seven `figure_structures_*` functions live
//! here: [`figure_structures_designer_preview`] (L5634-5705),
//! [`figure_structures_sizing`] (L5706-5818) and [`figure_structures_loads`]
//! (L5819-5906). The other four -- stress, modes, vibration, patran -- are a
//! separate module, `families::structures_dynamics`.
//!
//! Split into one file per figure to stay under this repository's per-file
//! line limit, the way `alas-geom::aircraft::airfoil` splits across a same-named
//! directory: [`designer_preview`], [`sizing`] and [`loads`] each contribute
//! one public figure function, and this file carries what all three share --
//! the rib-perpendicular spar-line geometry (`_true_spar_xy`), the wingbox
//! outline, the tab10 spar palette, and the status-message fallback every
//! `structural_result`-driven figure degrades to when the analysis did not
//! run or failed.

mod designer_preview;
mod loads;
mod sizing;

pub use super::structures_dynamics::{
    figure_structures_modes, figure_structures_patran, figure_structures_stress,
    figure_structures_vibration,
};
pub use designer_preview::figure_structures_designer_preview;
pub use loads::figure_structures_loads;
pub use sizing::figure_structures_sizing;

use crate::scene::{Axes2D, Color, Scene, Stroke};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_pipeline::structural::StructuralAnalysisResult;

pub(super) use crate::families::common::linspace;

/// `plt_cm_tab10()`'s six colors, in order. `Color::from_hex` special-cases
/// `tab:blue/orange/green/red` (the four the rest of this crate already
/// needed); `tab:purple` and `tab:brown` are spelled out as literal hex here
/// rather than added to that shared table, the same choice
/// `families::mission` already made for its own two extra Tableau colors.
pub(super) const SPAR_COLORS: [&str; 6] = [
    "tab:blue",
    "tab:orange",
    "tab:green",
    "tab:red",
    "#9467bd", // tab:purple
    "#8c564b", // tab:brown
];

/// `tab:gray`, matplotlib's Tableau palette -- the fallback color for a load
/// case name none of `figure_structures_loads`'s three known cases matches.
pub(super) const TAB_GRAY: &str = "#7f7f7f";

/// The four semi-wing mass components, in the order upstream's
/// `mass_breakdown_kg` dict lists them (`structural_sizing.py:246-251`) and
/// therefore the order the mass-breakdown chart presents them.
pub(super) fn mass_breakdown_items(
    mb: &alas_struct::sizing::MassBreakdown,
) -> [(&'static str, f64); 4] {
    [
        ("Spar caps", mb.spar_caps),
        ("Spar webs", mb.spar_webs),
        ("Skin", mb.skin),
        ("Ribs", mb.ribs),
    ]
}

/// Format a mass in kilograms the way Python's `f"{value:,.0f}"` does:
/// rounded to the nearest integer, with a thousands separator. There is no
/// standard-library equivalent, so this reproduces the two behaviours that
/// matter here -- banker's-rounding-free `f64::round` (Python's format spec
/// uses round-half-to-even on the *decimal* representation, but at zero
/// decimal places both agree for the mass magnitudes this program ever
/// prints) and comma grouping from the right.
pub(super) fn format_thousands(value: f64) -> String {
    let rounded = value.round();
    let sign = if rounded < 0.0 { "-" } else { "" };
    let digits = format!("{:.0}", rounded.abs());
    let mut grouped = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{}", grouped.chars().rev().collect::<String>())
}

/// `_true_spar_xy`: the chordwise (X) position of the spar at `frac` chord
/// fraction, at each of `y_stations`, using the same rib-perpendicular
/// reference-line geometry `alas-struct::mesh` actually meshes rather than a
/// naive `x_le + frac*local_chord` straight percent-chord line -- see the
/// Python docstring this ports for why that naive line silently diverges by
/// metres near the root. Returns `NaN` at a station past a partial-span
/// spar's own break-station endpoint or past a root-adjacent transition
/// rib's truncated reach, matching the reference's "don't draw a point the
/// real mesh never places" convention exactly (matplotlib simply stops
/// drawing a polyline at a `NaN`; [`spar_line_series`] reproduces that by
/// breaking into separate runs).
pub(super) fn true_spar_xy(wsg: &WingStructureGeometry, y_stations: &[f64], frac: f64) -> Vec<f64> {
    let frac_idx = wsg
        .spar_fracs
        .iter()
        .position(|&f| f == frac)
        .unwrap_or_else(|| {
            wsg.spar_fracs
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1 - frac).abs().total_cmp(&(b.1 - frac).abs()))
                .map(|(i, _)| i)
                .unwrap_or(0)
        });

    y_stations
        .iter()
        .map(|&y| {
            let eta = y / wsg.semi_span;
            let x_le_val = wsg.x_le(eta);
            let (aft_x, aft_y) = wsg.rib_vector(eta);
            let (l_nominal, l_actual) = wsg.get_rib_lengths(y, x_le_val, aft_x, aft_y);
            let s_spars = wsg.compute_spar_intersections(y, x_le_val, aft_x, aft_y, l_nominal);
            match s_spars[frac_idx] {
                Some(s) if s <= l_actual + 1e-9 => x_le_val + s * aft_x,
                _ => f64::NAN,
            }
        })
        .collect()
}

/// Draw a line series, breaking it into separate polylines at any `NaN` --
/// the same "stop drawing here" convention matplotlib applies and
/// [`true_spar_xy`]'s own docs describe. Shared by every spar-line panel in
/// this family.
pub(super) fn spar_line_series(
    axes: &Axes2D,
    scene: &mut Scene,
    y: &[f64],
    x: &[f64],
    stroke: Stroke,
) {
    let mut run: Vec<(f64, f64)> = Vec::new();
    for (&yv, &xv) in y.iter().zip(x) {
        if xv.is_finite() {
            run.push((yv, xv));
        } else if !run.is_empty() {
            axes.add_line_series(scene, &run, stroke.clone());
            run.clear();
        }
    }
    if !run.is_empty() {
        axes.add_line_series(scene, &run, stroke);
    }
}

/// Return structural spar stations with an honest root point at `Y = 0`.
///
/// The first transition rib can be truncated by the root-plane cut, so the
/// mesh-derived series may begin at a positive span station or contain a
/// `NaN` at zero. The root rib is deliberately streamwise; its intersection
/// with a spar is therefore the existing root leading edge plus the spar
/// fraction of the local root chord. Prepending that point preserves the
/// geometry represented by the station data and keeps the rendered trace
/// continuous.
pub(super) fn root_connected_spar_series(
    wsg: &WingStructureGeometry,
    y_stations: &[f64],
    frac: f64,
) -> (Vec<f64>, Vec<f64>) {
    let mut y = y_stations.to_vec();
    let mut x = true_spar_xy(wsg, y_stations, frac);
    let root_x = true_spar_xy(wsg, &[0.0], frac)
        .first()
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or_else(|| wsg.x_le(0.0) + frac * wsg.local_chord(0.0));

    match (y.first().copied(), x.first_mut()) {
        (Some(first_y), Some(first_x)) if first_y.abs() <= 1e-9 => {
            if !first_x.is_finite() {
                *first_x = root_x;
            }
        }
        (Some(_), _) => {
            y.insert(0, 0.0);
            x.insert(0, root_x);
        }
        (None, _) => {
            y.push(0.0);
            x.push(root_x);
        }
    }

    // A root-adjacent transition rib can make one or more of the original
    // stations non-drawable. Keeping those NaNs immediately after the root
    // would leave the root as an isolated point and start the visible spar at
    // the first later station. Remove only that leading gap so the root joins
    // the first real mesh intersection; later NaNs still delimit genuine
    // partial-span breaks.
    let first_finite_after_root = x
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, value)| value.is_finite().then_some(index));
    if let Some(index) = first_finite_after_root {
        if index > 1 {
            y.drain(1..index);
            x.drain(1..index);
        }
    }
    (y, x)
}

/// The wingbox outline every planform panel starts with: leading- and
/// trailing-edge lines plus the root and tip closing segments.
pub(super) fn draw_wing_outline(
    scene: &mut Scene,
    axes: &Axes2D,
    y: &[f64],
    le: &[f64],
    te: &[f64],
    color: Color,
    width: f64,
) {
    if y.is_empty() || le.is_empty() || te.is_empty() || y.len() != le.len() || le.len() != te.len()
    {
        return;
    }
    let stroke = Stroke::new(color, width);
    let le_pts: Vec<(f64, f64)> = y.iter().zip(le).map(|(&yv, &lv)| (yv, lv)).collect();
    let te_pts: Vec<(f64, f64)> = y.iter().zip(te).map(|(&yv, &tv)| (yv, tv)).collect();
    axes.add_line_series(scene, &le_pts, stroke.clone());
    axes.add_line_series(scene, &te_pts, stroke.clone());
    if let (Some(&y0), Some(&y1)) = (y.first(), y.last()) {
        axes.add_line_series(scene, &[(y0, le[0]), (y0, te[0])], stroke.clone());
        let last = le.len() - 1;
        axes.add_line_series(scene, &[(y1, le[last]), (y1, te[last])], stroke);
    }
}

/// The chordwise (vertical-axis) extent a planform panel needs to show both
/// LE and TE with a small margin.
pub(super) fn chord_bounds(le: &[f64], te: &[f64]) -> (f64, f64) {
    let lo = le.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = te.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).max(1e-6);
    (lo - span * 0.08, hi + span * 0.08)
}

/// `_structures_unavailable_message`: `None` when the analysis is ok and a
/// figure should render its real content, `Some(message)` when it should
/// degrade to [`status_message_scene`] instead.
pub(super) fn structures_unavailable_message(
    result: Option<&StructuralAnalysisResult>,
) -> Option<String> {
    match result {
        None => Some(
            "Structural analysis was not run for this design (Advanced Settings -> Structural Analysis)."
                .to_owned(),
        ),
        Some(r) if r.status != "ok" => Some(format!(
            "Structural analysis failed: {}",
            r.error.as_deref().unwrap_or("unknown error")
        )),
        Some(_) => None,
    }
}

/// `figure_status_message`, scoped to the `ok=False` branch this family's
/// two `structural_result`-driven figures actually reach. The shared
/// placeholder in [`crate::status_figure`] draws it, so the reason text wraps
/// inside the canvas instead of running past it.
pub(super) fn status_message_scene(title: &str, message: &str, theme: Option<&str>) -> Scene {
    crate::status_figure::figure_status_message(title, message, false, theme)
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn format_thousands_groups_from_the_right_and_keeps_the_sign() {
        assert_eq!(format_thousands(6200.0), "6,200");
        assert_eq!(format_thousands(999.0), "999");
        assert_eq!(format_thousands(1_000_000.0), "1,000,000");
        assert_eq!(format_thousands(-2500.4), "-2,500");
    }

    #[test]
    fn mass_breakdown_items_lists_the_four_components_in_upstream_dict_order() {
        let mb = alas_struct::sizing::MassBreakdown {
            spar_caps: 1.0,
            spar_webs: 2.0,
            skin: 3.0,
            ribs: 4.0,
        };
        let items = mass_breakdown_items(&mb);
        assert_eq!(
            items.map(|(name, _)| name),
            ["Spar caps", "Spar webs", "Skin", "Ribs"]
        );
        assert_eq!(items.map(|(_, v)| v), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn chord_bounds_pads_the_le_te_extent_symmetrically() {
        let le = [0.0, 1.0];
        let te = [4.0, 3.0];
        let (lo, hi) = chord_bounds(&le, &te);
        assert!(lo < 0.0);
        assert!(hi > 4.0);
    }

    #[test]
    fn linspace_pins_both_endpoints() {
        assert_eq!(linspace(0.0, 1.0, 5), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(2.0, 3.0, 1), vec![2.0]);
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn structures_unavailable_message_distinguishes_absent_from_failed() {
        assert!(structures_unavailable_message(None)
            .unwrap()
            .contains("not run"));

        let mut failed = StructuralAnalysisResult {
            status: "error".to_owned(),
            error: Some("bad material".to_owned()),
            ..Default::default()
        };
        assert!(structures_unavailable_message(Some(&failed))
            .unwrap()
            .contains("bad material"));

        failed.status = "ok".to_owned();
        assert!(structures_unavailable_message(Some(&failed)).is_none());
    }
}
