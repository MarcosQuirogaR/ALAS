// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Show parasite, induced, and wave drag at the design point.
pub fn figure_drag_breakdown(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag Breakdown at Design CL".to_owned());
    draw_title(&mut scene, "Drag Breakdown at Design CL", pal);
    scene.suppress_derived_title();

    let p = &report.polar;
    let dp = &report.design_point;
    let fit = &report.polar_fit;

    // A failed fit carries no coefficients, so without a polar sample at the
    // design point there is no breakdown to draw.
    let breakdown_available = nearest_cl_index(&p.cl, dp.cl).is_some() || fit.status.is_fitted();
    // The components are interpolated to the design CL, so the bar total and the CL / L/D the
    // figure states refer to the same point. A design CL outside the swept range falls back to
    // the nearest sample, and the figure says which CL that is.
    let mut nearest_sample_cl = None;
    let (cd_p, cd_i, cd_w) = match interpolate_components(p, dp.cl) {
        Some(((cd_p, cd_i, cd_w), exact)) => {
            if !exact {
                nearest_sample_cl = nearest_cl_index(&p.cl, dp.cl).map(|idx| p.cl[idx]);
            }
            (cd_p, cd_i, cd_w)
        }
        None if fit.status.is_fitted() => (fit.cd0, fit.k * dp.cl * dp.cl, 0.0),
        None => (0.0, 0.0, 0.0),
    };
    let cd_total = (cd_p + cd_i + cd_w).max(1e-9);

    let ax_bar = Axes2D::new(
        (80.0, 60.0, 300.0, 340.0),
        (0.0, 1.0),
        (0.0, cd_total * 1.3),
    );
    // A single stacked bar has no numeric x quantity: no x ticks, a caption.
    crate::chart_kit::draw_axes_without_x_tick_labels(&ax_bar, &mut scene, pal, None, Some("CD"));
    scene.add(SceneElement::Text {
        text: "Design point".to_owned(),
        pos: [
            ax_bar.left + ax_bar.width * 0.5,
            ax_bar.top + ax_bar.height + 10.0,
        ],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    panel_title(&mut scene, &ax_bar, "Drag breakdown at design CL", pal);
    if let Some(sample_cl) = nearest_sample_cl {
        scene.add(SceneElement::Text {
            text: format!(
                "Design CL is outside the swept polar: bars are the nearest sample, CL {sample_cl:.3}"
            ),
            pos: [ax_bar.left + ax_bar.width * 0.5, ax_bar.top + ax_bar.height + 30.0],
            font_size: 8.5,
            color: Color::from_hex("#c0392b"),
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
    }
    if !breakdown_available {
        scene.add(SceneElement::Text {
            text: "Not available (polar fit failed)".to_owned(),
            pos: [
                ax_bar.left + ax_bar.width * 0.5,
                ax_bar.top + ax_bar.height * 0.5,
            ],
            font_size: 10.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });
    }

    let bar_w = 90.0;
    let x_center = ax_bar.map_point(0.5, 0.0)[0];
    let mut y0 = 0.0;
    for (val, color, name) in [
        (cd_p, "tab:blue", "CD0"),
        (cd_i, "tab:orange", "CDi"),
        (cd_w, "tab:red", "CDwave"),
    ] {
        let p_top = ax_bar.map_point(0.5, y0 + val);
        let p_bot = ax_bar.map_point(0.5, y0);
        scene.add(SceneElement::Rect {
            x: x_center - bar_w * 0.5,
            y: p_top[1],
            width: bar_w,
            height: (p_bot[1] - p_top[1]).max(1.0),
            rx: 2.0,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: Some(Stroke::new(Color::from_hex(pal.title), 1.0)),
        });
        let bar_height = (p_bot[1] - p_top[1]).max(1.0);
        if bar_height >= 22.0 {
            scene.add(SceneElement::Text {
                text: format!("{name}\n{val:.4}"),
                pos: [x_center, (p_top[1] + p_bot[1]) * 0.5],
                font_size: 8.0,
                color: Color::from_hex("#ffffff"),
                align: TextAlign::Center,
                baseline: TextBaseline::Middle,
                angle_deg: 0.0,
                bold: true,
            });
        } else {
            scene.add(SceneElement::Text {
                text: format!("{name} = {val:.4}"),
                pos: [x_center + bar_w * 0.5 + 8.0, p_bot[1] - 3.0],
                font_size: 8.0,
                color: Color::from_hex(pal.tick),
                align: TextAlign::Left,
                baseline: TextBaseline::Bottom,
                angle_deg: 0.0,
                bold: true,
            });
        }
        y0 += val;
    }

    let ax_eff = Axes2D::new(
        (470.0, 60.0, 360.0, 340.0),
        padded_range(p.cl.iter().copied().chain(std::iter::once(dp.cl)), 0.08),
        padded_range(
            p.l_over_d
                .iter()
                .copied()
                .chain(std::iter::once(dp.l_over_d)),
            0.08,
        ),
    );
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
    panel_title(&mut scene, &ax_eff, "Efficiency curve", pal);
    let eff_pts: Vec<(f64, f64)> =
        p.cl.iter()
            .copied()
            .zip(p.l_over_d.iter().copied())
            .collect();
    ax_eff.add_line_series(
        &mut scene,
        &eff_pts,
        Stroke::new(Color::from_hex("tab:purple"), 2.0),
    );
    add_markers(&ax_eff, &mut scene, &eff_pts, Color::from_hex("tab:purple"));
    let y_range = padded_range(
        p.l_over_d
            .iter()
            .copied()
            .chain(std::iter::once(dp.l_over_d)),
        0.08,
    );
    let x_range = padded_range(p.cl.iter().copied().chain(std::iter::once(dp.cl)), 0.08);
    vline(
        &ax_eff,
        &mut scene,
        dp.cl,
        y_range,
        Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0),
    );
    hline(
        &ax_eff,
        &mut scene,
        dp.l_over_d,
        x_range,
        Stroke::dashed(Color::from_hex("#e6c200"), 1.2, 4.0, 4.0),
    );
    let design_pt = ax_eff.map_point(dp.cl, dp.l_over_d);
    scene.add(SceneElement::Circle {
        center: design_pt,
        radius: 4.5,
        fill: Some(Fill::new(Color::from_hex("#ff0000"))),
        stroke: None,
    });
    // Flag a design CL past the L/D peak of the swept polar (labelled, never
    // moved): the point is off the efficiency optimum.
    if let Some(peak) = p
        .l_over_d
        .iter()
        .copied()
        .zip(p.cl.iter().copied())
        .filter(|(ld, cl)| ld.is_finite() && cl.is_finite())
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        let (peak_ld, peak_cl) = peak;
        if dp.cl.is_finite() && peak_cl > 0.0 && (dp.cl - peak_cl).abs() > 0.1 * peak_cl {
            let side = if dp.cl > peak_cl { "above" } else { "below" };
            scene.add(SceneElement::Text {
                text: format!(
                    "Off L/D peak: design CL {:.2} is {side} the peak (CL {:.2}, L/D {:.1}); \
                     design L/D is {:.0} % of peak",
                    dp.cl,
                    peak_cl,
                    peak_ld,
                    100.0 * dp.l_over_d / peak_ld.max(1e-9),
                ),
                pos: [
                    ax_eff.left + ax_eff.width * 0.5,
                    ax_eff.top + ax_eff.height + 46.0,
                ],
                font_size: 8.5,
                color: Color::from_hex("#c0392b"),
                align: TextAlign::Center,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: true,
            });
        }
    }
    draw_legend(
        &mut scene,
        [
            ax_eff.left + ax_eff.width - 145.0,
            ax_eff.top + ax_eff.height - 58.0,
        ],
        &[
            (
                "Efficiency".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("tab:purple"), 2.0)),
            ),
            (
                format!("Design CL = {:.3}", dp.cl),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0)),
            ),
            (
                format!("L/D = {:.2}", dp.l_over_d),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("#e6c200"), 1.2, 4.0, 4.0)),
            ),
        ],
        pal,
        8.0,
    );

    scene
}

