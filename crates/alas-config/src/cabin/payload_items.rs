// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User-declared payload items: floor zones and point masses.
//!
//! The seat classes and unit-load devices of the cabin model cover a
//! conventional airliner or freighter. Special aircraft (a presidential
//! layout with offices and bedrooms, a mothership carrying a drone, a
//! humanitarian drop) also carry things the class mix cannot describe: a
//! furnished room, a radome equipment rack, a store that is released in
//! flight. This module is where those are declared.
//!
//! Frame: aircraft body axes, metres from the nose tip, x aft, y starboard,
//! z up, the same frame the payload layout uses. Masses are kilograms,
//! areas square metres, inertias kg m^2 about the item's own centre of mass.
//!
//! Every list is empty by default and is omitted from serialized files when
//! empty, so configurations written before these items existed load and
//! serialize unchanged.

use serde::{Deserialize, Serialize};

/// Whether an item counts against the operating empty mass or the payload.
///
/// Furnishing a room and mounting a radome rack are part of the aircraft's
/// operating empty mass (they fly on every sortie and are not revenue load).
/// Occupants, freight and stores that are released or unloaded are payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MassCategory {
    /// Counted in the payload and in the loading envelope.
    #[default]
    Payload,
    /// Counted in the operating empty mass (furnishings ledger line).
    Equipment,
}

/// What a floor zone is for. Only the label and the default behaviour differ;
/// every kind other than `Seats` and `Cargo` takes its floor out of the seat
/// layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    /// Seats of the cabin class mix (informational; carves nothing out).
    Seats,
    /// Office or conference room.
    Office,
    /// Bedroom or crew-rest room.
    Bedroom,
    /// Operations, communications or mission centre.
    Ops,
    /// Galley.
    Galley,
    /// Lavatory or shower.
    Lav,
    /// Cargo or stores area on the passenger deck.
    Cargo,
    /// Anything else.
    #[default]
    Custom,
}

impl ZoneKind {
    /// Whether the zone takes floor away from the seat layout.
    pub fn takes_seat_floor(self) -> bool {
        !matches!(self, Self::Seats)
    }
}

/// Which deck a zone is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneDeck {
    /// The main passenger deck.
    #[default]
    Main,
    /// The upper deck of a double-deck body.
    Upper,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Whether `range` is a finite, ordered `[forward, aft]` span containing
/// `[from, to]`.
fn range_holds(range: Option<[f64; 2]>, from: f64, to: f64) -> bool {
    range.is_some_and(|[a, b]| a.is_finite() && b.is_finite() && a < b && a <= from && to <= b)
}

/// A furnished stretch of cabin floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorZoneConfig {
    /// Display name.
    pub name: String,
    /// What the zone is.
    #[serde(default)]
    pub kind: ZoneKind,
    /// Which deck it is on.
    #[serde(default)]
    pub deck: ZoneDeck,
    /// Forward station, m.
    pub x_start_m: f64,
    /// Aft station, m.
    pub x_end_m: f64,
    /// Lateral extent, m, centred on `y_center_m`; `None` is the full cabin
    /// width at each station (the zone is then a full-width room).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_m: Option<f64>,
    /// Lateral centre, m (positive starboard).
    #[serde(default)]
    pub y_center_m: f64,
    /// Furnishing mass per unit floor area, kg/m2, counted in the operating
    /// empty mass. Ignored when `total_mass_kg` is given.
    #[serde(default)]
    pub furnishing_kg_per_m2: f64,
    /// Total furnishing mass, kg, replacing the area-based figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_mass_kg: Option<f64>,
    /// People occupying the zone; each carries the product passenger load
    /// case mass (`requirements.passenger_mass_kg`) as payload.
    #[serde(default)]
    pub occupants: i64,
    /// Whether the zone's floor is cut out of the seat layout. A zone of
    /// kind `Seats` never is.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub carve_seat_floor: bool,
    /// Whether the payload auto-balance may slide the zone along the cabin
    /// (the zone keeps its length). Needs `x_range_m`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub movable: bool,
    /// The stations, m, a movable zone may occupy: [forward, aft].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_range_m: Option<[f64; 2]>,
}

impl FloorZoneConfig {
    /// Length along x, m.
    pub fn length_m(&self) -> f64 {
        self.x_end_m - self.x_start_m
    }
}

