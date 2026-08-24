// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rendering one bulk-data card in the NASTRAN-95 fixed-field dialect.
//!
//! This is where the row's five dialect rules live, each found by a run that
//! failed silently until it was obeyed (see the module doc). The two that shape
//! this code:
//!
//! * **Every field is at most eight characters.** [`super::super::nastran`]'s
//!   large-field writer carries sixteen, and its `%.8g` free-field renderer
//!   overflows eight outright (`20.00000000` was rejected). So the whole deck is
//!   fixed field: an eight-column card name, then fields of exactly eight
//!   columns each, and [`real`] is a formatter that never spends a ninth.
//! * **A card holds at most eight data fields before it must continue.** In
//!   fixed field a physical line is the card name (field 1) plus fields 2-9,
//!   with field 10 -- columns 73-80 -- reserved for a continuation tag that the
//!   next line repeats in its own field 1. [`Card::render`] chunks a field list
//!   across as many lines as it needs, minting a unique tag for each break, so
//!   the mesh's root-rib `SPC1` and every `CRBE3` continue rather than being
//!   quoted back and silently dropped.
//!
//! The precision cost of rule one is real and is why fixed field was chosen over
//! free: fixed field accepts the classic `2.2467-6` exponent shorthand, which
//! carries about five significant digits in eight columns, where free field
//! would demand `2.2467E-6` and leave about three. A spar cap's second moment is
//! `1e-6`-small, so those two digits matter to the cross-solver agreement this
//! row is judged by.

/// Columns in one fixed field, card name included.
const FIELD_WIDTH: usize = 8;

/// Data fields on one physical line: fields 2 through 9, with field 1 the card
/// name or a continuation tag and field 10 the tag pointing at the next line.
const FIELDS_PER_LINE: usize = 8;

/// One bulk-data field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    /// An omitted field, which NASTRAN reads as the card's default.
    Blank,
    /// An integer identifier or component list.
    Int(i64),
    /// A character field: a section name, a component string, a flag.
    Text(&'static str),
    /// A real, rendered by [`real`].
    Real(f64),
}

/// Mints the unique continuation tags a deck's long cards need.
///
/// One counter for the whole deck, because NASTRAN pairs a parent line's field
/// 10 with the child line's field 1 by exact string match: two cards that both
/// minted `+1` would splice into each other. The tag is `+` and a decimal
/// counter, which stays within eight columns for any deck this program builds.
#[derive(Debug, Default)]
pub struct ContinuationTags {
    next: u64,
}

impl ContinuationTags {
    /// A fresh counter.
    pub fn new() -> Self {
        Self::default()
    }

    fn mint(&mut self) -> String {
        self.next += 1;
        format!("+{}", self.next)
    }
}

/// A card as a name and its data fields, ready to be rendered across as many
/// continuation lines as the fields need.
pub struct Card<'a> {
    name: &'a str,
    fields: Vec<Field>,
}

impl<'a> Card<'a> {
    /// A card named `name` carrying `fields` (field 2 onward -- the name is
    /// field 1).
    pub fn new(name: &'a str, fields: Vec<Field>) -> Self {
        Self { name, fields }
    }

    /// Append this card to `out`, continuing onto tagged lines when it carries
    /// more than eight fields.
    pub fn render(&self, out: &mut String, tags: &mut ContinuationTags) {
        let chunks: Vec<&[Field]> = if self.fields.is_empty() {
            vec![&[]]
        } else {
            self.fields.chunks(FIELDS_PER_LINE).collect()
        };
        let mut opener = self.name.to_string();
        for (index, chunk) in chunks.iter().enumerate() {
            let mut line = format!("{opener:<FIELD_WIDTH$}");
            for field in *chunk {
                line.push_str(&format!("{:<FIELD_WIDTH$}", render_field(*field)));
            }
            let last = index + 1 == chunks.len();
            if last {
                out.push_str(line.trim_end());
                out.push('\n');
            } else {
                // Field 10, columns 73-80, names the line that continues this
                // one; the next iteration opens with the same tag.
                let tag = tags.mint();
                // Pad the data region to column 72 so the tag lands in field 10.
                let padded = format!(
                    "{line:<width$}",
                    width = FIELD_WIDTH * (FIELDS_PER_LINE + 1)
                );
                out.push_str(format!("{padded}{tag:<FIELD_WIDTH$}").trim_end());
                out.push('\n');
                opener = tag;
            }
        }
    }
}

