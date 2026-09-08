// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Draw a vertical colorbar: gradient strip, border, min/mid/max ticks, and
/// a horizontal label above it.
#[allow(clippy::too_many_arguments)]
pub fn draw_colorbar(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
    label: &str,
    pal: &Palette,
) {
    let (x, y, w, h) = rect;
    let steps = 64;
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let color = cmap.sample(t);
        let seg_h = h / steps as f64;
        // High values at the top: sample increases as i increases, drawn bottom-up.
        let seg_y = y + h - (i as f64 + 1.0) * seg_h;
        scene.add(SceneElement::Rect {
            x,
            y: seg_y,
            width: w,
            height: seg_h + 0.5,
            rx: 0.0,
            fill: Some(Fill::new(color)),
            stroke: None,
        });
    }
    scene.add(SceneElement::Rect {
        x,
        y,
        width: w,
        height: h,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
    });

    let tick_labels = [
        (0.0, format_colorbar_tick(vmin, vmin, vmax)),
        (0.5, format_colorbar_tick((vmin + vmax) * 0.5, vmin, vmax)),
        (1.0, format_colorbar_tick(vmax, vmin, vmax)),
    ];
    for (frac, text) in &tick_labels {
        let ty = y + h - frac * h;
        scene.add(SceneElement::Text {
            text: text.clone(),
            pos: [x + w + 4.0, ty],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    scene.add(SceneElement::Text {
        text: title_case_label(label),
        pos: [x + w * 0.5, y - 6.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
}

fn format_colorbar_tick(value: f64, vmin: f64, vmax: f64) -> String {
    let magnitude = vmin.abs().max(vmax.abs()).max(value.abs());
    let decimals = if magnitude >= 1_000.0 {
        0
    } else if magnitude >= 100.0 {
        1
    } else if magnitude >= 10.0 {
        2
    } else {
        3
    };
    format!("{value:.decimals$}")
}

fn title_case_label(label: &str) -> String {
    let mut changed = false;
    label
        .chars()
        .flat_map(|character| {
            if !changed && character.is_alphabetic() {
                changed = true;
                character.to_uppercase().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

/// Swatch style for one legend row.
#[derive(Debug, Clone)]
pub enum LegendMarker {
    /// A short line segment, styled with `stroke` (solid or dashed).
    Line(Stroke),
    /// A filled rectangular patch (for stacked-bar / filled-region legends).
    Patch(Color),
    /// A filled circular marker (for scatter series).
    Circle(Color),
}

/// Draw a top-left-anchored legend box: one row per `(label, marker)` entry.
pub fn draw_legend(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    let line_h = font_size + 6.0;
    for (i, (label, marker)) in entries.iter().enumerate() {
        let y = pos[1] + (i as f64) * line_h;
        let mid_y = y + font_size * 0.5;
        match marker {
            LegendMarker::Line(stroke) => {
                scene.add(SceneElement::Line {
                    p1: [pos[0], mid_y],
                    p2: [pos[0] + 18.0, mid_y],
                    stroke: stroke.clone(),
                });
            }
            LegendMarker::Patch(color) => {
                scene.add(SceneElement::Rect {
                    x: pos[0],
                    y,
                    width: 14.0,
                    height: font_size,
                    rx: 1.0,
                    fill: Some(Fill::new(*color)),
                    stroke: None,
                });
            }
            LegendMarker::Circle(color) => {
                scene.add(SceneElement::Circle {
                    center: [pos[0] + 7.0, mid_y],
                    radius: 5.0,
                    fill: Some(Fill::new(*color)),
                    stroke: None,
                });
            }
        }
        scene.add(SceneElement::Text {
            text: title_case_label(label),
            pos: [pos[0] + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

/// Draw a single horizontal legend row.
///
/// Figure families use this for legends below a plot. The width estimate is
/// deliberately conservative because the backend-neutral scene has no text
/// measurement primitive; leaving a little extra space keeps the final label
/// inside the canvas in both SVG and raster backends.
pub fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    let estimated_width = entries.iter().fold(0.0, |width, (label, _)| {
        width + 42.0 + label.chars().count() as f64 * font_size * 1.20
    });
    let available_width = (scene.width - pos[0] - 4.0).max(1.0);
    draw_horizontal_legend_impl(
        scene,
        pos,
        entries,
        pal,
        font_size,
        entries.len() > 2 || estimated_width > available_width,
    );
}

/// Draw a horizontal legend in fixed columns, keeping long localized labels
/// from colliding while retaining one centered row of entries.
pub fn draw_horizontal_legend_columns(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    draw_horizontal_legend_impl(scene, pos, entries, pal, font_size, true);
}

fn draw_horizontal_legend_impl(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
    use_columns: bool,
) {
    if entries.is_empty() {
        return;
    }
    let available_width = (scene.width - pos[0] - 4.0).max(1.0);
    let column_width = available_width / entries.len() as f64;
    let wrap_label = |label: &str, max_width: f64| {
        let max_chars = (max_width / (font_size * 0.70)).floor().max(1.0) as usize;
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in label.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if candidate.chars().count() <= max_chars || current.is_empty() {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_owned();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        lines.join("\n")
    };
    let mut x = pos[0];
    let mid_y = pos[1] + font_size * 0.5;
    for (index, (label, marker)) in entries.iter().enumerate() {
        if use_columns {
            x = pos[0] + index as f64 * column_width;
        }
        let label = title_case_label(label);
        let max_label_width = if use_columns {
            (column_width - 28.0).max(1.0)
        } else {
            (scene.width - x - 28.0).max(1.0)
        };
        let wrapped_label = wrap_label(&label, max_label_width);
        match marker {
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y: pos[1],
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
        }
        scene.add(SceneElement::Text {
            text: wrapped_label.clone(),
            pos: [x + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        if !use_columns {
            let longest_line = wrapped_label
                .lines()
                .map(str::len)
                .max()
                .unwrap_or_default();
            x += 42.0 + longest_line as f64 * font_size * 0.90;
        }
    }
}