/// A named lumped mass at a point, optionally released in flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointMassConfig {
    /// Stable identifier. Mission events refer to a releasable item by it.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Station, m.
    pub x_m: f64,
    /// Lateral position, m (positive starboard).
    #[serde(default)]
    pub y_m: f64,
    /// Height, m (positive up, same datum as the cabin geometry).
    pub z_m: f64,
    /// Mass, kg.
    pub mass_kg: f64,
    /// Payload or operating-empty equipment.
    #[serde(default)]
    pub category: MassCategory,
    /// Principal inertias [Ixx, Iyy, Izz] about the item's own centre, kg m2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inertia_kgm2: Option<[f64; 3]>,
    /// Whether a mission event may release it (a drop, a launched drone).
    #[serde(default)]
    pub releasable: bool,
    /// Parasite drag area increment while carried, m2 (equivalent flat-plate
    /// area, i.e. delta CD times the reference area).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag_area_m2: Option<f64>,
    /// Whether the payload auto-balance may move the item along the
    /// fuselage. Needs `x_range_m`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub movable: bool,
    /// The stations, m, a movable item may occupy: [forward, aft].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_range_m: Option<[f64; 2]>,
}

/// Declared floor zones and point masses.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadItemsConfig {
    /// Furnished floor zones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<FloorZoneConfig>,
    /// Lumped masses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_masses: Vec<PointMassConfig>,
    /// Custom payload: no seats and no cargo are laid out; the payload is
    /// only these zones and point masses.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom_only: bool,
}

impl PayloadItemsConfig {
    /// Whether any zone or point mass is declared.
    pub fn has_items(&self) -> bool {
        !self.zones.is_empty() || !self.point_masses.is_empty()
    }

