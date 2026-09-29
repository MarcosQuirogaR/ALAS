// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `LandingGearConfig::validation_errors`: source-backed gear geometry and
//! rotation-criterion checks, kept apart from the configuration types.

use super::LandingGearConfig;

impl LandingGearConfig {
    /// Return configuration errors that concern source-backed gear geometry.
    ///
    /// The global configuration validator turns these `(field, message)`
    /// pairs into blocking `ValidationIssue`s. Keeping the shape checks here
    /// makes the typed list contract available to callers that only own a
    /// landing-gear configuration, while the central validator still catches
    /// malformed values before a run starts.
    pub fn validation_errors(&self) -> Vec<(String, String)> {
        let mut errors = Vec::new();

        if let Some(value) = self.reference_wheelbase_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_wheelbase_m".to_owned(),
                    format!(
                        "Reference wheelbase ({value:?} m) must be finite and greater than zero; it is a comparison datum only and cannot define absolute gear stations."
                    ),
                ));
            }
        }

        if let Some(value) = self.reference_body_wheelbase_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_body_wheelbase_m".to_owned(),
                    format!(
                        "Reference body-gear wheelbase ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }

        let station_fields_present = self.reference_station_fuselage_length_m.is_some()
            || self.reference_nlg_x_fraction.is_some()
            || self.reference_mlg_x_fractions.is_some();
        let station_fields_complete = self.reference_station_fuselage_length_m.is_some()
            && self.reference_nlg_x_fraction.is_some()
            && self.reference_mlg_x_fractions.is_some();
        if station_fields_present && !station_fields_complete {
            errors.push((
                "landing_gear.reference_station".to_owned(),
                "Normalized source stations require reference_station_fuselage_length_m, reference_nlg_x_fraction, and reference_mlg_x_fractions together.".to_owned(),
            ));
        }
        if let Some(value) = self.reference_station_fuselage_length_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_station_fuselage_length_m".to_owned(),
                    format!(
                        "Reference station fuselage length ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }
        if let Some(value) = self.reference_nlg_x_fraction {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                errors.push((
                    "landing_gear.reference_nlg_x_fraction".to_owned(),
                    format!(
                        "Reference nose-gear station fraction ({value:?}) must be finite and lie between zero and one."
                    ),
                ));
            }
        }
        if let Some(fractions) = &self.reference_mlg_x_fractions {
            let expected = if self.n_mlg_struts > 0 {
                Some(self.n_mlg_struts as usize)
            } else {
                None
            };
            let length_is_valid = expected.map_or(matches!(fractions.len(), 2..=4), |expected| {
                fractions.len() == expected
            });
            if !length_is_valid {
                let expected_text = expected.map_or_else(
                    || "2, 3, or 4 for automatic strut count".to_owned(),
                    |value| value.to_string(),
                );
                errors.push((
                    "landing_gear.reference_mlg_x_fractions".to_owned(),
                    format!(
                        "Normalized source main-gear station list has {} entries; expected {expected_text}.",
                        fractions.len()
                    ),
                ));
            }
            for (index, &value) in fractions.iter().enumerate() {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    errors.push((
                        format!("landing_gear.reference_mlg_x_fractions[{index}]"),
                        format!(
                            "Reference main-gear station fraction ({value:?}) must be finite and lie between zero and one."
                        ),
                    ));
                }
            }
        }

        if let Some(value) = self.reference_track_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_track_m".to_owned(),
                    format!(
                        "Reference main-gear track ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }

        if let Some(counts) = &self.mlg_strut_bogie_wheels {
            let expected = if self.n_mlg_struts > 0 {
                Some(self.n_mlg_struts as usize)
            } else {
                None
            };
            let length_is_valid = expected.map_or(matches!(counts.len(), 2 | 4), |expected| {
                counts.len() == expected
            });
            if !length_is_valid {
                let expected_text = expected.map_or_else(
                    || "2 or 4 for automatic strut count".to_owned(),
                    |value| value.to_string(),
                );
                errors.push((
                    "landing_gear.mlg_strut_bogie_wheels".to_owned(),
                    format!(
                        "Per-strut main-gear wheel list has {} entries; expected {expected_text}.",
                        counts.len()
                    ),
                ));
            }

            for (index, &count) in counts.iter().enumerate() {
                if !matches!(count, 2 | 4 | 6) {
                    errors.push((
                        format!("landing_gear.mlg_strut_bogie_wheels[{index}]"),
                        format!(
                            "Main-gear bogie wheel count {count} is unsupported; use one of the standard even counts 2, 4 or 6."
                        ),
                    ));
                }
            }
        }

        errors
    }
}
