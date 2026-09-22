// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolve a built ALAS transport into the physical inputs required by FLOPS.
//!
//! The equations are useful only when their architecture inputs are declared.
//! This adapter therefore returns an explicit `Unverified` result instead of
//! filling absent range, crew, cabin, hydraulic, or fuel-system data from a
//! preset or from a generic percentage.

use alas_config::{
    ActiveEngineModel, CabinConfig, CargoHoldLoading, ControlSurfacesConfig, DesignRequirements,
    FlopsTransportConfig, FlopsTurbopropConfig, GeometryConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

use super::movable_area::movable_surface_area;
use super::{
    estimate_flops_transport, FlopsTransportEvaluation, FlopsTransportInputs,
    FlopsTransportUnverifiedReason, PartialFlopsTransportBreakdown, PropulsionSizing,
};

pub(super) fn main_wing(plane: &Airplane) -> Option<&Wing> {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())
        .filter(|wing| wing.xsecs.len() >= 2)
}

pub(super) fn primary_fuselage(plane: &Airplane) -> Option<&Fuselage> {
    plane
        .fuselages
        .iter()
        .find(|fuselage| fuselage.name == "Fuselage")
        .or_else(|| {
            plane
                .fuselages
                .iter()
                .find(|fuselage| !fuselage.name.contains("Nacelle"))
        })
        .filter(|fuselage| fuselage.xsecs.len() >= 2)
}

pub(super) fn max_fuselage_width_depth(fuselage: &Fuselage) -> (f64, f64) {
    fuselage
        .xsecs
        .iter()
        .fold((0.0, 0.0), |(width, depth), xsec| {
            (width.max(xsec.width), depth.max(xsec.height))
        })
}

fn positive_optional(
    value: Option<f64>,
    reason: FlopsTransportUnverifiedReason,
) -> Result<f64, FlopsTransportUnverifiedReason> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => Err(reason),
    }
}

fn nonnegative_optional(
    value: Option<f64>,
    reason: FlopsTransportUnverifiedReason,
) -> Result<f64, FlopsTransportUnverifiedReason> {
    match value {
        Some(value) if value.is_finite() && value >= 0.0 => Ok(value),
        _ => Err(reason),
    }
}

fn count_optional(
    value: Option<usize>,
    reason: FlopsTransportUnverifiedReason,
) -> Result<usize, FlopsTransportUnverifiedReason> {
    value.ok_or(reason)
}

fn unavailable(reasons: Vec<FlopsTransportUnverifiedReason>) -> FlopsTransportEvaluation {
    let mut reasons = reasons;
    reasons.sort_unstable();
    reasons.dedup();
    FlopsTransportEvaluation::Unverified {
        reasons,
        partial: PartialFlopsTransportBreakdown::default(),
    }
}

/// Evaluate a built aircraft with NASA/TM-2017-219627 transport inputs.
///
/// Every optional datum in [`FlopsTransportConfig`] is a declared physical
/// input. Missing data remain visible as `Unverified`; no preset fraction is
/// used as a fallback. The returned input record is retained with the result
/// so an audit can reproduce exactly which architecture was evaluated.
pub fn evaluate_product(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry: &GeometryConfig,
    controls: &ControlSurfacesConfig,
    cabin: &CabinConfig,
    flops: &FlopsTransportConfig,
    turboprop: &FlopsTurbopropConfig,
) -> FlopsTransportEvaluation {
    evaluate_product_at_design_gross_mass(
        plane,
        requirements,
        geometry,
        controls,
        cabin,
        flops,
        turboprop,
        None,
    )
}