    /// Whether nothing is declared (the serialization default).
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty() && self.point_masses.is_empty() && !self.custom_only
    }

    /// Payload mass the declared items carry, kg: point masses and zone
    /// freight in the payload category, and `occupant_kg` per zone occupant.
    #[allow(clippy::cast_precision_loss)]
    pub fn payload_mass_kg(&self, occupant_kg: f64) -> f64 {
        let points: f64 = self
            .point_masses
            .iter()
            .filter(|p| p.category == MassCategory::Payload)
            .map(|p| p.mass_kg.max(0.0))
            .sum();
        let zones: f64 = self
            .zones
            .iter()
            .map(|z| (z.occupants.max(0) as f64) * occupant_kg.max(0.0))
            .sum();
        points + zones
    }

    /// [`Self::validate`] as a blocking validation issue, if it fails.
    pub(crate) fn issue(&self) -> Option<crate::ValidationIssue> {
        self.validate().err().map(|message| crate::ValidationIssue {
            severity: crate::Severity::Error,
            field_path: "cabin.payload_items".to_owned(),
            message,
        })
    }

    /// Check every declared item; the message names the first offender.
    ///
    /// # Errors
    ///
    /// A zone with a non-finite or inverted extent or a negative mass, an
    /// overlap between two zones on one deck, a point mass with a
    /// non-finite position or a non-positive mass, or a duplicate id.
    pub fn validate(&self) -> Result<(), String> {
        for zone in &self.zones {
            let finite = [
                zone.x_start_m,
                zone.x_end_m,
                zone.y_center_m,
                zone.furnishing_kg_per_m2,
            ]
            .iter()
            .all(|v| v.is_finite());
            if !finite || zone.length_m() <= 0.0 {
                return Err(format!(
                    "floor zone '{}' needs a finite extent with x_end_m > x_start_m",
                    zone.name
                ));
            }
            if zone.movable
                && (zone.furnishing_kg_per_m2 > 0.0 || zone.total_mass_kg.is_some_and(|m| m > 0.0))
            {
                return Err(format!(
                    "movable floor zone '{}' must carry no furnishing mass: furnishing is operating empty mass at its own station, only occupants are balanced",
                    zone.name
                ));
            }
            if zone.movable && !range_holds(zone.x_range_m, zone.x_start_m, zone.x_end_m) {
                return Err(format!(
                    "movable floor zone '{}' needs an x_range_m [forward, aft] that contains it",
                    zone.name
                ));
            }
            if zone.width_m.is_some_and(|w| !w.is_finite() || w <= 0.0) {
                return Err(format!("floor zone '{}' width must be positive", zone.name));
            }
            if zone.furnishing_kg_per_m2 < 0.0
                || zone.occupants < 0
                || zone
                    .total_mass_kg
                    .is_some_and(|m| !m.is_finite() || m < 0.0)
            {
                return Err(format!(
                    "floor zone '{}' mass, density and occupants must not be negative",
                    zone.name
                ));
            }
        }
        for (i, a) in self.zones.iter().enumerate() {
            for b in &self.zones[i + 1..] {
                if a.deck == b.deck && a.x_start_m < b.x_end_m && b.x_start_m < a.x_end_m {
                    let a_edge = a.y_center_m - a.width_m.unwrap_or(f64::INFINITY) / 2.0;
                    let a_far = a.y_center_m + a.width_m.unwrap_or(f64::INFINITY) / 2.0;
                    let b_edge = b.y_center_m - b.width_m.unwrap_or(f64::INFINITY) / 2.0;
                    let b_far = b.y_center_m + b.width_m.unwrap_or(f64::INFINITY) / 2.0;
                    if a_edge < b_far && b_edge < a_far {
                        return Err(format!(
                            "floor zones '{}' and '{}' overlap on the {:?} deck",
                            a.name, b.name, a.deck
                        ));
                    }
                }
            }
        }
        for (i, item) in self.point_masses.iter().enumerate() {
            let finite = [item.x_m, item.y_m, item.z_m, item.mass_kg]
                .iter()
                .all(|v| v.is_finite());
            if !finite || item.mass_kg <= 0.0 {
                return Err(format!(
                    "point mass '{}' needs a finite position and a positive mass",
                    item.id
                ));
            }
            if item.movable && item.category != MassCategory::Payload {
                return Err(format!(
                    "movable point mass '{}' must be payload: equipment is operating empty mass",
                    item.id
                ));
            }
            if item.movable && !range_holds(item.x_range_m, item.x_m, item.x_m) {
                return Err(format!(
                    "movable point mass '{}' needs an x_range_m [forward, aft] that contains it",
                    item.id
                ));
            }
            if item.drag_area_m2.is_some_and(|a| !a.is_finite() || a < 0.0)
                || item
                    .inertia_kgm2
                    .is_some_and(|j| j.iter().any(|v| !v.is_finite() || *v < 0.0))
            {
                return Err(format!(
                    "point mass '{}' inertia and drag area must be finite and not negative",
                    item.id
                ));
            }
            if item.id.trim().is_empty()
                || self.point_masses[..i]
                    .iter()
                    .any(|other| other.id == item.id)
            {
                return Err(format!(
                    "point mass id '{}' is empty or duplicated",
                    item.id
                ));
            }
        }
        Ok(())
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn zone(name: &str, x0: f64, x1: f64) -> FloorZoneConfig {
        FloorZoneConfig {
            name: name.to_owned(),
            kind: ZoneKind::Office,
            deck: ZoneDeck::Main,
            x_start_m: x0,
            x_end_m: x1,
            width_m: None,
            y_center_m: 0.0,
            furnishing_kg_per_m2: 180.0,
            total_mass_kg: None,
            occupants: 0,
            carve_seat_floor: true,
            movable: false,
            x_range_m: None,
        }
    }

    fn mass(id: &str) -> PointMassConfig {
        PointMassConfig {
            id: id.to_owned(),
            name: id.to_owned(),
            x_m: 20.0,
            y_m: 0.0,
            z_m: 1.0,
            mass_kg: 450.0,
            category: MassCategory::Equipment,
            inertia_kgm2: None,
            releasable: false,
            drag_area_m2: None,
            movable: false,
            x_range_m: None,
        }
    }

    #[test]
    fn an_empty_declaration_serializes_to_an_empty_object() {
        let value = serde_json::to_value(PayloadItemsConfig::default()).unwrap();
        assert_eq!(value, serde_json::json!({}));
        let loaded: PayloadItemsConfig = serde_json::from_value(value).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn declared_items_round_trip() {
        let items = PayloadItemsConfig {
            zones: vec![zone("Office", 10.0, 15.0)],
            point_masses: vec![PointMassConfig {
                releasable: true,
                drag_area_m2: Some(0.2),
                inertia_kgm2: Some([10.0, 40.0, 40.0]),
                ..mass("drone")
            }],
            ..PayloadItemsConfig::default()
        };
        let text = serde_json::to_string(&items).unwrap();
        let loaded: PayloadItemsConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(loaded, items);
        assert!(items.validate().is_ok());
    }

    #[test]
    fn overlapping_full_width_zones_are_refused() {
        let items = PayloadItemsConfig {
            zones: vec![zone("A", 10.0, 15.0), zone("B", 14.0, 18.0)],
            point_masses: Vec::new(),
            ..PayloadItemsConfig::default()
        };
        assert!(items.validate().unwrap_err().contains("overlap"));
    }

    #[test]
    fn side_by_side_zones_do_not_overlap() {
        let mut a = zone("A", 10.0, 15.0);
        let mut b = zone("B", 10.0, 15.0);
        a.width_m = Some(1.5);
        a.y_center_m = -1.0;
        b.width_m = Some(1.5);
        b.y_center_m = 1.0;
        let items = PayloadItemsConfig {
            zones: vec![a, b],
            point_masses: Vec::new(),
            ..PayloadItemsConfig::default()
        };
        assert!(items.validate().is_ok());
    }

    #[test]
    fn point_masses_need_unique_ids_and_positive_mass() {
        let duplicate = PayloadItemsConfig {
            zones: Vec::new(),
            point_masses: vec![mass("a"), mass("a")],
            ..PayloadItemsConfig::default()
        };
        assert!(duplicate.validate().is_err());
        let massless = PayloadItemsConfig {
            zones: Vec::new(),
            point_masses: vec![PointMassConfig {
                mass_kg: 0.0,
                ..mass("a")
            }],
            ..PayloadItemsConfig::default()
        };
        assert!(massless.validate().is_err());
    }
}
