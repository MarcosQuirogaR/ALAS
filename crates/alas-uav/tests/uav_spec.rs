// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Frozen `UavSpec` schema: round trip, validation and the competition preset
//! interface expressing all four supported families.

use std::collections::BTreeMap;

use alas_uav::spec::inventory::render_markdown;
use alas_uav::spec::master::*;
use alas_uav::spec::mission::*;
use alas_uav::spec::score::*;
use alas_uav::spec::skeleton::*;
use alas_uav::spec::topology::*;
use alas_uav::spec::{UavSpec, SPEC_SCHEMA_VERSION};
use alas_uav_db::ComponentId;

fn panel(span: f64, taper: f64) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: 3.0,
        dihedral_deg: 2.0,
        twist_tip_deg: -1.5,
        airfoil_root: "s1223".into(),
        airfoil_tip: "sd7037".into(),
    }
}

fn node(name: &str, parent: Option<&str>, frame: Option<&str>, part: Part) -> Node {
    Node {
        name: name.into(),
        parent: parent.map(str::to_owned),
        parent_frame: frame.map(str::to_owned),
        offset_m: [0.0; 3],
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: None,
        part,
    }
}

/// A V-tail, three-panel, twin-servo wing, boom aircraft in the ACC 2026 class.
fn reference() -> UavSpec {
    let mut subskeletons = BTreeMap::new();
    subskeletons.insert(
        Skeleton::Fuselage,
        vec![Publication {
            name: "FS_Battery_Bay".into(),
            owner: Skeleton::Fuselage,
            datum: Datum::Box {
                min_m: [0.15, -0.04, -0.03],
                max_m: [0.30, 0.04, 0.03],
            },
        }],
    );
    subskeletons.insert(
        Skeleton::Propulsion,
        vec![Publication {
            name: "MLS_Motor_Mount".into(),
            owner: Skeleton::Propulsion,
            datum: Datum::Point {
                xyz_m: [0.0, 0.0, 0.0],
            },
        }],
    );
    UavSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        name: "reference-vtail".into(),
        master: MasterParams {
            root_chord_m: 0.28,
            panels: vec![panel(0.4, 1.0), panel(0.5, 0.8), panel(0.3, 0.6)],
            winglet: None,
            wing_xyz_m: [0.25, 0.0, 0.03],
            fuselage_xyz_m: [0.0, 0.0, 0.0],
            tail_xyz_m: [1.15, 0.0, 0.06],
            tail_boom: Some(TailBoom {
                length_m: 0.9,
                outer_diameter_m: 0.02,
                wall_m: 0.0015,
            }),
            controls: vec![ControlSurface {
                kind: ControlKind::Aileron,
                host: HostSurface::Wing,
                span_start: 0.55,
                span_end: 0.95,
                chord_fraction: 0.25,
                max_down_deg: 25.0,
                max_up_deg: 25.0,
            }],
            empennage: EmpennageDriving::Driven {
                horizontal_volume: 0.55,
                vertical_volume: 0.04,
                arm_m: 0.9,
            },
            layout: Default::default(),
        },
        topology: Topology {
            wing: WingKind::Straight,
            tail: TailKind::V { dihedral_deg: 35.0 },
            motors: MotorLayout::SingleTractor,
            gear: GearKind::Taildragger,
            fuselage: FuselageKind::Boom,
        },
        subskeletons,
        tolerances_m: BTreeMap::from([("printing_tolerance_m".to_owned(), 0.0003)]),
        tree: vec![
            node("airframe", None, None, Part::Assembly),
            node(
                "motor",
                Some("airframe"),
                Some("MLS_Motor_Mount"),
                Part::Catalog {
                    component: ComponentId::new("tmotor-at2826-kv900").unwrap(),
                },
            ),
            node(
                "battery",
                Some("airframe"),
                Some("FS_Battery_Bay"),
                Part::Catalog {
                    component: ComponentId::new("pack-3s-2200").unwrap(),
                },
            ),
        ],
        mission: MissionSpec {
            preset: PresetRef {
                id: "acc-2026".into(),
                version: 1,
                variant: None,
            },
            parameters: BTreeMap::new(),
            decisions: Decisions {
                announced_takeoff_m: Some(40.0),
                payload_count: Some(9),
                declared_payload_count: Some(8),
                takeoff_confidence: Some(0.9),
            },
            segments: vec![Segment {
                name: "launch".into(),
                kind: SegmentKind::Launch {
                    method: "none".into(),
                },
            }],
            rounds: 3,
        },
        origins: BTreeMap::from([(
            "root_chord_m".to_owned(),
            alas_uav::spec::ParamOrigin {
                kind: alas_uav::spec::OriginKind::CadImport,
                note: Some("GS_Root_Chord".into()),
            },
        )]),
    }
}

