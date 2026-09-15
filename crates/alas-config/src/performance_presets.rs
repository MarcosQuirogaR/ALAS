// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/performance_presets.py
// Reference: alas @ rust-port-baseline.

//! Named high-lift and propulsion technology levels for the field-performance
//! model.
//!
//! What a wing can be made to do on approach is a property of its flaps and
//! slats, not of its planform, and the vortex-lattice analysis cannot see a
//! deployed high-lift system at all. So the maximum lift coefficients, the
//! thrust lapse, the engine-out climb factors and the balanced-field factor
//! are assumptions, and these presets bundle each set of them under the
//! technology level it describes.
//!
//! Getting that bundle right matters more than it looks: a widebody scored
//! against a regional jet's high-lift system overestimates its takeoff and
//! landing speeds by fifteen to twenty knots, which sizes it out of runways it
//! would in fact operate from.
//!
//! Every preset leaves the matching chart's axis limits and its plotting
//! resolution at their defaults, since those describe a figure and not an
//! aircraft. This is deliberately unrelated to [`crate::fidelity_presets`],
//! which is about how finely the analysis is resolved rather than what the
//! field-performance model assumes.

use std::sync::OnceLock;

use crate::PerformanceConfig;

/// A performance preset that was asked for and is not registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown performance preset '{name}'; available: {}", available.join(", "))]
pub struct UnknownPerformancePreset {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// A named bundle of field-performance and high-lift assumptions.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformancePreset {
    /// The key the configuration selects it by.
    pub name: &'static str,
    /// What the interface calls it.
    pub display_name: &'static str,
    /// What choosing it means, in one or two sentences.
    pub description: &'static str,
    /// The performance configuration it describes.
    pub settings: PerformanceConfig,
}

/// Every registered preset, in the order the interface lists them.
pub fn registry() -> &'static [PerformancePreset] {
    static REGISTRY: OnceLock<Vec<PerformancePreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// Look one preset up by name.
///
/// # Errors
///
/// [`UnknownPerformancePreset`], carrying what is available. Upstream raises
/// for the same input.
pub fn get(name: &str) -> Result<&'static PerformancePreset, UnknownPerformancePreset> {
    registry()
        .iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| UnknownPerformancePreset {
            name: name.to_owned(),
            available: sorted_names(),
        })
}

/// Every preset's name, in registration order.
pub fn available() -> Vec<&'static str> {
    registry().iter().map(|preset| preset.name).collect()
}

/// Every preset's name paired with what to call it, in registration order.
pub fn display_names() -> Vec<(&'static str, &'static str)> {
    registry()
        .iter()
        .map(|preset| (preset.name, preset.display_name))
        .collect()
}

fn sorted_names() -> Vec<String> {
    let mut names: Vec<String> = registry()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    names.sort();
    names
}

