// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Mission segment library: verification against closed forms and
//! self-consistency (step size, energy closure), then the preset pipeline.
//!
//! This is numerical verification of the implementation. The aircraft and the
//! powertrain are test doubles with round numbers; nothing here is a physical
//! validation against flight data.

use alas_uav::perf::*;
use alas_uav::score::{presets, ScoreEngine};
use alas_uav::spec::mission::{Segment, SegmentKind, Surface, Until};

const G0: f64 = 9.806_65;

/// Constant thrust, constant current, independent of throttle and speed.
struct ConstantThrust {
    thrust_n: f64,
}

impl Powertrain for ConstantThrust {
    fn evaluate(
        &self,
        _th: f64,
        _v: f64,
        _rho: f64,
        _b: &BatteryState,
    ) -> Result<PowerPoint, PerfError> {
        Ok(PowerPoint {
            thrust_n: self.thrust_n,
            battery_current_a: 20.0,
            battery_voltage_v: 11.0,
        })
    }
}

fn airframe(cd0: f64, k: f64) -> Airframe {
    Airframe {
        mass_kg: 6.0,
        payload_kg: 3.15,
        wing_area_m2: 0.8,
        span_m: 3.0,
        polar: Box::new(ParabolicPolar {
            cd0,
            k,
            cl_max: 1.4,
        }),
        cl_ground: 0.3,
        gear_drag_area_m2: 0.002,
        wing_height_m: 0.25,
        mu_roll: 0.05,
        mu_brake: 0.3,
        n_max: 4.0,
    }
}

fn stub() -> StubPowertrain {
    StubPowertrain {
        static_thrust_n: 30.0,
        zero_thrust_speed_m_s: 40.0,
        max_current_a: 60.0,
        open_circuit_v: 12.6,
        internal_resistance_ohm: 0.03,
        capacity_c: 18_000.0,
    }
}

fn env(headwind: f64) -> Environment {
    Environment::new(1.2, headwind).unwrap()
}

fn approx(a: f64, b: f64, rel: f64) {
    assert!(
        (a - b).abs() <= rel * b.abs().max(1e-12),
        "{a} differs from {b} by more than {rel} relative"
    );
}

#[test]
fn constant_thrust_ground_roll_matches_v_squared_over_two_a() {
    // No drag, no friction, constant thrust: a = T/m and s = V_LOF^2 / (2 a).
    let mut af = airframe(0.0, 0.0);
    af.mu_roll = 0.0;
    af.gear_drag_area_m2 = 0.0;
    let spec = TakeoffSpec {
        throttle_ramp_s: 1e-6,
        ..TakeoffSpec::default()
    };
    let r = ground_roll(
        &af,
        &ConstantThrust { thrust_n: 12.0 },
        env(0.0),
        spec,
        0.05,
    )
    .unwrap();
    let vs = af.stall_speed(1.2);
    let v_lof = 1.15 * vs;
    let a = 12.0 / af.mass_kg;
    approx(r.liftoff_airspeed_m_s, v_lof, 1e-6);
    approx(r.distance_m, v_lof * v_lof / (2.0 * a), 1e-6);
    approx(r.time_s, v_lof / a, 1e-6);
}

#[test]
fn headwind_shortens_and_friction_lengthens_the_roll() {
    let (af, pt) = (airframe(0.04, 0.035), stub());
    let calm = ground_roll(&af, &pt, env(0.0), TakeoffSpec::default(), 0.05).unwrap();
    let wind = ground_roll(&af, &pt, env(4.0), TakeoffSpec::default(), 0.05).unwrap();
    assert!(wind.distance_m < calm.distance_m);
    // The WUT ACC 2026 report: mu 0.05 -> 0.07 lengthens the roll (51.9 -> 56.9 m, +10 %).
    let mut rough = airframe(0.04, 0.035);
    rough.mu_roll = 0.07;
    let slow = ground_roll(&rough, &pt, env(0.0), TakeoffSpec::default(), 0.05).unwrap();
    assert!(slow.distance_m > calm.distance_m);
    assert!(
        calm.distance_m > 5.0 && calm.distance_m < 60.0,
        "{}",
        calm.distance_m
    );
    // Lift-off at 1.15 Vs.
    approx(calm.liftoff_airspeed_m_s, 1.15 * af.stall_speed(1.2), 1e-6);
}

