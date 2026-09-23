// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn assemble_pdf(pages: &[String]) -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    let page_ids = (0..pages.len())
        .map(|index| 4 + index * 2)
        .collect::<Vec<_>>();
    let kids = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes();

    for (index, page) in pages.iter().enumerate() {
        let page_id = page_ids[index];
        let content_id = page_id + 1;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .into_bytes(),
        );
        let mut content = format!("<< /Length {} >>\nstream\n", page.len()).into_bytes();
        content.extend_from_slice(page.as_bytes());
        content.extend_from_slice(b"endstream");
        objects.push(content);
    }

    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = output.len();
    output.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    output
}

fn color_component(value: u8) -> String {
    format_number(value as f64 / 255.0)
}

fn format_number(value: f64) -> String {
    format!("{value:.3}")
}


#[cfg(test)]
mod tests {
    use super::{append_scene_text, SceneTextStyle};
    use crate::scene::{Color, TextAlign, TextBaseline};

    fn style(baseline: TextBaseline, angle_deg: f64) -> SceneTextStyle {
        SceneTextStyle {
            font_size_pt: 12.0,
            color: Color::rgb(0, 0, 0),
            align: TextAlign::Left,
            baseline,
            angle_deg,
        }
    }

    #[test]
    fn scene_text_uses_points_one_row_per_line_and_the_svg_rotation() {
        let mut out = String::new();
        append_scene_text(&mut out, &["A", "B"], [100.0, 50.0], &style(TextBaseline::Middle, 0.0));
        // 12 pt is 16 CSS px; SVG centers the two rows at y = 40.4 and 59.6.
        assert_eq!(out.matches("/F1 16.000 Tf").count(), 2);
        assert!(out.contains("1.000 0.000 0.000 -1.000 100.000 45.200 Tm\n(A) Tj"));
        assert!(out.contains("1.000 0.000 0.000 -1.000 100.000 64.400 Tm\n(B) Tj"));

        let mut rotated = String::new();
        append_scene_text(&mut rotated, &["C"], [100.0, 50.0], &style(TextBaseline::Middle, -90.0));
        // Reading upward: glyph tops face -x, so the baseline sits 4.8 px toward +x.
        assert!(rotated.contains("0.000 -1.000 -1.000 -0.000 104.800 50.000 Tm"));
    }
}
