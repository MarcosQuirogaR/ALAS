// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Three ACC 2026 team aircraft built from PUBLISHED inputs only (team
//! technical reports, competitions.md digests; page references in each doc
//! line).  Every number not published is an ASSUMPTION marked as such and
//! shared by the three builds where it is generic.  Nothing here was changed
//! after seeing a prediction.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use alas_uav::aero::drag_lowre::{Component, Junction, Shape};
use alas_uav::coupled::{
    wheel, AeroSettings, ElectricSettings, Fuselage, GroundSetup, Hull, MotorStation, Vehicle,
};
use alas_uav::power::{BatteryModel, BemtGeometry, EscModel, MotorModel, Powertrain, PropModel};
use alas_uav::score::presets;
use alas_uav::spec::master::*;
use alas_uav::spec::mission::*;
use alas_uav::spec::skeleton::{Node, Part};
use alas_uav::spec::topology::*;
use alas_uav::spec::{UavSpec, SPEC_SCHEMA_VERSION};

/// One team aircraft and what its report says about it.
pub struct Team {
    pub name: &'static str,
    pub spec: UavSpec,
    pub hull: Hull,
    pub chain: Powertrain,
    pub ground: GroundSetup,
    pub rho: f64,
    /// Reported (team-predicted unless marked measured) figures.
    pub reported: Vec<Reported>,
    /// Published empty mass, kg (comparison only).
    pub empty_kg: f64,
}

pub struct Reported {
    pub what: &'static str,
    pub value: f64,
    pub unit: &'static str,
    pub measured: bool,
    pub source: &'static str,
}

pub fn rep(
    what: &'static str,
    value: f64,
    unit: &'static str,
    measured: bool,
    source: &'static str,
) -> Reported {
    Reported {
        what,
        value,
        unit,
        measured,
        source,
    }
}

fn kv_si(rpm_per_v: f64) -> f64 {
    rpm_per_v * std::f64::consts::TAU / 60.0
}

/// T-Motor AT2814 Long Shaft 900 KV: Kv 900 rpm/V, Rm 82 mOhm, I0 1.2 A at
/// 10 V (T-Motor product page, via `multi_source_catalog.json`).
pub fn at2814() -> MotorModel {
    MotorModel::new(kv_si(900.0), 0.082, 1.2, 10.0).unwrap()
}

/// LiPo pack.  Pack resistance is an ASSUMPTION (not published): a generic
/// `R_cell = 0.6 V / (C_rate * capacity_Ah)`, times series cells.
pub fn pack(capacity_ah: f64, cells: u32, c_rate: f64) -> BatteryModel {
    let r = f64::from(cells) * 0.6 / (c_rate * capacity_ah);
    let mut b = BatteryModel::new(capacity_ah * 3600.0, cells, r).unwrap();
    b.i_cont_a = Some(c_rate * capacity_ah);
    b
}

pub fn chain(prop_d_in: f64, prop_pitch_in: f64, pack: BatteryModel, motors: u32) -> Powertrain {
    Powertrain {
        motor: at2814(),
        // ASSUMPTION: ESC efficiency 0.96 at 30 A / 11.1 V (not published).
        esc: EscModel::from_nominal_efficiency(0.96, 11.1, 30.0).unwrap(),
        battery: pack,
        prop: PropModel::from_bemt(
            BemtGeometry::generic(prop_d_in * 0.0254, prop_pitch_in * 0.0254, 2, 0.15).unwrap(),
        ),
        n_motors: motors,
    }
}

fn node(name: &str, mass: f64, part: Part) -> Node {
    Node {
        name: name.into(),
        parent: None,
        parent_frame: None,
        offset_m: [0.0; 3],
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: Some(mass),
        part,
    }
}

