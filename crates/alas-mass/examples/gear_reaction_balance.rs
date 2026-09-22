// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why `min_nose_gear_load` and `nose_gear_strength` report a *negative*
//! nose-gear reaction on registered aircraft, named station by named station.
//!
//! The all-preset matrix rejects the ATR 72-600 (345 candidates), the
//! A380-800 (506) and the A320-200 on `min_nose_gear_load`, and the residual
//! table reports the constraint's `actual` as a **negative fraction of
//! weight**. A negative static nose reaction is not a marginal balance
//! finding: it means the model places the centre of gravity *aft of the
//! main-gear station*, so the aeroplane sits on its tail at every loading
//! state that is evaluated. This probe prints the three quantities that
//! decide that sign (the main-gear station, the nose-gear station and the
//! state centre of gravity) for every registered preset, so the finding can
//! be attributed to the station model or to the mass distribution instead of
//! being restated as a search rejection.
//!
//! Static reaction convention, the same one `alas_opt::envelope` applies:
//!
//! ```text
//! N_nose = m (x_mlg - x_cg) / (x_mlg - x_nlg)     [kg, positive down]
//! ```
//!
//! so `N_nose < 0` exactly when `x_cg > x_mlg`. The five loading states are
//! the ones `alas_opt::envelope::operational_loading_states` builds (bare
//! OEW, analysed zero fuel, 50 % and 10 % of the analysed fuel, analysed
//! take-off); they are reproduced here from the same lumped groups because
//! `alas-opt` sits above this crate and cannot be depended on from it.
//!
//! This is a **diagnostic**: it measures the delivered model, changes
//! nothing, and makes no claim that either the stations or the reaction
//! convention is validated against a real weight-and-balance manual.
//!
//! SI throughout: m, kg. `%MAC` is measured from the main wing's mean
//! aerodynamic chord leading edge, the same reference the balance residuals
//! use.
//!
//! Run: `cargo run --release -p alas-mass --example gear_reaction_balance \
//!        -- [structural|reference]`
//!
//! `structural` (the default) is the main-wing mass coordinate the product
//! search evaluates (`reference_mass_coordinates = false`); `reference` is
//! the frozen `alas/physics/mass.py`-compatible point. Only the main wing's
//! coordinate differs between them.

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_product_mass_analysis_with_groups, MassBreakdown, MassCoordinateModel, MassCoordinates,
    OEW_KEYS,
};
use alas_mass::stations::component_stations_with_gear;

/// Mass and longitudinal centre of gravity of the operating empty weight.
///
/// The same summation over the same canonical [`OEW_KEYS`] that
/// `alas_payload::oew::oew_and_cg` performs; reproduced here because
/// `alas-payload` sits above this crate.
fn oew_and_cg(masses: &MassBreakdown, coords: &MassCoordinates) -> (f64, f64) {
    let positions = coords.as_pairs();
    let mut total = 0.0;
    let mut moment = 0.0;
    for key in OEW_KEYS {
        let mass = masses.get(key).unwrap_or(0.0).max(0.0);
        if mass <= 0.0 {
            continue;
        }
        if let Some((_, xyz)) = positions.iter().find(|&&(name, _)| name == key) {
            total += mass;
            moment += mass * xyz[0];
        }
    }
    (total, if total > 0.0 { moment / total } else { 0.0 })
}

/// One evaluated loading state: name, mass and longitudinal centre of gravity.
struct State {
    name: &'static str,
    mass_kg: f64,
    cg_x_m: f64,
}

/// The same five states `alas_opt::envelope::operational_loading_states`
/// builds, from the same lumped groups.
fn loading_states(
    oew_mass: f64,
    oew_cg_x: f64,
    payload_mass: f64,
    payload_cg_x: f64,
    fuel_mass: f64,
    fuel_cg_x: f64,
    takeoff_cg_x: f64,
) -> Vec<State> {
    let mzfw_mass = oew_mass + payload_mass;
    let mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / mzfw_mass.max(1.0);
    let with_fuel = |fraction: f64, name: &'static str| {
        let fuel = fuel_mass.max(0.0) * fraction;
        let mass = mzfw_mass + fuel;
        let cg_x_m = if fuel > 0.0 {
            (mzfw_mass * mzfw_cg_x + fuel * fuel_cg_x) / mass.max(1.0)
        } else {
            mzfw_cg_x
        };
        State {
            name,
            mass_kg: mass,
            cg_x_m,
        }
    };
    vec![
        State {
            name: "operating_empty",
            mass_kg: oew_mass,
            cg_x_m: oew_cg_x,
        },
        State {
            name: "analyzed_zero_fuel",
            mass_kg: mzfw_mass,
            cg_x_m: mzfw_cg_x,
        },
        with_fuel(0.50, "mid_mission"),
        with_fuel(0.10, "reserve"),
        State {
            name: "analyzed_takeoff",
            mass_kg: mzfw_mass + fuel_mass.max(0.0),
            cg_x_m: takeoff_cg_x,
        },
    ]
}

