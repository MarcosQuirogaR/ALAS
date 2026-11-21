// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolution of the longitudinal landing-gear stations in the geometry frame.

use super::station::{
    effective_main_gear_station, EffectiveGearStationExt, LandingGearStationPositions,
    MainGearFallbackRefusal, WingMountedGearDomain,
};
use super::LandingGearConfig;

impl LandingGearConfig {
    /// Resolve the longitudinal landing-gear stations in the active geometry
    /// frame, **without** checking that the caller's main-gear fallback is
    /// inside its stated domain.
    ///
    /// A complete source anchor set is expressed as nose-tip drawing
    /// fractions and scales with the active fuselage length.  This preserves
    /// the geometry design space during shrink/optimization while making a
    /// source-backed preset reproducible.  If the source anchor is absent or
    /// malformed, the caller's existing model-derived fallback stations are
    /// retained.  Source stations are geometric evidence; they do not use
    /// mass, CG, or reaction loads to calibrate a position.
    ///
    /// # When this entry point is the wrong one
    ///
    /// `fallback_x_mlg_m` is, for every caller in this workspace, the
    /// wing-mounted rule `mac_le + mlg_x_fraction_mac * MAC`, which only
    /// stands where a wing-root gear bay exists (see
    /// [`WingMountedGearDomain`]). This method cannot tell whether it does,
    /// so it returns whatever fallback it was handed.
    ///
    /// It is retained for the one caller that applies the domain gate itself
    /// before calling - `alas_mass::stations::main_gear_station`, which owns
    /// the geometric comparison and raises its own typed missing-datum error.
    /// **Every other consumer must use
    /// [`Self::resolved_station_positions_checked`]**, so that a layout
    /// outside the fallback's domain is refused once, here, rather than being
    /// re-derived independently at each export, figure and diagnostic.
    pub fn resolved_station_positions(
        &self,
        fallback_x_nlg_m: f64,
        fallback_x_mlg_m: f64,
        fuselage_start_x_m: f64,
        fuselage_length_m: f64,
    ) -> LandingGearStationPositions {
        let source = self
            .reference_station_fuselage_length_m
            .filter(|length| length.is_finite() && *length > 0.0)
            .zip(self.reference_nlg_x_fraction)
            .zip(self.reference_mlg_x_fractions.as_deref())
            .filter(|((_, nlg), mlg)| {
                fuselage_start_x_m.is_finite()
                    && nlg.is_finite()
                    && (0.0..=1.0).contains(nlg)
                    && !mlg.is_empty()
                    && mlg
                        .iter()
                        .all(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
                    && fuselage_length_m.is_finite()
                    && fuselage_length_m > 0.0
            });
        if let Some(((_, nlg_fraction), mlg_fractions)) = source {
            let x_nlg_m = fuselage_start_x_m + fuselage_length_m * nlg_fraction;
            let main_gear_x_m: Vec<f64> = mlg_fractions
                .iter()
                .map(|fraction| fuselage_start_x_m + fuselage_length_m * fraction)
                .collect();
            let effective =
                effective_main_gear_station(&main_gear_x_m, self.mlg_strut_bogie_wheels.as_deref());
            return self.apply_derived_main_gear(LandingGearStationPositions {
                x_nlg_m,
                x_mlg_m: effective.primary_station_ignoring_rejection(),
                main_gear_x_m,
                source_scaled: true,
                derived: false,
                resolution: effective,
            });
        }

        self.apply_derived_main_gear(LandingGearStationPositions {
            x_nlg_m: fallback_x_nlg_m,
            x_mlg_m: fallback_x_mlg_m,
            main_gear_x_m: vec![fallback_x_mlg_m],
            source_scaled: false,
            derived: false,
            resolution: effective_main_gear_station(&[fallback_x_mlg_m], None),
        })
    }

    /// [`Self::resolved_station_positions`], refusing the caller's
    /// wing-mounted main-gear fallback when the layout is outside the rule's
    /// stated domain.
    ///
    /// The order matters and mirrors the mass model's: a complete source
    /// anchor is a published station scaled onto the active fuselage and is
    /// admissible on any layout, so `domain` is never consulted when the
    /// positions come out `source_scaled`. It is consulted exactly when the
    /// answer would otherwise be the wing-mounted fallback, which is why it
    /// is taken as a closure - a caller whose domain query costs something
    /// (a station resolution, a geometry pass) pays for it only on the
    /// aircraft where it decides the outcome.
    ///
    /// # Errors
    ///
    /// [`MainGearFallbackRefusal`] when the fallback would be used and
    /// `domain` reports [`WingMountedGearDomain::WingRootAboveFuselageCrown`].
    /// The refusal carries the two heights that decided it, in the geometry
    /// frame (z up, m), so a consumer reports the missing datum rather than a
    /// number it did not measure.
    pub fn resolved_station_positions_checked<F>(
        &self,
        fallback_x_nlg_m: f64,
        fallback_x_mlg_m: f64,
        fuselage_start_x_m: f64,
        fuselage_length_m: f64,
        domain: F,
    ) -> Result<LandingGearStationPositions, MainGearFallbackRefusal>
    where
        F: FnOnce() -> WingMountedGearDomain,
    {
        let positions = self.resolved_station_positions(
            fallback_x_nlg_m,
            fallback_x_mlg_m,
            fuselage_start_x_m,
            fuselage_length_m,
        );
        if positions.source_scaled {
            return Ok(positions);
        }
        match domain() {
            WingMountedGearDomain::Applicable => Ok(positions),
            WingMountedGearDomain::WingRootAboveFuselageCrown {
                wing_root_z_m,
                fuselage_crown_z_m,
            } => Err(MainGearFallbackRefusal {
                wing_root_z_m,
                fuselage_crown_z_m,
            }),
        }
    }
}