/// The parasite, induced and wave drag of `polar` linearly interpolated to `target_cl`, and
/// whether `target_cl` lies inside the swept CL range. Outside it the nearest sample is returned
/// instead (`false`). `None` for an empty or non-finite polar.
fn interpolate_components(
    polar: &alas_aero::analysis::PolarSweep,
    target_cl: f64,
) -> Option<((f64, f64, f64), bool)> {
    let n = polar
        .cl
        .len()
        .min(polar.cd_parasite.len())
        .min(polar.cd_induced.len())
        .min(polar.cd_wave.len());
    if n == 0 || !target_cl.is_finite() {
        return None;
    }
    let at = |i: usize| (polar.cd_parasite[i], polar.cd_induced[i], polar.cd_wave[i]);
    for i in 0..n.saturating_sub(1) {
        let (c0, c1) = (polar.cl[i], polar.cl[i + 1]);
        let (lo, hi) = (c0.min(c1), c0.max(c1));
        if c1 != c0 && target_cl >= lo && target_cl <= hi {
            let t = (target_cl - c0) / (c1 - c0);
            let (a, b) = (at(i), at(i + 1));
            let lerp = |x: f64, y: f64| x + t * (y - x);
            return Some(((lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2)), true));
        }
    }
    let idx = nearest_cl_index(&polar.cl[..n], target_cl)?;
    Some((at(idx), n == 1 && polar.cl[0] == target_cl))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod interpolation_tests {
    use super::*;

    fn polar() -> alas_aero::analysis::PolarSweep {
        alas_aero::analysis::PolarSweep {
            alpha_deg: vec![0.0, 2.0, 4.0],
            geometric_alpha_deg: vec![0.0, 2.0, 4.0],
            compressible_mach: Some(0.0),
            cl: vec![0.4, 0.6, 0.8],
            cd: vec![0.03, 0.04, 0.06],
            cd_induced: vec![0.01, 0.02, 0.04],
            cd_wave: vec![0.0, 0.0, 0.002],
            cd_parasite: vec![0.02, 0.02, 0.018],
            cm: vec![0.0; 3],
            l_over_d: vec![10.0, 15.0, 13.0],
        }
    }

    #[test]
    fn components_are_interpolated_to_the_design_cl() {
        let ((parasite, induced, wave), exact) =
            interpolate_components(&polar(), 0.5).expect("interpolates");
        assert!(exact);
        assert!((parasite - 0.02).abs() < 1e-12);
        assert!((induced - 0.015).abs() < 1e-12);
        assert!(wave.abs() < 1e-12);
        let ((_, induced, _), _) = interpolate_components(&polar(), 0.7).expect("interpolates");
        assert!((induced - 0.03).abs() < 1e-12);
    }

    #[test]
    fn a_design_cl_outside_the_sweep_falls_back_to_the_nearest_sample() {
        let ((_, induced, _), exact) =
            interpolate_components(&polar(), 1.2).expect("nearest sample");
        assert!(!exact);
        assert!((induced - 0.04).abs() < 1e-12);
    }
}
