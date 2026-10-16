// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Live feasibility traffic lights (UAV-PLAN 6.2).
//!
//! * **L0** (closed form, recomputed on every edit): packaging, mass basis,
//!   static margin per load case, tail volume coefficients, wing loading and
//!   stall speed from an ASSUMED CLmax, static thrust and electrical ratings.
//! * **L1** (background, cancellable): the M2 chain - viscous VLM polar with
//!   the derived hull, powertrain, quasi-steady performance and ground roll.
//!
//! A light is not a verdict of physics: thresholds are named constants marked
//! ESTIMATE, and the value, the limit and the offending field path are carried
//! so a UI can explain and link it. `Unknown` means the model could not run.

use std::sync::atomic::{AtomicBool, Ordering};

use alas_uav_db::Database;

use crate::airframe::geometry::{AirframeGeometry, TailShape};
use crate::airframe::mass::LoadCase;
use crate::airframe::MassInputs;
use crate::coupled::{performance_summary, AeroSettings, ElectricSettings, GroundSetup, Vehicle};
use crate::perf::{Environment, TakeoffSpec, G0};
use crate::power;
use crate::spec::skeleton::Part;
use crate::spec::UavSpec;

use super::check::{check, Severity};
use super::hull::{derive_hull, with_explicit_empennage};
use super::markers::{markers, mass_inputs_for};
use super::StudioError;

pub use super::lights::{
    Area, Item, Light, Report, Requirements, MAX_ESTIMATED_MASS_SHARE, MIN_STATIC_THRUST_TO_WEIGHT,
    MIN_TAIL_VOLUME, RATING_MARGIN, STATIC_MARGIN_BAND,
};

/// Everything the lights need besides the spec.
pub struct Inputs<'a> {
    /// Design (with derived publications).
    pub spec: &'a UavSpec,
    /// Component database.
    pub db: &'a Database,
    /// Motor - ESC - pack - propeller.
    pub chain: &'a power::Powertrain,
    /// Electric settings.
    pub electric: ElectricSettings,
    /// Ground handling.
    pub ground: GroundSetup,
    /// Aerodynamic settings (L1).
    pub aero: AeroSettings,
    /// Air.
    pub env: Environment,
    /// Load cases; the first one is the take-off case of L1.
    pub cases: &'a [LoadCase],
    /// Requirements.
    pub requirements: Requirements,
    /// CLmax ASSUMED by L0 for the stall estimate (clean wing; L1 computes it).
    pub cl_max_assumed: f64,
    /// Take-off settings (L1).
    pub takeoff: TakeoffSpec,
    /// Motor can diameter for the nacelle drag item, m.
    pub motor_diameter_m: Option<f64>,
}

fn rating(used: f64, rated: f64) -> Light {
    if used > rated {
        Light::Red
    } else if used > RATING_MARGIN * rated {
        Light::Amber
    } else {
        Light::Green
    }
}

