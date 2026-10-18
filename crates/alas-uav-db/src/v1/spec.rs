// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-kind member mapping of the v1 converter.

use serde_json::{json, Map, Value};

use super::{text, Ctx};
use crate::error::{DbError, DbResult};
use crate::units::{si_symbol, to_si};

/// Put `spec[key]` (v1 unit `unit`) into `out[name]` converted to SI.
fn num(
    out: &mut Map<String, Value>,
    ctx: &Ctx,
    spec: &Value,
    key: &str,
    name: &str,
    unit: &str,
) -> DbResult<Option<f64>> {
    let Some(value) = spec.get(key).and_then(Value::as_f64) else {
        return Ok(None);
    };
    let (si, quantity) = to_si(value, unit)?;
    out.insert(
        name.into(),
        ctx.field(
            json!(si),
            si_symbol(quantity),
            format!("v1 {key}={value} {unit}"),
        ),
    );
    Ok(Some(si))
}

fn count(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value, key: &str, name: &str) {
    if let Some(value) = spec.get(key).and_then(Value::as_u64) {
        out.insert(
            name.into(),
            ctx.field(json!(value), "1", format!("v1 {key}={value}")),
        );
    }
}

fn string(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value, key: &str, name: &str) {
    if let Some(value) = text(spec, key) {
        out.insert(
            name.into(),
            ctx.field(json!(value), "1", format!("v1 {key}={value}")),
        );
    }
}

fn boolean(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value, key: &str, name: &str) {
    if let Some(value) = spec.get(key).and_then(Value::as_bool) {
        out.insert(
            name.into(),
            ctx.field(json!(value), "1", format!("v1 {key}={value}")),
        );
    }
}

fn strings(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value, key: &str, name: &str) {
    let items: Vec<&str> = spec
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !items.is_empty() {
        out.insert(
            name.into(),
            ctx.field(json!(items), "1", format!("v1 {key}={items:?}")),
        );
    }
}

/// v1 `dimensions` as `[length, width, height]` metres.
fn dims(spec: &Value) -> Option<[f64; 3]> {
    let d = spec.get("dimensions").filter(|d| d.is_object())?;
    Some([
        d.get("length_m")?.as_f64()?,
        d.get("width_m")?.as_f64()?,
        d.get("height_m")?.as_f64()?,
    ])
}

fn put_dims(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value, name: &str) {
    if let Some(d) = dims(spec) {
        out.insert(
            name.into(),
            ctx.field(json!(d), "m", format!("v1 dimensions={d:?} m")),
        );
    }
}

