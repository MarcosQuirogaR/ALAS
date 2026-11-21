// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Enabling the upper-deck hump from the Sandbox.
//!
//! The hump needs a complete, strictly ordered set (height and four
//! stations): `FuselageConfig::upper_deck_hump` ignores a lone height, which
//! is why a bare 0.1 m "hump" showed nothing. Setting the height therefore
//! fills all five values, scaled to the aircraft from the B747 preset's
//! (cabin start 6.0 m, fuselage length 68.63 m, hump 0.85 m high from 5.4 m
//! to 29.5 m with the full-height crown from 11.25 m to 20.5 m).
//!
//! The hump is a law of the parametric fuselage: a fuselage lofted from an
//! embedded surface (`geometry.fuselage.loft`) ignores it, so enabling the
//! hump on such a preset first asks to switch the fuselage to parametric.

use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

use super::fields::{self, SandboxField};

/// The field whose enabling fills the whole hump.
pub const HEIGHT_ID: &str = "geometry.fuselage.hump_height_m";
const START_ID: &str = "geometry.fuselage.hump_start_x_m";
const CROWN_START_ID: &str = "geometry.fuselage.hump_crown_start_x_m";
const CROWN_END_ID: &str = "geometry.fuselage.hump_crown_end_x_m";
const END_ID: &str = "geometry.fuselage.hump_end_x_m";
const LOFT_POINTER: &str = "/geometry/fuselage/loft";

// B747 reference (alas-config presets::widebody::b747), metres.
const REF_LENGTH_M: f64 = 68.63;
const REF_CABIN_START_M: f64 = 6.0;
const REF_HEIGHT_M: f64 = 0.85;
const REF_START_M: f64 = 5.4;
const REF_FORE_FAIRING_M: f64 = 11.25 - 5.4;
const REF_CROWN_M: f64 = 20.5 - 11.25;
const REF_AFT_FAIRING_M: f64 = 29.5 - 20.5;
/// The hump stays clear of the tail: its end is at most this fraction of the
/// fuselage length.
const MAX_END_FRACTION: f64 = 0.95;
/// Upper bound of every hump station field (see `fields::hump`).
const STATION_MAX_M: f64 = 60.0;
/// Bounds of the hump height field.
const HEIGHT_RANGE_M: (f64, f64) = (0.1, 4.0);

/// A complete hump: height and the four ordered stations, metres aft of the
/// nose tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HumpSet {
    /// Crown rise, m.
    pub height_m: f64,
    /// Where the crown starts to rise.
    pub start_x_m: f64,
    /// Where the full height is reached.
    pub crown_start_x_m: f64,
    /// Where the aft fairing starts.
    pub crown_end_x_m: f64,
    /// Where the crown is back on the main crown.
    pub end_x_m: f64,
}

/// The B747 hump scaled to an aircraft: the start follows the cabin start,
/// the fairing and crown lengths and the height follow the fuselage length.
/// Strictly ordered for any positive inputs; the end is kept inside the
/// fuselage and the station field range by shortening the three lengths
/// together.
pub fn scaled_hump(cabin_start_x_m: f64, fuselage_length_m: f64) -> HumpSet {
    let length = if fuselage_length_m.is_finite() && fuselage_length_m > 0.0 {
        fuselage_length_m
    } else {
        REF_LENGTH_M
    };
    let cabin = if cabin_start_x_m.is_finite() && cabin_start_x_m > 0.0 {
        cabin_start_x_m
    } else {
        REF_CABIN_START_M * length / REF_LENGTH_M
    };
    let limit = (MAX_END_FRACTION * length).min(STATION_MAX_M);
    let start = (REF_START_M / REF_CABIN_START_M * cabin).min(0.5 * limit);
    let k = length / REF_LENGTH_M;
    let widths = [
        REF_FORE_FAIRING_M * k,
        REF_CROWN_M * k,
        REF_AFT_FAIRING_M * k,
    ];
    let fit = ((limit - start) / widths.iter().sum::<f64>()).min(1.0);
    let crown_start = start + widths[0] * fit;
    let crown_end = crown_start + widths[1] * fit;
    HumpSet {
        height_m: (REF_HEIGHT_M * k).clamp(HEIGHT_RANGE_M.0, HEIGHT_RANGE_M.1),
        start_x_m: start,
        crown_start_x_m: crown_start,
        crown_end_x_m: crown_end,
        end_x_m: crown_end + widths[2] * fit,
    }
}

fn field<'a>(fields: &'a [SandboxField], id: &str) -> Option<&'a SandboxField> {
    fields.iter().find(|f| f.id == id)
}

fn number(state: &AppState, id: &str) -> Option<f64> {
    let field = field(&state.sandbox.fields, id)?;
    fields::read_value(field, &state.config_values, &state.design_values).as_f64()
}

/// Whether the fuselage is lofted from an embedded surface.
fn is_lofted(state: &AppState) -> bool {
    state
        .config_values
        .pointer(LOFT_POINTER)
        .is_some_and(|loft| !loft.is_null())
}

