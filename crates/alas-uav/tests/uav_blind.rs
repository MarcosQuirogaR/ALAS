// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
// The BLIND|/E2E| lines are the reported evidence of this target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

//! M2 exit criterion: blind prediction of three published ACC 2026 team
//! aircraft (Trencalos, Xtra2UPV, UVigo) from published geometry, mass and
//! propulsion only.
//!
//! The test runs the full chain (spec -> viscous VLM + drag build-up ->
//! powertrain -> mission -> ACC score), prints one `BLIND|` line per
//! comparison and asserts only that the numbers are finite and physical: the
//! errors against the reports are REPORTED (agent report), not forced into a
//! band by tuning.  Reported figures are the teams' own predictions except
//! where marked measured (static thrust test stands).
//!
//! Documented bands (fixed before the first run, see the report): static
//! thrust +-20 % (repo regression allows 20 % CT/CP error; the v1 target is
//! 10 %), stall speed +-8 %, maximum and cruise speed +-10 %, rate of climb
//! +-35 % (difference of two large forces), take-off distance +-25 %.

#[path = "support/blind_aircraft.rs"]
mod blind;

use alas_uav::coupled::{performance_summary, ElectricPowertrain, ElectricSettings, Vehicle};
use alas_uav::perf::{Environment, TakeoffSpec};
use alas_uav::score::presets;

use blind::{vehicle, Team};

fn line(team: &str, what: &str, predicted: f64, reported: f64, unit: &str, tag: &str) {
    println!(
        "BLIND|{team}|{what}|{predicted:.3}|{reported:.3}|{unit}|{:+.1}%|{tag}",
        100.0 * (predicted - reported) / reported
    );
}

fn stand(t: &Team, reported_n: f64) -> f64 {
    // One motor at the 15 A bench condition: the published stand figure.
    let mut chain = t.chain.clone();
    chain.n_motors = 1;
    let pt = ElectricPowertrain::new(
        chain,
        ElectricSettings {
            battery_current_cap_a: Some(15.0),
            ..ElectricSettings::default()
        },
        t.rho,
    )
    .unwrap();
    let op = pt.operating_point(1.0, 0.0, 1.0).unwrap();
    line(
        t.name,
        "static thrust @15 A",
        op.thrust_n,
        reported_n,
        "N",
        "measured",
    );
    println!(
        "BLIND-INFO|{}|static: {:.1} A battery, {:.0} rpm, eta_motor {:.2}, flags {:?} {:?}",
        t.name, op.i_battery_a, op.rpm, op.eta_motor, op.limited_by, op.warnings
    );
    op.thrust_n
}

