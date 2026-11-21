// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Zone rules of the payload editor that do not depend on the mouse: sliding
//! a zone without ever overlapping a neighbour, the presets a zone kind fills,
//! and the mass a zone's densities add up to.

use alas_config::{FloorZoneConfig, ZoneKind};

use super::placement::{snap, SNAP_M};
use super::side_view::SideView;

/// Tolerance when comparing stations, m.
const EPS: f64 = 1.0e-9;

/// The free stretches of `region` for the zone `index`: what the other zones
/// of its deck leave.
fn gaps(zones: &[FloorZoneConfig], index: usize, region: [f64; 2]) -> Vec<[f64; 2]> {
    let Some(zone) = zones.get(index) else {
        return Vec::new();
    };
    let mut taken: Vec<[f64; 2]> = zones
        .iter()
        .enumerate()
        .filter(|(other, candidate)| *other != index && candidate.deck == zone.deck)
        .map(|(_, candidate)| [candidate.x_start_m, candidate.x_end_m])
        .collect();
    taken.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut free = Vec::new();
    let mut cursor = region[0];
    for [start, end] in taken {
        if start > cursor {
            free.push([cursor, start.min(region[1])]);
        }
        cursor = cursor.max(end);
    }
    if cursor < region[1] {
        free.push([cursor, region[1]]);
    }
    free.retain(|gap| gap[1] - gap[0] > EPS);
    free
}

/// `desired` held to a gap for a zone of `length`, on the 0.1 m grid unless
/// the grid would leave the gap (a neighbour off the grid).
fn within(gap: [f64; 2], length: f64, desired: f64) -> f64 {
    let held = desired.clamp(gap[0], (gap[1] - length).max(gap[0]));
    let snapped = snap(held);
    if snapped >= gap[0] - EPS && snapped + length <= gap[1] + EPS {
        snapped
    } else {
        held
    }
}

/// Where the zone `index` stands for a pointer wanting its start at
/// `desired`, and where it would land on release.
///
/// The zone is held in the gap it stands in, so it stops against the
/// neighbour (`held`). Once the pointer is past the neighbour's midpoint the
/// nearest start over all gaps that fit the zone lies in another gap: that is
/// where release puts it (`landing`), and the editor draws it as a ghost.
/// The two are equal while the zone is not pushed past a neighbour's middle.
pub(super) fn slide(
    zones: &[FloorZoneConfig],
    index: usize,
    region: [f64; 2],
    desired: f64,
) -> Option<(f64, f64)> {
    let zone = zones.get(index)?;
    let length = zone.length_m();
    let free = gaps(zones, index, region);
    let fits: Vec<[f64; 2]> = free
        .iter()
        .copied()
        .filter(|gap| gap[1] - gap[0] >= length - EPS)
        .collect();
    let here = fits
        .iter()
        .copied()
        .find(|gap| zone.x_start_m >= gap[0] - EPS && zone.x_end_m <= gap[1] + EPS)?;
    let held = within(here, length, desired);
    let landing = fits
        .iter()
        .map(|gap| within(*gap, length, desired))
        .min_by(|a, b| (a - desired).abs().total_cmp(&(b - desired).abs()))
        .unwrap_or(held);
    Some((held, landing))
}

/// Move the zone to start at `start`, keeping its length.
pub(super) fn set_start(zone: &mut FloorZoneConfig, start: f64) {
    let length = zone.length_m();
    zone.x_start_m = start;
    zone.x_end_m = start + length;
}

/// What picking a kind fills in. These are planning defaults to start from,
/// not data of a certified interior: every value stays editable and shows a
/// note once it differs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Preset {
    /// Furnishing mass per floor area, kg/m2.
    pub density: f64,
    /// Occupants the zone starts with.
    pub occupants: i64,
    /// Most occupants the zone takes.
    pub occupant_limit: i64,
    /// Approximate length, m.
    pub length: f64,
}

/// The preset of `kind`; `None` for a custom zone, which has none.
pub(super) fn preset(kind: ZoneKind) -> Option<Preset> {
    let (density, occupants, occupant_limit, length) = match kind {
        ZoneKind::Seats => (0.0, 6, 12, 3.0),
        ZoneKind::Office => (40.0, 4, 8, 4.0),
        ZoneKind::Bedroom => (60.0, 2, 4, 3.0),
        ZoneKind::Ops => (90.0, 4, 8, 5.0),
        ZoneKind::Galley => (120.0, 0, 2, 2.0),
        ZoneKind::Lav => (80.0, 0, 1, 1.5),
        ZoneKind::Cargo => (0.0, 0, 0, 3.0),
        ZoneKind::Custom => return None,
    };
    Some(Preset {
        density,
        occupants,
        occupant_limit,
        length,
    })
}

/// Which of the preset's values the zone no longer has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Modified {
    pub density: bool,
    pub occupants: bool,
    pub length: bool,
}

/// How the zone differs from its kind's preset.
pub(super) fn modified(zone: &FloorZoneConfig) -> Modified {
    let Some(preset) = preset(zone.kind) else {
        return Modified::default();
    };
    Modified {
        density: (zone.furnishing_kg_per_m2 - preset.density).abs() > 1.0e-9
            || zone.total_mass_kg.is_some(),
        occupants: zone.occupants != preset.occupants,
        length: (zone.length_m() - preset.length).abs() > SNAP_M / 2.0,
    }
}

