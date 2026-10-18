// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compatibility filter for propulsion bundles (motor, ESC, pack, propeller).
//!
//! A bundle is one motor, one ESC, one battery and one propeller that have a
//! thrust-at-speed map for the same battery cell count. The search draws only
//! bundles that pass every rule below, so an incompatible combination is never
//! spent as an objective evaluation. Missing published fields make a bundle
//! incompatible; no default is inferred.

use crate::catalog::{BatterySpec, ComponentKind, EscSpec, MotorSpec, PropellerSpec};
use crate::optimizer::PropulsionMap;
use crate::Catalog;

/// Lowest accepted propeller diameter as a fraction of the motor's published
/// recommendation (ASSUMPTION: no catalogue rule; the window is a flagged
/// engineering choice, not a source value).
pub const PROPELLER_DIAMETER_MIN_FRACTION: f64 = 0.7;
/// Highest accepted propeller diameter as a fraction of the motor's published
/// recommendation (ASSUMPTION, see [`PROPELLER_DIAMETER_MIN_FRACTION`]).
pub const PROPELLER_DIAMETER_MAX_FRACTION: f64 = 1.15;
/// Propeller tip-speed limit, m/s (ASSUMPTION: about 0.7 Mach at sea level;
/// no catalogue field carries the limit).
pub const MAXIMUM_TIP_SPEED_M_S: f64 = 238.0;
/// Speed of the loaded propeller relative to the no-load speed (ASSUMPTION).
pub const LOADED_SPEED_FRACTION: f64 = 0.85;
/// Fully charged LiPo cell voltage used for the no-load speed check, V.
pub const FULL_CELL_VOLTAGE_V: f64 = 4.2;

/// Identifiers of one propulsion bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleIds {
    /// Motor record id.
    pub motor_id: String,
    /// ESC record id.
    pub esc_id: String,
    /// Battery record id.
    pub battery_id: String,
    /// Propeller record id.
    pub propeller_id: String,
    /// Battery series-cell count shared by the battery and the map.
    pub series_cells: u16,
    /// Propulsor count stored in the matched propulsion map.
    pub motor_count: u16,
}

/// Physical rule that a bundle violates or cannot be checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleRule {
    /// Motor or ESC does not support the battery cell count.
    Voltage,
    /// ESC continuous rating below the motor's maximum current.
    EscCurrent,
    /// Pack rated discharge current below the total motor current.
    BatteryCurrent,
    /// Propeller diameter outside the motor's recommended window.
    PropellerSize,
    /// Loaded propeller tip speed above the limit at this pack voltage.
    TipSpeed,
    /// A required published field is absent, or no propulsion map exists.
    MissingEvidence,
}

/// One rule outcome for a bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleIssue {
    /// Rule that failed.
    pub rule: BundleRule,
    /// Quantities involved, for review output.
    pub message: String,
}

/// A bundle that failed at least one rule, with the reasons.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedBundle {
    /// Identifiers of the rejected combination.
    pub ids: BundleIds,
    /// Every failed rule (empty never occurs here).
    pub issues: Vec<BundleIssue>,
}

/// Result of enumerating all motor x ESC x battery x propeller combinations.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleCatalogue {
    /// Bundles that pass every rule, in catalogue order.
    pub compatible: Vec<BundleIds>,
    /// Bundles with at least one failed rule.
    pub rejected: Vec<RejectedBundle>,
}

/// Find the propulsion map the optimizer would use for these identifiers.
///
/// The first map that matches motor, propeller and cell count is used, the same
/// rule as candidate generation.
pub fn map_for<'a>(
    maps: &'a [PropulsionMap],
    motor_id: &str,
    propeller_id: &str,
    series_cells: u16,
) -> Option<&'a PropulsionMap> {
    maps.iter().find(|map| {
        map.motor_id == motor_id
            && map.propeller_id == propeller_id
            && map.series_cells == series_cells
    })
}

/// Enumerate every bundle of the catalogue and split it into compatible and
/// rejected sets.
pub fn propulsion_bundles(catalog: &Catalog, maps: &[PropulsionMap]) -> BundleCatalogue {
    let mut compatible = Vec::new();
    let mut rejected = Vec::new();
    for motor_record in catalog.records.iter() {
        let ComponentKind::Motor(motor) = &motor_record.kind else {
            continue;
        };
        for propeller_record in catalog.records.iter() {
            let ComponentKind::Propeller(propeller) = &propeller_record.kind else {
                continue;
            };
            for battery_record in catalog.records.iter() {
                let ComponentKind::Battery(battery) = &battery_record.kind else {
                    continue;
                };
                for esc_record in catalog.records.iter() {
                    let ComponentKind::Esc(esc) = &esc_record.kind else {
                        continue;
                    };
                    let cells = battery.series_cells.unwrap_or(0);
                    let Some(map) = map_for(maps, &motor_record.id, &propeller_record.id, cells)
                    else {
                        let ids = BundleIds {
                            motor_id: motor_record.id.clone(),
                            esc_id: esc_record.id.clone(),
                            battery_id: battery_record.id.clone(),
                            propeller_id: propeller_record.id.clone(),
                            series_cells: cells,
                            motor_count: 0,
                        };
                        let issue = BundleIssue {
                            rule: BundleRule::MissingEvidence,
                            message:
                                "no thrust-at-speed map for this motor, propeller and cell count"
                                    .to_owned(),
                        };
                        rejected.push(RejectedBundle {
                            ids,
                            issues: vec![issue],
                        });
                        continue;
                    };
                    let issues = check_bundle(motor, esc, battery, propeller, map);
                    let ids = BundleIds {
                        motor_id: motor_record.id.clone(),
                        esc_id: esc_record.id.clone(),
                        battery_id: battery_record.id.clone(),
                        propeller_id: propeller_record.id.clone(),
                        series_cells: map.series_cells,
                        motor_count: map.motor_count,
                    };
                    if issues.is_empty() {
                        compatible.push(ids);
                    } else {
                        rejected.push(RejectedBundle { ids, issues });
                    }
                }
            }
        }
    }
    BundleCatalogue {
        compatible,
        rejected,
    }
}

