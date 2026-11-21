// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small painted icons for the Files explorer, one per kind of item. They are
//! drawn with the painter in unit coordinates, so they stay sharp at any size
//! and need no font or image file.

use egui::{pos2, Color32, Painter, Pos2, Rect, Rounding, Shape, Stroke};

use super::files_model::ItemKind;

const FOLDER: Color32 = Color32::from_rgb(232, 178, 62);
const PAPER: Color32 = Color32::from_rgb(236, 238, 242);

/// The accent colour of a kind of item.
fn accent(kind: ItemKind) -> Color32 {
    match kind {
        ItemKind::Folder => FOLDER,
        ItemKind::RunFolder => Color32::from_rgb(86, 168, 120),
        ItemKind::Aircraft | ItemKind::Config => Color32::from_rgb(70, 132, 235),
        ItemKind::Mission => Color32::from_rgb(52, 170, 150),
        ItemKind::Payload | ItemKind::Cabin => Color32::from_rgb(222, 140, 50),
        ItemKind::Results => Color32::from_rgb(150, 110, 220),
        ItemKind::Airport => Color32::from_rgb(110, 118, 135),
        ItemKind::Engine => Color32::from_rgb(200, 90, 90),
        ItemKind::Airfoil => Color32::from_rgb(90, 150, 215),
        ItemKind::Image => Color32::from_rgb(190, 100, 170),
        ItemKind::Csv => Color32::from_rgb(70, 160, 90),
        ItemKind::Log | ItemKind::Other => Color32::from_rgb(130, 136, 150),
    }
}

/// Maps unit coordinates onto the icon rectangle.
struct Frame {
    rect: Rect,
}

impl Frame {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.left() + x * self.rect.width(),
            self.rect.top() + y * self.rect.height(),
        )
    }

    fn area(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.at(x0, y0), self.at(x1, y1))
    }

    fn line(&self, painter: &Painter, points: &[(f32, f32)], stroke: Stroke) {
        let points = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        painter.add(Shape::line(points, stroke));
    }

    fn polygon(&self, painter: &Painter, points: &[(f32, f32)], fill: Color32) {
        let points = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        painter.add(Shape::convex_polygon(points, fill, Stroke::NONE));
    }
}

fn page(frame: &Frame, painter: &Painter) {
    let sheet = frame.area(0.2, 0.06, 0.8, 0.94);
    painter.rect_filled(sheet, Rounding::same(frame.rect.width() * 0.06), PAPER);
    painter.rect_stroke(
        sheet,
        Rounding::same(frame.rect.width() * 0.06),
        Stroke::new(1.0_f32, Color32::from_gray(150)),
    );
}

fn folder(frame: &Frame, painter: &Painter, color: Color32) {
    let round = Rounding::same(frame.rect.width() * 0.07);
    painter.rect_filled(
        frame.area(0.06, 0.2, 0.42, 0.42),
        round,
        color.gamma_multiply(0.8),
    );
    painter.rect_filled(frame.area(0.06, 0.3, 0.94, 0.84), round, color);
}

