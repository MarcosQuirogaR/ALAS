// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The second mass pass: the detailed payload layout and the buildup priced
//! on the cabin it seats.
//!
//! The layout places cargo against an aircraft centre-of-gravity target, so
//! its payload mass and station depend on the operating empty mass and its
//! station it is handed. The first pass prices the declared (seed) cabin;
//! once the layout has seated the geometry-resolved cabin, the operating
//! empty mass `cabin_sync` re-prices is that cabin's. A layout left on the
//! seed cabin's empty mass is a different load from the one the sizing
//! closure flew: the closure resolves the cabin before its first pass
//! (`alas_opt::objective_model::apply_candidate_payload_load_case`) and
//! lays the payload out against that empty mass (`alas_opt::mdo::mda`). On
//! the DC-10 with its wing engines at the 8.18 m plan-view station the seed
//! empty mass was 946 kg heavier and 0.031 m further forward, the CG-targeted
//! cargo came out 82 kg heavier, and the report flew the design range 111 kg
//! above the takeoff mass the closure sized. The product pass therefore lays
//! the payload out again on the synchronized empty mass until the layout
//! stops changing.

use alas_config::{DesignRequirements, MassModelConfig, PassengerCabinConfig};
use alas_mass::breakdown::{MassBreakdown, MassCoordinates, PayloadLayoutSummary};

use super::*;

/// Layouts the product pass may build before it keeps the last one: the
/// seed-cabin layout, the layout on the synchronized cabin's empty mass, and
/// two more for a cabin whose seated count moves once more with the empty
/// mass. A seated count is an integer, so a non-oscillating cabin settles in
/// a few layouts; the bound only stops a pathological two-cycle.
const MAX_PAYLOAD_LAYOUTS: usize = 4;

/// The payload layout and the mass buildup priced on it.
pub(super) struct PayloadPass {
    /// The detailed layout the masses were priced on.
    pub layout: PayloadLayout,
    /// The structural payload limit the layout was bounded by, kg, when the
    /// registered maximum zero-fuel mass sets one.
    pub structural_payload_limit_kg: Option<f64>,
    /// Payload the layout placed above that limit and the route therefore
    /// does not carry, kg (see `route_payload_cap`). Zero when the limit
    /// holds.
    pub offloaded_payload_kg: f64,
    /// Masses, lumped coordinates, CG and the FLOPS groups of the pass.
    pub analysis: (
        MassBreakdown,
        MassCoordinates,
        [f64; 3],
        Option<Box<FlopsMassBuildup>>,
    ),
}

/// The first pass's cabin and mass model the second pass starts from.
pub(super) struct FirstPass<'a> {
    pub requirements: &'a DesignRequirements,
    pub mass_model: &'a MassModelConfig,
    pub cabin: &'a PassengerCabinConfig,
    /// Operating empty mass, kg, and its station, m, of the first pass.
    pub oew_kg: f64,
    pub x_oew_m: f64,
}

impl FullAnalysis {
    /// Lay the payload out and price the masses on it.
    pub(super) fn payload_pass(
        &self,
        design: &DesignVector,
        plane: &Airplane,
        first: &FirstPass<'_>,
        coordinate_model: MassCoordinateModel<'_>,
    ) -> Result<PayloadPass, String> {
        if self.reference_compatibility {
            return self.reference_payload_pass(design, plane, first, coordinate_model);
        }
        let (mut oew_kg, mut x_oew_m) = (first.oew_kg, first.x_oew_m);
        let mut previous: Option<PayloadPass> = None;
        for _ in 0..MAX_PAYLOAD_LAYOUTS {
            let structural_payload_limit_kg =
                effective_structural_payload_limit_kg(&self.config, design, oew_kg);
            let mut payload_config = self.config.clone();
            payload_config.cabin.passenger = first.cabin.clone();
            if let Some(limit_kg) = structural_payload_limit_kg {
                payload_config.requirements.max_structural_payload_kg = limit_kg;
            }
            let layout = build_payload_layout(plane, &payload_config, oew_kg, x_oew_m)
                .map_err(|error| format!("payload layout error: {error}"))?;
            let (layout, offloaded_payload_kg) = route_payload_cap::cap_route_payload(
                layout,
                structural_payload_limit_kg,
                payload_config.cabin.passenger.belly_cargo_kg,
            );
            if let Some(pass) = previous.take() {
                if summary_of(&pass.layout) == summary_of(&layout) {
                    return Ok(pass);
                }
            }
            // One cabin per case (see `cabin_sync`).
            let (cabin_requirements, cabin_mass_model) = cabin_sync::cabin_synchronized_for_cabin(
                first.requirements,
                first.mass_model,
                first.cabin,
                &layout,
            );
            let analysis = alas_mass::breakdown::run_product_mass_analysis_with_groups(
                plane,
                &cabin_requirements,
                &self.config.geometry,
                &payload_config.cabin,
                &self.config.control_surfaces,
                Some(&cabin_mass_model),
                Some(&summary_of(&layout)),
                coordinate_model,
                &self.config.landing_gear,
            )
            .map_err(|error| format!("mass-coordinate error: {error}"))?;
            let (stations, _) = self.station_coordinates(design, plane, &analysis.0, analysis.1)?;
            (oew_kg, x_oew_m) = oew_and_cg(&analysis.0, &stations);
            previous = Some(PayloadPass {
                layout,
                structural_payload_limit_kg,
                offloaded_payload_kg,
                analysis,
            });
        }
        previous.ok_or_else(|| "payload layout error: no layout was built".to_owned())
    }

