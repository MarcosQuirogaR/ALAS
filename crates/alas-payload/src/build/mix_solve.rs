// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The inverse solve from a seat-count class mix to a floor-length class mix.

use alas_config::{CertifiedExitLayout, PassengerCabinConfig};

use super::{class_config, simulate_passenger_counts_product, PassengerCounts};
use crate::cabin::{abreast, resolve_aisle_width, MIN_PITCH};
use crate::geometry::CabinGeometry;

/// Largest summed absolute deviation of the class seat shares from their
/// targets (a fraction, both classes of a two-class cabin counted) for which a
/// pass of the mix solve may stand in for its last pass. Engineering estimate:
/// about two share points per class, the size of the whole-row discontinuity
/// on a widebody business class.
const MIX_ERROR_TOLERANCE: f64 = 0.06;

/// Floor-length share of each class, by class name.
type LengthMix<'a> = Vec<(&'a str, f64)>;

/// The floor-length class mix whose whole-row layout best reproduces the
/// requested seat-count mix under the seat ceiling.
pub(super) fn length_mix_for_seat_targets<'a>(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    target_mix: &[(&'a str, f64)],
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> LengthMix<'a> {
    let mut weights = target_mix
        .iter()
        .filter(|(_, share)| share.is_finite() && *share > 0.0)
        .map(|&(name, share)| {
            let class = class_config(pax, name);
            let deck = &g.passenger_decks[0];
            let x = 0.5 * (g.cabin_start_x + g.cabin_end_x);
            let seats_abreast = abreast(class, deck, g, resolve_aisle_width(pax, 20), x).max(1);
            (
                name,
                share * class.pitch_m.max(MIN_PITCH) / seats_abreast as f64,
            )
        })
        .collect::<Vec<_>>();
    normalize_mix(&mut weights);

    // Row rounding makes the inverse discontinuous, so a damped multiplicative
    // correction is both more stable and more honest than pretending there is
    // a closed form. Twenty passes is tiny beside one geometry build.
    //
    // Whole rows leave the iteration hunting between two neighbouring row
    // counts, so under a seat ceiling the last pass is an arbitrary pick of
    // the two, and one of them can leave the floor short of the ceiling. There
    // the pass that seats the most passengers wins, the closest seat mix
    // breaking a tie.
    let track_best = source_capacity_cap.is_some();
    let mut best: Option<(i64, f64, LengthMix<'a>)> = None;
    let mut track = |weights: &[(&'a str, f64)], counts: PassengerCounts| {
        let seats = counts.total();
        let total = seats.max(1) as f64;
        let error: f64 = target_mix
            .iter()
            .map(|(name, share)| (counts.for_class(name) as f64 / total - share).abs())
            .sum();
        let better = error <= MIX_ERROR_TOLERANCE
            && best
                .as_ref()
                .is_none_or(|(most, least, _)| seats > *most || (seats == *most && error < *least));
        if better {
            best = Some((seats, error, weights.to_vec()));
        }
    };
    for _ in 0..20 {
        let counts = simulate_passenger_counts_product(
            g,
            pax,
            &weights,
            source_capacity_cap,
            source_exit_layout,
        );
        if track_best {
            track(&weights, counts);
        }
        let total = counts.total().max(1) as f64;
        for (name, weight) in &mut weights {
            let target = target_mix
                .iter()
                .find(|(other, _)| other == name)
                .map_or(0.0, |(_, share)| *share);
            let achieved = counts.for_class(name) as f64 / total;
            let correction = if achieved > 0.0 {
                (target / achieved).clamp(0.25, 4.0).powf(0.65)
            } else {
                2.0
            };
            *weight *= correction;
        }
        normalize_mix(&mut weights);
    }
    if track_best {
        let counts = simulate_passenger_counts_product(
            g,
            pax,
            &weights,
            source_capacity_cap,
            source_exit_layout,
        );
        track(&weights, counts);
        if let Some((_, _, fullest)) = best {
            return fullest;
        }
    }
    weights
}

fn normalize_mix(mix: &mut [(&str, f64)]) {
    let total = mix.iter().map(|(_, share)| *share).sum::<f64>();
    if total > 0.0 && total.is_finite() {
        for (_, share) in mix {
            *share /= total;
        }
    }
}
