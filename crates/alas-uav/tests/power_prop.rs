// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Propeller map consumer and BEMT verification.
//!
//! Analytic references: actuator-disk momentum theory (T = 2 rho A (V + u) u,
//! ideal power T (V + u), static ideal power T^1.5 / sqrt(2 rho A)), the
//! coefficient definitions of the UIUC propeller database (UIUC Propeller Data
//! Site, https://m-selig.ae.illinois.edu/props/propDB.html, page read
//! 2026-10-12: CT = T/(rho n^2 D^4), CP = P/(rho n^3 D^5), J = V/(n D)), and
//! the identity eta = J CT / CP.  The PER3 consumer test uses the SYNTHETIC
//! fixtures in `tests/fixtures/prop_data` (made-up numbers).  UIUC measured
//! data are NOT compared here: they need the blade geometry (APC supplies
//! it separately, see the UIUC page) and permission; that is the M6 V2 case.

use std::sync::Arc;

use alas_uav::power::{BemtGeometry, PropMapTable, PropModel, PropSource, SectionPolar};
use alas_uav_db::{ComponentId, IsoDate, PerfKind, PropPerfRow, PropPerfSet, SourceId};

const RHO: f64 = 1.225;
const MU: f64 = 1.789e-5;

/// Bilinear synthetic map: CT = (0.12 - 0.08 J)(1 + 1e-4 omega/10), CP likewise.
fn synthetic_set(kind: PerfKind) -> PropPerfSet {
    let mut rows = Vec::new();
    for omega in [300.0, 500.0, 700.0] {
        for j in [0.0, 0.2, 0.4, 0.6, 0.8] {
            let s = 1.0 + 1.0e-5 * omega;
            rows.push(PropPerfRow {
                omega_rad_s: omega,
                j,
                re: None,
                ct: (0.12 - 0.08 * j) * s,
                cp: (0.05 - 0.03 * j) * s,
                eta: None,
            });
        }
    }
    PropPerfSet {
        prop_id: ComponentId::new("synthetic-10x5").unwrap(),
        source_id: SourceId::new("synthetic").unwrap(),
        kind,
        retrieved_at: Some(IsoDate::parse("2026-10-12").unwrap()),
        rows,
    }
}

#[test]
fn map_reproduces_a_bilinear_law_exactly_and_does_not_extrapolate() {
    let table = PropMapTable::from_set(&synthetic_set(PerfKind::Measured), 0.254).unwrap();
    for (omega, j) in [(300.0, 0.0), (412.3, 0.37), (699.0, 0.79), (550.0, 0.5)] {
        let s = 1.0 + 1.0e-5 * omega;
        let (ct, cp) = table.coefficients(omega, j).unwrap();
        assert!((ct - (0.12 - 0.08 * j) * s).abs() < 1.0e-12);
        assert!((cp - (0.05 - 0.03 * j) * s).abs() < 1.0e-12);
    }
    assert!(table.coefficients(299.0, 0.3).is_none());
    assert!(table.coefficients(701.0, 0.3).is_none());
    assert!(table.coefficients(500.0, 0.81).is_none());
    assert!(table.coefficients(500.0, -0.1).is_none());
    assert!(table.coefficients(f64::NAN, 0.3).is_none());
}

#[test]
fn map_loads_follow_the_uiuc_definitions_and_eta_identity() {
    let model = PropModel::from_map(
        PropMapTable::from_set(&synthetic_set(PerfKind::Measured), 0.254).unwrap(),
        true,
    );
    assert_eq!(model.source(), PropSource::MeasuredMap);
    let (omega, v) = (500.0, 10.0);
    let l = model.loads(omega, v, RHO, MU).unwrap();
    let n = omega / std::f64::consts::TAU;
    assert!((l.j - v / (n * 0.254)).abs() < 1.0e-12);
    assert!((l.thrust_n - l.ct * RHO * n * n * 0.254_f64.powi(4)).abs() < 1.0e-12);
    assert!((l.p_shaft_w - l.cp * RHO * n.powi(3) * 0.254_f64.powi(5)).abs() < 1.0e-12);
    assert!((l.torque_n_m * omega - l.p_shaft_w).abs() < 1.0e-12);
    // eta = J CT / CP equals T V / P.
    assert!((l.j * l.ct / l.cp - l.thrust_n * v / l.p_shaft_w).abs() < 1.0e-12);
    // Outside the map: an error, never an extrapolated number.
    assert!(model.loads(500.0, 40.0, RHO, MU).is_err());
}

/// Thin-airfoil section without drag or stall, for the momentum comparison.
#[derive(Debug)]
struct IdealSection;

impl SectionPolar for IdealSection {
    fn cl_cd(&self, alpha_rad: f64, _re: f64) -> (f64, f64) {
        (2.0 * std::f64::consts::PI * alpha_rad, 0.0)
    }
}

