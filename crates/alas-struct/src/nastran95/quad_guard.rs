// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pre-flight screen for NASTRAN-95's `CQUAD4` degeneracy test.
//!
//! NASTRAN-95's `Q4NRMS` rejects an element with user fatal message 2224
//! (`ILLEGAL GEOMETRY OR CONNECTIONS`) when, at any corner, the squared
//! magnitude of the cross product of the two isoparametric tangent vectors is
//! not above the absolute constant `1.0E-11` in the deck's length unit to the
//! fourth power ([`super::units`]). The solver names the element only inside a
//! failed run; this screen evaluates the same quantity on the coordinates as
//! the deck will print them, before the solver starts, so a flagged element is
//! reported by id and number instead of as an opaque fatal.
//!
//! At a corner with neighbours `P+` and `P-`, the tangent vectors of the
//! bilinear map are the half-edges `(P+ - P)/2` and `(P- - P)/2`, so the
//! measure is `|(P+ - P) x (P- - P)|^2 / 16`. The doc example in
//! [`super::units`] (half-edges 14 mm and 0.235 mm) reproduces the quoted
//! 1.1e-11 m^4.
//!
//! The screen is deliberately a little stricter than the solver's constant
//! ([`GUARD_MARGIN`]): the solver evaluates the test in single precision on
//! coordinates it has read back from eight-column fields, so an element within
//! rounding of the limit is not trusted to pass. It is a guard against the
//! solver's fatal, not a mesh-quality criterion.

use super::field;
use super::units::UnitScale;
use crate::mesh::Deck;

/// NASTRAN-95's absolute degeneracy constant, deck length units to the fourth.
pub const QUAD4_DEGENERACY_LIMIT: f64 = 1.0e-11;

/// Screening factor on [`QUAD4_DEGENERACY_LIMIT`] covering single-precision
/// evaluation and eight-column coordinate rounding (the quoted B747 panel is
/// 1.1 times the limit and was rejected).
pub const GUARD_MARGIN: f64 = 2.0;

/// One `CQUAD4` the solver would reject.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegenerateQuad {
    /// Element identifier.
    pub eid: i64,
    /// Smallest corner `|dX/dxi x dX/deta|^2`, deck length units to the fourth.
    pub measure: f64,
}

/// A coordinate as the deck prints it (at most eight columns).
fn printed(value_m: f64, units: UnitScale) -> f64 {
    read_fixed_field_real(&field::real(value_m * units.length)).unwrap_or(f64::NAN)
}

/// Read a real in the fixed-field spelling [`field::real`] writes, including
/// the exponent shorthand without an `E` (`2.2467-6`, `7.1+10`).
fn read_fixed_field_real(text: &str) -> Option<f64> {
    if let Ok(value) = text.parse::<f64>() {
        return Some(value);
    }
    // The shorthand's exponent sign is the last `+` or `-` after the first
    // character (a leading sign belongs to the mantissa).
    let split = text
        .char_indices()
        .skip(1)
        .filter(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i)
        .last()?;
    format!("{}e{}", &text[..split], &text[split..])
        .parse::<f64>()
        .ok()
}

/// Every `CQUAD4` whose smallest corner measure is within
/// [`GUARD_MARGIN`] times [`QUAD4_DEGENERACY_LIMIT`] (or is not finite), in
/// element order.
pub fn degenerate_quads(deck: &Deck, units: UnitScale) -> Vec<DegenerateQuad> {
    let grid = |nid: i64| -> Option<[f64; 3]> {
        let xyz = deck.grid_xyz(nid)?;
        Some([
            printed(xyz[0], units),
            printed(xyz[1], units),
            printed(xyz[2], units),
        ])
    };
    let mut flagged = Vec::new();
    for quad in deck.quads() {
        let corners: Option<Vec<[f64; 3]>> = quad.nodes.iter().map(|&nid| grid(nid)).collect();
        let measure = corners
            .filter(|corners| {
                corners.len() == 4 && corners.iter().flatten().all(|value| value.is_finite())
            })
            .map_or(f64::NAN, |corners| smallest_corner_measure(&corners));
        if measure.is_nan() || measure <= GUARD_MARGIN * QUAD4_DEGENERACY_LIMIT {
            flagged.push(DegenerateQuad {
                eid: quad.eid,
                measure,
            });
        }
    }
    flagged
}

fn smallest_corner_measure(corners: &[[f64; 3]]) -> f64 {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let mut smallest = f64::INFINITY;
    for i in 0..4 {
        let here = corners[i];
        let next = sub(corners[(i + 1) % 4], here);
        let previous = sub(corners[(i + 3) % 4], here);
        let cross = [
            next[1] * previous[2] - next[2] * previous[1],
            next[2] * previous[0] - next[0] * previous[2],
            next[0] * previous[1] - next[1] * previous[0],
        ];
        let measure = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]) / 16.0;
        smallest = smallest.min(measure);
    }
    smallest
}