#[test]
fn ground_roll_is_converged_in_the_time_step() {
    let (af, pt) = (airframe(0.04, 0.035), stub());
    let a = ground_roll(&af, &pt, env(0.0), TakeoffSpec::default(), 0.05).unwrap();
    let b = ground_roll(&af, &pt, env(0.0), TakeoffSpec::default(), 0.0125).unwrap();
    approx(a.distance_m, b.distance_m, 1e-5);
    approx(a.time_s, b.time_s, 1e-5);
}

#[test]
fn energy_and_charge_close_over_the_roll() {
    let (af, pt) = (airframe(0.04, 0.035), stub());
    let r = ground_roll(&af, &pt, env(0.0), TakeoffSpec::default(), 0.05).unwrap();
    let last = r.samples.last().unwrap();
    // Trapezoid of the logged power and current against the integrated states.
    let (mut e, mut q) = (0.0, 0.0);
    for w in r.samples.windows(2) {
        let dt = w[1].t - w[0].t;
        e += 0.5 * (w[0].power_w + w[1].power_w) * dt;
        q += 0.5 * (w[0].current_a + w[1].current_a) * dt;
    }
    approx(e, last.energy_j, 2e-3);
    approx(q, last.charge_c, 2e-3);
    assert!(r.peak_current_a > 10.0 && r.peak_current_a <= 60.0);
}

#[test]
fn invalid_takeoffs_are_reported_not_hidden() {
    let (af, pt) = (airframe(0.04, 0.035), stub());
    assert!(matches!(
        ground_roll(&af, &pt, env(-2.0), TakeoffSpec::default(), 0.05),
        Err(PerfError::InvalidInput(_))
    ));
    assert!(matches!(
        ground_roll(&af, &pt, env(0.0), TakeoffSpec::default(), 0.2),
        Err(PerfError::InvalidInput(_))
    ));
    // Thrust below rolling resistance: it never leaves the runway.
    let weak = ConstantThrust { thrust_n: 0.5 };
    assert!(matches!(
        ground_roll(&af, &weak, env(0.0), TakeoffSpec::default(), 0.05),
        Err(PerfError::NoConvergence(_))
    ));
    let mut bad = airframe(0.04, 0.035);
    bad.mass_kg = -1.0;
    assert!(ground_roll(&bad, &pt, env(0.0), TakeoffSpec::default(), 0.05).is_err());
}

#[test]
fn steady_turn_closed_form() {
    let af = airframe(0.04, 0.035);
    let bank = 45.0_f64.to_radians();
    let t = steady_turn(&af, 1.2, 20.0, bank).unwrap();
    approx(t.load_factor, 2_f64.sqrt(), 1e-12);
    approx(t.radius_m, 400.0 / G0, 1e-12); // V^2 / (g tan 45 deg)
    approx(t.rate_rad_s, G0 / 20.0, 1e-12);
    approx(
        t.cl,
        t.load_factor * af.weight() / (0.5 * 1.2 * 400.0 * 0.8),
        1e-12,
    );
    assert!(t.stall_margin > 1.0);
    // Slow and steep stalls; beyond n_max is rejected.
    assert!(matches!(
        steady_turn(&af, 1.2, 11.0, bank),
        Err(PerfError::Stall { .. })
    ));
    assert!(steady_turn(&af, 1.2, 20.0, 80.0_f64.to_radians()).is_err());
    assert!(steady_turn(&af, 1.2, 20.0, 0.0).is_err());
}

fn seg(name: &str, kind: SegmentKind) -> Segment {
    Segment {
        name: name.into(),
        kind,
    }
}

fn takeoff(runway: f64) -> Segment {
    seg(
        "takeoff",
        SegmentKind::Takeoff {
            runway_m: runway,
            surface: Surface::Hard,
        },
    )
}

