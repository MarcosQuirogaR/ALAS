// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.3 evidence for the fuselage mass correlation.
//!
//! This test compares the reference and Rust inputs in order: configuration,
//! aircraft geometry, then each intermediate in Torenbeek Eq. 8-16 and the
//! resulting fuselage mass. A plain assertion is intentional here. W6.3 is an
//! evidence handoff, so the first divergence is the useful result and later
//! values must not obscure it with a second diagnosis.

// The checked-in fixture is test input; malformed evidence should fail at the
// field being decoded instead of turning every comparison helper into a Result.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{
    DesignRequirements, FlopsTransportConfig, GeometryConfig, MassArchitecture, MassModelConfig,
};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::calculate_component_masses;
use alas_mass::torenbeek::mass_fuselage_simple;
use alas_testkit::{agrees, load, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct DesignVectorRecord {
    span_m: f64,
    root_chord_m: f64,
    break_chord_m: f64,
    tip_chord_m: f64,
    sweep_deg: f64,
    tip_twist_deg: f64,
    wing_x_shift_m: f64,
    tail_scale: f64,
    fuselage_length_m: f64,
    tail_x_shift_m: f64,
    airfoil_thickness_scale: f64,
    airfoil_camber_scale: f64,
    bump_upper_front: f64,
    bump_upper_rear: f64,
    bump_lower_mid: f64,
    bump_lower_rear: f64,
}

impl DesignVectorRecord {
    fn build(&self) -> alas_config::DesignVector {
        alas_config::DesignVector {
            span_m: self.span_m,
            root_chord_m: self.root_chord_m,
            break_chord_m: self.break_chord_m,
            tip_chord_m: self.tip_chord_m,
            sweep_deg: self.sweep_deg,
            tip_twist_deg: self.tip_twist_deg,
            wing_x_shift_m: self.wing_x_shift_m,
            tail_scale: self.tail_scale,
            fuselage_length_m: self.fuselage_length_m,
            tail_x_shift_m: self.tail_x_shift_m,
            airfoil_thickness_scale: self.airfoil_thickness_scale,
            airfoil_camber_scale: self.airfoil_camber_scale,
            bump_upper_front: self.bump_upper_front,
            bump_upper_rear: self.bump_upper_rear,
            bump_lower_mid: self.bump_lower_mid,
            bump_lower_rear: self.bump_lower_rear,
        }
    }
}

#[derive(Debug, Deserialize)]
struct XsecGeometry {
    xyz_le_m: [f64; 3],
    chord_m: f64,
    twist_deg: f64,
}

#[derive(Debug, Deserialize)]
struct WingGeometry {
    aerodynamic_center_m: [f64; 3],
    xsecs: Vec<XsecGeometry>,
}

#[derive(Debug, Deserialize)]
struct FuselageXsecGeometry {
    xyz_c_m: [f64; 3],
    width_m: f64,
    height_m: f64,
    shape: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageGeometry {
    area_wetted_m2: f64,
    xsecs: Vec<FuselageXsecGeometry>,
}

#[derive(Debug, Deserialize)]
struct AircraftGeometry {
    main_wing: WingGeometry,
    horizontal_stabilizer: WingGeometry,
    fuselage: FuselageGeometry,
}

#[derive(Debug, Deserialize)]
struct SoftmaxTrace {
    values: Vec<f64>,
    softness: f64,
    scaled_values: Vec<f64>,
    scaled_max: f64,
    exponential_sum: f64,
    logsumexp: f64,
    softmax: f64,
}

#[derive(Debug, Deserialize)]
struct CorrelationTrace {
    width: SoftmaxTrace,
    height: SoftmaxTrace,
    mean_width_m: f64,
    mean_height_m: f64,
    max_width_m: f64,
    max_height_m: f64,
    width_height_sum_m: f64,
    never_exceed_airspeed_m_s: f64,
    wing_to_tail_distance_m: f64,
    speed_distance_over_sum_m_s: f64,
    square_root_term: f64,
    area_wetted_m2: f64,
    area_wetted_power_1_2: f64,
    coefficient: f64,
    fuselage_mass_kg: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    design_vector: DesignVectorRecord,
    requirements_overrides: Map<String, Value>,
    mass_model_overrides: Map<String, Value>,
    requirements: Value,
    mass_model: Value,
    geometry_config: Value,
    aircraft_geometry: AircraftGeometry,
    model_choice: Map<String, Value>,
    correlation: CorrelationTrace,
    direct_mass_fuselage_kg: f64,
    component_mass_fuselage_kg: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    model_choice: Map<String, Value>,
    rust_source_corrections: Map<String, Value>,
    cases: Vec<Case>,
}

fn reference_requirements_view(
    requirements: &DesignRequirements,
    corrections: &Map<String, Value>,
) -> Value {
    let mut serialized = serde_json::to_value(requirements).unwrap();
    let object = serialized.as_object_mut().unwrap();
    object.remove("optimize_passenger_capacity");
    // The cargo capacity objective (clarified ledger App Features 2, decision
    // D10) is a native product addition the frozen W6.3 fixture has no field
    // for. It is a ranking target that enters no mass correlation, so it is
    // taken out of the compared view rather than recorded as a divergence -
    // after pinning that this replay carries no request at all, which is what
    // keeps the frozen mass point attributable to the translated model.
    assert_eq!(
        object.remove("cargo_objective_kg"),
        Some(Value::from(0.0)),
        "the frozen W6.3 replay must carry no cargo objective"
    );
    let optimized_capacity =
        serde_json::to_value(requirements.optimize_passenger_capacity).unwrap();
    assert_eq!(
        corrections.get("requirements.optimize_passenger_capacity"),
        Some(&optimized_capacity),
        "the Rust-only source correction must be pinned by the evidence fixture"
    );
    serialized
}

fn requirements_for(overrides: &Map<String, Value>) -> DesignRequirements {
    let mut value = DesignRequirements::default();
    for (key, item) in overrides {
        match key.as_str() {
            "aircraft_type" => value.aircraft_type = item.as_str().unwrap().to_owned(),
            "cabin_preset" => value.cabin_preset = item.as_str().unwrap().to_owned(),
            "cargo_payload_kg" => value.cargo_payload_kg = item.as_f64().unwrap(),
            "passenger_mass_kg" => value.passenger_mass_kg = item.as_f64().unwrap(),
            "dive_speed_m_s" => value.dive_speed_m_s = item.as_f64().unwrap(),
            other => panic!("fixture set an unhandled requirement: {other}"),
        }
    }
    value
}

fn mass_model_for(overrides: &Map<String, Value>) -> MassModelConfig {
    // W6.3 is a frozen Torenbeek correlation fixture.  The production
    // default is pure FLOPS now, so select the legacy method explicitly and
    // keep its FLOPS-only node empty before comparing the historical wire
    // representation below.
    let mut value = MassModelConfig {
        mass_architecture: MassArchitecture::LegacyReferenceCompatibleComparison,
        flops_transport: FlopsTransportConfig::default(),
        ..MassModelConfig::default()
    };
    value.apply_architecture();
    for (key, item) in overrides {
        match key.as_str() {
            "systems_mass_fraction" => value.systems_mass_fraction = item.as_f64().unwrap(),
            "furnishings_mass_fraction" => value.furnishings_mass_fraction = item.as_f64().unwrap(),
            "cabin_payload_density_kg_m" => {
                value.cabin_payload_density_kg_m = item.as_f64().unwrap()
            }
            other => panic!("fixture set an unhandled mass-model field: {other}"),
        }
    }
    value
}

fn scalar(case: &str, name: &str, actual: f64, expected: f64) {
    assert!(
        agrees(actual, expected, Tier::Closed),
        "first W6.3 divergence in {case} at {name}: got {actual:.17e}, reference {expected:.17e}"
    );
}

fn vector(case: &str, name: &str, actual: &[f64], expected: &[f64]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "first W6.3 divergence in {case} at {name} length"
    );
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        scalar(case, &format!("{name}[{index}]"), actual, expected);
    }
}

