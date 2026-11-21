// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail-off wing-body aerodynamic centre of public tunnel models through the
//! product chain, term by term, with the residual against each measurement
//! Sources and plot reads: NASA TM X-1165,
//! TM X-3149, TM X-3276, TP-1580, CR-134184 (C-5A), CR-172278 (L-1011),
//! CR-147641 (747-100), CR-3677 (DC-10). Geometry of the L-1011, 747 and DC-10
//! rows is the standard full-scale figure where the report prints none.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    missing_docs
)]

use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{SpacingFunction, Wing, WingXSec};
use alas_stab::neutral_point::{neutral_point_conditions, tunnel_ac_correction, NpConditionsInput};

struct Model {
    name: &'static str,
    measured: f64,
    sweep_c4_deg: f64,
    ar: f64,
    taper: f64,
    s: f64,
    b_f: f64,
    h_f: f64,
    l_f: f64,
    l_fn: f64,
    /// (x inlet from nose, y, diameter, length), one side.
    nacelles: Vec<(f64, f64, f64, f64)>,
    mach: f64,
    role: Role,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    /// Public tail-off data with a published planform.
    Fit,
    /// The two wide-bodies the lane validates (747-100, DC-10).
    HoldOut,
    /// Grade C, reported only.
    Excluded,
}

fn ellipse(x: f64, y: f64, z: f64, w: f64, h: f64) -> FuselageXSec {
    FuselageXSec::new([x, y, z], None, Some(w), Some(h), DEFAULT_SHAPE).unwrap()
}

/// Returns the airplane and its trapezoid MAC (chord, x of c/4).
fn build(model: &Model) -> (Airplane, f64, f64) {
    // Every term is scale-invariant; scale a tunnel model to about 300 m2 so
    // no reference-dimension floor (1 m2, 1 m span) touches it.
    let k = (300.0 / model.s).sqrt();
    let m = &Model {
        name: model.name,
        measured: model.measured,
        sweep_c4_deg: model.sweep_c4_deg,
        ar: model.ar,
        taper: model.taper,
        s: model.s * k * k,
        b_f: model.b_f * k,
        h_f: model.h_f * k,
        l_f: model.l_f * k,
        l_fn: model.l_fn * k,
        nacelles: model
            .nacelles
            .iter()
            .map(|&(x, y, d, l)| (x * k, y * k, d * k, l * k))
            .collect(),
        mach: model.mach,
        role: model.role,
    };
    let b = (m.s * m.ar).sqrt();
    let cr = 2.0 * m.s / (b * (1.0 + m.taper));
    let ct = m.taper * cr;
    let half = b / 2.0;
    let tan_c4 = m.sweep_c4_deg.to_radians().tan();
    let tan_le = tan_c4 + 0.25 * (cr - ct) / half;
    let mac = 2.0 / 3.0 * cr * (1.0 + m.taper + m.taper * m.taper) / (1.0 + m.taper);
    let y_mac = b / 6.0 * (1.0 + 2.0 * m.taper) / (1.0 + m.taper);
    // Nose at x = 0: the leading edge meets the body side at l_fn.
    let x_apex = m.l_fn - 0.5 * m.b_f * tan_le;
    let x_c4_mac = x_apex + y_mac * tan_le + 0.25 * mac;
    let af = Airfoil::from_name("naca0012").unwrap();
    let wing = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([x_apex, 0.0, 0.0], cr, 0.0, af.clone()),
            WingXSec::new([x_apex + half * tan_le, half, 0.0], ct, 0.0, af),
        ],
        true,
    )
    .mesh_spanwise(24, SpacingFunction::Linspace)
    .unwrap();
    let body = [
        (0.0, 0.05),
        (0.04, 0.55),
        (0.10, 0.9),
        (0.15, 1.0),
        (0.62, 1.0),
        (0.80, 0.7),
        (0.93, 0.35),
        (1.0, 0.08),
    ];
    let mut fuselages = vec![Fuselage::new(
        "Fuselage",
        body.iter()
            .map(|&(f, k)| ellipse(f * m.l_f, 0.0, 0.0, m.b_f * k, m.h_f * k))
            .collect(),
    )];
    for &(x, y, d, l) in &m.nacelles {
        for side in [-1.0, 1.0] {
            fuselages.push(Fuselage::new(
                "Nacelle",
                [(0.0, 0.85), (0.2, 1.0), (1.0, 0.6)]
                    .iter()
                    .map(|&(f, k)| ellipse(x + f * l, side * y, -0.3 * d, d * k, d * k))
                    .collect(),
            ));
        }
    }
    let plane = Airplane {
        name: m.name.to_owned(),
        xyz_ref: [x_c4_mac, 0.0, 0.0],
        wings: vec![wing],
        fuselages,
        s_ref: m.s,
        c_ref: mac,
        b_ref: b,
    };
    (plane, mac, x_c4_mac)
}