#[test]
fn climb_gradient_matches_the_force_balance() {
    // Constant thrust and a held speed: sin(gamma) = (T - D) / W with
    // L = W cos(gamma). The expected angle is solved independently by
    // fixed-point iteration in the test.
    let af = airframe(0.04, 0.035);
    let pt = ConstantThrust { thrust_n: 25.0 };
    let v = 14.0;
    let cfg = MissionConfig {
        climb_speed_m_s: Some(v),
        ..MissionConfig::default()
    };
    let segs = [
        takeoff(80.0),
        seg(
            "climb",
            SegmentKind::Climb {
                end: Until::Altitude { meters: 80.0 },
            },
        ),
    ];
    let m = fly_mission(&af, &pt, env(0.0), &segs, &cfg).unwrap();
    let w = af.weight();
    let mut gamma = 0.0_f64;
    for _ in 0..50 {
        let cl = w * gamma.cos() / (0.5 * 1.2 * v * v * 0.8);
        let d = 0.5 * 1.2 * v * v * 0.8 * (0.04 + 0.035 * cl * cl);
        gamma = ((25.0 - d) / w).asin();
    }
    // Take the gradient well after the speed transient from lift-off.
    let late: Vec<_> = m.samples.iter().filter(|s| s.h > 60.0).collect();
    assert!(!late.is_empty());
    for s in late {
        approx(s.gamma, gamma, 2e-3);
        approx(s.v, v, 2e-3);
    }
    let c = m.segment("climb").unwrap();
    approx(c.h_end, 80.0, 1e-9);
}

#[test]
fn glide_ratio_at_best_glide_speed_is_l_over_d_max() {
    // Parabolic polar: (L/D)max = 1 / (2 sqrt(CD0 k)), and the ground
    // distance of a steady glide from height h is h L/D.
    let (cd0, k) = (0.03, 0.03);
    let af = airframe(cd0, k);
    let v_bg = af.best_glide_speed(1.2);
    let cfg = MissionConfig {
        climb_speed_m_s: Some(v_bg),
        glide_speed_m_s: Some(v_bg),
        ..MissionConfig::default()
    };
    let segs = [
        takeoff(80.0),
        seg(
            "climb",
            SegmentKind::Climb {
                end: Until::Altitude { meters: 100.0 },
            },
        ),
        seg(
            "glide",
            SegmentKind::Glide {
                end: Until::Altitude { meters: 0.0 },
            },
        ),
    ];
    // The constant-thrust double also produces thrust in the glide, so use a
    // powertrain that goes silent at zero throttle.
    struct Throttled(f64);
    impl Powertrain for Throttled {
        fn evaluate(
            &self,
            th: f64,
            _v: f64,
            _r: f64,
            _b: &BatteryState,
        ) -> Result<PowerPoint, PerfError> {
            Ok(PowerPoint {
                thrust_n: self.0 * th,
                battery_current_a: 20.0 * th,
                battery_voltage_v: 11.0,
            })
        }
    }
    let m = fly_mission(&af, &Throttled(30.0), env(0.0), &segs, &cfg).unwrap();
    let g = m.segment("glide").unwrap();
    let ld_max = 1.0 / (2.0 * (cd0 * k).sqrt());
    let ratio = (g.x_end - g.x_start) / (100.0 - g.h_end);
    approx(ratio, ld_max, 1.5e-2);
    assert!(
        g.h_end.abs() < 1e-6,
        "glide ends on the ground, h = {}",
        g.h_end
    );
    // The best-glide scan lands within 2 % of sqrt(2 W / (rho S CL*)), CL* = sqrt(CD0 / k).
    let cl_star = (cd0 / k).sqrt();
    approx(
        v_bg,
        (2.0 * af.weight() / (1.2 * 0.8 * cl_star)).sqrt(),
        2e-2,
    );
}

#[test]
fn level_flight_holds_speed_and_turn_follows_the_closed_form() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let v = 20.0;
    let cfg = MissionConfig {
        cruise: CruiseControl::HoldSpeed(v),
        climb_speed_m_s: Some(v),
        turn_bank_rad: 45.0_f64.to_radians(),
        ..MissionConfig::default()
    };
    // Two straights of 300 m joined by one 180 degree turn, no wind.
    let segs = [
        takeoff(80.0),
        seg(
            "climb",
            SegmentKind::Climb {
                end: Until::Altitude { meters: 30.0 },
            },
        ),
        seg(
            "circuit",
            SegmentKind::Laps {
                lap_length_m: 600.0,
                turn_gate: true,
                count: 1,
            },
        ),
    ];
    let m = fly_mission(&af, &pt, env(0.0), &segs, &cfg).unwrap();
    let c = m.segment("circuit").unwrap();
    assert_eq!(c.detail["legs"], 2.0);
    assert_eq!(c.detail["laps"], 1.0);
    // Speed held (throttle governor) within 2 % after the first seconds.
    let in_laps: Vec<_> = m
        .samples
        .iter()
        .filter(|s| s.segment == 2 && s.t > c.t_start + 5.0)
        .collect();
    for s in &in_laps {
        approx(s.v, v, 2e-2);
    }
    // Straights: 300 m at 20 m/s = 15 s each; the turn: pi R / V = pi V / (g tan phi).
    let turn = pi_turn_time(v, 45.0_f64.to_radians());
    let straight = 300.0 / v;
    let total = c.t_end - c.t_start;
    approx(total, 2.0 * straight + turn, 3e-2);
    // Heading changes by pi exactly across the turn.
    let psi_end = m
        .samples
        .iter()
        .filter(|s| s.segment == 2)
        .map(|s| s.psi)
        .fold(0.0, f64::max);
    approx(psi_end, std::f64::consts::PI, 1e-9);
    // Load factor in the turn is 1/cos(bank).
    let n_max = in_laps.iter().map(|s| s.load_factor).fold(0.0, f64::max);
    approx(n_max, 2_f64.sqrt(), 1e-9);
    // Ground distance of the segment: 600 m of straights plus the turn arc.
    let arc = std::f64::consts::PI * v * v / (G0 * 45.0_f64.to_radians().tan());
    approx(c.x_end - c.x_start, 600.0 + arc, 3e-2);
}

