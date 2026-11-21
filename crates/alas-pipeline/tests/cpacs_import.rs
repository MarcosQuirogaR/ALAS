// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused tests for the typed CPACS 3.5 import boundary.

#![allow(clippy::expect_used)]

use std::borrow::Cow;

use alas_geom::builder::AircraftBuilder;
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
fn an_own_export_keeps_the_dorsal_skin_and_the_ventral_fin_and_so_the_masses() {
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": "LJ60" }))
        .expect("the preset loads");
    let design = alas_config::presets::get("LJ60")
        .expect("preset")
        .design_vector;
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .expect("the configured aircraft builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-11-06T00:00:00Z").expect("renders");
    let imported = read_cpacs(&xml)
        .expect("reads")
        .to_airplane()
        .expect("converts");
    for name_is in [
        alas_geom::aircraft::wing::Wing::is_dorsal_fin,
        alas_geom::aircraft::wing::Wing::is_ventral_fin,
    ] {
        let area = |plane: &alas_geom::aircraft::airplane::Airplane| {
            let surface = plane
                .wings
                .iter()
                .find(|wing| name_is(wing))
                .expect("surface");
            assert!(surface.is_geometry_only());
            surface.skin_planform().unfolded_area()
        };
        assert!((area(&imported) - area(&airplane)).abs() < 1e-6);
    }
    let masses = |plane: &alas_geom::aircraft::airplane::Airplane| {
        alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
            plane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&config.mass_model),
            None,
            alas_mass::breakdown::MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .expect("masses")
        .0
        .as_pairs()
        .iter()
        .map(|pair| pair.1)
        .sum::<f64>()
    };
    assert!((masses(&imported) - masses(&airplane)).abs() < 1e-3);
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
    assert_eq!(imported_config.geometry.engine.thrust_kn(), 467.0);
    assert_eq!(imported_config.geometry.engine.bypass_ratio, 10.0);
    assert_eq!(imported_config.geometry.engine.overall_pressure_ratio, 60.0);
    assert_eq!(imported_config.geometry.engine.fan_pressure_ratio, 1.45);
    assert_eq!(
        imported_config.geometry.engine.part_power_fuel_flow_ratios,
        config.geometry.engine.part_power_fuel_flow_ratios
    );
    assert_eq!(
        imported_config.geometry.engine.part_power_source,
        config.geometry.engine.part_power_source
    );
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

    assert_eq!(pipeline.config.geometry.engine.thrust_kn(), 467.0);
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

/// A document with no ALAS record has no reference wing and no datum: the box is
/// the loft itself, as it is for any wing whose sections are the design planform.
#[test]
fn a_document_without_the_alas_record_keeps_its_sections_as_the_design_planform() {
    let document = read_cpacs(MINIMAL_CPACS_35).expect("valid fixture");
    assert_eq!(document.aircraft.main_wing_record, None);
    let airplane = document.to_airplane().expect("fixture geometry converts");
    let wing = &airplane.wings[0];
    assert_eq!(wing.reference, None);
    assert_eq!(wing.planform_datum, None);
    assert!(matches!(wing.carried_box(), Cow::Borrowed(_)));
    assert_eq!(
        wing.design_root(),
        (wing.xsecs[0].xyz_le, wing.xsecs[0].chord)
    );
}

/// An own export restores the reference wing and the datum on the import, so the
/// box readers see the same box; a record that no longer fits the wing is dropped.
#[test]
fn an_own_export_restores_the_reference_wing_and_the_datum() {
    let preset = alas_config::presets::registry()
        .iter()
        .find(|preset| preset.name == "B747-400")
        .expect("registered preset");
    let config = alas_config::AlasConfig {
        geometry: preset.geometry.clone(),
        ..alas_config::AlasConfig::default()
    };
    let airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), false)
        .expect("preset builds");
    let built = &airplane.wings[0];
    let datum = built.planform_datum.expect("the preset has a datum");

    let xml = render_cpacs_v35(&airplane, &config, "2026-11-05T00:00:00Z").expect("renders");
    let imported = read_cpacs(&xml)
        .expect("reads")
        .to_airplane()
        .expect("converts");
    let wing = &imported.wings[0];
    assert_eq!(wing.reference, built.reference);
    assert_eq!(wing.planform_datum, Some(datum));
    let (carried, expected) = (wing.carried_box(), built.carried_box());
    for (a, b) in carried.xsecs.iter().zip(&expected.xsecs) {
        assert!((a.xyz_le[0] - b.xyz_le[0]).abs() < 1e-9);
        assert!((a.chord - b.chord).abs() < 1e-9);
        assert!((a.twist - b.twist).abs() < 1e-9);
    }

    let edited = xml.replace(
        &format!(
            "<sideOfBodyChordM>{}</sideOfBodyChordM>",
            datum.side_of_body_chord_m
        ),
        &format!(
            "<sideOfBodyChordM>{}</sideOfBodyChordM>",
            datum.side_of_body_chord_m + 1.0
        ),
    );
    let stale = read_cpacs(&edited)
        .expect("reads")
        .to_airplane()
        .expect("converts");
    assert_eq!(stale.wings[0].planform_datum, None);
    assert_eq!(stale.wings[0].reference, None);
}

