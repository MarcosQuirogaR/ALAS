// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build powertrain models from `alas-uav-db` rows.
//!
//! A required field that is absent is reported as [`PowerError::Missing`];
//! nothing is defaulted silently.  The two estimates that are not database
//! fields are the motor heat capacity (`C_th = 450 J/(kg K) x mass`, a lumped
//! stator/winding/housing estimate between copper 385 and aluminium 900) and
//! the ESC nominal efficiency passed by the caller.

use alas_uav_db::parts::{BatterySpec, EscSpec, MotorSpec, PropellerSpec};
use alas_uav_db::{Curve, Num, PerfKind, PropPerfSet};

use crate::propulsion_electric::apc_performance_map;

use super::battery::BatteryModel;
use super::bemt::BemtGeometry;
use super::esc::EscModel;
use super::motor::{MotorLimits, MotorModel, MotorThermal};
use super::polar::AnalyticPolar;
use super::prop::PropModel;
use super::prop_map::PropMapTable;
use super::PowerError;

/// Lumped specific heat used to turn motor mass into a heat capacity, J/(kg K).
pub const MOTOR_SPECIFIC_HEAT: f64 = 450.0;

fn need(field: &Option<Num>, what: &str, name: &'static str) -> Result<f64, PowerError> {
    field
        .as_ref()
        .map(|f| f.value)
        .ok_or_else(|| PowerError::Missing {
            what: what.to_owned(),
            field: name,
        })
}

fn val(field: &Option<Num>) -> Option<f64> {
    field.as_ref().map(|f| f.value)
}

/// Motor model from a database motor (`id` is used in error messages).
///
/// # Errors
///
/// [`PowerError::Missing`] without Kv, Rm, I0 and the I0 test voltage.
pub fn motor_from_spec(spec: &MotorSpec, id: &str) -> Result<MotorModel, PowerError> {
    let mut motor = MotorModel::new(
        need(&spec.kv_rad_s_per_v, id, "kv_rad_s_per_v")?,
        need(&spec.rm_ohm, id, "rm_ohm")?,
        need(&spec.i0_a, id, "i0_a")?,
        need(&spec.i0_test_v, id, "i0_test_v")?,
    )?;
    if let Some(curve) = &spec.i0_vs_omega {
        motor = motor.with_i0_table(curve.x.clone(), curve.y.clone())?;
    }
    if let (Some(r_th), Some(mass)) = (val(&spec.thermal_r_k_per_w), val(&spec.mass_kg)) {
        motor.thermal = Some(MotorThermal {
            r_th_k_per_w: r_th,
            c_th_j_per_k: MOTOR_SPECIFIC_HEAT * mass,
        });
    }
    motor.limits = MotorLimits {
        i_cont_a: val(&spec.i_max_cont_a),
        i_burst_a: val(&spec.i_max_burst_a),
        burst_s: val(&spec.burst_s),
        p_max_w: val(&spec.p_max_w),
        omega_max_rad_s: val(&spec.omega_max_rad_s),
        t_winding_max_k: val(&spec.winding_temp_max_k),
    };
    Ok(motor)
}

/// Battery model from a database pack.  Needs capacity, series count and the
/// pack DC resistance; uses `voc_vs_soc` and `r_dc_vs_soc_t` when present.
///
/// # Errors
///
/// [`PowerError::Missing`] for an absent required field.
pub fn battery_from_spec(spec: &BatterySpec, id: &str) -> Result<BatteryModel, PowerError> {
    let capacity = need(&spec.capacity_c, id, "capacity_c")?;
    let cells = spec
        .cells_s
        .as_ref()
        .map(|f| f.value)
        .ok_or_else(|| PowerError::Missing {
            what: id.to_owned(),
            field: "cells_s",
        })?;
    let mut battery = BatteryModel::new(capacity, cells, need(&spec.r_dc_ohm, id, "r_dc_ohm")?)?;
    if let Some(Curve { x, y, .. }) = &spec.voc_vs_soc {
        battery = battery.with_voc_table(x.clone(), y.clone())?;
    }
    for curve in &spec.r_dc_vs_soc_t {
        if let Some(p) = curve.param.as_ref().filter(|p| p.name == "temp") {
            battery = battery.with_r_curve(p.value, curve.x.clone(), curve.y.clone())?;
        }
    }
    if let Some(v) = val(&spec.v_min_v) {
        battery.v_cutoff_v = v;
    }
    battery.mass_kg = val(&spec.mass_kg).unwrap_or(0.0);
    battery.i_cont_a = val(&spec.i_cont_a);
    Ok(battery)
}

