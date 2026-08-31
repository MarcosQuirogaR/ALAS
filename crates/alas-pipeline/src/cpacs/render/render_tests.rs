// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused contracts for the CPACS renderer's optional analysis branches.

use std::collections::HashMap;

use alas_aero::analysis::PolarSweep;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_math::hybrd::Status;
use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};
use alas_mission::{MissionResult, SegmentSolution};

use crate::feasibility::{FeasibilityReport, FuelLoadingAssessment};
use crate::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};

use super::{render_cpacs_v35, render_cpacs_v35_with_analysis};
use crate::cpacs::read_cpacs;

const TEST_TIMESTAMP: &str = "2026-08-23T00:00:00Z";

#[test]
fn the_engine_cycle_is_written_to_machine_readable_cpacs_fields() {
    let config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the default configured airframe builds");

    let xml = render_cpacs_v35(&airplane, &config, TEST_TIMESTAMP)
        .expect("the default configured airframe is CPACS representable");

    assert!(xml.contains("<thrust00>467000</thrust00>"));
    assert!(xml.contains("<fpr00>1.45</fpr00>"));
    assert!(xml.contains("<bpr00>10</bpr00>"));
    assert!(xml.contains("<opr00>60</opr00>"));
    assert!(xml.contains("<length>7.8</length>"));
    assert!(xml.contains("<diameter>4.2</diameter>"));
    assert!(!xml.contains("<turbineInletTemperature>"));
}

#[test]
fn turboprop_export_uses_shaft_power_extension_not_zero_jet_fields() {
    let preset = alas_config::presets::get("ATR72-600").expect("ATR preset");
    let mut config = AlasConfig::default();
    config.geometry = preset.geometry.clone();
    config.geometry.engine.apply_engine_spec();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("ATR geometry builds");

    let xml = render_cpacs_v35(&airplane, &config, TEST_TIMESTAMP)
        .expect("typed turboprop is CPACS representable");

    assert!(!xml.contains("<thrust00>"));
    assert!(!xml.contains("<fpr00>"));
    assert!(!xml.contains("<bpr00>"));
    assert!(!xml.contains("<opr00>"));
    assert!(xml.contains("<technology>turboprop</technology>"));
    assert!(xml.contains("<engineModel>PW127M</engineModel>"));
    assert!(xml.contains("<propellerModel>Hamilton Sundstrand 568F-1</propellerModel>"));
    assert!(xml.contains("<takeoffShaftPowerKW>1845.6071832</takeoffShaftPowerKW>"));
    assert!(xml.contains("<maximumClimbShaftPowerKW>1634.574119424</maximumClimbShaftPowerKW>"));

    let imported = read_cpacs(&xml).expect("exported extension remains readable");
    assert_eq!(imported.engines[0].name, "PW127M");
    assert_eq!(imported.engines[0].thrust00_n, None);
    let turboprop = imported.engines[0]
        .turboprop
        .as_ref()
        .expect("toolspecific turboprop values round-trip");
    assert_eq!(turboprop.takeoff_shaft_power_kw, 1845.6071832);
    assert_eq!(turboprop.maximum_climb_shaft_power_kw, 1634.574119424);
    assert_eq!(turboprop.maximum_cruise_fuel_flow_kg_h, 762.0);
    assert_eq!(turboprop.propeller_diameter_m, 3.93);
}

#[test]
fn unavailable_engine_geometry_is_not_serialized_as_zero_values() {
    let mut config = AlasConfig::default();
    config.geometry.engine.nacelle_profile.clear();
    config.geometry.engine.spanwise_positions_m.clear();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the airframe without modeled nacelles still builds");

    let xml = render_cpacs_v35(&airplane, &config, TEST_TIMESTAMP)
        .expect("the airframe without modeled nacelles is CPACS representable");

    assert!(!xml.contains("<geometry>"));
    assert!(!xml.contains("<length>0</length>"));
    assert!(!xml.contains("<diameter>0</diameter>"));
}

#[test]
fn an_analysis_report_populates_aero_and_weight_balance_nodes() {
    let config = AlasConfig::default();
    let report = test_report(&config);

    let xml = render_cpacs_v35_with_analysis(&report, &config, None, None, TEST_TIMESTAMP)
        .expect("the synthetic completed report is CPACS representable");

    assert!(xml.contains("<analyses>"));
    assert!(xml.contains("<aeroPerformance>"));
    assert!(xml.contains("<aeroMap uID=\"alas-aero-polar\">"));
    assert!(xml.contains("<cms mapType=\"vector\">"));
    assert!(xml.contains("<angleOfAttack mapType=\"vector\">-2;0;2</angleOfAttack>"));
    assert!(xml.contains("<cl mapType=\"vector\">0.1;0.3;0.5</cl>"));
    assert!(xml.contains("<cd mapType=\"vector\">0.02;0.03;0.05</cd>"));
    assert!(xml.contains("<cms mapType=\"vector\">0.02;0.01;0</cms>"));
    assert!(xml.contains("<aeroMap uID=\"alas-aero-design-point\">"));
    assert!(xml.contains("<cl mapType=\"vector\">0.3</cl>"));
    assert!(xml.contains("<cd mapType=\"vector\">0.03</cd>"));
    assert!(xml.contains("<weightAndBalance>"));
    assert!(xml.contains("<operationalCase uID=\"alas-wb-analysis\">"));
    assert!(xml.contains("<mass mapType=\"vector\">10</mass>"));
    assert!(xml.contains("<x mapType=\"vector\">1</x>"));
    assert!(xml.contains("<coG>"));
    assert!(!xml.contains("<massBreakdown>"));
}

