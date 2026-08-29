// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A small, fast-meshing probe aircraft and its wrapping [`AnalysisReport`],
//! shared by every figure's unit tests in this family so each one exercises
//! real geometry (VLM-solvable, unlike an empty [`Airplane`]) without
//! rebuilding one from scratch per file.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use alas_aero::analysis::PolarSweep;
use alas_config::design_variables::DesignVector;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit, PolarFitStatus};

pub fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("valid 4-digit NACA name")
}

/// A short circular fuselage spanning past the wing trailing edge -- the same
/// shape `alas-stab::trim`'s own test probe uses, proven to mesh and solve.
fn probe_fuselage() -> Fuselage {
    let station = |x: f64, r: f64| {
        FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
            .expect("radius alone is valid")
    };
    Fuselage::new(
        "Fuselage",
        vec![
            station(0.0, 0.5),
            station(2.0, 1.5),
            station(10.0, 1.5),
            station(18.0, 0.8),
        ],
    )
}

/// A small probe: a symmetric rectangular main wing, a circular fuselage, and
/// optionally a horizontal and/or vertical stabilizer.
pub fn probe_airplane(with_hstab: bool, with_vstab: bool) -> Airplane {
    let main = Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
            WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, naca("naca0012")),
        ],
        true,
    );
    let mut wings = vec![main];
    if with_hstab {
        wings.push(Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([15.0, 0.0, 0.0], 1.5, -2.0, naca("naca0012")),
                WingXSec::new([15.0, 3.0, 0.0], 1.5, -2.0, naca("naca0012")),
            ],
            true,
        ));
    }
    if with_vstab {
        wings.push(Wing::new(
            "Vertical Stabilizer",
            vec![
                WingXSec::new([15.0, 0.0, 0.0], 1.5, 0.0, naca("naca0012")),
                WingXSec::new([16.0, 0.0, 2.5], 1.0, 0.0, naca("naca0012")),
            ],
            false,
        ));
    }
    let s_ref = wings[0].reference_area();
    let b_ref = wings[0].reference_span();
    let c_ref = wings[0].mean_aerodynamic_chord();
    Airplane {
        name: "Probe".to_owned(),
        xyz_ref: [5.0, 0.0, 0.0],
        wings,
        fuselages: vec![probe_fuselage()],
        s_ref,
        c_ref,
        b_ref,
    }
}

/// Wrap `airplane` in a hand-built [`AnalysisReport`] with a plausible (not
/// physically consistent) polar, mass and CG -- enough for every stability
/// figure to have real numbers to plot.
pub fn probe_report(airplane: Airplane) -> AnalysisReport {
    let mut component_masses = HashMap::new();
    component_masses.insert("Wing".to_owned(), 8000.0);
    component_masses.insert("Fuselage".to_owned(), 12000.0);

    let x_ref = airplane.xyz_ref[0];
    AnalysisReport {
        design: DesignVector::default(),
        physical_cg: [x_ref + 0.2, 0.0, 0.0],
        polar: PolarSweep {
            alpha_deg: vec![-2.0, 0.0, 2.0, 4.0, 6.0],
            geometric_alpha_deg: vec![-2.0, 0.0, 2.0, 4.0, 6.0],
            cl: vec![-0.1, 0.15, 0.4, 0.65, 0.9],
            cd: vec![0.02, 0.02, 0.025, 0.03, 0.04],
            cd_induced: vec![0.001, 0.002, 0.006, 0.012, 0.02],
            cd_wave: vec![0.0; 5],
            cd_parasite: vec![0.019, 0.018, 0.019, 0.018, 0.02],
            cm: vec![0.05, 0.02, -0.02, -0.06, -0.1],
            l_over_d: vec![-5.0, 7.5, 16.0, 21.7, 22.5],
        },
        design_point: DesignPoint {
            alpha_deg: 2.0,
            cl: 0.4,
            cd: 0.025,
            l_over_d: 16.0,
        },
        polar_fit: PolarFit {
            cd0: 0.02,
            k: 0.04,
            oswald_e: 0.85,
            aspect_ratio: 9.0,
            status: PolarFitStatus::Fitted,
        },
        static_margin: 0.10,
        x_neutral_point: x_ref + 1.0,
        geometry_summary: HashMap::new(),
        component_masses,
        mass_coordinates: HashMap::new(),
        payload_layout: None,
        // Dynamic-mode figures are explicitly gated on a solved trim point.
        // The probe is intentionally lightweight, so use a finite hand-built
        // point here to exercise the real mode solver and leave the
        // unavailable-trim branch to the figure's dedicated status tests.
        trimmed_design_point: Some(alas_pipeline::full_analysis::TrimmedDesignPoint {
            alpha_deg: 2.0,
            geometric_body_alpha_deg: 2.0,
            trim_ih_deg: 0.0,
            cl: 0.4,
            cd: 0.025,
            l_over_d: 16.0,
            cm_residual: 0.0,
        }),
        cg_envelope_ok: None,
        airplane,
    }
}
