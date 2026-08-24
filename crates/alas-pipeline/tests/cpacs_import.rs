// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused tests for the typed CPACS 3.5 import boundary.

#![allow(clippy::expect_used)]

use alas_pipeline::{read_cpacs, render_cpacs_v35, CpacsReadError, CPACS_35_VERSION};

const MINIMAL_CPACS_35: &str = r#"
<cpacs>
  <header>
    <versionInfos>
      <versionInfo version="fixture">
        <cpacsVersion>3.5</cpacsVersion>
      </versionInfo>
    </versionInfos>
  </header>
  <vehicles>
    <aircraft>
      <model uID="model-uid">
        <name>Fixture aircraft</name>
        <reference>
          <area>12.5</area>
          <length>3.4</length>
          <point>
            <x>1.0</x>
            <y>-2.0</y>
            <z>0.25</z>
          </point>
        </reference>
        <fuselages>
          <fuselage uID="fuselage-uid">
            <name>Fixture fuselage</name>
            <sections>
              <section uID="fuselage-section-uid">
                <name>Fixture fuselage section</name>
                <elements>
                  <element uID="fuselage-element-uid">
                    <name>Fixture fuselage element</name>
                    <profileUID>profile-uid</profileUID>
                  </element>
                </elements>
              </section>
            </sections>
            <segments>
              <segment uID="fuselage-segment-uid">
                <name>Fixture fuselage segment</name>
                <fromElementUID>fuselage-element-uid</fromElementUID>
                <toElementUID>fuselage-element-uid</toElementUID>
              </segment>
            </segments>
          </fuselage>
        </fuselages>
        <wings>
          <wing uID="wing-uid" symmetry="x-z-plane">
            <name>Fixture wing</name>
            <sections>
              <section uID="section-uid">
                <name>Fixture section</name>
                <elements>
                  <element uID="element-uid">
                    <name>Fixture element</name>
                    <airfoilUID>airfoil-uid</airfoilUID>
                    <transformation>
                      <scaling>
                        <x>2</x>
                        <y>1</y>
                        <z>2</z>
                      </scaling>
                      <rotation>
                        <x>0</x>
                        <y>4.5</y>
                        <z>0</z>
                      </rotation>
                      <translation refType="absLocal">
                        <x>10</x>
                        <y>2</y>
                        <z>-1</z>
                      </translation>
                    </transformation>
                  </element>
                </elements>
              </section>
            </sections>
            <segments>
              <segment uID="segment-uid">
                <name>Fixture segment</name>
                <fromElementUID>element-uid</fromElementUID>
                <toElementUID>element-uid</toElementUID>
              </segment>
            </segments>
          </wing>
        </wings>
        <engines>
          <engine uID="position-uid">
            <name>Fixture engine position</name>
            <engineUID>engine-uid</engineUID>
            <parentUID>wing-uid</parentUID>
            <transformation>
              <translation refType="absLocal">
                <x>3</x>
                <y>4</y>
                <z>5</z>
              </translation>
            </transformation>
          </engine>
        </engines>
      </model>
    </aircraft>
    <engines>
      <engine uID="engine-uid">
        <name>Fixture engine</name>
      </engine>
    </engines>
    <profiles>
      <fuselageProfiles>
        <fuselageProfile uID="profile-uid">
          <name>Fixture profile</name>
          <pointList mapType="vector">
            <x mapType="vector">0;0;0</x>
            <y mapType="vector">1;0;-1</y>
            <z mapType="vector">0;1;0</z>
          </pointList>
        </fuselageProfile>
      </fuselageProfiles>
      <wingAirfoils>
        <wingAirfoil uID="airfoil-uid">
          <name>Fixture airfoil</name>
          <pointList mapType="vector">
            <x mapType="vector">1;0;1</x>
            <y mapType="vector">0;0;0</y>
            <z mapType="vector">0;0.1;0</z>
          </pointList>
        </wingAirfoil>
      </wingAirfoils>
    </profiles>
  </vehicles>
</cpacs>
"#;

