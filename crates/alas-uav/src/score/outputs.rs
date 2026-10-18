// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Names a score or gate expression may read besides the frozen
//! `spec::mission::OUTPUTS`: rule-input quantities that come from the design
//! or the declared decisions rather than from the flight simulation, the
//! measured signals, and per-competition counts. All values are SI.
//!
//! The frozen list is not edited (schema freeze, decision D1); these names
//! belong to the score engine and are documented here.

/// A declared input of the score language.
#[derive(Debug, Clone, Copy)]
pub struct Declared {
    /// Name used in expressions.
    pub name: &'static str,
    /// SI unit string (see `units::parse_unit`).
    pub unit_si: &'static str,
    /// Meaning.
    pub doc: &'static str,
}

/// Scalar inputs added to `spec::mission::OUTPUTS`.
#[rustfmt::skip]
pub const EXTRA_INPUTS: &[Declared] = &[
    Declared { name: "announced_takeoff", unit_si: "m", doc: "Announced take-off distance (decision)" },
    Declared { name: "declared_payload_count", unit_si: "1", doc: "Declared payload count for the prediction bonus (decision)" },
    Declared { name: "distance_eff", unit_si: "m", doc: "Ground distance in the 90 s efficiency window (ACC 2024)" },
    Declared { name: "energy_eff", unit_si: "J", doc: "Battery energy used in the efficiency window (ACC 2024)" },
    Declared { name: "air_density", unit_si: "kg/m3", doc: "Air density measured before the round" },
    Declared { name: "laps_m2", unit_si: "1", doc: "Laps completed in the DBF charter mission" },
    Declared { name: "laps_m3", unit_si: "1", doc: "Laps completed in the DBF banner mission" },
    Declared { name: "motor_count", unit_si: "1", doc: "Number of propulsion motors" },
    Declared { name: "cell_count", unit_si: "1", doc: "Series cells of the propulsion pack" },
    Declared { name: "pack_voltage_max", unit_si: "V", doc: "Fully charged pack voltage" },
    Declared { name: "battery_continuous_rating", unit_si: "A", doc: "Continuous discharge rating of the pack" },
    Declared { name: "battery_capacity", unit_si: "A*s", doc: "Pack capacity (charge)" },
    Declared { name: "transport_box_sum", unit_si: "m", doc: "Sum of the inner transport-box edges (ACC)" },
    Declared { name: "transport_box_mass", unit_si: "kg", doc: "Box plus aircraft plus assembly tools mass (ACC)" },
    Declared { name: "bay_can_capacity", unit_si: "1", doc: "Cans the cargo bay holds" },
    Declared { name: "chord_min", unit_si: "m", doc: "Smallest wing chord (SAE Regular)" },
    Declared { name: "body_length", unit_si: "m", doc: "Body-axis length (SAE Regular)" },
    Declared { name: "frp_used", unit_si: "1", doc: "1 when fibre-reinforced plastic is used (SAE Regular forbids it)" },
    Declared { name: "container_volume", unit_si: "m3", doc: "Payload container volume (SAE Micro)" },
    Declared { name: "prop_diameter", unit_si: "m", doc: "Propeller diameter" },
    Declared { name: "fuse_rating", unit_si: "A", doc: "Arming fuse rating (DBF)" },
    Declared { name: "pack_energy_max", unit_si: "J", doc: "Largest single-pack energy" },
];

/// Time-series inputs (battery side, 10 Hz in the ACC measurement box).
#[rustfmt::skip]
pub const SIGNALS: &[Declared] = &[
    Declared { name: "battery_current", unit_si: "A", doc: "Battery-side current" },
    Declared { name: "battery_voltage", unit_si: "V", doc: "Battery terminal voltage" },
    Declared { name: "battery_power", unit_si: "W", doc: "Battery-side electrical power" },
];
