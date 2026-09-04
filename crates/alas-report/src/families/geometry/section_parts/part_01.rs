// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_pipeline::cabin_scene::{
    Box3, CabinScene, CargoItem, ResolvedDeck, SeatRow, SectionStation, SourcedContour,
    WindowAperture,
};

use crate::scene::{Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// A closed contour in the section plane: `[y, z]` pairs in metres, y to
/// starboard and z up, in the scene's own aircraft frame.
type Ring = Vec<[f64; 2]>;

/// Half-thickness of a drawn deck floor, in metres.
///
/// The scene resolves a floor as a height, not as a slab: it is a plane in the
/// layout solver. A section has to give it a thickness to be visible at all,
/// and this one is a drawing constant rather than a structural depth.
const FLOOR_HALF_THICKNESS_M: f64 = 0.045;

/// Standing height of the scale figure placed in an aisle, in metres.
const OCCUPANT_HEIGHT_M: f64 = 1.75;

/// Shoulder half-width of the scale figure, in metres.
const OCCUPANT_HALF_WIDTH_M: f64 = 0.235;

/// Clear distance a drawn overhead run may stand off the liner before the
/// figure reports it as unattached, in metres.
///
/// The scene exports overhead runs as free envelopes and states that their
/// attachment geometry is missing. A section is where that shows: a bin
/// floating clear of the lining is drawn where the solver put it, and the gap
/// is measured rather than closed with a decorative bracket.
const BIN_ATTACHMENT_TOLERANCE_M: f64 = 0.06;

/// One seat cut by the section plane.
struct SeatSlice {
    /// Lateral centre, in metres.
    y_m: f64,
    /// Seat width, in metres.
    width_m: f64,
    /// Floor the seat stands on, in metres.
    floor_z_m: f64,
    /// Overall seat height above the floor, in metres.
    height_m: f64,
    /// Whether the layout solver filled this seat.
    occupied: bool,
}

/// One overhead run cut by the section plane, with its exported profile.
struct BinSlice {
    /// Exported cross-section profile, unmodified.
    profile: Ring,
    /// How the layout solver classified the run.
    kind: String,
}

/// One cargo item cut by the section plane.
struct CargoSlice {
    /// Contour to draw: the ULD's normalized profile fitted to its envelope,
    /// or the envelope rectangle when no ULD contour was resolved.
    ring: Ring,
    /// Label printed on the item.
    label: String,
    /// Item mass, in kilograms.
    mass_kg: f64,
    /// Whether the contour came from a registered ULD rather than a box.
    from_uld: bool,
}

/// One deck cut by the section plane, with everything standing on it.
struct DeckSlice<'a> {
    /// The resolved deck this came from.
    deck: &'a ResolvedDeck,
    /// The seat row the plane cuts, if it cuts one.
    row: Option<&'a SeatRow>,
    /// Seats in that row, ordered by lateral position.
    seats: Vec<SeatSlice>,
    /// Overhead runs the plane cuts.
    bins: Vec<BinSlice>,
    /// Lateral centre of each aisle between seat blocks, in metres.
    aisles: Vec<f64>,
    /// Aisle centres where an unscaled standing figure clears everything.
    occupants: Vec<f64>,
}

/// Why one station was drawn and not another.
struct StationChoice {
    /// Station identifier from the scene.
    id: String,
    /// Longitudinal coordinate, in metres.
    x_m: f64,
    /// Passenger decks whose seat rows the plane cuts.
    decks_with_rows: usize,
    /// Passenger decks the scene resolved.
    passenger_decks: usize,
    /// Cargo items the plane cuts.
    cargo_items: usize,
    /// Overhead runs the plane cuts.
    overhead_runs: usize,
}

/// Everything one transverse plane cuts, resolved before anything is drawn.
struct SectionSlice<'a> {
    /// Outer mould line at the station.
    outer: Ring,
    /// Structural inner boundary, when the station carries one.
    inner: Option<Ring>,
    /// Cabin liner, when the station carries one.
    liner: Option<Ring>,
    /// Hold liner, when the station carries one.
    hold: Option<Ring>,
    /// Decks, ordered from the highest floor down.
    decks: Vec<DeckSlice<'a>>,
    /// Cargo items in the hold.
    cargo: Vec<CargoSlice>,
    /// Window apertures the plane cuts, or the nearest ones projected onto it.
    windows: Vec<&'a WindowAperture>,
    /// Whether those windows were projected rather than cut.
    ///
    /// Every exported aperture is already a nominal fallback rather than an
    /// aircraft window schedule, and the pitch that would put one exactly on a
    /// fuselage station is not known. Showing the nearest one and saying so is
    /// a labelled drawing convention; silently moving it would not be.
    windows_projected: bool,
    /// Which station was drawn, and how well it was covered.
    choice: StationChoice,
    /// Geometric findings, in the order they were detected.
    findings: Vec<String>,
}

