// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(super) fn vline(axes: &Axes2D, scene: &mut Scene, x: f64, y_range: (f64, f64), stroke: Stroke) {
    axes.add_line_series(scene, &[(x, y_range.0), (x, y_range.1)], stroke);
}

pub(super) fn hline(axes: &Axes2D, scene: &mut Scene, y: f64, x_range: (f64, f64), stroke: Stroke) {
    axes.add_line_series(scene, &[(x_range.0, y), (x_range.1, y)], stroke);
}

pub(super) fn add_markers(axes: &Axes2D, scene: &mut Scene, points: &[(f64, f64)], color: Color) {
    for &(x, y) in points {
        scene.add(SceneElement::Circle {
            center: axes.map_point(x, y),
            radius: 2.8,
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(Color::from_hex("#ffffff"), 0.6)),
        });
    }
}

/// Four-panel polar set: lift curve, drag polar, efficiency, longitudinal
/// stability, as implemented by `figure_aero_panel`.
pub fn figure_aero_panel(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    figure_aero_panel_with_cl_max(report, None, theme)
}

/// The polar set with the run's own clean CL_max (`performance.cl_max_clean`
/// of the run configuration), at which the curves stop. `None` draws the full
/// sweep and says no CL_max was available.
pub fn figure_aero_panel_with_cl_max(
    report: &AnalysisReport,
    cl_max: Option<f64>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 700.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Aerodynamic Polar Set".to_owned());
    draw_title(&mut scene, "Aerodynamic Polar Set", pal);
    scene.suppress_derived_title();
    // The vortex lattice is linear: it has no stall. The curves stop at the
    // estimated wing CL_max, which is marked, instead of running to CL 1.8+.
    let cl_max = cl_max.filter(|value| value.is_finite() && *value > 0.0);
    let kept = cl_max.map_or(report.polar.cl.len(), |limit| {
        report
            .polar
            .cl
            .iter()
            .position(|&cl| cl > limit)
            .unwrap_or(report.polar.cl.len())
    });
    let truncated = truncate_polar(&report.polar, kept.max(2));
    let p = &truncated;
    // The reporting angle (Prandtl-Glauert relabel), the one axis every figure
    // that draws this sweep uses; see `ReportingAxis`.
    let alpha_axis: &[f64] = &p.alpha_deg;
    // Zero-lift angle from the first two points (linear extrapolation).
    let alpha_zero_lift = match (alpha_axis.get(..2), p.cl.get(..2)) {
        (Some(a), Some(c)) if (c[1] - c[0]).abs() > 1e-9 => {
            let alpha0 = a[0] - c[0] * (a[1] - a[0]) / (c[1] - c[0]);
            alpha0.is_finite().then_some(alpha0)
        }
        _ => None,
    };
    let alpha_range = padded_range(alpha_axis.iter().copied().chain(alpha_zero_lift), 0.05);
    let cl_range = padded_range(p.cl.iter().copied().chain(std::iter::once(0.0)), 0.08);
    let cd_range = padded_range(
        p.cd.iter().copied().chain(p.cd_induced.iter().copied()),
        0.08,
    );
    let ld_range = padded_range(p.l_over_d.iter().copied(), 0.08);
    let cm_range = padded_range(p.cm.iter().copied().chain(std::iter::once(0.0)), 0.15);

    let blue = Stroke::new(Color::from_hex("tab:blue"), 1.8);

    let ax_lift = Axes2D::new((70.0, 60.0, 340.0, 260.0), alpha_range, cl_range);
    ax_lift.draw_frame_with_labels(&mut scene, pal, "\u{03b1} [deg]", "CL");
    let lift_pts: Vec<(f64, f64)> = alpha_axis
        .iter()
        .copied()
        .zip(p.cl.iter().copied())
        .collect();
    ax_lift.add_line_series(&mut scene, &lift_pts, blue.clone());
    add_markers(&ax_lift, &mut scene, &lift_pts, Color::from_hex("tab:blue"));
    if let (Some(alpha0), Some(&alpha_first), Some(&cl_first)) =
        (alpha_zero_lift, alpha_axis.first(), p.cl.first())
    {
        // The sweep starts above CL = 0; the dotted segment is the linear
        // extension to the zero-lift angle, not computed points.
        ax_lift.add_line_series(
            &mut scene,
            &[(alpha0, 0.0), (alpha_first, cl_first)],
            Stroke::dashed(Color::from_hex("tab:blue"), 1.0, 2.0, 3.0),
        );
        let at = ax_lift.map_point(alpha0, 0.0);
        scene.add(SceneElement::Text {
            text: format!("extrapolated \u{03b1}0L = {alpha0:.1}\u{b0}"),
            pos: [at[0] + 10.0, at[1] + 6.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    if let Some(limit) = cl_max.filter(|_| kept < report.polar.cl.len()) {
        hline(
            &ax_lift,
            &mut scene,
            limit,
            alpha_range,
            Stroke::dashed(Color::from_hex("#d62728"), 1.0, 4.0, 3.0),
        );
        let at = ax_lift.map_point(alpha_range.0, limit);
        scene.add(SceneElement::Text {
            text: format!("Run CLmax {limit:.2}: curves stop here (lattice has no stall)"),
            pos: [at[0] + 6.0, at[1] + 3.0],
            font_size: 8.0,
            color: Color::from_hex("#d62728"),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    if cl_max.is_none() {
        scene.add(SceneElement::Text {
            text: "No CLmax in this run: full sweep drawn (the lattice has no stall)".to_owned(),
            pos: [ax_lift.left, ax_lift.top + ax_lift.height + 44.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    panel_title(&mut scene, &ax_lift, "Lift curve (\u{03b1} vs CL)", pal);

    let ax_drag = Axes2D::new((500.0, 60.0, 340.0, 260.0), cd_range, cl_range);
    ax_drag.draw_frame_with_labels(&mut scene, pal, "CD", "CL");
    let total_pts: Vec<(f64, f64)> = p.cd.iter().copied().zip(p.cl.iter().copied()).collect();
    let induced_pts: Vec<(f64, f64)> = p
        .cd_induced
        .iter()
        .copied()
        .zip(p.cl.iter().copied())
        .collect();
    ax_drag.add_line_series(
        &mut scene,
        &total_pts,
        Stroke::new(Color::from_hex("tab:red"), 1.8),
    );
    add_markers(&ax_drag, &mut scene, &total_pts, Color::from_hex("tab:red"));
    ax_drag.add_line_series(
        &mut scene,
        &induced_pts,
        Stroke::dashed(Color::from_hex("tab:blue"), 1.4, 5.0, 3.0),
    );
    panel_title(&mut scene, &ax_drag, "Drag polar", pal);
    draw_legend(
        &mut scene,
        [ax_drag.left + 8.0, ax_drag.top + 8.0],
        &[
            (
                "Total (corrected)".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("tab:red"), 1.8)),
            ),
            (
                "Induced (VLM)".to_owned(),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:blue"), 1.4, 5.0, 3.0)),
            ),
        ],
        pal,
        8.0,
    );

    // Keep the lower-panel axis labels inside the canvas at card resolution.
    let ax_eff = Axes2D::new((70.0, 390.0, 340.0, 250.0), cl_range, ld_range);
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
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
    vline(
        &ax_eff,
        &mut scene,
        report.design_point.cl,
        ld_range,
        Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0),
    );
    panel_title(&mut scene, &ax_eff, "Efficiency", pal);
    draw_legend(
        &mut scene,
        [ax_eff.left + 8.0, ax_eff.top + 8.0],
        &[(
            design_cl_legend(report),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0)),
        )],
        pal,
        8.0,
    );

    let ax_cm = Axes2D::new((500.0, 390.0, 340.0, 250.0), alpha_range, cm_range);
    ax_cm.draw_frame_with_labels(&mut scene, pal, "\u{03b1} [deg]", "Cm");
    let cm_pts: Vec<(f64, f64)> = alpha_axis
        .iter()
        .copied()
        .zip(p.cm.iter().copied())
        .collect();
    ax_cm.add_line_series(&mut scene, &cm_pts, blue);
    hline(
        &ax_cm,
        &mut scene,
        0.0,
        alpha_range,
        Stroke::dashed(Color::from_hex("#d62728"), 1.2, 4.0, 4.0),
    );
    panel_title(&mut scene, &ax_cm, "Longitudinal stability", pal);

    super::super::support::draw_alpha_axis_note(&mut scene, 450.0, 692.0, pal);
    scene
}

/// Legend text of the design-CL marker: it says so when the reported cruise
/// point is not the polar at the cruise lift coefficient.
fn design_cl_legend(report: &AnalysisReport) -> String {
    if alas_pipeline::cruise_point_off_polar_note(report).is_some() {
        "Design CL (outside the analysed polar)".to_owned()
    } else {
        "Design CL".to_owned()
    }
}

/// The polar sweep up to the last point whose CL does not exceed `cl_max`, and whether any
/// point was dropped. The vortex lattice is linear and has no stall, so a sweep run past the
/// wing's CL_max is not a prediction; `None` (or a non-positive/non-finite limit) keeps it whole.
pub(in crate::families::aerodynamics) fn polar_up_to_cl_max(
    polar: &alas_aero::analysis::PolarSweep,
    cl_max: Option<f64>,
) -> (alas_aero::analysis::PolarSweep, bool) {
    let total = polar.cl.len();
    let kept = cl_max
        .filter(|limit| limit.is_finite() && *limit > 0.0)
        .map_or(total, |limit| {
            polar.cl.iter().position(|&cl| cl > limit).unwrap_or(total)
        })
        .max(2)
        .min(total);
    (truncate_polar(polar, kept), kept < total)
}

/// The polar sweep restricted to its first `n` points.
fn truncate_polar(
    polar: &alas_aero::analysis::PolarSweep,
    n: usize,
) -> alas_aero::analysis::PolarSweep {
    let cut = |v: &Vec<f64>| v.iter().copied().take(n).collect::<Vec<_>>();
    alas_aero::analysis::PolarSweep {
        alpha_deg: cut(&polar.alpha_deg),
        geometric_alpha_deg: cut(&polar.geometric_alpha_deg),
        compressible_mach: polar.compressible_mach,
        cl: cut(&polar.cl),
        cd: cut(&polar.cd),
        cd_induced: cut(&polar.cd_induced),
        cd_wave: cut(&polar.cd_wave),
        cd_parasite: cut(&polar.cd_parasite),
        cm: cut(&polar.cm),
        l_over_d: cut(&polar.l_over_d),
    }
}

pub(super) fn panel_title(scene: &mut Scene, axes: &Axes2D, text: &str, pal: &Palette) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 8.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Compare baseline and optimized drag polar and efficiency traces.
pub fn figure_polar_comparison(
    baseline: &AnalysisReport,
    optimized: &AnalysisReport,
    labels: (&str, &str),
    ghost_reports: Option<&[&AnalysisReport]>,
    theme: Option<&str>,
) -> Scene {
    figure_polar_comparison_with_cl_max(baseline, optimized, labels, ghost_reports, None, theme)
}

/// [`figure_polar_comparison`] with the run's clean CL_max: every polar stops at the last
/// sweep point at or below it and the limit is marked, as in the aero panel.
pub fn figure_polar_comparison_with_cl_max(
    baseline: &AnalysisReport,
    optimized: &AnalysisReport,
    labels: (&str, &str),
    ghost_reports: Option<&[&AnalysisReport]>,
    cl_max: Option<f64>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag Polar Comparison".to_owned());
    draw_title(&mut scene, "Drag Polar Comparison", pal);
    scene.suppress_derived_title();

    let (b_cut, b_trunc) = polar_up_to_cl_max(&baseline.polar, cl_max);
    let (o_cut, o_trunc) = polar_up_to_cl_max(&optimized.polar, cl_max);
    let ghost_cuts: Vec<(alas_aero::analysis::PolarSweep, bool)> = ghost_reports
        .into_iter()
        .flatten()
        .map(|ghost| polar_up_to_cl_max(&ghost.polar, cl_max))
        .collect();
    let truncated = b_trunc || o_trunc || ghost_cuts.iter().any(|(_, cut)| *cut);
    let (b, o) = (&b_cut, &o_cut);
    let label_b = labels.0.to_owned();
    // Two overlapping curves read as one: say so, so a run without a design
    // change does not look like a plotting fault.
    let identical = b.cl == o.cl && b.cd == o.cd && b.l_over_d == o.l_over_d;
    let label_o = if identical {
        format!("{} (= {})", labels.1, labels.0)
    } else {
        labels.1.to_owned()
    };

    let cd_range = padded_range(
        b.cd.iter()
            .copied()
            .chain(o.cd.iter().copied())
            .chain(ghost_cuts.iter().flat_map(|(g, _)| g.cd.iter().copied())),
        0.08,
    );
    let cl_range = padded_range(
        b.cl.iter()
            .copied()
            .chain(o.cl.iter().copied())
            .chain(ghost_cuts.iter().flat_map(|(g, _)| g.cl.iter().copied()))
            .chain(cl_max.filter(|_| truncated)),
        0.08,
    );
    let ld_range = padded_range(
        b.l_over_d
            .iter()
            .copied()
            .chain(o.l_over_d.iter().copied())
            .chain(
                ghost_cuts
                    .iter()
                    .flat_map(|(g, _)| g.l_over_d.iter().copied()),
            ),
        0.08,
    );

    let ax_polar = Axes2D::new((70.0, 60.0, 360.0, 340.0), cd_range, cl_range);
    let ax_eff = Axes2D::new((470.0, 60.0, 360.0, 340.0), cl_range, ld_range);
    ax_polar.draw_frame_with_labels(&mut scene, pal, "CD", "CL");
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
    panel_title(&mut scene, &ax_polar, "Drag polar", pal);
    panel_title(&mut scene, &ax_eff, "Efficiency", pal);

    let mut legend_entries = Vec::new();
    if ghost_reports.is_some() {
        let ghost_stroke = Stroke::dashed(Color::from_hex(GHOST_COLOR), 1.2, 4.0, 3.0);
        for (i, (ghost, _)) in ghost_cuts.iter().enumerate() {
            let polar_pts: Vec<(f64, f64)> = ghost
                .cd
                .iter()
                .copied()
                .zip(ghost.cl.iter().copied())
                .collect();
            let eff_pts: Vec<(f64, f64)> = ghost
                .cl
                .iter()
                .copied()
                .zip(ghost.l_over_d.iter().copied())
                .collect();
            ax_polar.add_line_series(&mut scene, &polar_pts, ghost_stroke.clone());
            ax_eff.add_line_series(&mut scene, &eff_pts, ghost_stroke.clone());
            legend_entries.push((
                format!("Prior run {}", i + 1),
                LegendMarker::Line(ghost_stroke.clone()),
            ));
        }
    }

    let baseline_stroke = Stroke::dashed(Color::from_hex(BASELINE_COLOR), 1.8, 5.0, 3.0);
    let optimized_stroke = Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0);
    ax_polar.add_line_series(
        &mut scene,
        &b.cd
            .iter()
            .copied()
            .zip(b.cl.iter().copied())
            .collect::<Vec<_>>(),
        baseline_stroke.clone(),
    );
    add_markers(
        &ax_polar,
        &mut scene,
        &b.cd
            .iter()
            .copied()
            .zip(b.cl.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(BASELINE_COLOR),
    );
    ax_polar.add_line_series(
        &mut scene,
        &o.cd
            .iter()
            .copied()
            .zip(o.cl.iter().copied())
            .collect::<Vec<_>>(),
        optimized_stroke.clone(),
    );
    add_markers(
        &ax_polar,
        &mut scene,
        &o.cd
            .iter()
            .copied()
            .zip(o.cl.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(OPTIMIZED_COLOR),
    );
    ax_eff.add_line_series(
        &mut scene,
        &b.cl
            .iter()
            .copied()
            .zip(b.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        baseline_stroke.clone(),
    );
    ax_eff.add_line_series(
        &mut scene,
        &o.cl
            .iter()
            .copied()
            .zip(o.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        optimized_stroke.clone(),
    );
    add_markers(
        &ax_eff,
        &mut scene,
        &o.cl
            .iter()
            .copied()
            .zip(o.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(OPTIMIZED_COLOR),
    );
    legend_entries.push((label_b, LegendMarker::Line(baseline_stroke)));
    legend_entries.push((label_o, LegendMarker::Line(optimized_stroke)));

    draw_legend(
        &mut scene,
        [ax_polar.left + 8.0, ax_polar.top + 8.0],
        &legend_entries,
        pal,
        8.0,
    );
    draw_legend(
        &mut scene,
        [ax_eff.left + 8.0, ax_eff.top + 8.0],
        &legend_entries,
        pal,
        8.0,
    );

    if let Some(limit) = cl_max.filter(|_| truncated) {
        scene.height += 20.0;
        let red = Stroke::dashed(Color::from_hex("#d62728"), 1.0, 4.0, 3.0);
        hline(&ax_polar, &mut scene, limit, cd_range, red.clone());
        vline(&ax_eff, &mut scene, limit, ld_range, red);
        for (line, text) in [
            format!("Run CLmax {limit:.2}"),
            "Polars stop here: the lattice has no stall".to_owned(),
        ]
        .into_iter()
        .enumerate()
        {
            scene.add(SceneElement::Text {
                text,
                pos: [
                    ax_polar.left,
                    ax_polar.top + ax_polar.height + 44.0 + 14.0 * line as f64,
                ],
                font_size: 8.0,
                color: Color::from_hex("#d62728"),
                align: TextAlign::Left,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: false,
            });
        }
    }

    scene
}

pub(super) fn nearest_cl_index(cl: &[f64], target_cl: f64) -> Option<usize> {
    cl.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - target_cl).abs().total_cmp(&(*b - target_cl).abs()))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod cl_max_tests {
    use super::*;

    fn sweep(cl: &[f64]) -> alas_aero::analysis::PolarSweep {
        let column = |scale: f64| cl.iter().map(|c| c * scale).collect::<Vec<_>>();
        alas_aero::analysis::PolarSweep {
            alpha_deg: column(5.0),
            geometric_alpha_deg: column(5.0),
            compressible_mach: Some(0.0),
            cl: cl.to_vec(),
            cd: column(0.05),
            cd_induced: column(0.03),
            cd_wave: column(0.0),
            cd_parasite: column(0.02),
            cm: column(-0.1),
            l_over_d: column(10.0),
        }
    }

    #[test]
    fn the_sweep_stops_at_the_last_point_not_above_cl_max() {
        let polar = sweep(&[0.0, 0.5, 1.0, 1.4, 1.5, 1.8]);
        let (cut, truncated) = polar_up_to_cl_max(&polar, Some(1.45));
        assert!(truncated);
        assert_eq!(cut.cl, vec![0.0, 0.5, 1.0, 1.4]);
        assert_eq!(cut.alpha_deg.len(), 4);
    }

    #[test]
    fn no_limit_or_a_limit_above_the_sweep_keeps_every_point() {
        let polar = sweep(&[0.0, 0.5, 1.0]);
        for limit in [None, Some(2.0), Some(f64::NAN), Some(-1.0)] {
            let (cut, truncated) = polar_up_to_cl_max(&polar, limit);
            assert!(!truncated);
            assert_eq!(cut.cl.len(), 3);
        }
    }
}