fn ideal_blade(blades: u32, v: f64, omega: f64, alpha0_deg: f64) -> BemtGeometry {
    let radius = 0.127;
    let (mut r, mut c, mut th) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..=14 {
        let x = 0.15 + 0.84 * f64::from(k) / 14.0;
        r.push(x * radius);
        c.push(0.012);
        th.push((v / (omega * x * radius)).atan() + alpha0_deg.to_radians());
    }
    BemtGeometry::new(blades, r, c, th, Arc::new(IdealSection)).unwrap()
}

#[test]
fn bemt_approaches_momentum_theory_at_light_loading() {
    let (v, omega) = (20.0, 2.0 * std::f64::consts::PI * 90.0);
    let area = std::f64::consts::PI * 0.127_f64.powi(2);
    let mut last = f64::INFINITY;
    for alpha0 in [2.0, 1.0, 0.5] {
        let s = ideal_blade(8, v, omega, alpha0)
            .solve(omega, v, RHO, MU)
            .unwrap();
        assert_eq!(s.fallback_fraction, 0.0);
        let t = s.thrust_n;
        // Actuator disk: T = 2 rho A (V + u) u, ideal power T (V + u).
        let u = -0.5 * v + (0.25 * v * v + t / (2.0 * RHO * area)).sqrt();
        let ratio = s.torque_n_m * omega / (t * (v + u));
        assert!(t > 0.0 && ratio > 1.0, "T = {t}, ratio = {ratio}");
        assert!(
            ratio < last,
            "ratio must fall toward 1 with loading: {ratio} !< {last}"
        );
        last = ratio;
    }
    // No profile drag and many blades: only swirl and finite-blade tip losses remain.
    assert!(last < 1.01, "P/P_ideal at the lightest loading = {last}");
}

#[test]
fn bemt_static_figure_of_merit_is_physical() {
    // Generic 10x5 inch blade (ESTIMATED chord, pitch-line twist): a
    // plausibility band, not a validation against a measured propeller.
    let g = BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap();
    assert!(g.generic);
    let omega = 2.0 * std::f64::consts::PI * 100.0;
    let s = g.solve(omega, 0.0, RHO, MU).unwrap();
    let area = std::f64::consts::PI * 0.127_f64.powi(2);
    let p = s.torque_n_m * omega;
    let fm = s.thrust_n.powf(1.5) / (2.0 * RHO * area).sqrt() / p;
    assert!(s.thrust_n > 0.0 && p > 0.0);
    assert!((0.25..1.0).contains(&fm), "figure of merit {fm}");
    let n = 100.0;
    let ct = s.thrust_n / (RHO * n * n * 0.254_f64.powi(4));
    assert!((0.03..0.2).contains(&ct), "static CT {ct}");
}

#[test]
fn bemt_thrust_falls_with_advance_ratio_and_efficiency_peaks() {
    let model = PropModel::from_bemt(BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap());
    assert_eq!(model.source(), PropSource::BemtGeneric);
    let omega = 2.0 * std::f64::consts::PI * 120.0;
    let mut previous_ct = f64::INFINITY;
    let mut etas = Vec::new();
    for v in [0.0, 4.0, 8.0, 12.0, 16.0] {
        let l = model.loads(omega, v, RHO, MU).unwrap();
        assert!(l.ct < previous_ct, "CT must fall with J");
        previous_ct = l.ct;
        etas.push(l.j * l.ct / l.cp);
        assert!((l.thrust_n * v / l.p_shaft_w - l.j * l.ct / l.cp).abs() < 1.0e-9);
    }
    assert_eq!(etas[0], 0.0);
    assert!(etas[2] > etas[1] && etas.iter().all(|&e| e < 1.0));
}

#[test]
fn neuralfoil_polar_drives_bemt() {
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_uav::power::TabulatedPolar;
    let naca = Airfoil::from_name("naca4412").unwrap();
    let polar = TabulatedPolar::from_neuralfoil(&naca, &[5.0e4, 1.0e5, 2.0e5], (-10, 20)).unwrap();
    let (cl5, cd5) = polar.cl_cd(5.0_f64.to_radians(), 1.0e5);
    assert!(
        (0.6..1.5).contains(&cl5) && (0.005..0.05).contains(&cd5),
        "cl {cl5} cd {cd5}"
    );
    // Beyond the table it relaxes to the flat-plate law: |cl| bounded, cd large at 90 deg.
    let (cl90, cd90) = polar.cl_cd(std::f64::consts::FRAC_PI_2, 1.0e5);
    assert!(cl90.abs() < 0.2 && cd90 > 1.5);
    let base = BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap();
    let mut g = base.clone();
    for i in 0..12 {
        g.set_station_polar(i, Arc::new(polar.clone()));
    }
    let omega = 2.0 * std::f64::consts::PI * 100.0;
    let with_nf = g.solve(omega, 6.0, RHO, MU).unwrap();
    let analytic = base.solve(omega, 6.0, RHO, MU).unwrap();
    assert!(with_nf.thrust_n > 0.0);
    // Same blade, two section models: the same order of magnitude, not identical.
    assert!((0.4..2.5).contains(&(with_nf.thrust_n / analytic.thrust_n)));
}
