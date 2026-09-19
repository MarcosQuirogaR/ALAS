// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The pure FLOPS product mass buildup.
//!
//! Every one of the eight operating-empty slots is a FLOPS equation. Nothing
//! here starts from the frozen Torenbeek/fraction buildup and replaces part of
//! it: that arrangement produced a mass belonging to no published method, and
//! the group boundaries did not line up. FLOPS assigns items on its own
//! conventions and the mapping into the ALAS slots is fixed here, once:
//!
//! | FLOPS | ALAS slot |
//! |---|---|
//! | Wing, both tails, gear (eqs. 10-67) | `wing`, `h_stab`, `v_stab`, `gear` |
//! | Fuselage + paint (eqs. 56, 68) | `fuselage` |
//! | Installed propulsion + **nacelles** (eqs. 73-92, 136) | `propulsion` |
//! | Systems and equipment less furnishings (eq. 138) | `systems` |
//! | Furnishings `WFURN` + operating items `WOPIT` (eqs. 138, 140) | `furnishings` |
//! | Empty-mass margin `WMARG` (eq. 139) | `systems` |
//!
//! **Nacelles are charged to `propulsion` and to nothing else.** FLOPS prints
//! them in the structural group statement, but they sit on the engines and the
//! ALAS mass stations put them at the nacelle centroid. Because both groups
//! are always evaluated together here there is no selection under which the
//! nacelle group can be dropped or added twice, which is what the previous
//! per-group selection allowed.

use alas_config::{
    CabinConfig, ControlSurfacesConfig, DesignRequirements, GeometryConfig, MassModelConfig,
};
use alas_geom::aircraft::airplane::Airplane;

use crate::flops_transport::{
    evaluate_airframe_product, evaluate_product_at_design_gross_mass, FlopsAirframeBreakdown,
    FlopsAirframeEvaluation, FlopsAirframeRequest, FlopsAirframeSelection, FlopsTransportBreakdown,
    FlopsTransportEvaluation, FlopsTransportInputs, PartialFlopsTransportBreakdown,
};

use super::{ComponentMassError, MassBreakdown, OEW_KEYS};

/// A complete FLOPS mass buildup with the groups that produced it.
///
/// The ledger needs the component groups, not just the eight lumped slots:
/// without them it can only place a single "systems" row and has to label it
/// from the configuration rather than from what was evaluated. Returning them
/// together is what lets the item-level statement be built from the same
/// evaluation the breakdown came from instead of a second, independent one.
#[derive(Debug, Clone, PartialEq)]
pub struct FlopsMassBuildup {
    /// The eight operating-empty slots plus payload and the fuel remainder.
    pub masses: MassBreakdown,
    /// Systems, equipment and operating items, per component.
    pub systems_and_operating_items: FlopsTransportBreakdown,
    /// Structural and propulsion groups, per component, with their inputs.
    pub airframe: Box<FlopsAirframeBreakdown>,
    /// The resolved systems inputs, kept for audit and evidence.
    pub inputs: FlopsTransportInputs,
    /// Revision-locked evidence for every declared non-geometric input.
    pub provenance: Box<alas_config::FlopsTransportProvenance>,
}

impl FlopsMassBuildup {
    /// The nacelle group, which belongs to the propulsion slot exactly once.
    pub fn nacelle_kg(&self) -> f64 {
        self.airframe
            .structure
            .map_or(0.0, |structure| structure.nacelle_kg)
    }

    /// The installed engines, reversers, controls, starters and fuel system,
    /// *without* the nacelles.
    ///
    /// For a turboprop this is the shaft-power group instead: engines,
    /// propellers, gearboxes charged separately, pylons, the declared
    /// installation mass and the fuel system, also without the nacelles.
    pub fn propulsion_without_nacelles_kg(&self) -> f64 {
        self.airframe
            .propulsion
            .map(|propulsion| propulsion.total_kg)
            .or_else(|| {
                self.airframe
                    .turboprop_propulsion
                    .map(|group| group.total_without_nacelles_kg)
            })
            .unwrap_or(0.0)
    }
}