/// L0: closed-form lights.
///
/// # Errors
///
/// Failures of the geometry, mass or database models (not of a light).
pub fn l0(inp: &Inputs<'_>) -> Result<Report, StudioError> {
    let mut rep = Report {
        level: 0,
        ..Report::default()
    };
    let packed = check(inp.spec, inp.db)?;
    let worst = packed.issues.first();
    let light = match worst.map(|i| i.severity) {
        Some(Severity::Error) => Light::Red,
        Some(Severity::Warning) => Light::Amber,
        None => Light::Green,
    };
    let note = worst.map_or_else(
        || format!("{} parts, no interference", packed.parts.len()),
        |i| format!("{} ({} finding(s))", i.message, packed.issues.len()),
    );
    rep.push(Area::Packaging, light, "packaging", note);

    let mass_inputs: MassInputs = mass_inputs_for(inp.spec);
    let mk = markers(inp.spec, inp.db, &mass_inputs, inp.cases)?;
    let total = mk.mass.total_mass_kg();
    let share = mk.mass.estimate_fraction();
    let it = rep.push(
        Area::Mass,
        if share > MAX_ESTIMATED_MASS_SHARE {
            Light::Amber
        } else {
            Light::Green
        },
        "mass",
        format!(
            "{:.0} % of the mass is an uncalibrated estimate (band +/-8 % of that part)",
            100.0 * share
        ),
    );
    it.value = Some(total);
    it.unit = "kg";

    for sp in &mk.stability {
        let (lo, hi) = inp.requirements.static_margin;
        let light = if sp.static_margin < lo || sp.static_margin > hi {
            Light::Red
        } else {
            Light::Green
        };
        let it = rep.push(
            Area::Stability,
            light,
            &format!("static margin ({})", sp.case),
            format!(
                "CG {:.3} m, NP {:.3} m, closed-form power-off estimate",
                sp.x_cg_m, mk.neutral_point.x_np_m
            ),
        );
        it.value = Some(sp.static_margin);
        it.limit = Some(lo);
        it.unit = "MAC";
        it.field = Some("tree (battery / ballast position)".into());
    }

    let g =
        AirframeGeometry::from_master(&inp.spec.master, &inp.spec.topology, &TailShape::default())?;
    let s_w = g.wing.projected_area_m2();
    let weight = total * G0;
    let vs = (2.0 * weight / (inp.env.rho * s_w * inp.cl_max_assumed)).sqrt();
    let light = match inp.requirements.max_stall_m_s {
        Some(v) if vs > v => Light::Red,
        _ => Light::Amber,
    };
    let it = rep.push(
        Area::Aero,
        light,
        "stall speed (L0)",
        format!(
            "wing loading {:.1} kg/m2; CLmax {:.2} is ASSUMED (clean wing), L1 computes it",
            total / s_w,
            inp.cl_max_assumed
        ),
    );
    it.value = Some(vs);
    it.limit = inp.requirements.max_stall_m_s;
    it.unit = "m/s";
    it.field = Some("master.panels / root_chord_m".into());

    if let Some(h) = &g.horizontal {
        let arm =
            h.planform.aerodynamic_centre_m()[0] - (g.wing.x_le_mac_m() + 0.25 * g.wing.mac_m());
        let vh = h.effective_horizontal_area_m2() * arm / (s_w * g.wing.mac_m());
        let it = rep.push(
            Area::Stability,
            if vh < MIN_TAIL_VOLUME.0 {
                Light::Amber
            } else {
                Light::Green
            },
            "horizontal tail volume",
            "effective area (V pair: S cos^2 gamma) times arm over S c".into(),
        );
        it.value = Some(vh);
        it.limit = Some(MIN_TAIL_VOLUME.0);
        it.unit = "1";
        it.field = Some("master.empennage".into());
        if let Some(v) = &g.vertical {
            let vv = v.effective_vertical_area_m2() * arm / (s_w * g.wing.span_m());
            let it = rep.push(
                Area::Stability,
                if vv < MIN_TAIL_VOLUME.1 {
                    Light::Amber
                } else {
                    Light::Green
                },
                "vertical tail volume",
                "fin area times the same arm over S b".into(),
            );
            it.value = Some(vv);
            it.limit = Some(MIN_TAIL_VOLUME.1);
            it.unit = "1";
        }
    }
    electrical_l0(inp, weight, &mut rep)?;
    Ok(rep)
}

fn electrical_l0(inp: &Inputs<'_>, weight: f64, rep: &mut Report) -> Result<(), StudioError> {
    // Direct equilibrium at full duty: no limiter and no BEMT tabulation, so L0
    // stays cheap.
    let t_amb = inp.electric.t_amb_k;
    let dynamics = power::Dynamics {
        battery: inp
            .chain
            .battery
            .start_state(inp.electric.soc_start, t_amb)?,
        t_winding_k: inp.electric.t_winding_k.unwrap_or(t_amb),
    };
    let flight = power::Flight::new(0.0, inp.env.rho, t_amb);
    let p = match inp.chain.solve_duty(&flight, &dynamics, 1.0) {
        Ok(p) => p,
        Err(e) => {
            rep.push(
                Area::Electrical,
                Light::Unknown,
                "static point",
                format!("the powertrain has no equilibrium at V = 0: {e}"),
            );
            return Ok(());
        }
    };
    let tw = p.thrust_n / weight;
    let it = rep.push(
        Area::Electrical,
        if tw < MIN_STATIC_THRUST_TO_WEIGHT {
            Light::Amber
        } else {
            Light::Green
        },
        "static thrust / weight",
        "full duty, fresh pack, generic or mapped propeller as configured".into(),
    );
    it.value = Some(tw);
    it.limit = Some(MIN_STATIC_THRUST_TO_WEIGHT);
    it.unit = "1";
    let n = f64::from(inp.chain.n_motors.max(1));
    for (label, used, rated) in [
        (
            "pack current vs rating",
            p.i_battery_a,
            inp.chain.battery.i_cont_a,
        ),
        (
            "ESC current vs rating",
            p.i_battery_a / n,
            inp.chain.esc.i_cont_a,
        ),
        (
            "motor current vs rating",
            p.i_battery_a / n,
            inp.chain.motor.limits.i_cont_a,
        ),
    ] {
        match rated {
            Some(r) => {
                let it = rep.push(
                    Area::Electrical,
                    rating(used, r),
                    label,
                    "static current, uncapped, as a share of the continuous rating".into(),
                );
                it.value = Some(used);
                it.limit = Some(r);
                it.unit = "A";
            }
            None => {
                rep.push(
                    Area::Electrical,
                    Light::Unknown,
                    label,
                    "the database row has no continuous rating".into(),
                );
            }
        }
    }
    Ok(())
}

