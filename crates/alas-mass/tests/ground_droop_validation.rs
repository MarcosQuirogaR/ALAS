// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The computed static ground droop of the registered wings, beside the
//! published airport-planning evidence.
//!
//! This is VALIDATION, not calibration: nothing here is tuned to the tables.
//! The test asserts physical sanity (sign, monotonic growth outboard, a droop
//! of a few percent of the semispan at most, heavier fuel droops more) and
//! prints the comparison with `--nocapture`:
//!
//! `cargo test -p alas-mass --release --test ground_droop_validation -- --nocapture`
//!
//! Evidence (Airbus A380 AC Rev 20, 2-2-0 drawing against 2-3-0 table):
//! the drawn (unloaded) wing stands 1.41 m (tip fence), 0.41 m (outboard
//! nacelle N2) and 0.06 m (inboard nacelle N1) above the MRW table. The table's
//! own light-versus-MRW sensitivity (300 t aft CG against MRW aft CG) raises
//! the tip fence by 0.73 m, N2 by 0.37 m and N1 by 0.22 m, of which 0.21 m is
//! the fuselage rise. Airbus A380 Facts and Figures (2022), p.4: "During
//! take-off the wing will flex upwards by over 4m" (ground droop plus 1 g rise,
//! one `EI(y)`): the model's own sum is printed beside it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::{presets, AlasConfig, WingShape};
use alas_geom::builder::AircraftBuilder;
use alas_mass::ground_clearance::ground_clearances;
use alas_mass::stations::ground_plane_z_m;
use alas_mass::wing_reconciliation::{
    complete_wing_mass_kg, one_g_tip_rise_m, wing_ground_deflection, GroundCondition,
    WingGroundResponse,
};

struct Case {
    config: AlasConfig,
    design: alas_config::design_variables::DesignVector,
    plane: alas_geom::aircraft::airplane::Airplane,
    complete_wing_kg: f64,
}

fn case(preset: &str) -> Case {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
    let design = presets::get(preset).unwrap().design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build_shape(Some(&design), true, WingShape::Flight)
        .unwrap();
    let complete_wing_kg = complete_wing_mass_kg(&config, &plane).unwrap();
    Case {
        config,
        design,
        plane,
        complete_wing_kg,
    }
}

fn response(case: &Case, condition: GroundCondition) -> WingGroundResponse {
    wing_ground_deflection(
        &case.config,
        &case.design,
        &case.plane,
        case.complete_wing_kg,
        condition,
    )
    .unwrap()
}

fn print_case(name: &str, case: &Case, label: &str, resp: &WingGroundResponse) {
    let ground_z = ground_plane_z_m(
        &case.plane.fuselages[0],
        &case.config.geometry,
        &case.config.landing_gear,
    );
    let clear = ground_clearances(&case.plane, ground_z, &resp.deflection).unwrap();
    let nacelles: Vec<String> = clear
        .nacelles
        .iter()
        .map(|n| {
            format!(
                "y={:.2} unloaded {:.3} droop {:.3}",
                n.y_m, n.unloaded_m, n.droop_m
            )
        })
        .collect();
    println!(
        "{name} {label}: support {:.2} m, box {:.0} kg, non-box {:.0}, fuel {:.0}, engines {:.0} (semi); \
         EI root {:.3e} tip {:.3e}; tip y={:.2} droop {:.3} m ({:.2}% semispan); {}",
        resp.support_y_m,
        resp.box_kg,
        resp.non_box_kg,
        resp.fuel_kg,
        resp.engines_kg,
        resp.ei_nm2[0],
        resp.ei_nm2[resp.ei_nm2.len() - 1],
        clear.tip_y_m,
        clear.tip_droop_m,
        100.0 * clear.tip_droop_m / clear.tip_y_m,
        nacelles.join("; ")
    );
}

