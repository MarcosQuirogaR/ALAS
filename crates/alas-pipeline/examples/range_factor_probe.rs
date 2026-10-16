// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Breguet-factor decomposition probe (model side only).
//!
//! For each preset of a targets file (the kappa-lane `tow_kg` / `fuel_tank_kg`
//! / `delta_payload_kg` format) this builds the baseline full analysis, flies
//! the corner-C trip at the chart takeoff mass, and reports every factor of
//! the Breguet range equation at the flown mid-cruise point:
//!
//! - geometry and Reynolds state: wetted areas, lengths, Re, Cf, FF, Q and the
//!   drag of each parasite component, plus the build-up total and the drag
//!   table's own `CD0`;
//! - drag: `CD0`, lift-dependent viscous, induced + trim, wave, `L/D`, the
//!   equivalent skin-friction coefficient `Cfe = CD0 Sref / Swet`;
//! - propulsion: the deck TSFC at the thrust that balances the drag;
//! - the flown leg (cruise altitude, climb/descent footprints and fuel) and
//!   the Breguet range factor `V (L/D) / (g TSFC)`.
//!
//! No reference range is read here.
//!
//! Usage: `range_factor_probe --targets T.json --out OUT.json`
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]

use std::path::PathBuf;

use alas_aero::analysis::AeroAnalysis;
use alas_config::{presets, AlasConfig};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};
use alas_opt::mdo::PlannedTrips;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::quick_analysis::corners::CORNER_TOLERANCE_M;
use alas_prop::system::PropulsionRating;
use serde_json::{json, Value};

/// Switches of one run: the cruise ceiling (`route` as the report flies it,
/// `design` as a range search), the presets priced under the FAA flag
/// reserve rule, and optional drag-model overrides.
struct Options {
    ceiling: String,
    faa_for: Vec<String>,
    kappa: Option<f64>,
    margin: Option<f64>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("range_factor_probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut targets = None;
    let mut out = None;
    let mut only: Option<String> = None;
    let mut options = Options {
        ceiling: "route".to_owned(),
        faa_for: Vec::new(),
        kappa: None,
        margin: None,
    };
    let mut index = 0;
    while index < argv.len() {
        let value = argv
            .get(index + 1)
            .cloned()
            .ok_or_else(|| format!("{} needs a value", argv[index]))?;
        match argv[index].as_str() {
            "--targets" => targets = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--only" => only = Some(value),
            "--ceiling" => options.ceiling = value,
            "--kappa" => options.kappa = value.parse().ok(),
            "--margin" => options.margin = value.parse().ok(),
            "--faa-for" => options.faa_for = value.split(',').map(str::to_owned).collect(),
            other => return Err(format!("unknown argument {other}")),
        }
        index += 2;
    }
    let targets = targets.ok_or("--targets is required")?;
    let out = out.ok_or("--out is required")?;
    let text = std::fs::read_to_string(&targets).map_err(|e| e.to_string())?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let entries = doc
        .get("presets")
        .and_then(Value::as_array)
        .ok_or("presets array")?;
    let mut results = Vec::new();
    for entry in entries {
        let name = entry["preset"].as_str().ok_or("preset name")?;
        if only.as_deref().is_some_and(|wanted| wanted != name) {
            continue;
        }
        let tow = entry["tow_kg"].as_f64().ok_or("tow_kg")?;
        let tank = entry["fuel_tank_kg"].as_f64().ok_or("fuel_tank_kg")?;
        let delta = entry["delta_payload_kg"].as_f64().ok_or("delta")?;
        eprintln!("[range factor] {name}");
        results.push(one(name, tow, tank, delta, &options)?);
    }
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&json!({ "results": results })).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {}", out.display());
    Ok(())
}