fn pi_turn_time(v: f64, bank: f64) -> f64 {
    std::f64::consts::PI * v / (G0 * bank.tan())
}

#[test]
fn laps_alternate_headwind_and_respect_the_window() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let v = 18.0;
    let w = 4.0;
    let cfg = MissionConfig {
        cruise: CruiseControl::HoldSpeed(v),
        climb_speed_m_s: Some(v),
        ..MissionConfig::default()
    };
    // No turns, 600 m legs: upwind leg at (v - w), downwind at (v + w).
    let segs = [
        takeoff(80.0),
        seg(
            "climb",
            SegmentKind::Climb {
                end: Until::Altitude { meters: 30.0 },
            },
        ),
        seg(
            "charter",
            SegmentKind::Laps {
                lap_length_m: 1200.0,
                turn_gate: false,
                count: 2,
            },
        ),
    ];
    let m = fly_mission(&af, &pt, env(w), &segs, &cfg).unwrap();
    let s = m.segment("charter").unwrap();
    assert_eq!(s.detail["legs"], 4.0);
    // Four legs: two up, two down.
    let expected = 2.0 * 600.0 / (v - w) + 2.0 * 600.0 / (v + w);
    approx(s.t_end - s.t_start, expected, 3e-2);
    // A 40 s window ends the segment at the cap and counts completed legs only.
    let capped = MissionConfig {
        window_s: Some(40.0),
        ..cfg
    };
    let m = fly_mission(&af, &pt, env(w), &segs, &capped).unwrap();
    let s = m.segment("charter").unwrap();
    approx(s.t_end - s.t_start, 40.0, 1e-6);
    assert!(s.detail["legs"] < 4.0);
    assert_eq!(s.detail["laps"], (s.detail["legs"] / 2.0).floor());
}

#[test]
fn flight_clock_from_throttle_up_includes_the_ground_roll() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let mk = |from_throttle: bool| {
        let cfg = MissionConfig {
            flight_clock_from_throttle_up: from_throttle,
            ..MissionConfig::default()
        };
        let segs = [
            takeoff(80.0),
            seg(
                "climb",
                SegmentKind::Climb {
                    end: Until::Time { seconds: 60.0 },
                },
            ),
        ];
        fly_mission(&af, &pt, env(0.0), &segs, &cfg).unwrap()
    };
    let acc = mk(true);
    approx(acc.segment("climb").unwrap().t_end, 60.0, 1e-9);
    let xc = mk(false);
    let lift = xc.liftoff_time_s.unwrap();
    approx(xc.segment("climb").unwrap().t_end, lift + 60.0, 1e-9);
}

