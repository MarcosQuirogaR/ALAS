// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Contract tests for native CPACS 3.5 interchange artifacts.

// Fixture construction failures are test failures with the attached contract message.
#![allow(clippy::expect_used)]

use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::{render_cpacs_v35, CpacsExportError};

#[test]
fn cpacs_document_contains_computed_geometry_and_cpacs35_metadata() {
    let config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the default configured airframe builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-08-22T00:00:00Z")
        .expect("the default configured airframe is CPACS representable");

    assert!(xml.contains("xsi:noNamespaceSchemaLocation"));
    assert!(xml.contains("<wing uID=\"alas-wing-0\" symmetry=\"x-z-plane\""));
    assert!(xml.contains("<fuselage uID=\"alas-fuselage-0\""));
    assert!(xml.contains("<versionInfos>"));
    assert!(xml.contains("<cpacsVersion>3.5</cpacsVersion>"));
    assert!(!xml.contains("<toolspecific>"));
    assert!(xml.contains("<wingAirfoil uID=\"alas-wing-0-airfoil-0\""));
    assert!(xml.contains("<thrust00>467000</thrust00>"));
    assert!(xml.contains("<fpr00>1.45</fpr00>"));
    assert!(xml.contains("<bpr00>10</bpr00>"));
    assert!(xml.contains("<opr00>60</opr00>"));
    assert!(xml.contains("<length>7.8</length>"));
    assert!(xml.contains("<diameter>4.2</diameter>"));
    assert!(!xml.contains("<turbineInletTemperature>"));
}

#[test]
fn cpacs_document_escapes_source_names_without_creating_markup() {
    let config = AlasConfig::default();
    let mut airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the default configured airframe builds");
    airplane.name = "A&B < C > D \"E\" 'F'".to_owned();

    let xml = render_cpacs_v35(&airplane, &config, "2026-08-22T00:00:00Z")
        .expect("a changed display name does not change the geometry contract");

    assert!(xml.contains("A&amp;B &lt; C &gt; D &quot;E&quot; &apos;F&apos;"));
}

#[test]
fn cpacs_export_refuses_a_nonfinite_engine_input() {
    let mut config = AlasConfig::default();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the default configured airframe builds");
    config
        .geometry
        .engine
        .turbofan
        .as_mut()
        .expect("the default engine binds a turbofan payload")
        .rated_thrust_kn = f64::NAN;

    assert!(matches!(
        render_cpacs_v35(&airplane, &config, "2026-08-22T00:00:00Z"),
        Err(CpacsExportError::NonFinite { .. })
    ));
}
