// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Simple syntax colouring for the read-only add-on code viewer: comments,
//! strings, numbers and keywords for Python, Rhai, TOML and JSON. It is a
//! line-based tokenizer for reading, not a parser.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

/// Languages the viewer colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    Python,
    Rhai,
    Toml,
    Json,
    Plain,
}

impl Language {
    pub(crate) fn from_path(path: &std::path::Path) -> Self {
        match path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .as_deref()
        {
            Some("py") => Self::Python,
            Some("rhai") => Self::Rhai,
            Some("toml") => Self::Toml,
            Some("json") => Self::Json,
            _ => Self::Plain,
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Python => &[
                "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", "else",
                "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
                "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "False", "try",
                "while", "with", "yield",
            ],
            Self::Rhai => &[
                "let", "const", "fn", "if", "else", "while", "loop", "for", "in", "return",
                "break", "continue", "true", "false", "switch", "throw", "try", "catch", "this",
            ],
            Self::Toml | Self::Json => &["true", "false", "null"],
            Self::Plain => &[],
        }
    }

    fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Python | Self::Toml => Some("#"),
            Self::Rhai => Some("//"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    Comment,
    Text,
    Number,
    Keyword,
    Heading,
}

fn color(kind: Kind, dark: bool) -> Color32 {
    match (kind, dark) {
        (Kind::Plain, true) => Color32::from_rgb(212, 212, 212),
        (Kind::Plain, false) => Color32::from_rgb(40, 40, 40),
        (Kind::Comment, true) => Color32::from_rgb(106, 153, 85),
        (Kind::Comment, false) => Color32::from_rgb(0, 128, 0),
        (Kind::Text, true) => Color32::from_rgb(206, 145, 120),
        (Kind::Text, false) => Color32::from_rgb(163, 21, 21),
        (Kind::Number, true) => Color32::from_rgb(181, 206, 168),
        (Kind::Number, false) => Color32::from_rgb(9, 134, 88),
        (Kind::Keyword, true) => Color32::from_rgb(86, 156, 214),
        (Kind::Keyword, false) => Color32::from_rgb(0, 0, 255),
        (Kind::Heading, true) => Color32::from_rgb(220, 220, 170),
        (Kind::Heading, false) => Color32::from_rgb(121, 94, 38),
    }
}

/// Split `text` into (kind, slice) runs.
fn tokens(text: &str, language: Language) -> Vec<(Kind, &str)> {
    let mut runs: Vec<(Kind, &str)> = Vec::new();
    let mut in_triple: Option<&str> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        if language == Language::Toml && body.trim_start().starts_with('[') && in_triple.is_none() {
            runs.push((Kind::Heading, &text[start..offset]));
            continue;
        }
        let bytes = body.as_bytes();
        let mut i = 0;
        let mut plain_from = 0;
        while i < bytes.len() {
            if let Some(quote) = in_triple {
                match body[i..].find(quote) {
                    Some(end) => {
                        push(&mut runs, text, start, Kind::Text, plain_from, i + end + 3);
                        i += end + 3;
                        plain_from = i;
                        in_triple = None;
                    }
                    None => {
                        push(&mut runs, text, start, Kind::Text, plain_from, bytes.len());
                        i = bytes.len();
                        plain_from = i;
                    }
                }
                continue;
            }
            let rest = &body[i..];
            if language.line_comment().is_some_and(|c| rest.starts_with(c)) {
                push(&mut runs, text, start, Kind::Plain, plain_from, i);
                push(&mut runs, text, start, Kind::Comment, i, bytes.len());
                i = bytes.len();
                plain_from = i;
            } else if language == Language::Python
                && (rest.starts_with("\"\"\"") || rest.starts_with("'''"))
            {
                push(&mut runs, text, start, Kind::Plain, plain_from, i);
                in_triple = Some(&rest[..3]);
                plain_from = i;
                i += 3;
                // The closing quotes may be later on this line.
                if let Some(end) = body[i..].find(&rest[..3]) {
                    push(&mut runs, text, start, Kind::Text, plain_from, i + end + 3);
                    i += end + 3;
                    plain_from = i;
                    in_triple = None;
                }
            } else if (bytes[i] == b'"' || (bytes[i] == b'\'' && language != Language::Json))
                && language != Language::Plain
            {
                push(&mut runs, text, start, Kind::Plain, plain_from, i);
                let quote = bytes[i];
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(bytes.len());
                push(&mut runs, text, start, Kind::Text, i, end);
                i = end;
                plain_from = i;
            } else if bytes[i].is_ascii_digit() && language != Language::Plain {
                push(&mut runs, text, start, Kind::Plain, plain_from, i);
                let mut j = i;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'_')
                {
                    j += 1;
                }
                push(&mut runs, text, start, Kind::Number, i, j);
                i = j;
                plain_from = i;
            } else if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                let mut j = i;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if language.keywords().contains(&&body[i..j]) {
                    push(&mut runs, text, start, Kind::Plain, plain_from, i);
                    push(&mut runs, text, start, Kind::Keyword, i, j);
                    plain_from = j;
                }
                i = j;
            } else {
                i += 1;
                while i < bytes.len() && !body.is_char_boundary(i) {
                    i += 1;
                }
            }
        }
        push(&mut runs, text, start, Kind::Plain, plain_from, bytes.len());
        // The line terminator.
        if body.len() < line.len() {
            runs.push((Kind::Plain, &text[start + body.len()..offset]));
        }
    }
    runs
}

