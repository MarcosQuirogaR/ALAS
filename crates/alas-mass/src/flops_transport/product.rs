// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolve a built ALAS transport into the physical inputs required by FLOPS.
//!
//! The equations are useful only when their architecture inputs are declared.
//! This adapter therefore returns an explicit `Unverified` result instead of
//! filling absent range, crew, cabin, hydraulic, or fuel-system data from a
//! preset or from a generic percentage.

use alas_config::{
    ControlSurfacesConfig, DesignRequirements, FlopsTransportConfig, GeometryConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

use super::{
    estimate_flops_transport, FlopsTransportEvaluation, FlopsTransportInputs,
    FlopsTransportUnverifiedReason, PartialFlopsTransportBreakdown,
};

fn main_wing(plane: &Airplane) -> Option<&Wing> {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())
        .filter(|wing| wing.xsecs.len() >= 2)
}

fn primary_fuselage(plane: &Airplane) -> Option<&Fuselage> {
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

fn max_fuselage_width_depth(fuselage: &Fuselage) -> (f64, f64) {
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

fn movable_surface_area(plane: &Airplane, controls: &ControlSurfacesConfig) -> Option<f64> {
    let wing = main_wing(plane)?;
    let hstab = plane
        .wings
        .iter()
        .find(|surface| surface.name == "Horizontal Stabilizer")
        .or_else(|| plane.wings.get(1))?;
    let vstab = plane
        .wings
        .iter()
        .find(|surface| surface.name == "Vertical Stabilizer")
        .or_else(|| plane.wings.get(2))?;
    let wing_fraction = |chord: f64, start: f64, end: f64| chord.max(0.0) * (end - start).max(0.0);
    let wing_area = wing.area();
    let hstab_area = hstab.area();
    let vstab_area = vstab.area();
    let movable = wing_area
        * (wing_fraction(
            controls.slat_chord_fraction,
            controls.slat_span_start_frac,
            controls.slat_span_end_frac,
        ) + wing_fraction(
            controls.flap_chord_fraction,
            controls.flap_span_start_frac,
            controls.flap_span_end_frac,
        ) + wing_fraction(
            controls.aileron_chord_fraction,
            controls.aileron_span_start_frac,
            controls.aileron_span_end_frac,
        ) + wing_fraction(
            controls.spoiler_chord_fraction,
            controls.spoiler_span_start_frac,
            controls.spoiler_span_end_frac,
        ))
        + hstab_area
            * wing_fraction(
                controls.elevator_chord_fraction,
                controls.elevator_span_start_frac,
                controls.elevator_span_end_frac,
            )
        + vstab_area
            * wing_fraction(
                controls.rudder_chord_fraction,
                controls.rudder_span_start_frac,
                controls.rudder_span_end_frac,
            );
    (movable.is_finite() && movable > 0.0).then_some(movable)
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
    flops: &FlopsTransportConfig,
) -> FlopsTransportEvaluation {
    let mut reasons = Vec::new();
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
    let engine_count = wing_engines + fuselage_engines;
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
    if requirements.aircraft_type == "passenger"
        && first + business + tourist != requested_passengers
    {
        reasons.push(FlopsTransportUnverifiedReason::PassengerClassCounts);
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
        design_gross_mass_kg: requirements.mtow_kg,
        wing_area_m2: wing.area(),
        movable_surface_area_m2,
        wing_span_m: wing.span(),
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
        rated_thrust_per_engine_n: geometry.engine.thrust_kn * 1_000.0,
        nacelle_diameter_m: nacelle_diameter,
        hydraulic_pressure_pa,
        variable_sweep_penalty,
        maximum_fuel_capacity_kg,
        fuel_tank_count,
        containerized_cargo_kg,
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
            &FlopsTransportConfig::default(),
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
            containerized_cargo_kg: Some(0.0),
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
            &flops,
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
            &flops,
        );
        let long_range = evaluate_product(
            &plane,
            &requirements,
            &geometry,
            &ControlSurfacesConfig::default(),
            &longer_range,
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

    fn complete_provenance(document: &str) -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: document.to_owned(),
            revision: "test revision".to_owned(),
            location: "test locator".to_owned(),
            applicability: "test configured variant".to_owned(),
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
            containerized_cargo_kg: Some(0.0),
            provenance: FlopsTransportProvenance {
                mission: complete_provenance("test design mission"),
                cabin: complete_provenance("test cabin layout"),
                architecture: complete_provenance("test installation"),
            },
        }
    }
}
