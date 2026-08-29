// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shape of `golden/aero/lift_surrogate.json`.
//!
//! The vehicle this row trains on is the same one `golden/aero/vorlax.json`
//! records, and it is read out of *that* fixture rather than restated here:
//! the two generators build it the same way, and a second transcription of
//! twenty wing dimensions is a second thing that can drift.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub fuselage_lift_correction: f64,
    pub supersonic_surrogate_is_absent: bool,
    pub transonic_surrogate_is_absent: bool,
}

#[derive(Debug, Deserialize)]
pub struct Training {
    pub angle_of_attack_rad: Vec<f64>,
    pub mach: Vec<f64>,
    pub lift_coefficient: Vec<Vec<f64>>,
    pub drag_coefficient: Vec<Vec<f64>>,
    pub wing_lift_coefficient: HashMap<String, Vec<Vec<f64>>>,
    pub wing_drag_coefficient: HashMap<String, Vec<Vec<f64>>>,
}

/// One fitted `RectBivariateSpline`, as its knots and coefficients.
#[derive(Debug, Deserialize)]
pub struct Spline {
    pub knots_x: Vec<f64>,
    pub knots_y: Vec<f64>,
    pub coefficients: Vec<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Surrogates {
    pub lift_coefficient: Spline,
    pub drag_coefficient: Spline,
    pub wing_lift_coefficient: HashMap<String, Spline>,
    pub wing_drag_coefficient: HashMap<String, Spline>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub tag: String,
    pub angle_of_attack_deg: f64,
    pub mach: f64,
    pub inviscid_lift_coefficient: f64,
    pub inviscid_induced_drag_coefficient: f64,
    pub aircraft_lift_coefficient: f64,
    pub wing_lift_coefficient: HashMap<String, f64>,
    pub wing_induced_drag_coefficient: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub settings: Settings,
    pub wing_tags: Vec<String>,
    pub training: Training,
    pub surrogates: Surrogates,
    pub cases: Vec<Case>,
}