fn models() -> Vec<Model> {
    let inch = 0.0254;
    let ft = 0.3048;
    let x1165 = |name: &'static str, measured: f64, b_f: f64, h_f: f64, l_fn: f64| Model {
        name,
        measured,
        sweep_c4_deg: 24.5,
        ar: 7.8,
        taper: 0.253,
        s: 1.92 * ft * ft,
        b_f: b_f * inch,
        h_f: h_f * inch,
        l_f: 49.5 * inch,
        l_fn: l_fn * inch,
        nacelles: vec![],
        mach: 0.21,
        role: Role::Fit,
    };
    vec![
        x1165(
            "TM X-1165 B1 (24.5 deg, pear body)",
            0.145,
            5.80,
            6.50,
            19.8,
        ),
        x1165(
            "TM X-1165 B3 (24.5 deg, circular body)",
            0.12,
            6.0,
            6.0,
            19.8,
        ),
        x1165(
            "TM X-1165 B4 (24.5 deg, small body)",
            0.18,
            4.24,
            5.62,
            19.4,
        ),
        Model {
            name: "C-5A CR-134184 (24 deg, high wing)",
            measured: 0.14,
            sweep_c4_deg: 24.27,
            ar: 7.744,
            taper: 0.371,
            s: 9.878 * ft * ft,
            b_f: 12.69 * inch,
            h_f: 12.69 * inch,
            l_f: 110.49 * inch,
            l_fn: 35.4 * inch,
            nacelles: vec![
                (35.58 * inch, 19.0 * inch, 4.09 * inch, 9.23 * inch),
                (41.10 * inch, 29.7 * inch, 4.09 * inch, 9.23 * inch),
            ],
            mach: 0.2,
            role: Role::Fit,
        },
        Model {
            name: "TM X-3149 (42 deg, no nacelles)",
            measured: 0.15,
            sweep_c4_deg: 42.27,
            ar: 6.394,
            taper: 0.39,
            s: 0.530,
            b_f: 0.20,
            h_f: 0.20,
            l_f: 2.204,
            l_fn: 0.91,
            nacelles: vec![],
            role: Role::Fit,
            mach: 0.18,
        },
        Model {
            name: "TM X-3276 (42 deg, no nacelles)",
            measured: 0.12,
            sweep_c4_deg: 42.33,
            ar: 7.046,
            taper: 0.354,
            s: 0.656,
            b_f: 0.28,
            h_f: 0.28,
            l_f: 2.827,
            l_fn: 1.08,
            role: Role::Fit,
            nacelles: vec![],
            mach: 0.18,
        },
        Model {
            name: "TP-1580 EET AR10 (27 deg, no nacelles modelled)",
            measured: 0.22,
            sweep_c4_deg: 27.0,
            ar: 10.0,
            taper: 0.41,
            s: 1.04,
            b_f: 0.3505,
            h_f: 0.3505,
            l_f: 3.02,
            role: Role::Excluded,
            l_fn: 1.28,
            nacelles: vec![],
            mach: 0.15,
        },
        Model {
            name: "747-100 CR-147641 generic (37.5 deg, 4 pods) HOLD-OUT",
            measured: 0.167,
            sweep_c4_deg: 37.5,
            ar: 6.96,
            taper: 0.28,
            s: 511.0,
            b_f: 6.50,
            h_f: 7.8,
            role: Role::HoldOut,
            l_f: 68.6,
            l_fn: 22.0,
            nacelles: vec![(23.1, 11.7, 2.5, 7.0), (32.2, 21.0, 2.5, 7.0)],
            mach: 0.15,
        },
        Model {
            name: "DC-10 generic (35 deg, 2 pods) HOLD-OUT (meas. M0.65)",
            measured: 0.245,
            sweep_c4_deg: 35.0,
            ar: 7.5,
            taper: 0.25,
            s: 338.8,
            b_f: 6.02,
            role: Role::HoldOut,
            h_f: 6.02,
            l_f: 52.0,
            l_fn: 20.5,
            nacelles: vec![(19.5, 8.6, 2.7, 6.5)],
            mach: 0.2,
        },
        Model {
            name: "L-1011 generic (35 deg, 2 pods; book M0.2)",
            measured: 0.197,
            sweep_c4_deg: 35.0,
            ar: 6.95,
            taper: 0.27,
            s: 321.1,
            role: Role::Fit,
            b_f: 5.97,
            h_f: 5.97,
            l_f: 54.2,
            l_fn: 19.5,
            nacelles: vec![(18.5, 9.4, 2.6, 6.5)],
            mach: 0.2,
        },
    ]
}