#[test]
fn an_own_export_gives_back_the_bodies_the_analyses_read() {
    for name in ["LJ60", "DC-10", "A320-200", "ATR72-600", "B747-400"] {
        let preset = alas_config::presets::registry()
            .iter()
            .find(|preset| preset.name == name)
            .expect("registered preset");
        let config = alas_config::AlasConfig {
            geometry: preset.geometry.clone(),
            ..alas_config::AlasConfig::default()
        };
        let airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset builds");
        let xml = render_cpacs_v35(&airplane, &config, "2026-11-05T00:00:00Z").expect("renders");
        let imported = read_cpacs(&xml)
            .expect("reads")
            .to_airplane()
            .expect("converts");
        assert_eq!(imported.fuselages.len(), airplane.fuselages.len());
        for (body, source) in imported.fuselages.iter().zip(&airplane.fuselages) {
            assert_eq!(body.xsecs, source.xsecs, "{name} {}", body.name);
            assert_eq!(body.lip_scarf, source.lip_scarf, "{name} {}", body.name);
            assert_eq!(
                body.area_wetted(),
                source.area_wetted(),
                "{name} {}",
                body.name
            );
        }
        // An edited record no longer describes the document, which keeps its closed body.
        let at = xml.find("<widthM>").expect("a recorded width") + "<widthM>".len();
        let edited = format!("{}9{}", &xml[..at], &xml[at..]);
        let stale = read_cpacs(&edited)
            .expect("reads")
            .to_airplane()
            .expect("converts");
        let source = &airplane.fuselages[0];
        let closed = source.closed_stations();
        assert_eq!(stale.fuselages[0].xsecs.len(), closed.len());
        for (read, expected) in stale.fuselages[0].xsecs.iter().zip(&closed) {
            assert_close(&read.xyz_c, &expected.xyz_c);
        }
    }
}

#[test]
fn an_own_export_gives_back_the_lofted_wetted_area_to_the_bit() {
    for preset in alas_config::presets::registry() {
        let mut config = alas_config::AlasConfig {
            geometry: preset.geometry.clone(),
            ..alas_config::AlasConfig::default()
        };
        let airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset builds");
        let name = preset.name;
        let xml = render_cpacs_v35(&airplane, &config, "2026-11-05T00:00:00Z").expect("renders");
        let imported = read_cpacs(&xml)
            .expect("reads")
            .to_airplane()
            .expect("converts");
        for (body, source) in imported.fuselages.iter().zip(&airplane.fuselages) {
            let label = format!("{name} {}", body.name);
            assert_eq!(
                body.area_wetted_lofted().to_bits(),
                source.area_wetted_lofted().to_bits(),
                "{label}"
            );
            assert_eq!(body.loft.is_some(), source.loft.is_some(), "{label}");
            if let (Some(read), Some(built)) = (&body.loft, &source.loft) {
                assert_eq!(read.volume().to_bits(), built.volume().to_bits(), "{label}");
                assert_eq!(read.params(), built.params(), "{label}");
            }
            assert_eq!(
                body.apu_cone.as_ref().map(|cone| &cone.xsecs),
                source.apu_cone.as_ref().map(|cone| &cone.xsecs),
                "{label}"
            );
        }
        let Some(lofted) = airplane.fuselages.iter().find(|body| body.loft.is_some()) else {
            continue;
        };
        let read_hull = |document: &str| {
            let read = read_cpacs(document)
                .expect("reads")
                .to_airplane()
                .expect("converts");
            read.fuselages
                .into_iter()
                .find(|body| body.name == lofted.name)
                .expect("hull")
        };
        // A hull edited after the export drops the record: the document's sections
        // are the body, on the area they give.
        let text = xml.find("<widthM>").expect("a recorded width") + "<widthM>".len();
        let hull = read_hull(&format!("{}9{}", &xml[..text], &xml[text..]));
        assert!(hull.loft.is_none() && hull.apu_cone.is_none(), "{name}");
        // Loft data that does not span the stations goes alone: the hull keeps them.
        let start = xml.find("<specJson>").expect("a recorded loft");
        let length = start + xml[start..].find("<lengthM>").expect("the loft length");
        let text = length + "<lengthM>".len();
        let hull = read_hull(&format!("{}1{}", &xml[..text], &xml[text..]));
        assert!(hull.loft.is_none(), "{name}");
        assert_eq!(hull.xsecs, lofted.xsecs, "{name}");
        // The flare of a wing with no datum comes back with its taper and sweep.
        let mut flared = preset.geometry.clone();
        flared.wing.side_of_body_span_fraction = None;
        flared.wing.side_of_body_chord_ratio = None;
        let sharklet = alas_config::presets::get("A320-200")
            .expect("preset")
            .geometry
            .wing
            .wingtip_device
            .clone();
        let device = flared
            .wing
            .wingtip_device
            .get_or_insert(sharklet.expect("device"));
        device.root_te_flare = Some(alas_config::WingtipTeFlare {
            offset_m: 0.2,
            fade_length_m: 0.4,
        });
        if let Ok(built) =
            AircraftBuilder::new(Some(flared.clone())).build(Some(&preset.design_vector), false)
        {
            config.geometry = flared;
            let document =
                render_cpacs_v35(&built, &config, "2026-11-05T00:00:00Z").expect("renders");
            let read = read_cpacs(&document)
                .expect("reads")
                .to_airplane()
                .expect("converts");
            let (wing, source) = (&read.wings[0], &built.wings[0]);
            assert_eq!(wing.tip_te_flare_m, source.tip_te_flare_m, "{name}");
            assert_eq!(wing.planform_datum, source.planform_datum, "{name}");
            assert!(
                (wing.taper_ratio() - source.taper_ratio()).abs() < 1e-12,
                "{name}"
            );
        }
        // Unreadable loft data is the same.
        let hull = read_hull(&xml.replacen("<specJson>{", "<specJson>[", 1));
        assert!(hull.loft.is_none(), "{name}");
        assert_eq!(hull.xsecs, lofted.xsecs, "{name}");
    }
}

