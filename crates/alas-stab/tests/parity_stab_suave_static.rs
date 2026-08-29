// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-stab::static_stability` against
//! `SUAVE.Analyses.Stability.Fidelity_Zero` (static branch), via
//! `golden/generators/gen_stab_suave_static.py`.
//!
//! # One tier: `closed`
//!
//! `docs/PORTING.md` names `closed` for this row, and it holds: nothing here
//! goes through a factorization. Every reported number is closed-form `f64`
//! arithmetic -- the DATCOM slope, the tube-and-wing moment sums, the fuselage
//! correlations -- that both implementations evaluate in the same order, so the
//! parity test asserts at `Tier::Closed`. The fixture geometry (recorded once
//! from the SUAVE vehicle) is fed into the port's flat input, and each case's
//! flight condition drives one `static_stability` call.
//!
//! # Every centre of gravity is the origin
//!
//! Every case's `cg_x` is `0.0`, because this program's SUAVE integration never
//! populates a centre of gravity; `static_stability.rs`'s module doc records why.
//! The nonzero-CG behaviour of the neutral point is a unit test there, not a
//! fixture case, since the fixture cannot reach it.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_stab::static_stability::{
    self, Fuselage, MainWing, StabilityWing, StaticStabilityInput, VerticalStabilizer,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WingFixture {
    aspect_ratio: f64,
    sweep_quarter_chord_rad: f64,
    taper: f64,
    area_ref_m2: f64,
    origin_x_m: f64,
    dynamic_pressure_ratio: f64,
    vertical: bool,
    twist_root_rad: f64,
    twist_tip_rad: f64,
}

impl WingFixture {
    fn to_wing(&self) -> StabilityWing {
        StabilityWing {
            aspect_ratio: self.aspect_ratio,
            sweep_quarter_chord_rad: self.sweep_quarter_chord_rad,
            taper: self.taper,
            area_ref_m2: self.area_ref_m2,
            origin_x_m: self.origin_x_m,
            dynamic_pressure_ratio: self.dynamic_pressure_ratio,
            vertical: self.vertical,
            twist_root_rad: self.twist_root_rad,
            twist_tip_rad: self.twist_tip_rad,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MainWingFixture {
    #[serde(flatten)]
    wing: WingFixture,
    origin_z_m: f64,
    chord_root_m: f64,
    span_m: f64,
    mac_m: f64,
}

#[derive(Debug, Deserialize)]
struct VerticalStabilizerFixture {
    #[serde(flatten)]
    wing: WingFixture,
    origin_z_m: f64,
    chord_root_m: f64,
    chord_tip_m: f64,
    span_m: f64,
    symmetric: bool,
}

#[derive(Debug, Deserialize)]
struct FuselageFixture {
    width_m: f64,
    length_m: f64,
    side_projected_area_m2: f64,
    height_max_m: f64,
    height_at_quarter_length_m: f64,
    height_at_three_quarters_length_m: f64,
    height_at_wing_root_quarter_chord_m: f64,
}

#[derive(Debug, Deserialize)]
struct GeometryFixture {
    reference_area_m2: f64,
    main_wing: MainWingFixture,
    horizontal_stabilizer: WingFixture,
    vertical_stabilizer: VerticalStabilizerFixture,
    fuselage: FuselageFixture,
}

#[derive(Debug, Deserialize)]
struct CaseInputs {
    mach: f64,
    alpha_rad: f64,
    velocity_m_s: f64,
    density_kg_m3: f64,
    dynamic_viscosity_pa_s: f64,
    cg_x_m: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    inputs: CaseInputs,
    cl_alpha: f64,
    cm_alpha: f64,
    cm0: f64,
    cm: f64,
    cn_beta: f64,
    static_margin: f64,
    neutral_point: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    geometry: GeometryFixture,
    cases: HashMap<String, Case>,
}

impl GeometryFixture {
    fn to_input(&self, inputs: &CaseInputs) -> StaticStabilityInput {
        StaticStabilityInput {
            mach: inputs.mach,
            alpha_rad: inputs.alpha_rad,
            velocity_m_s: inputs.velocity_m_s,
            density_kg_m3: inputs.density_kg_m3,
            dynamic_viscosity_pa_s: inputs.dynamic_viscosity_pa_s,
            cg_x_m: inputs.cg_x_m,
            reference_area_m2: self.reference_area_m2,
            main_wing: MainWing {
                wing: self.main_wing.wing.to_wing(),
                origin_z_m: self.main_wing.origin_z_m,
                chord_root_m: self.main_wing.chord_root_m,
                span_m: self.main_wing.span_m,
                mac_m: self.main_wing.mac_m,
            },
            horizontal_stabilizer: self.horizontal_stabilizer.to_wing(),
            vertical_stabilizer: Some(VerticalStabilizer {
                wing: self.vertical_stabilizer.wing.to_wing(),
                origin_z_m: self.vertical_stabilizer.origin_z_m,
                chord_root_m: self.vertical_stabilizer.chord_root_m,
                chord_tip_m: self.vertical_stabilizer.chord_tip_m,
                span_m: self.vertical_stabilizer.span_m,
                symmetric: self.vertical_stabilizer.symmetric,
            }),
            fuselage: Some(Fuselage {
                width_m: self.fuselage.width_m,
                length_m: self.fuselage.length_m,
                side_projected_area_m2: self.fuselage.side_projected_area_m2,
                height_max_m: self.fuselage.height_max_m,
                height_at_quarter_length_m: self.fuselage.height_at_quarter_length_m,
                height_at_three_quarters_length_m: self.fuselage.height_at_three_quarters_length_m,
                height_at_wing_root_quarter_chord_m: self
                    .fuselage
                    .height_at_wing_root_quarter_chord_m,
            }),
        }
    }
}

/// Sorted case names, so a failure report reads the same way on every run.
fn names(cases: &HashMap<String, Case>) -> Vec<&String> {
    let mut names: Vec<&String> = cases.keys().collect();
    names.sort();
    names
}

#[test]
fn the_static_stability_branch_matches_suave() {
    let fixture: Fixture = alas_testkit::load("stab", "suave_static");

    let mut comparison = Comparison::new("static_stability", Tier::Closed);
    for name in names(&fixture.cases) {
        let case = &fixture.cases[name];
        let result = static_stability::static_stability(&fixture.geometry.to_input(&case.inputs));
        comparison.scalar(&format!("{name}.cl_alpha"), result.cl_alpha, case.cl_alpha);
        comparison.scalar(&format!("{name}.cm_alpha"), result.cm_alpha, case.cm_alpha);
        comparison.scalar(&format!("{name}.cm0"), result.cm0, case.cm0);
        comparison.scalar(&format!("{name}.cm"), result.cm, case.cm);
        comparison.scalar(&format!("{name}.cn_beta"), result.cn_beta, case.cn_beta);
        comparison.scalar(
            &format!("{name}.static_margin"),
            result.static_margin,
            case.static_margin,
        );
        comparison.scalar(
            &format!("{name}.neutral_point"),
            result.neutral_point,
            case.neutral_point,
        );
    }
    comparison.finish();
}