    /// The frozen reference replay: one layout on the first pass.
    fn reference_payload_pass(
        &self,
        design: &DesignVector,
        plane: &Airplane,
        first: &FirstPass<'_>,
        coordinate_model: MassCoordinateModel<'_>,
    ) -> Result<PayloadPass, String> {
        let structural_payload_limit_kg =
            effective_structural_payload_limit_kg(&self.config, design, first.oew_kg);
        let layout = build_payload_layout_reference_compatibility(
            plane,
            &self.config,
            first.oew_kg,
            first.x_oew_m,
        )
        .map_err(|error| format!("payload layout error: {error}"))?;
        let (masses, coords, cg) =
            alas_mass::breakdown::run_mass_analysis_with_model_checked_with_gear(
                plane,
                first.requirements,
                &self.config.geometry,
                &self.config.cabin,
                &self.config.control_surfaces,
                Some(&self.config.mass_model),
                Some(&summary_of(&layout)),
                coordinate_model,
                &self.config.landing_gear,
            )
            .map_err(|error| format!("mass-coordinate error: {error}"))?;
        Ok(PayloadPass {
            layout,
            structural_payload_limit_kg,
            offloaded_payload_kg: 0.0,
            analysis: (masses, coords, cg, None),
        })
    }
}

/// The three layout attributes the mass buildup reads.
fn summary_of(layout: &PayloadLayout) -> PayloadLayoutSummary {
    PayloadLayoutSummary {
        total_mass: layout.total_mass,
        cg_x: layout.cg_x,
        cg_y: layout.cg_y,
    }
}

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A report bound to a sized candidate carries the zero-fuel mass the
    /// closure sized: the same operating empty mass and the same payload,
    /// whose CG-targeted cargo is laid out against the same empty mass. The
    /// DC-10 drawn from DAC-67803A Rev A Figure 2.2 (printed 55.35 m length,
    /// wing engines at the 8.18 m station) is where the seed-cabin layout
    /// would carry 82 kg more cargo than the closure.
    #[test]
    fn a_sized_report_carries_the_closure_zero_fuel_mass() {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "DC-10" })).unwrap();
        config.structures.enabled = false;
        config.geometry.engine.spanwise_positions_m = vec![8.18, -8.18, 0.0];
        let mut design = presets::get("DC-10").unwrap().design_vector;
        design.fuselage_length_m = 55.35;
        let assessment = alas_opt::assess_product_candidate(&config, &design).unwrap();
        let sized = &assessment.sized;
        let report = FullAnalysis::new(config)
            .run_sized_candidate(&assessment.resolved.design, sized)
            .unwrap();
        let masses = &report.component_masses;
        let fuel_kg = masses["Fuel"];
        let zero_fuel_mass_kg: f64 = masses.values().sum::<f64>() - fuel_kg;
        // Both sides sum the same component masses; the bound is rounding.
        let bound_kg = 1e-9 * sized.zero_fuel_mass_kg;
        assert!(
            (masses["Payload"] - sized.payload_kg).abs() <= bound_kg,
            "payload {} kg vs closure {} kg",
            masses["Payload"],
            sized.payload_kg
        );
        assert!(
            (zero_fuel_mass_kg - sized.zero_fuel_mass_kg).abs() <= bound_kg,
            "zero-fuel mass {zero_fuel_mass_kg} kg vs closure {} kg",
            sized.zero_fuel_mass_kg
        );
    }
}
