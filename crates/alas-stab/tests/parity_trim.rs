// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-stab::trim` against `alas/physics/stability.py`, via
//! `golden/generators/gen_stab_trim.py`.
//!
//! # Two tiers, and the split is a tightening
//!
//! `docs/PORTING.md` names `closed` + `linalg` for this row. Half the module:
//! `static_margin`, `neutral_point`, `autobalance` and `stability_and_trim`:
//! reports numbers that come out of a dense VLM AIC solve, which is exactly the
//! construction `linalg` describes, and those are compared there. The other
//! half: `munk_apparent_mass_factor`, `fuselage_cm_alpha` and
//! `tail_volume_coefficients`, never goes near a factorization: it is a table
//! interpolation, a midpoint quadrature over the fuselage station list and two
//! ratios of wing areas and moment arms, all closed-form `f64` arithmetic that
//! both implementations evaluate in the same order. Comparing that half at
//! `linalg` would let three orders of magnitude through on formulas that in
//! fact agree to about 1.5e-16 relative, one ulp, with every `munk` case
//! bit-identical, so it is compared at `closed`, the tier the tolerance table
//! defines for exactly this construction. `parity_analysis.rs` and
//! `parity_layout.rs` already run their two comparisons side by side the same
//! way.
//!
//! # The aircraft
//!
//! This runs on the frozen-reference aircraft because
//! `fuselage_cm_alpha` integrates the real fuselage area distribution and the
//! trim solve runs VLM on the built geometry. Product geometry has a newer
//! transport-planform default and must not be mixed into this fixture.
//!
//! The `no_hstab` cases and the two- and one-wing tail-volume cases are the
//! same aircraft with wings removed, reconstructed here exactly as the
//! generator's `copy.deepcopy` + wing-list edit does on its side.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_config::analysis::AnalysisConfig;
use alas_config::geometry::GeometryConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_stab::trim;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

const HSTAB_NAME: &str = "Horizontal Stabilizer";

#[derive(Debug, Deserialize)]
struct AirplaneFixture {
    s_ref: f64,
    c_ref: f64,
    b_ref: f64,
    xyz_ref: [f64; 3],
    wing_names: Vec<String>,
    fuselage_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MunkInputs {
    fineness: f64,
}

#[derive(Debug, Deserialize)]
struct MunkCase {
    inputs: MunkInputs,
    factor: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageInputs {
    cl_alpha: f64,
    has_hstab: bool,
}

#[derive(Debug, Deserialize)]
struct FuselageCase {
    inputs: FuselageInputs,
    cm_alpha: f64,
}

#[derive(Debug, Deserialize)]
struct StaticMarginFixture {
    static_margin: f64,
}

#[derive(Debug, Deserialize)]
struct NeutralPointFixture {
    x_np: f64,
    static_margin: f64,
    cl_alpha: f64,
}

#[derive(Debug, Deserialize)]
struct AutobalanceInputs {
    target_static_margin: f64,
}

#[derive(Debug, Deserialize)]
struct AutobalanceCase {
    inputs: AutobalanceInputs,
    /// `null` where the static margin was NaN, see `gen_stab_trim.py`.
    sm_before: Option<f64>,
    xyz_ref_x_after: f64,
}

#[derive(Debug, Deserialize)]
struct TrimInputs {
    cl_target: f64,
    mach: f64,
    altitude: f64,
    has_hstab: bool,
}

#[derive(Debug, Deserialize)]
struct TrimCase {
    inputs: TrimInputs,
    x_np: f64,
    static_margin: f64,
    cl_alpha: f64,
    trim_alpha_deg: f64,
    /// `null` (the pure-alpha fallback) maps back to NaN.
    trim_ih_deg: Option<f64>,
    cl_ih: f64,
    cm_ih: f64,
}

#[derive(Debug, Deserialize)]
struct TailVolumeInputs {
    n_wings: usize,
}

#[derive(Debug, Deserialize)]
struct TailVolumeCase {
    inputs: TailVolumeInputs,
    vh: Option<f64>,
    vv: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    airplane: AirplaneFixture,
    munk: HashMap<String, MunkCase>,
    fuselage_cm_alpha: HashMap<String, FuselageCase>,
    static_margin: StaticMarginFixture,
    neutral_point: NeutralPointFixture,
    autobalance: HashMap<String, AutobalanceCase>,
    stability_and_trim: HashMap<String, TrimCase>,
    tail_volume: HashMap<String, TailVolumeCase>,
}

/// A `null` scalar is a NaN one: the generator's `_scalar` convention.
fn or_nan(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

/// The mesh `golden/stab/trim.json` was generated at.
///
/// The reference implementation meshed at one panel in each direction; the
/// product default has since moved to eight chordwise panels (see
/// `alas_config::analysis`), so this restores the frozen mesh explicitly
/// rather than inheriting a default that is no longer it. Mirrors
/// `alas-aero`'s `tests/support::reference_mesh`, which every other VLM-fed
/// parity fixture in this workspace already calls.
fn reference_mesh() -> AnalysisConfig {
    let mut analysis = AnalysisConfig::default();
    analysis.restore_reference_mesh();
    analysis
}

/// The nominal aircraft: the generator's
/// `AircraftBuilder(GeometryConfig()).build()`.
fn build() -> Airplane {
    let mut plane = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()))
        .build(None, true)
        .expect("the default aircraft builds");
    // Preserve the historical area scale for this frozen translation fixture;
    // its lateral/Y b_ref was already the builder's projected value.
    if let Some(wing) = plane.wings.first() {
        let s_ref = wing.unfolded_area();
        plane.s_ref = s_ref;
    }
    plane
}

/// The nominal aircraft with the horizontal stabilizer removed: the
/// generator's `_without_wing(plane, HSTAB)`.
fn without_hstab(plane: &Airplane) -> Airplane {
    let mut stripped = plane.clone();
    stripped.wings.retain(|w| w.name != HSTAB_NAME);
    stripped
}

/// The nominal aircraft keeping only its first `count` wings, in order: the
/// generator's `_first_wings(plane, count)`, which is what
/// `tail_volume_coefficients` indexes by position.
fn first_wings(plane: &Airplane, count: usize) -> Airplane {
    let mut stripped = plane.clone();
    stripped.wings.truncate(count);
    stripped
}

/// Sorted case names, so a failure report reads the same way on every run.
fn names<T>(cases: &HashMap<String, T>) -> Vec<&String> {
    let mut names: Vec<&String> = cases.keys().collect();
    names.sort();
    names
}

#[test]
fn the_two_implementations_are_analysing_the_same_aeroplane() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let plane = build();

