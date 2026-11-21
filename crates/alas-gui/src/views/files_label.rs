// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a file name is set under its large icon: at most two lines, broken
//! only after a separator (space, `-`, `_`, `.`), never inside a word. A word
//! wider than the cell is cut with an ellipsis instead of being split, and a
//! name that needs more than two lines ends in one.

/// The most lines a name takes.
const MAX_LINES: usize = 2;
const ELLIPSIS: &str = "\u{2026}";

/// Split `name` into pieces that end after a separator.
fn pieces(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch == ' ' {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
        if matches!(ch, '-' | '_' | '.') {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `text` shortened in the middle (with an ellipsis) until `measure` says it
/// fits `width`: the start and the end are kept, because the end of a name
/// (a version, a date, the extension) is what tells similar names apart.
fn elide(measure: &dyn Fn(&str) -> f32, text: &str, width: f32) -> String {
    if measure(text) <= width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    (1..chars.len())
        .rev()
        .map(|keep| {
            let tail = keep / 2;
            let head = keep - tail;
            let start: String = chars[..head].iter().collect();
            let end: String = chars[chars.len() - tail..].iter().collect();
            format!("{start}{ELLIPSIS}{end}")
        })
        .find(|candidate| measure(candidate) <= width)
        .unwrap_or_else(|| ELLIPSIS.to_owned())
}

/// `line` followed by the pieces still to place, as one text.
fn with_rest(line: &str, rest: impl Iterator<Item = String>) -> String {
    let mut text = line.trim_end().to_owned();
    for piece in rest {
        if !text.is_empty() && !text.ends_with(['-', '_', '.']) {
            text.push(' ');
        }
        text.push_str(&piece);
    }
    text
}

/// The lines of `name` for a cell `width` points wide, and whether anything
/// was cut (the caller then shows the full name as a tooltip).
pub(super) fn lines(measure: &dyn Fn(&str) -> f32, name: &str, width: f32) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut cut = false;
    let mut queue = pieces(name).into_iter().peekable();
    while let Some(piece) = queue.next() {
        let joined = format!("{current}{piece}");
        if measure(joined.trim_end()) <= width {
            current = if piece.ends_with(['-', '_', '.']) {
                joined
            } else {
                format!("{joined} ")
            };
            continue;
        }
        if current.trim().is_empty() {
            // A single word wider than the cell: cut it, never split it.
            let last_line = lines.len() + 1 >= MAX_LINES;
            // On the last line the whole rest of the name is elided, so its
            // end stays visible.
            let text = if last_line {
                with_rest(&piece, queue.by_ref())
            } else {
                piece.clone()
            };
            lines.push(elide(measure, &text, width));
            cut = true;
            current = String::new();
        } else {
            lines.push(current.trim_end().to_owned());
            current = String::new();
            // Retry the piece on the new line.
            let mut rest: Vec<String> = vec![piece];
            rest.extend(queue.by_ref());
            queue = rest.into_iter().peekable();
        }
        if lines.len() >= MAX_LINES {
            // Anything left means the name does not fit in two lines.
            if queue.peek().is_some() || !current.is_empty() {
                cut = true;
                if let Some(last) = lines.last_mut() {
                    let tail = with_rest(last, queue.by_ref());
                    *last = elide(measure, &tail, width);
                }
            }
            return (lines, cut);
        }
    }
    if !current.trim().is_empty() {
        lines.push(current.trim_end().to_owned());
    }
    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        cut = true;
    }
    (lines, cut)
}

#[cfg(test)]
mod tests {
    use super::lines;

    /// Eight points a character.
    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 8.0
    }

    #[test]
    fn a_long_name_breaks_after_a_separator_never_inside_a_word() {
        let (out, cut) = lines(&measure, "airfoil-cfd-environment.json", 14.0 * 8.0);
        assert!(!cut || out.len() == 2, "{out:?}");
        for line in &out {
            // An elided line keeps the start and the end of its run.
            for part in line.split('\u{2026}') {
                assert!(
                    "airfoil-cfd-environment.json".contains(part),
                    "{line} is a run of the name"
                );
            }
        }
        assert_eq!(out[0], "airfoil-cfd-");
    }

    #[test]
    fn a_word_wider_than_the_cell_is_cut_with_an_ellipsis_not_split() {
        let (out, cut) = lines(&measure, "installedtoolsconfiguration", 10.0 * 8.0);
        assert!(cut);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].contains('\u{2026}') && measure(&out[0]) <= 80.0,
            "{out:?}"
        );
        // The end of the word survives, so similar names stay distinguishable.
        assert!(out[0].ends_with("ion"), "{out:?}");
    }

    #[test]
    fn two_names_that_differ_only_at_the_end_stay_different() {
        let a = lines(&measure, "tool-preferences.json.bak-2026", 14.0 * 8.0).0;
        let b = lines(&measure, "tool-preferences.json.bak-2027", 14.0 * 8.0).0;
        assert_ne!(a, b);
        assert!(a.last().is_some_and(|l| l.ends_with("2026")), "{a:?}");
    }

    #[test]
    fn a_short_name_is_one_line_and_a_very_long_one_ends_in_an_ellipsis() {
        let (out, cut) = lines(&measure, "notes.txt", 100.0);
        assert_eq!((out, cut), (vec!["notes.txt".to_owned()], false));
        let (out, cut) = lines(
            &measure,
            "alpha-beta-gamma-delta-epsilon-zeta-eta.json",
            8.0 * 8.0,
        );
        assert!(cut);
        assert_eq!(out.len(), 2);
        assert!(
            out[1].contains('\u{2026}') && out[1].ends_with("son"),
            "{out:?}"
        );
    }
}