/// Give the zone `index` the preset of `kind`: density, occupants and length
/// (the length shortened to the free room after its start). A zone that was
/// Movable stays so only when the kind leaves it free of furnishing.
pub(super) fn apply_preset(
    zones: &mut [FloorZoneConfig],
    index: usize,
    kind: ZoneKind,
    region: [f64; 2],
) {
    let Some(preset) = preset(kind) else {
        if let Some(zone) = zones.get_mut(index) {
            zone.kind = kind;
        }
        return;
    };
    let room = gaps(zones, index, region)
        .into_iter()
        .find(|gap| {
            zones
                .get(index)
                .is_some_and(|zone| zone.x_start_m >= gap[0] - EPS && zone.x_start_m < gap[1])
        })
        .map_or(preset.length, |gap| {
            gap[1] - zones.get(index).map_or(gap[0], |zone| zone.x_start_m)
        });
    let Some(zone) = zones.get_mut(index) else {
        return;
    };
    zone.kind = kind;
    zone.furnishing_kg_per_m2 = preset.density;
    zone.total_mass_kg = None;
    zone.occupants = preset.occupants;
    zone.x_end_m = zone.x_start_m + preset.length.min(room).max(SNAP_M);
    if zone.movable && preset.density > 0.0 {
        zone.movable = false;
        zone.x_range_m = None;
    }
}

/// Floor area of a zone on the main deck, m2: its width (the declared one,
/// held to the usable floor width) integrated over its stations.
pub(super) fn area_m2(view: &SideView, zone: &FloorZoneConfig) -> f64 {
    const SLICES: usize = 24;
    let dx = zone.length_m() / SLICES as f64;
    (0..SLICES)
        .map(|n| {
            let x = zone.x_start_m + (n as f64 + 0.5) * dx;
            let usable = view.at(&view.main_width, x);
            zone.width_m.map_or(usable, |w| w.min(usable)) * dx
        })
        .sum()
}

/// Furnishing mass of a zone, kg: the declared total, else density times
/// floor area (the rule the payload builder applies).
pub(super) fn furnishing_kg(view: &SideView, zone: &FloorZoneConfig) -> f64 {
    zone.total_mass_kg
        .unwrap_or_else(|| zone.furnishing_kg_per_m2 * area_m2(view, zone))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{PayloadItemsConfig, ZoneDeck};

    fn zone(name: &str, start: f64, end: f64) -> FloorZoneConfig {
        let mut items = PayloadItemsConfig::default();
        items.zones.push(FloorZoneConfig {
            name: name.to_owned(),
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
        items.zones.remove(0)
    }

    const REGION: [f64; 2] = [5.0, 36.0];

    #[test]
    fn a_dragged_zone_stops_against_its_neighbour() {
        let zones = [zone("A", 10.0, 13.0), zone("B", 20.0, 22.0)];
        // Pointer wants A at 19: B starts at 20, A is 3 m long -> stops at 17.
        let (held, landing) = slide(&zones, 0, REGION, 19.0).unwrap();
        assert!((held - 17.0).abs() < 1e-9, "{held}");
        assert!((landing - 17.0).abs() < 1e-9, "{landing}");
    }

    #[test]
    fn past_the_neighbours_midpoint_the_landing_jumps_to_the_next_gap() {
        let zones = [zone("A", 10.0, 13.0), zone("B", 20.0, 22.0)];
        // Wanting start 20.6: the zone's start is past B's middle (21).
        let (held, landing) = slide(&zones, 0, REGION, 21.4).unwrap();
        assert!((held - 17.0).abs() < 1e-9, "stays against B: {held}");
        assert!((landing - 22.0).abs() < 1e-9, "lands beyond B: {landing}");
        // Never overlapping B at either place.
        for start in [held, landing] {
            assert!(start + 3.0 <= 20.0 + 1e-9 || start >= 22.0 - 1e-9);
        }
    }

    #[test]
    fn a_gap_too_short_for_the_zone_is_never_a_landing() {
        let zones = [
            zone("A", 5.0, 8.0),
            zone("B", 10.0, 12.0),
            zone("C", 13.0, 20.0),
        ];
        // The 1 m gap between B and C (12 to 13) cannot take A (3 m): the
        // nearest landing is the one ahead of B.
        let (_, landing) = slide(&zones, 0, REGION, 12.0).unwrap();
        assert!((landing - 7.0).abs() < 1e-9, "{landing}");
    }

    #[test]
    fn a_kind_fills_density_occupants_and_length_and_edits_are_flagged() {
        let mut zones = [zone("A", 10.0, 11.0), zone("B", 12.0, 30.0)];
        apply_preset(&mut zones, 0, ZoneKind::Office, REGION);
        let office = preset(ZoneKind::Office).unwrap();
        assert_eq!(zones[0].furnishing_kg_per_m2, office.density);
        assert_eq!(zones[0].occupants, office.occupants);
        // The 4 m office is cut to the 2 m before B.
        assert!((zones[0].length_m() - 2.0).abs() < 1e-9);
        assert!(modified(&zones[0]).length && !modified(&zones[0]).density);
        zones[0].furnishing_kg_per_m2 += 5.0;
        assert!(modified(&zones[0]).density);
        assert_eq!(modified(&zone("C", 1.0, 2.0)), Modified::default());
    }

    #[test]
    fn furnishing_is_the_declared_total_else_density_times_area() {
        let mut view = crate::views::payload_editor::side_view::test_body();
        view.main_width = vec![3.0; view.stations.len()];
        let mut z = zone("A", 10.0, 14.0);
        z.furnishing_kg_per_m2 = 50.0;
        assert!((furnishing_kg(&view, &z) - 600.0).abs() < 1e-6);
        z.width_m = Some(1.0);
        assert!((furnishing_kg(&view, &z) - 200.0).abs() < 1e-6);
        z.total_mass_kg = Some(75.0);
        assert!((furnishing_kg(&view, &z) - 75.0).abs() < 1e-9);
    }
}
