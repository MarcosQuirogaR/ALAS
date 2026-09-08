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

fn pdf_literal(text: &str) -> String {
    let mut out = String::from("(");
    for character in text.chars() {
        let byte = win_ansi_byte(character);
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out.push(')');
    out
}

fn win_ansi_byte(character: char) -> u8 {
    match character {
        '\u{00b0}' => 0xb0,
        '\u{00b1}' => 0xb1,
        '\u{00b5}' => 0xb5,
        '\u{00b7}' => 0xb7,
        '\u{00d7}' => 0xd7,
        '\u{00f7}' => 0xf7,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{2018}' | '\u{2019}' => 0x92,
        '\u{201c}' | '\u{201d}' => 0x94,
        '\u{2026}' => 0x85,
        character if character.is_ascii() => character as u8,
        _ => b'?',
    }
}