/// The checked baggage FLOPS charges to the cargo containers, kg.
///
/// The cabin configuration owns the baggage share of the single combined
/// occupant mass (`CabinConfig::checked_bag_mass_kg`), so this reads the load
/// case the rest of the product already uses rather than inventing an
/// allowance. A freighter has no seated passengers and therefore no checked
/// baggage; its containerized revenue cargo is the declared input instead.
///
/// Only the share that actually rides in a unit load device reaches equations
/// 125-126: the tare is a container, and an aircraft with loose-loaded holds
/// has none to charge. [`CargoHoldLoading`] carries that architecture per
/// aircraft, and a mixed arrangement must declare its containerised share
/// rather than have one assumed.
fn containerized_baggage_kg(
    requirements: &DesignRequirements,
    cabin: &CabinConfig,
    loading: CargoHoldLoading,
    declared_fraction: Option<f64>,
) -> Result<f64, FlopsTransportUnverifiedReason> {
    let share = match loading {
        CargoHoldLoading::Bulk => 0.0,
        CargoHoldLoading::Containerized => 1.0,
        CargoHoldLoading::Mixed => match declared_fraction {
            Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => value,
            _ => return Err(FlopsTransportUnverifiedReason::CargoHoldLoading),
        },
    };
    if share == 0.0 || requirements.aircraft_type != "passenger" {
        return Ok(0.0);
    }
    let passengers = f64::from(i32::try_from(requirements.num_passengers).unwrap_or(0)).max(0.0);
    let per_passenger_kg = cabin.passenger.checked_bag_mass_kg;
    if !per_passenger_kg.is_finite() || per_passenger_kg <= 0.0 {
        return Ok(0.0);
    }
    Ok(share * passengers * per_passenger_kg)
}

