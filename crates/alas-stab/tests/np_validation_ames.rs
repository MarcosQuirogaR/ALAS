// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rigid neutral point and lift slope against two measured large-scale
//! transports: NASA TN D-3797 (T-tail, aft pods) and NACA RM
//! A58H12 (low tail, four wing pods, the 747 class), both Ames 40 x 80 ft,
//! Mach about 0.1. Geometry and readings are documented in the probes
//! `alas-pipeline/examples/sm1_validation_tnd3797.rs` and
//! `np747_validation_rma58h12.rs`; the surfaces are meshed spanwise as the
//! product builder meshes a preset (24 and 6 panels per semispan).
//!
//! Tolerances are the evidence, not a fit: plot readings (about +-0.01 MAC on
//! the measured slopes), drawing-scaled fuselage and nacelle stations, and a
//! residual of +0.034 MAC on A58H12 that no sourced term explains. The test
//! fails if a change to the chain moves either model outside them, and it
//! documents that a one-strip-per-semispan wing is not a valid stand-in.

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{SpacingFunction, Wing, WingXSec};
use alas_stab::neutral_point::{
    neutral_point_conditions, NeutralPointConditions, NpConditionsInput,
};

const FT: f64 = 0.3048;
const WING_PANELS: usize = 24;
const TAIL_PANELS: usize = 6;

fn section(x: f64, y: f64, z: f64, chord: f64, twist: f64) -> WingXSec {
    WingXSec::new(
        [x * FT, y * FT, z * FT],
        chord * FT,
        twist,
        Airfoil::from_name("naca0012").unwrap(),
    )
}

fn meshed(name: &str, sections: Vec<WingXSec>, panels: usize) -> Wing {
    Wing::new(name, sections, true)
        .mesh_spanwise(panels, SpacingFunction::Linspace)
        .unwrap()
}

fn ellipse(x: f64, y: f64, z: f64, w: f64, h: f64) -> FuselageXSec {
    FuselageXSec::new(
        [x * FT, y * FT, z * FT],
        None,
        Some(w * FT),
        Some(h * FT),
        DEFAULT_SHAPE,
    )
    .unwrap()
}

/// Trapezoid of area `s`, aspect ratio `ar`, taper `taper`, quarter-chord
/// sweep 35 deg: `(span, root chord, tip chord, tip LE offset, MAC, y_mac)`.
fn trapezoid(s: f64, ar: f64, taper: f64) -> (f64, f64, f64, f64, f64, f64) {
    let b = (s * ar).sqrt();
    let cr = 2.0 * s / (b * (1.0 + taper));
    let ct = taper * cr;
    let tip_le = b / 2.0 * 35.0_f64.to_radians().tan() + 0.25 * (cr - ct);
    let mac = 2.0 / 3.0 * cr * (1.0 + taper + taper * taper) / (1.0 + taper);
    let y_mac = b / 6.0 * (1.0 + 2.0 * taper) / (1.0 + taper);
    (b, cr, ct, tip_le, mac, y_mac)
}

/// NACA RM A58H12, tail on, nacelles on; `wing_panels` spanwise panels on the
/// wing. Moment centre (0.25 of the 7.15 ft MAC) at the origin of the result.
fn rm_a58h12(wing_panels: usize) -> Airplane {
    let (b, cr, ct, tip_le, _, _) = trapezoid(302.0, 7.0, 0.3);
    let half = b / 2.0;
    let x_mc = 23.2;
    let x_apex = x_mc - 9.95 * 35.0_f64.to_radians().tan() - 0.25 * cr;
    let z_root = -2.07;
    let z_tip = z_root + half * 6.0_f64.to_radians().tan();
    let wing = meshed(
        "Main Wing",
        vec![
            section(x_apex, 0.0, z_root, cr, 2.0),
            section(x_apex + tip_le, half, z_tip, ct, 2.0),
        ],
        wing_panels,
    );
    let (bt, crt, ctt, tip_t, mac_t, y_t) = trapezoid(62.8, 3.7, 0.375);
    let x_root = x_mc + 21.46 - 0.25 * mac_t - y_t * tip_t / (bt / 2.0);
    let tail = meshed(
        "Horizontal Stabilizer",
        vec![
            section(x_root, 0.0, 1.08, crt, 0.0),
            section(x_root + tip_t, bt / 2.0, 1.08, ctt, 0.0),
        ],
        TAIL_PANELS,
    );
    let body = [
        (0.0, 0.05),
        (1.0, 0.45),
        (2.5, 0.68),
        (5.0, 0.88),
        (8.0, 1.0),
        (25.7, 1.0),
        (33.0, 0.78),
        (40.0, 0.55),
        (46.0, 0.30),
        (49.5, 0.08),
    ];
    let mut fuselages = vec![Fuselage::new(
        "Fuselage",
        body.iter()
            .map(|&(x, k)| ellipse(x, 0.0, 0.0, 4.0 * k, 5.0 * k))
            .collect(),
    )];
    for (y, x_inlet) in [(9.2, 14.65), (16.10, 20.45)] {
        for side in [-1.0, 1.0] {
            fuselages.push(Fuselage::new(
                "Nacelle",
                [(0.0, 0.85), (1.5, 1.0), (6.6, 0.6)]
                    .iter()
                    .map(|&(dx, k)| ellipse(x_inlet + dx, side * y, -3.0, 1.85 * k, 1.85 * k))
                    .collect(),
            ));
        }
    }
    Airplane {
        name: "NACA RM A58H12".to_owned(),
        xyz_ref: [x_mc * FT, 0.0, 0.0],
        wings: vec![wing, tail],
        fuselages,
        s_ref: 302.0 * FT * FT,
        c_ref: 7.15 * FT,
        b_ref: b * FT,
    }
}

