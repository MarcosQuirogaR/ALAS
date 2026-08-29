// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py (`fig_section_shapes`)
// Reference: alas @ rust-port-baseline.

//! Overlaid section geometries of the top few picks and the current
//! section: is the ranking reaching for a thin low-Reynolds sliver, or a
//! sensible transport section? Sections are staggered vertically rather than
//! superimposed, since overlaid thin outlines read poorly on a dark panel.

use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke};
use crate::theme::get_palette;
use alas_geom::airfoil_library::AirfoilLibrary;
use alas_screen::AirfoilScreeningResult;

use super::{ok_candidates, refined_candidates};

/// Colour-blind-safe (Okabe-Ito derived) series palette, cycled by draw
/// order -- `series` upstream.
const SERIES_COLORS: [&str; 5] = ["#4f8cff", "#ff8a5c", "#43c59e", "#f5c518", "#c792ea"];

/// Dash patterns cycled alongside [`SERIES_COLORS`] -- `dashes` upstream.
/// `None` is solid; a trailing `(0.0, 0.0)` pair means "two-element
/// pattern", matching the reference's shorter tuples.
const DASHES: [Option<(f64, f64, f64, f64)>; 5] = [
    None,
    Some((6.0, 2.0, 0.0, 0.0)),
    Some((2.0, 1.6, 0.0, 0.0)),
    Some((7.0, 2.0, 1.5, 2.0)),
    Some((4.0, 1.5, 1.0, 1.5)),
];

/// The vertical stagger between overlaid sections -- `offset_step` upstream.
const OFFSET_STEP: f64 = 0.16;

struct Section {
    name: String,
    coords: Vec<(f64, f64)>,
    offset: f64,
    is_base: bool,
    color: &'static str,
    dash: Option<(f64, f64, f64, f64)>,
}

