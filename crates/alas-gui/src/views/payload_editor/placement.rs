// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Placement rules of the payload editor, on the declared items
//! (`cabin.payload_items`): clicks and drags snap to 0.1 m stations. A zone
//! lives on the main deck between the cabin bulkheads; a point mass may be put
//! anywhere in the airframe envelope (see [`super::envelope`]: fuselage, belly,
//! under the wing, nose), and a click outside every valid volume is refused.

use alas_config::{
    FloorZoneConfig, MassCategory, PayloadItemsConfig, PointMassConfig, ZoneDeck, ZoneKind,
};

use super::side_view::SideView;

/// Station snap, m.
pub(super) const SNAP_M: f64 = 0.1;
/// Length of a newly placed zone, m.
const NEW_ZONE_LENGTH_M: f64 = 3.0;
/// How far outside the skin a front-view click may land and still be placed on
/// it, m.
const EDGE_TOLERANCE_M: f64 = 0.6;
/// Mass of a newly placed point mass, kg.
const NEW_POINT_MASS_KG: f64 = 100.0;

/// Why a placement was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The click is outside every cabin and hold volume.
    NoValidStation,
    /// The new zone would overlap one on the same deck.
    ZoneOverlap,
    /// The click is along the fuselage but outside the cabin length.
    ZoneOutsideCabin,
}

impl Refusal {
    pub(super) const fn text(self) -> &'static str {
        match self {
            Self::NoValidStation => "No Valid Station Here",
            Self::ZoneOverlap => "Zones Overlap",
            Self::ZoneOutsideCabin => "Zones Go Inside The Cabin Length",
        }
    }
}

/// A volume an item lives in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Region {
    /// Forward and aft station limits, m.
    pub x: [f64; 2],
    pub hold: bool,
}

/// Snap a station to the 0.1 m grid.
pub(super) fn snap(x: f64) -> f64 {
    (x / SNAP_M).round() * SNAP_M
}

impl SideView {
    /// The region containing the point `(x, z)`, if it is in a valid volume.
    pub(super) fn region_at(&self, x: f64, z: f64) -> Option<Region> {
        if x >= self.cabin_x[0]
            && x <= self.cabin_x[1]
            && z >= self.floor_z(x)
            && z <= self.ceiling_z(x)
        {
            return Some(Region {
                x: self.cabin_x,
                hold: false,
            });
        }
        if !self.has_hold || z < self.hold_floor_z(x) || z > self.hold_ceiling_z(x) {
            return None;
        }
        if self.declared_holds {
            return self
                .hold_spans
                .iter()
                .find(|span| x >= span[0] && x <= span[1])
                .map(|span| Region {
                    x: *span,
                    hold: true,
                });
        }
        let [lo, hi] = self.cabin_x;
        match self.hold_gap_x {
            Some([gap_from, gap_to]) if x > gap_from && x < gap_to => None,
            Some([gap_from, _]) if x <= gap_from && x >= lo => Some(Region {
                x: [lo, gap_from],
                hold: true,
            }),
            Some([_, gap_to]) if x >= gap_to && x <= hi => Some(Region {
                x: [gap_to, hi],
                hold: true,
            }),
            None if x >= lo && x <= hi => Some(Region {
                x: [lo, hi],
                hold: true,
            }),
            _ => None,
        }
    }

    /// The region a declared item with its centre at `(x, z)` lives in.
    pub(super) fn region_of(&self, x: f64, z: f64) -> Option<Region> {
        self.region_at(x, z)
    }
}

/// The id a new point mass takes: `mass-1`, `mass-2`, ... not in use.
fn new_point_mass_id(items: &PayloadItemsConfig) -> String {
    (1..)
        .map(|n| format!("mass-{n}"))
        .find(|id| items.point_masses.iter().all(|other| &other.id != id))
        .unwrap_or_default()
}

/// A zone name not in use.
fn new_zone_name(items: &PayloadItemsConfig) -> String {
    (1..)
        .map(|n| format!("Zone {n}"))
        .find(|name| items.zones.iter().all(|other| &other.name != name))
        .unwrap_or_default()
}