/// ESC model from a database ESC and a nominal efficiency at the reference
/// pack voltage `v_ref` (a caller's estimate unless the database carries
/// `efficiency_vs_current`, whose value at the continuous rating is used).
///
/// # Errors
///
/// [`PowerError::Missing`] without a continuous or burst current rating.
pub fn esc_from_spec(
    spec: &EscSpec,
    id: &str,
    eta_nominal: f64,
    v_ref: f64,
) -> Result<EscModel, PowerError> {
    let i_ref = val(&spec.i_cont_a)
        .or_else(|| val(&spec.i_burst_a).map(|i| 0.5 * i))
        .ok_or_else(|| PowerError::Missing {
            what: id.to_owned(),
            field: "i_cont_a",
        })?;
    let eta = spec
        .efficiency_vs_current
        .as_ref()
        .and_then(|c| {
            (c.x.len() == c.y.len() && !c.x.is_empty()).then(|| interp_clamped(&c.x, &c.y, i_ref))
        })
        .unwrap_or(eta_nominal);
    let mut esc = EscModel::from_nominal_efficiency(eta, v_ref, i_ref)?;
    esc.i_cont_a = val(&spec.i_cont_a);
    esc.i_burst_a = val(&spec.i_burst_a);
    Ok(esc)
}

fn interp_clamped(x: &[f64], y: &[f64], q: f64) -> f64 {
    let n = x.len();
    if q <= x[0] || n == 1 {
        return y[0];
    }
    if q >= x[n - 1] {
        return y[n - 1];
    }
    let i = x.partition_point(|&v| v <= q) - 1;
    y[i] + (q - x[i]) / (x[i + 1] - x[i]) * (y[i + 1] - y[i])
}

/// Choose the propeller model: measured map, installed PER3 table (read
/// through the fetch-on-demand reader; `per3_id` is its catalogue id such as
/// `apc-10x5e`), computed map, BEMT on database geometry, then BEMT on a
/// generic blade built from diameter and pitch.
///
/// # Errors
///
/// [`PowerError::Missing`] when none of the above can be built.
pub fn resolve_propeller(
    id: &str,
    spec: &PropellerSpec,
    maps: &[&PropPerfSet],
    per3_id: Option<&str>,
) -> Result<PropModel, PowerError> {
    let diameter = val(&spec.diameter_m);
    let from_set = |kind: PerfKind| -> Option<Result<PropModel, PowerError>> {
        let set = maps.iter().find(|s| s.kind == kind)?;
        Some(
            diameter
                .ok_or_else(|| PowerError::Missing {
                    what: id.to_owned(),
                    field: "diameter_m",
                })
                .and_then(|d| PropMapTable::from_set(set, d))
                .map(|t| PropModel::from_map(t, kind == PerfKind::Measured)),
        )
    };
    if let Some(model) = from_set(PerfKind::Measured) {
        return model;
    }
    if let Some(table) = per3_id.and_then(|p| apc_performance_map(p).ok()) {
        return Ok(PropModel::from_per3(table.clone()));
    }
    if let Some(model) = from_set(PerfKind::Computed) {
        return model;
    }
    let blades = spec.blades.as_ref().map_or(2, |b| b.value);
    if let Some(g) = &spec.geometry {
        let geometry = BemtGeometry::new(
            blades,
            g.r_m.clone(),
            g.chord_m.clone(),
            g.twist_rad.clone(),
            std::sync::Arc::new(AnalyticPolar::default()),
        )?;
        return Ok(PropModel::from_bemt(geometry));
    }
    let d = diameter.ok_or_else(|| PowerError::Missing {
        what: id.to_owned(),
        field: "diameter_m",
    })?;
    let pitch = need(&spec.pitch_m, id, "pitch_m")?;
    Ok(PropModel::from_bemt(BemtGeometry::generic(
        d, pitch, blades, 0.15,
    )?))
}