    let mut discrete = Comparison::new("trim.airplane (discrete)", Tier::Exact);
    discrete.exact(
        "wing_names",
        &plane
            .wings
            .iter()
            .map(|w| w.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.wing_names,
    );
    discrete.exact(
        "fuselage_names",
        &plane
            .fuselages
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.fuselage_names,
    );
    discrete.finish();

    // Geometry reference dimensions, pinned at `closed` in builder.json:
    // the sanity check `parity_analysis.rs` makes the same way.
    let mut numeric = Comparison::new("trim.airplane", Tier::Closed);
    numeric.scalar("s_ref", plane.s_ref, fixture.airplane.s_ref);
    numeric.scalar("c_ref", plane.c_ref, fixture.airplane.c_ref);
    numeric.scalar("b_ref", plane.b_ref, fixture.airplane.b_ref);
    numeric.slice("xyz_ref", &plane.xyz_ref, &fixture.airplane.xyz_ref);
    numeric.finish();
}

#[test]
fn the_munk_and_tail_volume_correlations_match_python() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let plane = build();

    let mut comparison = Comparison::new("trim (correlations)", Tier::Closed);

    for name in names(&fixture.munk) {
        let case = &fixture.munk[name];
        comparison.scalar(
            &format!("munk.{name}"),
            trim::munk_apparent_mass_factor(case.inputs.fineness),
            case.factor,
        );
    }

    for name in names(&fixture.tail_volume) {
        let case = &fixture.tail_volume[name];
        let target = first_wings(&plane, case.inputs.n_wings);
        let (vh, vv) = trim::tail_volume_coefficients_reference_compatibility(&target);
        // Presence is discrete: `None` where the aircraft has no such tail.
        let mut presence = Comparison::new(format!("tail_volume.{name} (presence)"), Tier::Exact);
        presence.exact("vh.is_some", &vh.is_some(), &case.vh.is_some());
        presence.exact("vv.is_some", &vv.is_some(), &case.vv.is_some());
        presence.finish();
        if let (Some(actual), Some(expected)) = (vh, case.vh) {
            comparison.scalar(&format!("tail_volume.{name}.vh"), actual, expected);
        }
        if let (Some(actual), Some(expected)) = (vv, case.vv) {
            comparison.scalar(&format!("tail_volume.{name}.vv"), actual, expected);
        }
    }