/// Spec for the M2 chain: explicit empennage and a tree of two mass nodes
/// (airframe and parts bottom-up, payload of the first load case).
///
/// # Errors
///
/// Mass-statement or geometry failures.
pub fn spec_for_chain(
    spec: &UavSpec,
    db: &Database,
    cases: &[LoadCase],
) -> Result<UavSpec, StudioError> {
    let mut out = with_explicit_empennage(spec)?;
    let case = cases
        .first()
        .ok_or_else(|| StudioError::Invalid("no load case".into()))?;
    let ms = crate::airframe::build_mass_statement(spec, db, &mass_inputs_for(spec), case)?;
    let payload = ms.group_mass_kg(crate::airframe::MassGroup::Payload);
    let node = |name: &str, mass: f64, part: Part| crate::spec::skeleton::Node {
        name: name.into(),
        parent: None,
        parent_frame: None,
        offset_m: [0.0; 3],
        euler_deg: [0.0; 3],
        cg_point: None,
        mass_override_kg: Some(mass),
        part,
    };
    out.tree = vec![node(
        "airframe and parts (bottom-up)",
        ms.total_mass_kg() - payload,
        Part::Allowance {
            mass_kg: ms.total_mass_kg() - payload,
        },
    )];
    if payload > 0.0 {
        out.tree.push(node(
            "payload",
            payload,
            Part::Payload {
                role: "payload".into(),
            },
        ));
    }
    Ok(out)
}

/// L1: L0 plus the coupled chain. `cancel` is polled between stages.
///
/// # Errors
///
/// As [`l0`]; failures of the coupled models become `Unknown` rows.
pub fn l1(inp: &Inputs<'_>, cancel: Option<&AtomicBool>) -> Result<Report, StudioError> {
    let mut rep = l0(inp)?;
    rep.level = 1;
    let stop = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    if stop() {
        return Ok(rep);
    }
    let resolved = spec_for_chain(inp.spec, inp.db, inp.cases)?;
    let vehicle = Vehicle {
        spec: &resolved,
        hull: derive_hull(inp.spec, inp.motor_diameter_m)?,
        chain: inp.chain.clone(),
        electric: inp.electric,
        ground: inp.ground,
        aero: inp.aero,
        extra_inputs: std::collections::BTreeMap::new(),
    };
    let models = match vehicle.models(&inp.env) {
        Ok(m) => m,
        Err(e) => {
            rep.push(Area::Aero, Light::Unknown, "L1 aerodynamics", e.to_string());
            return Ok(rep);
        }
    };
    if stop() {
        return Ok(rep);
    }
    for note in &models.notes {
        rep.push(Area::Aero, Light::Amber, "model limit", note.clone());
    }
    let s = match performance_summary(&models, inp.env, inp.takeoff) {
        Ok(s) => s,
        Err(e) => {
            rep.push(
                Area::Performance,
                Light::Unknown,
                "L1 performance",
                e.to_string(),
            );
            return Ok(rep);
        }
    };
    let light = match inp.requirements.max_stall_m_s {
        Some(v) if s.stall_speed_m_s > v => Light::Red,
        _ => Light::Green,
    };
    let it = rep.push(
        Area::Aero,
        light,
        "stall speed (L1)",
        "viscous VLM CLmax at its own Re".into(),
    );
    it.value = Some(s.stall_speed_m_s);
    it.limit = inp.requirements.max_stall_m_s;
    it.unit = "m/s";
    let it = rep.push(
        Area::Performance,
        if s.max_climb_rate_m_s > 0.0 {
            Light::Green
        } else {
            Light::Red
        },
        "best rate of climb",
        format!("at {:.1} m/s", s.v_best_climb_m_s),
    );
    it.value = Some(s.max_climb_rate_m_s);
    it.limit = Some(0.0);
    it.unit = "m/s";
    let light = match s.v_max_m_s {
        Some(v) if v > 1.2 * s.stall_speed_m_s => Light::Green,
        _ => Light::Red,
    };
    let it = rep.push(
        Area::Performance,
        light,
        "level speed margin",
        "Vmax over 1.2 Vstall".into(),
    );
    it.value = s.v_max_m_s;
    it.limit = Some(1.2 * s.stall_speed_m_s);
    it.unit = "m/s";
    let light = match inp.requirements.runway_m {
        Some(r) if s.takeoff.distance_m > r => Light::Red,
        _ => Light::Green,
    };
    let it = rep.push(
        Area::Performance,
        light,
        "take-off ground roll",
        "ground-effect roll with the current cap".into(),
    );
    it.value = Some(s.takeoff.distance_m);
    it.limit = inp.requirements.runway_m;
    it.unit = "m";
    if let Some(r) = inp.chain.battery.i_cont_a {
        let it = rep.push(
            Area::Electrical,
            rating(s.takeoff.peak_current_a, r),
            "take-off peak current",
            "peak battery current in the ground roll vs the continuous rating".into(),
        );
        it.value = Some(s.takeoff.peak_current_a);
        it.limit = Some(r);
        it.unit = "A";
    }
    Ok(rep)
}
