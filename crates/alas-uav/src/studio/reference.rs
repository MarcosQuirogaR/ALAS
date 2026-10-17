// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reference aircraft: Xtra2UPV "XTRA28 Lince" (ACC 2026 report 02), built
//! from a spec with the studio functions.
//!
//! It is the closest PUBLISHED aircraft to the owner's class (V-tail on a
//! boom, twin AT2814 tractors on a low wing, taildragger). The owner's own CAD
//! assembly is not available, so nothing here is claimed to be that aircraft.
//!
//! Provenance of the numbers (page references are the report's PDF pages,
//! competitions.md section 2.2):
//! * published: wing span 2.378 m, root chord 0.316 m, area 0.650 m2, dihedral
//!   0/2 deg, V-tail span 0.625 m, root chord 0.209 m, area 0.11 m2, 40 deg
//!   [p.14, p.32]; AT2814 900 KV, 11x8 propellers, 2.6 Ah 3S pack [p.9-10];
//!   9 cans of 0.35 kg [p.6]; OEW 2.917 kg estimated, 3.2 kg as built [p.19, p.30].
//! * ASSUMED (not published; same values as the M2 blind builds): short boom
//!   0.55 m x 20 mm, pod 0.85 m long with an equivalent diameter 0.17 m
//!   (here 0.19 x 0.12 m section), twin motors at 25 % half span, wing LE at
//!   25 % of the pod, pack resistance `0.6 V cells / (C Ah)`, ESC efficiency 0.96.
//! * SUBSTITUTED parts (database geometry and mass stand in for a part that is
//!   not in the catalogue; listed in [`Reference::substitutions`]).

use alas_uav_db::v1::{convert_v1, V1Options};
use alas_uav_db::{ComponentId, Database, IsoDate};

use crate::airframe::mass::LoadCase;
use crate::coupled::{AeroSettings, ElectricSettings, GroundSetup};
use crate::perf::{Environment, TakeoffSpec};
use crate::power::{
    from_spec::{esc_from_spec, motor_from_spec},
    BatteryModel, BemtGeometry, Powertrain, PropModel,
};
use crate::spec::master::{EmpennageDriving, TailBoom, TailSurface, WingPanel};
use crate::spec::skeleton::{Datum, Node, Part};
use crate::spec::topology::{GearKind, TailKind};
use crate::spec::UavSpec;

use super::feasibility::{Inputs, Requirements};
use super::markers::fit_wing_x_for_margin;
use super::pack::{mount, mount_motor, pack, PartRequest};
use super::publish::refresh;
use super::topology::{build, Preset, Sizing};
use super::StudioError;

const CATALOGUE: &str = include_str!("../../data/multi_source_catalog.json");
const IN: f64 = 0.0254;
/// ASSUMED mass of one 11x8 carbon-composite propeller, kg (not published).
const PROP_MASS_ASSUMED_KG: f64 = 0.020;

/// A stand-in part.
#[derive(Debug, Clone, PartialEq)]
pub struct Substitution {
    /// What the report says.
    pub published: &'static str,
    /// Database id used for geometry and mass.
    pub used: &'static str,
    /// Why it is acceptable and what differs.
    pub note: &'static str,
}

/// The built reference.
pub struct Reference {
    /// The design (publications derived, parts packed and mounted).
    pub spec: UavSpec,
    /// Component database (v1 catalogue converted; confidence 0.5 per field).
    pub db: Database,
    /// Powertrain for the electric models.
    pub chain: Powertrain,
    /// Load cases: 0, 9 and 14 cans.
    pub cases: Vec<LoadCase>,
    /// Stand-in parts.
    pub substitutions: Vec<Substitution>,
    /// Published empty mass, CAD estimate, kg [p.19].
    pub published_oew_estimate_kg: f64,
    /// Published empty mass of the first built unit, kg [p.30].
    pub published_oew_built_kg: f64,
    /// Motor can diameter, m (for the nacelle drag item).
    pub motor_diameter_m: f64,
}

impl Reference {
    /// Feasibility inputs at the conditions of the report (density 1.17 kg/m3,
    /// 30 A cap, 60 m take-off, stall below 11.9 m/s) for a borrowed `Reference`.
    pub fn inputs(&self) -> Inputs<'_> {
        Inputs {
            spec: &self.spec,
            db: &self.db,
            chain: &self.chain,
            electric: ElectricSettings {
                battery_current_cap_a: Some(30.0),
                ..ElectricSettings::default()
            },
            ground: GroundSetup {
                mu_roll: 0.06,
                mu_brake: 0.3,
                ground_alpha_deg: 2.0,
                wing_height_m: 0.12,
                n_max: 3.5,
            },
            aero: AeroSettings::default(),
            env: Environment {
                rho: 1.17,
                headwind_m_s: 0.0,
            },
            cases: &self.cases,
            requirements: Requirements {
                max_stall_m_s: Some(11.9),
                runway_m: Some(60.0),
                ..Requirements::default()
            },
            cl_max_assumed: 1.2,
            takeoff: TakeoffSpec::default(),
            motor_diameter_m: Some(self.motor_diameter_m),
        }
    }
}