/// NASA TN D-3797 with its published planform (inboard trailing-edge chord
/// extension); `with_tail` selects tail on/off, nacelles on. Moment centre
/// (0.44 of the 7.96 ft MAC) at `xyz_ref`.
fn tn_d3797(with_tail: bool) -> Airplane {
    let half = 18.31_f64;
    let tan_le = 38.5_f64.to_radians().tan();
    let (x_apex, y_kink, c_root, c_tip) = (14.75, 6.73, 12.59, 2.91);
    let dihedral = 3.0_f64.to_radians().tan();
    let z = -1.25;
    let wing = meshed(
        "Main Wing",
        vec![
            section(x_apex, 0.0, z, c_root, 0.0),
            section(
                x_apex + y_kink * tan_le,
                y_kink,
                z + y_kink * dihedral,
                c_root - y_kink * tan_le,
                0.0,
            ),
            section(
                x_apex + half * tan_le,
                half,
                z + half * dihedral,
                c_tip,
                0.0,
            ),
        ],
        WING_PANELS,
    );
    let x_ref = x_apex + 5.81 * tan_le + 0.44 * 7.96;
    let mut wings = vec![wing];
    if with_tail {
        let (st, art, tt) = (60.0_f64, 3.23_f64, 0.40_f64);
        let bt = (st * art).sqrt();
        let cr = 2.0 * st / (bt * (1.0 + tt));
        let ct = tt * cr;
        let tip_off = bt / 2.0 * 35.0_f64.to_radians().tan() + 0.25 * (cr - ct);
        let mac_t = 2.0 / 3.0 * cr * (1.0 + tt + tt * tt) / (1.0 + tt);
        let y_t = bt / 6.0 * (1.0 + 2.0 * tt) / (1.0 + tt);
        let x_root_le = x_ref + 20.3 - 0.25 * mac_t - y_t * tip_off / (bt / 2.0);
        wings.push(meshed(
            "Horizontal Stabilizer",
            vec![
                section(x_root_le, 0.0, 10.92, cr, 0.0),
                section(x_root_le + tip_off, bt / 2.0, 10.92, ct, 0.0),
            ],
            TAIL_PANELS,
        ));
    }
    let round = |name: &str, stations: &[(f64, f64, f64)]| {
        Fuselage::new(
            name,
            stations
                .iter()
                .map(|&(x, y, r)| ellipse(x, y, 0.0, 2.0 * r, 2.0 * r))
                .collect(),
        )
    };
    let mut fuselages = vec![round(
        "Fuselage",
        &[
            (0.0, 0.0, 0.05),
            (0.6, 0.0, 0.9),
            (1.6, 0.0, 1.55),
            (3.0, 0.0, 1.9),
            (4.75, 0.0, 2.03),
            (30.75, 0.0, 2.03),
            (36.0, 0.0, 1.5),
            (40.0, 0.0, 0.8),
            (43.4, 0.0, 0.25),
        ],
    )];
    for side in [-1.0, 1.0] {
        fuselages.push(round(
            "Nacelle",
            &[
                (30.2, side * 2.5, 0.62),
                (33.2, side * 2.5, 0.78),
                (36.2, side * 2.5, 0.55),
            ],
        ));
    }
    Airplane {
        name: "TN D-3797".to_owned(),
        xyz_ref: [x_ref * FT, 0.0, 0.0],
        wings,
        fuselages,
        s_ref: 249.0 * FT * FT,
        c_ref: 7.96 * FT,
        b_ref: 2.0 * half * FT,
    }
}

