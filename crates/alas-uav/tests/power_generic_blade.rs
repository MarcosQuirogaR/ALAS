// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Generic-blade fallback with a NeuralFoil cambered section, its low-Re
//! warning, and the BEMT-to-map tabulation used by the mission adapter.
//!
//! Verification only: M2 lane A measured the static coefficients of the same
//! section; the numbers below are that lane's results re-derived, plus the
//! old analytic placeholder for the before/after comparison.  Typical measured
//! APC thin-electric figures (about 0.10 / 0.04, from memory of UIUC plots,
//! NOT verified on a page) are context, not a reference.

use alas_uav::power::{tabulate_bemt, BemtGeometry, PropModel};

const RHO: f64 = 1.225;
const MU: f64 = 1.789e-5;

fn static_coefficients(g: &BemtGeometry, rpm: f64, d: f64) -> (f64, f64, f64) {
    let omega = rpm * std::f64::consts::TAU / 60.0;
    let s = g.solve(omega, 0.0, RHO, MU).unwrap();
    let n = rpm / 60.0;
    (
        s.thrust_n / (RHO * n * n * d.powi(4)),
        s.torque_n_m * omega / (RHO * n.powi(3) * d.powi(5)),
        s.re_75,
    )
}

#[test]
fn nf_cambered_generic_blade_raises_static_ct_over_the_analytic_placeholder() {
    let d = 0.254;
    let analytic = BemtGeometry::generic_analytic(d, 0.127, 2, 0.15).unwrap();
    let nf = BemtGeometry::generic(d, 0.127, 2, 0.15).unwrap();
    assert!(nf.generic && analytic.generic);
    let (ct_a, cp_a, _) = static_coefficients(&analytic, 6000.0, d);
    let (ct_n, cp_n, re75) = static_coefficients(&nf, 6000.0, d);
    // Before: 0.073 / 0.024 (lane B).  After: about 0.09 / 0.035 (lane A).
    assert!((0.065..0.08).contains(&ct_a), "analytic CT {ct_a}");
    assert!((0.08..0.11).contains(&ct_n), "NF CT {ct_n}");
    assert!((0.028..0.045).contains(&cp_n), "NF CP {cp_n}");
    assert!(ct_n > ct_a && cp_n > cp_a);
    // Blade Reynolds number at 0.75 R is in the NeuralFoil warning band.
    assert!((3.0e4..1.0e5).contains(&re75), "Re75 {re75}");
}

#[test]
fn low_re_blade_sections_raise_the_warning_and_big_fast_blades_do_not() {
    let small = PropModel::from_bemt(BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap());
    let l = small.loads(628.0, 0.0, RHO, MU).unwrap();
    assert!(l.low_re, "10x5 at 6000 rpm static");
    // 16 inch blade at 6000 rpm: chord and speed both larger.
    let big = PropModel::from_bemt(BemtGeometry::generic(0.406, 0.2, 2, 0.15).unwrap());
    let lb = big.loads(628.0, 0.0, RHO, MU).unwrap();
    assert!(!lb.low_re, "16 inch blade must be above 8e4");
    // The operating-point solver surfaces it as a warning.
    use alas_uav::power::*;
    let chain = Powertrain {
        motor: MotorModel::new(1000.0 * std::f64::consts::TAU / 60.0, 0.03, 1.5, 10.0).unwrap(),
        esc: EscModel::from_nominal_efficiency(0.96, 11.1, 30.0).unwrap(),
        battery: BatteryModel::new(3.0 * 3600.0, 3, 0.02).unwrap(),
        prop: small,
        n_motors: 1,
    };
    let fl = Flight::new(0.0, RHO, 288.15);
    let dy = Dynamics {
        battery: BatteryState::new(1.0, 288.15),
        t_winding_k: 288.15,
    };
    let op = chain
        .solve(&fl, &dy, Demand::Throttle(0.3), &Limiter::none())
        .unwrap();
    assert!(
        op.warnings.contains(&LimitFlag::PropLowRe),
        "{:?}",
        op.warnings
    );
}

#[test]
fn tabulated_bemt_reproduces_the_bemt_inside_three_percent() {
    let model = PropModel::from_bemt(BemtGeometry::generic(0.2794, 0.2032, 2, 0.15).unwrap());
    let tab = tabulate_bemt(&model, RHO, MU).unwrap();
    assert!(tab.max_ct_error < 0.03, "{}", tab.max_ct_error);
    assert!(tab.low_re_fraction > 0.0 && tab.low_re_fraction < 1.0);
    for (omega, v) in [(300.0, 6.0), (450.0, 12.0), (550.0, 18.0), (500.0, 0.0)] {
        let a = model.loads(omega, v, RHO, MU).unwrap();
        let b = tab.model.loads(omega, v, RHO, MU).unwrap();
        assert!(
            (b.thrust_n / a.thrust_n - 1.0).abs() < 0.05
                && (b.p_shaft_w / a.p_shaft_w - 1.0).abs() < 0.05,
            "omega {omega} v {v}: T {} vs {}, P {} vs {}",
            b.thrust_n,
            a.thrust_n,
            b.p_shaft_w,
            a.p_shaft_w
        );
    }
}