/// [`evaluate_product`] with the FLOPS design gross mass `DG` declared
/// separately from the takeoff-mass requirement.
///
/// `None` sizes at `requirements.mtow_kg`, the takeoff mass of the case being
/// evaluated. `Some` is the declared structural design weight: the
/// `flops_structure.design_gross_mass_kg` override, which is also how a
/// fixed-aircraft mission closure keeps the surface-controls term at the
/// aircraft's design weight while the closure mass moves.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_product_at_design_gross_mass(
    plane: &Airplane,
    requirements: &DesignRequirements,
    geometry: &GeometryConfig,
    controls: &ControlSurfacesConfig,
    cabin: &CabinConfig,
    flops: &FlopsTransportConfig,
    turboprop: &FlopsTurbopropConfig,
    design_gross_mass_kg: Option<f64>,
) -> FlopsTransportEvaluation {
    let mut reasons = Vec::new();
    // Only equations 121 and 122 read an engine rating. A turbofan feeds them
    // its rated thrust; a propeller installation has none, and takes the
    // thrust-free substitutions declared by `PropulsionSizing::ShaftPower`
    // instead of being given a thrust it does not have.
    let (rated_thrust_per_engine_n, propulsion_sizing) = match geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => (
            spec.rated_thrust_kn * 1_000.0,
            PropulsionSizing::RatedThrust,
        ),
        Ok(ActiveEngineModel::Turboprop(_)) => (
            0.0,
            PropulsionSizing::ShaftPower {
                engine_oil_kg: turboprop.engine_oil_mass_kg,
            },
        ),
        Err(_) => {
            reasons.push(FlopsTransportUnverifiedReason::InvalidResolvedInput);
            (0.0, PropulsionSizing::RatedThrust)
        }
    };
    let wing = main_wing(plane);
    let fuselage = primary_fuselage(plane);
    if wing.is_none() {
        reasons.push(FlopsTransportUnverifiedReason::MainWingGeometry);
    }
    if fuselage.is_none() {
        reasons.push(FlopsTransportUnverifiedReason::FuselageGeometry);
    }
    let (Some(wing), Some(fuselage)) = (wing, fuselage) else {
        return unavailable(reasons);
    };
    let (width, depth) = max_fuselage_width_depth(fuselage);
    let fuselage_length = fuselage
        .xsecs
        .last()
        .map(|last| last.xyz_c[0])
        .unwrap_or(0.0)
        - fuselage
            .xsecs
            .first()
            .map(|first| first.xyz_c[0])
            .unwrap_or(0.0);
    let compartment_length =
        (fuselage_length - geometry.fuselage.cabin_start_x_m - geometry.fuselage.tailcone_length_m)
            .max(0.0);
    let nacelles: Vec<&Fuselage> = plane
        .fuselages
        .iter()
        .filter(|body| body.name.contains("Nacelle"))
        .collect();
    let wing_engines = match count_optional(
        flops.wing_mounted_engine_count,
        FlopsTransportUnverifiedReason::EngineMounting,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0
        }
    };
    let fuselage_engines = match count_optional(
        flops.fuselage_mounted_engine_count,
        FlopsTransportUnverifiedReason::EngineMounting,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0
        }
    };
    let engine_count = match wing_engines.checked_add(fuselage_engines) {
        Some(value) => value,
        None => {
            reasons.push(FlopsTransportUnverifiedReason::EngineMounting);
            0
        }
    };
    let nacelle_diameter = if nacelles.is_empty() {
        geometry.engine.radius_scale_m * 2.0
    } else {
        nacelles
            .iter()
            .flat_map(|body| body.xsecs.iter().map(|xsec| xsec.width))
            .fold(0.0, f64::max)
    };
    let maximum_mach = match positive_optional(
        flops.maximum_mach,
        FlopsTransportUnverifiedReason::MaximumMach,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    let design_range_nmi = match positive_optional(
        flops.design_range_nmi,
        FlopsTransportUnverifiedReason::DesignRange,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    if !flops.provenance.mission.is_declared() {
        reasons.push(FlopsTransportUnverifiedReason::MissionProvenance);
    }
    let flight_crew_count = match count_optional(
        flops.flight_crew_count,
        FlopsTransportUnverifiedReason::FlightCrewCount,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0
        }
    };
    let flight_attendant_count = match count_optional(
        flops.flight_attendant_count,
        FlopsTransportUnverifiedReason::FlightAttendantCount,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0
        }
    };
    let galley_crew_count = match count_optional(
        flops.galley_crew_count,
        FlopsTransportUnverifiedReason::GalleyCrewCount,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0
        }
    };
    let class_values = match (
        flops.first_class_passenger_count,
        flops.business_class_passenger_count,
        flops.tourist_class_passenger_count,
    ) {
        (Some(first), Some(business), Some(tourist)) => [first, business, tourist],
        _ => {
            reasons.push(FlopsTransportUnverifiedReason::PassengerClassCounts);
            [0, 0, 0]
        }
    };
    let [first, business, tourist] = class_values;
    let requested_passengers = match usize::try_from(requirements.num_passengers) {
        Ok(value) => value,
        Err(_) => usize::MAX,
    };
    let passenger_count = first
        .checked_add(business)
        .and_then(|value| value.checked_add(tourist));
    if requirements.aircraft_type == "passenger" {
        match passenger_count {
            Some(value) if value == requested_passengers => {}
            Some(_) | None => reasons.push(FlopsTransportUnverifiedReason::PassengerClassCounts),
        }
    }
    if !flops.provenance.cabin.is_declared() {
        reasons.push(FlopsTransportUnverifiedReason::CabinProvenance);
    }
    if engine_count == 0 || engine_count != geometry.engine.spanwise_positions_m.len() {
        reasons.push(FlopsTransportUnverifiedReason::EngineMounting);
    }
    let hydraulic_pressure_pa = match positive_optional(
        flops.hydraulic_pressure_pa,
        FlopsTransportUnverifiedReason::HydraulicPressure,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    let variable_sweep_penalty = match flops.variable_sweep_penalty {
        Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => value,
        _ => {
            reasons.push(FlopsTransportUnverifiedReason::VariableSweepArchitecture);
            0.0
        }
    };
    let fuel_tank_count = match count_optional(
        flops.fuel_tank_count,
        FlopsTransportUnverifiedReason::FuelTankCount,
    ) {
        Ok(value) if value > 0 => value,
        Ok(_) | Err(_) => {
            reasons.push(FlopsTransportUnverifiedReason::FuelTankCount);
            0
        }
    };
    let maximum_fuel_capacity_kg = match positive_optional(
        flops.maximum_fuel_capacity_kg,
        FlopsTransportUnverifiedReason::MaximumFuelCapacity,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    if !flops.provenance.architecture.is_declared() {
        reasons.push(FlopsTransportUnverifiedReason::ArchitectureProvenance);
    }
    let containerized_cargo_kg = match nonnegative_optional(
        flops.containerized_cargo_kg,
        FlopsTransportUnverifiedReason::ContainerizedCargo,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    // A missing declaration keeps the containerised convention the FLOPS
    // source itself assumes; only a *declared* bulk or mixed architecture
    // changes the tare, and a mixed one without its share is refused by name.
    let cargo_loading = flops.cargo_loading.unwrap_or_default();
    let containerized_baggage_kg = match containerized_baggage_kg(
        requirements,
        cabin,
        cargo_loading,
        flops.containerized_baggage_fraction,
    ) {
        Ok(value) => value,
        Err(reason) => {
            reasons.push(reason);
            0.0
        }
    };
    let Some(movable_surface_area_m2) = movable_surface_area(plane, controls) else {
        reasons.push(FlopsTransportUnverifiedReason::MovableSurfaceGeometry);
        return unavailable(reasons);
    };
    if !fuselage_length.is_finite() || fuselage_length <= 0.0 || width <= 0.0 || depth <= 0.0 {
        reasons.push(FlopsTransportUnverifiedReason::FuselageGeometry);
    }
    let inputs = FlopsTransportInputs {
        maximum_mach,
        design_range_nmi,
        design_gross_mass_kg: design_gross_mass_kg.unwrap_or(requirements.mtow_kg),
        wing_area_m2: wing.reference_area(),
        movable_surface_area_m2,
        wing_span_m: wing.reference_span(),
        quarter_chord_sweep_deg: wing.mean_sweep_angle(0.25),
        fuselage_length_m: fuselage_length,
        fuselage_width_m: width,
        fuselage_depth_m: depth,
        fuselage_count: 1,
        passenger_compartment_length_m: compartment_length,
        first_class_passenger_count: first,
        business_class_passenger_count: business,
        tourist_class_passenger_count: tourist,
        flight_crew_count,
        flight_attendant_count,
        galley_crew_count,
        wing_mounted_engine_count: wing_engines,
        fuselage_mounted_engine_count: fuselage_engines,
        engine_count,
        rated_thrust_per_engine_n,
        nacelle_diameter_m: nacelle_diameter,
        hydraulic_pressure_pa,
        variable_sweep_penalty,
        maximum_fuel_capacity_kg,
        fuel_tank_count,
        containerized_cargo_kg,
        containerized_baggage_kg,
        apu_installed: flops.apu_installed,
        cargo_loading,
        cabin_equipment_method: flops.cabin_equipment_method,
        haul_class: flops.haul_class.unwrap_or_default(),
        propulsion_sizing,
    };
    if !inputs.wing_area_m2.is_finite() || inputs.wing_area_m2 <= 0.0 {
        reasons.push(FlopsTransportUnverifiedReason::MainWingGeometry);
    }
    if !reasons.is_empty() {
        return unavailable(reasons);
    }
    match estimate_flops_transport(&inputs) {
        Ok(breakdown) => FlopsTransportEvaluation::Verified {
            inputs,
            provenance: Box::new(flops.provenance.clone()),
            breakdown,
        },
        Err(_) => unavailable(vec![FlopsTransportUnverifiedReason::InvalidResolvedInput]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{
        ControlSurfacesConfig, DesignRequirements, FlopsInputProvenance, FlopsTransportConfig,
        FlopsTransportProvenance, GeometryConfig,
    };
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn absent_geometry_is_unverified_instead_of_using_a_mass_fraction() {
        let plane = Airplane {
            name: "empty".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 0.0,
            c_ref: 0.0,
            b_ref: 0.0,
        };
        let result = evaluate_product(
            &plane,
            &DesignRequirements::default(),
            &GeometryConfig::default(),
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &FlopsTransportConfig::default(),
            &alas_config::FlopsTurbopropConfig::default(),
        );
        assert_eq!(
            result.verification_status().as_str(),
            "unverified_architecture"
        );
        let FlopsTransportEvaluation::Unverified { reasons, .. } = result else {
            panic!("missing geometry must not produce a verified systems mass");
        };
        assert!(reasons.contains(&FlopsTransportUnverifiedReason::MainWingGeometry));
        assert!(reasons.contains(&FlopsTransportUnverifiedReason::FuselageGeometry));
    }

    #[test]
    fn product_boundary_rejects_passenger_count_overflow_without_panicking() {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .expect("default geometry is a valid product fixture");
        let mut flops = complete_test_config();
        flops.first_class_passenger_count = Some(usize::MAX);
        flops.business_class_passenger_count = Some(1);
        flops.tourist_class_passenger_count = Some(0);
        let result = evaluate_product(
            &plane,
            &DesignRequirements::default(),
            &geometry,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &flops,
            &alas_config::FlopsTurbopropConfig::default(),
        );
        let FlopsTransportEvaluation::Unverified { reasons, .. } = result else {
            panic!("overflowed passenger counts must remain unverified");
        };
        assert!(reasons.contains(&FlopsTransportUnverifiedReason::PassengerClassCounts));
    }

    #[test]
    fn a_complete_declared_architecture_reaches_the_published_equations() {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .expect("default geometry is a valid product fixture");
        let requirements = DesignRequirements::default();
        let flops = FlopsTransportConfig {
            maximum_mach: Some(0.84),
            design_range_nmi: Some(3_000.0),
            flight_crew_count: Some(2),
            flight_attendant_count: Some(6),
            galley_crew_count: Some(1),
            first_class_passenger_count: Some(0),
            business_class_passenger_count: Some(50),
            tourist_class_passenger_count: Some(300),
            hydraulic_pressure_pa: Some(20_684_271.879_504),
            variable_sweep_penalty: Some(0.0),
            wing_mounted_engine_count: Some(2),
            fuselage_mounted_engine_count: Some(0),
            fuel_tank_count: Some(4),
            maximum_fuel_capacity_kg: Some(100_000.0),
            apu_installed: true,
            containerized_cargo_kg: Some(0.0),
            cargo_loading: Some(alas_config::CargoHoldLoading::Containerized),
            containerized_baggage_fraction: None,
            cabin_equipment_method: alas_config::CabinEquipmentMethod::FlopsTransportV1,
            haul_class: None,
            provenance: FlopsTransportProvenance {
                mission: complete_provenance("test design mission"),
                cabin: complete_provenance("test cabin layout"),
                architecture: complete_provenance("test installation"),
            },
        };
        let result = evaluate_product(
            &plane,
            &requirements,
            &geometry,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &flops,
            &alas_config::FlopsTurbopropConfig::default(),
        );
        assert_eq!(
            result.verification_status().as_str(),
            "verified_architecture"
        );
        let FlopsTransportEvaluation::Verified {
            breakdown,
            inputs,
            provenance,
        } = result
        else {
            panic!("complete architecture should evaluate FLOPS transport equations: {result:?}");
        };
        assert!(breakdown.systems.avionics_kg > 0.0);
        assert!(breakdown.systems.total_kg > breakdown.systems.avionics_kg);
        assert_eq!(inputs.passenger_count(), 350);
        assert_eq!(inputs.wing_area_m2, plane.wings[0].reference_area());
        assert_eq!(inputs.wing_span_m, plane.wings[0].reference_span());
        assert_eq!(provenance.cabin.document, "test cabin layout");
    }

    #[test]
    fn declared_architecture_changes_avionics_at_equal_mtow_and_retains_provenance() {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .expect("default geometry is a valid product fixture");
        let requirements = DesignRequirements::default();
        let flops = complete_test_config();
        let mut longer_range = flops.clone();
        longer_range.design_range_nmi = Some(6_000.0);
        longer_range.provenance.mission.location = "test long-range mission table".to_owned();

        let baseline = evaluate_product(
            &plane,
            &requirements,
            &geometry,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &flops,
            &alas_config::FlopsTurbopropConfig::default(),
        );
        let long_range = evaluate_product(
            &plane,
            &requirements,
            &geometry,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &longer_range,
            &alas_config::FlopsTurbopropConfig::default(),
        );
        let FlopsTransportEvaluation::Verified {
            inputs: baseline_inputs,
            breakdown: baseline_breakdown,
            ..
        } = baseline
        else {
            panic!("complete baseline declaration must be verified");
        };
        let FlopsTransportEvaluation::Verified {
            inputs: long_range_inputs,
            provenance,
            breakdown: long_range_breakdown,
        } = long_range
        else {
            panic!("complete long-range declaration must be verified");
        };

        assert_eq!(
            baseline_inputs.design_gross_mass_kg,
            long_range_inputs.design_gross_mass_kg
        );
        assert!(long_range_breakdown.systems.avionics_kg > baseline_breakdown.systems.avionics_kg);
        assert_eq!(
            provenance.mission.location, "test long-range mission table",
            "the result must retain the evidence associated with the evaluated architecture"
        );
    }

    #[test]
    fn a_turboprop_is_never_given_a_thrust_and_takes_the_thrust_free_substitutions() {
        // NASA/TM-2017-219627 Vol. I has no propeller, gearbox or shaft-power
        // mass equation, and Appendix D lists no power variable. Of the
        // systems and operating-item equations this adapter feeds, only the
        // unusable fuel (121) and the engine oil (122) read a rating at all,
        // so the resolved thrust must stay exactly zero and those two terms
        // must come from the declared shaft-power substitutions instead.
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .unwrap_or_else(|error| panic!("default geometry builds: {error}"));
        let turboprop_config =
            alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
                .unwrap_or_else(|error| panic!("the ATR preset loads: {error}"));
        let mut turboprop_geometry = geometry;
        turboprop_geometry.engine = turboprop_config.geometry.engine;
        let Ok(ActiveEngineModel::Turboprop(_)) = turboprop_geometry.engine.active_model() else {
            panic!("the ATR baseline must resolve as a turboprop model");
        };
        let declared = alas_config::FlopsTurbopropConfig {
            engine_oil_mass_kg: 26.0,
            ..alas_config::FlopsTurbopropConfig::default()
        };
        let result = evaluate_product(
            &plane,
            &DesignRequirements::default(),
            &turboprop_geometry,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &complete_test_config(),
            &declared,
        );
        let FlopsTransportEvaluation::Verified {
            inputs, breakdown, ..
        } = result
        else {
            panic!("a declared turboprop cabin and architecture must evaluate: {result:?}");
        };
        // No thrust is manufactured from shaft power anywhere on this path.
        assert_eq!(inputs.rated_thrust_per_engine_n, 0.0);
        assert_eq!(
            inputs.propulsion_sizing,
            PropulsionSizing::ShaftPower {
                engine_oil_kg: 26.0
            }
        );
        // Equation 161 replaces 121; the declared oil replaces 122.
        assert!(
            (breakdown.operating_items.unusable_fuel_kg - 0.0084 * inputs.maximum_fuel_capacity_kg)
                .abs()
                < 1e-9
        );
        assert_eq!(breakdown.operating_items.engine_oil_kg, 26.0);
        // Every systems mass is still a real, positive FLOPS result: none of
        // those equations reads a rating, so a propeller does not degrade them.
        assert!(breakdown.systems.total_kg > 0.0);
        assert!(breakdown.systems.avionics_kg > 0.0);
        assert!(breakdown.systems.furnishings_kg > 0.0);
    }

    #[test]
    fn a_turbofan_with_no_rated_thrust_is_still_refused() {
        // The propeller branch must not become a way of slipping a
        // zero-thrust turbofan past the check.
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .unwrap_or_else(|error| panic!("default geometry builds: {error}"));
        let mut zero_thrust = geometry;
        if let Some(spec) = zero_thrust.engine.turbofan.as_mut() {
            spec.rated_thrust_kn = 0.0;
        }
        let result = evaluate_product(
            &plane,
            &DesignRequirements::default(),
            &zero_thrust,
            &ControlSurfacesConfig::default(),
            &alas_config::CabinConfig::default(),
            &complete_test_config(),
            &alas_config::FlopsTurbopropConfig::default(),
        );
        assert_eq!(
            result.verification_status().as_str(),
            "unverified_architecture"
        );
    }

    /// The container tare is hardware, so it exists only on an aircraft whose
    /// holds take a container. A bulk-loaded aircraft must reach equations
    /// 125-126 with nothing in the containers, and a declared mixed
    /// arrangement without its share must be refused by name rather than
    /// given one.
    #[test]
    fn only_a_containerised_hold_charges_a_unit_load_device_tare() {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .expect("default geometry is a valid product fixture");
        let requirements = DesignRequirements::default();
        let cabin = alas_config::CabinConfig::default();
        let evaluate = |flops: &FlopsTransportConfig| {
            evaluate_product(
                &plane,
                &requirements,
                &geometry,
                &ControlSurfacesConfig::default(),
                &cabin,
                flops,
                &alas_config::FlopsTurbopropConfig::default(),
            )
        };

        let mut containerised = complete_test_config();
        containerised.cargo_loading = Some(alas_config::CargoHoldLoading::Containerized);
        let FlopsTransportEvaluation::Verified {
            breakdown: with_containers,
            inputs: containerised_inputs,
            ..
        } = evaluate(&containerised)
        else {
            panic!("a containerised declaration must evaluate");
        };
        assert!(
            with_containers.operating_items.cargo_containers_kg > 0.0,
            "the fixture must carry checked baggage so the tare is exercised"
        );
        assert!(containerised_inputs.containerized_baggage_kg > 0.0);

        let mut bulk = complete_test_config();
        bulk.cargo_loading = Some(alas_config::CargoHoldLoading::Bulk);
        let FlopsTransportEvaluation::Verified {
            breakdown: without_containers,
            inputs: bulk_inputs,
            ..
        } = evaluate(&bulk)
        else {
            panic!("a bulk declaration must evaluate");
        };
        assert_eq!(bulk_inputs.containerized_baggage_kg, 0.0);
        assert_eq!(without_containers.operating_items.cargo_containers_kg, 0.0);
        // Nothing else in the buildup may move: the loading architecture
        // decides the tare and only the tare.
        assert_eq!(with_containers.systems, without_containers.systems);
        // And the tare is outside the operating-empty boundary, so the loading
        // architecture cannot move an operating empty mass at all. The whole
        // difference between the two declarations appears in FLOPS' own
        // `WOPIT` and nowhere else.
        assert_eq!(
            with_containers.operating_items.total_kg,
            without_containers.operating_items.total_kg
        );
        assert!(
            (with_containers
                .operating_items
                .total_with_cargo_containers_kg
                - without_containers
                    .operating_items
                    .total_with_cargo_containers_kg
                - with_containers.operating_items.cargo_containers_kg)
                .abs()
                < 1e-9
        );

        // A mixed arrangement carries part of the baggage in the bulk hold,
        // and the share it carries has to be declared, not assumed.
        let mut mixed = complete_test_config();
        mixed.cargo_loading = Some(alas_config::CargoHoldLoading::Mixed);
        let FlopsTransportEvaluation::Unverified { reasons, .. } = evaluate(&mixed) else {
            panic!("a mixed hold with no declared share must not evaluate");
        };
        assert!(reasons.contains(&FlopsTransportUnverifiedReason::CargoHoldLoading));
        mixed.containerized_baggage_fraction = Some(0.876);
        let FlopsTransportEvaluation::Verified {
            inputs: mixed_inputs,
            ..
        } = evaluate(&mixed)
        else {
            panic!("a declared mixed share must evaluate");
        };
        assert!(
            (mixed_inputs.containerized_baggage_kg
                - 0.876 * containerised_inputs.containerized_baggage_kg)
                .abs()
                < 1e-9
        );

        // An undeclared aircraft keeps the convention the FLOPS source itself
        // assumes rather than silently losing the tare.
        let mut undeclared = complete_test_config();
        undeclared.cargo_loading = None;
        let FlopsTransportEvaluation::Verified {
            inputs: undeclared_inputs,
            ..
        } = evaluate(&undeclared)
        else {
            panic!("an undeclared loading architecture must still evaluate");
        };
        assert_eq!(
            undeclared_inputs.containerized_baggage_kg,
            containerised_inputs.containerized_baggage_kg
        );
    }

    fn complete_provenance(document: &str) -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: document.to_owned(),
            revision: "test revision".to_owned(),
            location: "test locator".to_owned(),
            applicability: "test configured variant".to_owned(),
            evidence: alas_config::FlopsInputEvidence::UserDeclared,
            uncertainty: "test fixture".to_owned(),
        }
    }

    fn complete_test_config() -> FlopsTransportConfig {
        FlopsTransportConfig {
            maximum_mach: Some(0.84),
            design_range_nmi: Some(3_000.0),
            flight_crew_count: Some(2),
            flight_attendant_count: Some(6),
            galley_crew_count: Some(1),
            first_class_passenger_count: Some(0),
            business_class_passenger_count: Some(50),
            tourist_class_passenger_count: Some(300),
            hydraulic_pressure_pa: Some(20_684_271.879_504),
            variable_sweep_penalty: Some(0.0),
            wing_mounted_engine_count: Some(2),
            fuselage_mounted_engine_count: Some(0),
            fuel_tank_count: Some(4),
            maximum_fuel_capacity_kg: Some(100_000.0),
            apu_installed: true,
            containerized_cargo_kg: Some(0.0),
            cargo_loading: Some(alas_config::CargoHoldLoading::Containerized),
            containerized_baggage_fraction: None,
            cabin_equipment_method: alas_config::CabinEquipmentMethod::FlopsTransportV1,
            haul_class: None,
            provenance: FlopsTransportProvenance {
                mission: complete_provenance("test design mission"),
                cabin: complete_provenance("test cabin layout"),
                architecture: complete_provenance("test installation"),
            },
        }
    }
}