/// The product's tail-off aerodynamic centre (fraction of the trapezoid's
/// own MAC) and its terms: `(h_wb, wing, D1, share x D2, nacelles)`.
fn tail_off(m: &Model) -> (f64, f64, f64, f64, f64) {
    let mut analysis = AnalysisConfig::default();
    analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
    analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
    let (plane, mac, x_c4) = build(m);
    let input = NpConditionsInput {
        low_speed_mach: m.mach,
        low_speed_altitude_m: 0.0,
        cruise_mach: m.mach,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    let c = neutral_point_conditions(&plane, &analysis, &input).unwrap();
    let d = &c.diagnostics;
    let f = &d.fuselage;
    // The uncorrected chain: the flagged tunnel-data correction is fitted to
    // these residuals, so it is taken out here.
    let tunnel = d.wing_lift_fraction.clamp(0.0, 1.0) * f.tunnel_correction_shift_m;
    (
        0.25 + (c.clean_low_speed - tunnel - x_c4) / mac,
        0.25 + (d.x_wing_alone_ac - x_c4) / mac,
        f.torenbeek_d1_shift_m / mac,
        d.wing_lift_fraction * f.torenbeek_d2_shift_m / mac,
        d.nacelle_shift_m / mac,
    )
}

#[test]
#[ignore = "diagnostic table"]
fn tail_off_table() {
    for m in models() {
        let (h, wing, d1, d2, nac) = tail_off(&m);
        println!(
            "{:<58} {:?} meas {:.3} | wing {:.3} D1 {:+.3} D2 {:+.3} nac {:+.3} -> h_wb {:.3} | resid {:+.3}",
            m.name, m.role, m.measured, wing, d1, d2, nac, h, m.measured - h
        );
    }
}

/// The transport-configuration rows (TM X-1165 x3, C-5A, TM X-3149,
/// TM X-3276, L-1011 data book) all sit forward of the uncorrected product
/// chain, by 0.067 MAC on average (scatter 0.029), at every sweep from 24 to
/// 42 deg: these residuals are the fit set of the flagged tunnel-data
/// correction (fleet-wide), `alas_stab::neutral_point::tunnel_ac_correction`,
/// `dh = -0.0189 - 0.0779 tan(Lambda_c/4)`. With it the fit rows are centred
/// (mean within 0.005) and the 747-100 hold-out (NASA CR-147641, 0.167) moves
/// from -0.098 to about -0.02. The Ames models (`np_validation_ames.rs`) are
/// not in the set: simple circular bodies, no wing nacelles, high Reynolds
/// number.
#[test]
fn transport_tail_off_data_sit_forward_of_the_chain_and_fix_the_tunnel_correction() {
    let mut residuals = Vec::new();
    let mut corrected = Vec::new();
    for m in models() {
        let (h, ..) = tail_off(&m);
        let residual = m.measured - h;
        let fix = tunnel_ac_correction(m.sweep_c4_deg.to_radians());
        if m.role == Role::Fit {
            assert!(
                (-0.12..0.0).contains(&residual),
                "{}: residual {residual:+.3}",
                m.name
            );
            residuals.push(residual);
            corrected.push(residual - fix);
        }
        if m.name.starts_with("747") {
            assert!(
                (-0.12..-0.07).contains(&residual),
                "747-100 residual {residual:+.3}"
            );
            assert!(
                (residual - fix).abs() < 0.04,
                "747-100 corrected residual {:+.3}",
                residual - fix
            );
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    assert!(
        (mean(&residuals) + 0.067).abs() < 0.01,
        "transport mean residual {:+.3}",
        mean(&residuals)
    );
    assert!(
        mean(&corrected).abs() < 0.005,
        "corrected mean residual {:+.3}",
        mean(&corrected)
    );
}
