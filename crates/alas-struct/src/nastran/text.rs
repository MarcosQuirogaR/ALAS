// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the str.splitlines()/str.strip() calls in
// alas/integration/nastran_runner.py (_tail and _run_nastran's .f06 scan).
// Reference: alas @ rust-port-baseline.

//! Python's line splitting and whitespace stripping, as the run report needs
//! them.
//!
//! This module exists because `str::lines` and `str::trim` are not what
//! upstream calls, and on the one input that matters the difference is
//! visible. A NASTRAN `.f06` is a paginated print file: pages are separated by
//! form feeds, and a `USER FATAL MESSAGE` frequently sits on the first line of
//! a page, joined to what precedes it by `\x0c` rather than by `\n`. Python's
//! `splitlines` treats a form feed as a line boundary and Rust's `lines` does
//! not, so scanning an `.f06` with `lines` finds the fatal message glued to the
//! tail of the previous page and reports a line the solver never printed.
//!
//! The two functions are therefore written to Python's rules rather than to
//! Rust's, and the parity fixture feeds them an `.f06` body containing form
//! feeds, carriage returns and `\r\n` for exactly that reason.

/// The characters Python's `str.splitlines` breaks a line at.
///
/// `\r\n` is handled as one boundary by [`splitlines`] rather than appearing
/// here. The list is Python's, including the three information separators and
/// the three Unicode line breaks, none of which Rust's `lines` recognizes.
const LINE_BOUNDARIES: [char; 9] = [
    '\n',       // line feed
    '\r',       // carriage return
    '\u{b}',    // line tabulation
    '\u{c}',    // form feed -- the one an .f06 is full of
    '\u{1c}',   // file separator
    '\u{1d}',   // group separator
    '\u{1e}',   // record separator
    '\u{2028}', // line separator
    '\u{2029}', // paragraph separator
];

/// Split `text` into lines the way Python's `str.splitlines` does.
///
/// Breaks at any of [`LINE_BOUNDARIES`] (and at `\u{85}`, the Unicode next-line
/// character), treats `\r\n` as a single boundary, and -- like Python, unlike a
/// naive split -- does not yield a trailing empty line for a text that ends in
/// a boundary. An empty input yields no lines at all.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        if !is_line_boundary(character) {
            continue;
        }
        lines.push(&text[start..index]);
        start = index + character.len_utf8();
        // A carriage return followed by a line feed is one boundary, not two.
        if character == '\r' && chars.peek().map(|&(_, next)| next) == Some('\n') {
            let (_, next) = chars.next().unwrap_or((start, '\n'));
            start += next.len_utf8();
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

fn is_line_boundary(character: char) -> bool {
    character == '\u{85}' || LINE_BOUNDARIES.contains(&character)
}

/// Trim `text` the way Python's `str.strip` with no argument does.
///
/// Python strips every character its `str.isspace` accepts, which is Rust's
/// `char::is_whitespace` set plus the four ASCII information separators
/// `\x1c`-`\x1f`. Those are not plausible in an `.f06`, but the whole reason
/// this module exists is that the plausible-looking assumption about Python's
/// text handling was wrong once already.
pub fn strip(text: &str) -> &str {
    text.trim_matches(is_python_space)
}

fn is_python_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_feed_is_a_line_boundary_and_a_rust_newline_is_not_enough() {
        let page = "end of page\u{c}   *** USER FATAL MESSAGE 9994 (IFP)";
        assert_eq!(
            splitlines(page),
            ["end of page", "   *** USER FATAL MESSAGE 9994 (IFP)"]
        );
        // The reason this module is not `str::lines`: that call sees one line.
        assert_eq!(page.lines().count(), 1);
    }

    #[test]
    fn a_carriage_return_line_feed_pair_is_one_boundary() {
        assert_eq!(splitlines("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
    }

    #[test]
    fn a_trailing_boundary_does_not_produce_an_empty_last_line() {
        assert_eq!(splitlines("a\nb\n"), ["a", "b"]);
        assert_eq!(splitlines("a\nb\n\n"), ["a", "b", ""]);
        assert!(splitlines("").is_empty());
    }

    #[test]
    fn stripping_covers_the_separators_rust_does_not_call_whitespace() {
        assert_eq!(strip("  padded \t\n"), "padded");
        assert_eq!(strip("\u{1c}\u{1f}kept\u{1e}"), "kept");
        assert_eq!(strip("nothing to do"), "nothing to do");
    }
}