/// Whether an envelope's longitudinal extent contains a station.
fn spans(envelope: &Box3, x_m: f64) -> bool {
    (envelope.center_x_m - x_m).abs() <= envelope.length_m * 0.5 + 1e-6
}

/// Convert an exported contour to a ring.
fn ring_of(contour: &SourcedContour) -> Ring {
    contour.points_yz_m.iter().map(|p| [p.y, p.z]).collect()
}

/// Axis-aligned bounds of a ring as `[y_min, z_min, y_max, z_max]`.
fn bounds(ring: &[[f64; 2]]) -> [f64; 4] {
    ring.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |acc, p| {
            [
                acc[0].min(p[0]),
                acc[1].min(p[1]),
                acc[2].max(p[0]),
                acc[3].max(p[1]),
            ]
        },
    )
}

/// Area enclosed by a ring, by the shoelace formula, in square metres.
fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for index in 0..ring.len() {
        let a = ring[index];
        let b = ring[(index + 1) % ring.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    (sum * 0.5).abs()
}

/// Whether a point lies inside a ring, by crossing number.
///
/// A point exactly on an edge is not classified reliably by this test, which is
/// why containment is asked with a small inward tolerance rather than on the
/// boundary itself.
fn contains_point(ring: &[[f64; 2]], point: [f64; 2]) -> bool {
    let mut inside = false;
    for index in 0..ring.len() {
        let a = ring[index];
        let b = ring[(index + 1) % ring.len()];
        if (a[1] > point[1]) == (b[1] > point[1]) {
            continue;
        }
        let denominator = b[1] - a[1];
        if denominator == 0.0 {
            continue;
        }
        let crossing = a[0] + (point[1] - a[1]) / denominator * (b[0] - a[0]);
        if point[0] < crossing {
            inside = !inside;
        }
    }
    inside
}

/// Whether every vertex of `inner` lies inside `outer`.
///
/// Vertex containment is not general polygon containment: a ring whose vertices
/// are all inside can still bulge across a concave boundary between them. The
/// contours tested against here are the elliptical liner and hold sections, and
/// the rings tested are seat, bin, occupant and ULD outlines, so the difference
/// does not arise. It is stated because a non-convex liner would need an edge
/// crossing test as well.
fn ring_within(inner: &[[f64; 2]], outer: &[[f64; 2]]) -> bool {
    inner.iter().all(|&point| contains_point(outer, point))
}

/// Whether two axis-aligned bounds overlap by more than a tolerance.
fn bounds_overlap(a: [f64; 4], b: [f64; 4], tolerance_m: f64) -> bool {
    a[0] < b[2] - tolerance_m
        && b[0] < a[2] - tolerance_m
        && a[1] < b[3] - tolerance_m
        && b[1] < a[3] - tolerance_m
}

/// Half-width of a ring at one height, on the side the sign of `toward` gives.
///
/// Returns nothing when the height is outside the ring, which is the honest
/// answer for a bin drawn above the crown or below the floor line.
fn half_width_at(ring: &[[f64; 2]], z_m: f64, toward: f64) -> Option<f64> {
    let mut crossings: Vec<f64> = Vec::new();
    for index in 0..ring.len() {
        let a = ring[index];
        let b = ring[(index + 1) % ring.len()];
        if (a[1] > z_m) == (b[1] > z_m) {
            continue;
        }
        let denominator = b[1] - a[1];
        if denominator == 0.0 {
            continue;
        }
        crossings.push(a[0] + (z_m - a[1]) / denominator * (b[0] - a[0]));
    }
    crossings
        .into_iter()
        .filter(|crossing| (*crossing >= 0.0) == (toward >= 0.0))
        .map(f64::abs)
        .fold(None, |best: Option<f64>, value| {
            Some(best.map_or(value, |current| current.max(value)))
        })
}

/// Rectangle ring from a lateral centre, a width and a vertical interval.
fn rectangle(y_center: f64, width_m: f64, z0: f64, z1: f64) -> Ring {
    let half = width_m * 0.5;
    vec![
        [y_center - half, z0],
        [y_center + half, z0],
        [y_center + half, z1],
        [y_center - half, z1],
    ]
}

/// The scale figure's silhouette, standing on a floor at an aisle centre.
///
/// Half-widths are fractions of the shoulder half-width and heights fractions
/// of the standing height, so the figure is one shape at one size rather than
/// something stretched to whatever gap it is put in. The head is drawn as a
/// circle on top of this outline; the outline stops at the neck.
const OCCUPANT_PROFILE: [[f64; 2]; 8] = [
    [0.55, 0.000],
    [0.66, 0.440],
    [0.79, 0.520],
    [0.96, 0.640],
    [0.87, 0.760],
    [1.00, 0.800],
    [0.49, 0.855],
    [0.32, 0.890],
];

/// The scale figure's silhouette, standing on a floor at an aisle centre.
fn occupant_ring(y_m: f64, floor_z_m: f64) -> Ring {
    let (w, h) = (OCCUPANT_HALF_WIDTH_M, OCCUPANT_HEIGHT_M);
    let mut ring: Ring = OCCUPANT_PROFILE
        .iter()
        .map(|point| [y_m - w * point[0], floor_z_m + h * point[1]])
        .collect();
    ring.extend(
        OCCUPANT_PROFILE
            .iter()
            .rev()
            .map(|point| [y_m + w * point[0], floor_z_m + h * point[1]]),
    );
    ring
}

/// Score a candidate station so the drawn plane is the most informative one.
///
/// The ordering is deliberate and is the whole reason this figure exists: a
/// transverse section has exactly one longitudinal coordinate, so a station
/// that cuts seat rows on every passenger deck outranks one that merely cuts
/// more seats, and no quantity is borrowed from a neighbouring station to make
/// the picture look fuller.
fn station_score(scene: &CabinScene, x_m: f64) -> (usize, usize, usize, usize) {
    let decks_with_rows = scene
        .decks
        .iter()
        .filter(|deck| deck.passenger)
        .filter(|deck| {
            scene
                .seat_rows
                .iter()
                .any(|row| row.deck_id == deck.id && spans(&row.envelope, x_m))
        })
        .count();
    let cargo = scene
        .cargo
        .items
        .iter()
        .filter(|item| spans(&item.envelope, x_m))
        .count();
    let overhead = scene
        .overhead
        .runs
        .iter()
        .filter(|run| spans(&run.envelope, x_m))
        .count();
    let seats = scene
        .seats
        .iter()
        .filter(|seat| {
            scene
                .seat_rows
                .iter()
                .any(|row| row.id == seat.row_id && spans(&row.envelope, x_m))
        })
        .count();
    (decks_with_rows, cargo.min(1), overhead, seats)
}

/// Pick the station to draw, preferring one that carries a hold contour too.
fn select_station(scene: &CabinScene) -> Option<&SectionStation> {
    scene
        .stations
        .iter()
        .filter(|station| station.liner.is_some() || station.inner.is_some())
        .max_by_key(|station| {
            let (decks, cargo, overhead, seats) = station_score(scene, station.x_m);
            (
                decks,
                cargo,
                usize::from(station.hold.is_some()),
                overhead,
                seats,
            )
        })
}

/// Lateral centres of the aisles in a row, derived from its seat positions.
///
/// The row records how many seats each block holds and how wide the aisle is;
/// the gap between the last seat of one block and the first of the next is the
/// aisle, and its centre is where a standing figure would be.
fn aisle_centers(row: &SeatRow, seats: &[SeatSlice]) -> Vec<f64> {
    let mut centers = Vec::new();
    let mut index = 0usize;
    for block in row.blocks.iter().take(row.blocks.len().saturating_sub(1)) {
        let end = index + (*block).max(0) as usize;
        if end == 0 || end >= seats.len() {
            break;
        }
        let left = seats[end - 1].y_m + seats[end - 1].width_m * 0.5;
        let right = seats[end].y_m - seats[end].width_m * 0.5;
        if right > left {
            centers.push(0.5 * (left + right));
        }
        index = end;
    }
    centers
}