/// Append `text[start + from..start + to]` as one run of `kind`.
fn push<'a>(
    runs: &mut Vec<(Kind, &'a str)>,
    text: &'a str,
    start: usize,
    kind: Kind,
    from: usize,
    to: usize,
) {
    if to > from {
        runs.push((kind, &text[start + from..start + to]));
    }
}

/// The coloured text as an egui layout job in the monospace font.
pub(crate) fn layout(text: &str, language: Language, font: FontId, dark: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (kind, slice) in tokens(text, language) {
        job.append(
            slice,
            0.0,
            TextFormat::simple(font.clone(), color(kind, dark)),
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, language: Language) -> Vec<(bool, String)> {
        tokens(text, language)
            .into_iter()
            .map(|(kind, slice)| (kind != Kind::Plain, slice.to_owned()))
            .collect()
    }

    #[test]
    fn the_runs_reassemble_the_original_text_exactly() {
        let source = "# note\nimport math\nx = 3.5  # tail\ns = \"a # b\"\n\"\"\"doc\nmore\"\"\"\ny = '\u{e9}'\n";
        for language in [
            Language::Python,
            Language::Rhai,
            Language::Toml,
            Language::Json,
            Language::Plain,
        ] {
            let joined: String = tokens(source, language).iter().map(|(_, s)| *s).collect();
            assert_eq!(joined, source, "{language:?}");
        }
    }

    #[test]
    fn python_comments_strings_numbers_and_keywords_are_coloured() {
        let runs = kinds("import x  # hi\ny = \"a#b\" + 2\n", Language::Python);
        let coloured: Vec<_> = runs
            .iter()
            .filter(|(c, _)| *c)
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(coloured, ["import", "# hi", "\"a#b\"", "2"]);
    }

    #[test]
    fn toml_section_headings_and_rhai_comments_are_coloured() {
        let toml = kinds("[[inputs]]\nname = \"x\"\n", Language::Toml);
        assert!(toml.iter().any(|(c, s)| *c && s.starts_with("[[inputs]]")));
        let rhai = kinds("let a = 1; // c\n", Language::Rhai);
        assert!(rhai.iter().any(|(c, s)| *c && s == "// c"));
    }

    #[test]
    fn a_multiline_python_docstring_is_one_string_colour() {
        let runs = kinds("\"\"\"a\nb\"\"\"\nx\n", Language::Python);
        let coloured: String = runs
            .iter()
            .filter(|(c, _)| *c)
            .map(|(_, s)| s.as_str())
            .collect();
        assert!(coloured.contains('a') && coloured.contains('b'));
    }
}