/// `None` when no candidate resolves to real coordinates in
/// [`AirfoilLibrary`].
pub fn fig_section_shapes(result: &AirfoilScreeningResult, theme: Option<&str>) -> Option<Scene> {
    let refined = refined_candidates(result);
    let cands = if !refined.is_empty() {
        refined
    } else {
        ok_candidates(result)
    };
    if cands.is_empty() {
        return None;
    }

    let mut names: Vec<String> = Vec::new();
    for c in cands.iter().take(4) {
        if !names.contains(&c.name) {
            names.push(c.name.clone());
        }
    }
    let baseline = result.baseline_airfoil.clone();
    if !baseline.is_empty() && !names.contains(&baseline) {
        names.push(baseline.clone());
    }

    let mut sections = Vec::new();
    let mut plotted = 0usize;
    for name in &names {
        let Some(airfoil) = AirfoilLibrary::get(name) else {
            continue;
        };
        if airfoil.coordinates.is_empty() {
            continue;
        }
        sections.push(Section {
            name: name.clone(),
            coords: airfoil.coordinates,
            offset: -OFFSET_STEP * plotted as f64,
            is_base: *name == baseline,
            color: SERIES_COLORS[plotted % SERIES_COLORS.len()],
            dash: DASHES[plotted % DASHES.len()],
        });
        plotted += 1;
    }
    if sections.is_empty() {
        return None;
    }

    let pal = get_palette(theme);
    let mut scene = Scene::new(650.0, 380.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Section shapes -- top picks vs current".to_owned());

    let (x_lo, x_hi, y_lo, y_hi) = combined_offset_bbox(&sections);
    let plot_rect = (60.0, 40.0, 520.0, 280.0);
    let (x_range, y_range) =
        equal_aspect_ranges(x_lo, x_hi, plot_rect.2, y_lo, y_hi, plot_rect.3, 0.08);
    let axes = Axes2D::new(plot_rect, x_range, y_range);
    axes.draw_frame_with_labels(&mut scene, pal, "x/c", "y/c  (sections offset vertically)");

    let mut legend_entries = Vec::new();
    for section in &sections {
        // Faint per-section chord reference line, spanning the full plotted
        // width -- `ax.axhline(offset, ...)`.
        let p0 = axes.map_point(axes.x_min, section.offset);
        let p1 = axes.map_point(axes.x_max, section.offset);
        let mut chord_color = Color::from_hex(pal.spine);
        chord_color.a = 90;
        scene.add(SceneElement::Line {
            p1: p0,
            p2: p1,
            stroke: Stroke::new(chord_color, 0.5),
        });

        let color = if section.is_base {
            Color::from_hex(pal.accent)
        } else {
            Color::from_hex(section.color)
        };
        let width = if section.is_base { 2.4 } else { 1.8 };
        let stroke = match (section.is_base, section.dash) {
            (false, Some((a, b, c, d))) => {
                let mut pattern = vec![a, b];
                if c > 0.0 || d > 0.0 {
                    pattern.push(c);
                    pattern.push(d);
                }
                Stroke {
                    color,
                    width,
                    dash_array: Some(pattern),
                }
            }
            _ => Stroke::new(color, width),
        };

        let offset_coords: Vec<(f64, f64)> = section
            .coords
            .iter()
            .map(|&(x, y)| (x, y + section.offset))
            .collect();
        axes.add_line_series(&mut scene, &offset_coords, stroke.clone());

        let label = if section.is_base {
            format!("{} (current)", section.name)
        } else {
            section.name.clone()
        };
        legend_entries.push((label, LegendMarker::Line(stroke)));
    }
    draw_legend(
        &mut scene,
        [plot_rect.0 + 8.0, plot_rect.1 + 8.0],
        &legend_entries,
        pal,
        8.0,
    );

    Some(scene)
}

/// Data-space bounding box of every staggered section outline.
fn combined_offset_bbox(sections: &[Section]) -> (f64, f64, f64, f64) {
    let (mut x_lo, mut x_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_lo, mut y_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for section in sections {
        for &(x, y) in &section.coords {
            x_lo = x_lo.min(x);
            x_hi = x_hi.max(x);
            let yo = y + section.offset;
            y_lo = y_lo.min(yo);
            y_hi = y_hi.max(yo);
        }
    }
    if !x_lo.is_finite() {
        return (0.0, 1.0, -0.2, 0.2);
    }
    (x_lo, x_hi, y_lo, y_hi)
}

/// Two axis ranges sharing one data-units-per-pixel scale, centred on each
/// data interval -- the substitute for `ax.set_aspect("equal")` this crate's
/// [`Axes2D`] has no flag for. Duplicated from
/// `families::geometry::shared::equal_aspect_ranges`, which is `pub(super)`
/// to that family and out of reach from here.
fn equal_aspect_ranges(
    u_lo: f64,
    u_hi: f64,
    u_px: f64,
    v_lo: f64,
    v_hi: f64,
    v_px: f64,
    pad_frac: f64,
) -> ((f64, f64), (f64, f64)) {
    let u_span = (u_hi - u_lo).abs().max(1e-6) * (1.0 + pad_frac);
    let v_span = (v_hi - v_lo).abs().max(1e-6) * (1.0 + pad_frac);
    let scale = (u_px / u_span).min(v_px / v_span);
    let u_half = u_px / scale / 2.0;
    let v_half = v_px / scale / 2.0;
    let u_c = (u_lo + u_hi) / 2.0;
    let v_c = (v_lo + v_hi) / 2.0;
    ((u_c - u_half, u_c + u_half), (v_c - v_half, v_c + v_half))
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_screen::AirfoilCandidateResult;

    fn candidate(name: &str) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: "ok".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_when_no_candidate_survived_screening() {
        let result = AirfoilScreeningResult::default();
        assert!(fig_section_shapes(&result, None).is_none());
    }

    #[test]
    fn returns_none_when_no_name_resolves_in_the_airfoil_library() {
        let result = AirfoilScreeningResult {
            candidates: vec![candidate("not-a-real-airfoil-name")],
            ..Default::default()
        };
        assert!(fig_section_shapes(&result, None).is_none());
    }

    #[test]
    fn resolves_real_coordinates_and_stacks_each_section_by_a_distinct_offset() {
        let result = AirfoilScreeningResult {
            baseline_airfoil: "naca0012".to_owned(),
            candidates: vec![candidate("naca2412"), candidate("naca0012")],
            ..Default::default()
        };
        let scene = fig_section_shapes(&result, None).expect("both names resolve");
        let polylines: Vec<_> = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .collect();
        assert_eq!(polylines.len(), 2);
    }

    #[test]
    fn combined_offset_bbox_of_no_sections_falls_back_to_a_finite_box() {
        let (x_lo, x_hi, y_lo, y_hi) = combined_offset_bbox(&[]);
        for v in [x_lo, x_hi, y_lo, y_hi] {
            assert!(v.is_finite());
        }
    }
}