/// Paint the icon of `kind` filling `rect`.
pub(super) fn paint(painter: &Painter, rect: Rect, kind: ItemKind) {
    let frame = Frame { rect };
    let color = accent(kind);
    let ink = Stroke::new((rect.width() * 0.07).max(1.0), color);
    match kind {
        ItemKind::Folder => folder(&frame, painter, color),
        ItemKind::RunFolder => {
            folder(&frame, painter, color);
            frame.polygon(
                painter,
                &[(0.42, 0.44), (0.42, 0.74), (0.68, 0.59)],
                Color32::from_white_alpha(230),
            );
        }
        ItemKind::Image => {
            page(&frame, painter);
            painter.circle_filled(frame.at(0.62, 0.3), rect.width() * 0.06, color);
            frame.polygon(painter, &[(0.28, 0.82), (0.46, 0.46), (0.72, 0.82)], color);
        }
        ItemKind::Csv => {
            page(&frame, painter);
            for row in 0..3 {
                let y = 0.3 + row as f32 * 0.2;
                frame.line(painter, &[(0.28, y), (0.72, y)], ink);
            }
            frame.line(painter, &[(0.46, 0.22), (0.46, 0.8)], ink);
        }
        ItemKind::Log => {
            page(&frame, painter);
            for row in 0..4 {
                let y = 0.25 + row as f32 * 0.16;
                let end = if row == 3 { 0.55 } else { 0.72 };
                frame.line(painter, &[(0.28, y), (end, y)], ink);
            }
        }
        ItemKind::Aircraft | ItemKind::Config => {
            page(&frame, painter);
            // Convex parts: fuselage, two wings and the tail.
            frame.polygon(
                painter,
                &[(0.47, 0.2), (0.53, 0.2), (0.55, 0.8), (0.45, 0.8)],
                color,
            );
            frame.polygon(
                painter,
                &[(0.52, 0.42), (0.76, 0.62), (0.76, 0.68), (0.52, 0.6)],
                color,
            );
            frame.polygon(
                painter,
                &[(0.48, 0.42), (0.24, 0.62), (0.24, 0.68), (0.48, 0.6)],
                color,
            );
            frame.polygon(painter, &[(0.5, 0.7), (0.64, 0.84), (0.36, 0.84)], color);
        }
        ItemKind::Mission => {
            page(&frame, painter);
            frame.line(
                painter,
                &[(0.3, 0.76), (0.42, 0.5), (0.58, 0.58), (0.7, 0.28)],
                ink,
            );
            painter.circle_filled(frame.at(0.3, 0.76), rect.width() * 0.06, color);
            painter.circle_filled(frame.at(0.7, 0.28), rect.width() * 0.06, color);
        }
        ItemKind::Payload | ItemKind::Cabin => {
            page(&frame, painter);
            for row in 0..3 {
                for column in 0..2 {
                    let x = 0.3 + column as f32 * 0.24;
                    let y = 0.22 + row as f32 * 0.21;
                    painter.rect_filled(
                        frame.area(x, y, x + 0.16, y + 0.14),
                        Rounding::same(1.5),
                        color,
                    );
                }
            }
        }
        ItemKind::Results => {
            page(&frame, painter);
            for (index, height) in [0.22_f32, 0.38, 0.3].into_iter().enumerate() {
                let x = 0.3 + index as f32 * 0.15;
                painter.rect_filled(
                    frame.area(x, 0.8 - height, x + 0.1, 0.8),
                    Rounding::ZERO,
                    color,
                );
            }
        }
        ItemKind::Airport => {
            page(&frame, painter);
            frame.polygon(
                painter,
                &[(0.44, 0.18), (0.56, 0.18), (0.66, 0.84), (0.34, 0.84)],
                color,
            );
            for y in [0.3, 0.5, 0.7] {
                frame.line(
                    painter,
                    &[(0.5, y), (0.5, y + 0.07)],
                    Stroke::new(1.0_f32, PAPER),
                );
            }
        }
        ItemKind::Engine => {
            page(&frame, painter);
            let center = frame.at(0.5, 0.5);
            let radius = rect.width() * 0.2;
            painter.circle_stroke(center, radius, ink);
            frame.line(painter, &[(0.5, 0.3), (0.5, 0.7)], ink);
            frame.line(painter, &[(0.3, 0.5), (0.7, 0.5)], ink);
        }
        ItemKind::Airfoil => {
            page(&frame, painter);
            frame.polygon(
                painter,
                &[
                    (0.26, 0.52),
                    (0.36, 0.42),
                    (0.5, 0.39),
                    (0.7, 0.45),
                    (0.76, 0.52),
                    (0.7, 0.57),
                    (0.5, 0.6),
                    (0.36, 0.58),
                ],
                color,
            );
        }
        ItemKind::Other => {
            page(&frame, painter);
            frame.line(
                painter,
                &[(0.3, 0.3), (0.7, 0.3)],
                Stroke::new(1.0_f32, color),
            );
            frame.line(
                painter,
                &[(0.3, 0.46), (0.7, 0.46)],
                Stroke::new(1.0_f32, color),
            );
        }
    }
}

