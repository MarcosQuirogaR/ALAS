// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Propeller source resolution: measured map, installed PER3 (fetch-on-demand
//! reader), computed map, BEMT geometry, generic BEMT.
//!
//! The reader keeps process-wide state (the data directory), so everything is
//! one test.  The PER3 tables are the SYNTHETIC fixtures (made-up numbers).
//! Nothing here touches the network.

use std::path::PathBuf;

use alas_uav::power::from_spec::resolve_propeller;
use alas_uav::power::{PropModel, PropSource};
use alas_uav::propulsion_electric::prop_data::{reload_prop_data, set_prop_data_dir};
use alas_uav_db::parts::PropellerSpec;
use alas_uav_db::{
    ComponentId, Field, IsoDate, Method, PerfKind, PropGeometry, PropPerfRow, PropPerfSet,
    Provenance, SourceId,
};

fn num(v: f64, unit: &str) -> Field<f64> {
    Field::new(v, unit, SourceId::new("test").unwrap(), None, Method::User)
}

fn spec() -> PropellerSpec {
    PropellerSpec {
        diameter_m: Some(num(12.0 * 0.0254, "m")),
        pitch_m: Some(num(6.0 * 0.0254, "m")),
        blades: Some(Field::new(
            2,
            "1",
            SourceId::new("test").unwrap(),
            None,
            Method::User,
        )),
        ..PropellerSpec::default()
    }
}

fn map(kind: PerfKind) -> PropPerfSet {
    let rows = [100.0, 300.0]
        .iter()
        .flat_map(|&omega| {
            [0.0, 0.4, 0.8].map(|j| PropPerfRow {
                omega_rad_s: omega,
                j,
                re: None,
                ct: 0.1 - 0.08 * j,
                cp: 0.05 - 0.03 * j,
                eta: None,
            })
        })
        .collect();
    PropPerfSet {
        prop_id: ComponentId::new("test-12x6").unwrap(),
        source_id: SourceId::new("test").unwrap(),
        kind,
        retrieved_at: Some(IsoDate::parse("2026-10-12").unwrap()),
        rows,
    }
}

#[test]
fn sources_are_resolved_in_the_documented_order() {
    let empty = std::env::temp_dir().join(format!("alas-power-resolve-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prop_data");

    // 1. No installed tables, no maps, no geometry: generic BEMT from diameter and pitch.
    set_prop_data_dir(Some(empty.clone()));
    reload_prop_data();
    let p = resolve_propeller("test", &spec(), &[], Some("apc-12x6e")).unwrap();
    assert_eq!(p.source(), PropSource::BemtGeneric);

    // 2. PER3 tables installed: they are read through the fetch-on-demand reader.
    set_prop_data_dir(Some(fixtures));
    reload_prop_data();
    let p = resolve_propeller("test", &spec(), &[], Some("apc-12x6e")).unwrap();
    assert_eq!(p.source(), PropSource::InstalledPer3);
    assert!((p.diameter_m() - 12.0 * 0.0254).abs() < 1.0e-12);
    let (lo, hi) = p.omega_range(0.0).unwrap();
    assert!(lo > 100.0 && hi > lo); // 1000..10000 rpm
    assert!(p.loads(500.0, 0.0, 1.225, 1.8e-5).unwrap().thrust_n > 0.0);
    // An id that is not installed falls through (here to BEMT).
    let p = resolve_propeller("test", &spec(), &[], Some("apc-99x99e")).unwrap();
    assert_eq!(p.source(), PropSource::BemtGeneric);

    // 3. A measured database map outranks the installed table; a computed one does not.
    let measured = map(PerfKind::Measured);
    let computed = map(PerfKind::Computed);
    let p = resolve_propeller("test", &spec(), &[&computed, &measured], Some("apc-12x6e")).unwrap();
    assert_eq!(p.source(), PropSource::MeasuredMap);
    let p = resolve_propeller("test", &spec(), &[&computed], Some("apc-12x6e")).unwrap();
    assert_eq!(p.source(), PropSource::InstalledPer3);

    // 4. Computed map when nothing else but BEMT is available.
    set_prop_data_dir(Some(empty.clone()));
    reload_prop_data();
    let p = resolve_propeller("test", &spec(), &[&computed], None).unwrap();
    assert_eq!(p.source(), PropSource::ComputedMap);

    // 5. Database blade geometry gives non-generic BEMT.
    let mut with_geometry = spec();
    let radius = 0.1524;
    let r: Vec<f64> = (1..=8)
        .map(|k| radius * (0.1 + 0.11 * f64::from(k)))
        .collect();
    with_geometry.geometry = Some(PropGeometry {
        chord_m: vec![0.015; r.len()],
        twist_rad: r
            .iter()
            .map(|&ri| (0.1524 / (std::f64::consts::TAU * ri)).atan())
            .collect(),
        airfoil: vec!["generic".to_owned(); r.len()],
        r_m: r,
        prov: Provenance::new(SourceId::new("test").unwrap(), None, Method::User),
    });
    let p: PropModel = resolve_propeller("test", &with_geometry, &[], None).unwrap();
    assert_eq!(p.source(), PropSource::Bemt);
    assert!(p.loads(500.0, 8.0, 1.225, 1.8e-5).unwrap().thrust_n > 0.0);

    // 6. Nothing usable: reported, never invented.
    let err = resolve_propeller("bare", &PropellerSpec::default(), &[], None).unwrap_err();
    assert!(err.to_string().contains("diameter_m"), "{err}");

    set_prop_data_dir(None);
    reload_prop_data();
    std::fs::remove_dir_all(empty).ok();
}
