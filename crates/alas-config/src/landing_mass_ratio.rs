// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The class-level default maximum-landing-to-maximum-takeoff mass ratio.
//!
//! [`crate::MassModelConfig::mlw_fraction_mtow`] is only a default for
//! configurations with no certified MLW/MTOW pair: a registered aircraft
//! replaces it with its own certified ratio when it loads (see
//! [`crate::AlasConfig`]). For the others, the default follows the declared
//! operating haul class ([`crate::OperatingHaulClass`], the class selector
//! the FLOPS/LTH operating-item relations already read), rather than a mass
//! threshold this project would have to invent. FLOPS sizes the main gear on
//! the design landing mass (`WLDG^0.95`), so this ratio sets the gear mass of
//! a notional or clean-sheet aircraft.

use crate::{AlasConfig, FlopsTransportConfig, OperatingHaulClass};

/// Long-haul default MLW/MTOW, dimensionless: the AVE's benchmark aircraft.
///
/// Boeing, *777-9 Airplane Characteristics for Airport Planning*, D6-86073
/// Revision G (September 2025, preliminary information), Table 2-1: maximum
/// design landing weight 587,000 lb (266,258 kg) over maximum design takeoff
/// weight 775,000 lb (351,534 kg), 0.7574. It sits inside the certified
/// spread of the registry's long-haul presets (A380-800 0.689, A340-300
/// 0.723, DC-10 0.736, B787-9 0.757).
pub const LONG_HAUL_MLW_FRACTION_MTOW: f64 = 266_258.0 / 351_534.0;

/// Short/medium-haul default MLW/MTOW, dimensionless: a class statistic.
///
/// The arithmetic mean of the certified MLW/MTOW of the registry's
/// short/medium-haul jet transports, each with the TCDS or airport-planning
/// provenance of its preset reference block: A320-200 66,000 / 78,000 kg
/// (0.8462) and A220-300 58,740 / 67,585 kg (0.8691), mean 0.8576.
///
/// The ATR 72-600 (22,350 / 23,000 kg, 0.9717) shares the haul class but is
/// excluded: a regional turboprop flying short sectors is certified with a
/// landing mass close to its takeoff mass, it takes its own regional
/// turboprop mass method, and including it would raise the jet value by
/// 0.04 (to 0.8957). A turboprop configuration without a certified pair
/// still falls back to this jet statistic; no turboprop class default is
/// declared.
pub const SHORT_MEDIUM_HAUL_MLW_FRACTION_MTOW: f64 =
    (66_000.0 / 78_000.0 + 58_740.0 / 67_585.0) / 2.0;

/// The default MLW/MTOW of an aircraft in `class`.
#[must_use]
pub const fn default_mlw_fraction_mtow(class: OperatingHaulClass) -> f64 {
    match class {
        OperatingHaulClass::ShortMediumHaul => SHORT_MEDIUM_HAUL_MLW_FRACTION_MTOW,
        OperatingHaulClass::LongHaul => LONG_HAUL_MLW_FRACTION_MTOW,
    }
}

/// The default of the unconfigured mass model, whose working FLOPS inputs
/// declare the long-haul AVE scenario.
#[must_use]
pub fn working_default_mlw_fraction_mtow() -> f64 {
    default_mlw_fraction_mtow(
        FlopsTransportConfig::working_default()
            .haul_class
            .unwrap_or_default(),
    )
}

/// Re-derive the default ratio from the loaded haul class.
///
/// Applies only when the loaded document states no `mlw_fraction_mtow` of
/// its own and its preset, if any, declares no certified MLW: an explicit
/// value and a certified ratio are both left untouched. A missing haul
/// class reads as short/medium-haul, as the FLOPS buildup reads it.
fn apply_class_default(loaded: &mut AlasConfig, data: &serde_json::Value) {
    let explicit = data
        .get("mass_model")
        .and_then(|mass_model| mass_model.get("mlw_fraction_mtow"))
        .is_some();
    let certified =
        crate::presets::get(&loaded.preset).is_ok_and(|preset| preset.reference.mlw_kg.is_some());
    if !explicit && !certified {
        let class = loaded
            .mass_model
            .flops_transport
            .haul_class
            .unwrap_or_default();
        loaded.mass_model.mlw_fraction_mtow = default_mlw_fraction_mtow(class);
    }
}

