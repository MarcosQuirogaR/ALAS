// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! PDF object and content-stream serialization for report scenes.

use super::win_ansi::pdf_literal;
use super::{PdfFigure, PdfSection};
use crate::scene::{
    text_line_center_offsets, wrap_text_to_width, Color, Fill, Point2D, Scene, SceneElement,
    Stroke, TextAlign, TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};

mod pages;
pub(super) use pages::*;
mod assemble;
use assemble::*;
