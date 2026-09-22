// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::neuralfoil` against NeuralFoil and AeroSandbox, via
//! `golden/generators/gen_aero_neuralfoil.py`.
//!
//! This row is checked at two tiers, and the split follows what each half of
//! it is.
//!
//! The **parameter blobs** are compared at `Tier::Exact`, by digest. They are
//! the one thing in this port that cannot be re-derived: a weight that
//! drifted would not fail any arithmetic check, it would quietly produce a
//! different aeroplane. The digest is the same FNV-1a `parity_selig.rs` uses
//! against the airfoil corpus, and for the same reason.
//!
//! Everything **computed** is compared at `Tier::Linalg`, which
//! `docs/PORTING.md` records as a tightening of this row's planned `f32`
//! tier. The planned tier came from a true statement read one step too far:
//! the trained parameters are stored as `f32`, so the row was written down as
//! `f32`. But NumPy promotes `f32 @ f64` to `f64` before multiplying, so
//! every product upstream evaluates is a double: the *values* are
//! `f32`-precision, the *arithmetic* is not. Reproducing that means holding
//! the same `f32` values and working in `f64`, which is what this port does.
//! Every one of the five thousand or so quantities this fixture records
//! agrees to better than 1e-12 relative, seven orders inside the tier that
//! was planned for it.
//!
//! Not `closed`, though it would pass there today. What this row does is
//! accumulate dot products 128 to 256 terms long, six layers deep, against a
//! BLAS whose blocking decides its own summation order: the same free
//! variable `alas-math::lstsq`'s doc declines to bet on, and the construction
//! `linalg` exists to frame. The two coordinate-taking entry points also run
//! a least-squares fit on the way in, which the tier table names outright.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::kulfan::KulfanAirfoil;
use alas_aero::neuralfoil::{
    aero_from_airfoil, aero_from_coordinates, aero_from_kulfan_airfoil,
    aero_from_kulfan_parameters, Aero, BoundaryLayer, Conditions, ModelSize, NetworkAero,
    BL_STATIONS,
};
use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::spacing::linspace;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

macro_rules! blob {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/", $name)) as &[u8],
        )
    };
}

/// Every file `gen_aero_neuralfoil.py` exports, as the crate embeds it.
const BLOBS: [(&str, &[u8]); 6] = [
    blob!("nn-small.bin"),
    blob!("nn-medium.bin"),
    blob!("nn-large.bin"),
    blob!("nn-xlarge.bin"),
    blob!("nn-xxlarge.bin"),
    blob!("nn-input-distribution.bin"),
];

#[derive(Debug, Deserialize)]
struct KulfanPayload {
    lower_weights: Vec<f64>,
    upper_weights: Vec<f64>,
    leading_edge_weight: f64,
    #[serde(rename = "TE_thickness")]
    te_thickness: f64,
}

