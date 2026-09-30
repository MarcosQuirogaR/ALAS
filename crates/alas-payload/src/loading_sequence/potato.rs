// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The CG envelope (potato) of a set of loading sequences, and end-to-end
//! composition of sequences.

use super::LoadingSequence;

/// One level of the loading-sequence envelope: at `mass_kg`, the most
/// forward and most aft CG any of the sampled sequences reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PotatoPoint {
    /// The common mass level, kg.
    pub mass_kg: f64,
    /// The most forward (smallest x) CG any sequence reaches at this mass,
    /// aircraft body axes, m.
    pub min_x_m: f64,
    /// The most aft (largest x) CG any sequence reaches at this mass,
    /// aircraft body axes, m.
    pub max_x_m: f64,
}

/// Linearly interpolate `sequence`'s CG at `mass_kg`; `None` if `sequence`
/// has no points, or if `mass_kg` lies outside this sequence's own
/// `[first.mass_kg, last.mass_kg]` span (beyond a small floating-point
/// tolerance).
///
/// Earlier versions clamped an out-of-span query to the nearest endpoint,
/// which let a sequence that ends below the shared ZFW (e.g. a cargo-only
/// sequence with no passenger mass added) hold its *last* CG flat at every
/// higher mass level a longer sequence still reaches -- silently
/// manufacturing a boundary at masses this sequence never actually visited.
/// Returning `None` instead makes the caller ([`potato_boundary`]) exclude
/// this sequence from every mass level it does not cover, so the envelope
/// at a given mass only ever reflects sequences that actually reach it.
pub(super) fn interpolate_x(sequence: &LoadingSequence, mass_kg: f64) -> Option<f64> {
    const TOLERANCE_KG: f64 = 1.0e-6;
    let points = &sequence.points;
    let first = points.first()?;
    let last = points.last()?;
    if mass_kg < first.mass_kg - TOLERANCE_KG || mass_kg > last.mass_kg + TOLERANCE_KG {
        return None;
    }
    if mass_kg <= first.mass_kg {
        return Some(first.x_m);
    }
    if mass_kg >= last.mass_kg {
        return Some(last.x_m);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if mass_kg >= a.mass_kg && mass_kg <= b.mass_kg {
            let span = b.mass_kg - a.mass_kg;
            if span <= 0.0 {
                return Some(a.x_m);
            }
            let blend = (mass_kg - a.mass_kg) / span;
            return Some(a.x_m + blend * (b.x_m - a.x_m));
        }
    }
    Some(last.x_m)
}

/// The CG envelope (potato) of `sequences`, at `n_levels` mass levels evenly
/// spaced between the lowest DOW mass and the highest ZFW mass any sequence
/// carries.
///
/// Every physical sequence built by this module shares the same DOW and ZFW
/// mass (all load the same total payload from the same starting point), so
/// in the ordinary case this samples from one shared endpoint to the other;
/// sequences from different starting points are still accepted; the
/// envelope is simply wider at the shared range's edges.
///
/// Returns an empty vector for `n_levels == 0` or an empty `sequences`.
///
/// A mass level where no sequence reaches (per [`interpolate_x`]'s own
/// exclusion of out-of-span queries) is omitted from the result rather than
/// synthesized from whichever sequence happens to hold that mass flat: see
/// [`interpolate_x`]'s doc comment for the composition bug this avoids.
pub fn potato_boundary(sequences: &[LoadingSequence], n_levels: usize) -> Vec<PotatoPoint> {
    if n_levels == 0 || sequences.is_empty() {
        return Vec::new();
    }
    let min_mass_kg = sequences
        .iter()
        .filter_map(|sequence| sequence.points.first().map(|point| point.mass_kg))
        .fold(f64::INFINITY, f64::min);
    let max_mass_kg = sequences
        .iter()
        .filter_map(|sequence| sequence.points.last().map(|point| point.mass_kg))
        .fold(f64::NEG_INFINITY, f64::max);
    if !(min_mass_kg.is_finite() && max_mass_kg.is_finite()) || max_mass_kg < min_mass_kg {
        return Vec::new();
    }
    let levels: Vec<f64> = (0..n_levels)
        .map(|index| {
            if n_levels == 1 {
                max_mass_kg
            } else {
                min_mass_kg + (max_mass_kg - min_mass_kg) * index as f64 / (n_levels - 1) as f64
            }
        })
        .collect();
    potato_boundary_at(sequences, &levels)
}

/// The CG envelope of `sequences` at the given mass `levels`, kg; a level no
/// sequence reaches is omitted (see [`potato_boundary`]).
pub fn potato_boundary_at(sequences: &[LoadingSequence], levels: &[f64]) -> Vec<PotatoPoint> {
    levels
        .iter()
        .filter_map(|&mass_kg| {
            let mut min_x_m = f64::INFINITY;
            let mut max_x_m = f64::NEG_INFINITY;
            for sequence in sequences {
                if let Some(x_m) = interpolate_x(sequence, mass_kg) {
                    min_x_m = min_x_m.min(x_m);
                    max_x_m = max_x_m.max(x_m);
                }
            }
            if !(min_x_m.is_finite() && max_x_m.is_finite()) {
                return None;
            }
            Some(PotatoPoint {
                mass_kg,
                min_x_m,
                max_x_m,
            })
        })
        .collect()
}

/// Physically compose two sequences end to end: `second` must already start
/// (its first point) at `first`'s own last point -- i.e. `second` was built
/// with `first`'s end point as its own `dow` argument. Returns `first`'s
/// points followed by `second`'s points with that shared junction point not
/// duplicated.
///
/// This is how a cargo sequence and a passenger sequence are chained into
/// one sequence that starts at the aircraft's true DOW and ends at the true
/// ZFW (DOW plus *all* payload, not just one category): loading cargo alone
/// or boarding passengers alone each only carries the aircraft to a partial
/// mass, so neither one, by itself, is a physical loading order for a
/// mixed-payload aircraft. Composing both orders (cargo-then-passengers and
/// passengers-then-cargo, in both directions each) is what
/// [`crate::loading_sequence`]'s callers are expected to build before
/// calling [`potato_boundary`] on a mixed-payload layout.
#[must_use]
pub fn concat_sequences(
    name: &str,
    first: &LoadingSequence,
    second: &LoadingSequence,
) -> LoadingSequence {
    let mut points = first.points.clone();
    if let (Some(junction), Some(second_first)) = (points.last().copied(), second.points.first()) {
        let same_junction = (junction.mass_kg - second_first.mass_kg).abs() < 1.0e-6
            && (junction.x_m - second_first.x_m).abs() < 1.0e-9;
        let tail = if same_junction {
            &second.points[1..]
        } else {
            &second.points[..]
        };
        points.extend_from_slice(tail);
    } else {
        points.extend_from_slice(&second.points);
    }
    LoadingSequence {
        name: name.to_owned(),
        points,
    }
}