#[test]
fn an_available_feasibility_load_case_is_exported_as_weight_and_balance() {
    let config = AlasConfig::default();
    let report = test_report(&config);
    let feasibility = FeasibilityReport {
        fuel_loading: FuelLoadingAssessment {
            analyzed_carried_fuel_kg: 5.0,
            ..FuelLoadingAssessment::default()
        },
        ..FeasibilityReport::default()
    };

    let xml =
        render_cpacs_v35_with_analysis(&report, &config, Some(&feasibility), None, TEST_TIMESTAMP)
            .expect("the available feasibility load case is CPACS representable");

    assert!(xml.contains("<operationalCase uID=\"alas-wb-analyzed-takeoff\">"));
    assert!(xml.contains("<mass mapType=\"vector\">14</mass>"));
}

#[test]
fn a_complete_mission_is_exported_as_a_cpacs_trajectory_summary() {
    let config = AlasConfig::default();
    let report = test_report(&config);
    let spec = SegmentSpec {
        tag: "cpacs_test_cruise".to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: Some(10_000.0),
            distance_m: 1_000.0,
        },
        air_speed_m_s: 250.0,
        true_course_rad: 0.0,
        temperature_deviation_k: 0.0,
        number_control_points: 2,
    };
    let mut segment = Segment::new(spec, None).expect("the synthetic mission segment builds");
    segment.conditions.time_s = vec![0.0, 10.0];
    segment.conditions.total_mass_kg = vec![20.0, 18.0];
    segment.conditions.aircraft_range_m = vec![0.0, 1_000.0];
    let mission = MissionResult {
        segments: vec![segment],
        solutions: vec![SegmentSolution {
            converged: true,
            status: Status::Converged,
            evaluations: 0,
            throttle_limited: false,
        }],
        scheduled_segment_count: 1,
        fuel_exhaustion: None,
    };

    let xml =
        render_cpacs_v35_with_analysis(&report, &config, None, Some(&mission), TEST_TIMESTAMP)
            .expect("the complete mission summary is CPACS representable");

    assert!(xml.contains("<trajectories>"));
    assert!(xml.contains("<trajectory uID=\"alas-mission-trajectory\">"));
    assert!(xml.contains("<fuelMass>2</fuelMass>"));
    assert!(xml.contains("<time>10</time>"));
    assert!(xml.contains("<distance>1000</distance>"));
}

#[test]
fn unavailable_analysis_data_is_left_out_of_the_cpacs_document() {
    let config = AlasConfig::default();
    let mut report = test_report(&config);
    report.polar = PolarSweep {
        alpha_deg: Vec::new(),
        geometric_alpha_deg: Vec::new(),
        cl: Vec::new(),
        cd: Vec::new(),
        cd_induced: Vec::new(),
        cd_wave: Vec::new(),
        cd_parasite: Vec::new(),
        cm: Vec::new(),
        l_over_d: Vec::new(),
    };
    report.component_masses.clear();
    report.mass_coordinates.clear();

    let xml = render_cpacs_v35_with_analysis(&report, &config, None, None, TEST_TIMESTAMP)
        .expect("missing optional analysis values do not invalidate geometry");

    assert!(!xml.contains("<analyses>"));
    assert!(!xml.contains("NaN"));
    assert!(!xml.contains("inf"));
}

#[test]
fn the_reference_point_remains_a_global_cpacs_point() {
    let config = AlasConfig::default();
    let report = test_report(&config);
    let point = report.airplane.xyz_ref;

    let xml = render_cpacs_v35_with_analysis(&report, &config, None, None, TEST_TIMESTAMP)
        .expect("the report is CPACS representable");

    let expected = format!(
        "<point>\n            <x>{}</x>\n            <y>{}</y>\n            <z>{}</z>",
        point[0], point[1], point[2]
    );
    assert!(xml.contains(&expected));
    assert!(!xml.contains("<point refType=\"absLocal\">"));
}

fn test_report(config: &AlasConfig) -> AnalysisReport {
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the default configured airframe builds");
    let component_names = [
        WING,
        H_STAB,
        V_STAB,
        FUSELAGE,
        GEAR,
        PROPULSION,
        SYSTEMS,
        FURNISHINGS,
        PAYLOAD,
        FUEL,
    ];
    let component_masses = component_names
        .into_iter()
        .map(|name| (name.to_owned(), 1.0))
        .collect::<HashMap<_, _>>();
    let mass_coordinates = component_names
        .into_iter()
        .map(|name| (name.to_owned(), [1.0, 0.0, 0.0]))
        .collect::<HashMap<_, _>>();

    AnalysisReport {
        design: DesignVector::default(),
        airplane,
        polar: PolarSweep {
            alpha_deg: vec![-2.0, 0.0, 2.0],
            geometric_alpha_deg: vec![-2.0, 0.0, 2.0],
            cl: vec![0.1, 0.3, 0.5],
            cd: vec![0.02, 0.03, 0.05],
            cd_induced: vec![0.001, 0.002, 0.004],
            cd_wave: vec![0.0, 0.0, 0.0],
            cd_parasite: vec![0.019, 0.028, 0.046],
            cm: vec![0.02, 0.01, 0.0],
            l_over_d: vec![5.0, 10.0, 10.0],
        },
        design_point: DesignPoint {
            alpha_deg: 0.0,
            cl: 0.3,
            cd: 0.03,
            l_over_d: 10.0,
        },
        polar_fit: PolarFit {
            cd0: 0.02,
            k: 0.04,
            oswald_e: 0.8,
            aspect_ratio: 9.0,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.1,
        x_neutral_point: 30.0,
        geometry_summary: HashMap::new(),
        component_masses,
        mass_coordinates,
        physical_cg: [1.0, 0.0, 0.0],
        payload_layout: None,
        trimmed_design_point: None,
        cg_envelope_ok: Some(true),
    }
}