/// Evaluate every compatibility rule for one bundle.
///
/// An empty result means the bundle is compatible. Rules are checked in the
/// order of the physical chain: voltage, ESC, pack, propeller size, speed.
pub fn check_bundle(
    motor: &MotorSpec,
    esc: &EscSpec,
    battery: &BatterySpec,
    propeller: &PropellerSpec,
    map: &PropulsionMap,
) -> Vec<BundleIssue> {
    let mut issues = Vec::new();
    let cells = map.series_cells;
    let mut missing = |field: &str| {
        issues.push(BundleIssue {
            rule: BundleRule::MissingEvidence,
            message: format!("published {field} is absent"),
        });
    };
    let motor_max_current = motor.max_current_a;
    let esc_current = esc.continuous_current_a;
    let rated_current = battery.rated_discharge_current_a();
    let recommended_diameter = motor.recommended_propeller_diameter_m;
    let kv = motor.kv_rpm_per_v;
    let diameter = propeller.diameter_m;
    if motor_max_current.is_none() {
        missing("motor maximum current");
    }
    if esc_current.is_none() {
        missing("ESC continuous current");
    }
    if rated_current.is_none() {
        missing("battery capacity or C rating");
    }
    if recommended_diameter.is_none() {
        missing("motor recommended propeller diameter");
    }
    if kv.is_none() {
        missing("motor Kv");
    }
    if diameter.is_none() {
        missing("propeller diameter");
    }
    if motor.min_series_cells.is_none() || motor.max_series_cells.is_none() {
        missing("motor series-cell range");
    }
    if esc.min_series_cells.is_none() || esc.max_series_cells.is_none() {
        missing("ESC series-cell range");
    }
    let supports_cells = |min: Option<u16>, max: Option<u16>| matches!((min, max), (Some(lo), Some(hi)) if (lo..=hi).contains(&cells));
    if !supports_cells(motor.min_series_cells, motor.max_series_cells)
        || !supports_cells(esc.min_series_cells, esc.max_series_cells)
    {
        issues.push(BundleIssue {
            rule: BundleRule::Voltage,
            message: format!("{cells}S is outside the motor or ESC cell range"),
        });
    }
    if let (Some(esc_current), Some(motor_current)) = (esc_current, motor_max_current) {
        if esc_current < motor_current {
            issues.push(BundleIssue {
                rule: BundleRule::EscCurrent,
                message: format!("ESC {esc_current:.1} A < motor maximum {motor_current:.1} A"),
            });
        }
    }
    if let (Some(rated), Some(motor_current)) = (rated_current, motor_max_current) {
        let total = f64::from(map.motor_count) * motor_current;
        if rated < total {
            issues.push(BundleIssue {
                rule: BundleRule::BatteryCurrent,
                message: format!("pack rating {rated:.1} A < {total:.1} A for all motors"),
            });
        }
    }
    if let (Some(diameter), Some(recommended)) = (diameter, recommended_diameter) {
        let low = PROPELLER_DIAMETER_MIN_FRACTION * recommended;
        let high = PROPELLER_DIAMETER_MAX_FRACTION * recommended;
        if !(low..=high).contains(&diameter) {
            issues.push(BundleIssue {
                rule: BundleRule::PropellerSize,
                message: format!("propeller {diameter:.3} m outside [{low:.3}, {high:.3}] m"),
            });
        }
    }
    if let (Some(kv), Some(diameter)) = (kv, diameter) {
        // Kv in rpm/V times the full-charge pack voltage gives the no-load
        // speed; the loaded speed is a flagged fraction of it.
        let rpm = kv * f64::from(cells) * FULL_CELL_VOLTAGE_V * LOADED_SPEED_FRACTION;
        let tip = rpm / 60.0 * std::f64::consts::PI * diameter;
        if tip > MAXIMUM_TIP_SPEED_M_S {
            issues.push(BundleIssue {
                rule: BundleRule::TipSpeed,
                message: format!("loaded tip speed {tip:.0} m/s > {MAXIMUM_TIP_SPEED_M_S:.0} m/s"),
            });
        }
    }
    issues
}
