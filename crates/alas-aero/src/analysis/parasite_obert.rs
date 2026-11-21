// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The excrescence items of a turboprop transport.
//!
//! E. Obert, *Aerodynamic Design of Transport Aircraft*, IOS Press, 2009,
//! Fig. 40.29 (p. 537): the industrial estimate of the subsonic zero-lift drag
//! of the Fokker 50 at M 0.50 against its flight-test parabolic polar. On a
//! component sum (wings, fuselage, nacelles, tails, rough skin) of 191.4
//! counts it books, as separate rows, control-surface gaps 12.6, engine
//! nacelle intakes and exhausts 14.3, other excrescences 6.4 and the extra
//! drag of excrescences in the propeller slipstream 2.1 counts. Each is taken
//! here as the same fraction of the component sum of every turboprop: the
//! rows the book identifies physically, 18.5 % together. Its "unaccounted
//! drag 2 % (previous experience)" is a closure on flight tests and its trim
//! drag (-2.3) the model computes itself; neither is applied. The items
//! replace, for a turboprop, Raymer's leakage and protuberance allowance and
//! the separate upsweep and base items (the book's fuselage row holds its
//! afterbody).

/// One item: its name and its share of the component sum.
pub const TURBOPROP_EXCRESCENCES: [(&str, f64); 4] = [
    (
        "Control-surface gaps (Obert 2009, Fig. 40.29)",
        12.6 / COMPONENT_SUM,
    ),
    (
        "Nacelle intakes and exhausts (Obert 2009, Fig. 40.29)",
        14.3 / COMPONENT_SUM,
    ),
    (
        "Other excrescences (Obert 2009, Fig. 40.29)",
        6.4 / COMPONENT_SUM,
    ),
    (
        "Excrescences in the slipstream (Obert 2009, Fig. 40.29)",
        2.1 / COMPONENT_SUM,
    ),
];

/// The component sum of Fig. 40.29, counts.
const COMPONENT_SUM: f64 = 191.4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_items_are_the_books_rows_over_its_component_sum() {
        let total: f64 = TURBOPROP_EXCRESCENCES.iter().map(|(_, share)| share).sum();
        assert!((total - 35.4 / 191.4).abs() < 1e-12);
        assert!((total - 0.185).abs() < 1e-3);
    }
}
