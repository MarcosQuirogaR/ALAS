// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolve a built ALAS transport into the FLOPS structural and propulsion
//! inputs, and evaluate whichever of the two groups is selected.
//!
//! Geometry comes from the built airplane (planform, thickness, tails,
//! fuselage, nacelles, engine stations); the architecture data FLOPS cannot
//! see in a geometry (engine mounting, maximum Mach, fuel capacity) comes
//! from the same declared [`alas_config::FlopsTransportConfig`] the systems
//! method reads, and the technology factors from
//! [`alas_config::FlopsStructureConfig`]. A missing datum is reported, never
//! filled from a fraction.

use alas_config::{
    ActiveEngineModel, ControlSurfacesConfig, DesignRequirements, FlopsWingBendingMethod,
    GeometryConfig, MassModelConfig,
};
use alas_geom::aircraft::airplane::Airplane;

use super::airframe_geometry::{
    average_thickness, detailed_stations, dihedral_deg, find_surface, nacelle_dimensions,
    surface_wetted_area,
};
use super::product::{main_wing, max_fuselage_width_depth, movable_surface_area, primary_fuselage};
use super::propulsion::{
    distributed_scaling, estimate_flops_propulsion, pod_mass_kg, total_nacelles,
    FlopsPropulsionBreakdown, FlopsPropulsionInputs,
};
use super::structure::{
    estimate_flops_structure, main_gear_oleo_length_m, nacelle_kg, nose_gear_oleo_length_m,
    FlopsStructureBreakdown, FlopsStructureInputs, FlopsWingInputs, WingBendingFactor,
};
use super::wing_bending::{detailed_bending_factor, DetailedBendingFactor};
use super::{FlopsSystemsBreakdown, FlopsTransportUnverifiedReason as Reason};

/// What the caller wants evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlopsAirframeSelection {
    /// Evaluate the structural group.
    pub structure: bool,
    /// Evaluate the propulsion group.
    pub propulsion: bool,
}

/// How the quantities FLOPS estimates when not declared were obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlopsAirframeSources {
    /// `declared` or `mlw_fraction_of_mtow`.
    pub landing_mass: &'static str,
    /// `declared` or `flops_equation_66`.
    pub main_gear_length: &'static str,
    /// `declared` or `flops_equation_67`.
    pub nose_gear_length: &'static str,
    /// `declared` or `flops_equation_76`.
    pub baseline_engine_mass: &'static str,
}

/// The evaluated groups with the resolved inputs kept for audit.
#[derive(Debug, Clone, PartialEq)]
pub struct FlopsAirframeBreakdown {
    /// Structural group, when selected.
    pub structure: Option<FlopsStructureBreakdown>,
    /// Propulsion group, when selected.
    pub propulsion: Option<FlopsPropulsionBreakdown>,
    /// Structural inputs as evaluated.
    pub structure_inputs: FlopsStructureInputs,
    /// Propulsion inputs as evaluated.
    pub propulsion_inputs: FlopsPropulsionInputs,
    /// The detailed bending factor, when that method was evaluated.
    pub detailed_bending: Option<DetailedBendingFactor>,
    /// Where each estimated quantity came from.
    pub sources: FlopsAirframeSources,
}

/// A verified airframe evaluation or the list of blockers.
#[derive(Debug, Clone, PartialEq)]
pub enum FlopsAirframeEvaluation {
    /// Every input resolved and the selected groups were evaluated.
    Verified(Box<FlopsAirframeBreakdown>),
    /// At least one datum is missing or inconsistent.
    Unverified {
        /// Sorted, deduplicated blockers.
        reasons: Vec<Reason>,
    },
}

