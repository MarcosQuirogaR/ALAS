// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plain-text engineering notation split into runs of normal, subscript and
//! superscript text, so every renderer (egui labels, SVG figures) can draw real
//! sub- and superscripts from the one place that decides what counts as such.
//!
//! Recognised, and nothing else:
//!
//! * `X_ref`, `L_fus`, `C_L`, `k_land`, `CLmax_TO`: a short symbol (one to three
//!   characters, or up to five when it contains a capital) followed by one
//!   underscore and a run of up to six letters or digits. Identifiers with
//!   several underscores (`break_chord_m`) and long words (`trim_alpha`) are
//!   left alone, so snake_case names are never mangled.
//! * `m^2`, `kg/m^3`, `slope^2`, `x^-1`, `y^1.5`, `x^{n+1}`: a caret after a
//!   non-space character followed by an optionally signed number or a braced
//!   group.
//! * `c/4` as a whole word becomes "¼-chord".

/// How one run of a notation string is placed relative to the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// On the baseline, full size.
    Normal,
    /// Lowered and reduced.
    Sub,
    /// Raised and reduced.
    Sup,
}

/// One run of text with a single placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The characters of the run, without the `_`, `^` or braces that marked it.
    pub text: String,
    /// Where the run sits.
    pub placement: Placement,
}

/// Line height, as a fraction of the surrounding font size, that egui
/// renderers give a subscript run. With `Align::BOTTOM` and the run's own
/// (smaller) font height, egui sits the subscript baseline slightly ABOVE the
/// text baseline, so it reads as a superscript; a shorter line box pushes the
/// run down below the baseline.
pub const SUB_LINE_HEIGHT_EM: f32 = 0.58;