fn run(t: &Team, takeoff_cap: Option<f64>) {
    let env = Environment::new(t.rho, 0.0).unwrap();
    // Flight performance with the ACC 30 A cap; take-off with `takeoff_cap`.
    let v = vehicle(t, Some(30.0));
    let models = v.models(&env).unwrap();
    println!(
        "BLIND-INFO|{}|polar: CLalpha {:.2}/rad, e {:.3}, CD0 {:.4}, (L/D)max {:.1}, CLmax {:.3}, Vs {:.2} m/s, AR {:.2}, mass {:.2} kg",
        t.name,
        models.polar.cl_alpha_per_rad,
        models.polar.span_efficiency,
        models.polar.cd0,
        models.polar.l_over_d_max,
        models.airframe.polar.cl_max(),
        models.polar.stall_speed_m_s,
        models.polar.aspect_ratio,
        models.airframe.mass_kg
    );
    for (name, cd0) in &models.polar.build_up {
        println!("BLIND-INFO|{}|build-up {name}: CD0 {cd0:.5}", t.name);
    }
    for n in models
        .polar
        .validity_notes
        .iter()
        .chain(&models.powertrain.warnings)
    {
        println!("BLIND-INFO|{}|note: {n}", t.name);
    }
    let sum = performance_summary(&models, env, TakeoffSpec::default()).unwrap();
    let find = |k: &str| t.reported.iter().find(|r| r.what.starts_with(k));
    let tag = |r: &blind::Reported| {
        if r.measured {
            "measured"
        } else {
            "team-prediction"
        }
    };
    if let Some(r) = find("stall speed") {
        line(
            t.name,
            "stall speed",
            sum.stall_speed_m_s,
            r.value,
            "m/s",
            tag(r),
        );
    }
    if let (Some(r), Some(vm)) = (find("maximum speed"), sum.v_max_m_s) {
        line(t.name, "maximum speed", vm, r.value, "m/s", tag(r));
    }
    if let Some(r) = find("max rate of climb").or_else(|| find("max vertical speed")) {
        line(
            t.name,
            "max rate of climb",
            sum.max_climb_rate_m_s,
            r.value,
            "m/s",
            tag(r),
        );
    }
    if let Some(r) = find("speed of best climb") {
        line(
            t.name,
            "speed of best climb",
            sum.v_best_climb_m_s,
            r.value,
            "m/s",
            tag(r),
        );
    }
    println!(
        "BLIND-INFO|{}|static thrust (full duty, both motors, cap 30 A) {:.1} N at {:.1} A; best climb at {:.1} A",
        t.name, sum.static_thrust_n, sum.static_current_a, sum.climb_current_a
    );
    // Take-off with its own current cap.
    let vt = vehicle(t, takeoff_cap);
    let mt = vt.models(&env).unwrap();
    let st = performance_summary(&mt, env, TakeoffSpec::default()).unwrap();
    if let Some(r) = find("take-off distance") {
        line(
            t.name,
            "take-off distance",
            st.takeoff.distance_m,
            r.value,
            "m",
            tag(r),
        );
    }
    println!(
        "BLIND-INFO|{}|take-off: {:.1} m, {:.1} s, lift-off {:.1} m/s, peak {:.0} A",
        t.name,
        st.takeoff.distance_m,
        st.takeoff.time_s,
        st.takeoff.liftoff_airspeed_m_s,
        st.takeoff.peak_current_a
    );
    assert!(sum.stall_speed_m_s.is_finite() && sum.stall_speed_m_s > 5.0);
    assert!(st.takeoff.distance_m > 3.0 && st.takeoff.distance_m < 300.0);
    assert!(sum.max_climb_rate_m_s.is_finite());
    score(t, &v, env);
}

fn score(t: &Team, v: &Vehicle<'_>, env: Environment) {
    let preset = presets::load("acc-2026").unwrap();
    let mut cfg = alas_uav::coupled::assess::mission_config_for(&preset);
    cfg.climb_throttle = 1.0;
    let a = v.assess(env, &cfg).unwrap();
    let d = a.inputs.values["distance_120s"];
    println!(
        "BLIND-INFO|{}|ACC round: distance_120s {:.0} m, peak current {:.1} A, takeoff {:.1} m, round score {:.1}",
        t.name,
        d,
        a.inputs.values["peak_current"],
        a.inputs.values["takeoff_distance"],
        a.round.score
    );
    assert!(d > 500.0 && d < 6000.0);
}

#[test]
fn trencalos_blind() {
    let t = blind::trencalos();
    stand(&t, 11.0);
    run(&t, Some(30.0));
}

#[test]
fn xtra2upv_blind() {
    let t = blind::xtra2upv();
    stand(&t, 10.24);
    run(&t, Some(30.0));
}

#[test]
fn uvigo_blind() {
    let t = blind::uvigo();
    // The UVigo take-off uses the native ~60 A (no limiter) [p.22].
    run(&t, None);
}

