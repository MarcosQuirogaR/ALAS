// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cross-field sanity checks (warnings, never errors).
//!
//! They catch the unit and transcription mistakes that single-field ranges
//! cannot: a power rating that disagrees with current times voltage, a pack
//! whose energy does not match capacity times voltage, a motor whose mass is
//! implausible for its size. Thresholds are deliberately loose (plan section
//! 7.3 asks for ~20 % on Pmax versus Imax V); a warning asks for a human look.

use crate::component::{Body, Component};
use crate::database::Database;
use crate::field::Num;
use crate::parts::{BatterySpec, EscSpec, MotorSpec};
use crate::validate::Report;

/// LiPo cell voltage band used to turn a series count into a voltage band, V.
const CELL_V_NOMINAL: f64 = 3.7;
const CELL_V_FULL: f64 = 4.2;

fn val(field: &Option<Num>) -> Option<f64> {
    field.as_ref().map(|f| f.value)
}

/// Run every cross-field check over all rows.
pub fn check(db: &Database, report: &mut Report) {
    for component in &db.components {
        match &component.body {
            Body::Motor(motor) => motor_checks(component, motor, report),
            Body::Esc(esc) => esc_checks(component, esc, report),
            Body::Battery(battery) => battery_checks(component, battery, report),
            _ => {}
        }
    }
}

fn motor_checks(component: &Component, motor: &MotorSpec, report: &mut Report) {
    let id = component.id.as_str();
    if let (Some(p), Some(i), Some(cells)) = (
        val(&motor.p_max_w),
        val(&motor.i_max_cont_a),
        motor.cells_max.as_ref().map(|c| f64::from(c.value)),
    ) {
        let low = 0.8 * i * cells * CELL_V_NOMINAL;
        let high = 1.2 * i * cells * CELL_V_FULL;
        if p < low || p > high {
            report.warn(
                id,
                format!(
                    "p_max {p:.0} W is outside {low:.0}..{high:.0} W implied by i_max_cont x cells_max"
                ),
            );
        }
    }
    if let (Some(cont), Some(burst)) = (val(&motor.i_max_cont_a), val(&motor.i_max_burst_a)) {
        if burst < cont {
            report.warn(id, "i_max_burst is below i_max_cont");
        }
    }
    if let (Some(m), Some(d), Some(l)) = (
        val(&motor.mass_kg),
        val(&motor.diameter_m),
        val(&motor.length_m),
    ) {
        let volume = std::f64::consts::FRAC_PI_4 * d * d * l;
        let density = m / volume;
        if !(1000.0..=9000.0).contains(&density) {
            report.warn(
                id,
                format!("mass over can volume is {density:.0} kg/m3, outside 1000..9000"),
            );
        }
    }
    if let (Some(min), Some(max)) = (&motor.cells_min, &motor.cells_max) {
        if min.value > max.value {
            report.warn(id, "cells_min exceeds cells_max");
        }
    }
}

fn esc_checks(component: &Component, esc: &EscSpec, report: &mut Report) {
    let id = component.id.as_str();
    if let (Some(cont), Some(burst)) = (val(&esc.i_cont_a), val(&esc.i_burst_a)) {
        if burst < cont {
            report.warn(id, "i_burst is below i_cont");
        }
    }
    if let (Some(lo), Some(hi)) = (val(&esc.v_min_v), val(&esc.v_max_v)) {
        if lo >= hi {
            report.warn(id, "v_min is not below v_max");
        }
    }
    if let (Some(lo), Some(hi)) = (val(&esc.bec_v_min_v), val(&esc.bec_v_max_v)) {
        if lo > hi {
            report.warn(id, "bec_v_min exceeds bec_v_max");
        }
    }
}

fn battery_checks(component: &Component, battery: &BatterySpec, report: &mut Report) {
    let id = component.id.as_str();
    if let (Some(cells), Some(v)) = (&battery.cells_s, val(&battery.v_nom_v)) {
        let expected = f64::from(cells.value) * CELL_V_NOMINAL;
        if !(0.8 * expected..=1.2 * expected).contains(&v) {
            report.warn(
                id,
                format!(
                    "v_nom {v} V is far from {expected:.1} V for {} cells",
                    cells.value
                ),
            );
        }
    }
    if let (Some(energy), Some(v), Some(q)) = (
        val(&battery.energy_j),
        val(&battery.v_nom_v),
        val(&battery.capacity_c),
    ) {
        let derived = v * q;
        if (energy - derived).abs() > 0.05 * derived {
            report.warn(
                id,
                format!(
                    "energy_j {energy:.0} J differs from v_nom x capacity {derived:.0} J by > 5 %"
                ),
            );
        }
    }
    if let (Some(cont), Some(burst)) = (val(&battery.i_cont_a), val(&battery.i_burst_a)) {
        if burst < cont {
            report.warn(id, "i_burst is below i_cont");
        }
    }
    if let (Some(lo), Some(hi)) = (val(&battery.v_min_v), val(&battery.v_max_v)) {
        if lo >= hi {
            report.warn(id, "v_min is not below v_max");
        }
    }
}
