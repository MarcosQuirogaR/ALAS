// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading a NASTRAN bulk-data file back into cards.
//!
//! The parity test compares *decks*, not the objects that produced them, for
//! the reason the generator records a deck: what a solve consumes is the file.
//! So the port's own output is written out and read back here, by a reader that
//! shares no code with the writer, and the cards that come out are what get
//! compared against the ones `pyNastran` read out of the reference's file.
//!
//! All three field layouts the format defines are handled -- eight-column
//! small field, sixteen-column large field marked by `*`, and comma-separated
//! free field -- because a later row writes its case-control decks free-field
//! while this one writes the mesh large-field, and a reader that only
//! understood one of them would be checking the writer against itself.

#![allow(clippy::unwrap_used, clippy::expect_used)]

/// One card: its name, and its fields with trailing blanks dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    /// The card name, without the large-field `*`.
    pub name: String,
    /// Its fields, blanks included where they sit between two written ones.
    pub fields: Vec<String>,
}

impl Card {
    /// Field `index` as an integer, or zero where the card left it blank.
    pub fn integer(&self, index: usize) -> i64 {
        self.fields
            .get(index)
            .map_or(0, |field| field.trim().parse().unwrap_or(0))
    }

    /// Field `index` as a real.
    pub fn real(&self, index: usize) -> f64 {
        self.fields.get(index).map_or(f64::NAN, |field| real(field))
    }

    /// Field `index` as text.
    pub fn text(&self, index: usize) -> String {
        self.fields.get(index).cloned().unwrap_or_default()
    }

    /// Fields from `index` onward as integers, stopping at the first blank.
    pub fn integers_from(&self, index: usize) -> Vec<i64> {
        self.fields
            .iter()
            .skip(index)
            .take_while(|field| !field.is_empty())
            .map(|field| field.trim().parse().unwrap_or(0))
            .collect()
    }

    /// Fields from `index` onward as reals, stopping at the first blank.
    pub fn reals_from(&self, index: usize) -> Vec<f64> {
        self.fields
            .iter()
            .skip(index)
            .take_while(|field| !field.is_empty())
            .map(|field| real(field))
            .collect()
    }
}

/// Split a bulk-data file into cards, joining each one's continuations.
pub fn parse(text: &str) -> Vec<Card> {
    let mut cards: Vec<Card> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('$') {
            continue;
        }
        let opener = line.chars().next().unwrap_or(' ');
        if matches!(opener, '*' | '+' | ' ' | ',') {
            let continued = fields_of(line, opener == '*');
            if let Some(card) = cards.last_mut() {
                card.fields.extend(continued);
            }
            continue;
        }
        if let Some((name, rest)) = line.split_once(',') {
            cards.push(Card {
                name: name.trim().to_string(),
                fields: rest.split(',').map(|f| f.trim().to_string()).collect(),
            });
            continue;
        }
        let opening: String = line.chars().take(8).collect();
        let wide = opening.contains('*');
        cards.push(Card {
            name: opening.trim().trim_end_matches('*').to_string(),
            fields: fields_of(line, wide),
        });
    }
    for card in &mut cards {
        while card.fields.last().is_some_and(|field| field.is_empty()) {
            card.fields.pop();
        }
    }
    cards
}

/// The fields on one physical line, after its eight-column opener.
///
/// A fixed-field line always carries its full complement -- four large or eight
/// small -- however short the text is: the format is columnar, so a line that
/// stops early has left those columns blank rather than omitted them. Getting
/// this wrong silently shifts every field after a card's reserved columns.
fn fields_of(line: &str, wide: bool) -> Vec<String> {
    if let Some((_, rest)) = line.split_once(',') {
        return rest
            .split(',')
            .map(|field| field.trim().to_string())
            .collect();
    }
    let width = if wide { 16 } else { 8 };
    let count = if wide { 4 } else { 8 };
    let characters: Vec<char> = line.chars().collect();
    (0..count)
        .map(|index| {
            let start = (8 + index * width).min(characters.len());
            let end = (start + width).min(characters.len());
            characters[start..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect()
}

/// One real field, in any of the notations NASTRAN accepts.
///
/// The awkward one is the classic implicit exponent: `1.5-3` means `1.5e-3`,
/// because the format predates having a column to spare for the `E`.
pub fn real(field: &str) -> f64 {
    let text = field.trim().replace(['d', 'D'], "E");
    if text.is_empty() {
        return 0.0;
    }
    if !text.contains(['e', 'E']) {
        for (index, character) in text.char_indices().skip(1) {
            if matches!(character, '+' | '-') {
                let (mantissa, exponent) = text.split_at(index);
                return format!("{mantissa}E{exponent}").parse().unwrap_or(f64::NAN);
            }
        }
    }
    text.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_field_card_reads_its_continuation_as_more_fields() {
        let text = "\
GRID*                  7                             1.5              0.
*                  -2.25
";
        let cards = parse(text);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "GRID");
        assert_eq!(cards[0].integer(0), 7);
        assert_eq!(cards[0].text(1), "");
        assert_eq!(cards[0].real(2), 1.5);
        assert_eq!(cards[0].real(4), -2.25);
    }

    #[test]
    fn a_small_field_card_and_a_free_field_card_read_the_same_fields() {
        let small = parse("CTRIA3       116       4       8       9      16\n");
        let free = parse("CTRIA3,116,4,8,9,16\n");
        assert_eq!(small[0].name, free[0].name);
        assert_eq!(small[0].integers_from(0), free[0].integers_from(0));
        assert_eq!(small[0].integers_from(0), vec![116, 4, 8, 9, 16]);
    }

    #[test]
    fn a_comment_and_a_blank_line_are_not_cards() {
        assert!(parse("$pyNastran: punch=True\n\n").is_empty());
    }

    #[test]
    fn every_notation_the_format_allows_for_a_real_reads_the_same_number() {
        for text in [".0015", "1.5-3", "1.5E-3", "1.5e-3", "1.5D-3", "0.0015"] {
            assert!((real(text) - 0.0015).abs() < 1e-18, "{text}");
        }
        assert_eq!(real("71000000000."), 7.1e10);
        assert_eq!(real("-2.1"), -2.1);
    }
}
