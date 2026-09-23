// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! PDF literal strings in the WinAnsiEncoding of the standard Helvetica font.

/// `text` as a PDF literal string `( ... )` of WinAnsi bytes, with the
/// delimiters and backslash escaped and every byte outside printable ASCII
/// written as an octal escape.
pub(super) fn pdf_literal(text: &str) -> String {
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

/// The WinAnsiEncoding (PDF 1.7, Annex D.2) byte for `character`, or `?`
/// when the standard Type 1 font cannot show it.
///
/// Latin-1 letters and symbols (U+00A0 to U+00FF) keep their code point as
/// the byte; the 0x80 to 0x9F row holds the Windows-1252 punctuation.
fn win_ansi_byte(character: char) -> u8 {
    match character {
        '\u{20ac}' => 0x80,
        '\u{201a}' => 0x82,
        '\u{0192}' => 0x83,
        '\u{201e}' => 0x84,
        '\u{2026}' => 0x85,
        '\u{2020}' => 0x86,
        '\u{2021}' => 0x87,
        '\u{02c6}' => 0x88,
        '\u{2030}' => 0x89,
        '\u{0160}' => 0x8a,
        '\u{2039}' => 0x8b,
        '\u{0152}' => 0x8c,
        '\u{017d}' => 0x8e,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{02dc}' => 0x98,
        '\u{2122}' => 0x99,
        '\u{0161}' => 0x9a,
        '\u{203a}' => 0x9b,
        '\u{0153}' => 0x9c,
        '\u{017e}' => 0x9e,
        '\u{0178}' => 0x9f,
        // The typographic minus renders as the ASCII hyphen-minus glyph.
        '\u{2212}' => b'-',
        character => match u8::try_from(u32::from(character)) {
            Ok(byte) if byte.is_ascii() || byte >= 0xa0 => byte,
            _ => b'?',
        },
    }
}

#[cfg(test)]
mod tests {
    use super::pdf_literal;

    #[test]
    fn latin1_and_windows_1252_text_keeps_its_letters() {
        assert_eq!(
            pdf_literal(
                "Envergadura \u{00f1}\u{00e1} \u{2018}x\u{2019} \u{201c}y\u{201d} 5\u{00b0}"
            ),
            "(Envergadura \\361\\341 \\221x\\222 \\223y\\224 5\\260)"
        );
        assert_eq!(
            pdf_literal("a(b)\\ \u{2212}1 \u{03b1}"),
            "(a\\(b\\)\\\\ -1 ?)"
        );
    }
}