#[test]
fn unsupported_and_misordered_segments_are_errors() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let cfg = MissionConfig::default();
    let launch = [seg(
        "l",
        SegmentKind::Launch {
            method: "hand".into(),
        },
    )];
    assert!(matches!(
        fly_mission(&af, &pt, env(0.0), &launch, &cfg),
        Err(PerfError::InvalidInput(_) | PerfError::Unsupported(_))
    ));
    let early = [seg(
        "c",
        SegmentKind::Climb {
            end: Until::Time { seconds: 5.0 },
        },
    )];
    assert!(matches!(
        fly_mission(&af, &pt, env(0.0), &early, &cfg),
        Err(PerfError::InvalidInput(_))
    ));
    let delivery = [takeoff(80.0), seg("d", SegmentKind::Delivery)];
    assert!(matches!(
        fly_mission(&af, &pt, env(0.0), &delivery, &cfg),
        Err(PerfError::Unsupported(_))
    ));
    // Holding a speed below the stall speed is a stall, not a number.
    let slow = MissionConfig {
        cruise: CruiseControl::HoldSpeed(7.0),
        climb_speed_m_s: Some(14.0),
        ..cfg
    };
    let segs = [
        takeoff(80.0),
        seg(
            "climb",
            SegmentKind::Climb {
                end: Until::Altitude { meters: 20.0 },
            },
        ),
        seg(
            "cruise",
            SegmentKind::Cruise {
                end: Until::Time { seconds: 10.0 },
                altitude_window_m: [10.0, 100.0],
            },
        ),
    ];
    assert!(matches!(
        fly_mission(&af, &pt, env(0.0), &segs, &slow),
        Err(PerfError::Stall { .. })
    ));
}

#[test]
fn landing_touches_down_above_stall_and_rolls_out() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let mut y0 = [0.0; 6];
    y0[1] = 30.0; // h
    y0[2] = 1.3 * af.stall_speed(1.2); // v
    let r = land(&af, &pt, env(0.0), LandingSpec::default(), 0.0, y0, 0.05).unwrap();
    assert!(r.touchdown_over_stall >= 1.0, "{}", r.touchdown_over_stall);
    assert!(r.rollout_m > 1.0 && r.rollout_m < 60.0, "{}", r.rollout_m);
    assert!(r.flare_distance_m > 0.0);
    // Independent rollout: RK4 at 1 ms with the same equations (L = q S CLg,
    // D with the ground-effect factor and gear drag, brakes mu_brake).
    let (rho, s_ref, w, m) = (1.2, 0.8, af.weight(), af.mass_kg);
    let phi = af.ground_effect_factor();
    let deriv = |v: f64| {
        let q = 0.5 * rho * v * v;
        let lift = q * s_ref * 0.3;
        let drag = q * s_ref * (0.04 + 0.002 / 0.8 + phi * 0.035 * 0.09);
        (-(drag) - 0.3 * (w - lift).max(0.0)) / m
    };
    let (mut v, mut x) = (r.touchdown_speed_m_s, 0.0);
    let dt = 1e-3;
    while v > 0.05 {
        let k1 = deriv(v);
        let k2 = deriv(v + 0.5 * dt * k1);
        let k3 = deriv(v + 0.5 * dt * k2);
        let k4 = deriv(v + dt * k3);
        let vn = v + dt / 6.0 * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
        x += 0.5 * (v + vn) * dt;
        v = vn;
    }
    approx(r.rollout_m, x, 5e-3);
}

#[test]
fn flare_ends_at_the_touchdown_lift_limit_before_stall() {
    // Before the fix the idle flare decelerated from 1.3 Vs onto the CLmax
    // boundary and stopped with a stall error. The flare must now end at the
    // touchdown lift limit: V_TD = Vs / sqrt(fraction), i.e. 1.15 Vs by default.
    let pt = stub();
    let spec = LandingSpec::default();
    let want = 1.0 / spec.touchdown_cl_fraction.sqrt();
    for (cd0, k) in [(0.03, 0.05), (0.05, 0.035)] {
        let af = airframe(cd0, k);
        let mut y0 = [0.0; 6];
        y0[1] = 30.0;
        y0[2] = 1.3 * af.stall_speed(1.2);
        let r = land(&af, &pt, env(0.0), spec, 0.0, y0, 0.05).unwrap();
        approx(r.touchdown_over_stall, want, 1e-2);
        assert!(r.flare_distance_m > 0.0 && r.rollout_m > 0.0);
    }
    // A limit above 1 (or non-positive) is refused, not clamped.
    let bad = LandingSpec {
        touchdown_cl_fraction: 1.2,
        ..spec
    };
    let mut y0 = [0.0; 6];
    y0[1] = 30.0;
    y0[2] = 20.0;
    assert!(land(&airframe(0.03, 0.05), &pt, env(0.0), bad, 0.0, y0, 0.05).is_err());
}

fn acc2026_segments() -> Vec<Segment> {
    presets::load("acc-2026").unwrap().segments
}