    comparison.finish();
}

#[test]
fn fuselage_cm_alpha_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let plane = build();
    let plane_no_hstab = without_hstab(&plane);

    let mut comparison = Comparison::new("trim.fuselage_cm_alpha", Tier::Closed);
    for name in names(&fixture.fuselage_cm_alpha) {
        let case = &fixture.fuselage_cm_alpha[name];
        let target = if case.inputs.has_hstab {
            &plane
        } else {
            &plane_no_hstab
        };
        comparison.scalar(
            &format!("fuselage_cm_alpha.{name}"),
            trim::fuselage_cm_alpha_reference_compatibility(target, case.inputs.cl_alpha),
            case.cm_alpha,
        );
    }
    comparison.finish();
}

#[test]
fn the_static_margin_and_neutral_point_match_python() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let plane = build();
    let analysis = reference_mesh();

    let mut comparison = Comparison::new("trim (static margin, neutral point)", Tier::Linalg);

    let sm =
        trim::static_margin(&plane, &analysis).expect("the nominal aircraft meshes and solves");
    comparison.scalar("static_margin", sm, fixture.static_margin.static_margin);

    let (x_np, np_sm, cl_alpha) = trim::neutral_point_reference_compatibility(&plane, &analysis)
        .expect("the nominal aircraft meshes and solves");
    comparison.scalar("neutral_point.x_np", x_np, fixture.neutral_point.x_np);
    comparison.scalar(
        "neutral_point.static_margin",
        np_sm,
        fixture.neutral_point.static_margin,
    );
    comparison.scalar(
        "neutral_point.cl_alpha",
        cl_alpha,
        fixture.neutral_point.cl_alpha,
    );

    comparison.finish();
}

#[test]
fn autobalance_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let analysis = reference_mesh();

    let mut comparison = Comparison::new("trim.autobalance", Tier::Linalg);
    for name in names(&fixture.autobalance) {
        let case = &fixture.autobalance[name];
        // Each case balances a fresh aircraft, as the generator's per-case
        // `copy.deepcopy` does: autobalance mutates `xyz_ref[0]` in place.
        let mut candidate = build();
        let sm_before =
            trim::autobalance(&mut candidate, case.inputs.target_static_margin, &analysis)
                .expect("the nominal aircraft meshes and solves");
        comparison.scalar(
            &format!("autobalance.{name}.sm_before"),
            sm_before,
            or_nan(case.sm_before),
        );
        comparison.scalar(
            &format!("autobalance.{name}.xyz_ref_x_after"),
            candidate.xyz_ref[0],
            case.xyz_ref_x_after,
        );
    }
    comparison.finish();
}

#[test]
fn stability_and_trim_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "trim");
    let plane = build();
    let plane_no_hstab = without_hstab(&plane);
    let analysis = reference_mesh();

    let mut comparison = Comparison::new("trim.stability_and_trim", Tier::Linalg);
    for name in names(&fixture.stability_and_trim) {
        let case = &fixture.stability_and_trim[name];
        let target = if case.inputs.has_hstab {
            &plane
        } else {
            &plane_no_hstab
        };
        let result = trim::stability_and_trim_reference_compatibility(
            target,
            &analysis,
            case.inputs.cl_target,
            case.inputs.mach,
            case.inputs.altitude,
        )
        .expect("the nominal aircraft meshes and solves");
        comparison.scalar(&format!("{name}.x_np"), result.x_np, case.x_np);
        comparison.scalar(
            &format!("{name}.static_margin"),
            result.static_margin,
            case.static_margin,
        );
        comparison.scalar(&format!("{name}.cl_alpha"), result.cl_alpha, case.cl_alpha);
        comparison.scalar(
            &format!("{name}.trim_alpha_deg"),
            result.trim_alpha_deg,
            case.trim_alpha_deg,
        );
        comparison.scalar(
            &format!("{name}.trim_ih_deg"),
            result.trim_ih_deg,
            or_nan(case.trim_ih_deg),
        );
        comparison.scalar(&format!("{name}.cl_ih"), result.cl_ih, case.cl_ih);
        comparison.scalar(&format!("{name}.cm_ih"), result.cm_ih, case.cm_ih);
    }
    comparison.finish();
}