fn id(s: &str) -> Result<ComponentId, StudioError> {
    ComponentId::new(s).map_err(|e| StudioError::Invalid(e.to_string()))
}

fn panel(span: f64, taper: f64, sweep: f64, dihedral: f64) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: sweep,
        dihedral_deg: dihedral,
        twist_tip_deg: 0.0,
        airfoil_root: "ag44ct02r".into(),
        airfoil_tip: "ag44ct02r".into(),
    }
}

/// Build the Xtra2UPV reference.
///
/// # Errors
///
/// Catalogue conversion, packaging or model failures.
pub fn xtra2upv() -> Result<Reference, StudioError> {
    // Wing [p.14, p.32]: elliptic planform approximated by a centre rectangle
    // (width derived so that S = 0.650 m2 holds with a 0.05 m tip chord) and a
    // linear taper outside, as in the M2 blind build.
    let (half, c0, c_tip, area): (f64, f64, f64, f64) = (1.189, 0.316, 0.05, 0.650);
    let w = (area / 2.0 - 0.5 * (c0 + c_tip) * half) / (c0 - 0.5 * (c0 + c_tip));
    let outer = half - w;
    let sweep = (((c0 - c_tip) / 4.0) / outer).atan().to_degrees();
    let sizing = Sizing {
        name: "xtra2upv-xtra28".into(),
        span_m: 2.0 * half,
        root_chord_m: c0,
        taper: c_tip / c0,
        airfoil: "ag44ct02r".into(),
        pod_length_m: 0.85,
        pod_width_m: 0.19,
        pod_height_m: 0.12,
        tail_arm_m: 1.2,
        mission_preset: "acc-2026".into(),
    };
    let mut spec = build(Preset::VTailTwin, &sizing)?;
    spec.master.panels = vec![
        panel(w, 1.0, 0.0, 0.0),
        panel(outer, c_tip / c0, sweep, 2.0),
    ];
    // V-tail [p.14, p.32]: span along the surface 0.625 m, S = 0.11 m2.
    let mean = 0.11 / 0.625;
    spec.master.empennage = EmpennageDriving::Explicit {
        horizontal: Some(TailSurface {
            span_m: 0.625,
            root_chord_m: 0.209,
            taper_ratio: 2.0 * mean / 0.209 - 1.0,
            sweep_le_deg: 0.0,
            incidence_deg: 0.0,
            dihedral_deg: 40.0,
            airfoil: "naca0006".into(),
        }),
        vertical: None,
    };
    spec.topology.tail = TailKind::V { dihedral_deg: 40.0 };
    spec.topology.gear = GearKind::Taildragger;
    spec.master.tail_boom = Some(TailBoom {
        length_m: 0.55,
        outer_diameter_m: 0.02,
        wall_m: 0.0015,
    });
    spec.master.tail_xyz_m = [0.85 + 0.55, 0.0, 0.0];
    if let Some(g) = spec.master.layout.gear.as_mut() {
        g.aux_x_m = 1.45;
    }
    spec.mission.decisions.announced_takeoff_m = Some(60.0);
    spec.mission.decisions.payload_count = Some(9);
    spec.mission.decisions.declared_payload_count = Some(9);
    refresh(&mut spec)?;

    let converted = convert_v1(
        CATALOGUE,
        &V1Options {
            retrieved_at: None,
            today: IsoDate::parse("2026-10-13").map_err(|e| StudioError::Invalid(e.to_string()))?,
        },
    )
    .map_err(|e| StudioError::Invalid(e.to_string()))?;
    let db = converted.database;

    // Parts. Servos use the published 9 g mass (override), catalogue geometry.
    let (motor, esc, pack_id, rx, servo, prop) = (
        id("tmotor-at2814-900kv")?,
        id("hobbywing-skywalker-40a-v2-na")?,
        id("unmannedtech-cnhl-black-2000-3s-100c-xt60")?,
        id("radiomaster-er8gv-elrs-pwm-vario")?,
        id("hitec-hs225bb")?,
        id("apc-11x7e")?,
    );
    let mut s = pack_parts(&spec, &db, &esc, &pack_id, &rx)?;
    for side in ["R", "L"] {
        let frame = format!("PS_Motor_{side}_Plane");
        s.tree.push(mount_motor(
            &db,
            &format!("motor {side}"),
            &motor,
            &frame,
            false,
        )?);
        let mut p = mount(
            &format!("propeller {side}"),
            &prop,
            &frame,
            [0.0; 3],
            [0.0; 3],
        );
        // ASSUMED mass: the catalogue row carries none and the report does not give it.
        p.mass_override_kg = Some(PROP_MASS_ASSUMED_KG);
        s.tree.push(p);
    }
    let seats: Vec<String> = s
        .subskeletons
        .values()
        .flatten()
        .filter(|p| p.name.contains("_Servo_") && matches!(p.datum, Datum::Point { .. }))
        .map(|p| p.name.clone())
        .collect();
    for seat in seats {
        let mut n = mount(&format!("servo {seat}"), &servo, &seat, [0.0; 3], [0.0; 3]);
        n.mass_override_kg = Some(0.009);
        s.tree.push(n);
    }
    s.tree.push(Node {
        name: "cans".into(),
        parent: None,
        parent_frame: Some("FS_Bay_Payload".into()),
        offset_m: [0.0; 3],
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: None,
        part: Part::Payload {
            role: "cans".into(),
        },
    });
    let case = |name: &str, kg: f64| {
        let mut c = LoadCase {
            name: name.into(),
            ..LoadCase::default()
        };
        c.payload_kg.insert("cans".into(), kg);
        c
    };
    let cases = vec![
        case("9 cans", 3.15),
        case("0 cans", 0.0),
        case("14 cans", 4.9),
    ];
    // The wing position is not published: slide it so the smallest margin over
    // the three cases is 10 % MAC (a design choice, ESTIMATE), keep the main
    // axle 0.3 root chords behind the wing leading edge.
    fit_wing_x_for_margin(&mut s, &db, &cases, 0.10)?;
    if let Some(g) = s.master.layout.gear.as_mut() {
        g.main_x_m = s.master.wing_xyz_m[0] + 0.3 * c0;
    }
    refresh(&mut s)?;
    s.validate()
        .map_err(|p| StudioError::Invalid(p.join("; ")))?;

    let row = db
        .current(&motor)
        .ok_or_else(|| StudioError::UnknownName(motor.as_str().into()))?;
    let alas_uav_db::Body::Motor(ms) = &row.body else {
        return Err(StudioError::Invalid("motor row".into()));
    };
    let alas_uav_db::Body::Esc(es) = &db
        .current(&esc)
        .ok_or_else(|| StudioError::UnknownName(esc.as_str().into()))?
        .body
    else {
        return Err(StudioError::Invalid("ESC row".into()));
    };
    let motor_diameter_m = ms.diameter_m.as_ref().map_or(0.035, |d| d.value);
    // Pack: 2 x 1300 mAh 3S 100C in parallel [p.10]; resistance is ASSUMED.
    let mut battery = BatteryModel::new(2.6 * 3600.0, 3, 3.0 * 0.6 / (100.0 * 2.6))?;
    battery.i_cont_a = Some(100.0 * 2.6);
    let chain = Powertrain {
        motor: motor_from_spec(ms, motor.as_str())?,
        esc: esc_from_spec(es, esc.as_str(), 0.96, 11.1)?,
        battery,
        prop: PropModel::from_bemt(BemtGeometry::generic(11.0 * IN, 8.0 * IN, 2, 0.15)?),
        n_motors: 2,
    };
    Ok(Reference {
        spec: s,
        db,
        chain,
        cases,
        substitutions: vec![
            Substitution {
                published: "T-Motor AM66A ESC",
                used: "hobbywing-skywalker-40a-v2-na",
                note: "2 x 40 A for geometry and mass only; the chain uses eta 0.96",
            },
            Substitution {
                published: "2 x Tattu Funfly 1300 mAh 3S 100C",
                used: "unmannedtech-cnhl-black-2000-3s-100c-xt60",
                note: "one 2.0 Ah pack for geometry and mass; the chain uses the published 2.6 Ah",
            },
            Substitution {
                published: "KST X08H Plus servo (9 g)",
                used: "hitec-hs225bb",
                note: "catalogue box (32 x 17 x 31 mm); mass overridden by the published 9 g",
            },
            Substitution {
                published: "Aero-naut CamCarbon 11x8",
                used: "apc-11x7e",
                note: "disc diameter only, mass ASSUMED 20 g; the chain uses a generic 11x8 BEMT blade",
            },
        ],
        published_oew_estimate_kg: 2.917,
        published_oew_built_kg: 3.2,
        motor_diameter_m,
    })
}

fn pack_parts(
    spec: &UavSpec,
    db: &Database,
    esc: &ComponentId,
    pack_id: &ComponentId,
    rx: &ComponentId,
) -> Result<UavSpec, StudioError> {
    let req = |name: &str, c: &ComponentId| PartRequest {
        name: name.into(),
        component: c.clone(),
        bay: None,
    };
    pack(
        spec,
        db,
        &[
            req("esc R", esc),
            req("esc L", esc),
            req("battery 1", pack_id),
            req("receiver", rx),
        ],
    )
}