/// Place a zone starting at the clicked station `x`, `z` being the click's
/// height. A zone lives on the main deck: a click in a hold or outside the
/// cabin is refused. Its length shortens against the cabin's aft end.
///
/// # Errors
///
/// [`Refusal::NoValidStation`] for a click outside the main cabin.
pub(super) fn place_zone(
    items: &mut PayloadItemsConfig,
    view: &SideView,
    x: f64,
    z: f64,
) -> Result<usize, Refusal> {
    // A click anywhere in the fuselage along the cabin puts the zone on the
    // main deck there: the deck is a thin strip in the side view, so asking
    // for the strip itself refuses nearly every click.
    let in_cabin = x >= view.cabin_x[0] && x <= view.cabin_x[1];
    let region = view
        .region_at(x, z)
        .filter(|region| !region.hold)
        .or_else(|| {
            (in_cabin && view.in_envelope(x, 0.0, z)).then_some(Region {
                x: view.cabin_x,
                hold: false,
            })
        });
    let Some(region) = region else {
        return Err(if in_cabin {
            Refusal::NoValidStation
        } else {
            Refusal::ZoneOutsideCabin
        });
    };
    let start = snap(x).clamp(region.x[0], region.x[1]);
    let end = (start + NEW_ZONE_LENGTH_M).min(region.x[1]);
    if end - start < SNAP_M {
        return Err(Refusal::NoValidStation);
    }
    if items
        .zones
        .iter()
        .any(|other| other.deck == ZoneDeck::Main && other.x_start_m < end && start < other.x_end_m)
    {
        return Err(Refusal::ZoneOverlap);
    }
    let name = new_zone_name(items);
    items.zones.push(FloorZoneConfig {
        name,
        kind: ZoneKind::Custom,
        deck: ZoneDeck::Main,
        x_start_m: start,
        x_end_m: end,
        width_m: None,
        y_center_m: 0.0,
        furnishing_kg_per_m2: 0.0,
        total_mass_kg: None,
        occupants: 0,
        carve_seat_floor: true,
        movable: false,
        x_range_m: None,
    });
    Ok(items.zones.len() - 1)
}

/// Place a point mass at the side-view click `(x, z)`: anywhere in the
/// airframe envelope. Its lateral position is the one that puts the click in
/// the envelope (the centreline in the fuselage, the span station under a
/// wing).
///
/// # Errors
///
/// [`Refusal::NoValidStation`] for a click outside the envelope.
pub(super) fn place_point_mass(
    items: &mut PayloadItemsConfig,
    view: &SideView,
    x: f64,
    z: f64,
) -> Result<usize, Refusal> {
    let station = snap(x);
    let y = view
        .lateral_for(station, z)
        .ok_or(Refusal::NoValidStation)?;
    Ok(push_point_mass(items, station, y, z))
}

/// [`place_point_mass`] for a front-view click at section `x`, lateral `y`
/// and height `z`.
///
/// # Errors
///
/// [`Refusal::NoValidStation`] for a click outside the envelope.
pub(super) fn place_point_mass_at(
    items: &mut PayloadItemsConfig,
    view: &SideView,
    x: f64,
    y: f64,
    z: f64,
) -> Result<usize, Refusal> {
    let (station, mut lateral) = (snap(x), snap(y));
    let mut height = z;
    if !view.in_envelope(station, lateral, height) {
        // A click on or just outside the skin lands on it: the section is
        // small on screen, so the edge is hard to hit exactly. A click well
        // outside is still refused.
        let (pulled_y, pulled_z) = view.clamp_front(station, lateral, height);
        let near = (pulled_y - lateral).hypot(pulled_z - height) <= EDGE_TOLERANCE_M;
        if !near || !view.in_envelope(station, pulled_y, pulled_z) {
            return Err(Refusal::NoValidStation);
        }
        (lateral, height) = (pulled_y, pulled_z);
    }
    Ok(push_point_mass(items, station, lateral, height))
}

fn push_point_mass(items: &mut PayloadItemsConfig, x: f64, y: f64, z: f64) -> usize {
    let id = new_point_mass_id(items);
    items.point_masses.push(PointMassConfig {
        name: id.clone(),
        id,
        x_m: x,
        y_m: y,
        z_m: z,
        mass_kg: NEW_POINT_MASS_KG,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable: false,
        drag_area_m2: None,
        movable: false,
        x_range_m: None,
    });
    items.point_masses.len() - 1
}

/// Move a point mass to the side-view drag position `(x, z)`, held in the
/// airframe envelope. Returns whether it moved.
pub(super) fn drag_point_mass(item: &mut PointMassConfig, view: &SideView, x: f64, z: f64) -> bool {
    let (station, lateral, height) = view.clamp_side(snap(x), item.y_m, z);
    let moved = (station, lateral, height) != (item.x_m, item.y_m, item.z_m);
    (item.x_m, item.y_m, item.z_m) = (station, lateral, height);
    moved
}

