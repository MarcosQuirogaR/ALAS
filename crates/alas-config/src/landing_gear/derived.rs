// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A candidate's main-gear placement, carried beside the published anchors.
//!
//! The published stations of a registered aircraft are geometric evidence and
//! stay untouched. A redesigned candidate keeps the published nose-gear
//! station and the published leg-to-leg spacing of its main-gear group; the
//! group as a whole is translated along the body x axis (aft positive, m) by
//! the placement rule that sets its tip-back and static nose reaction. The
//! translation is a solved quantity of one candidate, not a user setting, so
//! it is not an editable configuration field: a replay writes it back
//! explicitly, as it does the solved tail sizing, and a saved delivered
//! configuration carries it so a reloaded result draws the same gear.

use super::{effective_main_gear_station, LandingGearConfig, LandingGearStationPositions};

/// The rigid longitudinal translation of every main-gear leg of one
/// candidate, relative to the stations the configuration resolves without it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedMainGearStation {
    /// Translation of the whole main-gear group, m, aft positive.
    pub translation_m: f64,
}

impl LandingGearConfig {
    /// Apply [`Self::derived_main_gear`], when set, to resolved positions.
    ///
    /// Every main leg moves by the same translation, so the effective station
    /// and the primary station move by it exactly and the bogie weighting is
    /// unchanged. The nose-gear station is not moved. A non-finite translation
    /// is not applied, and the positions keep `derived == false`, so the
    /// caller's own domain checks still see the unmodified layout.
    pub(super) fn apply_derived_main_gear(
        &self,
        positions: LandingGearStationPositions,
    ) -> LandingGearStationPositions {
        let Some(translation_m) = self
            .derived_main_gear
            .map(|derived| derived.translation_m)
            .filter(|value| value.is_finite())
        else {
            return positions;
        };
        let main_gear_x_m: Vec<f64> = positions
            .main_gear_x_m
            .iter()
            .map(|station| station + translation_m)
            .collect();
        let resolution =
            effective_main_gear_station(&main_gear_x_m, self.mlg_strut_bogie_wheels.as_deref());
        LandingGearStationPositions {
            x_nlg_m: positions.x_nlg_m,
            x_mlg_m: positions.x_mlg_m + translation_m,
            main_gear_x_m,
            source_scaled: positions.source_scaled,
            derived: true,
            resolution,
        }
    }
}

#[cfg(test)]
// Tests build every fixture they assert on, so a failed unwrap is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn anchored() -> LandingGearConfig {
        LandingGearConfig {
            n_mlg_struts: 3,
            reference_station_fuselage_length_m: Some(50.0),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.5, 0.5, 0.6]),
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            ..LandingGearConfig::default()
        }
    }

    #[test]
    fn a_derived_translation_moves_every_main_leg_and_keeps_the_nose_gear() {
        let reference = anchored();
        let published = reference.resolved_station_positions(0.0, 0.0, 1.0, 50.0);
        let mut moved = anchored();
        moved.derived_main_gear = Some(DerivedMainGearStation { translation_m: 0.4 });
        let placed = moved.resolved_station_positions(0.0, 0.0, 1.0, 50.0);
        assert!(placed.derived && placed.source_scaled);
        assert_eq!(placed.x_nlg_m, published.x_nlg_m);
        for (moved_leg, published_leg) in placed.main_gear_x_m.iter().zip(&published.main_gear_x_m)
        {
            assert!((moved_leg - published_leg - 0.4).abs() < 1e-12);
        }
        assert!((placed.x_mlg_m - published.x_mlg_m - 0.4).abs() < 1e-12);
        let station = |positions: &LandingGearStationPositions| {
            positions
                .resolution
                .as_ref()
                .map(|valid| valid.station_m())
                .ok()
        };
        let difference = station(&placed)
            .zip(station(&published))
            .map(|(a, b)| a - b);
        assert!(difference.is_some_and(|value| (value - 0.4).abs() < 1e-12));
    }

    #[test]
    fn a_saved_translation_round_trips_and_an_absent_one_is_not_written() {
        let mut config = anchored();
        let plain = serde_json::to_value(&config).unwrap();
        assert!(plain.get("derived_main_gear").is_none());
        config.derived_main_gear = Some(DerivedMainGearStation {
            translation_m: 0.25,
        });
        let saved = serde_json::to_string(&config).unwrap();
        let restored: LandingGearConfig = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, config);
    }

    #[test]
    fn a_non_finite_translation_leaves_the_published_layout() {
        let mut config = anchored();
        config.derived_main_gear = Some(DerivedMainGearStation {
            translation_m: f64::NAN,
        });
        let placed = config.resolved_station_positions(0.0, 0.0, 1.0, 50.0);
        assert!(!placed.derived);
        assert_eq!(
            placed,
            anchored().resolved_station_positions(0.0, 0.0, 1.0, 50.0)
        );
    }
}
