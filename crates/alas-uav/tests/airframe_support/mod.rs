// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Shared fixtures of the airframe tests; a failed unwrap here is malformed
// test evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::collections::BTreeMap;

use alas_uav::spec::master::*;
use alas_uav::spec::mission::*;
use alas_uav::spec::topology::*;
use alas_uav::spec::{UavSpec, SPEC_SCHEMA_VERSION};
use alas_uav_db::{
    Body, Component, ComponentId, IsoDate, Method, Num, SourceId, Status, SCHEMA_VERSION,
};

/// A numeric field in a test row (value in SI, `unit` documents it).
pub fn num(value: f64, unit: &str) -> Num {
    Num::new(
        value,
        unit,
        SourceId::new("test-src").unwrap(),
        None,
        Method::User,
    )
}

/// A database row with the given id and body.
pub fn component(id: &str, body: Body) -> Component {
    let day = IsoDate::parse("2026-10-12").unwrap();
    Component {
        id: ComponentId::new(id).unwrap(),
        mpn: id.into(),
        manufacturer: "test".into(),
        status: Status::Active,
        schema_version: SCHEMA_VERSION,
        valid_from: day.clone(),
        valid_to: None,
        created_at: day.clone(),
        updated_at: day,
        body,
    }
}

fn panel(span: f64, taper: f64, sweep: f64, dihedral: f64) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: sweep,
        dihedral_deg: dihedral,
        twist_tip_deg: 0.0,
        airfoil_root: "s1223".into(),
        airfoil_tip: "sd7037".into(),
    }
}

/// A 2.4 m, three-panel, V-tail, boom aircraft in the ACC class; the wing
/// root leading edge at x = 0.25 m, the tail mount at x = 1.15 m.
pub fn reference_spec() -> UavSpec {
    spec_with(
        TailKind::V { dihedral_deg: 35.0 },
        WingKind::Straight,
        EmpennageDriving::Driven {
            horizontal_volume: 0.55,
            vertical_volume: 0.04,
            arm_m: 0.9,
        },
    )
}

/// The reference aircraft with another tail kind, wing kind and empennage mode.
pub fn spec_with(tail: TailKind, wing: WingKind, empennage: EmpennageDriving) -> UavSpec {
    UavSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        name: "airframe-test".into(),
        master: MasterParams {
            root_chord_m: 0.28,
            panels: vec![
                panel(0.4, 1.0, 3.0, 2.0),
                panel(0.5, 0.8, 3.0, 2.0),
                panel(0.3, 0.6, 3.0, 2.0),
            ],
            winglet: None,
            wing_xyz_m: [0.25, 0.0, 0.03],
            fuselage_xyz_m: [0.0, 0.0, 0.0],
            tail_xyz_m: [1.15, 0.0, 0.06],
            tail_boom: Some(TailBoom {
                length_m: 0.9,
                outer_diameter_m: 0.02,
                wall_m: 0.0015,
            }),
            controls: Vec::new(),
            empennage,
            layout: Default::default(),
        },
        topology: Topology {
            wing,
            tail,
            motors: MotorLayout::SingleTractor,
            gear: GearKind::Taildragger,
            fuselage: FuselageKind::Boom,
        },
        subskeletons: BTreeMap::new(),
        tolerances_m: BTreeMap::new(),
        tree: Vec::new(),
        mission: MissionSpec {
            preset: PresetRef {
                id: "acc-2026".into(),
                version: 1,
                variant: None,
            },
            parameters: BTreeMap::new(),
            decisions: Decisions {
                announced_takeoff_m: None,
                payload_count: None,
                declared_payload_count: None,
                takeoff_confidence: None,
            },
            segments: Vec::new(),
            rounds: 1,
        },
        origins: BTreeMap::new(),
    }
}
