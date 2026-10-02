// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A main deck bounded by its declared doors, and the monuments that share
//! the floor with its seats.
//!
//! An airliner seats its passengers between its first and its last door pair:
//! the flight deck, the forward galley or lavatory complex and the door-1
//! cross-aisle lie ahead of the first seat, and the aft galley, the last
//! cross-aisle and the aft pressure bulkhead behind the last one. Where a
//! source prints the door stations, the main deck therefore runs from one
//! monument bay ahead of the first door to one behind the last, and every
//! door between them is a cross-aisle no seat row may overlap. The generic
//! nose and tail-cone lengths only bound a cabin whose doors are unknown.
//!
//! # Frames
//!
//! A declared station is metres aft of the nose tip on the body the source
//! draws, and a built station is the cabin frame's own x (positive aft).
//! On a body of the declared length the two differ by the nose position
//! only. A body of another length is a stretch or a shrink: the first door
//! stays where it is relative to the nose, the last door moves with the tail,
//! and the doors between are spaced in proportion, which is how a fuselage
//! plug moves them. A mapping that would make two door zones overlap or put
//! one outside the body is refused, and the generic frame applies instead.
//!
//! # Monument lengths
//!
//! A lavatory module is 32 in (0.813 m) long in the Boeing 777-9 standard
//! arrangement (777X ACAP D6-86073 Rev G, Figure 2-4, "32-in lavatory"). A
//! galley is as deep as the trolley it stows: a full-size ATLAS trolley is
//! 0.81 m long (ATLAS trolley standard, as summarized in the Wikipedia
//! article "Airline service trolley"; a secondary source, carried until a
//! galley drawing is in the evidence set). A bay is as long as its longest
//! monument, so a declared cabin charges [`MONUMENT_BAY_LENGTH_M`] per bay.

use alas_config::CertifiedExitLayout;

use super::exit_rules::{exit_spec, ExitSpec};
use crate::geometry::CabinGeometry;

/// One inch, m.
const INCH_M: f64 = 0.0254;

/// Longitudinal length of a lavatory module, m (Boeing 777X ACAP D6-86073
/// Rev G, Figure 2-4: 32 in).
pub const LAVATORY_LENGTH_M: f64 = 32.0 * INCH_M;

/// Longitudinal depth of a galley, m: the length of the full-size ATLAS
/// trolley it stows (secondary source, see the module documentation).
pub const GALLEY_LENGTH_M: f64 = 0.81;

/// Length a monument bay of a declared cabin takes from the seats: its
/// longest monument, the lavatory.
pub const MONUMENT_BAY_LENGTH_M: f64 = LAVATORY_LENGTH_M;

/// Tolerance on a body length that counts as the declared one, m.
const SAME_BODY_TOLERANCE_M: f64 = 1e-6;

/// A cabin monument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonumentKind {
    /// A galley, with its trolleys.
    Galley,
    /// A lavatory module.
    Lavatory,
}

impl MonumentKind {
    /// Its sourced longitudinal length, m.
    pub const fn length_m(self) -> f64 {
        match self {
            Self::Galley => GALLEY_LENGTH_M,
            Self::Lavatory => LAVATORY_LENGTH_M,
        }
    }
}

/// One declared door pair, placed on the built body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorStation {
    /// Station of the door centre in the cabin frame, m.
    pub x: f64,
    /// The pair's CS 25.807 classification.
    pub spec: &'static ExitSpec,
}

impl DoorStation {
    /// The cross-aisle the door pair keeps clear of seats: its opening, fore
    /// and aft of the door centre.
    pub fn zone(&self) -> (f64, f64) {
        let half = 0.5 * self.spec.width_m;
        (self.x - half, self.x + half)
    }
}

/// The declared doors of `layout` on the built body of `g`, forward to aft,
/// or `None` when the layout does not print usable stations.
pub(crate) fn resolve_door_stations(
    g: &CabinGeometry,
    layout: &CertifiedExitLayout,
) -> Option<Vec<DoorStation>> {
    if !layout.has_stations() {
        return None;
    }
    let reference_length = layout.station_body_length_m?;
    let mut declared = Vec::with_capacity(layout.pairs.len());
    for pair in layout.pairs {
        declared.push((pair.station_m?, exit_spec(pair.exit_type)?));
    }
    declared.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (first, last) = (declared.first()?.0, declared.last()?.0);
    if last <= first || !g.fus_len.is_finite() || g.fus_len <= 0.0 {
        return None;
    }

    let stretch = g.fus_len - reference_length;
    let (x_first, x_last) = (g.x_min + first, g.x_min + last + stretch);
    let doors: Vec<DoorStation> = declared
        .into_iter()
        .map(|(station, spec)| {
            let x = if stretch.abs() <= SAME_BODY_TOLERANCE_M {
                g.x_min + station
            } else {
                x_first + (station - first) * (x_last - x_first) / (last - first)
            };
            DoorStation { x, spec }
        })
        .collect();

    let inside = doors.iter().all(|door| {
        let (a, b) = door.zone();
        a.is_finite() && b.is_finite() && a >= g.x_min && b <= g.x_max
    });
    let separated = doors
        .windows(2)
        .all(|pair| pair[0].zone().1 < pair[1].zone().0);
    (inside && separated).then_some(doors)
}