impl KulfanPayload {
    fn airfoil(&self) -> KulfanAirfoil {
        KulfanAirfoil {
            lower_weights: self.lower_weights.clone(),
            upper_weights: self.upper_weights.clone(),
            leading_edge_weight: self.leading_edge_weight,
            te_thickness: self.te_thickness,
            n1: 0.5,
            n2: 1.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NetworkCase {
    kulfan: KulfanPayload,
    model_size: String,
    alpha: f64,
    #[serde(rename = "Re")]
    reynolds: f64,
    n_crit: f64,
    xtr_upper: f64,
    xtr_lower: f64,
    outputs: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct KulfanAirfoilCase {
    kulfan: KulfanPayload,
    model_size: String,
    alpha: f64,
    #[serde(rename = "Re")]
    reynolds: f64,
    mach: f64,
    max_thickness: f64,
    outputs: HashMap<String, f64>,
    boundary_layer: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct AirfoilCase {
    coordinates: Vec<(f64, f64)>,
    model_size: String,
    alpha: f64,
    #[serde(rename = "Re")]
    reynolds: f64,
    mach: f64,
    rotation_angle: f64,
    scale_factor: f64,
    outputs: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct CoordinatesCase {
    coordinates: Vec<(f64, f64)>,
    alpha: f64,
    #[serde(rename = "Re")]
    reynolds: f64,
    outputs: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    digests: HashMap<String, String>,
    network: HashMap<String, NetworkCase>,
    kulfan_airfoil: HashMap<String, KulfanAirfoilCase>,
    airfoil: HashMap<String, AirfoilCase>,
    coordinates: HashMap<String, CoordinatesCase>,
}

/// FNV-1a, 64-bit, as `gen_aero_neuralfoil.py`'s `_fnv1a64` computes it and
/// as `parity_selig.rs` already implements it against the airfoil corpus.
fn fnv1a64(data: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut digest = OFFSET_BASIS;
    for &byte in data {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(PRIME);
    }
    format!("{digest:016x}")
}

fn model_size(name: &str) -> ModelSize {
    ModelSize::from_name(name).unwrap_or_else(|| panic!("the fixture names a shipped size: {name}"))
}

fn conditions(case: &NetworkCase) -> Conditions {
    Conditions {
        alpha_deg: case.alpha,
        reynolds: case.reynolds,
        n_crit: case.n_crit,
        xtr_upper: case.xtr_upper,
        xtr_lower: case.xtr_lower,
    }
}

/// One boundary layer's three channels, under the names upstream returns them
/// by, so that a permutation error in the flip-and-average shows up as a
/// mismatched *value under a named key* rather than as a shifted array.
fn boundary_layer_named(surface: &str, layer: &BoundaryLayer) -> Vec<(String, f64)> {
    let mut named = Vec::with_capacity(3 * BL_STATIONS);
    for station in 0..BL_STATIONS {
        named.push((
            format!("{surface}_bl_theta_{station}"),
            layer.theta[station],
        ));
    }
    for station in 0..BL_STATIONS {
        named.push((
            format!("{surface}_bl_H_{station}"),
            layer.shape_factor[station],
        ));
    }
    for station in 0..BL_STATIONS {
        named.push((
            format!("{surface}_bl_ue/vinf_{station}"),
            layer.ue_over_vinf[station],
        ));
    }
    named
}

fn network_named(aero: &NetworkAero) -> Vec<(String, f64)> {
    let mut named = vec![
        ("analysis_confidence".to_owned(), aero.analysis_confidence),
        ("CL".to_owned(), aero.cl),
        ("CD".to_owned(), aero.cd),
        ("CM".to_owned(), aero.cm),
        ("Top_Xtr".to_owned(), aero.top_xtr),
        ("Bot_Xtr".to_owned(), aero.bot_xtr),
    ];
    named.extend(boundary_layer_named("upper", &aero.upper));
    named.extend(boundary_layer_named("lower", &aero.lower));
    named
}

fn corrected_named(aero: &Aero) -> Vec<(String, f64)> {
    vec![
        ("analysis_confidence".to_owned(), aero.analysis_confidence),
        ("CL".to_owned(), aero.cl),
        ("CD".to_owned(), aero.cd),
        ("CM".to_owned(), aero.cm),
        ("Cpmin".to_owned(), aero.cpmin),
        ("Top_Xtr".to_owned(), aero.top_xtr),
        ("Bot_Xtr".to_owned(), aero.bot_xtr),
        ("mach_crit".to_owned(), aero.mach_crit),
        ("mach_dd".to_owned(), aero.mach_dd),
        ("Cpmin_0".to_owned(), aero.cpmin_0),
    ]
}

/// Compare every named quantity, and fail on a key the reference has and this
/// side does not rather than quietly checking the overlap.
fn compare_named(
    comparison: &mut Comparison,
    case: &str,
    produced: &[(String, f64)],
    expected: &HashMap<String, f64>,
) {
    comparison.exact(
        &format!("{case} (quantity count)"),
        &produced.len(),
        &expected.len(),
    );
    for (key, value) in produced {
        match expected.get(key) {
            Some(&reference) => comparison.scalar(&format!("{case}.{key}"), *value, reference),
            None => comparison.exact(
                &format!("{case}.{key} is a quantity the reference reports"),
                &false,
                &true,
            ),
        };
    }
}

#[test]
fn fnv1a64_matches_the_published_test_vectors() {
    assert_eq!(fnv1a64(b""), "cbf29ce484222325");
    assert_eq!(fnv1a64(b"a"), "af63dc4c8601ec8c");
    assert_eq!(fnv1a64(b"foobar"), "85944171f73967e8");
}

#[test]
fn every_embedded_parameter_blob_digests_to_what_the_package_exported() {
    // The one check in this row that is not about arithmetic. A weight that
    // drifted from the installed NeuralFoil would pass every comparison
    // below, because both sides would be evaluating the same wrong network.
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let mut comparison = Comparison::new("alas-aero::neuralfoil (parameters)", Tier::Exact);

    for (name, bytes) in BLOBS {
        match fixture.digests.get(name) {
            Some(expected) => comparison.exact(name, &fnv1a64(bytes), expected),
            None => panic!("{name} is shipped but the fixture has no digest for it"),
        };
    }
    comparison.exact(
        "the fixture digests exactly the blobs that ship",
        &fixture.digests.len(),
        &BLOBS.len(),
    );
    comparison.finish();
}

#[test]
fn the_raw_network_matches_neuralfoil_on_every_case() {
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let mut comparison = Comparison::new(
        "alas-aero::neuralfoil (get_aero_from_kulfan_parameters)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.network {
        let aero = aero_from_kulfan_parameters(
            &case.kulfan.airfoil(),
            &conditions(case),
            model_size(&case.model_size),
        )
        .expect("an eight-weight section, as the reference fitted");
        compare_named(&mut comparison, name, &network_named(&aero), &case.outputs);
    }
    comparison.finish();
}

#[test]
fn every_shipped_model_size_is_exercised_by_the_fixture() {
    // The five differ in depth as well as width, four, five, five, six and
    // six weight layers. A port that assumed a fixed architecture would agree
    // on `large` and fail here, which is the point of not testing one size.
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let exercised: Vec<&str> = fixture
        .network
        .values()
        .map(|case| case.model_size.as_str())
        .collect();
    for size in ModelSize::all() {
        assert!(
            exercised.contains(&size.name()),
            "no fixture case runs the {} model",
            size.name()
        );
    }
}

#[test]
fn the_corrected_surrogate_matches_aerosandbox_on_every_case() {
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let mut comparison = Comparison::new(
        "alas-aero::neuralfoil (KulfanAirfoil.get_aero_from_neuralfoil)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.kulfan_airfoil {
        let airfoil = case.kulfan.airfoil();
        let aero = aero_from_kulfan_airfoil(
            &airfoil,
            &Conditions::new(case.alpha, case.reynolds),
            case.mach,
            model_size(&case.model_size),
        )
        .expect("an eight-weight section, as the reference fitted");

        compare_named(
            &mut comparison,
            name,
            &corrected_named(&aero),
            &case.outputs,
        );

        // The boundary layer passes through this wrapper untouched, which is
        // worth checking rather than assuming: it is what `Cpmin_0` is
        // computed from, so a wrapper that reordered it would move the whole
        // transonic schedule.
        let mut passthrough = boundary_layer_named("upper", &aero.upper);
        passthrough.extend(boundary_layer_named("lower", &aero.lower));
        compare_named(
            &mut comparison,
            &format!("{name}.boundary_layer"),
            &passthrough,
            &case.boundary_layer,
        );

        // `t/c` sets the supersonic end of the wave-drag schedule and comes
        // from the Kulfan surfaces rather than a vertex list.
        comparison.scalar(
            &format!("{name}.max_thickness"),
            airfoil.max_thickness(&linspace(0.0, 1.0, 101)),
            case.max_thickness,
        );
    }
    comparison.finish();
}

#[test]
fn the_screening_entry_point_matches_aerosandbox_on_real_sections() {
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let mut comparison = Comparison::new(
        "alas-aero::neuralfoil (Airfoil.get_aero_from_neuralfoil)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.airfoil {
        let airfoil = Airfoil::from_coordinates(name.clone(), case.coordinates.clone());
        let aero = aero_from_airfoil(
            &airfoil,
            &Conditions::new(case.alpha, case.reynolds),
            case.mach,
            model_size(&case.model_size),
        )
        .expect("a fittable section, as the reference fitted");
        compare_named(
            &mut comparison,
            name,
            &corrected_named(&aero),
            &case.outputs,
        );
    }
    comparison.finish();
}

#[test]
fn the_visualization_entry_point_reaches_the_raw_network_and_not_the_corrections() {
    // `neuralfoil.get_aero_from_coordinates` normalizes and fits exactly as
    // the screening path does, and then stops: no Mach number, no post-stall
    // blend. The fixture's expected values come from that function, so a port
    // that routed this through the corrected path fails here even though it
    // would agree at Mach zero and low incidence.
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");
    let mut comparison = Comparison::new(
        "alas-aero::neuralfoil (neuralfoil.get_aero_from_coordinates)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.coordinates {
        let aero = aero_from_coordinates(
            &case.coordinates,
            &Conditions::new(case.alpha, case.reynolds),
            // `visualization.py` passes no `model_size`, so the default of
            // the function it calls applies, and that default is `large`,
            // not the `xlarge` its neighbours in `neuralfoil.main` default to.
            ModelSize::Large,
        )
        .expect("a fittable section, as the reference fitted");
        compare_named(&mut comparison, name, &network_named(&aero), &case.outputs);
    }
    comparison.finish();
}

#[test]
fn the_fixture_reaches_the_branches_its_generator_promises() {
    // `gen_aero_neuralfoil.py` refuses to write a fixture that misses one of
    // these; this states the same properties from the side that would
    // otherwise pass silently against a stale fixture.
    let fixture: Fixture = alas_testkit::load("aero", "neuralfoil");

    let mut branches = [false; 4];
    for case in fixture.kulfan_airfoil.values() {
        let crit = case.outputs["mach_crit"];
        let divergence = case.outputs["mach_dd"];
        let branch = if case.mach < crit {
            0
        } else if case.mach < divergence {
            1
        } else if case.mach < 1.1 {
            2
        } else {
            3
        };
        branches[branch] = true;
    }
    assert!(
        branches.iter().all(|reached| *reached),
        "the wave-drag schedule's four branches are not all reached: {branches:?}"
    );

    let wrapped = |alpha: f64| (alpha + 180.0).rem_euclid(360.0) - 180.0;
    assert!(fixture
        .kulfan_airfoil
        .values()
        .any(|case| wrapped(case.alpha).abs() >= 22.0));
    assert!(fixture
        .kulfan_airfoil
        .values()
        .any(|case| wrapped(case.alpha).abs() <= 10.0));
    assert!(fixture
        .kulfan_airfoil
        .values()
        .any(|case| case.alpha.abs() > 180.0));

    assert!(fixture
        .airfoil
        .values()
        .any(|case| case.rotation_angle != 0.0 && case.scale_factor != 1.0));
}
