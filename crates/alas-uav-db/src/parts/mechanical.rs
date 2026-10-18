// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airframe-side purchased parts: propeller, servo, avionics, wheel, gear,
//! connector.

use serde::{Deserialize, Serialize};

use crate::error::{DbError, DbResult};
use crate::field::{Curve, Field, Num, Provenance};

/// Blade geometry tabulated by radius (one provenance for the table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropGeometry {
    /// Station radii, m, strictly increasing.
    pub r_m: Vec<f64>,
    /// Chord at each station, m.
    pub chord_m: Vec<f64>,
    /// Twist at each station, rad (positive nose-up, measured to the zero-lift
    /// reference stated by the source in `prov.raw`).
    pub twist_rad: Vec<f64>,
    /// Airfoil name at each station.
    pub airfoil: Vec<String>,
    /// Provenance of the table.
    #[serde(flatten)]
    pub prov: Provenance,
}

impl PropGeometry {
    /// Check lengths, monotone radius and positive chords.
    pub fn validate(&self, what: &str) -> DbResult<()> {
        self.prov.validate(what)?;
        let n = self.r_m.len();
        if n < 2 || [self.chord_m.len(), self.twist_rad.len(), self.airfoil.len()] != [n; 3] {
            return Err(DbError::new(format!(
                "{what}: geometry columns must have equal length >= 2"
            )));
        }
        let finite = self.r_m.iter().chain(&self.chord_m).chain(&self.twist_rad);
        if finite.clone().any(|v| !v.is_finite())
            || self.r_m.windows(2).any(|w| w[1] <= w[0])
            || self.r_m.iter().chain(&self.chord_m).any(|&v| v <= 0.0)
        {
            return Err(DbError::new(format!(
                "{what}: geometry needs finite values, increasing positive radius and chord"
            )));
        }
        Ok(())
    }
}

/// A propeller (geometry only; measured and computed maps are `PropPerfSet`s).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropellerSpec {
    /// Diameter, m.
    pub diameter_m: Option<Num>,
    /// Geometric pitch, m.
    pub pitch_m: Option<Num>,
    /// Blade count.
    pub blades: Option<Field<u32>>,
    /// Product family text (for example "APC Thin Electric").
    pub family: Option<Field<String>>,
    /// Mass, kg.
    pub mass_kg: Option<Num>,
    /// Hub bore, m.
    pub hub_bore_m: Option<Num>,
    /// Whether the blades fold.
    pub folding: Option<Field<bool>>,
    /// Polar moment of inertia about the shaft, kg*m2.
    pub inertia_kg_m2: Option<Num>,
    /// Whether the vendor permits electric propulsion.
    pub electric_compatible: Option<Field<bool>>,
    /// Blade geometry for BEMT.
    pub geometry: Option<PropGeometry>,
}

/// A servo. Torque and speed are valid only at the voltage they were stated.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServoSpec {
    /// Stall torque (N*m) versus supply voltage (V).
    pub torque_vs_voltage: Option<Curve>,
    /// No-load speed (rad/s) versus supply voltage (V).
    pub speed_vs_voltage: Option<Curve>,
    /// Stall current (A) versus supply voltage (V).
    pub i_stall_vs_voltage: Option<Curve>,
    /// Minimum supply voltage, V.
    pub v_min_v: Option<Num>,
    /// Maximum supply voltage, V.
    pub v_max_v: Option<Num>,
    /// Mass, kg.
    pub mass_kg: Option<Num>,
    /// Bounding box length, width, height, m.
    pub dims_m: Option<Field<[f64; 3]>>,
    /// Gear type text (metal, nylon, titanium, ...).
    pub gear: Option<Field<String>>,
}

/// Receiver, flight controller or other avionics item (power and mass budget).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvionicsSpec {
    /// Functional role text (for example "receiver", "gps", "bec").
    pub role: Option<Field<String>>,
    /// Mass, kg.
    pub mass_kg: Option<Num>,
    /// Minimum supply voltage, V.
    pub v_in_min_v: Option<Num>,
    /// Maximum supply voltage, V.
    pub v_in_max_v: Option<Num>,
    /// Supply current draw, A.
    pub i_draw_a: Option<Num>,
    /// Rated maximum current of an output or limiter, A (a rating, not a draw).
    pub i_max_a: Option<Num>,
    /// Output channel count.
    pub channels: Option<Field<u32>>,
    /// Supported protocols.
    pub protocols: Option<Field<Vec<String>>>,
    /// Whether the item provides telemetry.
    pub telemetry: Option<Field<bool>>,
    /// Bounding box length, width, height, m.
    pub dims_m: Option<Field<[f64; 3]>>,
}

/// A wheel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelSpec {
    /// Outer diameter, m.
    pub diameter_m: Option<Num>,
    /// Tyre width, m.
    pub width_m: Option<Num>,
    /// Mass, kg.
    pub mass_kg: Option<Num>,
    /// Rolling-resistance coefficient on short grass (1).
    pub crr_grass: Option<Num>,
}

/// A purchased landing-gear assembly or retract mechanism.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GearSpec {
    /// Form text (fixed_skid, retract_mechanism, strut, ...).
    pub form: Option<Field<String>>,
    /// Assembly mass, kg.
    pub mass_kg: Option<Num>,
    /// Maximum supported aircraft mass, kg.
    pub max_aircraft_mass_kg: Option<Num>,
    /// Nominal mechanism voltage, V.
    pub nominal_voltage_v: Option<Num>,
    /// Bounding box length, width, height, m.
    pub dims_m: Option<Field<[f64; 3]>>,
}

/// A connector family.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSpec {
    /// Continuous current rating, A.
    pub i_cont_a: Option<Num>,
    /// Contact-pair resistance, Ohm.
    pub r_contact_ohm: Option<Num>,
    /// Mass of a mated pair, kg.
    pub mass_kg: Option<Num>,
}