/// The main deck of a cabin with declared doors: from one monument bay ahead
/// of the first door's cross-aisle to one behind the last, inside the body.
pub(crate) fn declared_main_deck_bounds(g: &CabinGeometry) -> Option<(f64, f64)> {
    let first = g.door_stations.first()?;
    let last = g.door_stations.last()?;
    let x0 = (first.zone().0 - MONUMENT_BAY_LENGTH_M).max(g.x_min);
    let x1 = (last.zone().1 + MONUMENT_BAY_LENGTH_M).min(g.x_max);
    (x1 > x0).then_some((x0, x1))
}

/// The floor no seat row may overlap on a declared main deck, forward to
/// aft: every door's cross-aisle, and behind the doors between the first and
/// the last, the `extra_bays` monument bays a cabin needs beyond its end and
/// class-boundary bays, one per door in turn from the middle of the cabin
/// outward.
///
/// Returns the spans and the centres of the monument bays it placed.
pub(crate) fn seat_obstacles(
    doors: &[DoorStation],
    extra_bays: i64,
) -> (Vec<(f64, f64)>, Vec<f64>) {
    let mut spans: Vec<(f64, f64)> = doors.iter().map(DoorStation::zone).collect();
    let mut bay_centres = Vec::new();
    if doors.is_empty() {
        return (spans, bay_centres);
    }
    let interior: Vec<usize> = if doors.len() > 2 {
        (1..doors.len() - 1).collect()
    } else {
        vec![0]
    };
    let order = middle_out(&interior);
    for k in 0..extra_bays.max(0) as usize {
        let door = order[k % order.len()];
        let start = spans[door].1;
        spans[door].1 = start + MONUMENT_BAY_LENGTH_M;
        bay_centres.push(start + 0.5 * MONUMENT_BAY_LENGTH_M);
    }
    (spans, bay_centres)
}

/// `items` reordered from the middle outward, so a cabin with one extra bay
/// puts it amidships and further ones alternate forward and aft.
fn middle_out(items: &[usize]) -> Vec<usize> {
    let n = items.len();
    let mut order = Vec::with_capacity(n);
    let mid = (n.saturating_sub(1)) / 2;
    order.push(items[mid]);
    for step in 1..n {
        if mid + step < n {
            order.push(items[mid + step]);
        }
        if step <= mid {
            order.push(items[mid - step]);
        }
    }
    order.truncate(n);
    order
}

/// The first station at or after `x` from which a block `length` long clears
/// every obstacle.
pub(crate) fn clear_of(obstacles: &[(f64, f64)], mut x: f64, length: f64) -> f64 {
    loop {
        let blocked = obstacles
            .iter()
            .find(|&&(a, b)| x < b - 1e-9 && x + length > a + 1e-9);
        match blocked {
            Some(&(_, b)) => x = b,
            None => return x,
        }
    }
}

/// Total obstacle length inside `[x0, x1]`.
pub(crate) fn obstacle_length_within(obstacles: &[(f64, f64)], x0: f64, x1: f64) -> f64 {
    obstacles
        .iter()
        .map(|&(a, b)| (b.min(x1) - a.max(x0)).max(0.0))
        .sum()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bay_is_as_long_as_its_longest_monument() {
        assert!(MONUMENT_BAY_LENGTH_M >= MonumentKind::Galley.length_m());
        assert!(MONUMENT_BAY_LENGTH_M >= MonumentKind::Lavatory.length_m());
        assert!((LAVATORY_LENGTH_M - 0.8128).abs() < 1e-12);
    }

    #[test]
    fn a_block_jumps_an_obstacle_it_would_overlap_and_not_one_it_clears() {
        let obstacles = [(10.0, 11.0), (20.0, 21.5)];
        assert_eq!(clear_of(&obstacles, 9.0, 0.8), 9.0);
        assert_eq!(clear_of(&obstacles, 9.5, 0.8), 11.0);
        assert_eq!(clear_of(&obstacles, 10.5, 0.8), 11.0);
        assert_eq!(clear_of(&obstacles, 19.9, 0.8), 21.5);
        assert!((obstacle_length_within(&obstacles, 10.5, 21.0) - 1.5).abs() < 1e-12);
    }

    #[test]
    fn extra_bays_go_amidships_first_and_extend_the_doors_aft() {
        let spec = exit_spec("A").unwrap();
        let doors: Vec<DoorStation> = [5.0, 15.0, 25.0, 35.0, 45.0]
            .into_iter()
            .map(|x| DoorStation { x, spec })
            .collect();
        let (spans, bays) = seat_obstacles(&doors, 2);
        // Interior doors 1, 2, 3: the middle one first, then aft of it.
        assert_eq!(bays.len(), 2);
        assert!((spans[2].1 - (25.0 + 0.535 + MONUMENT_BAY_LENGTH_M)).abs() < 1e-12);
        assert!((spans[3].1 - (35.0 + 0.535 + MONUMENT_BAY_LENGTH_M)).abs() < 1e-12);
        assert!((spans[1].1 - (15.0 + 0.535)).abs() < 1e-12);
        assert_eq!(middle_out(&[1, 2, 3, 4]), vec![2, 3, 1, 4]);
        assert_eq!(middle_out(&[7]), vec![7]);
    }
}