/// The last mass-model steps of loading a document: the class-default
/// landing ratio ([`apply_class_default`]), then the architecture
/// reconciliation [`crate::MassModelConfig::normalize_architecture`], whose
/// migration report is returned.
pub(crate) fn finish_loaded_mass_model(
    loaded: &mut AlasConfig,
    data: &serde_json::Value,
) -> crate::MassArchitectureMigration {
    apply_class_default(loaded, data);
    loaded.mass_model.normalize_architecture()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load(value: serde_json::Value) -> AlasConfig {
        AlasConfig::from_value(&value).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn the_long_haul_default_is_the_777_9_table_value() {
        // Hand check against D6-86073 Rev G Table 2-1: the pound ratio and
        // the kilogram ratio agree to the table's rounding, which pins the
        // column pairing of the extracted table.
        assert!((LONG_HAUL_MLW_FRACTION_MTOW - 587_000.0 / 775_000.0).abs() < 1.0e-5);
        assert!((LONG_HAUL_MLW_FRACTION_MTOW - 0.7574).abs() < 5.0e-5);
    }

    #[test]
    fn each_class_default_is_the_statistic_of_the_registered_presets_of_that_class() {
        // Recomputed from the registry: every preset declaring both masses,
        // grouped by its declared haul class.
        let mut short = Vec::new();
        let mut long = Vec::new();
        for preset in crate::presets::registry() {
            let (Some(mlw), Some(mtow)) = (preset.reference.mlw_kg, preset.reference.mtow_kg)
            else {
                continue;
            };
            let class = crate::preset_flops::inputs_for(preset.name)
                .and_then(|inputs| inputs.transport.haul_class)
                .unwrap_or_default();
            let turboprop = crate::engines::get(preset.engine_name)
                .is_ok_and(|engine| engine.technology == crate::PropulsionTechnology::Turboprop);
            match class {
                // The regional turboprop is excluded from the jet statistic.
                OperatingHaulClass::ShortMediumHaul if turboprop => {}
                OperatingHaulClass::ShortMediumHaul => short.push(mlw / mtow),
                OperatingHaulClass::LongHaul => long.push(mlw / mtow),
            }
        }
        assert_eq!(short.len(), 2, "A320-200 and A220-300");
        let mean = short.iter().sum::<f64>() / short.len() as f64;
        assert!((SHORT_MEDIUM_HAUL_MLW_FRACTION_MTOW - mean).abs() < 1.0e-12);
        assert!((SHORT_MEDIUM_HAUL_MLW_FRACTION_MTOW - 0.8576).abs() < 5.0e-5);
        let low = long.iter().copied().fold(f64::INFINITY, f64::min);
        let high = long.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!((low..=high + 1.0e-3).contains(&LONG_HAUL_MLW_FRACTION_MTOW));
    }

    #[test]
    fn a_blank_configuration_takes_the_default_of_its_declared_class() {
        // The unconfigured defaults declare the long-haul AVE scenario.
        let blank = load(json!({}));
        assert_eq!(
            blank.mass_model.mlw_fraction_mtow,
            LONG_HAUL_MLW_FRACTION_MTOW
        );
        assert_eq!(
            crate::MassModelConfig::default().mlw_fraction_mtow,
            LONG_HAUL_MLW_FRACTION_MTOW
        );
        let short = load(json!({
            "mass_model": {"flops_transport": {"haul_class": "short_medium_haul"}}
        }));
        assert_eq!(
            short.mass_model.mlw_fraction_mtow,
            SHORT_MEDIUM_HAUL_MLW_FRACTION_MTOW
        );
        // An explicit ratio always wins over the class default.
        let explicit = load(json!({
            "mass_model": {
                "mlw_fraction_mtow": 0.8,
                "flops_transport": {"haul_class": "short_medium_haul"}
            }
        }));
        assert_eq!(explicit.mass_model.mlw_fraction_mtow, 0.8);
    }

    #[test]
    fn ave_takes_the_long_haul_default_and_real_presets_keep_their_certified_ratio() {
        let ave = load(json!({"preset": "AVE"}));
        assert_eq!(
            ave.mass_model.mlw_fraction_mtow,
            LONG_HAUL_MLW_FRACTION_MTOW
        );
        for (name, mlw, mtow) in [
            ("A320-200", 66_000.0, 78_000.0),
            ("A380-800", 386_000.0, 560_000.0),
            ("ATR72-600", 22_350.0, 23_000.0),
        ] {
            let config = load(json!({ "preset": name }));
            assert!(
                (config.mass_model.mlw_fraction_mtow - mlw / mtow).abs() < 1.0e-12,
                "{name}"
            );
        }
    }
}