pub(super) fn convert_spec(
    id: &str,
    kind: &str,
    spec: &Value,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> DbResult<(&'static str, Map<String, Value>)> {
    let mut out = Map::new();
    let o = &mut out;
    let category = match kind {
        "motor" => {
            num(o, ctx, spec, "kv_rpm_per_v", "kv_rad_s_per_v", "rpm/V")?;
            num(o, ctx, spec, "winding_resistance_ohm", "rm_ohm", "Ohm")?;
            num(o, ctx, spec, "no_load_current_a", "i0_a", "A")?;
            num(o, ctx, spec, "no_load_test_voltage_v", "i0_test_v", "V")?;
            count(o, ctx, spec, "min_series_cells", "cells_min");
            count(o, ctx, spec, "max_series_cells", "cells_max");
            num(o, ctx, spec, "max_current_a", "i_max_burst_a", "A")?;
            num(o, ctx, spec, "max_power_w", "p_max_w", "W")?;
            num(
                o,
                ctx,
                spec,
                "max_static_thrust_n",
                "thrust_static_max_n",
                "N",
            )?;
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            num(
                o,
                ctx,
                spec,
                "recommended_propeller_diameter_m",
                "rec_prop_diameter_m",
                "m",
            )?;
            num(
                o,
                ctx,
                spec,
                "recommended_propeller_pitch_m",
                "rec_prop_pitch_m",
                "m",
            )?;
            if let Some([length, width, height]) = dims(spec) {
                if (length - width).abs() > 1e-9 {
                    warnings.push(format!(
                        "{id}: motor length_m != width_m; diameter taken from width_m"
                    ));
                }
                let raw = format!("v1 dimensions=[{length}, {width}, {height}] m");
                o.insert(
                    "diameter_m".into(),
                    ctx.field(json!(width), "m", raw.clone()),
                );
                o.insert("length_m".into(), ctx.field(json!(height), "m", raw));
            }
            "motor"
        }
        "esc" => {
            count(o, ctx, spec, "min_series_cells", "cells_min");
            count(o, ctx, spec, "max_series_cells", "cells_max");
            num(o, ctx, spec, "continuous_current_a", "i_cont_a", "A")?;
            num(o, ctx, spec, "bec_min_voltage_v", "bec_v_min_v", "V")?;
            num(o, ctx, spec, "bec_max_voltage_v", "bec_v_max_v", "V")?;
            num(
                o,
                ctx,
                spec,
                "bec_continuous_current_a",
                "bec_a_cont_a",
                "A",
            )?;
            num(o, ctx, spec, "bec_peak_current_a", "bec_a_peak_a", "A")?;
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            put_dims(o, ctx, spec, "dims_m");
            "esc"
        }
        "battery" => {
            string(o, ctx, spec, "chemistry", "chemistry");
            count(o, ctx, spec, "series_cells", "cells_s");
            num(o, ctx, spec, "nominal_voltage_v", "v_nom_v", "V")?;
            num(o, ctx, spec, "capacity_ah", "capacity_c", "Ah")?;
            num(
                o,
                ctx,
                spec,
                "discharge_rating_c",
                "claimed_c_rating",
                "1/h",
            )?;
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            put_dims(o, ctx, spec, "dims_m");
            string(o, ctx, spec, "connector", "connector");
            "battery"
        }
        "servo" => {
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            put_dims(o, ctx, spec, "dims_m");
            servo_curves(o, ctx, spec);
            "servo"
        }
        "propeller" => {
            num(o, ctx, spec, "diameter_m", "diameter_m", "m")?;
            num(o, ctx, spec, "pitch_m", "pitch_m", "m")?;
            count(o, ctx, spec, "blade_count", "blades");
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            num(o, ctx, spec, "bore_diameter_m", "hub_bore_m", "m")?;
            boolean(o, ctx, spec, "electric_compatible", "electric_compatible");
            "propeller"
        }
        "material_stock" => {
            string(o, ctx, spec, "form", "form");
            put_dims(o, ctx, spec, "stock_dims_m");
            num(o, ctx, spec, "mass_kg", "stock_mass_kg", "kg")?;
            num(o, ctx, spec, "youngs_modulus_pa", "e_pa", "Pa")?;
            num(o, ctx, spec, "allowable_stress_pa", "strength_pa", "Pa")?;
            "material"
        }
        "receiver" => {
            count(o, ctx, spec, "channel_count", "channels");
            num(o, ctx, spec, "min_voltage_v", "v_in_min_v", "V")?;
            num(o, ctx, spec, "max_voltage_v", "v_in_max_v", "V")?;
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            put_dims(o, ctx, spec, "dims_m");
            strings(o, ctx, spec, "protocols", "protocols");
            boolean(o, ctx, spec, "telemetry", "telemetry");
            "receiver"
        }
        "electronics" => {
            string(o, ctx, spec, "role", "role");
            num(o, ctx, spec, "min_voltage_v", "v_in_min_v", "V")?;
            num(o, ctx, spec, "max_voltage_v", "v_in_max_v", "V")?;
            num(o, ctx, spec, "max_current_a", "i_max_a", "A")?;
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            put_dims(o, ctx, spec, "dims_m");
            strings(o, ctx, spec, "protocols", "protocols");
            if text(spec, "role") == Some("flight_controller") {
                "flight_controller"
            } else {
                "electronics"
            }
        }
        "landing_gear" => {
            string(o, ctx, spec, "form", "form");
            num(o, ctx, spec, "mass_kg", "mass_kg", "kg")?;
            num(
                o,
                ctx,
                spec,
                "max_aircraft_mass_kg",
                "max_aircraft_mass_kg",
                "kg",
            )?;
            num(o, ctx, spec, "nominal_voltage_v", "nominal_voltage_v", "V")?;
            put_dims(o, ctx, spec, "dims_m");
            "landing_gear"
        }
        other => return Err(DbError::new(format!("unknown v1 kind '{other}'"))),
    };
    Ok((category, out))
}

fn servo_curves(out: &mut Map<String, Value>, ctx: &Ctx, spec: &Value) {
    let points: Vec<&Value> = spec
        .get("operating_points")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    if points.is_empty() {
        return;
    }
    let pick = |key: &str| -> (Vec<f64>, Vec<f64>) {
        points
            .iter()
            .filter_map(|p| Some((p.get("voltage_v")?.as_f64()?, p.get(key)?.as_f64()?)))
            .unzip()
    };
    let (vx, torque) = pick("stall_torque_nm");
    out.insert(
        "torque_vs_voltage".into(),
        ctx.curve(
            "voltage",
            "V",
            "N*m",
            vx,
            torque,
            "v1 operating_points.stall_torque_nm".into(),
        ),
    );
    let (vx, seconds) = pick("seconds_per_60_deg");
    if !vx.is_empty() {
        let speed = seconds
            .iter()
            .map(|s| std::f64::consts::FRAC_PI_3 / s)
            .collect();
        out.insert(
            "speed_vs_voltage".into(),
            ctx.curve(
                "voltage",
                "V",
                "rad/s",
                vx,
                speed,
                "v1 operating_points.seconds_per_60_deg (pi/3 / t)".into(),
            ),
        );
    }
    let (vx, current) = pick("stall_current_a");
    if !vx.is_empty() {
        out.insert(
            "i_stall_vs_voltage".into(),
            ctx.curve(
                "voltage",
                "V",
                "A",
                vx,
                current,
                "v1 operating_points.stall_current_a".into(),
            ),
        );
    }
}