/// Evaluate the complete FLOPS buildup for one built aircraft.
///
/// # Errors
///
/// [`ComponentMassError::FlopsUnverified`] with every blocker when any
/// required datum is absent or inconsistent. There is no partial result and
/// no fallback: a missing input never becomes a fraction of takeoff mass.
pub(super) fn build_pure_flops(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry: &GeometryConfig,
    controls: &ControlSurfacesConfig,
    cabin: &CabinConfig,
    mass_model: &MassModelConfig,
) -> Result<FlopsMassBuildup, ComponentMassError> {
    // The systems group reads the same design gross mass as the airframe:
    // a declared `flops_structure.design_gross_mass_kg` pins both, otherwise
    // both size at the takeoff-mass requirement of the case being evaluated.
    let (groups, inputs, provenance) = match evaluate_product_at_design_gross_mass(
        plane,
        requirements,
        geometry,
        controls,
        cabin,
        &mass_model.flops_transport,
        &mass_model.flops_turboprop,
        mass_model.flops_structure.design_gross_mass_kg,
    ) {
        FlopsTransportEvaluation::Verified {
            inputs,
            provenance,
            breakdown,
        } => (breakdown, inputs, provenance),
        FlopsTransportEvaluation::Unverified { reasons, partial } => {
            return Err(ComponentMassError::FlopsUnverified {
                reasons,
                partial: Box::new(partial),
            });
        }
    };

    // Both groups, always. The detailed wing method needs the systems group
    // for its pod inertia relief, which is why the systems buildup is
    // evaluated first and handed in rather than recomputed.
    let airframe = match evaluate_airframe_product(&FlopsAirframeRequest {
        plane,
        requirements,
        geometry,
        controls,
        mass_model,
        systems: Some(&groups.systems),
        selection: FlopsAirframeSelection {
            structure: true,
            propulsion: true,
        },
    }) {
        FlopsAirframeEvaluation::Verified(airframe) => airframe,
        FlopsAirframeEvaluation::Unverified { reasons } => {
            return Err(ComponentMassError::FlopsUnverified {
                reasons,
                partial: Box::new(PartialFlopsTransportBreakdown::default()),
            });
        }
    };

    // Asking for both groups and being handed one would mean the evaluator
    // reported success for a selection it did not honour. Refuse rather than
    // publish a breakdown with a silently empty slot. Exactly one propulsion
    // group is populated: the thrust-based FLOPS one for a turbofan, the
    // shaft-power one for a turboprop, never both and never neither.
    let Some(structure) = airframe.structure else {
        return Err(ComponentMassError::FlopsIncompleteAirframe);
    };
    let propulsion_without_nacelles_kg = match (
        airframe.propulsion.as_ref(),
        airframe.turboprop_propulsion.as_ref(),
    ) {
        (Some(group), None) => group.total_kg,
        (None, Some(group)) => group.total_without_nacelles_kg,
        (Some(_), Some(_)) | (None, None) => {
            return Err(ComponentMassError::FlopsIncompleteAirframe)
        }
    };

    let mut masses = MassBreakdown {
        wing: structure.wing.total_kg,
        h_stab: structure.horizontal_tail_kg,
        v_stab: structure.vertical_tail_kg,
        fuselage: structure.fuselage_kg + structure.paint_kg,
        gear: structure.main_gear_kg + structure.nose_gear_kg,
        // Nacelles ride with the engines, here and nowhere else.
        propulsion: propulsion_without_nacelles_kg + structure.nacelle_kg,
        // Equation 138 counts furnishings inside the systems-and-equipment
        // group; the ALAS breakdown carries them in their own slot, so the
        // systems slot is the group total less furnishings and `WFURN` is
        // counted once.
        systems: groups.systems.total_kg - groups.systems.furnishings_kg,
        // Operating items sit above empty mass and below OEW, as equation 141
        // has them.
        furnishings: groups.systems.furnishings_kg + groups.operating_items.total_kg,
        payload: requirements.payload_kg(),
        fuel: 0.0,
    };

    // Equation 139: the empty-mass margin is a fraction of the structural,
    // propulsion and systems groups. Operating items are not part of the
    // empty mass and must stay out of the margin base.
    let margin_fraction = mass_model.flops_structure.empty_mass_margin_fraction;
    if margin_fraction > 0.0 {
        let empty = oew_sum(&masses) - groups.operating_items.total_kg;
        masses.systems += margin_fraction * empty;
    }

    // Fuel is the takeoff-mass closure remainder, and a negative one is kept
    // visible: it means this aircraft's operating empty mass and payload do
    // not fit under its declared takeoff mass, which is a finding rather than
    // something to clamp away.
    masses.fuel = requirements.mtow_kg - oew_sum(&masses) - masses.payload;

    Ok(FlopsMassBuildup {
        masses,
        systems_and_operating_items: groups,
        airframe,
        inputs,
        provenance,
    })
}

