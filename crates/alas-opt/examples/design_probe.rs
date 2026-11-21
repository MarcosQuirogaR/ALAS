// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a design-vector change buys: the full in-loop assessment of named
//! design vectors under one configuration, as JSON (every residual with its
//! achieved value and limit, the mass breakdown and stations, centre of
//! gravity, neutral point, mean aerodynamic chord and the cruise drag terms
//! at the configured cruise Mach and each candidate's mid-cruise lift).
//!
//! Usage:
//!   design_probe CONFIG_JSON MTOW_MODE|config VARIANTS_JSON OUTPUT_JSON
//! `VARIANTS_JSON` maps a label to a design-vector object keyed by the design
//! variable names (missing names take the registered preset's value).
//! Units: SI (kg, m, degrees for angles), coefficients on the reference area.

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, DesignVector, MtowSizing};
use alas_opt::mdo::{assess_product_candidate, CandidateDrag};
use serde_json::{json, Map, Value};
use std::{error::Error, fs, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        return Err(
            "usage: design_probe CONFIG_JSON MTOW_MODE|config VARIANTS_JSON OUTPUT_JSON".into(),
        );
    }
    let mut config = AlasConfig::from_value(&serde_json::from_str::<Value>(&fs::read_to_string(
        &args[1],
    )?)?)?;
    match args[2].as_str() {
        "config" => {}
        "fixed_requirement" => {
            config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement
        }
        "mtow_band" => config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand,
        "payload_adjusted" => config.optimizer.objective.mtow_sizing = MtowSizing::PayloadAdjusted,
        other => return Err(format!("unknown MTOW mode {other:?}").into()),
    }
    let nominal = config.configured_nominal_design().to_array();
    let variants: Map<String, Value> = serde_json::from_str(&fs::read_to_string(&args[3])?)?;
    let mut report = Map::new();
    for (label, object) in &variants {
        let values: Vec<f64> = alas_config::design_variables::SPECS
            .iter()
            .zip(nominal.clone())
            .map(|(spec, nominal)| object[spec.name].as_f64().unwrap_or(nominal))
            .collect();
        let design = DesignVector::from_array(&values)?;
        let started = Instant::now();
        let row = match assess_product_candidate(&config, &design) {
            Ok(a) => {
                let req = &config.requirements;
                let arts = &a.sized.fuel_artifacts;
                let atmosphere = Atmosphere::new(req.cruise_altitude_m);
                let q = 0.5
                    * atmosphere.density()
                    * (req.cruise_mach * atmosphere.speed_of_sound()).powi(2);
                let mid_kg = a.sized.takeoff_mass_kg - 0.5 * a.sized.block_fuel_kg;
                let cl = mid_kg * req.gravity_m_s2 / (q * arts.reference_area_m2);
                let drag = match &arts.drag {
                    CandidateDrag::Table(t) => {
                        let (m, h) = (req.cruise_mach, req.cruise_altitude_m);
                        let cd0 = t.cd0(m, h);
                        json!({
                            "cl": cl, "mach": m, "altitude_m": h, "cd0": cd0,
                            "induced": t.induced_cd_at(cl, m),
                            "viscous_lift": t.viscous_lift_cd(cl, cd0),
                            "upsweep": t.upsweep_attitude_cd(cl, m),
                            "wave": t.wave_cd(cl, m),
                            "basic_loading": t.basic_loading_cd(),
                            "lattice_induced": t.lattice_induced_cd_at(cl, m),
                            "induced_cruise_curve": t.induced_cd(cl),
                            "effective_oswald": t.effective_oswald(cl),
                            "design_cl": t.design_cl(),
                            "table": if std::env::var_os("DESIGN_PROBE_TABLE").is_some() {
                                serde_json::to_value(t).ok().and_then(|v| v.get("induced").cloned())
                            } else { None },
                            "parasite": t.parasite_components(m, h)
                                .map(|(l, c)| (l.to_string(), json!(c))).collect::<Map<_, _>>(),
                        })
                    }
                    _ => Value::Null,
                };
                let c = &a.resolved.coords;
                // The candidate as the report draws it: solved tails, placed
                // gear; its mean-chord frame, tailplane area and main legs.
                let mut drawn = config.clone();
                let mut drawn_design = a.resolved.design;
                a.resolved
                    .tail_sizing
                    .apply_to(&mut drawn.geometry.empennage, &mut drawn_design);
                drawn.landing_gear.derived_main_gear = a.resolved.main_gear_placement;
                let layout = alas_geom::builder::AircraftBuilder::new(Some(drawn.geometry.clone()))
                    .build(Some(&drawn_design), false)
                    .ok()
                    .and_then(|plane| {
                        let frame = plane.mac_frame()?;
                        let body = plane.fuselages.first()?;
                        let start = body.xsecs.first()?.xyz_c[0];
                        let length = body.xsecs.last()?.xyz_c[0] - start;
                        let stations = drawn.landing_gear.resolved_station_positions(
                            start + length * drawn.mass_model.nlg_x_fraction,
                            frame.x_lemac_m + drawn.mass_model.mlg_x_fraction_mac * frame.chord_m,
                            start,
                            length,
                        );
                        let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / frame.chord_m;
                        let (box_le, box_chord) = plane.wings.first()?.carried_root();
                        let front_spar = alas_config::StructuresConfig::default()
                            .spar_chord_fractions
                            .iter()
                            .copied()
                            .fold(f64::INFINITY, f64::min);
                        let holds: Vec<Value> = drawn
                            .cabin
                            .cargo
                            .hold_compartments
                            .iter()
                            .map(|h| json!([h.name, format!("{:?}", h.deck), h.x_start_m, h.x_end_m]))
                            .collect();
                        Some(json!({
                            "box_le_m": box_le[0],
                            "box_chord_m": box_chord,
                            "box_gap_m": [box_le[0] + front_spar * box_chord, box_le[0] + box_chord],
                            "holds": holds,
                            "x_lemac_m": frame.x_lemac_m,
                            "mac_frame_m": frame.chord_m,
                            "h_tail_area_m2": plane.wings.get(1).map(|w| w.unfolded_area()),
                            "v_tail_area_m2": plane.wings.get(2).map(|w| w.unfolded_area()),
                            "main_gear_x_m": stations.main_gear_x_m,
                            "main_gear_pct_mac": pct(stations.x_mlg_m),
                            "cg_pct_mac": pct(a.resolved.cg_x_m),
                            "np_pct_mac": pct(a.resolved.x_neutral_point_m),
                        }))
                    });
                json!({
                    "takeoff_mass_kg": a.sized.takeoff_mass_kg,
                    "block_fuel_kg": a.sized.block_fuel_kg,
                    "objective_value": a.objective_value,
                    "cost": a.cost,
                    "hard_feasible": a.hard_feasible,
                    "hard_violation_sum": a.hard_violation_sum,
                    "oew_kg": a.sized.operating_empty_mass_kg,
                    "payload_kg": a.sized.payload_kg,
                    "usable_capacity_kg": a.sized.usable_capacity_kg,
                    "lift_to_drag": a.sized.lift_to_drag,
                    "reference_area_m2": arts.reference_area_m2,
                    "design_resolved": a.resolved.design.to_array().to_vec(),
                    "masses": serde_json::to_value(a.resolved.masses)?,
                    "x_wing_m": c.wing[0], "x_h_stab_m": c.h_stab[0], "x_gear_m": c.gear[0],
                    "x_propulsion_m": c.propulsion[0],
                    "cg_x_m": a.resolved.cg_x_m,
                    "x_neutral_point_m": a.resolved.x_neutral_point_m,
                    "mac_m": a.resolved.mac_m,
                    "static_margin": (a.resolved.x_neutral_point_m - a.resolved.cg_x_m) / a.resolved.mac_m,
                    "cruise_drag": drag,
                    "layout": layout,
                    "main_gear_translation_m": a.resolved.main_gear_placement.map(|p| p.translation_m),
                    "wall_s": started.elapsed().as_secs_f64(),
                    "residuals": a.residuals.iter().map(|r| json!({
                        "id": r.id, "actual": r.actual, "limit": r.limit, "unit": r.unit,
                        "raw": r.raw_residual, "violation": r.normalized_violation,
                        "role": format!("{:?}", r.role), "violated": r.violated(),
                    })).collect::<Vec<_>>(),
                })
            }
            Err(reason) => json!({ "error": format!("{reason:?}") }),
        };
        println!("{label}: {:.1} s", started.elapsed().as_secs_f64());
        report.insert(label.clone(), row);
    }
    fs::write(
        &args[4],
        serde_json::to_string_pretty(&Value::Object(report))?,
    )?;
    Ok(())
}