#[test]
fn current_cpacs35_output_preserves_aircraft_geometry_and_references() {
    let config = alas_config::AlasConfig::default();
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the configured aircraft builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-08-23T00:00:00Z")
        .expect("the current renderer produces CPACS 3.5");
    let document = read_cpacs(&xml).expect("the current CPACS output imports");

    assert_eq!(document.cpacs_version, CPACS_35_VERSION);
    assert_eq!(document.aircraft.uid, "alas-aircraft-model");
    assert_eq!(
        document
            .aircraft
            .reference
            .as_ref()
            .and_then(|value| value.area),
        Some(airplane.s_ref)
    );
    assert_eq!(
        document
            .aircraft
            .reference
            .as_ref()
            .and_then(|value| value.length),
        Some(airplane.c_ref)
    );
    assert_eq!(
        document
            .aircraft
            .reference
            .as_ref()
            .and_then(|value| value.point),
        Some(airplane.xyz_ref)
    );
    assert!(document.aircraft.wings.len() >= 3);
    assert_eq!(
        document.aircraft.wings[0].symmetry.as_deref(),
        Some("x-z-plane")
    );
    assert_eq!(document.aircraft.wings[0].uid, "alas-wing-0");
    assert!(!document.aircraft.wings[0].sections.is_empty());
    assert!(!document.wing_airfoils.is_empty());
    assert!(!document.wing_airfoils[0].points.is_empty());
    assert!(document.aircraft.engine_positions.len() >= 2);
    assert_eq!(
        document.aircraft.engine_positions[0].engine_uid,
        "alas-engine"
    );
    assert_eq!(document.engines[0].uid, "alas-engine");
    assert_eq!(document.engines[0].geometry_length_m, Some(7.8));
    assert_eq!(document.engines[0].geometry_diameter_m, Some(4.2));
    assert_eq!(document.engines[0].thrust00_n, Some(467_000.0));
    assert_eq!(document.engines[0].fpr00, Some(1.45));
    assert_eq!(document.engines[0].bpr00, Some(10.0));
    assert_eq!(document.engines[0].opr00, Some(60.0));
}

#[test]
fn current_cpacs35_output_round_trips_into_native_aircraft_geometry() {
    let config = alas_config::AlasConfig::default();
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the configured aircraft builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-08-23T00:00:00Z")
        .expect("the current renderer produces CPACS 3.5");
    let document = read_cpacs(&xml).expect("the current CPACS output imports");
    let imported = document
        .to_airplane()
        .expect("the current CPACS geometry is native-physics representable");
    let mut imported_config = config.clone();
    document
        .apply_engine_data_to_config(&mut imported_config, &imported)
        .expect("the current CPACS engine is physics representable");

    assert_eq!(imported.name, airplane.name);
    assert_eq!(imported.xyz_ref, airplane.xyz_ref);
    assert_eq!(imported.s_ref, airplane.s_ref);
    assert_eq!(imported.c_ref, airplane.c_ref);
    assert_eq!(imported.b_ref, airplane.b_ref);
    assert_eq!(imported_config.geometry.engine.thrust_kn, 467.0);
    assert_eq!(imported_config.geometry.engine.bypass_ratio, 10.0);
    assert_eq!(imported_config.geometry.engine.overall_pressure_ratio, 60.0);
    assert_eq!(imported_config.geometry.engine.fan_pressure_ratio, 1.45);
    assert_eq!(imported.wings.len(), airplane.wings.len());
    for (imported_wing, source_wing) in imported.wings.iter().zip(&airplane.wings) {
        assert_eq!(imported_wing.name, source_wing.name);
        assert_eq!(imported_wing.symmetric, source_wing.symmetric);
        assert_eq!(imported_wing.xsecs.len(), source_wing.xsecs.len());
        for (imported_section, source_section) in imported_wing.xsecs.iter().zip(&source_wing.xsecs)
        {
            assert_close(&imported_section.xyz_le, &source_section.xyz_le);
            assert_close(&[imported_section.chord], &[source_section.chord]);
            assert_close(&[imported_section.twist], &[source_section.twist]);
            assert_eq!(imported_section.airfoil.name, source_section.airfoil.name);
            assert_eq!(
                imported_section.airfoil.coordinates.len(),
                source_section.airfoil.coordinates.len()
            );
            for (imported_point, source_point) in imported_section
                .airfoil
                .coordinates
                .iter()
                .zip(&source_section.airfoil.coordinates)
            {
                assert_close(
                    &[imported_point.0, imported_point.1],
                    &[source_point.0, source_point.1],
                );
            }
        }
    }
    assert_eq!(imported.fuselages.len(), airplane.fuselages.len());
    for (imported_fuselage, source_fuselage) in imported.fuselages.iter().zip(&airplane.fuselages) {
        assert_eq!(imported_fuselage.name, source_fuselage.name);
        assert_eq!(imported_fuselage.xsecs.len(), source_fuselage.xsecs.len());
        for (imported_section, source_section) in
            imported_fuselage.xsecs.iter().zip(&source_fuselage.xsecs)
        {
            assert_close(&imported_section.xyz_c, &source_section.xyz_c);
            assert_close(&[imported_section.width], &[source_section.width]);
            assert_close(&[imported_section.height], &[source_section.height]);
        }
    }
}