fn render_field(field: Field) -> String {
    match field {
        Field::Blank => String::new(),
        Field::Int(number) => number.to_string(),
        Field::Text(text) => text.to_string(),
        Field::Real(value) => real(value),
    }
}

/// One real in at most eight columns, always carrying a decimal point.
///
/// NASTRAN reads a bare `500` as an integer and fails the card, so a real must
/// show its point. The formatter tries decreasing significant precision until
/// the rendering fits eight columns: an ordinary magnitude comes out in fixed
/// notation (`0.015564`), and one that needs an exponent uses the fixed-field
/// shorthand that drops the `E` (`2.2467-6`, `7.1+10`), which is legal here and
/// buys back the two columns an explicit `E` would cost.
pub fn real(value: f64) -> String {
    if value == 0.0 {
        return "0.".to_string();
    }
    for precision in (1..=FIELD_WIDTH).rev() {
        if let Some(text) = render_at(value, precision) {
            if text.len() <= FIELD_WIDTH {
                return text;
            }
        }
    }
    // Every finite double fits at one significant digit: a mantissa `d.` and a
    // signed exponent of at most three digits is six columns. A non-finite value
    // cannot reach here -- the deck's arithmetic is over measured geometry.
    render_at(value, 1).unwrap_or_else(|| "0.".to_string())
}

/// `value` at `precision` significant digits, in the fixed-field spelling: a
/// decimal point always present, and an exponent written as the classic
/// `mantissa` + signed digits with no `E`.
fn render_at(value: f64, precision: usize) -> Option<String> {
    let significant = precision.max(1);
    let exponent = value.abs().log10().floor() as i32;
    // The `%g` rule: an exponent inside `[-4, precision)` prints in fixed
    // notation, otherwise in scientific. Matching it keeps ordinary numbers
    // free of an exponent they do not need.
    if exponent < -4 || exponent >= significant as i32 {
        let mantissa_text = format!("{value:.*e}", significant - 1);
        let (mantissa, exp) = mantissa_text.split_once('e')?;
        let mantissa = trim_zeros(mantissa);
        let mantissa = if mantissa.contains('.') {
            mantissa
        } else {
            format!("{mantissa}.")
        };
        let exp: i32 = exp.parse().ok()?;
        Some(format!("{mantissa}{exp:+}"))
    } else {
        let decimals = (significant as i32 - 1 - exponent).max(0) as usize;
        let text = trim_zeros(&format!("{value:.decimals$}"));
        Some(if text.contains('.') {
            text
        } else {
            format!("{text}.")
        })
    }
}