fn compare_wing(
    case: &str,
    name: &str,
    actual: &alas_geom::asb::wing::Wing,
    expected: &WingGeometry,
) {
    vector(
        case,
        &format!("{name}.aerodynamic_center_m"),
        &actual.aerodynamic_center(0.25),
        &expected.aerodynamic_center_m,
    );
    assert_eq!(
        actual.xsecs.len(),
        expected.xsecs.len(),
        "first W6.3 divergence in {case} at {name}.xsecs length"
    );
    for (index, (actual, expected)) in actual.xsecs.iter().zip(&expected.xsecs).enumerate() {
        let label = format!("{name}.xsecs[{index}]");
        vector(
            case,
            &format!("{label}.xyz_le_m"),
            &actual.xyz_le,
            &expected.xyz_le_m,
        );
        scalar(
            case,
            &format!("{label}.chord_m"),
            actual.chord,
            expected.chord_m,
        );
        scalar(
            case,
            &format!("{label}.twist_deg"),
            actual.twist,
            expected.twist_deg,
        );
    }
}

fn compare_geometry(case: &Case, plane: &alas_geom::asb::airplane::Airplane) {
    compare_wing(
        case.name.as_str(),
        "main_wing",
        &plane.wings[0],
        &case.aircraft_geometry.main_wing,
    );
    compare_wing(
        case.name.as_str(),
        "horizontal_stabilizer",
        &plane.wings[1],
        &case.aircraft_geometry.horizontal_stabilizer,
    );
    let actual = &plane.fuselages[0];
    let expected = &case.aircraft_geometry.fuselage;
    scalar(
        case.name.as_str(),
        "fuselage.area_wetted_m2",
        actual.area_wetted(),
        expected.area_wetted_m2,
    );
    assert_eq!(
        actual.xsecs.len(),
        expected.xsecs.len(),
        "first W6.3 divergence in {} at fuselage.xsecs length",
        case.name
    );
    for (index, (actual, expected)) in actual.xsecs.iter().zip(&expected.xsecs).enumerate() {
        let label = format!("fuselage.xsecs[{index}]");
        vector(
            case.name.as_str(),
            &format!("{label}.xyz_c_m"),
            &actual.xyz_c,
            &expected.xyz_c_m,
        );
        scalar(
            case.name.as_str(),
            &format!("{label}.width_m"),
            actual.width,
            expected.width_m,
        );
        scalar(
            case.name.as_str(),
            &format!("{label}.height_m"),
            actual.height,
            expected.height_m,
        );
        scalar(
            case.name.as_str(),
            &format!("{label}.shape"),
            actual.shape,
            expected.shape,
        );
    }
}