fn main() {
    let selector = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "structural".to_owned());
    let use_reference = selector.eq_ignore_ascii_case("reference");
    println!(
        "main-wing mass coordinate: {}",
        if use_reference {
            "ReferenceCompatibility"
        } else {
            "StructuralWingbox (product search default)"
        }
    );
    for preset_name in presets::available() {
        println!("================ {preset_name} ================");
        let Ok(preset) = presets::get(preset_name) else {
            println!("preset lookup failed");
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": preset_name }))
        else {
            println!("config load failed");
            continue;
        };
        let airplane = match AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
        {
            Ok(airplane) => airplane,
            Err(error) => {
                println!("geometry build failed: {error:?}");
                continue;
            }
        };
        let Some(wing) = airplane.wings.iter().find(|wing| wing.name == "Main Wing") else {
            println!("no main wing");
            continue;
        };
        let stations = match component_stations_with_gear(
            &airplane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
            &config.landing_gear,
        ) {
            Ok(stations) => stations,
            Err(error) => {
                println!("station resolution failed: {error}");
                continue;
            }
        };
        let (masses, coords, cg) = match run_product_mass_analysis_with_groups(
            &airplane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&config.mass_model),
            None,
            if use_reference {
                MassCoordinateModel::ReferenceCompatibility
            } else {
                MassCoordinateModel::StructuralWingbox(&config.structures)
            },
            &config.landing_gear,
        ) {
            Ok((masses, coords, cg, _)) => (masses, coords, cg),
            Err(error) => {
                println!("mass analysis failed: {error}");
                continue;
            }
        };

        let mac = wing.mean_aerodynamic_chord();
        let mac_le_x = wing.aerodynamic_center(0.25)[0] - 0.25 * mac;
        let to_pct_mac = |x_m: f64| 100.0 * (x_m - mac_le_x) / mac;
        let x_nlg = stations.nose_gear.position_m[0];
        let x_mlg = stations.main_gear.position_m[0];
        let wheelbase = x_mlg - x_nlg;
        let (fus_start, fus_end) = airplane.fuselages.first().map_or((0.0, 0.0), |fuselage| {
            (
                fuselage.xsecs.first().map_or(0.0, |xsec| xsec.xyz_c[0]),
                fuselage.xsecs.last().map_or(0.0, |xsec| xsec.xyz_c[0]),
            )
        });

        println!(
            "fuselage: start {fus_start:.3} m, end {fus_end:.3} m, length {:.3} m | mac_le {mac_le_x:.3} m, mac {mac:.3} m",
            fus_end - fus_start
        );
        println!(
            "gear: x_nlg {x_nlg:.3} m ({:.1} %MAC, {:.4} of fuselage) [{}] | x_mlg {x_mlg:.3} m ({:.1} %MAC, {:.4} of fuselage) [{}] | wheelbase {wheelbase:.3} m",
            to_pct_mac(x_nlg),
            (x_nlg - fus_start) / (fus_end - fus_start).max(1e-9),
            stations.nose_gear.method,
            to_pct_mac(x_mlg),
            (x_mlg - fus_start) / (fus_end - fus_start).max(1e-9),
            stations.main_gear.method,
        );
        println!("group,mass_kg,x_m,pct_mac");
        for ((name, mass_kg), (_, xyz)) in masses.as_pairs().into_iter().zip(coords.as_pairs()) {
            println!(
                "{name},{mass_kg:.1},{:.3},{:.1}",
                xyz[0],
                to_pct_mac(xyz[0])
            );
        }

        let (oew_mass, oew_cg_x) = oew_and_cg(&masses, &coords);
        let states = loading_states(
            oew_mass,
            oew_cg_x,
            masses.payload,
            coords.payload[0],
            masses.fuel,
            coords.fuel[0],
            cg[0],
        );
        let floor = config.mass_model.pct_load_nlg_min;
        println!(
            "state,mass_kg,cg_x_m,cg_pct_mac,nose_load_kg,nose_fraction,floor,required_x_mlg_m"
        );
        for state in &states {
            let nose_load_kg = state.mass_kg * (x_mlg - state.cg_x_m) / wheelbase;
            // The main-gear station that would put this state exactly on the
            // floor, holding the nose-gear station and the centre of gravity
            // fixed: x_mlg = (x_cg - f x_nlg) / (1 - f).
            let required_x_mlg = (state.cg_x_m - floor * x_nlg) / (1.0 - floor);
            println!(
                "{},{:.1},{:.3},{:.2},{:.1},{:.5},{floor:.5},{required_x_mlg:.3}",
                state.name,
                state.mass_kg,
                state.cg_x_m,
                to_pct_mac(state.cg_x_m),
                nose_load_kg,
                nose_load_kg / state.mass_kg.max(1.0),
            );
        }
    }
}
