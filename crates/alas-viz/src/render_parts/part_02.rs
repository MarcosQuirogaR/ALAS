// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl DrawingBackend for EguiBackend<'_> {
    type ErrorType = std::io::Error;

    fn get_size(&self) -> (u32, u32) {
        (
            self.scene.width.max(1.0) as u32,
            self.scene.height.max(1.0) as u32,
        )
    }
    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }
    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }
    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let p = pos2(point.0 as f32, point.1 as f32);
        self.shapes
            .push(Shape::circle_filled(p, 0.5, Self::color(color)));
        Ok(())
    }
    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.shapes.push(Shape::line_segment(
            [
                pos2(from.0 as f32, from.1 as f32),
                pos2(to.0 as f32, to.1 as f32),
            ],
            EguiStroke::new(style.stroke_width() as f32, Self::color(style.color())),
        ));
        Ok(())
    }
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let points = path
            .into_iter()
            .map(|(x, y)| pos2(x as f32, y as f32))
            .collect::<Vec<_>>();
        if points.len() >= 2 {
            self.shapes.push(Shape::Path(PathShape::line(
                points,
                EguiStroke::new(
                    style.stroke_width().max(1) as f32,
                    Self::color(style.color()),
                ),
            )));
        }
        Ok(())
    }
    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let rect = Rect::from_two_pos(
            pos2(upper_left.0 as f32, upper_left.1 as f32),
            pos2(bottom_right.0 as f32, bottom_right.1 as f32),
        );
        let color = Self::color(style.color());
        let stroke = if fill {
            EguiStroke::NONE
        } else {
            EguiStroke::new(style.stroke_width() as f32, color)
        };
        self.shapes.push(Shape::Rect(RectShape::new(
            rect,
            Rounding::ZERO,
            if fill { color } else { Color32::TRANSPARENT },
            stroke,
        )));
        Ok(())
    }
    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let color = Self::color(style.color());
        self.shapes.push(Shape::Circle(CircleShape {
            center: pos2(center.0 as f32, center.1 as f32),
            radius: radius as f32,
            fill: if fill { color } else { Color32::TRANSPARENT },
            stroke: if fill {
                EguiStroke::NONE
            } else {
                EguiStroke::new(style.stroke_width() as f32, color)
            },
        }));
        Ok(())
    }
    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vert: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let points = vert
            .into_iter()
            .map(|(x, y)| pos2(x as f32, y as f32))
            .collect::<Vec<_>>();
        self.shapes.push(Shape::Path(PathShape::convex_polygon(
            points,
            Self::color(style.color()),
            EguiStroke::NONE,
        )));
        Ok(())
    }
    fn draw_text<TStyle: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &TStyle,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        self.draw_text_with_options(text, style, pos, 0.0, false)
    }
}

fn to_hpos(align: TextAlign) -> HPos {
    match align {
        TextAlign::Left => HPos::Left,
        TextAlign::Center => HPos::Center,
        TextAlign::Right => HPos::Right,
    }
}
fn to_vpos(baseline: TextBaseline) -> VPos {
    match baseline {
        TextBaseline::Top => VPos::Top,
        TextBaseline::Middle => VPos::Center,
        TextBaseline::Bottom => VPos::Bottom,
    }
}

