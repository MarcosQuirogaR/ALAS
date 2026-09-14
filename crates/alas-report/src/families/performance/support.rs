// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures_extra.py (`_resolve_airport`, `_style_axes`)
// and alas/physics/performance.py (`static_thrust_to_weight`).
// Reference: alas @ rust-port-baseline.

//! Shared helpers for the matching-chart and landing/take-off figures:
//! airport-string resolution, sea-level static thrust-to-weight, a status
//! placeholder for an airport that cannot be resolved, and drawing
//! primitives (arrowheads, star and diamond markers) that neither figure
//! needed on its own but both do now.

use alas_config::airports::{Airport, UnknownAirport};
use alas_config::AlasConfig;

use crate::scene::{Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Standard gravity as `performance.py` writes it locally (`_G`), reproduced
/// here rather than imported since `alas_perf::performance`'s own copy is
/// crate-private -- the two are the same literal, `9.81`.
pub(super) const G: f64 = 9.81;

/// Resolve a configured airport string to a database entry -- `_resolve_airport`.
///
/// Tries an exact name-or-ICAO match first, which is all the shipped default
/// configuration ever needs since the table's own `name` field already
/// carries the "Name (ICAO)" form `AlasConfig::departure_airport`/
/// `arrival_airport` store; falls back to extracting the ICAO from a
/// trailing `(...)` and then the bare name before it, exactly as upstream's
/// `except KeyError` chain does for a string typed or edited by hand.
pub(super) fn resolve_airport(s: &str) -> Result<&'static Airport, UnknownAirport> {
    let s = s.trim();
    if let Ok(airport) = alas_config::airports::get(s) {
        return Ok(airport);
    }
    if let Some(open) = s.rfind('(') {
        if s.ends_with(')') {
            let icao = s[open + 1..s.len() - 1].trim();
            let name = s[..open].trim();
            for candidate in [icao, name] {
                if let Ok(airport) = alas_config::airports::get(candidate) {
                    return Ok(airport);
                }
            }
        }
    }
    Err(UnknownAirport(s.to_owned()))
}

/// Sea-level static thrust-to-weight at MTOW -- `static_thrust_to_weight`.
///
/// Upstream wraps the whole expression in a bare `except Exception` and
/// returns `default` on any failure; the only way this particular expression
/// fails is a division by a non-positive `MTOW * g`, which this reproduces as
/// an explicit guard rather than a caught panic.
pub(super) fn static_thrust_to_weight(config: &AlasConfig, default: f64) -> f64 {
    let n_eng = config.geometry.engine.spanwise_positions_m.len() as f64;
    let mtow_g = config.requirements.mtow_kg * G;
    if mtow_g <= 0.0 {
        return default;
    }
    let Ok(alas_config::ActiveEngineModel::Turbofan(spec)) = config.geometry.engine.active_model()
    else {
        return default;
    };
    n_eng * spec.rated_thrust_kn * 1000.0 / mtow_g
}

