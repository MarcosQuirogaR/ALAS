// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the freighter items of a civil main-deck freighter sit.
//!
//! The masses are priced elsewhere and are not changed here: the fuselage
//! carries Torenbeek's freighter increment (*Synthesis of Subsonic Airplane
//! Design*, 1982, section 8.4.1, p. 282, "an extra 10% for freighter
//! aircraft"; [`super::military_cargo::MAIN_DECK_CARGO_DOOR_FUSELAGE_FRACTION`])
//! and the systems group carries the pallet handling system. Both were placed
//! with their groups, at the fuselage and systems stations. The hardware is
//! not there: the increment is the freight floor and the cargo doors, and the
//! handling system lies on the main deck.
//!
//! Torenbeek's Appendix D prices the parts of the increment one by one, and
//! that subdivision places it:
//!
//! - nose loading door: 15 lb/ft^2 (73.236 kg/m^2) of frontal area (section
//!   D-3, p. 462), on the nose loft ahead of the door break;
//! - main-deck side cargo door: Table D-2, p. 461, pressurized cargo hold
//!   door, filling `48.8 sqrt(dp) b h` kg and surround `35.7 sqrt(A)` kg on
//!   the front fuselage or `50.6 sqrt(A)` kg on the rear fuselage (`dp` in
//!   kg/cm^2, `b`, `h` in m, `A` in m^2), at the door centreline;
//! - freight floor: the rest of the increment, at the middle of the main deck
//!   (section D-4.1, p. 462: a containerized freight floor weighs
//!   `10.867 S^1.045` kg against a passenger floor's 5 to 10 kg/m^2).
//!
//! The doors come out of the increment and never add to it, so the fuselage
//! mass is the same to the last digit; only its moment moves. A door with no
//! declared station stays in the floor remainder.

use alas_config::{AlasConfig, CabinConfig, GeometryConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;

use crate::breakdown::{MassBreakdown, MassCoordinates};

/// Torenbeek 1982 section D-3: nose loading door, kg per m^2 of frontal area.
const NOSE_DOOR_KG_PER_M2: f64 = 73.236;
/// Torenbeek 1982 Table D-2: pressurized cargo hold door filling coefficient.
const CARGO_DOOR_FILLING: f64 = 48.8;
/// Torenbeek 1982 Table D-2: cargo hold door surround, front fuselage.
const CARGO_DOOR_SURROUND_FRONT: f64 = 35.7;
/// Torenbeek 1982 Table D-2: cargo hold door surround, rear fuselage.
const CARGO_DOOR_SURROUND_REAR: f64 = 50.6;
/// One kg/cm^2 in Pa.
const KG_PER_CM2_PA: f64 = 98_066.5;

/// The stations of a civil main-deck freighter's freighter items, in the
/// geometry frame, with the door masses taken out of the fuselage increment.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct MainDeckFreightPlacement {
    /// The freighter increment inside the fuselage mass, kg.
    pub structure_increment_kg: f64,
    /// Nose loading door, kg; zero without one.
    pub nose_door_kg: f64,
    /// Station of the nose door, m.
    pub nose_door_x_m: f64,
    /// Main-deck side cargo door with its surround, kg; zero when its
    /// station is not declared.
    pub side_door_kg: f64,
    /// Station of the side cargo door centreline, m.
    pub side_door_x_m: f64,
    /// Middle of the main-deck floor, m: the freight floor and the pallet
    /// handling system.
    pub deck_centroid_x_m: f64,
}

impl MainDeckFreightPlacement {
    /// The freight floor: what the doors leave of the increment, kg.
    pub fn floor_kg(&self) -> f64 {
        self.structure_increment_kg - self.nose_door_kg - self.side_door_kg
    }
}

/// Mass of a pressurized side cargo door with its surround, kg (Table D-2).
pub fn side_cargo_door_kg(
    width_m: f64,
    height_m: f64,
    pressure_differential_pa: f64,
    rear: bool,
) -> f64 {
    let area_m2 = width_m * height_m;
    let surround = if rear {
        CARGO_DOOR_SURROUND_REAR
    } else {
        CARGO_DOOR_SURROUND_FRONT
    };
    CARGO_DOOR_FILLING * (pressure_differential_pa / KG_PER_CM2_PA).sqrt() * area_m2
        + surround * area_m2.sqrt()
}

