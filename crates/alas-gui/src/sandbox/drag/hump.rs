// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuselage station rows the handles share with the builder: the
//! generated stations' X fractions, the upper-deck hump's crown rise on the
//! generated rows, and the hump's own handles.
//!
//! The hump law is `alas_config::FuselageConfig::raise_crown`, the same one
//! `alas_geom::builder` lofts, so a drawn crown and a lofted one cannot
//! disagree. Hump handles sit on the raised crown line: the height handle at
//! the middle of the full-height crown moves up, the four station handles
//! (start, crown start, crown end, end) move along the body axis.

use alas_config::{AlasConfig, DesignVector, FuselageConfig, FuselageSection};

use super::{handle, Discipline, Handle, HandleKind};
use crate::state::AppState;

/// The X fraction of every generated fuselage station, nose to tail.
pub(super) fn generated_fuselage_station_fractions(state: &AppState) -> Vec<f64> {
    let (Some(config), Some(design)) = (state.typed_config(), state.current_design()) else {
        return Vec::new();
    };
    let fuselage = &config.geometry.fuselage;
    let length_m = design.fuselage_length_m;
    // Reject NaN explicitly: `length_m <= 0.0` alone is false for NaN, which
    // would fall through to station fraction math on garbage input.
    if !length_m.is_finite() || length_m <= 0.0 {
        return Vec::new();
    }
    let cabin_start = fuselage.cabin_start_x_m;
    let tailcone = fuselage.tailcone_length_m;
    let cabin_end = fuselage.aft_body_start_m(length_m);
    if !(cabin_start >= 0.0 && cabin_end >= cabin_start && tailcone >= 0.0) {
        return Vec::new();
    }
    let mut fractions = Vec::with_capacity(20);
    for index in 0..9 {
        let angle = std::f64::consts::FRAC_PI_2 * index as f64 / 9.0;
        let xi = 1.0 - angle.cos();
        fractions.push(fuselage.nose_station(xi).x_m / length_m);
    }
    fractions.push(cabin_start / length_m);
    fractions.push(cabin_end / length_m);
    for index in 1..10 {
        let xi = index as f64 / 9.0;
        fractions.push(fuselage.aft_body_station(xi, length_m).x_m / length_m);
    }
    fractions
}

/// Raise the crown of the generated rows by the hump, before any saved
/// override replaces them, as the builder does.
pub(super) fn raise_generated_rows<P>(
    fuselage: &FuselageConfig,
    length_m: f64,
    rows: &mut [(FuselageSection, P)],
) {
    for (section, _) in rows {
        let (z_m, height_m) =
            fuselage.raise_crown(section.x_fraction * length_m, section.z_m, section.height_m);
        section.z_m = z_m;
        section.height_m = height_m;
    }
}

/// The hump handles, when the fuselage has a hump.
pub(super) fn push_hump_handles(config: &AlasConfig, design: &DesignVector, out: &mut Vec<Handle>) {
    let fuselage = &config.geometry.fuselage;
    let Some(hump) = fuselage.upper_deck_hump() else {
        return;
    };
    let length_m = design.fuselage_length_m;
    let crown = |x: f64| {
        let base = fuselage.body_station(x, length_m);
        base.z_m + 0.5 * base.height_m + hump.crown_rise_m(x)
    };
    let middle = 0.5 * (hump.crown_start_x_m + hump.crown_end_x_m);
    out.push(handle(
        HandleKind::HumpHeight,
        Discipline::Fuselage,
        "geometry.fuselage.hump_height_m",
        "Upper-deck hump height",
        [middle, 0.0, crown(middle)],
        [0.0, 0.0, 1.0],
        1.0,
    ));
    for (field, label, x) in [
        (
            "geometry.fuselage.hump_start_x_m",
            "Hump start station",
            hump.start_x_m,
        ),
        (
            "geometry.fuselage.hump_crown_start_x_m",
            "Hump crown start station",
            hump.crown_start_x_m,
        ),
        (
            "geometry.fuselage.hump_crown_end_x_m",
            "Hump crown end station",
            hump.crown_end_x_m,
        ),
        (
            "geometry.fuselage.hump_end_x_m",
            "Hump end station",
            hump.end_x_m,
        ),
    ] {
        out.push(handle(
            HandleKind::HumpStation,
            Discipline::Fuselage,
            field,
            label,
            [x, 0.0, crown(x)],
            [1.0, 0.0, 0.0],
            1.0,
        ));
    }
}

// A failed expect in a test is the assertion failing on a registered preset.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_b747_hump_has_one_height_and_four_station_handles_on_its_crown() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "B747-400" }))
            .expect("B747-400 preset loads");
        let design = alas_config::presets::get("B747-400")
            .map(|preset| preset.design_vector)
            .expect("B747-400 preset resolves");
        let mut out = Vec::new();
        push_hump_handles(&config, &design, &mut out);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].field_id, "geometry.fuselage.hump_height_m");
        let f = &config.geometry.fuselage;
        let main_crown = f.cabin_z_m + 0.5 * f.effective_height_m();
        assert!((out[0].point[2] - main_crown - 0.85).abs() < 1e-9);
        // Start and end handles sit on the main crown line.
        assert!((out[4].point[2] - main_crown).abs() < 1e-9);
        // A body without a hump has no hump handles.
        let mut plain = Vec::new();
        push_hump_handles(&AlasConfig::default(), &design, &mut plain);
        assert!(plain.is_empty());
        // Every hump handle edits a sandbox field.
        for handle in &out {
            assert!(
                super::super::fields::inventory(&state.schema)
                    .iter()
                    .any(|field| field.id == handle.field_id),
                "{} is not a sandbox field",
                handle.field_id
            );
        }
    }

    #[test]
    fn a_b747_loaded_in_the_sandbox_exposes_editable_hump_fields_and_handles() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state.load_preset("B747-400");
        assert!(state.sandbox.active());
        assert!(!state.manual_geometry_locked());
        let fields = super::super::fields::inventory(&state.schema);
        let height = fields
            .iter()
            .find(|field| field.id == "geometry.fuselage.hump_height_m")
            .expect("hump height field");
        let current =
            super::super::fields::read_value(height, &state.config_values, &state.design_values);
        assert_eq!(current.as_f64(), Some(0.85));
        // Like the nose and diameter handles, they show in the fuselage focus.
        state.set_sandbox_focus(Some(Discipline::Fuselage));
        let handles = state.sandbox_handles();
        assert!(handles.iter().any(|h| h.kind == HandleKind::HumpHeight));
        assert_eq!(
            handles
                .iter()
                .filter(|h| h.kind == HandleKind::HumpStation)
                .count(),
            4
        );
        super::super::fields::write_value(
            height,
            serde_json::json!(1.2),
            &mut state.config_values,
            &mut state.design_values,
        );
        let config = state.typed_config().expect("edited configuration");
        let hump = config.geometry.fuselage.upper_deck_hump().expect("hump");
        assert_eq!(hump.height_m, 1.2);
    }
}
