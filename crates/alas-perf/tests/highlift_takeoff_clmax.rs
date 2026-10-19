// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The declared-high-lift CLmax_TO of the registered jets (fix lane CLmax-TO,
//! 2026-10-16): consistency with the Raymer Eq. 12.21 forms of
//! `alas_perf::performance::clmax`, and the landing value of the same method
//! against the Vref-recovered `cl_max_land` of each preset.
//!
//! The landing comparison is a plausibility guard, not a validation: the
//! Vref-recovered coefficients are effective values (see
//! `presets::landing_reference`), and the handbook method is a conceptual
//! estimate. The per-preset table is printed with `--nocapture`.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::presets;
use alas_perf::performance::clmax::raymer_flap_increment;

#[test]
fn the_declared_system_reproduces_the_raymer_increment_forms() {
    for system in presets::declared_systems() {
        let preset = presets::get(system.preset_name).unwrap();
        let parts = presets::preset_breakdown(preset).unwrap();
        // Same flap row through the shared closed form: dcl = k c'/c.
        let section = system.trailing_edge.section_increment_per_chord_ratio() * 1.275;
        // Recover the flapped-area/hinge product from the breakdown and
        // re-evaluate it through `raymer_flap_increment` with unit geometry.
        let unit = raymer_flap_increment(section, 1.0, 0.0);
        assert!(unit > 0.0);
        let area_cos = parts.flap_landing / unit;
        assert!(
            (0.2..0.9).contains(&area_cos),
            "{}: S_flapped/S cos(hinge) = {area_cos}",
            system.preset_name
        );
        // Take-off value: clean + share x landing flap increment + slat, the
        // share 0.70 unless the reference declares the take-off flap, and the
        // landing increment the published-speed coefficient less the slats-out,
        // flaps-up coefficient where the preset has one.
        let share = match system.takeoff_flap {
            Some(flap) => {
                let share = flap.increment_share().unwrap();
                assert!(
                    (0.3..0.95).contains(&share),
                    "{}: {share}",
                    system.preset_name
                );
                share
            }
            None => 0.7,
        };
        assert_eq!(parts.takeoff_share, share, "{}", system.preset_name);
        let increment = match parts.landing_anchor {
            Some(anchor) => anchor - parts.clean - parts.leading_edge,
            None => parts.flap_landing,
        };
        assert!(
            (parts.takeoff() - (parts.clean + share * increment + parts.leading_edge)).abs()
                < 1e-12
        );
        assert_eq!(
            parts.landing_anchor.is_some(),
            presets::published_landing_reference(system.preset_name).is_some(),
            "{}",
            system.preset_name
        );
    }
}

#[test]
fn the_landing_value_of_the_method_is_plausible_against_the_vref_recovered_coefficient() {
    let mut sum = 0.0;
    let mut count = 0.0;
    for system in presets::declared_systems() {
        let preset = presets::get(system.preset_name).unwrap();
        let parts = presets::preset_breakdown(preset).unwrap();
        let vref = preset.performance.as_ref().unwrap().cl_max_land;
        let error = parts.landing() / vref - 1.0;
        println!(
            "{:10} TO {:.3} landing {:.3} Vref-recovered {:.3} ({:+.1} %)",
            system.preset_name,
            parts.takeoff(),
            parts.landing(),
            vref,
            100.0 * error
        );
        assert!(error.abs() < 0.40, "{} {error}", system.preset_name);
        sum += error.abs();
        count += 1.0;
    }
    assert!(sum / count < 0.15, "mean |error| {}", sum / count);
}
