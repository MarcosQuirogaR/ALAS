// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A clean-sheet brief's seat count as the target its cabin is sized to.
//!
//! For a registered aircraft, or the shipped reference brief, a named cabin
//! style fills whatever cabin the geometry provides and the passenger count
//! follows. A clean-sheet brief (see `alas_config::clean_sheet`) states the
//! seats it must carry, and its fuselage is sized to them, so the geometry
//! must not rewrite that count. Here the brief's count wins: the style still
//! supplies the seat geometry of each class (pitch, width, seats abreast)
//! and the class mix, the requested total is allocated over that mix, and
//! the cabin becomes an installed count cabin that every later payload,
//! mass and layout step honours as declared.

use alas_config::AlasConfig;

use super::presets::passenger_preset_mix;

/// Turn a clean-sheet passenger brief's cabin into a count cabin holding its
/// requested seats in the selected style. Returns whether it did; a
/// registered aircraft, the reference brief, a freighter, a cabin that
/// already declares counts or a non-positive target are left alone.
pub(super) fn apply_brief_seat_target(config: &mut AlasConfig) -> bool {
    let target = config.requirements.num_passengers;
    if config.requirements.aircraft_type != "passenger"
        || target <= 0
        || !config.derives_clean_sheet_start()
    {
        return false;
    }
    if config.cabin.passenger.class_mix_mode == "count" && config.cabin.passenger.total_seats() > 0
    {
        return false;
    }
    let style = config.requirements.cabin_preset.clone();
    let mix = if style == "Custom" {
        config.cabin.passenger.length_share_mix()
    } else {
        passenger_preset_mix(config, &style).unwrap_or_default()
    };
    let pax = &mut config.cabin.passenger;
    if !mix.is_empty() {
        pax.set_length_share_mix(&mix);
    }
    for class in [
        &mut pax.first,
        &mut pax.business,
        &mut pax.premium,
        &mut pax.economy,
    ] {
        class.count = 0;
    }
    pax.set_fixed_passenger_count(target);
    pax.class_mix_mode = "count".to_owned();
    config.requirements.num_passengers = config.cabin.passenger.total_seats();
    true
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn brief(seats: i64, style: &str) -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({
            "requirements": {
                "mtow_kg": 72_500.0, "num_passengers": seats,
                "cruise_mach": 0.785, "cabin_preset": style
            },
            "geometry": {"fuselage": {"diameter_m": 3.96}}
        }))
        .unwrap()
    }

    #[test]
    fn the_brief_count_wins_and_the_style_supplies_the_seat_geometry() {
        let mut config = brief(168, "Ryanair");
        assert!(apply_brief_seat_target(&mut config));
        let pax = &config.cabin.passenger;
        assert_eq!(pax.class_mix_mode, "count");
        assert_eq!(pax.total_seats(), 168);
        assert_eq!(pax.economy.count, 168);
        assert_eq!(config.requirements.num_passengers, 168);
        assert!((pax.economy.pitch_m - 0.7112).abs() < 1e-12);
        // Applying it again changes nothing.
        let before = config.clone();
        assert!(!apply_brief_seat_target(&mut config));
        assert_eq!(config, before);
    }

    #[test]
    fn a_multi_class_style_splits_the_count_and_keeps_its_total() {
        let mut config = brief(150, "Iberia");
        assert!(apply_brief_seat_target(&mut config));
        let pax = &config.cabin.passenger;
        assert_eq!(pax.total_seats(), 150);
        assert!(pax.business.count > 0 && pax.economy.count > pax.business.count);
    }

    #[test]
    fn registered_and_reference_cabins_keep_their_geometry_derived_counts() {
        let mut preset =
            AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        assert!(!apply_brief_seat_target(&mut preset));
        let mut reference = AlasConfig::default();
        assert!(!apply_brief_seat_target(&mut reference));
    }
}