// One argument per published design number; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub fn spec(
    name: &str,
    root_chord: f64,
    panels: Vec<WingPanel>,
    wing_x: f64,
    horizontal: TailSurface,
    vertical: Option<TailSurface>,
    tail: TailKind,
    boom: Option<TailBoom>,
    empty_kg: f64,
    payload_kg: f64,
    cans: u32,
    announced_m: f64,
) -> UavSpec {
    let preset = presets::load("acc-2026").unwrap();
    UavSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        name: name.into(),
        master: MasterParams {
            root_chord_m: root_chord,
            panels,
            winglet: None,
            wing_xyz_m: [wing_x, 0.0, 0.0],
            fuselage_xyz_m: [0.0; 3],
            tail_xyz_m: [1.2, 0.0, 0.0],
            tail_boom: boom,
            controls: Vec::new(),
            empennage: EmpennageDriving::Explicit {
                horizontal: Some(horizontal),
                vertical,
            },
            layout: Default::default(),
        },
        topology: Topology {
            wing: WingKind::Straight,
            tail,
            motors: MotorLayout::Twin {
                span_fraction: 0.2,
                rotation: Rotation::Counter,
            },
            gear: GearKind::Taildragger,
            fuselage: FuselageKind::Boom,
        },
        subskeletons: BTreeMap::new(),
        tolerances_m: BTreeMap::new(),
        tree: vec![
            node(
                "empty aircraft (published)",
                empty_kg,
                Part::Allowance { mass_kg: empty_kg },
            ),
            node(
                "cans",
                payload_kg,
                Part::Payload {
                    role: "cans".into(),
                },
            ),
        ],
        mission: MissionSpec {
            preset: PresetRef {
                id: "acc-2026".into(),
                version: preset.version,
                variant: None,
            },
            parameters: BTreeMap::new(),
            decisions: Decisions {
                announced_takeoff_m: Some(announced_m),
                payload_count: Some(cans),
                declared_payload_count: Some(cans),
                takeoff_confidence: None,
            },
            segments: preset.segments,
            rounds: 3,
        },
        origins: BTreeMap::new(),
    }
}

pub fn panel(span: f64, taper: f64, sweep: f64, dihedral: f64, foil: &str) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: sweep,
        dihedral_deg: dihedral,
        twist_tip_deg: 0.0,
        airfoil_root: foil.into(),
        airfoil_tip: foil.into(),
    }
}

pub fn tail(span: f64, root: f64, taper: f64, dihedral: f64, foil: &str) -> TailSurface {
    TailSurface {
        span_m: span,
        root_chord_m: root,
        taper_ratio: taper,
        sweep_le_deg: 0.0,
        incidence_deg: 0.0,
        dihedral_deg: dihedral,
        airfoil: foil.into(),
    }
}

/// Twin AT2814 nacelles: bluff 35 mm cylinders on the wing (CD 0.8 on frontal
/// area, Q 1.3; ASSUMPTION).
fn nacelles() -> Component {
    Component::new(
        "motor nacelles",
        Shape::Item {
            cd: 0.8,
            frontal_m2: std::f64::consts::PI / 4.0 * 0.035_f64.powi(2),
        },
    )
    .with_junction(Junction::NacelleNear)
    .with_count(2)
    .in_slipstream()
}

/// Taildragger gear: two main wheels 80 x 25 mm, tail wheel 30 x 10 mm, two
/// flat spring legs (Raymer Table 12.6: flat spring leg CD 1.4 on 12 x 120 mm;
/// wheels CD 0.25 regular).  Sizes are ASSUMPTIONS.
fn taildragger_gear() -> Vec<Component> {
    vec![
        wheel("main wheels", 0.08, 0.025, 2),
        wheel("tail wheel", 0.03, 0.01, 1),
        Component::new(
            "main gear legs",
            Shape::Item {
                cd: 1.4,
                frontal_m2: 0.012 * 0.12,
            },
        )
        .with_count(2)
        .as_gear(),
        nacelles(),
    ]
}