/// A centered placeholder figure for data that could not be resolved -- the
/// role upstream's `viz.figure_status_message` plays. No equivalent exists
/// yet anywhere in this crate (the retired figure audit listed it as
/// unported), so this reproduces just the two figures here need: a title and
/// a centered, possibly multi-line, message.
pub(super) fn status_message_scene(title: &str, message: &str, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    // `wrap_text` reflows any caller line that is still too long for the
    // canvas onto multiple rows; a caller-inserted `\n` remains a required
    // paragraph break (see `wrap_text`'s doc comment).
    let wrapped = crate::chart_kit::wrap_text(message, 84);
    let lines = wrapped.lines().collect::<Vec<_>>();
    const MESSAGE_TOP: f64 = 140.0;
    const LINE_HEIGHT: f64 = 16.0;
    const BOTTOM_MARGIN: f64 = 24.0;
    let height =
        (300.0_f64).max(MESSAGE_TOP + lines.len().max(1) as f64 * LINE_HEIGHT + BOTTOM_MARGIN);
    let mut scene = Scene::new(600.0, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    for (i, line) in lines.into_iter().enumerate() {
        scene.add(SceneElement::Text {
            text: line.to_owned(),
            pos: [300.0, MESSAGE_TOP + (i as f64) * LINE_HEIGHT],
            font_size: 12.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene
}

/// Format a non-negative magnitude with thousands separators, matching
/// Python's `f"{x:,.0f}"` -- the one formatting flag either figure uses that
/// Rust's own formatter has no direct equivalent for.
pub(super) fn format_thousands(value: f64) -> String {
    let rounded = value.round().max(0.0) as i64;
    let digits = rounded.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Draw a straight arrow from `p0` to `p1` in canvas pixel coordinates: a
/// shaft plus a filled triangular head, approximating matplotlib's
/// `arrowstyle="->"` with the primitives [`crate::scene`] offers.
pub(super) fn draw_arrow(scene: &mut Scene, p0: Point2D, p1: Point2D, color: Color, width: f64) {
    scene.add(SceneElement::Line {
        p1: p0,
        p2: p1,
        stroke: Stroke::new(color, width),
    });
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let head_len = 7.0;
    let head_w = 3.5;
    let base = [p1[0] - ux * head_len, p1[1] - uy * head_len];
    let left = [base[0] + px * head_w, base[1] + py * head_w];
    let right = [base[0] - px * head_w, base[1] - py * head_w];
    scene.add(SceneElement::Polygon {
        points: vec![p1, left, right],
        fill: Some(Fill::new(color)),
        stroke: None,
    });
}

/// Vertices of a diamond centered at `center` with "radius" `r` -- the V-n
/// diagram's cruise-point marker (matplotlib's `marker="D"`).
#[allow(dead_code)] // The enhanced envelope renderer owns the only current call site.
pub(super) fn diamond_points(center: Point2D, r: f64) -> Vec<Point2D> {
    vec![
        [center[0], center[1] - r],
        [center[0] + r, center[1]],
        [center[0], center[1] + r],
        [center[0] - r, center[1]],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_display_name_resolves_without_the_parenthesis_fallback() {
        let Ok(airport) = resolve_airport("London Heathrow (EGLL)") else {
            panic!("expected the shipped default departure airport to resolve");
        };
        assert_eq!(airport.icao, "EGLL");
    }

    #[test]
    fn a_bare_icao_inside_parens_resolves_when_the_full_string_does_not_match() {
        // A label the table does not carry verbatim but whose parenthesized
        // code it does -- the fallback branch upstream's `except KeyError` reaches.
        let Ok(airport) = resolve_airport("Some Other Label (EGLL)") else {
            panic!("expected the parenthesized ICAO fallback to resolve");
        };
        assert_eq!(airport.icao, "EGLL");
    }

    #[test]
    fn an_unresolvable_airport_string_is_an_error_not_a_panic() {
        assert!(resolve_airport("Nowhere At All").is_err());
    }

    #[test]
    fn static_thrust_to_weight_matches_hand_computed_ratio() {
        let mut cfg = AlasConfig::default();
        cfg.geometry.engine.spanwise_positions_m = vec![1.0, -1.0];
        cfg.geometry
            .engine
            .turbofan
            .as_mut()
            .unwrap()
            .rated_thrust_kn = 400.0;
        cfg.requirements.mtow_kg = 80_000.0;
        let tw = static_thrust_to_weight(&cfg, 0.30);
        let expected = 2.0 * 400.0 * 1000.0 / (80_000.0 * G);
        assert!((tw - expected).abs() < 1e-9);
    }

    #[test]
    fn static_thrust_to_weight_falls_back_when_mtow_is_non_positive() {
        let mut cfg = AlasConfig::default();
        cfg.requirements.mtow_kg = 0.0;
        assert_eq!(static_thrust_to_weight(&cfg, 0.30), 0.30);
    }

    #[test]
    fn thousands_formatting_places_a_comma_every_three_digits() {
        assert_eq!(format_thousands(1_234_567.4), "1,234,567");
        assert_eq!(format_thousands(42.0), "42");
        assert_eq!(format_thousands(0.0), "0");
    }

    #[test]
    fn a_diamond_has_four_vertices_at_the_given_radius() {
        let pts = diamond_points([1.0, 2.0], 5.0);
        assert_eq!(pts.len(), 4);
        for p in &pts {
            let r = ((p[0] - 1.0).powi(2) + (p[1] - 2.0).powi(2)).sqrt();
            assert!((r - 5.0).abs() < 1e-9);
        }
    }
}
