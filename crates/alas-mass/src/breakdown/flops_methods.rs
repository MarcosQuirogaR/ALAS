// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Apply the selected FLOPS mass methods to a product mass buildup.
//!
//! The reference-compatible buildup is evaluated first; each FLOPS group
//! that is selected then replaces the groups it covers. The systems method
//! replaces the systems and furnishings groups (operating items ride with
//! furnishings, as the frozen fractions define them); the structural method
//! replaces the wing, both tails, the fuselage and the landing gear; the
//! propulsion method replaces the installed propulsion group. Nacelles are a
//! structural item in the FLOPS group statement but sit on the engines, so
//! they are carried in the propulsion group here, where the mass stations
//! put them at the nacelle centroid.

use alas_config::{
    ControlSurfacesConfig, DesignRequirements, GeometryConfig, MassModelConfig,
};
use alas_geom::aircraft::airplane::Airplane;

use crate::flops_transport::{
    evaluate_airframe_product, evaluate_product, FlopsAirframeEvaluation,
    FlopsAirframeRequest, FlopsAirframeSelection, FlopsSystemsBreakdown,
    FlopsTransportEvaluation, PartialFlopsTransportBreakdown,
};

use super::{ComponentMassError, MassBreakdown, OEW_KEYS};

/// Replace the groups the selected FLOPS methods cover and re-close the
/// fuel remainder against the design gross mass.
///
/// # Errors
///
/// [`ComponentMassError::FlopsUnverified`] with every blocker when a
/// selected method cannot be evaluated from the declared architecture.
pub(super) fn apply_selected_methods(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry: &GeometryConfig,
    controls: &ControlSurfacesConfig,
    mass_model: &MassModelConfig,
    mut masses: MassBreakdown,
) -> Result<MassBreakdown, ComponentMassError> {
    let mut systems: Option<FlopsSystemsBreakdown> = None;
    let mut operating_items_kg = 0.0;
    if !mass_model.systems_mass_method.is_reference_compatible() {
        let evaluation = evaluate_product(
            plane,
            requirements,
            geometry,
            controls,
            &mass_model.flops_transport,
        );
        let breakdown = match evaluation {
            FlopsTransportEvaluation::Verified { breakdown, .. } => breakdown,
            FlopsTransportEvaluation::Unverified { reasons, partial } => {
                return Err(ComponentMassError::FlopsUnverified {
                    reasons,
                    partial: Box::new(partial),
                });
            }
        };
        masses.systems = breakdown.systems.total_kg;
        masses.furnishings =
            breakdown.systems.furnishings_kg + breakdown.operating_items.total_kg;
        operating_items_kg = breakdown.operating_items.total_kg;
        systems = Some(breakdown.systems);
    }

    let selection = FlopsAirframeSelection {
        structure: !mass_model.structural_mass_method.is_reference_compatible(),
        propulsion: !mass_model.propulsion_mass_method.is_reference_compatible(),
    };
    if selection.structure || selection.propulsion {
        let evaluation = evaluate_airframe_product(&FlopsAirframeRequest {
            plane,
            requirements,
            geometry,
            controls,
            mass_model,
            systems: systems.as_ref(),
            selection,
        });
        let airframe = match evaluation {
            FlopsAirframeEvaluation::Verified(airframe) => airframe,
            FlopsAirframeEvaluation::Unverified { reasons } => {
                return Err(ComponentMassError::FlopsUnverified {
                    reasons,
                    partial: Box::new(PartialFlopsTransportBreakdown::default()),
                });
            }
        };
        let nacelle_kg = airframe
            .structure
            .map_or(0.0, |structure| structure.nacelle_kg);
        if let Some(structure) = airframe.structure {
            masses.wing = structure.wing.total_kg;
            masses.h_stab = structure.horizontal_tail_kg;
            masses.v_stab = structure.vertical_tail_kg;
            masses.fuselage = structure.fuselage_kg + structure.paint_kg;
            masses.gear = structure.main_gear_kg + structure.nose_gear_kg;
        }
        match airframe.propulsion {
            Some(propulsion) => masses.propulsion = propulsion.total_kg + nacelle_kg,
            None => masses.propulsion += nacelle_kg,
        }
    }

    // Equation 139: the empty-mass margin is a fraction of the structural,
    // propulsion and systems groups (operating items excluded), carried in
    // the systems group here.
    let margin_fraction = mass_model.flops_structure.empty_mass_margin_fraction;
    if margin_fraction > 0.0 {
        let empty = OEW_KEYS
            .iter()
            .filter_map(|name| masses.get(name))
            .sum::<f64>()
            - operating_items_kg;
        masses.systems += margin_fraction * empty;
    }

    let oew = OEW_KEYS
        .iter()
        .filter_map(|name| masses.get(name))
        .sum::<f64>();
    masses.fuel = requirements.mtow_kg - oew - masses.payload;
    Ok(masses)
}