/// Drop a fixed-notation number's trailing zeros, and the point they leave bare.
fn trim_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    let trimmed = text.trim_end_matches('0');
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_carries_the_point_that_stops_it_reading_as_an_integer() {
        assert_eq!(real(500.0), "500.");
        assert_eq!(real(1.0), "1.");
        assert_eq!(real(-1.0), "-1.");
        assert_eq!(real(2700.0), "2700.");
    }

    #[test]
    fn zero_is_a_bare_point() {
        assert_eq!(real(0.0), "0.");
        assert_eq!(real(-0.0), "0.");
    }

    #[test]
    fn an_ordinary_magnitude_is_fixed_notation_trimmed_to_eight_columns() {
        assert_eq!(real(0.33), "0.33");
        assert_eq!(real(0.01), "0.01");
        // Eleven significant columns do not fit; five do.
        assert!(real(0.015_563_681_910_996).len() <= 8);
        assert!(real(0.015_563_681_910_996).starts_with("0.0155"));
    }

    #[test]
    fn a_small_magnitude_uses_the_no_e_exponent_shorthand() {
        // The spar-cap inertias the cross-solver check leans on.
        assert_eq!(real(2.246_666_7e-6), "2.2467-6");
        assert_eq!(real(2.091_666_7e-7), "2.0917-7");
        assert_eq!(real(3.666_666_7e-8), "3.6667-8");
    }

    #[test]
    fn a_large_magnitude_also_drops_the_e() {
        assert_eq!(real(7.1e10), "7.1+10");
        assert_eq!(real(7.0e10), "7.+10");
    }

    #[test]
    fn every_real_fits_eight_columns_and_shows_a_point() {
        let samples = [
            -2.1,
            0.006,
            71_000_000_000.0,
            26_691_729_323.308_27,
            1e-300,
            1e300,
            123_456.789_012_345,
            9.81,
        ];
        for value in samples {
            let text = real(value);
            assert!(
                text.len() <= 8,
                "{value:e} -> {text:?} ({} cols)",
                text.len()
            );
            assert!(text.contains('.'), "{value:e} -> {text:?}");
        }
    }

    #[test]
    fn a_real_reads_back_within_the_precision_eight_columns_can_hold() {
        // Fixed field carries about five significant digits; a wingbox stiffness
        // agreeing to that is far inside the cross-solver convergence tier.
        for value in [0.015_563_681, 2.246_666_7e-6, 71_000_000_000.0, -2.1] {
            let parsed: f64 = shorthand_to_f64(&real(value));
            let relative = (parsed - value).abs() / value.abs();
            assert!(
                relative < 1e-4,
                "{value:e} -> {} ({relative:e})",
                real(value)
            );
        }
    }

    #[test]
    fn a_card_within_eight_fields_stays_on_one_line() {
        let mut tags = ContinuationTags::new();
        let mut out = String::new();
        Card::new(
            "CQUAD4",
            vec![
                Field::Int(1),
                Field::Int(3),
                Field::Int(1),
                Field::Int(2),
                Field::Int(5),
                Field::Int(4),
            ],
        )
        .render(&mut out, &mut tags);
        assert_eq!(out, "CQUAD4  1       3       1       2       5       4\n");
    }

    #[test]
    fn a_long_card_continues_onto_a_tagged_line() {
        let mut tags = ContinuationTags::new();
        let mut out = String::new();
        // CRBE3 with two grids past the eight-field line.
        Card::new(
            "CRBE3",
            vec![
                Field::Int(50),
                Field::Blank,
                Field::Int(10),
                Field::Int(123),
                Field::Real(1.0),
                Field::Int(123),
                Field::Int(1),
                Field::Int(3),
                Field::Int(7),
                Field::Int(9),
            ],
        )
        .render(&mut out, &mut tags);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        // The parent's field 10 (columns 73-80) names the continuation.
        assert!(lines[0].ends_with("+1"), "{:?}", lines[0]);
        assert_eq!(&lines[0][72..], "+1");
        // The child opens with the same tag and carries the overflow grids.
        assert!(lines[1].starts_with("+1"), "{:?}", lines[1]);
        assert!(lines[1].contains('7') && lines[1].contains('9'));
    }

    #[test]
    fn each_continuation_tag_is_unique_across_the_deck() {
        let mut tags = ContinuationTags::new();
        let mut out = String::new();
        let long = Card::new("SPC1", (0..20).map(Field::Int).collect());
        long.render(&mut out, &mut tags);
        long.render(&mut out, &mut tags);
        // Four continuation lines across two cards, no tag repeated.
        let openers: Vec<&str> = out
            .lines()
            .filter(|line| line.starts_with('+'))
            .map(|line| line.split_whitespace().next().unwrap_or(""))
            .collect();
        let mut sorted = openers.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(openers.len(), sorted.len(), "{openers:?}");
    }

    /// Read a fixed-field real, expanding the no-`E` exponent shorthand, the way
    /// NASTRAN's own scanner does -- used only to check [`real`] round-trips.
    fn shorthand_to_f64(text: &str) -> f64 {
        if let Some(position) = text[1..]
            .find(['+', '-'])
            .map(|p| p + 1)
            .filter(|&p| !matches!(text.as_bytes()[p - 1], b'e' | b'E'))
        {
            let (mantissa, exponent) = text.split_at(position);
            return format!("{mantissa}e{exponent}").parse().unwrap_or(f64::NAN);
        }
        text.parse().unwrap_or(f64::NAN)
    }
}
