// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

    let (cd_p, cd_i, cd_w) = match nearest_cl_index(&p.cl, dp.cl) {
        Some(idx) => (p.cd_parasite[idx], p.cd_induced[idx], p.cd_wave[idx]),
        None => (fit.cd0, fit.k * dp.cl * dp.cl, 0.0),
    };
    let cd_total = (cd_p + cd_i + cd_w).max(1e-9);

    let ax_bar = Axes2D::new(
        (80.0, 60.0, 300.0, 340.0),
        (0.0, 1.0),
        (0.0, cd_total * 1.3),
    );
    ax_bar.draw_frame_with_labels(&mut scene, pal, "Design point", "CD");
    panel_title(&mut scene, &ax_bar, "Drag breakdown at design CL", pal);

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