#[test]
fn the_reference_design_validates_and_round_trips() {
    let spec = reference();
    spec.validate().unwrap();
    assert!((spec.master.span_m() - 2.4).abs() < 1e-12);
    let json = serde_json::to_string_pretty(&spec).unwrap();
    let back: UavSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(back, spec);
}

#[test]
fn validation_reports_range_configuration_and_tree_errors() {
    let mut spec = reference();
    spec.master.root_chord_m = 28.0; // centimetres typed as metres
    spec.master.panels.push(panel(0.1, 1.0));
    spec.master.panels.push(panel(0.1, 1.0));
    spec.topology.fuselage = FuselageKind::Pod;
    spec.tree[1].parent_frame = Some("nowhere".into());
    spec.tree[2].parent = Some("battery".into());
    spec.origins.insert(
        "panels[9].span_m".into(),
        alas_uav::spec::ParamOrigin {
            kind: alas_uav::spec::OriginKind::User,
            note: None,
        },
    );
    let problems = spec.validate().unwrap_err().join("\n");
    for expected in [
        "root_chord_m = 28 is outside",
        "1 to 4 panels",
        "master.tail_boom is set",
        "unknown publication 'nowhere'",
        "unknown or self parent",
        "'panels[9].span_m' is not a parameter",
    ] {
        assert!(problems.contains(expected), "{expected}\n{problems}");
    }
}

#[test]
fn a_flying_wing_needs_no_tail() {
    let mut spec = reference();
    spec.topology.wing = WingKind::Flying {
        sweep_deg: 25.0,
        washout_deg: 2.0,
        reflex_airfoil: true,
    };
    assert!(spec.validate().is_err());
    spec.topology.tail = TailKind::None;
    spec.topology.fuselage = FuselageKind::None;
    spec.master.tail_boom = None;
    spec.master.empennage = EmpennageDriving::Explicit {
        horizontal: None,
        vertical: None,
    };
    spec.validate().unwrap();
}

fn segment(name: &str, kind: SegmentKind) -> Segment {
    Segment {
        name: name.into(),
        kind,
    }
}

fn term(name: &str, expr: &str, bonus: bool) -> ScoreTerm {
    ScoreTerm {
        name: name.into(),
        expr: expr.into(),
        additive_bonus: bonus,
    }
}

fn preset(id: &str, family: Family, score: ScoreDef, segments: Vec<Segment>) -> Preset {
    Preset {
        id: id.into(),
        version: 1,
        family,
        name: id.into(),
        status: PresetStatus::Official,
        citation: "rulebook".into(),
        gates: vec![RuleGate {
            id: "mtow".into(),
            description: "MTOW limit".into(),
            margin_expr: "(25 kg - mtow) / (25 kg)".into(),
            hard: true,
            citation: "rules p.10".into(),
        }],
        parameters: vec![ParamDef {
            name: "current_limit".into(),
            unit_si: "A".into(),
            default: 30.0,
            min: 1.0,
            max: 100.0,
            doc: "battery-side limit".into(),
        }],
        segments,
        score,
        variants: vec![],
    }
}