/// Twin tractor motors on the wing: ASSUMPTION y = +-0.30 m, propeller plane
/// 0.20 m ahead of the quarter-chord line (not published for any team).
fn motor_stations() -> Vec<MotorStation> {
    [-0.30, 0.30]
        .into_iter()
        .map(|y_m| MotorStation {
            y_m,
            s_to_wing_m: 0.20,
        })
        .collect()
}

pub fn hull(length: f64, d_eq: f64) -> Hull {
    Hull {
        motors: motor_stations(),
        fuselage: Some(Fuselage {
            length_m: length,
            diameter_m: d_eq,
            wetted_m2: None,
        }),
        items: taildragger_gear(),
        ..Hull::default()
    }
}

/// Move the main wheels and legs of a hull to the ground-only list.
pub fn retractable(mut h: Hull) -> Hull {
    let (gone, kept): (Vec<_>, Vec<_>) = h
        .items
        .into_iter()
        .partition(|c| c.name == "main wheels" || c.name == "main gear legs");
    h.items = kept;
    h.ground_only = gone;
    h
}

pub fn ground() -> GroundSetup {
    // ASSUMPTIONS shared by all three: mu 0.06 (WUT measured 0.05-0.07 on the
    // ACC field), ground attitude 2 deg, wing 0.12 m above the ground,
    // n_max 3.5.
    GroundSetup {
        mu_roll: 0.06,
        mu_brake: 0.3,
        ground_alpha_deg: 2.0,
        wing_height_m: 0.12,
        n_max: 3.5,
    }
}

/// Trencalos Team (UPC), ACC 2026 report 05; page refs [p.N].
pub fn trencalos() -> Team {
    // Wing [p.13, p.38]: S3002, b 3.500 m, S 1.079 m2, c_root 0.370, c_tip
    // 0.1721, dihedral 3.5 deg, no twist, no sweep (quarter-chord straight);
    // two trapezoids per semi-wing with a central rectangular segment carrying
    // the motors [p.8-9].  The central half-width is DERIVED so that the
    // published S is met (1.316 m centre rectangle): S = 0.37 w + 0.27105 (3.5 - w).
    let w_c = (1.079_f64 - 0.27105 * 3.5) / (0.37 - 0.27105);
    let outer = 0.5 * (3.5 - w_c);
    let lead = (0.37 - 0.1721) / 4.0;
    let sweep = (lead / outer).atan().to_degrees();
    let panels = vec![
        panel(0.5 * w_c, 1.0, 0.0, 3.5, "s3002"),
        panel(outer, 0.1721 / 0.37, sweep, 3.5, "s3002"),
    ];
    // Tail [p.13]: Sh 0.180 m2, bh 1.024 m, NACA 0010; Sv 0.110 m2, bv 0.5 m.
    let spec = spec(
        "trencalos-acc2026",
        0.370,
        panels,
        0.3,
        tail(1.024, 0.180 / 1.024, 1.0, 0.0, "naca0010"),
        Some(tail(0.5, 0.110 / 0.5, 1.0, 0.0, "naca0010")),
        TailKind::Conventional,
        // ASSUMPTION: carbon-tube boom, 1.0 m x 25 mm (lt 1.12 m published).
        Some(TailBoom {
            length_m: 1.0,
            outer_diameter_m: 0.025,
            wall_m: 0.0015,
        }),
        // Component table: OEW 3.265 kg + 15 cans (0.350 kg) = 8.515 kg; the
        // performance table (Table 10, p.27) uses MTOW 8.79 kg: the
        // 0.275 kg difference is carried as an unexplained allowance.
        3.265 + 0.275,
        5.25,
        15,
        60.0,
    );
    // ASSUMPTION: can bay 3 rows x 5 cans (66 mm x 115 mm) plus nose:
    // 0.9 m long, equivalent diameter 0.175 m.
    Team {
        name: "Trencalos (UPC) report 05",
        spec,
        hull: hull(0.9, 0.175),
        // 2 x AT2814, APC 12x8 E [p.25], Gens Ace Soaring 3300 mAh 3S 30C.
        chain: chain(12.0, 8.0, pack(3.3, 3, 30.0), 2),
        ground: ground(),
        rho: 1.15,
        empty_kg: 3.265,
        reported: vec![
            rep(
                "static thrust at ~15 A (one motor)",
                11.0,
                "N",
                true,
                "p.25 test stand",
            ),
            rep("stall speed", 9.70, "m/s", false, "Table 10 p.27"),
            rep("maximum speed", 26.2, "m/s", false, "Table 10 p.27"),
            rep("max rate of climb", 1.301, "m/s", false, "Table 10 p.27"),
            rep("speed of best climb", 15.27, "m/s", false, "Table 10 p.27"),
            rep(
                "take-off distance",
                60.0,
                "m",
                false,
                "Table 10 p.27 (design constraint)",
            ),
        ],
    }
}

