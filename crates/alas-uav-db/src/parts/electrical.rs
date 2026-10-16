// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion-chain parts: motor, ESC, battery.
//!
//! All `Num`/count/text members are optional: an absent member is unknown, not
//! zero. The units, ranges and the physics models each member is mandatory for
//! live in [`crate::inventory`], the single table the validators read.

use serde::{Deserialize, Serialize};

use crate::field::{Curve, Field, Num};

/// A brushless motor. Rm is phase-to-phase resistance (component-data.md 3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotorSpec {
    /// Speed constant, rad/s per volt (vendor KV x 2 pi / 60).
    pub kv_rad_s_per_v: Option<Num>,
    /// Phase-to-phase winding resistance, Ohm (test temperature in `test`).
    pub rm_ohm: Option<Num>,
    /// No-load current, A.
    pub i0_a: Option<Num>,
    /// Voltage at which `i0_a` was stated, V.
    pub i0_test_v: Option<Num>,
    /// Maximum continuous current, A.
    pub i_max_cont_a: Option<Num>,
    /// Maximum burst current, A (duration in `burst_s`).
    pub i_max_burst_a: Option<Num>,
    /// Burst duration, s.
    pub burst_s: Option<Num>,
    /// Maximum power, W.
    pub p_max_w: Option<Num>,
    /// Number of magnetic poles.
    pub n_poles: Option<Field<u32>>,
    /// Mass including cables when stated, kg.
    pub mass_kg: Option<Num>,
    /// Can diameter, m.
    pub diameter_m: Option<Num>,
    /// Overall length without shaft, m.
    pub length_m: Option<Num>,
    /// Shaft diameter, m.
    pub shaft_d_m: Option<Num>,
    /// Mounting pattern text (for example "16x19 M3").
    pub mount_pattern: Option<Field<String>>,
    /// Maximum shaft speed, rad/s.
    pub omega_max_rad_s: Option<Num>,
    /// Winding-to-ambient thermal resistance, K/W.
    pub thermal_r_k_per_w: Option<Num>,
    /// Maximum winding temperature, K.
    pub winding_temp_max_k: Option<Num>,
    /// Minimum LiPo series count the vendor supports.
    pub cells_min: Option<Field<u32>>,
    /// Maximum LiPo series count the vendor supports.
    pub cells_max: Option<Field<u32>>,
    /// Vendor-claimed maximum static thrust, N (a claim, not a measurement).
    pub thrust_static_max_n: Option<Num>,
    /// Vendor-recommended propeller diameter, m.
    pub rec_prop_diameter_m: Option<Num>,
    /// Vendor-recommended propeller pitch, m.
    pub rec_prop_pitch_m: Option<Num>,
    /// No-load current versus shaft speed, A over rad/s (optional refinement).
    pub i0_vs_omega: Option<Curve>,
}

/// An electronic speed controller with optional BEC.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscSpec {
    /// Continuous output current, A.
    pub i_cont_a: Option<Num>,
    /// Burst output current, A.
    pub i_burst_a: Option<Num>,
    /// Minimum input voltage, V.
    pub v_min_v: Option<Num>,
    /// Maximum input voltage, V.
    pub v_max_v: Option<Num>,
    /// Minimum LiPo series count.
    pub cells_min: Option<Field<u32>>,
    /// Maximum LiPo series count.
    pub cells_max: Option<Field<u32>>,
    /// Minimum adjustable BEC voltage, V.
    pub bec_v_min_v: Option<Num>,
    /// Maximum adjustable BEC voltage, V.
    pub bec_v_max_v: Option<Num>,
    /// Continuous BEC current, A.
    pub bec_a_cont_a: Option<Num>,
    /// Peak BEC current, A.
    pub bec_a_peak_a: Option<Num>,
    /// Supported signal protocols (for example "DShot600").
    pub protocols: Option<Field<Vec<String>>>,
    /// Mass including cables when stated, kg.
    pub mass_kg: Option<Num>,
    /// Bounding box length, width, height, m.
    pub dims_m: Option<Field<[f64; 3]>>,
    /// Efficiency (1) versus output current (A).
    pub efficiency_vs_current: Option<Curve>,
}

/// A battery pack.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatterySpec {
    /// Chemistry text (for example "LiPo").
    pub chemistry: Option<Field<String>>,
    /// Series cell count.
    pub cells_s: Option<Field<u32>>,
    /// Parallel string count.
    pub parallel_p: Option<Field<u32>>,
    /// Rated capacity, C (coulomb; 1 Ah = 3600 C).
    pub capacity_c: Option<Num>,
    /// Nominal pack voltage, V.
    pub v_nom_v: Option<Num>,
    /// Fully charged voltage, V.
    pub v_max_v: Option<Num>,
    /// Cut-off voltage, V.
    pub v_min_v: Option<Num>,
    /// Stored energy, J (usually derived; kept when a vendor states it).
    pub energy_j: Option<Num>,
    /// Pack DC internal resistance, Ohm (state of charge and temperature in `test`).
    pub r_dc_ohm: Option<Num>,
    /// Continuous discharge current, A.
    pub i_cont_a: Option<Num>,
    /// Burst discharge current, A.
    pub i_burst_a: Option<Num>,
    /// Vendor C rating as printed (1/h; a claim, not a measurement).
    pub claimed_c_rating: Option<Num>,
    /// Pack mass, kg.
    pub mass_kg: Option<Num>,
    /// Bounding box length, width, height, m.
    pub dims_m: Option<Field<[f64; 3]>>,
    /// Main connector text.
    pub connector: Option<Field<String>>,
    /// Open-circuit voltage (V) versus state of charge (1).
    pub voc_vs_soc: Option<Curve>,
    /// Pack resistance (Ohm) versus state of charge (1), one curve per temperature.
    #[serde(default)]
    pub r_dc_vs_soc_t: Vec<Curve>,
}