/// Mass of a nose loading door of elliptical outline, kg (section D-3).
pub fn nose_cargo_door_kg(width_m: f64, height_m: f64) -> f64 {
    NOSE_DOOR_KG_PER_M2 * std::f64::consts::FRAC_PI_4 * width_m * height_m
}

/// Station of the nose shell ahead of the section where the loft first
/// reaches `door_width_m`: the centroid of the shell area from the nose to
/// that break, m. `None` when the loft never reaches the width.
fn nose_door_station_m(fuselage: &Fuselage, door_width_m: f64) -> Option<f64> {
    let mut area = 0.0;
    let mut moment = 0.0;
    for pair in fuselage.xsecs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (xa, xb) = (a.xyz_c[0], b.xyz_c[0]);
        if xb <= xa {
            continue;
        }
        // The break lies in this segment when its aft section is wide enough.
        let t = if b.width >= door_width_m {
            ((door_width_m - a.width) / (b.width - a.width)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let x_end = xa + t * (xb - xa);
        let girth = |width: f64, height: f64| 0.5 * std::f64::consts::PI * (width + height);
        let girth_a = girth(a.width, a.height);
        let girth_end = girth(
            a.width + t * (b.width - a.width),
            a.height + t * (b.height - a.height),
        );
        // Linear girth over the segment: exact area and moment.
        let length = x_end - xa;
        area += 0.5 * (girth_a + girth_end) * length;
        moment += length * (girth_a * (2.0 * xa + x_end) + girth_end * (xa + 2.0 * x_end)) / 6.0;
        if b.width >= door_width_m {
            return (area > 0.0).then(|| moment / area);
        }
    }
    None
}

/// Place the freighter items of a civil main-deck freighter.
///
/// `fuselage_kg` is the fuselage structure including the increment of
/// `increment_fraction`; `pressure_differential_pa` the cabin pressure
/// differential the doors hold.
pub fn main_deck_freight_placement(
    fuselage: &Fuselage,
    geometry: &GeometryConfig,
    cabin: &CabinConfig,
    fuselage_kg: f64,
    increment_fraction: f64,
    pressure_differential_pa: f64,
) -> Option<MainDeckFreightPlacement> {
    let nose_x = fuselage.xsecs.first()?.xyz_c[0];
    let tail_x = fuselage.xsecs.last()?.xyz_c[0];
    let length = tail_x - nose_x;
    if !(length > 0.0 && fuselage_kg > 0.0 && increment_fraction > 0.0) {
        return None;
    }
    let structure_increment_kg = fuselage_kg * increment_fraction / (1.0 + increment_fraction);
    let cargo = &cabin.cargo;
    let deck_start = nose_x + geometry.fuselage.cabin_start_x_m;
    let deck_end = tail_x - geometry.fuselage.tailcone_length_m;
    let deck_centroid_x_m = 0.5 * (deck_start + deck_end.max(deck_start));

    let (mut nose_door_kg, nose_door_x_m) = cargo
        .nose_cargo_door_width_m
        .zip(cargo.nose_cargo_door_height_m)
        .and_then(|(width, height)| {
            let station = nose_door_station_m(fuselage, width)?;
            Some((nose_cargo_door_kg(width, height), station))
        })
        .unwrap_or((0.0, nose_x));
    let (mut side_door_kg, side_door_x_m) = match (
        cargo.main_deck_door_station_m,
        cargo.main_deck_door_clear_width_m,
        cargo.main_deck_door_clear_height_m,
    ) {
        (Some(station), Some(width), Some(height)) => (
            side_cargo_door_kg(
                width,
                height,
                pressure_differential_pa,
                station > 0.5 * length,
            ),
            nose_x + station,
        ),
        _ => (0.0, deck_centroid_x_m),
    };
    // The doors are parts of the increment: they never exceed it.
    let doors_kg = nose_door_kg + side_door_kg;
    if doors_kg > structure_increment_kg {
        let scale = structure_increment_kg / doors_kg;
        nose_door_kg *= scale;
        side_door_kg *= scale;
    }
    Some(MainDeckFreightPlacement {
        structure_increment_kg,
        nose_door_kg,
        nose_door_x_m,
        side_door_kg,
        side_door_x_m,
        deck_centroid_x_m,
    })
}

/// The pressure differential a freighter's cargo doors hold, Pa.
pub fn door_pressure_differential_pa(config: &AlasConfig) -> f64 {
    super::structure::pressurized_fuselage::cabin_pressure_differential_pa(
        config
            .mass_model
            .flops_structure
            .maximum_operating_altitude_m
            .unwrap_or(config.requirements.cruise_altitude_m),
    )
}

/// Move the lumped fuselage and systems points of a civil main-deck freighter
/// to the centroids the item ledger gives those groups, so the lumped model
/// and the ledger carry the same balance. `masses.fuselage` includes the
/// paint, about one percent of the group, which the increment here therefore
/// reads; every other aircraft is untouched.
pub fn shift_lumped_coordinates(
    config: &AlasConfig,
    plane: &Airplane,
    masses: &MassBreakdown,
    coordinates: &mut MassCoordinates,
) {
    if !super::military_cargo::carries_main_deck_freight(&config.requirements, &config.cabin) {
        return;
    }
    let Some(fuselage) = super::product::primary_fuselage(plane) else {
        return;
    };
    let Some(placement) = main_deck_freight_placement(
        fuselage,
        &config.geometry,
        &config.cabin,
        masses.fuselage,
        super::military_cargo::MAIN_DECK_CARGO_DOOR_FUSELAGE_FRACTION,
        door_pressure_differential_pa(config),
    ) else {
        return;
    };
    if masses.fuselage > 0.0 {
        let x = coordinates.fuselage[0];
        coordinates.fuselage[0] += (placement.nose_door_kg * (placement.nose_door_x_m - x)
            + placement.side_door_kg * (placement.side_door_x_m - x)
            + placement.floor_kg() * (placement.deck_centroid_x_m - x))
            / masses.fuselage;
    }
    let handling_kg = super::military_cargo::military_cargo_equipment(
        &super::military_cargo::civil_main_deck_hold(fuselage, &config.geometry),
        config
            .mass_model
            .flops_structure
            .design_gross_mass_kg
            .unwrap_or(config.requirements.mtow_kg),
        0.0,
    )
    .total_kg();
    if masses.systems > handling_kg {
        coordinates.systems[0] +=
            handling_kg * (placement.deck_centroid_x_m - coordinates.systems[0]) / masses.systems;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_door_masses_follow_the_published_coefficients() {
        // Imperial form of Table D-2: 2.65 sqrt(dp psi) b h lb + 24 sqrt(A ft2).
        let (b, h, dp_psi) = (11.0_f64, 10.0_f64, 8.9_f64);
        let imperial_lb = 2.65 * dp_psi.sqrt() * b * h + 34.0 * (b * h).sqrt();
        let metric = side_cargo_door_kg(b * 0.3048, h * 0.3048, dp_psi * 6_894.757, true);
        assert!(
            (metric / 0.453_592_37 - imperial_lb).abs() / imperial_lb < 0.01,
            "{metric} kg vs {imperial_lb} lb"
        );
        // 15 lb/ft2 of frontal area.
        let nose = nose_cargo_door_kg(3.0, 3.0);
        let area_ft2 = std::f64::consts::FRAC_PI_4 * 9.0 / 0.092_903_04;
        assert!((nose / 0.453_592_37 - 15.0 * area_ft2).abs() < 0.5);
    }

    #[test]
    fn the_floor_takes_what_the_doors_leave() {
        let placement = MainDeckFreightPlacement {
            structure_increment_kg: 3_000.0,
            nose_door_kg: 700.0,
            nose_door_x_m: 2.0,
            side_door_kg: 500.0,
            side_door_x_m: 40.0,
            deck_centroid_x_m: 28.0,
        };
        assert!((placement.floor_kg() - 1_800.0).abs() < 1e-9);
    }
}
