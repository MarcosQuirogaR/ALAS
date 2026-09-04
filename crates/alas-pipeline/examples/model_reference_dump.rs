// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dump every model output that has a real-world counterpart, per preset.
//!
//! Companion to the hand-researched reference datasets under
//! `.agent/validation/`. This writes `MODEL.json` in the same key structure so
//! the two can be merged into a correlation table. It is a scratch validation
//! instrument, not a shipped artifact.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::{presets, ActiveEngineModel, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemMeta};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| ".agent/validation/MODEL.json".to_owned()),
    );
    let mut all = serde_json::Map::new();
    for name in presets::available() {
        eprintln!("[model dump] {name}");
        match dump(name) {
            Ok(value) => {
                all.insert(name.to_owned(), value);
            }
            Err(error) => {
                eprintln!("  failed: {error}");
                all.insert(name.to_owned(), json!({ "error": error }));
            }
        }
    }
    let document = Value::Object(all);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&document).unwrap()).unwrap();
    println!("wrote {}", out.display());
    if std::env::var("ALAS_MESH_STUDY").is_ok() {
        for name in presets::available() {
            match mesh_study(name) {
                Ok(v) => println!("{}", serde_json::to_string(&v).unwrap()),
                Err(e) => println!("{{\"preset\":\"{name}\",\"error\":\"{e}\"}}"),
            }
        }
    }
}

