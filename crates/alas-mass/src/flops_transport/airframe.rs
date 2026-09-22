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
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};

use super::airframe_geometry::{
    average_thickness, detailed_stations, dihedral_deg, find_surface, nacelle_dimensions,
    surface_wetted_area,
};
use super::movable_area::movable_surface_area;
use super::product::{main_wing, max_fuselage_width_depth, primary_fuselage};
use super::propulsion::{
    distributed_scaling, estimate_flops_propulsion, pod_mass_kg, total_nacelles,
    FlopsPropulsionBreakdown, FlopsPropulsionInputs,
};
use super::structure::{
    estimate_flops_structure, main_gear_oleo_length_m, nacelle_kg, nose_gear_oleo_length_m,
    FlopsStructureBreakdown, FlopsStructureInputs, FlopsWingInputs, WingBendingFactor,
};
use super::turboprop::{
    estimate_turboprop_propulsion, TurbopropMassUnverifiedReason, TurbopropPropulsionBreakdown,
    TurbopropPropulsionInputs,
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
    /// `declared` (a `flops_structure.design_gross_mass_kg` override, which
    /// is also how a fixed-aircraft closure pins the basis) or
    /// `requirements_mtow`.
    pub design_gross_mass: &'static str,
}

/// The evaluated groups with the resolved inputs kept for audit.
#[derive(Debug, Clone, PartialEq)]
pub struct FlopsAirframeBreakdown {
    /// Structural group, when selected.
    pub structure: Option<FlopsStructureBreakdown>,
    /// Thrust-based FLOPS propulsion group, when selected and the
    /// installation is a turbofan.
    pub propulsion: Option<FlopsPropulsionBreakdown>,
    /// Shaft-power propulsion group, when selected and the installation is a
    /// turboprop.
    ///
    /// Exactly one of this and [`Self::propulsion`] is ever populated: the
    /// FLOPS propulsion equations read a rated thrust that a propeller
    /// installation does not have, so the two are alternatives rather than
    /// contributions that could be summed.
    pub turboprop_propulsion: Option<TurbopropPropulsionBreakdown>,
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

/// Resolve the wetted area of one nacelle from the built aircraft whenever
/// nacelle bodies are present. The turboprop evaluator multiplies this
/// per-body area by the installed engine count, so the built areas are
/// averaged only to retain that existing per-engine interface. If a caller
/// intentionally omits engine bodies, rebuild the same configured profile in
/// memory; this keeps `include_engines` from changing a mass input. A
/// cylindrical proxy remains only for a legacy configuration with no profile.
fn turboprop_nacelle_wetted_area_m2(
    plane: &Airplane,
    geometry: &GeometryConfig,
    fallback_diameter_m: f64,
    fallback_length_m: f64,
) -> (f64, &'static str) {
    let bodies: Vec<_> = plane
        .fuselages
        .iter()
        .filter(|body| body.name.contains("Nacelle"))
        .collect();
    if !bodies.is_empty() {
        let areas_m2: Vec<f64> = bodies.iter().map(|body| body.area_wetted()).collect();
        if areas_m2
            .iter()
            .any(|area_m2| !area_m2.is_finite() || *area_m2 <= 0.0)
        {
            // A present but malformed body is a geometry error. Falling
            // through to a proxy here would silently hide a bad build and
            // make the mass depend on whether the body happened to be
            // present.
            return (f64::NAN, "invalid_built_nacelle_fuselage_wetted_area");
        }
        let total_area_m2: f64 = areas_m2.iter().sum();
        let average_area_m2 = total_area_m2 / bodies.len() as f64;
        if average_area_m2.is_finite() && average_area_m2 > 0.0 {
            return (average_area_m2, "built_nacelle_fuselage_wetted_area");
        }
        return (f64::NAN, "invalid_built_nacelle_fuselage_wetted_area");
    }

    let profile = &geometry.engine.nacelle_profile;
    if !profile.is_empty() {
        if profile.len() < 2 {
            return (f64::NAN, "invalid_configured_nacelle_profile");
        }
        if !geometry.engine.radius_scale_m.is_finite() || geometry.engine.radius_scale_m <= 0.0 {
            return (f64::NAN, "invalid_configured_nacelle_profile");
        }
        let xsecs: Result<Vec<_>, _> = profile
            .iter()
            .map(|&(x, radius_fraction)| {
                if !x.is_finite() || !radius_fraction.is_finite() || radius_fraction < 0.0 {
                    return Err(());
                }
                FuselageXSec::new(
                    [x, 0.0, 0.0],
                    Some(geometry.engine.radius_scale_m * radius_fraction),
                    None,
                    None,
                    DEFAULT_SHAPE,
                )
                .map_err(|_| ())
            })
            .collect();
        let Ok(xsecs) = xsecs else {
            return (f64::NAN, "invalid_configured_nacelle_profile");
        };
        let area_m2 = Fuselage::new("Configured Nacelle", xsecs).area_wetted();
        if area_m2.is_finite() && area_m2 > 0.0 {
            return (area_m2, "configured_nacelle_profile_wetted_area");
        }
        return (f64::NAN, "invalid_configured_nacelle_profile");
    }

    // This is intentionally a last-resort compatibility path for older
    // configurations that carry dimensions but no silhouette profile.
    (
        std::f64::consts::PI * fallback_diameter_m * fallback_length_m,
        "configured_cylindrical_nacelle_proxy_fallback",
    )
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

    // A turboprop is evaluated by the shaft-power group of
    // [`super::turboprop`], not by the thrust-based FLOPS propulsion
    // equations. Its rated thrust stays zero and is never read.
    let mut turboprop_spec = None;
    let rated_thrust_per_engine_n = match geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => spec.rated_thrust_kn * 1_000.0,
        Ok(ActiveEngineModel::Turboprop(spec)) => {
            turboprop_spec = Some(spec.clone());
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
    let (nacelle_wetted_area_m2, nacelle_area_basis) =
        turboprop_nacelle_wetted_area_m2(plane, geometry, nacelle_diameter_m, nacelle_length_m);
    if !(nacelle_diameter_m > 0.0 && nacelle_length_m > 0.0) {
        reasons.push(Reason::NacelleGeometry);
    }
    if !nacelle_wetted_area_m2.is_finite() || nacelle_wetted_area_m2 <= 0.0 {
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
        starter_scope: technology.starter_scope,
        // Equations 77-79 only when the configuration declares the inlet or
        // nozzle separately; `FlopsStructureConfig::validate` has already
        // refused a separate item without an explicit baseline core mass, so
        // the equation 76 all-in estimate can never be double counted.
        baseline_inlet_mass_kg: technology.baseline_inlet_mass_kg,
        inlet_scaling_exponent: technology.inlet_mass_scaling_exponent,
        baseline_nozzle_mass_kg: technology.baseline_nozzle_mass_kg,
        nozzle_scaling_exponent: technology.nozzle_mass_scaling_exponent,
        nozzle_scope: technology.nozzle_scope,
        thrust_reversers_installed: technology.thrust_reversers_installed,
        maximum_mach,
        nacelle_diameter_m,
        maximum_fuel_capacity_kg,
        misc_propulsion_mass_kg: technology.misc_propulsion_mass_kg,
        pylon_mass_method: technology.pylon_mass_method,
    };
    let propulsion = estimate_flops_propulsion(&propulsion_inputs);
    // The shaft-power group, when the installation is a propeller one. The
    // nacelle comes with it, because FLOPS equation 69 reads a rated thrust
    // that a turboprop does not have.
    let turboprop = match turboprop_spec.as_ref() {
        None => None,
        Some(spec) => {
            if matches!(
                technology.wing_bending_method,
                FlopsWingBendingMethod::Detailed
            ) {
                // Equations 39-41 relieve the wing with an engine-pod mass
                // built from the thrust-based propulsion terms; there is no
                // published shaft-power form of that pod.
                reasons.push(Reason::UnsupportedPropulsionTechnology);
            }
            let turboprop_inputs = TurbopropPropulsionInputs {
                engine_count,
                takeoff_shaft_power_per_engine_w: spec.takeoff_shaft_power_kw * 1_000.0,
                propeller_speed_rpm: spec.governed_propeller_speed_rpm,
                propeller_diameter_m: spec.propeller_diameter_m,
                reduction_ratio: spec.reduction_ratio,
                design_mach: requirements.cruise_mach,
                nacelle_wetted_area_m2,
                maximum_fuel_capacity_kg,
            };
            match estimate_turboprop_propulsion(
                &turboprop_inputs,
                &mass_model.flops_turboprop,
                maximum_mach,
            ) {
                Ok(mut group) => {
                    group.design_mach_basis = "requirements_cruise_mach";
                    group.nacelle_area_basis = nacelle_area_basis;
                    Some(group)
                }
                Err(blockers) => {
                    reasons.extend(blockers.into_iter().map(|blocker| match blocker {
                        TurbopropMassUnverifiedReason::InvalidConfiguration => {
                            Reason::TurbopropMassConfiguration
                        }
                        TurbopropMassUnverifiedReason::InvalidOperatingPoint => {
                            Reason::TurbopropOperatingPoint
                        }
                        TurbopropMassUnverifiedReason::ShaftPowerRating => {
                            Reason::TurbopropShaftPowerRating
                        }
                        TurbopropMassUnverifiedReason::PropellerGeometry => {
                            Reason::TurbopropPropellerGeometry
                        }
                        TurbopropMassUnverifiedReason::NacelleArchitecture => {
                            Reason::TurbopropNacelleArchitecture
                        }
                    }));
                    None
                }
            }
        }
    };
    if !reasons.is_empty() {
        return sort_reasons(reasons);
    }
    let scaling = distributed_scaling(
        engine_count,
        wing_engines,
        fuselage_engines,
        rated_thrust_per_engine_n,
        nacelle_diameter_m,
    );

    // The structural design gross mass is the declared override when the
    // configuration carries one (a weight-variant declaration, or the
    // fixed-aircraft basis `AlasConfig::at_closure_mass` writes so a mission
    // closure cannot re-size a registered aircraft) and otherwise the
    // takeoff-mass requirement of the case being evaluated.
    let (design_gross_mass_kg, design_gross_source) = match technology.design_gross_mass_kg {
        Some(value) => (value, "declared"),
        None => (requirements.mtow_kg, "requirements_mtow"),
    };
    // This function has no `AlasConfig`/`DesignMode`/preset identity in scope
    // (only `DesignRequirements` and `MassModelConfig`, via
    // `FlopsAirframeRequest`), so it cannot call the mode-aware
    // `AlasConfig::landing_mass_limit_kg` resolver itself. Every product call
    // site instead resolves that limit ahead of time and passes it down
    // through the ordinary `mlw_fraction_mtow` slot via
    // `AlasConfig::analysis_mass_model`, so `mass_model.mlw_fraction_mtow`
    // here already *is* the resolved limit divided by the takeoff-mass
    // requirement for those callers. The fraction therefore multiplies
    // `requirements.mtow_kg`, the mass it was derived from, and not the
    // design gross mass, which a declared override may have pinned elsewhere.
    // The plain fraction is only reached by standalone low-level callers
    // that build a `MassModelConfig` directly without going through
    // `AlasConfig`, where the documented fraction semantics still apply.
    let (design_landing_mass_kg, landing_source) = match technology.design_landing_mass_kg {
        Some(value) => (value, "declared"),
        None => (
            requirements.mtow_kg * mass_model.mlw_fraction_mtow,
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
        // FLOPS equation 69 reads a rated thrust, so a propeller installation
        // takes the NASA GASP area-density nacelle instead. Either way the
        // nacelle reaches the structural total once, from one method.
        nacelle_mass_override_kg: turboprop.map(|group| group.nacelles_kg),
    };
    let structure = selection
        .structure
        .then(|| estimate_flops_structure(&structure_inputs));
    let breakdown = FlopsAirframeBreakdown {
        structure,
        propulsion: selection
            .propulsion
            .then_some(propulsion)
            .filter(|_| turboprop.is_none()),
        turboprop_propulsion: selection.propulsion.then_some(turboprop).flatten(),
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
            design_gross_mass: design_gross_source,
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
        FlopsInputEvidence, FlopsInputProvenance, FlopsStructureConfig, FlopsTransportConfig,
        FlopsTransportProvenance,
    };
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::builder::AircraftBuilder;

    fn provenance(document: &str) -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: document.to_owned(),
            revision: "test".to_owned(),
            location: "test".to_owned(),
            applicability: "test".to_owned(),
            evidence: FlopsInputEvidence::UserDeclared,
            uncertainty: "test fixture".to_owned(),
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
                apu_installed: true,
                containerized_cargo_kg: Some(0.0),
                cargo_loading: Some(alas_config::CargoHoldLoading::Containerized),
                containerized_baggage_fraction: None,
                cabin_equipment_method: alas_config::CabinEquipmentMethod::FlopsTransportV1,
                haul_class: None,
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
        let model = MassModelConfig {
            flops_transport: FlopsTransportConfig::default(),
            ..MassModelConfig::default()
        };
        let result = evaluate(&plane, &geometry, &model, None);
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
    fn choosing_flops_for_the_atr_baseline_uses_the_declared_mlw_not_the_generic_fraction() {
        let (plane, geometry) = built_default();
        let mut config =
            alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
                .unwrap();
        config.optimizer.design_space.mode = alas_config::DesignMode::BaselineSandbox;
        assert_eq!(config.requirements.mtow_kg, 23_000.0);
        // Product callers feed the resolved reference landing limit through
        // `AlasConfig::analysis_mass_model` rather than the saved
        // `mlw_fraction_mtow`; only that derived model should reproduce the
        // preset's declared 22_350 kg MLW here.
        let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
        let mut mass_model = declared_mass_model();
        mass_model.mlw_fraction_mtow = analysis_mass_model.mlw_fraction_mtow;
        mass_model.flops_transport.wing_mounted_engine_count =
            Some(geometry.engine.spanwise_positions_m.len());
        let controls = ControlSurfacesConfig::default();
        let result = evaluate_airframe_product(&FlopsAirframeRequest {
            plane: &plane,
            requirements: &config.requirements,
            geometry: &geometry,
            controls: &controls,
            mass_model: &mass_model,
            systems: None,
            selection: FlopsAirframeSelection {
                structure: true,
                propulsion: true,
            },
        });
        let FlopsAirframeEvaluation::Verified(breakdown) = result else {
            panic!("declared architecture must evaluate: {result:?}");
        };
        assert_eq!(breakdown.sources.landing_mass, "mlw_fraction_of_mtow");
        assert!(
            (breakdown.structure_inputs.design_landing_mass_kg - 22_350.0).abs() < 1e-6,
            "{}",
            breakdown.structure_inputs.design_landing_mass_kg
        );
    }

    #[test]
    fn turboprop_nacelle_area_uses_the_built_profile_with_or_without_bodies() {
        let config =
            alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
                .expect("registered ATR preset");
        let geometry = config.geometry.clone();
        let built = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, true)
            .expect("ATR geometry with nacelles");
        let omitted = AircraftBuilder::new(Some(geometry.clone()))
            .build(None, false)
            .expect("ATR geometry without nacelles");
        let (diameter_m, length_m) = nacelle_dimensions(&built, &geometry);
        let (built_area_m2, built_basis) =
            turboprop_nacelle_wetted_area_m2(&built, &geometry, diameter_m, length_m);
        let (omitted_area_m2, omitted_basis) =
            turboprop_nacelle_wetted_area_m2(&omitted, &geometry, diameter_m, length_m);
        assert_eq!(built_basis, "built_nacelle_fuselage_wetted_area");
        assert_eq!(omitted_basis, "configured_nacelle_profile_wetted_area");
        assert!(built_area_m2 > 0.0);
        assert!((built_area_m2 - omitted_area_m2).abs() < 1.0e-12);
    }

    #[test]
    fn malformed_nacelle_geometry_is_not_replaced_by_a_cylindrical_proxy() {
        let geometry = GeometryConfig::default();
        let malformed_body = Fuselage::new(
            "Nacelle malformed",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("valid section"),
            ],
        );
        let body_plane = Airplane {
            name: "malformed".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: vec![malformed_body],
            s_ref: 0.0,
            c_ref: 0.0,
            b_ref: 0.0,
        };
        let (body_area_m2, body_basis) =
            turboprop_nacelle_wetted_area_m2(&body_plane, &geometry, 1.0, 3.0);
        assert!(body_area_m2.is_nan());
        assert_eq!(body_basis, "invalid_built_nacelle_fuselage_wetted_area");

        let mut malformed_profile = geometry;
        malformed_profile.engine.nacelle_profile = vec![(0.0, 0.5)];
        let empty_plane = Airplane {
            name: "empty".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 0.0,
            c_ref: 0.0,
            b_ref: 0.0,
        };
        let (profile_area_m2, profile_basis) =
            turboprop_nacelle_wetted_area_m2(&empty_plane, &malformed_profile, 1.0, 3.0);
        assert!(profile_area_m2.is_nan());
        assert_eq!(profile_basis, "invalid_configured_nacelle_profile");
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
        assert_eq!(breakdown.sources.design_gross_mass, "requirements_mtow");
        assert_eq!(breakdown.structure_inputs.design_landing_mass_kg, 300_000.0);
        assert_eq!(breakdown.structure_inputs.main_gear_oleo_length_m, 3.0);
        let propulsion = breakdown.propulsion.unwrap_or_else(|| panic!("selected"));
        assert!((propulsion.engine_each_kg - 6_000.0).abs() < 1e-9);
    }

    #[test]
    fn a_declared_design_gross_mass_sizes_the_structure_without_moving_the_landing_fallback() {
        // A fixed aircraft whose closure mass has dropped below its declared
        // design weight: the wing, tails and fuselage must still be sized at
        // the declared `DG`, while the landing-mass fraction keeps
        // multiplying the takeoff-mass requirement it was derived from.
        let (plane, geometry) = built_default();
        let mut mass_model = declared_mass_model();
        mass_model.flops_transport.wing_mounted_engine_count =
            Some(geometry.engine.spanwise_positions_m.len());
        let controls = ControlSurfacesConfig::default();
        let mut requirements = DesignRequirements::default();
        let declared_mtow_kg = requirements.mtow_kg;
        let evaluate_with = |requirements: &DesignRequirements, mass_model: &MassModelConfig| {
            match evaluate_airframe_product(&FlopsAirframeRequest {
                plane: &plane,
                requirements,
                geometry: &geometry,
                controls: &controls,
                mass_model,
                systems: None,
                selection: FlopsAirframeSelection {
                    structure: true,
                    propulsion: true,
                },
            }) {
                FlopsAirframeEvaluation::Verified(breakdown) => breakdown,
                other => panic!("declared architecture evaluates: {other:?}"),
            }
        };
        let at_design = evaluate_with(&requirements, &mass_model);
        assert_eq!(at_design.sources.design_gross_mass, "requirements_mtow");

        // Closure mass below the design weight, coupled: the wing shrinks.
        requirements.mtow_kg = 0.8 * declared_mtow_kg;
        let coupled = evaluate_with(&requirements, &mass_model);
        let wing = |b: &FlopsAirframeBreakdown| b.structure.map_or(0.0, |s| s.wing.total_kg);
        assert!(wing(&coupled) < wing(&at_design));

        // The same closure mass with the design weight pinned: the wing is
        // the design-weight wing again, and the gear follows the fraction of
        // the closure mass exactly as the fraction contract says.
        mass_model.flops_structure.design_gross_mass_kg = Some(declared_mtow_kg);
        let pinned = evaluate_with(&requirements, &mass_model);
        assert_eq!(pinned.sources.design_gross_mass, "declared");
        assert!((wing(&pinned) - wing(&at_design)).abs() < 1.0e-9);
        assert!(
            (pinned.structure_inputs.design_landing_mass_kg
                - requirements.mtow_kg * mass_model.mlw_fraction_mtow)
                .abs()
                < 1.0e-9
        );
        assert!(
            (pinned.structure_inputs.design_landing_mass_kg
                - coupled.structure_inputs.design_landing_mass_kg)
                .abs()
                < 1.0e-9
        );
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