#[test]
fn an_own_export_gives_back_an_apu_cone() {
    // No registered loft has a cone: put one behind the A320 hull as the builder does.
    let preset = alas_config::presets::get("A320-200").expect("registered preset");
    let config = alas_config::AlasConfig {
        geometry: preset.geometry.clone(),
        ..alas_config::AlasConfig::default()
    };
    let mut airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset builds");
    let hull = &mut airplane.fuselages[0];
    let x_end = hull.xsecs[hull.xsecs.len() - 1].xyz_c[0];
    let station = |x: f64, size: f64| alas_geom::aircraft::fuselage::FuselageXSec {
        xyz_c: [x, 0.0, 1.0],
        width: size,
        height: size,
        shape: 2.0,
    };
    let cone = vec![station(x_end + 1.0e-3, 0.6), station(x_end + 1.5, 0.2)];
    hull.xsecs.extend(cone.iter().cloned());
    hull.apu_cone = Some(std::sync::Arc::new(
        alas_geom::aircraft::fuselage::Fuselage::new("Fuselage APU cone", cone),
    ));

    let xml = render_cpacs_v35(&airplane, &config, "2026-11-05T00:00:00Z").expect("renders");
    let imported = read_cpacs(&xml)
        .expect("reads")
        .to_airplane()
        .expect("converts");
    let (read, source) = (&imported.fuselages[0], &airplane.fuselages[0]);
    assert!(read.loft.is_some());
    assert_eq!(read.apu_cone, source.apu_cone);
    assert_eq!(
        read.area_wetted_lofted().to_bits(),
        source.area_wetted_lofted().to_bits()
    );
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

#[test]
fn an_own_export_keeps_the_section_planes_of_a_wingtip_device() {
    // The B747-400 winglet root and tip-edge stations keep the wing-tip plane; the
    // document states it as their section rotation and the import gives it back,
    // so the device lofts with the same section frames on both sides.
    let preset = alas_config::presets::get("B747-400").expect("preset");
    let config = alas_config::AlasConfig {
        geometry: preset.geometry.clone(),
        ..alas_config::AlasConfig::default()
    };
    let airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset builds");
    let xml = render_cpacs_v35(&airplane, &config, "2026-11-05T00:00:00Z").expect("renders");
    let imported = read_cpacs(&xml)
        .expect("reads")
        .to_airplane()
        .expect("converts");
    let mut kept = 0;
    for (wing, source) in imported.wings.iter().zip(&airplane.wings) {
        for (read, built) in wing.xsecs.iter().zip(&source.xsecs) {
            match (read.span_axis, built.span_axis) {
                (Some(read), Some(built)) => {
                    kept += 1;
                    assert_close(&read, &built);
                }
                (None, None) => {}
                other => panic!("{} span axis {other:?}", wing.name),
            }
        }
        let (read, built) = (
            wing.section_outlines(Some(40)),
            source.section_outlines(Some(40)),
        );
        for (read, built) in read.iter().zip(&built) {
            for (p, q) in read.iter().zip(built) {
                assert!(
                    (0..3).all(|axis| (p[axis] - q[axis]).abs() < 1.0e-9),
                    "{} {p:?} {q:?}",
                    wing.name
                );
            }
        }
    }
    assert_eq!(kept, 4);
}