/// Xtra2UPV XTRA28 "Lince", ACC 2026 report 02.
pub fn xtra2upv() -> Team {
    // Wing [p.14, p.32]: S 0.650, b 2.378, c_root 0.316, elliptical planform
    // with chord 0.235 m at 80 % half span, dihedral 0/2 deg, AG44ct-02f.
    // The planform is approximated by a centre rectangle (derived half-width so
    // that S = 0.650 holds with a 0.05 m tip chord) and a linear taper outside.
    let half = 1.189;
    let (c0, c_tip) = (0.316, 0.05);
    let w = (0.650 / 2.0 - c_tip_term(half, c0, c_tip)) / (c0 - 0.5 * (c0 + c_tip));
    let outer = half - w;
    let lead = (c0 - c_tip) / 4.0;
    let panels = vec![
        panel(w, 1.0, 0.0, 0.0, "ag44ct02r"),
        panel(
            outer,
            c_tip / c0,
            (lead / outer).atan().to_degrees(),
            2.0,
            "ag44ct02r",
        ),
    ];
    // V-tail [p.14, p.32]: bt 0.625 m, root chord 0.209 m, S 0.11 m2, 40 deg.
    let root = 0.209;
    let mean = 0.11 / 0.625;
    let spec = spec(
        "xtra2upv-acc2026",
        c0,
        panels,
        0.3,
        tail(0.625, root, 2.0 * mean / root - 1.0, 40.0, "naca0006"),
        None,
        TailKind::V { dihedral_deg: 40.0 },
        // ASSUMPTION: short boom 0.55 m x 20 mm.
        Some(TailBoom {
            length_m: 0.55,
            outer_diameter_m: 0.02,
            wall_m: 0.0015,
        }),
        // 9 cans: OEW 2.917 kg estimated, MTOW 6.07 kg [p.19].
        2.917,
        3.15,
        9,
        60.0,
    );
    Team {
        name: "Xtra2UPV (UPV) report 02",
        spec,
        // ASSUMPTION: wide rounded prism, 0.85 m long, equivalent diameter 0.17 m.
        hull: hull(0.85, 0.17),
        // 2 x AT2814, Aero-naut CamCarbon 11x8, 2 x Tattu 1300 mAh 3S 100C in
        // parallel (2.6 Ah) [p.9-10].
        chain: chain(11.0, 8.0, pack(2.6, 3, 100.0), 2),
        ground: ground(),
        rho: 1.17,
        empty_kg: 3.2,
        reported: vec![
            rep(
                "static thrust at 15 A (one motor)",
                10.24,
                "N",
                true,
                "p.9 test stand",
            ),
            rep(
                "stall speed (VTO 46.6 km/h = 1.2 Vs)",
                46.6 / 3.6 / 1.2,
                "m/s",
                false,
                "p.22",
            ),
            rep("cruise speed", 81.0 / 3.6, "m/s", false, "p.22 Table 7.1"),
            rep("max rate of climb (9 cans)", 1.32, "m/s", false, "p.22"),
            rep(
                "take-off distance",
                60.0,
                "m",
                false,
                "p.22 (design constraint)",
            ),
        ],
    }
}