/// The user ticked "Set" on the hump height: fill the whole hump, or first
/// ask to switch a lofted fuselage to parametric.
pub fn request_enable(state: &mut AppState) {
    if is_lofted(state) {
        state.sandbox.hump_confirm = true;
    } else {
        enable(state, false);
    }
}

/// Fill the hump (and, when `clear_loft`, switch the fuselage to parametric)
/// as one undo step. Stations the user already set as a complete ordered
/// set are kept.
pub fn enable(state: &mut AppState, clear_loft: bool) {
    let cabin = number(state, "geometry.fuselage.cabin_start_x_m").unwrap_or(0.0);
    let length = number(state, "design.fuselage_length_m").unwrap_or(0.0);
    let scaled = scaled_hump(cabin, length);
    let kept: Vec<f64> = [START_ID, CROWN_START_ID, CROWN_END_ID, END_ID]
        .iter()
        .filter_map(|id| number(state, id))
        .collect();
    let keep_stations = kept.len() == 4 && kept.windows(2).all(|w| w[0] < w[1]);
    let mut values = vec![(HEIGHT_ID, scaled.height_m)];
    if !keep_stations {
        values.extend([
            (START_ID, scaled.start_x_m),
            (CROWN_START_ID, scaled.crown_start_x_m),
            (CROWN_END_ID, scaled.crown_end_x_m),
            (END_ID, scaled.end_x_m),
        ]);
    }
    let before = state.edit_snapshot();
    state.sandbox.undo.record(before);
    if clear_loft {
        if let Some(slot) = state.config_values.pointer_mut(LOFT_POINTER) {
            *slot = Value::Null;
        }
    }
    let all = state.sandbox.fields.clone();
    for (id, value) in values {
        let Some(target) = field(&all, id) else {
            continue;
        };
        // Round to the field's decimals so the editor shows what is stored.
        let scale = 10f64.powi(target.decimals as i32);
        let value = (value * scale).round() / scale;
        fields::write_value(
            target,
            Value::from(value),
            &mut state.config_values,
            &mut state.design_values,
        );
    }
    state.on_sandbox_model_changed();
    state.note_parameter_modified(
        tr("Upper-deck hump"),
        tr("Enabled with stations scaled from the B747 hump"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_b747_inputs_reproduce_the_b747_hump() {
        let h = scaled_hump(6.0, 68.63);
        assert!((h.height_m - 0.85).abs() < 1e-9);
        assert!((h.start_x_m - 5.4).abs() < 1e-9);
        assert!((h.crown_start_x_m - 11.25).abs() < 1e-9);
        assert!((h.crown_end_x_m - 20.5).abs() < 1e-9);
        assert!((h.end_x_m - 29.5).abs() < 1e-9);
    }

    #[test]
    fn the_stations_stay_strictly_ordered_and_inside_the_fuselage() {
        for (cabin, length) in [
            (6.0, 68.63),
            (0.5, 10.0),
            (40.0, 30.0),
            (25.0, 300.0),
            (f64::NAN, f64::NAN),
            (-1.0, 0.0),
        ] {
            let h = scaled_hump(cabin, length);
            assert!(
                h.start_x_m > 0.0
                    && h.start_x_m < h.crown_start_x_m
                    && h.crown_start_x_m < h.crown_end_x_m
                    && h.crown_end_x_m < h.end_x_m,
                "{cabin} {length}: {h:?}"
            );
            assert!(h.end_x_m <= STATION_MAX_M);
            if length.is_finite() && length > 0.0 {
                assert!(h.end_x_m < length, "{cabin} {length}: {h:?}");
            }
            assert!(h.height_m >= HEIGHT_RANGE_M.0 && h.height_m <= HEIGHT_RANGE_M.1);
        }
    }

    #[test]
    fn enabling_the_hump_on_the_ave_fills_a_hump_the_config_accepts() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        // The AVE reference is lofted: it asks first.
        request_enable(&mut state);
        assert!(state.sandbox.hump_confirm, "a lofted preset asks first");
        state.sandbox.hump_confirm = false;
        enable(&mut state, true);
        assert!(state.sandbox_undo());
        state.config_values["geometry"]["fuselage"]["loft"] = Value::Null;
        request_enable(&mut state);
        assert!(!state.sandbox.hump_confirm);
        let config = state.typed_config().expect("typed config");
        assert!(config.geometry.fuselage.upper_deck_hump().is_some());
        // One undo step takes the whole hump back.
        assert!(state.sandbox_undo());
        let config = state.typed_config().expect("typed config");
        assert!(config.geometry.fuselage.upper_deck_hump().is_none());
    }

    #[test]
    fn a_lofted_fuselage_asks_before_the_hump_is_applied() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state.config_values["geometry"]["fuselage"]["loft"] = serde_json::json!({ "id": "demo" });
        request_enable(&mut state);
        assert!(state.sandbox.hump_confirm, "the question is asked first");
        assert!(state.config_values["geometry"]["fuselage"]
            .get("hump_height_m")
            .is_none_or(Value::is_null));
        enable(&mut state, true);
        assert!(state.config_values["geometry"]["fuselage"]["loft"].is_null());
        assert!(state.config_values["geometry"]["fuselage"]["hump_height_m"].is_number());
    }
}