fn build() -> Vec<PerformancePreset> {
    vec![
        PerformancePreset {
            name: "conservative_simple_flaps",
            display_name: "Conservative (simple flaps)",
            description: "Older-generation / regional-jet high-lift system (single/double-slotted \
                          flaps, no slats). Lower CLmax, gentler thrust lapse.",
            settings: PerformanceConfig {
                cl_max_to: 1.60,
                cl_max_land: 2.20,
                thrust_lapse: 0.20,
                oei_gradient: 0.024,
                k_land: 0.58,
                oei_climb_cl: 1.0,
                oei_climb_delta_cd: 0.020,
                bfl_factor: 1.15,
                ..PerformanceConfig::default()
            },
        },
        PerformancePreset {
            name: "standard_narrowbody",
            display_name: "Standard Narrow-body (Recommended)",
            description: "Typical modern narrow-body twin: the PerformanceConfig defaults.",
            // Whatever `PerformanceConfig` itself defaults to, so the
            // recommended preset and an unconfigured run are the same run.
            settings: PerformanceConfig::default(),
        },
        PerformancePreset {
            name: "modern_narrowbody",
            display_name: "Modern Narrow-body (slats + Fowler flaps)",
            description:
                "Modern single-aisle twin with leading-edge slats and single/double-slotted \
                          Fowler flaps (e.g. A320/A220 family): higher CLmax than the generic \
                          'standard narrow-body' bucket, which undersells this common, \
                          well-documented high-lift system and overestimates V-speeds by ~15-20 kt.",
            settings: PerformanceConfig {
                cl_max_to: 2.10,
                cl_max_land: 2.90,
                ..PerformanceConfig::default()
            },
        },
        PerformancePreset {
            name: "advanced_highlift_widebody",
            display_name: "Advanced High-Lift (widebody)",
            description: "Modern widebody with triple-slotted flaps + slats and high-bypass \
                          engines. Higher CLmax, steeper OEI climb requirement (tri/quad-class \
                          margin).",
            settings: PerformanceConfig {
                cl_max_to: 2.10,
                cl_max_land: 2.95,
                thrust_lapse: 0.26,
                oei_gradient: 0.027,
                k_land: 0.63,
                oei_climb_cl: 1.35,
                oei_climb_delta_cd: 0.030,
                bfl_factor: 1.18,
                ..PerformanceConfig::default()
            },
        },
    ]
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_lands_more_lift_than_it_takes_off_with() {
        // Landing flap is a larger deflection than takeoff flap on every
        // transport high-lift system there is.
        for preset in registry() {
            assert!(
                preset.settings.cl_max_land > preset.settings.cl_max_to,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn a_more_advanced_high_lift_system_reaches_a_higher_lift_coefficient() {
        // The registry is a technology ladder, and a rung that did not climb
        // would be selectable without meaning anything.
        let simple = get("conservative_simple_flaps").unwrap();
        let standard = get("standard_narrowbody").unwrap();
        let modern = get("modern_narrowbody").unwrap();
        let widebody = get("advanced_highlift_widebody").unwrap();

        assert!(simple.settings.cl_max_land < standard.settings.cl_max_land);
        assert!(standard.settings.cl_max_land < modern.settings.cl_max_land);
        assert!(modern.settings.cl_max_land < widebody.settings.cl_max_land);
    }

    #[test]
    fn no_preset_moves_the_matching_charts_axes_or_its_plotting_resolution() {
        // Those describe a figure, not an aircraft; a preset that changed
        // them would redraw the chart while claiming to have changed the
        // high-lift system.
        let default = PerformanceConfig::default();
        for preset in registry() {
            assert_eq!(
                preset.settings.ws_min_pa, default.ws_min_pa,
                "{}",
                preset.name
            );
            assert_eq!(
                preset.settings.ws_max_pa, default.ws_max_pa,
                "{}",
                preset.name
            );
            assert_eq!(
                preset.settings.matching_chart_resolution, default.matching_chart_resolution,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn no_preset_moves_the_speed_schedule_the_regulation_fixes() {
        // The V-speed factors come from the certification rule, not from the
        // flaps, so they are the same for every technology level.
        let default = PerformanceConfig::default();
        for preset in registry() {
            assert_eq!(preset.settings.v2_vstall_factor, default.v2_vstall_factor);
            assert_eq!(preset.settings.vr_vstall_factor, default.vr_vstall_factor);
            assert_eq!(
                preset.settings.vapp_vstall_land_factor,
                default.vapp_vstall_land_factor
            );
        }
    }

    #[test]
    fn the_recommended_preset_is_what_an_unconfigured_run_already_does() {
        assert_eq!(
            get("standard_narrowbody").unwrap().settings,
            PerformanceConfig::default()
        );
    }

    #[test]
    fn an_unknown_preset_is_an_error_that_says_what_there_is() {
        let error = get("blown_flaps").unwrap_err();
        assert!(error.available.contains(&"standard_narrowbody".to_owned()));
        assert!(error.to_string().contains("standard_narrowbody"));
    }

    #[test]
    fn the_dropdown_lists_the_presets_in_registration_order() {
        assert_eq!(
            available(),
            vec![
                "conservative_simple_flaps",
                "standard_narrowbody",
                "modern_narrowbody",
                "advanced_highlift_widebody",
            ]
        );
    }
}