/// A small coloured pill with a number (the autosave badge).
pub(super) fn badge(painter: &Painter, right_center: Pos2, count: usize, font: egui::FontId) {
    let text = count.to_string();
    let galley = painter.layout_no_wrap(text, font, Color32::WHITE);
    let size = egui::vec2(
        galley.size().x.max(galley.size().y) + 8.0,
        galley.size().y + 2.0,
    );
    let rect = Rect::from_min_size(
        pos2(right_center.x - size.x, right_center.y - size.y * 0.5),
        size,
    );
    painter.rect_filled(
        rect,
        Rounding::same(size.y * 0.5),
        Color32::from_rgb(214, 96, 60),
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, Color32::WHITE);
}

/// Single-line text cut with an ellipsis at `max_width`; the flag says whether
/// it was cut.
pub(super) fn elided(
    painter: &Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) -> (std::sync::Arc<egui::Galley>, bool) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    let cut = galley.elided;
    (galley, cut)
}

/// The icon of a quick-access place: always the same folder, with a small
/// white mark that says what it holds (the sidebar of Windows Explorer does
/// the same with one folder style).
pub(super) fn paint_place(painter: &Painter, rect: Rect, kind: ItemKind) {
    let frame = Frame { rect };
    folder(&frame, painter, FOLDER);
    let mark = Color32::from_white_alpha(235);
    let ink = Stroke::new((rect.width() * 0.08).max(1.0), mark);
    match kind {
        ItemKind::RunFolder => {
            frame.polygon(painter, &[(0.42, 0.44), (0.42, 0.74), (0.68, 0.59)], mark)
        }
        ItemKind::Aircraft | ItemKind::Config => {
            frame.polygon(
                painter,
                &[(0.47, 0.4), (0.53, 0.4), (0.55, 0.8), (0.45, 0.8)],
                mark,
            );
            frame.polygon(painter, &[(0.52, 0.55), (0.74, 0.7), (0.52, 0.68)], mark);
            frame.polygon(painter, &[(0.48, 0.55), (0.26, 0.7), (0.48, 0.68)], mark);
        }
        ItemKind::Airport => {
            frame.polygon(
                painter,
                &[(0.45, 0.4), (0.55, 0.4), (0.62, 0.78), (0.38, 0.78)],
                mark,
            );
        }
        ItemKind::Engine => {
            painter.circle_stroke(frame.at(0.5, 0.58), rect.width() * 0.16, ink);
            frame.line(painter, &[(0.5, 0.46), (0.5, 0.7)], ink);
            frame.line(painter, &[(0.38, 0.58), (0.62, 0.58)], ink);
        }
        ItemKind::Cabin | ItemKind::Payload => {
            for row in 0..2 {
                for column in 0..2 {
                    let x = 0.3 + column as f32 * 0.22;
                    let y = 0.44 + row as f32 * 0.19;
                    painter.rect_filled(
                        frame.area(x, y, x + 0.16, y + 0.13),
                        Rounding::same(1.0),
                        mark,
                    );
                }
            }
        }
        ItemKind::Airfoil => frame.polygon(
            painter,
            &[
                (0.24, 0.6),
                (0.36, 0.5),
                (0.52, 0.47),
                (0.72, 0.54),
                (0.76, 0.6),
                (0.52, 0.68),
                (0.36, 0.66),
            ],
            mark,
        ),
        ItemKind::Mission => frame.line(
            painter,
            &[(0.28, 0.76), (0.42, 0.56), (0.58, 0.64), (0.72, 0.46)],
            ink,
        ),
        _ => {}
    }
}

/// The width `badge` needs for `count`.
pub(super) fn badge_width(painter: &Painter, count: usize, font: egui::FontId) -> f32 {
    let galley = painter.layout_no_wrap(count.to_string(), font, Color32::WHITE);
    galley.size().x.max(galley.size().y) + 8.0
}