/// Extra comparison requested by the coordinator: the airframe lane's
/// bottom-up STRUCTURE estimate (generic foam-composite preset, uncalibrated)
/// against what the reports publish.  The flights above use the published
/// masses; this only measures the mass model.
fn structure_estimate(t: &Team, fuselage: (f64, f64)) -> (f64, f64, f64) {
    use alas_uav::airframe::geometry::{AirframeGeometry, TailShape};
    use alas_uav::airframe::structure::{
        boom_structure, fuselage_structure, tail_structure, wing_structure, FuselageShell,
        StructureInputs,
    };
    // Wing: component build-up (foam core + 1 ply of 50 g/m2 glass per side +
    // stock carbon tube spar), t/c 0.12 as an ESTIMATE for the three airfoils;
    // fixed from cited gauge data before the comparison, no team factor.
    let mut inputs = StructureInputs::generic_foam_composite();
    let mass = alas_uav::coupled::tree_mass(&t.spec).unwrap().0;
    inputs.wing_buildup = Some(alas_uav::airframe::wing_buildup::WingBuildup::new(
        alas_uav::airframe::wing_buildup::Construction::FoamCoreComposite,
        0.12,
        mass,
    ));
    let g = AirframeGeometry::from_master(&t.spec.master, &t.spec.topology, &TailShape::default())
        .unwrap();
    let wing = wing_structure(&g.wing, &inputs).unwrap().mass_kg;
    let mut tails = 0.0;
    for s in g.horizontal.iter().chain(g.vertical.iter()) {
        tails += tail_structure(s, &inputs).unwrap().mass_kg;
    }
    if let Some(b) = &t.spec.master.tail_boom {
        tails += boom_structure(b, 1.2, [0.0, 0.0], &inputs).unwrap().mass_kg;
    }
    let shell = FuselageShell {
        x_start_m: 0.0,
        length_m: fuselage.0,
        diameter_m: fuselage.1,
        centre_yz_m: [0.0, 0.0],
    };
    let fus = fuselage_structure(&shell, &inputs).unwrap().mass_kg;
    let k = 1.0 + inputs.allowance_fraction;
    (wing * k, tails * k, fus * k)
}

#[test]
fn airframe_bottom_up_structure_against_published_masses() {
    // UVigo [p.21]: wing structure 292 g, fuselage 320 g (final table).
    let t = blind::uvigo();
    let (w, tl, f) = structure_estimate(&t, (1.4, 0.12));
    println!(
        "BLIND-MASS|{}|wing structure|{w:.3}|0.292|kg|{:+.0}%",
        t.name,
        100.0 * (w / 0.292 - 1.0)
    );
    println!(
        "BLIND-MASS|{}|fuselage structure|{f:.3}|0.320|kg|{:+.0}%",
        t.name,
        100.0 * (f / 0.320 - 1.0)
    );
    println!("BLIND-MASS|{}|tails+boom|{tl:.3}|n/a|kg|", t.name);
    // Others publish only the empty mass: structure share of the OEW.
    for (t, fus) in [
        (blind::trencalos(), (0.9, 0.175)),
        (blind::xtra2upv(), (0.85, 0.17)),
    ] {
        let (w, tl, f) = structure_estimate(&t, fus);
        println!(
            "BLIND-MASS|{}|bottom-up structure (wing {w:.2} + tails/boom {tl:.2} + fuselage {f:.2}) = {:.2} kg vs published OEW {:.3} kg (structure only; propulsion, battery, servos, wiring not in the estimate)",
            t.name, w + tl + f, t.empty_kg
        );
        assert!(w + tl + f > 0.0);
    }
}

#[test]
fn xtra2upv_thrust_at_100_kmh_against_the_team_stand() {
    // Report 02 p.9 Table 4.1: 2.15 N at 100 km/h for one motor at 15 A
    // (0.0129 N/W), measured on the team's dynamic test stand.
    let t = blind::xtra2upv();
    let mut chain = t.chain.clone();
    chain.n_motors = 1;
    let pt = ElectricPowertrain::new(
        chain,
        ElectricSettings {
            battery_current_cap_a: Some(15.0),
            ..ElectricSettings::default()
        },
        t.rho,
    )
    .unwrap();
    let op = pt.operating_point(1.0, 100.0 / 3.6, 1.0).unwrap();
    line(
        t.name,
        "thrust @100 km/h, 15 A",
        op.thrust_n,
        2.15,
        "N",
        "measured",
    );
    println!(
        "BLIND-INFO|{}|100 km/h: {:.1} A battery, {:.0} rpm, J {:.2}, eta_prop {:.2}, duty {:.2}, flags {:?}",
        t.name, op.i_battery_a, op.rpm, op.j, op.eta_prop, op.duty, op.limited_by
    );
    assert!(op.thrust_n > 0.0);
}