fn c_tip_term(half: f64, c0: f64, c_tip: f64) -> f64 {
    0.5 * (c0 + c_tip) * half
}

/// UVigo Aerotech "Aine", ACC 2026 report 29.
pub fn uvigo() -> Team {
    // Wing [p.12-14, p.29]: b 2.2 m (text; the drawing says 1.91 m), S 0.509,
    // taper 0.367, inner straight section, 7 deg dihedral from y = 0.5 m,
    // c_root 0.30 m (target 0.25-0.31; tip not below 0.1 m), modified NACA 2412.
    // The straight inner half-width 0.305 m is DERIVED to meet S = 0.509.
    let (c0, ct, half, w) = (0.30, 0.110, 1.1, 0.305);
    let chord = |y: f64| c0 - (c0 - ct) * (y - w) / (half - w);
    let lead = |y0: f64, y1: f64| {
        ((chord(y0) - chord(y1)) / 4.0 / (y1 - y0))
            .atan()
            .to_degrees()
    };
    let panels = vec![
        panel(w, 1.0, 0.0, 0.0, "naca2412"),
        panel(0.5 - w, chord(0.5) / c0, lead(w, 0.5), 0.0, "naca2412"),
        panel(
            half - 0.5,
            ct / chord(0.5),
            lead(0.5, half),
            7.0,
            "naca2412",
        ),
    ];
    // HS [p.17]: b 0.75, S 0.1, taper 0.5, dihedral 12, NACA 0012;
    // VS: b 0.55, S 0.04, taper 0.67.
    let h_root = 2.0 * (0.1 / 0.75) / 1.5;
    let v_root = 2.0 * (0.04 / 0.55) / 1.67;
    let spec = spec(
        "uvigo-acc2026",
        c0,
        panels,
        0.3,
        tail(0.75, h_root, 0.5, 12.0, "naca0012"),
        Some(tail(0.55, v_root, 0.67, 0.0, "naca0012")),
        TailKind::Conventional,
        None,
        // Drawing [p.29]: OEW 2.6 kg, payload 2.8 kg, MTOW 5.4 kg.
        2.6,
        2.8,
        8,
        40.0,
    );
    Team {
        name: "UVigo Aerotech report 29",
        spec,
        // ASSUMPTION: full-length slender fuselage 1.4 m, equivalent diameter 0.12 m.
        // The main gear retracts into the wing [p.13, p.23]: wheels and legs
        // act on the ground only; the tail wheel stays out.
        hull: retractable(hull(1.4, 0.12)),
        // 2 x AT2814, Aero-naut POWER 11x8, Gens Ace 4000 mAh 3S 30C [p.8-9].
        chain: chain(11.0, 8.0, pack(4.0, 3, 30.0), 2),
        ground: ground(),
        rho: 1.2,
        empty_kg: 2.6,
        reported: vec![
            rep(
                "take-off distance (60 A take-off)",
                35.0,
                "m",
                false,
                "p.19 prediction",
            ),
            rep("cruise speed", 24.0, "m/s", false, "p.12"),
            rep("maximum speed", 28.0, "m/s", false, "p.12"),
            rep("max vertical speed", 1.87, "m/s", false, "p.12"),
            rep(
                "stall speed requirement",
                10.0,
                "m/s",
                false,
                "p.12 (requirement, not a result)",
            ),
        ],
    }
}

pub fn vehicle(t: &Team, cap_a: Option<f64>) -> Vehicle<'_> {
    Vehicle {
        spec: &t.spec,
        hull: t.hull.clone(),
        chain: t.chain.clone(),
        electric: ElectricSettings {
            battery_current_cap_a: cap_a,
            ..ElectricSettings::default()
        },
        ground: t.ground,
        aero: AeroSettings::default(),
        extra_inputs: BTreeMap::new(),
    }
}