/// The interface must express the four families on day one.
#[test]
fn presets_of_all_four_families_fit_the_interface() {
    let acc = preset(
        "acc-2026",
        Family::Acc,
        ScoreDef {
            terms: vec![
                term(
                    "s_raw",
                    "b_to * payload_mass * distance_120s^2 - p_current",
                    false,
                ),
                term("b_load", "60 * (1 - loading_time / 120 s)", true),
            ],
            round_expr: "s_raw / s_raw_best * 1000 + b_load".into(),
            aggregation: Aggregation::MeanBestK {
                k: 2,
                min_rounds: 3,
            },
            reference_field: vec![ReferenceParam {
                name: "s_raw_best".into(),
                default: 1.0,
                unit_si: "kg*m2".into(),
                note: "assumption".into(),
            }],
        },
        vec![
            segment(
                "takeoff",
                SegmentKind::Takeoff {
                    runway_m: 40.0,
                    surface: Surface::Grass,
                },
            ),
            segment(
                "climb",
                SegmentKind::Climb {
                    end: Until::Time { seconds: 60.0 },
                },
            ),
            segment(
                "distance",
                SegmentKind::Cruise {
                    end: Until::Time { seconds: 120.0 },
                    altitude_window_m: [10.0, 200.0],
                },
            ),
            segment("landing", SegmentKind::Landing { zone_m: None }),
        ],
    );
    let xc = preset(
        "xc-2025-clubs",
        Family::XtraChallenge,
        ScoreDef {
            terms: vec![term("p_time", "200 * time_1km_min / time_1km", false)],
            round_expr: "((p_w + p_time + p_glide + p_alt) + b_load) * b_to * 0.75".into(),
            aggregation: Aggregation::MeanExceptWorst,
            reference_field: vec![],
        },
        vec![
            segment(
                "glide",
                SegmentKind::Glide {
                    end: Until::Altitude { meters: 0.0 },
                },
            ),
            segment(
                "legs",
                SegmentKind::Laps {
                    lap_length_m: 200.0,
                    turn_gate: true,
                    count: 5,
                },
            ),
        ],
    );
    let sae = preset(
        "sae-regular-2026",
        Family::SaeAeroDesign,
        ScoreDef {
            terms: vec![term("fs", "4 * empty_bottles + 15 * filled_bottles", false)],
            round_expr: "fs".into(),
            aggregation: Aggregation::MeanBestKPlusBonus {
                k: 3,
                bonus_expr: "max(10 - (fs - ps)^2, 0)".into(),
            },
            reference_field: vec![],
        },
        vec![segment("delivery", SegmentKind::Delivery)],
    );
    let dbf = preset(
        "dbf-2026",
        Family::AiaaDbf,
        ScoreDef {
            terms: vec![term("m2", "1 + net_income / net_income_max", false)],
            round_expr: "report_score * (m1 + m2 + m3)".into(),
            aggregation: Aggregation::Single,
            reference_field: vec![],
        },
        vec![segment(
            "laps",
            SegmentKind::Laps {
                lap_length_m: 300.0,
                turn_gate: true,
                count: 3,
            },
        )],
    );
    for preset in [acc, xc, sae, dbf] {
        let json = serde_json::to_string(&preset).unwrap();
        let back: Preset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, preset);
    }
}

#[test]
fn typed_outputs_have_unique_names_and_known_units() {
    let mut seen = std::collections::BTreeSet::new();
    for o in OUTPUTS {
        assert!(seen.insert(o.name), "{}", o.name);
        assert!(!o.unit_si.is_empty());
    }
    assert!(output("current_integral_over_limit").is_some());
}

#[test]
fn committed_spec_reference_matches_the_inventory() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/uav-spec-schema.md");
    let begin = "<!-- BEGIN GENERATED INVENTORY -->";
    let end = "<!-- END GENERATED INVENTORY -->";
    let generated = format!("{begin}\n{}{end}", render_markdown());
    if std::env::var_os("UPDATE_SCHEMA_DOC").is_some() {
        let text = std::fs::read_to_string(path).unwrap();
        let (head, rest) = text.split_once(begin).unwrap();
        let (_, tail) = rest.split_once(end).unwrap();
        std::fs::write(path, format!("{head}{generated}{tail}")).unwrap();
    }
    let text = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    assert!(
        text.contains(&generated),
        "stale; rerun with UPDATE_SCHEMA_DOC=1"
    );
}