#[test]
fn the_registered_wings_droop_physically_on_the_ground() {
    for name in [
        "A380-800", "A340-300", "A320-200", "A220-300", "B787-9", "B747-400",
    ] {
        let case = case(name);
        let mtow = case.config.requirements.mtow_kg;
        let heavy = response(&case, GroundCondition::at_rest(mtow));
        print_case(name, &case, "MTOW", &heavy);
        let declared_rise = case
            .config
            .geometry
            .wing
            .flight_tip_rise_m(*heavy.deflection.y_m.last().unwrap())
            .unwrap();
        println!(
            "{name} declared ground-to-flight tip rise of the preset {declared_rise:.3} m against              the computed MTOW droop {:.3} m",
            heavy.deflection.tip_droop_m()
        );
        let rise = one_g_tip_rise_m(&case.config, &case.design, &case.plane).unwrap();
        println!(
            "{name} 1 g level tip rise {rise:.3} m; ground-to-1g tip flex {:.3} m",
            rise + heavy.deflection.tip_droop_m()
        );
        let curve = &heavy.deflection.deflection_down_m;
        assert!(curve.iter().all(|w| w.is_finite() && *w >= 0.0), "{name}");
        assert!(
            curve.windows(2).all(|p| p[1] >= p[0] - 1e-12),
            "{name}: monotone"
        );
        let tip = heavy.deflection.tip_droop_m();
        let semispan = *heavy.deflection.y_m.last().unwrap();
        assert!(
            tip > 0.0 && tip < 0.15 * semispan,
            "{name}: tip droop {tip:.3} m"
        );
        // A wing with no fuel droops less than the same wing with fuel.
        let empty = response(
            &case,
            GroundCondition {
                gross_mass_kg: mtow,
                wing_fuel_kg: Some(0.0),
                load_factor: 1.0,
            },
        );
        assert!(empty.deflection.tip_droop_m() < tip, "{name}: fuel droop");
        // A taxi bump scales the whole response linearly.
        let bump = response(
            &case,
            GroundCondition {
                load_factor: 1.5,
                ..GroundCondition::at_rest(mtow)
            },
        );
        assert!(
            (bump.deflection.tip_droop_m() / tip - 1.5).abs() < 1e-9,
            "{name}"
        );
    }
}

#[test]
fn the_a380_droop_against_the_published_drawing_and_light_weight_evidence() {
    let case = case("A380-800");
    let mrw = response(&case, GroundCondition::at_rest(575_000.0));
    let light = response(&case, GroundCondition::at_rest(300_000.0));
    print_case("A380-800", &case, "MRW 575 t", &mrw);
    print_case("A380-800", &case, "light 300 t", &light);
    let ground_z = ground_plane_z_m(
        &case.plane.fuselages[0],
        &case.config.geometry,
        &case.config.landing_gear,
    );
    let heavy = ground_clearances(&case.plane, ground_z, &mrw.deflection).unwrap();
    println!(
        "A380 computed droop MRW: tip {:.3} (drawn-minus-table 1.41), N1 {:.3} (0.06), N2 {:.3} (0.41)",
        heavy.tip_droop_m, heavy.nacelles[0].droop_m, heavy.nacelles[1].droop_m
    );
    println!(
        "A380 computed light-vs-MRW rise: tip {:.3} (table 0.73 W2 / 0.52 vs fuselage), N1 {:.3}, N2 {:.3} (table N2 0.37 / 0.16 vs fuselage)",
        mrw.deflection.tip_droop_m() - light.deflection.tip_droop_m(),
        mrw.deflection.droop_at_m(heavy.nacelles[0].y_m) - light.deflection.droop_at_m(heavy.nacelles[0].y_m),
        mrw.deflection.droop_at_m(heavy.nacelles[1].y_m) - light.deflection.droop_at_m(heavy.nacelles[1].y_m),
    );
    assert!(mrw.deflection.tip_droop_m() > light.deflection.tip_droop_m());
}