/// Tail dynamic-pressure ratio the two wind-tunnel validations state: the
/// value NASA CR-2144's Boeing 747 flight derivatives bound (0.75 to 0.80 of
/// the lattice isolated-tail slope; `CL_q^2 / (2 Cm_q)`). The product's own
/// default is the geometry's wing wake (about 1 for these tails), which the
/// second pair of tests below pins as a residual against the same data.
const MEASURED_TAIL_EFFICIENCY: f64 = 0.78;

fn product_analysis(tail_efficiency: Option<f64>) -> AnalysisConfig {
    let mut analysis = AnalysisConfig::default();
    analysis.tail_efficiency = tail_efficiency;
    analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
    analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
    analysis
}

fn np(plane: &Airplane, mach: f64) -> NeutralPointConditions {
    np_at(plane, mach, Some(MEASURED_TAIL_EFFICIENCY))
}

fn np_at(plane: &Airplane, mach: f64, tail_efficiency: Option<f64>) -> NeutralPointConditions {
    let input = NpConditionsInput {
        low_speed_mach: mach,
        low_speed_altitude_m: 0.0,
        cruise_mach: mach,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    neutral_point_conditions(plane, &product_analysis(tail_efficiency), &input).unwrap()
}

/// The neutral point of the uncorrected chain, m: the product value less the
/// flagged tunnel-data correction (fleet-wide), which is fitted to transport
/// configurations and excludes these two simple circular-body Ames models
/// (`alas_stab::neutral_point::tunnel_ac_correction`). The pins below hold the
/// uncorrected chain; `the_tunnel_data_correction_moves_the_ames_models_forward`
/// pins the corrected values.
fn chain(c: &NeutralPointConditions) -> f64 {
    let d = &c.diagnostics;
    c.clean_low_speed - d.wing_lift_fraction.clamp(0.0, 1.0) * d.fuselage.tunnel_correction_shift_m
}

/// `x` (m) as a fraction of the MAC, given the moment centre's own fraction.
fn mac_fraction(plane: &Airplane, x_m: f64, reference_fraction: f64) -> f64 {
    reference_fraction + (x_m - plane.xyz_ref[0]) / plane.c_ref
}

#[test]
fn tn_d3797_neutral_points_and_lift_slope_match_the_test() {
    let on = tn_d3797(true);
    let tail_on = np(&on, 0.1);
    let np_on = mac_fraction(&on, chain(&tail_on), 0.44);
    // Published 0.715 (27.5 % margin at 0.44 MAC, page 5 text).
    assert!((np_on - 0.715).abs() < 0.03, "tail-on NP {np_on:.3}");
    // Fig. 11a: tail-on CL_alpha about 0.0765 per deg (4.4 per rad).
    let cla = tail_on.diagnostics.cl_alpha_total;
    assert!((cla / 4.4 - 1.0).abs() < 0.10, "tail-on CLa {cla:.3}");
    let off = tn_d3797(false);
    let np_off = mac_fraction(&off, chain(&np(&off, 0.1)), 0.44);
    // Fig. 11a: tail-off dCm/dCL about 0 about 0.44 MAC.
    assert!((np_off - 0.44).abs() < 0.04, "tail-off NP {np_off:.3}");
}

#[test]
fn rm_a58h12_neutral_point_and_lift_slope_match_the_test() {
    let plane = rm_a58h12(WING_PANELS);
    let result = np(&plane, 0.08);
    let x_np = mac_fraction(&plane, chain(&result), 0.25);
    // Fig. 13a, R 4.8-8.1 million: dCm/dCL -0.204, NP 0.454 MAC. The model
    // sits 0.03 aft of it (unexplained residual, stated in the brief).
    assert!((x_np - 0.454).abs() < 0.05, "NP {x_np:.3}");
    let cla = result.diagnostics.cl_alpha_total;
    assert!((cla / 4.4 - 1.0).abs() < 0.10, "CLa {cla:.3}");
}

/// The same two models with the tail left to the product default, the
/// free-stream dynamic pressure (`tail_efficiency` unset, no tail ratio on the
/// surface; the wing-wake model gives 1 for both tails). The neutral point
/// lies aft of the published one by 0.060 (TN D-3797) and 0.061 (A58H12) of
/// the mean chord: the lattice with an undiminished tail overpredicts the
/// stability of these tunnel models, and no sourced loss closes it (the wing
/// wake misses both tails; the fuselage boundary layer has no method). These
/// pins state the residual, they do not excuse it: a model change that moves
/// it must be read against the measurements above.
#[test]
fn the_free_stream_tail_overpredicts_the_measured_neutral_points() {
    let on = tn_d3797(true);
    let np_on = mac_fraction(&on, chain(&np_at(&on, 0.1, None)), 0.44);
    assert!((np_on - 0.775).abs() < 0.005, "tail-on NP {np_on:.3}");
    assert!(np_on - 0.715 > 0.05);
    let plane = rm_a58h12(WING_PANELS);
    let x_np = mac_fraction(&plane, chain(&np_at(&plane, 0.08, None)), 0.25);
    assert!((x_np - 0.515).abs() < 0.005, "NP {x_np:.3}");
    assert!(x_np - 0.454 > 0.05);
}

/// One spanwise strip per semispan (the former probe mesh) is not a valid
/// stand-in: the swept wing's lattice aerodynamic centre moves aft by more
/// than 0.1 MAC against the builder's 24 panels, which itself is within
/// 0.01 MAC of 96 panels.
#[test]
fn a_swept_wing_needs_the_builder_spanwise_mesh_for_its_aerodynamic_centre() {
    let atm = Atmosphere::new(0.0);
    let op = |a: f64| OperatingPoint::new(atm, 0.08 * atm.speed_of_sound(), a, 0.0, 0.0, 0.0, 0.0);
    let wing_ac = |panels: usize| {
        let mut alone = rm_a58h12(panels);
        alone.wings.truncate(1);
        alone.fuselages.clear();
        let system = VlmSystem::assemble(&alone, 1, 8).unwrap();
        let (lo, hi) = (
            system.solve(&op(0.0)).unwrap(),
            system.solve(&op(2.0)).unwrap(),
        );
        let shift = -(hi.cm_pitch - lo.cm_pitch) / (hi.cl_lift - lo.cl_lift);
        0.25 + shift
    };
    let (one, builder, fine) = (wing_ac(1), wing_ac(WING_PANELS), wing_ac(96));
    assert!(
        one - builder > 0.1,
        "1 strip {one:.3}, 24 panels {builder:.3}"
    );
    assert!(
        (builder - fine).abs() < 0.01,
        "24 {builder:.3}, 96 {fine:.3}"
    );
}

/// Both tunnel models are wing-body-tail airplanes (TN D-3797: fuselage and
/// aft pods; A58H12: fuselage and four wing pods), so the body term is in the
/// numbers above. The product sums Torenbeek's pair (E-37 `D1` + E-38 `D2`)
/// with the nacelle term E-41; the DATCOM 4.2.2.1 strip method (Multhopp) and
/// the NACA TR 711 `K_fus` correlation (Raymer Eq. 16.25) are reported as
/// cross-checks only. Their shifts of the neutral point, in MAC at the
/// wing-wake tail ratio, against the published 0.715 and 0.454:
///
/// | body term       | TN D-3797 | A58H12 |
/// |-----------------|-----------|--------|
/// | Torenbeek pair  | 0.775     | 0.515  |
/// | Multhopp strip  | 0.716     | 0.462  |
/// | TR 711 K_fus    | 0.689     | 0.456  |
///
/// No sourced body term closes both models to 0.02 MAC. The strip method and
/// `K_fus` agree with each other to a point of MAC on conventional jets, and
/// sit 5 to 9 points of MAC forward of Torenbeek's pair on them; this pins
/// the numbers, it does not choose among them.
#[test]
fn the_body_term_alternatives_bracket_the_measured_neutral_points() {
    let rows = [
        (tn_d3797(true), 0.1, 0.44, [0.775, 0.716, 0.689]),
        (rm_a58h12(WING_PANELS), 0.08, 0.25, [0.515, 0.462, 0.456]),
    ];
    for (plane, mach, reference, expected) in rows {
        let input = NpConditionsInput {
            low_speed_mach: mach,
            low_speed_altitude_m: 0.0,
            cruise_mach: mach,
            cruise_altitude_m: 0.0,
            include_flexibility: false,
            wing_stiffness: None,
            tail_flexibility: None,
            high_lift: None,
            propeller: None,
            jet_inlets: None,
            max_operating: None,
        };
        let c = neutral_point_conditions(&plane, &product_analysis(None), &input).unwrap();
        let d = &c.diagnostics;
        let nacelle = d.nacelle_shift_m;
        let alternatives = [
            chain(&c),
            d.x_np_surfaces + d.fuselage.multhopp_shift_m + nacelle,
            d.x_np_surfaces + d.fuselage.gilruth_shift_m + nacelle,
        ];
        for (x, want) in alternatives.into_iter().zip(expected) {
            let got = mac_fraction(&plane, x, reference);
            assert!(
                (got - want).abs() < 0.005,
                "{} NP {got:.3} vs {want}",
                plane.name
            );
        }
    }
}

/// The tail-off model discriminates the body terms: Fig. 11a gives
/// `dCm/dCL` about 0 about 0.44 MAC for TN D-3797 without its tail, a wing-body
/// aerodynamic centre at 0.44. The lattice with Torenbeek's wing-body lift
/// interference `K_I` (E-33, p. 479) gives 0.443 and the product's body terms
/// (Torenbeek E-37 + E-38, nacelles E-41) 0.441; the DATCOM
/// 4.2.2.1 strip method would give 0.374 and the TR 711 `K_fus` correlation
/// 0.342, 0.06 and 0.10 MAC forward of the measurement. So the tail-on excess
/// of the table above (0.775 against 0.715) is in the tail increment, not in
/// the body term: the measured tail adds 0.275 MAC to the neutral point, the
/// lattice with a free-stream tail 0.345, a ratio of 0.80 (about the 0.78 the
/// former default carried). The model's downwash gradient (0.24; DATCOM 4.4.1
/// gives 0.27) does not explain it.
#[test]
fn the_tail_off_model_selects_torenbeeks_body_term_over_the_strip_method() {
    let off = tn_d3797(false);
    let input = NpConditionsInput {
        low_speed_mach: 0.1,
        low_speed_altitude_m: 0.0,
        cruise_mach: 0.1,
        cruise_altitude_m: 0.0,
        include_flexibility: false,
        wing_stiffness: None,
        tail_flexibility: None,
        high_lift: None,
        propeller: None,
        jet_inlets: None,
        max_operating: None,
    };
    let c = neutral_point_conditions(&off, &product_analysis(None), &input).unwrap();
    let d = &c.diagnostics;
    let nacelle = d.nacelle_shift_m;
    let at = |x: f64| mac_fraction(&off, x, 0.44);
    assert!((at(d.x_np_surfaces) - 0.443).abs() < 0.005);
    assert!((at(chain(&c)) - 0.441).abs() < 0.005);
    assert!((at(d.x_np_surfaces + d.fuselage.multhopp_shift_m + nacelle) - 0.374).abs() < 0.005);
    assert!((at(d.x_np_surfaces + d.fuselage.gilruth_shift_m + nacelle) - 0.342).abs() < 0.005);
}

/// The flagged tunnel-data correction (fleet-wide) moves both Ames models
/// forward of their measurements: they are simple circular bodies without
/// wing nacelles at Reynolds numbers 4.5e6-6.5e6 and are not in its fit set.
/// Product values with the correction, at the measured tail ratio: TN D-3797
/// tail-on 0.65 (measured 0.715) and tail-off 0.38 (0.44); RM A58H12 0.40
/// (0.454).
#[test]
fn the_tunnel_data_correction_moves_the_ames_models_forward() {
    let on = tn_d3797(true);
    let off = tn_d3797(false);
    let a58 = rm_a58h12(WING_PANELS);
    let corrected = [
        (mac_fraction(&on, np(&on, 0.1).clean_low_speed, 0.44), 0.715),
        (
            mac_fraction(&off, np(&off, 0.1).clean_low_speed, 0.44),
            0.44,
        ),
        (
            mac_fraction(&a58, np(&a58, 0.08).clean_low_speed, 0.25),
            0.454,
        ),
    ];
    for (got, measured) in corrected {
        let delta = got - measured;
        assert!(
            (-0.09..-0.03).contains(&delta),
            "corrected NP {got:.3} vs measured {measured}"
        );
    }
}