#[test]
fn cpacs_constructor_keeps_machine_readable_engine_inputs_in_the_pipeline_config() {
    let config = alas_config::AlasConfig::default();
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("the configured aircraft builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-08-23T00:00:00Z")
        .expect("the current renderer produces CPACS 3.5");
    let document = read_cpacs(&xml).expect("the current CPACS output imports");
    let pipeline = alas_pipeline::DesignPipeline::new_with_cpacs_document(config, document)
        .expect("the current CPACS engine converts to pipeline inputs");

    assert_eq!(pipeline.config.geometry.engine.thrust_kn, 467.0);
    assert_eq!(pipeline.config.geometry.engine.bypass_ratio, 10.0);
    assert_eq!(pipeline.config.geometry.engine.overall_pressure_ratio, 60.0);
    assert_eq!(pipeline.config.geometry.engine.fan_pressure_ratio, 1.45);
    assert_eq!(
        pipeline.config.geometry.engine.spanwise_positions_m.len(),
        2
    );
}

fn assert_close(imported: &[f64], source: &[f64]) {
    assert_eq!(imported.len(), source.len());
    for (&imported, &source) in imported.iter().zip(source) {
        assert!(
            (imported - source).abs() <= 1.0e-12,
            "{imported} != {source}"
        );
    }
}

#[test]
fn fixture_preserves_optional_values_transformations_and_point_order() {
    let document = read_cpacs(MINIMAL_CPACS_35).expect("the valid fixture imports");
    let reference = document
        .aircraft
        .reference
        .as_ref()
        .expect("the fixture has reference data");
    assert_eq!(reference.area, Some(12.5));
    assert_eq!(reference.length, Some(3.4));
    assert_eq!(reference.point, Some([1.0, -2.0, 0.25]));
    assert_eq!(document.aircraft.fuselages.len(), 1);
    assert_eq!(document.fuselage_profiles[0].uid, "profile-uid");
    assert_eq!(
        document.fuselage_profiles[0].map_type.as_deref(),
        Some("vector")
    );

    let wing = &document.aircraft.wings[0];
    assert_eq!(wing.symmetry.as_deref(), Some("x-z-plane"));
    let transformation = wing.sections[0].elements[0]
        .transformation
        .as_ref()
        .expect("the element has a transformation");
    assert_eq!(transformation.scaling, Some([2.0, 1.0, 2.0]));
    assert_eq!(transformation.rotation, Some([0.0, 4.5, 0.0]));
    assert_eq!(transformation.translation, Some([10.0, 2.0, -1.0]));
    assert_eq!(
        transformation.translation_reference.as_deref(),
        Some("absLocal")
    );
    assert_eq!(
        document.wing_airfoils[0].points,
        vec![[1.0, 0.0, 0.0], [0.0, 0.0, 0.1], [1.0, 0.0, 0.0]]
    );
    assert_eq!(
        document.wing_airfoils[0].map_type.as_deref(),
        Some("vector")
    );
}

#[test]
fn unsupported_cpacs_version_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace("<cpacsVersion>3.5", "<cpacsVersion>4.0");
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::UnsupportedVersion { found }) if found == "4.0"
    ));
}

#[test]
fn malformed_airfoil_reference_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace(
        "<airfoilUID>airfoil-uid</airfoilUID>",
        "<airfoilUID>missing-airfoil</airfoilUID>",
    );
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::MalformedReference { target_uid, .. })
            if target_uid == "missing-airfoil"
    ));
}

#[test]
fn malformed_engine_reference_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace(
        "<engineUID>engine-uid</engineUID>",
        "<engineUID>missing-engine</engineUID>",
    );
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::MalformedReference { target_uid, .. })
            if target_uid == "missing-engine"
    ));
}

#[test]
fn malformed_parent_reference_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace(
        "<parentUID>wing-uid</parentUID>",
        "<parentUID>missing-parent</parentUID>",
    );
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::MalformedReference { target_uid, .. })
            if target_uid == "missing-parent"
    ));
}

#[test]
fn malformed_segment_endpoint_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace(
        "<toElementUID>element-uid</toElementUID>",
        "<toElementUID>missing-element</toElementUID>",
    );
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::MalformedReference { target_uid, .. })
            if target_uid == "missing-element"
    ));
}

#[test]
fn malformed_fuselage_profile_reference_is_rejected() {
    let xml = MINIMAL_CPACS_35.replace(
        "<profileUID>profile-uid</profileUID>",
        "<profileUID>missing-profile</profileUID>",
    );
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::MalformedReference { target_uid, .. })
            if target_uid == "missing-profile"
    ));
}

#[test]
fn duplicate_uid_is_rejected_across_cpacs_object_kinds() {
    let xml =
        MINIMAL_CPACS_35.replace("<engine uID=\"position-uid\">", "<engine uID=\"wing-uid\">");
    assert!(matches!(
        read_cpacs(&xml),
        Err(CpacsReadError::DuplicateUid { uid, .. }) if uid == "wing-uid"
    ));
}