#[test]
fn acc_2026_preset_template_flies_and_scores_end_to_end() {
    // The preset's own segment template, flown with the stub powertrain and
    // scored by the engine: the whole chain mission -> series -> expression.
    let af = airframe(0.04, 0.035);
    let pt = stub();
    // Part throttle keeps the 30 A overshoot small enough for the penalty not
    // to saturate at 1 (a full-throttle stub run is capped, which the score
    // tests cover).
    let cfg = MissionConfig {
        flight_clock_from_throttle_up: true,
        climb_throttle: 0.6,
        cruise: CruiseControl::Throttle(0.75),
        ..MissionConfig::default()
    };
    let m = fly_mission(&af, &pt, env(0.0), &acc2026_segments(), &cfg).unwrap();
    // Climb ends at flight time 60 s (rules 4.6.5), then 120 s of distance.
    approx(m.segment("climb").unwrap().t_end, 60.0, 1e-9);
    let d = m.segment("distance").unwrap();
    approx(d.t_end - d.t_start, 120.0, 1e-9);
    assert_eq!(d.detail["altitude_window_ok"], 1.0);
    assert_eq!(m.loading_time_s, 60.0);

    let mut inputs = score_inputs(&m, &af);
    for (k, v) in [
        ("payload_count", 9.0),
        ("announced_takeoff", 60.0),
        ("declared_payload_count", 9.0),
    ] {
        inputs.values.insert(k.into(), v);
    }
    assert!(inputs.values["distance_120s"] > 1500.0 && inputs.values["distance_120s"] < 4000.0);
    assert!(inputs.values["takeoff_distance"] < 60.0);
    // The stub draws more than 30 A at full throttle in cruise: a real penalty.
    assert!(inputs.values["peak_current"] > 30.0);
    let e = ScoreEngine::new(&presets::load("acc-2026").unwrap(), None).unwrap();
    let r = e.evaluate_round(&inputs).unwrap();
    assert!(r.term("i_over").unwrap() > 0.0);
    assert!(r.term("p_current").unwrap() > 0.0 && r.term("p_current").unwrap() < 1.0);
    let s_raw =
        3.15 * inputs.values["distance_120s"].powi(2) * (1.0 - r.term("p_current").unwrap());
    approx(r.term("s_raw").unwrap(), s_raw, 1e-9);
    assert!(r.score.is_finite() && r.score > 0.0);
}

#[test]
fn energy_closes_over_a_whole_mission() {
    let af = airframe(0.04, 0.035);
    let pt = stub();
    let cfg = MissionConfig {
        flight_clock_from_throttle_up: true,
        ..MissionConfig::default()
    };
    let m = fly_mission(&af, &pt, env(0.0), &acc2026_segments(), &cfg).unwrap();
    let (mut e, mut q) = (0.0, 0.0);
    for w in m.samples.windows(2) {
        let dt = w[1].t - w[0].t;
        e += 0.5 * (w[0].power_w + w[1].power_w) * dt;
        q += 0.5 * (w[0].current_a + w[1].current_a) * dt;
    }
    let last = m.samples.last().unwrap();
    approx(e, last.energy_j, 5e-3);
    approx(q, last.charge_c, 5e-3);
    // Samples are in time order and the step never exceeds 0.05 s.
    for w in m.samples.windows(2) {
        let dt = w[1].t - w[0].t;
        assert!(dt > 0.0 && dt <= 0.05 + 1e-9, "dt {dt}");
    }
    // Segment indices are non-decreasing.
    assert!(m.samples.windows(2).all(|w| w[1].segment >= w[0].segment));
}

#[test]
fn every_preset_template_with_flyable_segments_runs() {
    // Templates built from supported kinds fly with the stub; Launch and
    // Delivery are the only unsupported kinds and no preset uses them.
    let af = airframe(0.04, 0.035);
    let pt = stub();
    for p in presets::load_all().unwrap() {
        let cfg = MissionConfig {
            flight_clock_from_throttle_up: p
                .parameters
                .iter()
                .any(|d| d.name == "flight_clock_throttle_up" && d.default > 0.0),
            window_s: p
                .parameters
                .iter()
                .find(|d| d.name == "flight_window_s")
                .map(|d| d.default),
            cruise: CruiseControl::HoldSpeed(17.0),
            climb_speed_m_s: Some(14.0),
            ..MissionConfig::default()
        };
        let m = fly_mission(&af, &pt, env(0.0), &p.segments, &cfg)
            .unwrap_or_else(|e| panic!("{}: {e}", p.id));
        assert!(!m.samples.is_empty(), "{}", p.id);
        assert_eq!(m.segments.len(), p.segments.len());
    }
}