fn compare_softmax(case: &str, name: &str, actual: &SoftmaxTrace, expected: &SoftmaxTrace) {
    vector(
        case,
        &format!("{name}.values"),
        &actual.values,
        &expected.values,
    );
    scalar(
        case,
        &format!("{name}.softness"),
        actual.softness,
        expected.softness,
    );
    vector(
        case,
        &format!("{name}.scaled_values"),
        &actual.scaled_values,
        &expected.scaled_values,
    );
    scalar(
        case,
        &format!("{name}.scaled_max"),
        actual.scaled_max,
        expected.scaled_max,
    );
    scalar(
        case,
        &format!("{name}.exponential_sum"),
        actual.exponential_sum,
        expected.exponential_sum,
    );
    scalar(
        case,
        &format!("{name}.logsumexp"),
        actual.logsumexp,
        expected.logsumexp,
    );
    scalar(
        case,
        &format!("{name}.softmax"),
        actual.softmax,
        expected.softmax,
    );
}

#[test]
fn fuselage_mass_evidence_matches_in_input_and_correlation_order() {
    let fixture: Fixture = load("mass", "fuselage_evidence");
    assert_eq!(fixture.model_choice, fixture.cases[0].model_choice);

    for case in &fixture.cases {
        let requirements = requirements_for(&case.requirements_overrides);
        let mass_model = mass_model_for(&case.mass_model_overrides);
        // This is a frozen W6.3 translation fixture.  Keep its historical
        // geometry explicit; product defaults use the newer transport
        // planform and must be evidenced separately.
        let builder = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
        let design = case.design_vector.build();
        let plane = builder
            .build(Some(&design), true)
            .expect("fixture aircraft builds");

        assert_eq!(
            reference_requirements_view(&requirements, &fixture.rust_source_corrections),
            case.requirements,
            "first W6.3 divergence in {} at requirements",
            case.name
        );
        let mut mass_model_view = serde_json::to_value(&mass_model).unwrap();
        mass_model_view
            .as_object_mut()
            .expect("mass model serializes as an object")
            .remove("mass_architecture");
        mass_model_view
            .as_object_mut()
            .expect("mass model serializes as an object")
            .remove("schema_version");
        assert_eq!(
            mass_model_view, case.mass_model,
            "first W6.3 divergence in {} at mass_model",
            case.name
        );
        let mut geometry_view = serde_json::to_value(&builder.geometry).unwrap();
        // Propulsion evidence is independent of this frozen fuselage-mass
        // correlation; retain every historical geometry field in the oracle.
        for key in [
            "turbofan",
            "turboprop",
            "propulsion_technology",
            "part_power_fuel_flow_ratios",
            "part_power_source",
        ] {
            assert!(case.geometry_config["engine"].get(key).is_none());
            geometry_view["engine"].as_object_mut().unwrap().remove(key);
        }
        assert_eq!(
            geometry_view, case.geometry_config,
            "first W6.3 divergence in {} at geometry_config",
            case.name
        );
        assert_eq!(
            case.model_choice, fixture.model_choice,
            "first W6.3 divergence in {} at model_choice",
            case.name
        );
        compare_geometry(case, &plane);

        let wing = &plane.wings[0];
        let hstab = &plane.wings[1];
        let fuselage = &plane.fuselages[0];
        let wing_ac = wing.aerodynamic_center(0.25);
        let hstab_ac = hstab.aerodynamic_center(0.25);
        let tail_distance = (hstab_ac[0] - wing_ac[0]).max(1.0);
        let widths: Vec<f64> = fuselage.xsecs.iter().map(|xsec| xsec.width).collect();
        let heights: Vec<f64> = fuselage.xsecs.iter().map(|xsec| xsec.height).collect();
        let mean_width = widths.iter().sum::<f64>() / widths.len() as f64;
        let mean_height = heights.iter().sum::<f64>() / heights.len() as f64;
        let width_softness = mean_width * 0.01;
        let height_softness = mean_height * 0.01;
        let width_scaled: Vec<f64> = widths.iter().map(|value| value / width_softness).collect();
        let height_scaled: Vec<f64> = heights
            .iter()
            .map(|value| value / height_softness)
            .collect();
        let width_max = width_scaled
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let height_max = height_scaled
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let width_exponential_sum: f64 = width_scaled
            .iter()
            .map(|value| (value - width_max).max(-500.0).exp())
            .sum();
        let height_exponential_sum: f64 = height_scaled
            .iter()
            .map(|value| (value - height_max).max(-500.0).exp())
            .sum();
        let width_logsumexp = width_max + width_exponential_sum.ln();
        let height_logsumexp = height_max + height_exponential_sum.ln();
        let width_softmax = width_logsumexp * width_softness;
        let height_softmax = height_logsumexp * height_softness;
        let width_height_sum = width_softmax + height_softmax;
        let speed_distance_over_sum =
            requirements.dive_speed_m_s * tail_distance / width_height_sum;
        let square_root_term = speed_distance_over_sum.sqrt();
        let area_wetted = fuselage.area_wetted();
        let area_wetted_power = area_wetted.powf(1.2);
        let coefficient = 0.23;
        let expected_mass = coefficient * square_root_term * area_wetted_power;

        compare_softmax(
            case.name.as_str(),
            "correlation.width",
            &SoftmaxTrace {
                values: widths.clone(),
                softness: width_softness,
                scaled_values: width_scaled,
                scaled_max: width_max,
                exponential_sum: width_exponential_sum,
                logsumexp: width_logsumexp,
                softmax: width_softmax,
            },
            &case.correlation.width,
        );
        compare_softmax(
            case.name.as_str(),
            "correlation.height",
            &SoftmaxTrace {
                values: heights.clone(),
                softness: height_softness,
                scaled_values: height_scaled,
                scaled_max: height_max,
                exponential_sum: height_exponential_sum,
                logsumexp: height_logsumexp,
                softmax: height_softmax,
            },
            &case.correlation.height,
        );
        scalar(
            &case.name,
            "correlation.mean_width_m",
            mean_width,
            case.correlation.mean_width_m,
        );
        scalar(
            &case.name,
            "correlation.mean_height_m",
            mean_height,
            case.correlation.mean_height_m,
        );
        scalar(
            &case.name,
            "correlation.max_width_m",
            width_softmax,
            case.correlation.max_width_m,
        );
        scalar(
            &case.name,
            "correlation.max_height_m",
            height_softmax,
            case.correlation.max_height_m,
        );
        scalar(
            &case.name,
            "correlation.width_height_sum_m",
            width_height_sum,
            case.correlation.width_height_sum_m,
        );
        scalar(
            &case.name,
            "correlation.never_exceed_airspeed_m_s",
            requirements.dive_speed_m_s,
            case.correlation.never_exceed_airspeed_m_s,
        );
        scalar(
            &case.name,
            "correlation.wing_to_tail_distance_m",
            tail_distance,
            case.correlation.wing_to_tail_distance_m,
        );
        scalar(
            &case.name,
            "correlation.speed_distance_over_sum_m_s",
            speed_distance_over_sum,
            case.correlation.speed_distance_over_sum_m_s,
        );
        scalar(
            &case.name,
            "correlation.square_root_term",
            square_root_term,
            case.correlation.square_root_term,
        );
        scalar(
            &case.name,
            "correlation.area_wetted_m2",
            area_wetted,
            case.correlation.area_wetted_m2,
        );
        scalar(
            &case.name,
            "correlation.area_wetted_power_1_2",
            area_wetted_power,
            case.correlation.area_wetted_power_1_2,
        );
        scalar(
            &case.name,
            "correlation.coefficient",
            coefficient,
            case.correlation.coefficient,
        );
        scalar(
            &case.name,
            "correlation.fuselage_mass_kg",
            expected_mass,
            case.correlation.fuselage_mass_kg,
        );
        let direct_mass =
            mass_fuselage_simple(fuselage, requirements.dive_speed_m_s, tail_distance);
        scalar(
            &case.name,
            "direct_mass_fuselage_kg",
            direct_mass,
            case.direct_mass_fuselage_kg,
        );
        let masses =
            calculate_component_masses(&plane, &requirements, &builder.geometry, Some(&mass_model));
        scalar(
            &case.name,
            "component_mass_fuselage_kg",
            masses.fuselage,
            case.component_mass_fuselage_kg,
        );
    }
}
