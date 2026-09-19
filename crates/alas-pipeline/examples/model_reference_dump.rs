// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dump every model output that has a real-world counterpart, per preset.
//!
//! Companion to the hand-researched reference datasets under
//! `.agent/validation/`. This writes `MODEL.json` in the same key structure so
//! the two can be merged into a correlation table. It is a scratch validation
//! instrument, not a shipped artifact.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Standalone fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::{presets, ActiveEngineModel, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemMeta};
use alas_perf::landing_gear::size_landing_gear_with_group_stations;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::gear_stations::resolved_gear_stations;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

/// Return the independent landing-gear anchors registered for a preset.
///
/// These values are deliberately kept beside the model-derived output rather
/// than fed into the sizing call.  A source wheelbase/track is a published
/// geometry reference with its own definition; it does not supply the datum
/// needed to move `x_nlg` or `x_mlg`.  The topology counts are likewise
/// exported as evidence metadata so the parity harness can distinguish an
/// aircraft reference from a preliminary ALAS layout.
fn source_gear_reference(name: &str) -> Value {
    match name {
        "A220-300" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_a220_acp_2025",
            "definition": "Airbus ACP nominal wheelbase from NLG axle to MLG axle; main-gear track",
            "wheelbase_m": 15.23238,
            "track_width_m": 6.731,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 38.68928,
            "longitudinal_stations_m": {"nlg": 3.401568, "mlg": [18.633948, 18.633948]},
            "station_source_definition": "Airbus A220 ACP DM BD500-A-J06-10-00AAA-030A-A Rev 2023-11-01, pp.150-156; dimensions originate at the geometric nose-tip drawing extension and are nominal (weight/CG can change wheelbase)",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 2,
            "mlg_wheels_per_strut": [2, 2],
            "main_wheels_total": 4,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum or attachment hardpoints not asserted",
        }),
        "A320-200" => json!({
            "status": "registered_independent_reference",
            "source_cite": "easa_tcds_a064_i12",
            "definition": "A320-family source wheelbase from NLG axle to MLG axle; main-gear track",
            "wheelbase_m": 12.64,
            "track_width_m": 7.59,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 37.57,
            "longitudinal_stations_m": {"nlg": 5.07, "mlg": [17.71, 17.71]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-004-A01; dimensions originate at the geometric nose-tip drawing extension",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 2,
            "mlg_wheels_per_strut": [2, 2],
            "main_wheels_total": 4,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        "A340-300" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_ac_a340_2025",
            "definition": "Airbus AC source wheelbase to the wing MLG bogie centre; wing-gear centreline track",
            "wheelbase_m": 25.375,
            "track_width_m": 10.684,
            "centerline_wheelbase_m": 26.372,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 63.66,
            "longitudinal_stations_m": {"nlg": 6.67, "mlg": [32.05, 32.05, 33.04]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-007-A01; AC 7-2-0 gives 25.375 m primary wheelbase and 26.372 m NLG-to-centreline wheelbase",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 3,
            "mlg_wheels_per_strut": [4, 4, 2],
            "main_wheels_total": 10,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        "A380-800" => json!({
            "status": "registered_independent_reference",
            "source_cite": "airbus_ac_a380_2025",
            "definition": "Airbus AC source wheelbase to the wing-gear axle reference; wing-gear centreline track and bogie topology; body-gear wheelbase is retained separately",
            "wheelbase_m": 28.61,
            "body_wheelbase_m": 31.88,
            "track_width_m": 14.34,
            "station_frame": "nose_tip_drawing_reference",
            "reference_fuselage_length_m": 72.73,
            "longitudinal_stations_m": {"nlg": 4.97, "mlg": [33.58, 33.58, 36.85, 36.85]},
            "station_source_definition": "Airbus AC 2-2-0 Figure 2-2-0-991-001-A01; 28.61 m is NLG-to-WLG and 31.88 m is NLG-to-BLG",
            "n_nlg_wheels": 2,
            "n_mlg_struts": 4,
            "mlg_wheels_per_strut": [4, 4, 6, 6],
            "main_wheels_total": 20,
            "absolute_station_status": "drawing stations present in source_frame; certified WBM/AFM datum not asserted",
        }),
        _ => json!({
            "status": "no_registered_independent_reference",
            "source_cite": Value::Null,
            "definition": "No source topology/position anchor is registered for this preset",
            "wheelbase_m": Value::Null,
            "track_width_m": Value::Null,
            "n_nlg_wheels": Value::Null,
            "n_mlg_struts": Value::Null,
            "mlg_wheels_per_strut": Value::Null,
            "main_wheels_total": Value::Null,
            "absolute_station_status": "unavailable",
        }),
    }
}

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

    // Keep the validation export tied to the same gear sizing inputs used by
    // the report/OpenVSP path. These are model-derived design coordinates,
    // not claims about an operator's certified landing-gear stations.
    let main_wing = report
        .airplane
        .wings
        .first()
        .ok_or_else(|| "model has no wing for gear sizing".to_owned())?;
    let c_ref = report.airplane.c_ref.max(0.001);
    let x_mac_le = main_wing.aerodynamic_center(0.25)[0] - 0.25 * c_ref;
    let aero_aft_x = report.x_neutral_point - config.requirements.target_static_margin * c_ref;
    let aero_fwd_x = aero_aft_x - config.requirements.cg_range_pct_mac / 100.0 * c_ref;
    let main_fuselage = report
        .airplane
        .fuselages
        .first()
        .ok_or_else(|| "model has no fuselage for gear sizing".to_owned())?;
    let fuselage_start_x = main_fuselage
        .xsecs
        .first()
        .map_or(0.0, |section| section.xyz_c[0]);
    let fuselage_end_x = main_fuselage
        .xsecs
        .last()
        .map_or(fuselage_start_x, |section| section.xyz_c[0]);
    let fallback_x_nlg =
        fuselage_start_x + (fuselage_end_x - fuselage_start_x) * config.mass_model.nlg_x_fraction;
    let fallback_x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * c_ref;
    // Resolved through the shared gate, not rebuilt here: a diagnostic export
    // that invented a main-gear station the mass model refuses would be the
    // one artifact a reviewer trusts to show what the model actually holds.
    let gear_stations = resolved_gear_stations(
        &config,
        &report.airplane,
        fallback_x_nlg,
        fallback_x_mlg,
        fuselage_start_x,
        fuselage_end_x - fuselage_start_x,
    )
    .map_err(|refusal| format!("main_gear_station_not_measured: {refusal}"))?;
    let gear_mass_kg: f64 = report.component_masses.values().copied().sum();
    let gear_layout = size_landing_gear_with_group_stations(
        gear_mass_kg,
        gear_stations.x_nlg_m,
        gear_stations.x_mlg_m,
        aero_fwd_x,
        aero_aft_x,
        config.geometry.fuselage.diameter_m,
        config.geometry.fuselage.diameter_m * 1.1,
        &gear_stations.main_gear_x_m,
        &config.landing_gear,
    );

    let centerline_wheelbase_m = (gear_layout.main_gear_x_m.len() == 3)
        .then(|| gear_layout.main_gear_x_m[2] - gear_layout.x_nlg);
    let body_wheelbase_m = (gear_layout.main_gear_x_m.len() >= 4).then(|| {
        let last = gear_layout.main_gear_x_m.len();
        (gear_layout.main_gear_x_m[last - 2] + gear_layout.main_gear_x_m[last - 1]) / 2.0
            - gear_layout.x_nlg
    });

    // Keep the source topology order visible in the model export.  This is
    // intentionally derived from the actual generated wheel objects and is
    // not replaced with the source list below.
    let model_mlg_wheels_per_strut = gear_layout.mlg_wheels_per_strut.clone();

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
        "wheelbase_m": gear_layout.wheelbase_m,
        "wheel_track_m": gear_layout.track_width_m,
        "centerline_wheelbase_m": centerline_wheelbase_m,
        "body_wheelbase_m": body_wheelbase_m,
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
    let component_centroids_m: BTreeMap<String, [f64; 3]> = report
        .mass_coordinates
        .iter()
        .map(|(name, coordinate)| (name.clone(), *coordinate))
        .collect();
    let component_first_moments_kg_m: BTreeMap<String, [f64; 3]> = masses
        .iter()
        .filter_map(|(name, value)| {
            report.mass_coordinates.get(name).map(|coordinate| {
                let physical_mass = (*value).max(0.0);
                (
                    name.clone(),
                    [
                        physical_mass * coordinate[0],
                        physical_mass * coordinate[1],
                        physical_mass * coordinate[2],
                    ],
                )
            })
        })
        .collect();
    let mass = json!({
        "mtow_kg": config.requirements.mtow_kg,
        "oew_kg": oew,
        "component_masses_kg": masses.iter().map(|(k, v)| (k.clone(), *v)).collect::<BTreeMap<_, _>>(),
        "component_centroids_m": component_centroids_m,
        "component_first_moments_kg_m": component_first_moments_kg_m,
        "first_moment_frame": "aircraft nose-relative station; x forward, y right, z up; model estimates",
        "wing_loading_kg_m2": config.requirements.mtow_kg / s_ref,
    });

    let cg = json!({
        "coordinate_frame": "aircraft nose-relative station, m; x forward, y right, z up",
        "model_lemac_from_nose_m": x_lemac,
        "model_mac_m": mac,
        "physical_cg_x_m": report.physical_cg[0],
        "physical_cg_y_m": report.physical_cg[1],
        "physical_cg_z_m": report.physical_cg[2],
        "model_loaded_percent_mac": 100.0 * (report.physical_cg[0] - x_lemac) / mac,
        "source_planning_frame": null,
        "source_planning_frame_status": "unavailable unless a source LEMAC and MAC frame are registered",
    });

    let gear = json!({
        "coordinate_frame": "aircraft nose-relative station, m; x forward, y right, z up",
        "n_nlg_wheels": gear_layout.n_nlg_wheels,
        "n_mlg_struts": gear_layout.n_mlg_struts,
        "wheels_per_mlg_strut": gear_layout.wheels_per_mlg_strut,
        "mlg_wheels_per_strut": model_mlg_wheels_per_strut,
        "main_wheels_total": gear_layout
            .wheels
            .iter()
            .filter(|wheel| wheel.group == "MLG")
            .count(),
        "x_nlg_m": gear_layout.x_nlg,
        "x_mlg_m": gear_layout.x_mlg,
        "main_gear_x_m": gear_layout.main_gear_x_m,
        "main_gear_station_frame": config
            .landing_gear
            .reference_station_frame
            .as_deref()
            .unwrap_or("aircraft_nose_relative_model"),
        "stations_source_scaled": gear_stations.source_scaled,
        "wheelbase_m": gear_layout.wheelbase_m,
        "centerline_wheelbase_m": centerline_wheelbase_m,
        "body_wheelbase_m": body_wheelbase_m,
        "track_width_m": gear_layout.track_width_m,
        "model_reference_geometry": {
            "reference_wheelbase_m": gear_layout.reference_wheelbase_m,
            "reference_body_wheelbase_m": gear_layout.reference_body_wheelbase_m,
            "reference_track_width_m": gear_layout.reference_track_m,
            "status": "normalized source station anchors scale the active geometry; source scalar wheelbase/track remain comparison metadata and loads still use the actual model layout",
        },
        "nlg_tire_class": gear_layout.nlg_tire.code,
        "mlg_tire_class": gear_layout.mlg_tire.code,
        "strut_material": gear_layout.strut_material,
        "design_nlg_reaction_kg": gear_layout.r_nlg_design_kg,
        "design_mlg_reaction_kg": gear_layout.r_mlg_total_design_kg,
        "turnover_angle_deg": gear_layout.turnover_angle_deg,
        "turnover_ok": gear_layout.turnover_ok,
        "positions": gear_layout
            .wheels
            .iter()
            .map(|wheel| {
                json!({
                    "group": wheel.group,
                    "strut": wheel.strut_label,
                    "x_m": wheel.x,
                    "y_m": wheel.y,
                    "diameter_m": wheel.diameter_m,
                    "width_m": wheel.width_m,
                })
            })
            .collect::<Vec<_>>(),
        "source_status": "model-sized preliminary layout; normalized Airbus drawing anchors are present for A220/A320/A340/A380, while sizing loads remain preliminary",
        "source_reference": source_gear_reference(name),
    });

    // ---- cabin / payload -------------------------------------------------
    let layout = build_payload_layout(&plane, &config, oew, 0.0).map_err(|e| format!("{e:?}"))?;
    let seat_capacity = match &layout.summary {
        alas_payload::LayoutSummary::Passenger(summary) => json!({
            "geometry_exit_limit": summary.geometric_capacity,
            "effective_limit": summary.max_certifiable_capacity,
            "source_certified_cap": summary.source_capacity_cap,
            "source_exit_layout": summary.source_exit_layout,
            "binding": summary.capacity_binding,
        }),
        alas_payload::LayoutSummary::Cargo(_) => Value::Null,
    };
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
        "seat_capacity": seat_capacity,
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
        "cruise_aoa_deg": report.design_point.alpha_deg,
        "trimmed_cruise_aoa_deg": report.trimmed_design_point.as_ref().map(|point| point.alpha_deg),
        "trimmed_geometric_body_aoa_deg": report
            .trimmed_design_point
            .as_ref()
            .map(|point| point.geometric_body_alpha_deg),
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
    let mut pr = payload_range(&config, &report, oew, s_ref);
    if let Some(payload_range) = pr.as_mut().and_then(Value::as_object_mut) {
        let capacity_kg = payload_range
            .get("fuel_capacity_kg")
            .and_then(Value::as_f64);
        let density_kg_l = preset
            .reference
            .fuel_density_kg_l
            .or_else(|| Some(config.mass_model.fuel_density_kg_m3 / 1000.0));
        let capacity_l = preset.reference.usable_fuel_volume_l.or_else(|| {
            capacity_kg
                .zip(density_kg_l)
                .filter(|(_, density)| density.is_finite() && *density > 0.0)
                .map(|(mass, density)| mass / density)
        });
        payload_range.insert(
            "fuel_capacity_l".to_owned(),
            capacity_l.map_or(Value::Null, |value| json!(value)),
        );
        payload_range.insert(
            "fuel_capacity_density_kg_l".to_owned(),
            density_kg_l.map_or(Value::Null, |value| json!(value)),
        );
        payload_range.insert(
            "fuel_capacity_l_basis".to_owned(),
            json!(if preset.reference.usable_fuel_volume_l.is_some() {
                "registered preset reference volume"
            } else {
                "capacity mass divided by configured density"
            }),
        );
    }

    // ---- propulsion ------------------------------------------------------
    let propulsion = match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => json!({
            "engine_model": config.geometry.engine.engine_name,
            "kind": "turbofan",
            "sls_thrust_kn": spec.rated_thrust_kn,
            "config_thrust_kn": config.geometry.engine.thrust_kn(),
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
        "cg": cg,
        "gear": gear,
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