/// Move a point mass to the front-view drag position `(y, z)` at its own
/// station, held in the envelope. Returns whether it moved.
pub(super) fn drag_point_mass_front(
    item: &mut PointMassConfig,
    view: &SideView,
    y: f64,
    z: f64,
) -> bool {
    let (lateral, height) = view.clamp_front(item.x_m, snap(y), z);
    let moved = (lateral, height) != (item.y_m, item.z_m);
    (item.y_m, item.z_m) = (lateral, height);
    moved
}

/// Set or clear the Movable flag. A movable item needs the stations it may
/// take: the region it sits in, else the run of stations at its height that
/// stay in the envelope.
pub(super) fn set_point_mass_movable(item: &mut PointMassConfig, view: &SideView, movable: bool) {
    item.movable = movable;
    item.x_range_m = if movable {
        Some(
            view.region_of(item.x_m, item.z_m)
                .map_or_else(|| view.x_run_at(item.x_m, item.z_m), |region| region.x),
        )
    } else {
        None
    };
}

/// [`set_point_mass_movable`] for a zone, whose range is the cabin the whole
/// zone may slide in.
pub(super) fn set_zone_movable(zone: &mut FloorZoneConfig, view: &SideView, movable: bool) {
    zone.movable = movable;
    zone.x_range_m = movable.then_some(view.cabin_x);
}

/// Whether a zone may be flagged Movable: only occupants move with the
/// balance, so a furnished zone stays where it is.
pub(super) fn zone_can_move(zone: &FloorZoneConfig) -> bool {
    zone.kind != ZoneKind::Seats
        && zone.furnishing_kg_per_m2 <= 0.0
        && zone.total_mass_kg.is_none_or(|mass| mass <= 0.0)
}

/// Whether a point mass may be flagged Movable: payload only, equipment is
/// operating empty mass at its own station.
pub(super) fn point_mass_can_move(item: &PointMassConfig) -> bool {
    item.category == MassCategory::Payload
}

/// Hold a table edit to the rules of placement: against `before` (the items
/// before the edit, same order), a point mass that moved stays in the airframe
/// envelope, its station snapped to the 0.1 m grid, and a zone stays between
/// the bulkheads of the cabin.
pub(super) fn constrain(
    items: &mut PayloadItemsConfig,
    before: &PayloadItemsConfig,
    view: &SideView,
) {
    if items.point_masses.len() == before.point_masses.len() {
        for (item, old) in items.point_masses.iter_mut().zip(&before.point_masses) {
            if (item.x_m, item.y_m, item.z_m) == (old.x_m, old.y_m, old.z_m) {
                continue;
            }
            let moved_in_x = item.x_m != old.x_m;
            let x = if moved_in_x { snap(item.x_m) } else { item.x_m };
            (item.x_m, item.y_m, item.z_m) = view.clamp_side(x, item.y_m, item.z_m);
        }
    }
    let [low, high] = view.cabin_x;
    for zone in &mut items.zones {
        zone.x_start_m = zone.x_start_m.clamp(low, high - SNAP_M);
        zone.x_end_m = zone.x_end_m.clamp(zone.x_start_m + SNAP_M, high);
    }
}

/// Restore the rules an edit may have broken, so the declared items stay
/// valid: a flag that no longer applies is cleared, and the balance range of a
/// movable item is held inside the region it sits in (the cabin for a zone)
/// and made to contain the item.
///
/// Only the items that differ from `before` (the items before this frame's
/// edits; every item when `None`) are touched: the region comes from the
/// preview geometry, an estimate, so a case's own declared ranges are never
/// rewritten by drawing the page, wherever they lie. A declared range that
/// leaves its region is reported by [`ranges_leaving_regions`] instead.
pub(super) fn normalize(
    items: &mut PayloadItemsConfig,
    before: Option<&PayloadItemsConfig>,
    view: Option<&SideView>,
) {
    // Items are matched by identity (zone name, point-mass id), not by list
    // position: a removed row must not shift the comparison onto its neighbour.
    for zone in items.zones.iter_mut() {
        if before.and_then(|old| old.zones.iter().find(|o| o.name == zone.name)) == Some(&*zone) {
            continue;
        }
        if zone.movable && !zone_can_move(zone) {
            zone.movable = false;
            zone.x_range_m = None;
        }
        if zone.movable {
            zone.x_range_m = Some(fitted(
                zone.x_range_m,
                view.map(|view| view.cabin_x),
                zone.x_start_m,
                zone.x_end_m,
            ));
        }
    }
    for item in items.point_masses.iter_mut() {
        if before.and_then(|old| old.point_masses.iter().find(|o| o.id == item.id)) == Some(&*item)
        {
            continue;
        }
        if item.movable && !point_mass_can_move(item) {
            item.movable = false;
            item.x_range_m = None;
        }
        if item.movable {
            let region = view
                .and_then(|view| view.region_of(item.x_m, item.z_m))
                .map(|region| region.x);
            item.x_range_m = Some(fitted(item.x_range_m, region, item.x_m, item.x_m));
        }
    }
}