/// Group the integer digits of a formatted number in threes with a space
/// (`30000` -> `30 000`, `-12345.5` -> `-12 345.5`) from 1000 up, so plot axis
/// ticks read the same everywhere. Text without a leading integer run of at
/// least four digits (exponent forms included) is returned unchanged.
#[must_use]
pub fn group_thousands(number: &str) -> String {
    let (sign, rest) = number
        .strip_prefix('-')
        .map_or(("", number), |tail| ("-", tail));
    let int_len = rest.chars().take_while(char::is_ascii_digit).count();
    let after = &rest[int_len..];
    if int_len < 4 || after.starts_with('e') || after.starts_with('E') {
        return number.to_owned();
    }
    let mut out = String::from(sign);
    for (i, c) in rest[..int_len].chars().enumerate() {
        if i > 0 && (int_len - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out.push_str(after);
    out
}

/// The longest subscript run converted.
const MAX_SUB_CHARS: usize = 6;

/// Whether `text` holds any notation worth rendering specially.
#[must_use]
pub fn has_notation(text: &str) -> bool {
    let spans = parse(text);
    !(spans.len() == 1 && spans[0].placement == Placement::Normal && spans[0].text == text)
        && !spans.is_empty()
}

/// Split `text` into runs. Concatenating the `text` of every run, with the
/// marker characters removed, gives the visible string.
#[must_use]
pub fn parse(text: &str) -> Vec<Span> {
    let text = replace_quarter_chord(text);
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut normal = String::new();
    let mut i = 0;
    let flush = |normal: &mut String, spans: &mut Vec<Span>| {
        if !normal.is_empty() {
            spans.push(Span {
                text: std::mem::take(normal),
                placement: Placement::Normal,
            });
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '_' {
            if let Some((sub, end)) = subscript_run(&chars, i) {
                // A Greek-name symbol ("eta" in eta_t) is drawn as the letter.
                let mut start = i;
                while start > 0 && chars[start - 1].is_ascii_alphanumeric() {
                    start -= 1;
                }
                let base: String = chars[start..i].iter().collect();
                if let Some(letter) = greek(&base) {
                    let keep = normal.chars().count().saturating_sub(base.chars().count());
                    normal = normal.chars().take(keep).collect();
                    normal.push(letter);
                }
                flush(&mut normal, &mut spans);
                spans.push(Span {
                    text: sub,
                    placement: Placement::Sub,
                });
                i = end;
                continue;
            }
        } else if c == '^' {
            if let Some((start, end, next)) = superscript_range(&chars, i) {
                flush(&mut normal, &mut spans);
                spans.push(Span {
                    text: chars[start..end].iter().collect(),
                    placement: Placement::Sup,
                });
                i = next;
                continue;
            }
        }
        normal.push(c);
        i += 1;
    }
    flush(&mut normal, &mut spans);
    spans
}

/// The visible text of `text` with sub- and superscripts flattened, for places
/// that cannot style runs (window titles, alt text, search).
#[must_use]
pub fn plain(text: &str) -> String {
    parse(text).into_iter().map(|span| span.text).collect()
}

/// `text` as SVG text content: runs with a subscript or superscript become
/// `<tspan font-size="70%" dy=...>` elements, everything else is passed
/// through `escape` as plain text. Text without notation comes back as
/// `escape(text)` alone, so a figure without notation is byte-identical.
///
/// The shift is written as relative `dy` offsets, not `baseline-shift`: usvg
/// (the rasteriser behind the desktop figure cards) draws `baseline-shift="sub"`
/// above the baseline, while `dy` means the same in every SVG renderer. A `dy`
/// carries on to the following characters, so the next run restores the
/// baseline. Offsets are in em of the surrounding text, positive down.
pub fn svg_runs(text: &str, escape: impl Fn(&str) -> String) -> String {
    /// Subscript drop and superscript rise, em of the surrounding text.
    const SUB_DROP_EM: f64 = 0.25;
    const SUP_RISE_EM: f64 = 0.40;
    /// `font-size` of a script run relative to the surrounding text.
    const SCRIPT: f64 = 0.7;
    if !has_notation(text) {
        return escape(text);
    }
    let mut out = String::new();
    let mut current = 0.0_f64;
    for span in parse(text) {
        let target = match span.placement {
            Placement::Normal => 0.0,
            Placement::Sub => SUB_DROP_EM,
            Placement::Sup => -SUP_RISE_EM,
        };
        let delta = target - current;
        current = target;
        let body = escape(&span.text);
        match span.placement {
            Placement::Normal if delta == 0.0 => out.push_str(&body),
            Placement::Normal => {
                out.push_str(&format!("<tspan dy=\"{delta:.3}em\">{body}</tspan>"))
            }
            _ => out.push_str(&format!(
                "<tspan font-size=\"70%\" dy=\"{:.3}em\">{body}</tspan>",
                delta / SCRIPT
            )),
        }
    }
    out
}

fn replace_quarter_chord(text: &str) -> String {
    let text = if text.contains("Delta") {
        text.replace("Delta ", "\u{394}")
            .replace("Delta", "\u{394}")
    } else {
        text.to_owned()
    };
    // "Cm,c/4": the quarter-chord moment coefficient, c/4 as a subscript.
    let text = text.replace(",c/4", "_{c/4}");
    if !text.contains("c/4") {
        return text;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let word_start = i == 0 || !chars[i - 1].is_alphanumeric();
        let matches = i + 3 <= chars.len() && chars[i..i + 3] == ['c', '/', '4'];
        let word_end = matches && (i + 3 == chars.len() || !chars[i + 3].is_alphanumeric());
        let inside_braces = i >= 1 && chars[i - 1] == '{';
        if word_start && matches && word_end && !inside_braces {
            out.push_str("\u{bc}-chord");
            i += 3;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Lower-case Greek names drawn as the letter when they are a symbol or its
/// subscript.
fn greek(name: &str) -> Option<char> {
    Some(match name {
        "alpha" => '\u{3b1}',
        "beta" => '\u{3b2}',
        "gamma" => '\u{3b3}',
        "delta" => '\u{3b4}',
        "epsilon" => '\u{3b5}',
        "eta" => '\u{3b7}',
        "theta" => '\u{3b8}',
        "kappa" => '\u{3ba}',
        "lambda" => '\u{3bb}',
        "mu" => '\u{3bc}',
        "nu" => '\u{3bd}',
        "rho" => '\u{3c1}',
        "sigma" => '\u{3c3}',
        "tau" => '\u{3c4}',
        "phi" => '\u{3c6}',
        "psi" => '\u{3c8}',
        "omega" => '\u{3c9}',
        _ => return None,
    })
}

/// The subscript text after the `_` at `at` and the index one past it, when
/// the `_` follows a short symbol and is not part of a long snake_case chain.
/// A Greek letter written as such, as in the symbol Λ_c4.
fn is_greek(c: char) -> bool {
    ('Α'..='ω').contains(&c)
}

/// `_{...}` takes any braced text; a chain `x_mlg_aft` (single-letter symbol
/// only) reads as the subscript "mlg,aft".
fn subscript_run(chars: &[char], at: usize) -> Option<(String, usize)> {
    let mut start = at;
    while start > 0 && (chars[start - 1].is_ascii_alphanumeric() || is_greek(chars[start - 1])) {
        start -= 1;
    }
    let base = &chars[start..at];
    if base.is_empty() || !(base[0].is_ascii_alphabetic() || is_greek(base[0])) {
        return None;
    }
    if start > 0 && chars[start - 1] == '_' {
        return None;
    }
    let has_capital = base.iter().any(char::is_ascii_uppercase);
    let symbol_ok = base.len() <= 3 || (base.len() <= 5 && has_capital);
    if !symbol_ok {
        return None;
    }
    if chars.get(at + 1) == Some(&'{') {
        let close = chars[at + 2..].iter().position(|&c| c == '}')? + at + 2;
        if close == at + 2 {
            return None;
        }
        return Some((chars[at + 2..close].iter().collect(), close + 1));
    }
    let mut parts: Vec<String> = Vec::new();
    let mut end = at;
    while end < chars.len() && chars[end] == '_' {
        let mut stop = end + 1;
        while stop < chars.len() && chars[stop].is_ascii_alphanumeric() {
            stop += 1;
        }
        let len = stop - (end + 1);
        if len == 0 || len > MAX_SUB_CHARS {
            return None;
        }
        parts.push(chars[end + 1..stop].iter().collect());
        end = stop;
        // Chains only hang off a one-letter symbol (`x_mlg_aft`); `vr_vmc_factor`
        // and the like are identifiers.
        if base.len() > 1 {
            break;
        }
        if parts.len() == 3 {
            break;
        }
    }
    if base.len() > 1 && end < chars.len() && chars[end] == '_' {
        return None;
    }
    if base.len() == 1 && end < chars.len() && chars[end] == '_' {
        return None;
    }
    let joined = parts
        .iter()
        .map(|part| greek(part).map_or_else(|| part.clone(), |letter| letter.to_string()))
        .collect::<Vec<_>>()
        .join(",");
    Some((joined, end))
}

/// `(start, end, next)` of the exponent after the `^` at `at`: the exponent
/// text is `chars[start..end]` and parsing resumes at `next`.
fn superscript_range(chars: &[char], at: usize) -> Option<(usize, usize, usize)> {
    if at == 0 || chars[at - 1].is_whitespace() {
        return None;
    }
    let mut i = at + 1;
    if chars.get(i) == Some(&'{') {
        let close = chars[i + 1..].iter().position(|&c| c == '}')? + i + 1;
        return (close > i + 1).then_some((i + 1, close, close + 1));
    }
    let start = i;
    if matches!(chars.get(i), Some('-' | '+' | '\u{2212}')) {
        i += 1;
    }
    let digits_start = i;
    while chars.get(i).is_some_and(char::is_ascii_digit) {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
        i += 1;
        while chars.get(i).is_some_and(char::is_ascii_digit) {
            i += 1;
        }
    }
    Some((start, i, i))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, Placement)> {
        parse(text)
            .into_iter()
            .map(|span| (span.text, span.placement))
            .collect()
    }

    #[test]
    fn subscripts_follow_short_symbols() {
        assert_eq!(
            kinds("S_ref 525.2 m^2"),
            vec![
                ("S".into(), Placement::Normal),
                ("ref".into(), Placement::Sub),
                (" 525.2 m".into(), Placement::Normal),
                ("2".into(), Placement::Sup),
            ]
        );
        assert_eq!(plain("L_fus and CLmax_TO"), "Lfus and CLmaxTO");
    }

    #[test]
    fn a_greek_letter_symbol_takes_a_subscript() {
        assert_eq!(
            kinds("Λ_c4 12.0"),
            vec![
                ("Λ".to_owned(), Placement::Normal),
                ("c4".to_owned(), Placement::Sub),
                (" 12.0".to_owned(), Placement::Normal),
            ]
        );
        assert_eq!(plain("Λ_LE 25"), "ΛLE 25");
    }

    #[test]
    fn snake_case_identifiers_are_left_alone() {
        for text in ["break_chord_m", "trim_alpha", "vr_vmc_factor", "step_N"] {
            assert!(!has_notation(text), "{text}");
            assert_eq!(plain(text), text);
        }
    }

    #[test]
    fn superscripts_take_signed_decimal_and_braced_exponents() {
        assert_eq!(plain("kg/m^3"), "kg/m3");
        assert_eq!(kinds("x^-1")[1], ("-1".into(), Placement::Sup));
        assert_eq!(kinds("(1 + s^2)^1.5")[3], ("1.5".into(), Placement::Sup));
        assert_eq!(kinds("y^{n+1}")[1], ("n+1".into(), Placement::Sup));
        assert!(!has_notation("a ^ b"));
        assert!(!has_notation("^2"));
    }

    #[test]
    fn svg_runs_wrap_scripts_in_dy_shifted_tspans() {
        let esc = |s: &str| s.replace('&', "&amp;");
        assert_eq!(svg_runs("a & b", esc), "a &amp; b");
        let svg = svg_runs("S_ref [m^2]", esc);
        assert_eq!(
            svg,
            "S<tspan font-size=\"70%\" dy=\"0.357em\">ref</tspan><tspan dy=\"-0.250em\"> [m</tspan><tspan font-size=\"70%\" dy=\"-0.571em\">2</tspan><tspan dy=\"0.400em\">]</tspan>"
        );
    }

    #[test]
    fn greek_names_chains_and_braces_read_as_symbols() {
        assert_eq!(plain("eta_t"), "\u{3b7}t");
        assert_eq!(
            kinds("CL_alpha (1/rad)")[1],
            ("\u{3b1}".into(), Placement::Sub)
        );
        assert_eq!(kinds("x_mlg_aft")[1], ("mlg,aft".into(), Placement::Sub));
        assert_eq!(kinds("Cm,c/4")[1], ("c/4".into(), Placement::Sub));
        let delta = kinds("(Delta x_cg / MAC)^2");
        assert_eq!(delta[0].0, "(\u{394}x");
        assert_eq!(delta.last().unwrap(), &("2".into(), Placement::Sup));
        assert_eq!(plain("M_{max,TO}"), "Mmax,TO");
    }

    #[test]
    fn quarter_chord_is_spelled_out() {
        assert_eq!(plain("c/4 sweep"), "\u{bc}-chord sweep");
        assert_eq!(plain("abc/4"), "abc/4");
    }
}