/// A diagnostic naming the flagged elements, or `None` when there are none.
/// At most [`LISTED_ELEMENTS`] element ids are spelled out.
pub fn degeneracy_message(flagged: &[DegenerateQuad], units: UnitScale) -> Option<String> {
    if flagged.is_empty() {
        return None;
    }
    let listed = flagged
        .iter()
        .take(LISTED_ELEMENTS)
        .map(|quad| format!("{} ({:.2e})", quad.eid, quad.measure))
        .collect::<Vec<_>>()
        .join(", ");
    let more = flagged.len().saturating_sub(LISTED_ELEMENTS);
    Some(format!(
        "NASTRAN-95 element guard: {} CQUAD4 element(s) would be rejected by the solver's \
         corner test |dX/dxi x dX/deta|^2 > {:.0e} (deck length unit {} per m; screen at {}x \
         the limit): {listed}{}. Refine or merge these panels; the solver was not started",
        flagged.len(),
        QUAD4_DEGENERACY_LIMIT,
        units.length,
        GUARD_MARGIN,
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        },
    ))
}

/// Element ids spelled out in a guard diagnostic.
const LISTED_ELEMENTS: usize = 8;

// Tests assert on values they construct directly, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Shell;

    /// A rectangular quad of `a` by `b` metres in the x-z plane.
    fn deck_with_panel(a: f64, b: f64) -> Deck {
        let mut deck = Deck::new();
        for (nid, xyz) in [
            (1, [0.0, 0.0, 0.0]),
            (2, [a, 0.0, 0.0]),
            (3, [a, 0.0, b]),
            (4, [0.0, 0.0, b]),
        ] {
            deck.add_grid(nid, xyz);
        }
        deck.quads.push(Shell {
            eid: 77,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        deck
    }

    #[test]
    fn the_documented_trailing_edge_panel_is_flagged_in_metres_and_clear_in_decimetres() {
        // Half-edges 14 mm and 0.235 mm: |V1 x V2|^2 = 1.08e-11 m^4.
        let deck = deck_with_panel(0.028, 0.00047);
        let metres = degenerate_quads(&deck, UnitScale::SI);
        assert_eq!(metres.len(), 1);
        assert_eq!(metres[0].eid, 77);
        assert!(
            (metres[0].measure / 1.08e-11 - 1.0).abs() < 0.02,
            "{metres:?}"
        );
        assert!(degenerate_quads(&deck, UnitScale::DECIMETRE).is_empty());
    }

    #[test]
    fn the_measure_scales_with_the_fourth_power_of_the_length_unit() {
        let deck = deck_with_panel(0.028, 0.00047);
        let si = degenerate_quads(&deck, UnitScale::SI)[0].measure;
        let scaled = UnitScale {
            length: 10.0,
            mass: 0.1,
        };
        let flagged_at_10 = degenerate_quads(&deck, scaled);
        assert!(flagged_at_10.is_empty());
        // 1e4 above the SI value, checked through the unflagged measure.
        let panel = [
            [0.0, 0.0, 0.0],
            [0.28, 0.0, 0.0],
            [0.28, 0.0, 0.0047],
            [0.0, 0.0, 0.0047],
        ];
        assert!((smallest_corner_measure(&panel) / si / 1.0e4 - 1.0).abs() < 0.02);
    }

    #[test]
    fn a_collapsed_or_unplaced_element_is_flagged_never_passed() {
        let mut deck = deck_with_panel(1.0, 1.0);
        // Collapse one edge: a triangle written as a quad.
        deck.quads[0].nodes = vec![1, 2, 3, 3];
        assert_eq!(degenerate_quads(&deck, UnitScale::SI).len(), 1);
        // A quad naming a missing grid cannot be screened and is flagged.
        deck.quads[0].nodes = vec![1, 2, 3, 99];
        let flagged = degenerate_quads(&deck, UnitScale::SI);
        assert_eq!(flagged.len(), 1);
        assert!(flagged[0].measure.is_nan());
    }

    #[test]
    fn the_exponent_shorthand_reads_back() {
        for (text, value) in [
            ("2.2467-6", 2.2467e-6),
            ("7.1+10", 7.1e10),
            ("-1.5-5", -1.5e-5),
            ("0.015564", 0.015564),
            ("1.-5", 1.0e-5),
        ] {
            let read = read_fixed_field_real(text).unwrap();
            assert!((read / value - 1.0).abs() < 1e-12, "{text}");
        }
        assert!(read_fixed_field_real("abc").is_none());
    }

    #[test]
    fn the_message_names_the_elements() {
        let deck = deck_with_panel(0.028, 0.00047);
        let flagged = degenerate_quads(&deck, UnitScale::SI);
        let message = degeneracy_message(&flagged, UnitScale::SI).unwrap();
        assert!(message.contains("77 (") && message.contains("solver was not started"));
        assert!(degeneracy_message(&[], UnitScale::SI).is_none());
    }
}