/// The movable items whose declared balance range reaches outside the region
/// they sit in (across the wing-box gap or a compartment end): the case says
/// so, the editor does not rewrite it, and the status pill names the first.
pub(super) fn ranges_leaving_regions(
    items: &PayloadItemsConfig,
    view: &SideView,
) -> Option<String> {
    let leaves = |range: [f64; 2], region: [f64; 2]| {
        range[0] < region[0] - 1e-9 || range[1] > region[1] + 1e-9
    };
    for item in items.point_masses.iter().filter(|item| item.movable) {
        let region = view.region_of(item.x_m, item.z_m).map(|region| region.x);
        if let (Some(range), Some(region)) = (item.x_range_m, region) {
            if leaves(range, region) {
                return Some(item.name.clone());
            }
        }
    }
    items
        .zones
        .iter()
        .filter(|zone| zone.movable)
        .find(|zone| {
            zone.x_range_m
                .is_some_and(|range| leaves(range, view.cabin_x))
        })
        .map(|zone| zone.name.clone())
}

/// The balance range to keep: the declared one (else the region), inside the
/// region when there is one, and containing `[from, to]`.
fn fitted(declared: Option<[f64; 2]>, region: Option<[f64; 2]>, from: f64, to: f64) -> [f64; 2] {
    let range = match (declared, region) {
        (Some([a, b]), Some([low, high])) => [a.max(low).min(high), b.min(high).max(low)],
        (Some(range), None) => range,
        (None, Some(region)) => region,
        (None, None) => [from, to],
    };
    [range[0].min(from), range[1].max(to)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::payload_editor::side_view::test_body;

    #[test]
    fn normalizing_clears_flags_the_rules_refuse_and_keeps_the_items_valid() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        let mass = place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
        let zone = place_zone(&mut items, &view, 10.0, 1.0).unwrap();
        set_point_mass_movable(&mut items.point_masses[mass], &view, true);
        set_zone_movable(&mut items.zones[zone], &view, true);
        assert!(items.validate().is_ok());
        items.point_masses[mass].category = MassCategory::Equipment;
        items.zones[zone].furnishing_kg_per_m2 = 20.0;
        assert!(items.validate().is_err());
        normalize(&mut items, None, Some(&view));
        assert!(items.validate().is_ok(), "{:?}", items.validate());
        assert!(!items.point_masses[mass].movable && !items.zones[zone].movable);
        // A table edit outside the region is clamped back, and the balance
        // range stays the region (never widened across the wing-box gap).
        items.point_masses[mass].category = MassCategory::Payload;
        set_point_mass_movable(&mut items.point_masses[mass], &view, true);
        let before = items.clone();
        items.point_masses[mass].x_m = 40.0;
        items.point_masses[mass].z_m = 3.0;
        constrain(&mut items, &before, &view);
        normalize(&mut items, None, Some(&view));
        // Anywhere in the envelope: held to the body's end and crown, and the
        // balance range grows to contain the item.
        assert_eq!(items.point_masses[mass].x_m, 40.0);
        assert!((items.point_masses[mass].z_m - 2.0).abs() < 1e-9);
        assert_eq!(items.point_masses[mass].x_range_m, Some([22.0, 40.0]));
        assert!(items.validate().is_ok());
    }

    #[test]
    fn a_zone_edit_stays_between_the_bulkheads_and_a_new_zone_may_not_overlap() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        let zone = place_zone(&mut items, &view, 10.0, 1.0).unwrap();
        assert_eq!(
            place_zone(&mut items, &view, 11.0, 1.0),
            Err(Refusal::ZoneOverlap)
        );
        let before = items.clone();
        items.zones[zone].x_start_m = -4.0;
        items.zones[zone].x_end_m = 90.0;
        constrain(&mut items, &before, &view);
        assert_eq!(
            (items.zones[zone].x_start_m, items.zones[zone].x_end_m),
            (5.0, 36.0)
        );
        let mut seats = items.zones[zone].clone();
        seats.kind = ZoneKind::Seats;
        assert!(!zone_can_move(&seats), "a seats zone is fixed");
    }

    #[test]
    fn stations_snap_to_a_tenth_of_a_metre() {
        assert!((snap(12.34) - 12.3).abs() < 1e-9);
        assert!((snap(12.36) - 12.4).abs() < 1e-9);
    }

    #[test]
    fn a_click_above_the_floor_is_the_cabin_and_below_it_the_hold() {
        let view = test_body();
        assert_eq!(view.region_at(10.0, 1.0).map(|r| r.hold), Some(false));
        assert_eq!(view.region_at(30.0, -1.0).map(|r| r.hold), Some(true));
        // Between the hold ceiling and the floor, over the skin, past the end:
        // no region (a point mass may still sit there, see the envelope).
        assert!(view.region_at(30.0, -0.1).is_none());
        assert!(view.region_at(30.0, 3.0).is_none());
        assert!(view.region_at(2.0, 1.0).is_none());
        // The wing box gap holds no cargo.
        assert!(view.region_at(20.0, -1.0).is_none());
    }

    #[test]
    fn the_hold_is_two_regions_either_side_of_the_wing_box() {
        let view = test_body();
        assert_eq!(view.region_at(12.0, -1.0).unwrap().x, [5.0, 18.0]);
        assert_eq!(view.region_at(30.0, -1.0).unwrap().x, [22.0, 36.0]);
    }

    #[test]
    fn placing_a_point_mass_snaps_and_refuses_outside_a_volume() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        let index = place_point_mass(&mut items, &view, 12.34, 0.8).unwrap();
        assert_eq!(items.point_masses[index].x_m, 12.3);
        assert_eq!(items.point_masses[index].id, "mass-1");
        let refused = place_point_mass(&mut items, &view, 12.3, 5.0);
        assert_eq!(refused, Err(Refusal::NoValidStation));
        assert_eq!(items.point_masses.len(), 1);
        let second = place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
        assert_eq!(items.point_masses[second].id, "mass-2");
        assert!(items.validate().is_ok());
    }

    #[test]
    fn a_zone_lives_on_the_main_deck_and_stops_at_the_cabin_end() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        assert_eq!(
            place_zone(&mut items, &view, 3.0, 1.0),
            Err(Refusal::ZoneOutsideCabin),
            "not forward of the cabin"
        );
        // A click in the hold, below the thin deck strip, still puts the zone
        // on the main deck above it.
        let mut scratch = PayloadItemsConfig::default();
        let in_hold = place_zone(&mut scratch, &view, 30.0, -1.0).unwrap();
        assert_eq!(scratch.zones[in_hold].deck, ZoneDeck::Main);
        let index = place_zone(&mut items, &view, 35.0, 1.0).unwrap();
        assert_eq!(
            items.zones[index].x_end_m, 36.0,
            "shortened at the bulkhead"
        );
        let second = place_zone(&mut items, &view, 10.0, 1.0).unwrap();
        assert_eq!(items.zones[second].length_m(), 3.0);
        assert_eq!(items.zones[second].name, "Zone 2");
    }

    #[test]
    fn a_dragged_mass_may_go_anywhere_in_the_envelope_and_no_further() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        let index = place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
        let item = &mut items.point_masses[index];
        // Forward through the wing box gap and up into the cabin: free.
        assert!(drag_point_mass(item, &view, 10.0, 1.0));
        assert_eq!((item.x_m, item.z_m), (10.0, 1.0));
        // Past the tail and above the crown: held to the body.
        drag_point_mass(item, &view, 80.0, 9.0);
        assert_eq!((item.x_m, item.z_m), (40.0, 2.0));
        // Front view: only the lateral position and height move.
        assert!(drag_point_mass_front(item, &view, 0.0, -3.0));
        assert_eq!((item.x_m, item.y_m, item.z_m), (40.0, 0.0, -3.0));
    }

    #[test]
    fn movable_takes_the_region_range_and_clearing_it_drops_the_range() {
        let view = test_body();
        let mut items = PayloadItemsConfig::default();
        let index = place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
        let item = &mut items.point_masses[index];
        set_point_mass_movable(item, &view, true);
        assert_eq!(item.x_range_m, Some([22.0, 36.0]));
        set_point_mass_movable(item, &view, false);
        assert_eq!(item.x_range_m, None);
        assert!(!item.movable);
    }
}