/// The eight operating-empty slots, summed.
fn oew_sum(masses: &MassBreakdown) -> f64 {
    OEW_KEYS
        .iter()
        .filter_map(|name| masses.get(name))
        .sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{
        CabinConfig, FlopsInputEvidence, FlopsInputProvenance, FlopsTransportConfig,
        FlopsTransportProvenance, MassArchitecture,
    };
    use alas_geom::builder::AircraftBuilder;

    use crate::breakdown::calculate_component_masses_checked_product_with_gear;
    use crate::flops_transport::FlopsOperatingItemsBreakdown;

    fn provenance() -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: "test".to_owned(),
            revision: "test".to_owned(),
            location: "test".to_owned(),
            applicability: "test".to_owned(),
            evidence: FlopsInputEvidence::UserDeclared,
            uncertainty: "test fixture; no uncertainty is claimed".to_owned(),
        }
    }

    /// The default geometry with a fully declared FLOPS architecture, so the
    /// buildup evaluates rather than reporting a missing datum.
    fn fixture() -> (
        Airplane,
        GeometryConfig,
        DesignRequirements,
        MassModelConfig,
    ) {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .unwrap_or_else(|error| panic!("default geometry builds: {error}"));
        let requirements = DesignRequirements::default();
        let passengers = usize::try_from(requirements.num_passengers).unwrap_or(0);
        let mass_model = MassModelConfig {
            mass_architecture: MassArchitecture::PureFlopsTransportV1,
            flops_transport: FlopsTransportConfig {
                maximum_mach: Some(0.89),
                design_range_nmi: Some(7_000.0),
                flight_crew_count: Some(2),
                flight_attendant_count: Some(10),
                galley_crew_count: Some(1),
                first_class_passenger_count: Some(0),
                business_class_passenger_count: Some(0),
                tourist_class_passenger_count: Some(passengers),
                hydraulic_pressure_pa: Some(20_684_271.879_504),
                variable_sweep_penalty: Some(0.0),
                wing_mounted_engine_count: Some(geometry.engine.spanwise_positions_m.len()),
                fuselage_mounted_engine_count: Some(0),
                fuel_tank_count: Some(6),
                maximum_fuel_capacity_kg: Some(220_000.0),
                containerized_cargo_kg: Some(0.0),
                cargo_loading: Some(alas_config::CargoHoldLoading::Containerized),
                containerized_baggage_fraction: None,
                cabin_equipment_method: alas_config::CabinEquipmentMethod::FlopsTransportV1,
                haul_class: None,
                provenance: FlopsTransportProvenance {
                    mission: provenance(),
                    cabin: provenance(),
                    architecture: provenance(),
                },
            },
            ..MassModelConfig::default()
        };
        (plane, geometry, requirements, mass_model)
    }

    fn buildup(
        plane: &Airplane,
        requirements: &DesignRequirements,
        geometry: &GeometryConfig,
        mass_model: &MassModelConfig,
    ) -> FlopsMassBuildup {
        build_pure_flops(
            plane,
            requirements,
            geometry,
            &ControlSurfacesConfig::default(),
            &CabinConfig::default(),
            mass_model,
        )
        .unwrap_or_else(|error| panic!("the declared fixture must evaluate: {error}"))
    }

    fn applied(
        plane: &Airplane,
        requirements: &DesignRequirements,
        geometry: &GeometryConfig,
        mass_model: &MassModelConfig,
    ) -> MassBreakdown {
        calculate_component_masses_checked_product_with_gear(
            plane,
            requirements,
            geometry,
            &ControlSurfacesConfig::default(),
            Some(mass_model),
            &alas_config::LandingGearConfig::default(),
            &CabinConfig::default(),
        )
        .unwrap_or_else(|error| panic!("the declared fixture must evaluate: {error}"))
    }

    #[test]
    fn the_systems_slot_excludes_furnishings_so_the_group_is_counted_once() {
        let (plane, geometry, requirements, mass_model) = fixture();
        let built = buildup(&plane, &requirements, &geometry, &mass_model);
        let masses = &built.masses;
        let systems = built.systems_and_operating_items.systems;
        let operating_items = built.systems_and_operating_items.operating_items;

        assert!(
            (masses.systems - (systems.total_kg - systems.furnishings_kg)).abs() < 1e-9,
            "systems slot {} vs group less furnishings {}",
            masses.systems,
            systems.total_kg - systems.furnishings_kg
        );
        assert!(
            (masses.furnishings - (systems.furnishings_kg + operating_items.total_kg)).abs() < 1e-9,
            "furnishings slot {} vs furnishings plus operating items {}",
            masses.furnishings,
            systems.furnishings_kg + operating_items.total_kg
        );
        // The two slots together are the FLOPS group total plus the operating
        // items exactly once. The regression this pins is the buildup that
        // added `systems.total_kg` and `furnishings_kg` again.
        let carried = masses.systems + masses.furnishings;
        let expected = systems.total_kg + operating_items.total_kg;
        assert!(
            (carried - expected).abs() < 1e-9,
            "carried {carried} vs FLOPS {expected}"
        );
        assert!(
            systems.furnishings_kg > 0.0,
            "the fixture must exercise a nonzero furnishings group"
        );
        assert!(
            (carried - (expected + systems.furnishings_kg)).abs() > 1.0,
            "the double-counted total must not be reachable"
        );
    }

    #[test]
    fn the_nacelle_group_is_carried_by_the_propulsion_slot_exactly_once() {
        let (plane, geometry, requirements, mass_model) = fixture();
        let built = buildup(&plane, &requirements, &geometry, &mass_model);
        let nacelle_kg = built.nacelle_kg();
        assert!(
            nacelle_kg > 0.0,
            "the fixture must exercise a nonzero nacelle group"
        );
        assert!(
            (built.masses.propulsion - (built.propulsion_without_nacelles_kg() + nacelle_kg)).abs()
                < 1e-9,
            "propulsion slot {} vs engines {} plus nacelles {nacelle_kg}",
            built.masses.propulsion,
            built.propulsion_without_nacelles_kg()
        );
        // The structural slots must not carry it as well. FLOPS prints
        // nacelles in the structural group statement, and charging them
        // there *and* to propulsion is the double count this pins.
        let structure = built
            .airframe
            .structure
            .expect("the pure buildup always evaluates the structural group");
        let structural_slots = built.masses.wing
            + built.masses.h_stab
            + built.masses.v_stab
            + built.masses.fuselage
            + built.masses.gear;
        let structural_without_nacelles = structure.wing.total_kg
            + structure.horizontal_tail_kg
            + structure.vertical_tail_kg
            + structure.fuselage_kg
            + structure.paint_kg
            + structure.main_gear_kg
            + structure.nose_gear_kg;
        assert!(
            (structural_slots - structural_without_nacelles).abs() < 1e-9,
            "structural slots {structural_slots} must exclude the {nacelle_kg} kg nacelle group"
        );
    }

    #[test]
    fn the_oew_sum_closes_the_design_gross_mass_against_the_fuel_remainder() {
        let (plane, geometry, requirements, mass_model) = fixture();
        let masses = applied(&plane, &requirements, &geometry, &mass_model);
        let oew = oew_sum(&masses);
        assert!(
            (oew + masses.payload + masses.fuel - requirements.mtow_kg).abs() < 1e-6,
            "OEW {oew} + payload {} + fuel {} vs MTOW {}",
            masses.payload,
            masses.fuel,
            requirements.mtow_kg
        );
        // Every OEW slot must be positive; a zero would mean a group silently
        // dropped out of the sum.
        for name in OEW_KEYS {
            let value = masses.get(name).unwrap_or(0.0);
            assert!(value > 0.0, "{name} is {value} kg");
        }
    }

    #[test]
    fn the_empty_mass_margin_is_the_fraction_of_the_three_groups_without_operating_items() {
        // FLOPS equation 139: WWE = WSTRCT + WPRO + WSYS + WMARG, and the
        // margin is a fraction of those three groups. Operating items are not
        // part of the empty mass and must not enter the margin base.
        let (plane, geometry, requirements, mass_model) = fixture();
        let without = applied(&plane, &requirements, &geometry, &mass_model);
        let mut with_margin = mass_model.clone();
        with_margin.flops_structure.empty_mass_margin_fraction = 0.05;
        let with = applied(&plane, &requirements, &geometry, &with_margin);

        let built = buildup(&plane, &requirements, &geometry, &mass_model);
        let base = oew_sum(&without) - built.systems_and_operating_items.operating_items.total_kg;
        let margin = with.systems - without.systems;
        assert!(
            (margin - 0.05 * base).abs() < 1e-6,
            "margin {margin} vs 5 percent of {base}"
        );
        assert!(
            (with.furnishings - without.furnishings).abs() < 1e-9,
            "the margin must not move the operating items"
        );
    }

    #[test]
    fn the_operating_items_stay_out_of_the_systems_slot() {
        let (plane, geometry, requirements, mass_model) = fixture();
        let built = buildup(&plane, &requirements, &geometry, &mass_model);
        let FlopsOperatingItemsBreakdown { total_kg, .. } =
            built.systems_and_operating_items.operating_items;
        assert!(total_kg > 0.0);
        assert!(
            built.masses.systems < built.systems_and_operating_items.systems.total_kg,
            "the systems slot must be the group less furnishings, not the group plus items"
        );
    }

    #[test]
    fn an_undeclared_architecture_is_an_error_rather_than_a_fraction() {
        let (plane, geometry, requirements, mut mass_model) = fixture();
        mass_model.flops_transport.maximum_mach = None;
        let error = build_pure_flops(
            &plane,
            &requirements,
            &geometry,
            &ControlSurfacesConfig::default(),
            &CabinConfig::default(),
            &mass_model,
        )
        .expect_err("a missing maximum Mach must block the buildup");
        match error {
            ComponentMassError::FlopsUnverified { reasons, .. } => assert!(reasons
                .contains(&crate::flops_transport::FlopsTransportUnverifiedReason::MaximumMach)),
            other => panic!("expected an unverified FLOPS evaluation, got {other}"),
        }
    }
}