/// Everything the resolver reads.
pub struct FlopsAirframeRequest<'a> {
    /// The built aircraft.
    pub plane: &'a Airplane,
    /// Design gross mass and load factor.
    pub requirements: &'a DesignRequirements,
    /// Engine binding and nacelle geometry.
    pub geometry: &'a GeometryConfig,
    /// Movable-surface fractions.
    pub controls: &'a ControlSurfacesConfig,
    /// FLOPS architecture, technology factors and the landing-mass fraction.
    pub mass_model: &'a MassModelConfig,
    /// The verified FLOPS systems group, needed by the detailed wing method.
    pub systems: Option<&'a FlopsSystemsBreakdown>,
    /// Which groups to evaluate.
    pub selection: FlopsAirframeSelection,
}

fn sort_reasons(mut reasons: Vec<Reason>) -> FlopsAirframeEvaluation {
    reasons.sort_unstable();
    reasons.dedup();
    FlopsAirframeEvaluation::Unverified { reasons }
}

/// Resolve and evaluate the selected FLOPS airframe groups.
pub fn evaluate_airframe_product(request: &FlopsAirframeRequest<'_>) -> FlopsAirframeEvaluation {
    let FlopsAirframeRequest {
        plane,
        requirements,
        geometry,
        controls,
        mass_model,
        systems,
        selection,
    } = request;
    let flops = &mass_model.flops_transport;
    let technology = &mass_model.flops_structure;
    let mut reasons = Vec::new();
    if technology.validate().is_err() {
        reasons.push(Reason::StructureConfiguration);
    }

    let (Some(wing), Some(fuselage)) = (main_wing(plane), primary_fuselage(plane)) else {
        reasons.push(Reason::MainWingGeometry);
        reasons.push(Reason::FuselageGeometry);
        return sort_reasons(reasons);
    };
    let horizontal = find_surface(plane, "Horizontal Stabilizer", 1);
    let vertical = find_surface(plane, "Vertical Stabilizer", 2);
    let (Some(horizontal), Some(vertical)) = (horizontal, vertical) else {
        reasons.push(Reason::TailGeometry);
        return sort_reasons(reasons);
    };
    let vertical_count = plane
        .wings
        .iter()
        .filter(|surface| surface.name.contains("Vertical"))
        .count()
        .max(1);

    let wing_thickness = average_thickness(wing);
    if !wing_thickness.is_finite() || wing_thickness <= 0.0 {
        reasons.push(Reason::WingThickness);
    }
    let (width, depth) = max_fuselage_width_depth(fuselage);
    let fuselage_length = fuselage.xsecs.last().map_or(0.0, |x| x.xyz_c[0])
        - fuselage.xsecs.first().map_or(0.0, |x| x.xyz_c[0]);
    if fuselage_length <= 0.0 || width <= 0.0 || depth <= 0.0 {
        reasons.push(Reason::FuselageGeometry);
    }
    let Some(movable_surface_area_m2) = movable_surface_area(plane, controls) else {
        reasons.push(Reason::MovableSurfaceGeometry);
        return sort_reasons(reasons);
    };

    let rated_thrust_per_engine_n = match geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => spec.rated_thrust_kn * 1_000.0,
        Ok(ActiveEngineModel::Turboprop(_)) => {
            reasons.push(Reason::UnsupportedPropulsionTechnology);
            0.0
        }
        Err(_) => {
            reasons.push(Reason::InvalidResolvedInput);
            0.0
        }
    };
    let built_engines = geometry.engine.spanwise_positions_m.len();
    let (wing_engines, fuselage_engines) = match (
        flops.wing_mounted_engine_count,
        flops.fuselage_mounted_engine_count,
    ) {
        (Some(wing), Some(body)) if wing + body == built_engines && built_engines > 0 => {
            (wing, body)
        }
        _ => {
            reasons.push(Reason::EngineMounting);
            (0, 0)
        }
    };
    let engine_count = wing_engines + fuselage_engines;
    let maximum_mach = match flops.maximum_mach {
        Some(value) if value.is_finite() && value > 0.0 => value,
        _ => {
            reasons.push(Reason::MaximumMach);
            0.0
        }
    };
    let maximum_fuel_capacity_kg = match flops.maximum_fuel_capacity_kg {
        Some(value) if value.is_finite() && value > 0.0 => value,
        _ => {
            reasons.push(Reason::MaximumFuelCapacity);
            0.0
        }
    };
    let variable_sweep_penalty = match flops.variable_sweep_penalty {
        Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => value,
        _ => {
            reasons.push(Reason::VariableSweepArchitecture);
            0.0
        }
    };
    let (nacelle_diameter_m, nacelle_length_m) = nacelle_dimensions(plane, geometry);
    if !(nacelle_diameter_m > 0.0 && nacelle_length_m > 0.0) {
        reasons.push(Reason::NacelleGeometry);
    }
    if !reasons.is_empty() {
        return sort_reasons(reasons);
    }

    let baseline_thrust_n = technology
        .baseline_engine_thrust_kn
        .map_or(rated_thrust_per_engine_n, |kn| kn * 1_000.0);
    let propulsion_inputs = FlopsPropulsionInputs {
        engine_count,
        wing_mounted_engine_count: wing_engines,
        fuselage_mounted_engine_count: fuselage_engines,
        rated_thrust_per_engine_n,
        baseline_thrust_n,
        baseline_engine_mass_kg: technology.baseline_engine_mass_kg,
        scaling_exponent: technology.engine_mass_scaling_exponent,
        thrust_reversers_installed: technology.thrust_reversers_installed,
        maximum_mach,
        nacelle_diameter_m,
        maximum_fuel_capacity_kg,
        misc_propulsion_mass_kg: technology.misc_propulsion_mass_kg,
    };
    let propulsion = estimate_flops_propulsion(&propulsion_inputs);
    let scaling = distributed_scaling(
        engine_count,
        wing_engines,
        fuselage_engines,
        rated_thrust_per_engine_n,
        nacelle_diameter_m,
    );

    let design_gross_mass_kg = requirements.mtow_kg;
    let (design_landing_mass_kg, landing_source) = match technology.design_landing_mass_kg {
        Some(value) => (value, "declared"),
        None => (
            design_gross_mass_kg * mass_model.mlw_fraction_mtow,
            "mlw_fraction_of_mtow",
        ),
    };
    let outboard_engine_y = (wing_engines > 0)
        .then(|| {
            geometry
                .engine
                .spanwise_positions_m
                .iter()
                .map(|y| y.abs())
                .fold(0.0, f64::max)
        })
        .filter(|y| *y > 0.0);
    let (main_gear_oleo, main_source) = match technology.main_gear_oleo_length_m {
        Some(value) => (value, "declared"),
        None => (
            main_gear_oleo_length_m(
                scaling.nacelle_diameter_m,
                dihedral_deg(wing),
                outboard_engine_y,
                width,
                fuselage_length,
            ),
            "flops_equation_66",
        ),
    };
    let (nose_gear_oleo, nose_source) = match technology.nose_gear_oleo_length_m {
        Some(value) => (value, "declared"),
        None => (nose_gear_oleo_length_m(main_gear_oleo), "flops_equation_67"),
    };

    let nacelles = total_nacelles(engine_count);
    let nacelle_total_kg = nacelle_kg(
        nacelles,
        nacelle_diameter_m,
        nacelle_length_m,
        rated_thrust_per_engine_n,
    );
    let mut detailed_bending = None;
    let bending = match technology.wing_bending_method {
        FlopsWingBendingMethod::Simplified => WingBendingFactor::Simplified,
        FlopsWingBendingMethod::Detailed => {
            let Some(systems) = systems else {
                return sort_reasons(vec![Reason::DetailedWingRequiresFlopsSystems]);
            };
            let semispan = wing.reference_span() / 2.0;
            let engine_eta: Vec<f64> = geometry
                .engine
                .spanwise_positions_m
                .iter()
                .map(|y| y.abs() / semispan)
                .filter(|eta| *eta > 0.0 && *eta <= 1.0)
                .collect();
            let span_ft = wing.reference_span();
            let aspect_ratio = span_ft * span_ft / wing.reference_area();
            let Some(factor) = detailed_bending_factor(
                &detailed_stations(wing),
                &engine_eta,
                aspect_ratio,
                technology.aeroelastic_tailoring,
                technology.strut_bracing,
            ) else {
                return sort_reasons(vec![Reason::DetailedWingIntegration]);
            };
            detailed_bending = Some(factor);
            WingBendingFactor::Detailed {
                bt: factor.bt,
                bte: factor.bte,
                pod_mass_kg: pod_mass_kg(
                    &propulsion,
                    nacelle_total_kg,
                    systems.instruments_kg,
                    systems.electrical_kg,
                    systems.hydraulics_kg,
                    engine_count,
                ),
            }
        }
    };

    let painted_wetted_area_m2 = surface_wetted_area(wing, wing_thickness)
        + surface_wetted_area(horizontal, average_thickness(horizontal))
        + surface_wetted_area(vertical, average_thickness(vertical))
        + fuselage.area_wetted()
        + nacelles * std::f64::consts::PI * nacelle_diameter_m * nacelle_length_m;

    let structure_inputs = FlopsStructureInputs {
        wing: FlopsWingInputs {
            design_gross_mass_kg,
            wing_area_m2: wing.reference_area(),
            wing_span_m: wing.reference_span(),
            taper_ratio: wing.taper_ratio(),
            quarter_chord_sweep_deg: wing.mean_sweep_angle(0.25),
            thickness_to_chord: wing_thickness,
            movable_surface_area_m2,
            ultimate_load_factor: requirements.ultimate_load_factor,
            composite_utilization: technology.composite_utilization,
            aeroelastic_tailoring: technology.aeroelastic_tailoring,
            strut_bracing: technology.strut_bracing,
            wing_load_fraction: technology.wing_load_fraction,
            fuselage_count: 1,
            variable_sweep_penalty,
            wing_mounted_engine_count: wing_engines,
            bending,
        },
        horizontal_tail_area_m2: horizontal.reference_area(),
        horizontal_tail_taper_ratio: horizontal.taper_ratio(),
        vertical_tail_area_m2: vertical.unfolded_area() / vertical_count as f64,
        vertical_tail_taper_ratio: vertical.taper_ratio(),
        vertical_tail_count: vertical_count,
        fuselage_length_m: fuselage_length,
        fuselage_width_m: width,
        fuselage_depth_m: depth,
        scaled_fuselage_engines: scaling.fuselage_engines,
        military_cargo_floor: technology.military_cargo_floor,
        design_landing_mass_kg,
        main_gear_oleo_length_m: main_gear_oleo,
        nose_gear_oleo_length_m: nose_gear_oleo,
        total_nacelles: nacelles,
        nacelle_diameter_m,
        nacelle_length_m,
        rated_thrust_per_engine_n,
        paint_area_density_kg_m2: technology.paint_area_density_kg_m2,
        painted_wetted_area_m2,
    };
    let structure = selection
        .structure
        .then(|| estimate_flops_structure(&structure_inputs));
    let breakdown = FlopsAirframeBreakdown {
        structure,
        propulsion: selection.propulsion.then_some(propulsion),
        structure_inputs,
        propulsion_inputs,
        detailed_bending,
        sources: FlopsAirframeSources {
            landing_mass: landing_source,
            main_gear_length: main_source,
            nose_gear_length: nose_source,
            baseline_engine_mass: if technology.baseline_engine_mass_kg.is_some() {
                "declared"
            } else {
                "flops_equation_76"
            },
        },
    };
    if !(breakdown
        .structure
        .is_none_or(|group| group.total_kg.is_finite() && group.total_kg > 0.0)
        && breakdown
            .propulsion
            .is_none_or(|group| group.total_kg.is_finite() && group.total_kg > 0.0))
    {
        return sort_reasons(vec![Reason::InvalidResolvedInput]);
    }
    FlopsAirframeEvaluation::Verified(Box::new(breakdown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{
        FlopsInputProvenance, FlopsStructureConfig, FlopsTransportConfig, FlopsTransportProvenance,
    };
    use alas_geom::builder::AircraftBuilder;

    fn provenance(document: &str) -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: document.to_owned(),
            revision: "test".to_owned(),
            location: "test".to_owned(),
            applicability: "test".to_owned(),
        }
    }

    fn declared_mass_model() -> MassModelConfig {
        MassModelConfig {
            flops_transport: FlopsTransportConfig {
                maximum_mach: Some(0.89),
                design_range_nmi: Some(7_000.0),
                flight_crew_count: Some(2),
                flight_attendant_count: Some(10),
                galley_crew_count: Some(1),
                first_class_passenger_count: Some(0),
                business_class_passenger_count: Some(50),
                tourist_class_passenger_count: Some(300),
                hydraulic_pressure_pa: Some(20_684_271.879_504),
                variable_sweep_penalty: Some(0.0),
                wing_mounted_engine_count: Some(4),
                fuselage_mounted_engine_count: Some(0),
                fuel_tank_count: Some(6),
                maximum_fuel_capacity_kg: Some(220_000.0),
                containerized_cargo_kg: Some(0.0),
                provenance: FlopsTransportProvenance {
                    mission: provenance("mission"),
                    cabin: provenance("cabin"),
                    architecture: provenance("architecture"),
                },
            },
            ..MassModelConfig::default()
        }
    }

    fn built_default() -> (Airplane, GeometryConfig) {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .unwrap_or_else(|error| panic!("{error}"));
        (plane, geometry)
    }

    fn evaluate(
        plane: &Airplane,
        geometry: &GeometryConfig,
        mass_model: &MassModelConfig,
        systems: Option<&FlopsSystemsBreakdown>,
    ) -> FlopsAirframeEvaluation {
        let requirements = DesignRequirements::default();
        let controls = ControlSurfacesConfig::default();
        evaluate_airframe_product(&FlopsAirframeRequest {
            plane,
            requirements: &requirements,
            geometry,
            controls: &controls,
            mass_model,
            systems,
            selection: FlopsAirframeSelection {
                structure: true,
                propulsion: true,
            },
        })
    }

    #[test]
    fn an_undeclared_architecture_is_unverified_rather_than_estimated() {
        let (plane, geometry) = built_default();
        let result = evaluate(&plane, &geometry, &MassModelConfig::default(), None);
        let FlopsAirframeEvaluation::Unverified { reasons } = result else {
            panic!("missing engine mounting and Mach must block the airframe method");
        };
        assert!(reasons.contains(&Reason::EngineMounting));
        assert!(reasons.contains(&Reason::MaximumMach));
        assert!(reasons.contains(&Reason::MaximumFuelCapacity));
    }

    #[test]
    fn the_default_geometry_with_a_declared_architecture_evaluates_both_groups() {
        let (plane, geometry) = built_default();
        let mut mass_model = declared_mass_model();
        let engines = geometry.engine.spanwise_positions_m.len();
        mass_model.flops_transport.wing_mounted_engine_count = Some(engines);
        let result = evaluate(&plane, &geometry, &mass_model, None);
        let FlopsAirframeEvaluation::Verified(breakdown) = result else {
            panic!("declared architecture must evaluate: {result:?}");
        };
        let structure = breakdown
            .structure
            .unwrap_or_else(|| panic!("structure selected"));
        let propulsion = breakdown
            .propulsion
            .unwrap_or_else(|| panic!("propulsion selected"));
        assert!(structure.wing.total_kg > 0.0);
        assert!(structure.fuselage_kg > structure.horizontal_tail_kg);
        assert!(structure.main_gear_kg > structure.nose_gear_kg);
        assert!(propulsion.engines_kg > propulsion.fuel_system_kg);
        assert_eq!(breakdown.sources.landing_mass, "mlw_fraction_of_mtow");
        assert_eq!(breakdown.sources.main_gear_length, "flops_equation_66");
        assert_eq!(breakdown.sources.baseline_engine_mass, "flops_equation_76");
        // A large four-engine transport: the structural group is a tenth to
        // a third of its design gross mass in every published statement.
        let fraction = structure.total_kg / DesignRequirements::default().mtow_kg;
        assert!(
            (0.10..=0.35).contains(&fraction),
            "structure fraction {fraction}"
        );
    }

    #[test]
    fn declared_overrides_replace_the_flops_estimates_and_are_recorded() {
        let (plane, geometry) = built_default();
        let mut mass_model = declared_mass_model();
        mass_model.flops_transport.wing_mounted_engine_count =
            Some(geometry.engine.spanwise_positions_m.len());
        mass_model.flops_structure = FlopsStructureConfig {
            design_landing_mass_kg: Some(300_000.0),
            main_gear_oleo_length_m: Some(3.0),
            nose_gear_oleo_length_m: Some(2.0),
            baseline_engine_mass_kg: Some(6_000.0),
            ..FlopsStructureConfig::default()
        };
        let FlopsAirframeEvaluation::Verified(breakdown) =
            evaluate(&plane, &geometry, &mass_model, None)
        else {
            panic!("declared overrides evaluate");
        };
        assert_eq!(breakdown.sources.landing_mass, "declared");
        assert_eq!(breakdown.sources.main_gear_length, "declared");
        assert_eq!(breakdown.sources.nose_gear_length, "declared");
        assert_eq!(breakdown.sources.baseline_engine_mass, "declared");
        assert_eq!(breakdown.structure_inputs.design_landing_mass_kg, 300_000.0);
        assert_eq!(breakdown.structure_inputs.main_gear_oleo_length_m, 3.0);
        let propulsion = breakdown.propulsion.unwrap_or_else(|| panic!("selected"));
        assert!((propulsion.engine_each_kg - 6_000.0).abs() < 1e-9);
    }

    #[test]
    fn the_detailed_wing_method_needs_the_flops_systems_group() {
        let (plane, geometry) = built_default();
        let mut mass_model = declared_mass_model();
        mass_model.flops_transport.wing_mounted_engine_count =
            Some(geometry.engine.spanwise_positions_m.len());
        mass_model.flops_structure.wing_bending_method = FlopsWingBendingMethod::Detailed;
        let FlopsAirframeEvaluation::Unverified { reasons } =
            evaluate(&plane, &geometry, &mass_model, None)
        else {
            panic!("the detailed method cannot price the pod without systems masses");
        };
        assert_eq!(reasons, vec![Reason::DetailedWingRequiresFlopsSystems]);

        let systems = FlopsSystemsBreakdown {
            surface_controls_kg: 2_000.0,
            apu_kg: 500.0,
            instruments_kg: 400.0,
            hydraulics_kg: 1_500.0,
            electrical_kg: 2_000.0,
            avionics_kg: 800.0,
            furnishings_kg: 20_000.0,
            air_conditioning_kg: 1_500.0,
            anti_ice_kg: 200.0,
            total_kg: 28_900.0,
        };
        let FlopsAirframeEvaluation::Verified(breakdown) =
            evaluate(&plane, &geometry, &mass_model, Some(&systems))
        else {
            panic!("the detailed method evaluates with the systems group");
        };
        let factor = breakdown
            .detailed_bending
            .unwrap_or_else(|| panic!("detailed"));
        assert!(factor.bt > 0.0 && factor.bte > 0.0);
        let structure = breakdown.structure.unwrap_or_else(|| panic!("selected"));
        assert!(structure.wing.inertia_relief_factor < 1.0);
        assert!(structure.wing.inertia_relief_factor > 0.5);
    }
}