fn one(name: &str, tow: f64, tank: f64, delta: f64, options: &Options) -> Result<Value, String> {
    let mut config =
        AlasConfig::from_value(&json!({ "preset": name })).map_err(|e| e.to_string())?;
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    if let Some(kappa) = options.kappa {
        config.drag_model.lift_dependent_viscous_fraction = kappa;
    }
    if let Some(margin) = options.margin {
        config.drag_model.viscous_margin = margin;
    }
    if options.faa_for.iter().any(|item| item == name) {
        config.fuel_policy.scheme = alas_config::FuelScheme::FaaFlagSupplemental;
    }
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let mut model = report_mission_model(&config, &report)?;
    if options.ceiling == "design" {
        model = model.for_range_search();
    }
    let taxi = model.taxi_fuel_flow_kg_s().map_err(|e| e.to_string())?
        * config.fuel_policy.taxi_time_min
        * 60.0;
    let fuel_c = tank - taxi;
    let fuel_b = fuel_c - delta;
    let price = |fuel: f64| -> Result<(f64, f64), String> {
        let corner = max_range_with_reserves(
            tow,
            fuel,
            &config.fuel_policy,
            &PlannedTrips(&model),
            CORNER_TOLERANCE_M,
        );
        match corner.status {
            RangeStatus::ModelFailed(reason) => Err(reason),
            _ => Ok((corner.range_m, corner.reserve_fuel_kg)),
        }
    };
    let (range_c, reserve_c) = price(fuel_c)?;
    let (range_b, reserve_b) = price(fuel_b)?;

    let planned =
        alas_opt::mdo::plan_for_mission(&model, tow, range_c).map_err(|e| e.to_string())?;
    let leg = planned.fly_trip(tow, range_c).map_err(|e| e.to_string())?;
    let artifacts = report.fuel.artifacts(&config, &report.design)?;
    let table = artifacts
        .drag
        .table()
        .ok_or("no native drag table")?
        .clone();

    // Reserve components at the corner-C landing mass: the diversion, the
    // final-reserve hold and the contingency the EASA-basic scheme builds.
    let landing = leg.end_mass_kg;
    let diversion = model
        .diversion(landing, config.fuel_policy.alternate_distance_nmi * 1852.0)
        .map(|leg| (leg.fuel_kg, leg.time_s))
        .map_err(|e| e.to_string())?;
    let hold_flow = model
        .holding_fuel_flow_kg_s(landing - diversion.0, model.holding_altitude_m)
        .map_err(|e| e.to_string())?;
    let hold_kg = hold_flow * config.fuel_policy.final_reserve_hold_min * 60.0;
    let contingency_kg = config.fuel_policy.contingency_trip_fraction * leg.leg.fuel_kg;

    // Mid-cruise mass: mean of the mass at top of climb and the mass at the
    // start of descent (the flown leg's end mass plus the descent burn is
    // not exposed; the end mass is used as a bracket, error <1 % of mass).
    let top_of_climb = tow - leg.climb_fuel_kg;
    let end = leg.end_mass_kg;
    let mid = 0.5 * (top_of_climb + end);
    let altitude = 0.5 * (leg.cruise_altitude_m + leg.final_cruise_altitude_m);
    let mach = model.cruise_mach;
    let atmosphere = alas_atmo::us1976_compute_values(altitude, model.isa_deviation_c);
    let tas = mach * atmosphere.speed_of_sound_m_s;
    let g = model.gravity_at(altitude);
    let s_ref = model.wing_area_m2;
    let q = 0.5 * atmosphere.density_kg_m3 * tas * tas;
    let weight = mid * g;
    let cl = weight / (q * s_ref);
    let cd = table.cd(cl, mach, altitude);
    let cd0 = table.cd0(mach, altitude);
    let cd_visc_lift = table.viscous_lift_cd(cl, cd0);
    let cd_induced = table.induced_cd(cl);
    let cd_wave = table.wave_cd(cl, mach);
    let drag_n = cd * q * s_ref;
    let flight = model
        .propulsion
        .flight_condition(altitude, tas, g, model.isa_deviation_c)
        .map_err(|e| e.to_string())?;
    let point = model
        .propulsion
        .at_thrust(flight, drag_n, PropulsionRating::Cruise)
        .map_err(|e| e.to_string())?;
    let tsfc_per_h = point.fuel_flow_kg_s / point.thrust_n * 3600.0 * g;
    let tsfc_mg_ns = point.fuel_flow_kg_s / point.thrust_n * 1.0e6;
    let range_factor_km = tas * (cl / cd) / (g * (point.fuel_flow_kg_s / point.thrust_n)) / 1000.0;

    // Specific-air-range grid at the design Mach: mass fractions of the chart
    // takeoff mass against altitudes around the flown level.
    let mut sar_grid = Vec::new();
    for dh_m in [-1000.0_f64, 0.0, 1000.0, 2000.0, 3000.0] {
        let h = altitude + dh_m;
        let atm = alas_atmo::us1976_compute_values(h, model.isa_deviation_c);
        let v = mach * atm.speed_of_sound_m_s;
        let gg = model.gravity_at(h);
        let qq = 0.5 * atm.density_kg_m3 * v * v;
        for fraction in [0.50_f64, 0.60, 0.70, 0.80, 0.90, 0.97] {
            let m = tow * fraction;
            let cl_g = m * gg / (qq * s_ref);
            let cd_g = table.cd(cl_g, mach, h);
            let drag = cd_g * qq * s_ref;
            let flight_g = model
                .propulsion
                .flight_condition(h, v, gg, model.isa_deviation_c)
                .map_err(|e| e.to_string())?;
            let row = match model
                .propulsion
                .at_thrust(flight_g, drag, PropulsionRating::Cruise)
            {
                Ok(p) => json!({
                    "h_m": h, "mass_kg": m, "cl": cl_g, "cd": cd_g, "ld": cl_g / cd_g,
                    "cd_wave": table.wave_cd(cl_g, mach),
                    "tsfc_per_h": p.fuel_flow_kg_s / p.thrust_n * 3600.0 * gg,
                    "sar_km_per_kg": v / p.fuel_flow_kg_s / 1000.0,
                    "rating_fraction": p.rating_fraction,
                }),
                Err(error) => json!({ "h_m": h, "mass_kg": m, "error": error.to_string() }),
            };
            sar_grid.push(row);
        }
    }

    // Component build-up on the report airplane (cross-check of the table).
    let aero = AeroAnalysis::new(
        &report.airplane,
        AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, report.design.sweep_deg),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(config.analysis.clone()),
    );
    let breakdown = aero.parasite_breakdown(mach, altitude, None, None);
    let swet: f64 = breakdown.components.iter().map(|c| c.wetted_area_m2).sum();
    let components: Vec<Value> = breakdown
        .components
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "swet_m2": c.wetted_area_m2,
                "re_length_m": c.reynolds_length_m,
                "re": c.reynolds,
                "cf": c.skin_friction,
                "ff": c.form_factor,
                "q": c.interference_factor,
                "cd": c.cd,
            })
        })
        .collect();
    let plane = &report.airplane;
    let fuselage_len_m = breakdown
        .components
        .iter()
        .find(|c| c.name.to_lowercase().contains("fus"))
        .map(|c| c.reynolds_length_m);
    let cfe = cd0 * s_ref / swet;
    let b = plane.b_ref;
    Ok(json!({
        "preset": name,
        "engine": model.propulsion.identity(),
        "model_cruise_altitude_m": model.cruise_altitude_m,
        "model_design_cruise_altitude_m": model.design_cruise_altitude_m,
        "model_cruise_mach": model.cruise_mach,
        "report_oew_kg": alas_pipeline::quick_analysis::report_oew_kg(&report),
        "design_mtow_kg": config.requirements.mtow_kg,
        "fuel_policy": format!("{:?}", config.fuel_policy),
        "reserve_parts": {"landing_mass_kg": landing, "diversion_kg": diversion.0, "diversion_time_s": diversion.1, "hold_kg": hold_kg, "hold_flow_kg_s": hold_flow, "contingency_kg": contingency_kg},
        "n_engines": model.propulsion.n_engines(),
        "tow_kg": tow,
        "tank_kg": tank,
        "delta_payload_kg": delta,
        "taxi_kg": taxi,
        "range_b_m": range_b, "reserve_b_kg": reserve_b,
        "range_c_m": range_c, "reserve_c_kg": reserve_c,
        "leg": {
            "trip_fuel_kg": leg.leg.fuel_kg,
            "time_s": leg.leg.time_s,
            "climb_fuel_kg": leg.climb_fuel_kg,
            "climb_footprint_m": leg.climb_footprint_m,
            "descent_footprint_m": leg.descent_footprint_m,
            "climb_time_s": leg.climb_time_s,
            "cruise_altitude_m": leg.cruise_altitude_m,
            "final_cruise_altitude_m": leg.final_cruise_altitude_m,
            "end_mass_kg": leg.end_mass_kg,
            "steps": leg.step_positions.len(),
            "flown_distance_m": leg.flown_distance_m,
        },
        "cruise_point": {
            "mid_mass_kg": mid, "altitude_m": altitude, "mach": mach, "tas_m_s": tas,
            "cl": cl, "cd": cd, "cd0_table": cd0, "cd0_buildup": breakdown.cd0,
            "cd0_components": breakdown.cd_components,
            "cd_viscous_lift": cd_visc_lift, "cd_induced_trim": cd_induced, "cd_wave": cd_wave,
            "l_over_d": cl / cd, "drag_n": drag_n,
            "thrust_n": point.thrust_n, "fuel_flow_kg_s": point.fuel_flow_kg_s,
            "rating_fraction": point.rating_fraction,
            "tsfc_per_h": tsfc_per_h, "tsfc_mg_per_ns": tsfc_mg_ns,
            "range_factor_km": range_factor_km,
            "cruise_distance_m": leg.flown_distance_m - leg.climb_footprint_m - leg.descent_footprint_m,
        },
        "geometry": {
            "s_ref_m2": s_ref, "b_m": b, "aspect_ratio": b * b / s_ref,
            "swet_total_m2": swet, "cfe": cfe,
            "fuselage_length_m": fuselage_len_m,
            "sweep_deg": report.design.sweep_deg,
            "mtow_cfg_kg": config.requirements.mtow_kg,
        },
        "parasite_components": components,
        "sar_grid": sar_grid,
        "viscous_margin": config.drag_model.viscous_margin,
        "kappa": config.drag_model.lift_dependent_viscous_fraction,
        "component_masses_kg": report.component_masses,
        "design_point": {"cl": report.design_point.cl, "cd": report.design_point.cd, "ld": report.design_point.l_over_d},
        "polar_fit": {"cd0": report.polar_fit.cd0, "k": report.polar_fit.k, "e": report.polar_fit.oswald_e},
    }))
}