fn dump(name: &str) -> Result<Value, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let config = AlasConfig::from_value(&json!({ "preset": name })).map_err(|e| e.to_string())?;
    let dv = &preset.design_vector;

    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(dv), true)
        .map_err(|e| format!("{e:?}"))?;
    let report = FullAnalysis::new(config.clone())
        .run(dv, false)
        .map_err(|e| e.to_string())?;

    // ---- geometry -------------------------------------------------------
    let pf = config
        .geometry
        .wing
        .transport_planform(dv)
        .map_err(|e| e.to_string())?;
    let semi = dv.span_m / 2.0;
    let (yk, cr, cb, ct) = (
        pf.kink.y_m,
        pf.root.chord_m,
        pf.kink.chord_m,
        pf.tip.chord_m,
    );
    let (mut a, mut ic, mut ix) = (0.0, 0.0, 0.0);
    // Integrate every active panel, including the clipped side-of-body
    // station. The certified reference MAC can use a different planform.
    for panel in pf.panels() {
        let p = panel.inboard;
        let q = panel.outboard;
        let dy = q.y_m - p.y_m;
        let area = dy * (p.chord_m + q.chord_m) / 2.0;
        a += area;
        ic += dy * (p.chord_m.powi(2) + p.chord_m * q.chord_m + q.chord_m.powi(2)) / 3.0;
        ix += p.leading_edge_x_m * area
            + (q.leading_edge_x_m - p.leading_edge_x_m) * dy * (p.chord_m + 2.0 * q.chord_m) / 6.0;
    }
    let mac = ic / a;
    let x_lemac = config.geometry.wing.root_datum_x_m + dv.wing_x_shift_m + ix / a;
    let sweep_c4 = ((pf.tip.leading_edge_x_m + 0.25 * ct - pf.kink.leading_edge_x_m - 0.25 * cb)
        / (semi - yk))
        .atan()
        .to_degrees();
    let emp = &config.geometry.empennage;
    let s_ref = report
        .geometry_summary
        .get("reference_area_m2")
        .copied()
        .unwrap_or(f64::NAN);
    let geometry = json!({
        "span_m": dv.span_m,
        "wing_area_m2": s_ref,
        "aspect_ratio": dv.span_m * dv.span_m / s_ref,
        "mac_m": mac,
        "lemac_from_nose_m": x_lemac,
        "sweep_le_deg": dv.sweep_deg,
        "sweep_c4_deg": sweep_c4,
        "taper_ratio": ct / cr,
        "root_chord_m": cr,
        "kink_chord_m": cb,
        "tip_chord_m": ct,
        "kink_span_fraction": yk / semi,
        "fuselage_length_m": dv.fuselage_length_m,
        "fuselage_width_m": config.geometry.fuselage.diameter_m,
        "fuselage_height_m": config.geometry.fuselage.height_m
            .unwrap_or(config.geometry.fuselage.diameter_m),
        "htail_area_m2": (emp.hstab_root_chord_m + emp.hstab_tip_chord_m) * emp.hstab_tip_le_m.1,
        "htail_span_m": 2.0 * emp.hstab_tip_le_m.1,
        "vtail_area_m2": (emp.vstab_root_chord_m + emp.vstab_tip_chord_m) / 2.0 * emp.vstab_tip_le_m.2,
        "vtail_height_m": emp.vstab_tip_le_m.2,
        "engine_spanwise_position_m": config.geometry.engine.spanwise_positions_m.first().copied(),
        "nacelle_length_m": config.geometry.engine.nacelle_profile.last().map(|p| p.0),
        "nacelle_max_diameter_m": 2.0 * config.geometry.engine.radius_scale_m,
    });

    // ---- mass -----------------------------------------------------------
    let masses = &report.component_masses;
    let oew: f64 = [
        "Wing",
        "H-Stab",
        "V-Stab",
        "Fuselage",
        "Gear",
        "Propulsion",
        "Systems",
        "Furnishings",
    ]
    .iter()
    .map(|k| masses.get(*k).copied().unwrap_or(0.0))
    .sum();
    let mass = json!({
        "mtow_kg": config.requirements.mtow_kg,
        "oew_kg": oew,
        "component_masses_kg": masses.iter().map(|(k, v)| (k.clone(), *v)).collect::<BTreeMap<_, _>>(),
        "wing_loading_kg_m2": config.requirements.mtow_kg / s_ref,
    });

    // ---- cabin / payload -------------------------------------------------
    let layout = build_payload_layout(&plane, &config, oew, 0.0).map_err(|e| format!("{e:?}"))?;
    let mut seats = 0i64;
    let mut containers: Vec<(f64, f64, &str, f64)> = Vec::new();
    let mut bulk_mass = 0.0;
    for item in &layout.items {
        match &item.meta {
            ItemMeta::Seat(s) => seats += s.filled,
            ItemMeta::Container(c) => containers.push((item.x, item.y, c.uld, item.mass)),
            ItemMeta::BulkBag => bulk_mass += item.mass,
            _ => {}
        }
    }
    let wing_box = plane
        .wings
        .first()
        .map(|w| w.aerodynamic_center(0.25)[0])
        .unwrap_or(f64::NAN);
    let fwd: Vec<_> = containers.iter().filter(|c| c.0 < wing_box).collect();
    let aft: Vec<_> = containers.iter().filter(|c| c.0 >= wing_box).collect();
    let distinct = |v: &[&(f64, f64, &str, f64)], f: fn(&(f64, f64, &str, f64)) -> f64| {
        let mut x: Vec<f64> = v.iter().map(|c| f(c)).collect();
        x.sort_by(f64::total_cmp);
        x.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        x.len()
    };
    let uld_volume = |code: &str| match code {
        "AKE" | "AKC" => 4.5,
        "AKH" => 3.6,
        "DPE" => 3.5,
        "ALF" => 9.1,
        _ => 0.0,
    };
    let container_volume: f64 = containers.iter().map(|c| uld_volume(c.2)).sum();
    let cabin_geometry = alas_payload::CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .map_err(|error| format!("{error:?}"))?;
    let cargo_manager = alas_payload::CargoLoadManager::new(
        &cabin_geometry,
        alas_config::CargoDeckConfig {
            use_main_deck: false,
            ..config.cabin.cargo.clone()
        },
    );
    let capacity = cargo_manager.capacity_summary();
    let cabin_payload = json!({
        "seats_modelled": seats,
        "seats_requested": config.requirements.num_passengers,
        "container_positions_total": containers.len(),
        "available_uld_positions": capacity.uld_positions,
        "available_bulk_positions": capacity.bulk_positions,
        "available_container_internal_volume_m3": capacity.container_internal_volume_m3,
        "available_bulk_nominal_volume_m3": capacity.bulk_nominal_volume_m3,
        "cargo_hold_volume_m3": null,
        "cargo_hold_volume_status": "unsupported: slot internal volume is not total usable hold volume; main-deck baggage compartments are not modelled",
        "container_positions_fwd": fwd.len(),
        "container_positions_aft": aft.len(),
        "fwd_rows": distinct(&fwd, |c| c.0),
        "fwd_abreast": distinct(&fwd, |c| c.1),
        "aft_rows": distinct(&aft, |c| c.0),
        "aft_abreast": distinct(&aft, |c| c.1),
        "uld_code": containers.first().map(|c| c.2),
        "container_volume_m3": container_volume,
        "bulk_block_mass_kg": bulk_mass,
        "payload_total_kg": layout.total_mass,
        "payload_cg_x_m": layout.cg_x,
    });

    // ---- aerodynamics ----------------------------------------------------
    let atmo = Atmosphere::new(config.requirements.cruise_altitude_m);
    let sweep = report.design.sweep_deg;
    // Same mesh the full analysis uses, otherwise the polar is not the one
    // the reported design point came from.
    let mut fine = config.analysis.clone();
    fine.spanwise_resolution = fine.fine_spanwise_resolution;
    fine.chordwise_resolution = fine.fine_chordwise_resolution;
    let cruise_sweep = AeroAnalysis::new(
        &report.airplane,
        sweep,
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(fine.clone()),
    )
    .run_sweep(
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
    )
    .map_err(|e| e.to_string())?;
    let lin = |x: &[f64], y: &[f64]| {
        let n = x.len() as f64;
        let mx = x.iter().sum::<f64>() / n;
        let my = y.iter().sum::<f64>() / n;
        let num: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
        let den: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
        (num / den, my - num / den * mx)
    };
    let (cl_alpha_deg, cl_intercept) = lin(&cruise_sweep.geometric_alpha_deg, &cruise_sweep.cl);
    // `PolarSweep::alpha_deg` is the Prandtl-Glauert *relabelled* reporting
    // axis; the coefficients are not recomputed. Report the slope on both axes
    // so a compressibility correction applied by relabelling can be told apart
    // from one applied to the coefficients.
    let (cl_alpha_deg_reported, cl_intercept_reported) =
        lin(&cruise_sweep.alpha_deg, &cruise_sweep.cl);
    let best_ld = cruise_sweep
        .l_over_d
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let aerodynamics = json!({
        "cruise_mach": config.requirements.cruise_mach,
        "cruise_altitude_m": config.requirements.cruise_altitude_m,
        "cl_cruise": report.design_point.cl,
        "cd_cruise": report.design_point.cd,
        "l_over_d_cruise": report.design_point.l_over_d,
        "l_over_d_max_in_sweep": best_ld,
        "cd0_cruise": report.polar_fit.cd0,
        "induced_drag_k": report.polar_fit.k,
        "oswald_e": report.polar_fit.oswald_e,
        "cl_alpha_per_deg": cl_alpha_deg,
        "cl_alpha_per_rad": cl_alpha_deg * 180.0 / std::f64::consts::PI,
        "cl_alpha_per_rad_reported_axis": cl_alpha_deg_reported * 180.0 / std::f64::consts::PI,
        "alpha_zero_lift_deg": -cl_intercept / cl_alpha_deg,
        "alpha_zero_lift_deg_reported_axis": -cl_intercept_reported / cl_alpha_deg_reported,
        "cruise_polar_reported_alpha_deg": cruise_sweep.alpha_deg,
        "static_margin": report.static_margin,
        "neutral_point_x_m": report.x_neutral_point,
        "cruise_polar_alpha_deg": cruise_sweep.geometric_alpha_deg,
        "cruise_polar_cl": cruise_sweep.cl,
        "cruise_polar_cd": cruise_sweep.cd,
        "cruise_polar_l_over_d": cruise_sweep.l_over_d,
    });

    // ---- payload / range -------------------------------------------------
    // Mirrors `alas_report::families::performance::payload_range_data`, kept
    // local so this instrument does not depend on the report crate compiling.
    let pr = payload_range(&config, &report, oew, s_ref);

    // ---- propulsion ------------------------------------------------------
    let propulsion = match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => json!({
            "engine_model": config.geometry.engine.engine_name,
            "kind": "turbofan",
            "sls_thrust_kn": spec.rated_thrust_kn,
            "config_thrust_kn": config.geometry.engine.thrust_kn,
            "cruise_reference_thrust_kn": spec.off_design.cruise_reference_thrust_n / 1000.0,
            "cruise_reference_altitude_m": spec.off_design.cruise_reference_altitude_m,
            "cruise_reference_mach": spec.off_design.cruise_reference_mach,
            "cruise_reference_evidence": spec.off_design.evidence,
            "cruise_tsfc_kg_kgf_h": spec.cruise_tsfc_kg_kgf_hr,
            "bypass_ratio": spec.bypass_ratio,
            "takeoff_bypass_ratio": spec.takeoff_bypass_ratio,
            "catalogue_lto_overall_pressure_ratio": alas_config::engines::get(&config.geometry.engine.engine_name)
                .ok().and_then(|catalogue| catalogue.lto_overall_pressure_ratio),
            "overall_pressure_ratio": spec.overall_pressure_ratio,
            "fan_diameter_m": config.geometry.engine.fan_diameter_m,
            "n_engines": config.geometry.engine.spanwise_positions_m.len(),
        }),
        Ok(ActiveEngineModel::Turboprop(spec)) => json!({
            "engine_model": config.geometry.engine.engine_name,
            "kind": "turboprop",
            "takeoff_shaft_power_kw": spec.takeoff_shaft_power_kw,
            "max_cruise_shaft_power_kw": spec.maximum_cruise_shaft_power_kw,
            "max_cruise_fuel_flow_kg_h": spec.maximum_cruise_fuel_flow_kg_h,
            "propeller_diameter_m": spec.propeller_diameter_m,
            "n_engines": config.geometry.engine.spanwise_positions_m.len(),
        }),
        Err(error) => json!({ "error": error.to_string() }),
    };

    // ---- structures ------------------------------------------------------
    let structures = json!({
        "skin_material": config.structures.skin_material,
        "spar_web_material": config.structures.spar_web_material,
        "spar_cap_material": config.structures.spar_cap_material,
        "rib_material": config.structures.rib_material,
    });

    // ---- mission ---------------------------------------------------------
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
        ..Default::default()
    };
    let mission = match DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
    {
        Err(error) => json!({"status": "pipeline_failed", "error": error.to_string()}),
        Ok(result) => {
            let mut summary = json!({"status": "route_unavailable"});
            if let Some(route) = result.route.as_ref() {
                summary = json!({
                    "status": "route_planned",
                    "departure": route.origin_airport.as_ref().map(|a| &a.icao)
                        .or_else(|| alas_config::airports::get(&config.departure_airport).ok().map(|a| &a.icao)),
                    "arrival": route.dest_airport.as_ref().map(|a| &a.icao)
                        .or_else(|| alas_config::airports::get(&config.arrival_airport).ok().map(|a| &a.icao)),
                    "route_source": route.source.as_str(),
                    "planned_route_km": route.total_distance_m() / 1000.0,
                    "route_waypoints": route.waypoints.len(),
                    "mission_status": "unavailable",
                });
            }
            if let Some(m) = result.mission_result.as_ref() {
                let mut cruise = None;
                for seg in &m.segments {
                    if format!("{:?}", seg.spec.kind)
                        .to_lowercase()
                        .contains("cruise")
                    {
                        let c = &seg.conditions;
                        let mid = c.altitude_m.len() / 2;
                        if !c.altitude_m.is_empty() {
                            cruise = Some(json!({
                                "altitude_m": c.altitude_m[mid],
                                "flight_level": c.altitude_m[mid] / 30.48,
                                "mach": c.mach[mid],
                                "tas_m_s": c.velocity_m_s[mid],
                                "mass_kg": c.total_mass_kg[mid],
                                "cl": c.lift_coefficient[mid],
                                "cd": c.drag_coefficient[mid],
                                "l_over_d": c.lift_coefficient[mid] / c.drag_coefficient[mid],
                                "thrust_n": c.thrust[mid].thrust_n,
                                "fuel_flow_kg_h": c.vehicle_mass_rate_kg_s[mid] * 3600.0,
                                "throttle": c.throttle[mid],
                            }));
                        }
                    }
                }
                let last = m.segments.last();
                let telemetry = json!({
                    "mission_status": if m.segments.len() == m.scheduled_segment_count {"completed"} else {"partial"},
                    "segments_flown": m.segments.len(),
                    "segments_scheduled": m.scheduled_segment_count,
                    "distance_flown_km": last.and_then(|s| s.conditions.aircraft_range_m.last()).map(|v| v / 1000.0),
                    "block_time_min": last.and_then(|s| s.conditions.time_s.last()).map(|v| v / 60.0),
                    "trip_fuel_kg": m.segments.first().and_then(|s| s.conditions.total_mass_kg.first().copied())
                        .zip(last.and_then(|s| s.conditions.total_mass_kg.last().copied()))
                        .map(|(a, b)| a - b),
                    "final_cruise": cruise,
                });
                if let (Some(target), Some(fields)) =
                    (summary.as_object_mut(), telemetry.as_object())
                {
                    target.extend(fields.clone());
                }
            }
            summary
        }
    };

    Ok(json!({
        "identity": {
            "preset": name,
            "model": preset.identity.model,
            "weight_variant": preset.identity.weight_variant,
            "engine_model": preset.identity.engine_model,
        },
        "geometry": geometry,
        "mass": mass,
        "cabin_payload": cabin_payload,
        "aerodynamics": aerodynamics,
        "payload_range": pr,
        "propulsion": propulsion,
        "structures": structures,
        "route": mission,
        "atmosphere_cruise": {
            "density_kg_m3": atmo.density(),
            "speed_of_sound_m_s": atmo.speed_of_sound(),
        },
    }))
}

include!("model_reference_dump_parts/part_01.rs");
