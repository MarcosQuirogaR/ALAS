// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing builders shared by the viscous-VLM tests (geometry axes x aft, y right, z up).

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_uav::aero::polar_csv::PolarTable;
use alas_uav::aero::section::SectionModel;

pub fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("valid NACA name")
}

pub fn airplane(wing: Wing) -> Airplane {
    let s = wing.reference_area();
    let b = wing.reference_span();
    let c = wing.mean_aerodynamic_chord();
    Airplane {
        name: "test".into(),
        xyz_ref: [0.25 * c, 0.0, 0.0],
        s_ref: s,
        c_ref: c,
        b_ref: b,
        wings: vec![wing],
        fuselages: Vec::new(),
    }
}

/// Untapered, untwisted wing of the given aspect ratio and chord.
pub fn rectangular(aspect: f64, chord: f64, foil: &str) -> Airplane {
    let half = 0.5 * aspect * chord;
    airplane(Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], chord, 0.0, naca(foil)),
            WingXSec::new([0.0, half, 0.0], chord, 0.0, naca(foil)),
        ],
        true,
    ))
}

/// Elliptic planform with a straight quarter-chord line, `sections` per half.
pub fn elliptic(aspect: f64, root_chord: f64, sections: usize, foil: &str) -> Airplane {
    // b = 4 S / (pi c0) -> A = b^2/S = 4 b / (pi c0).
    let span = aspect * std::f64::consts::PI * root_chord / 4.0;
    let half = 0.5 * span;
    let mut xsecs = Vec::new();
    for k in 0..=sections {
        // Cosine spacing resolves the tip.
        let theta = 0.5 * std::f64::consts::PI * k as f64 / sections as f64;
        let y = half * theta.sin() * (1.0 - 1.0e-3 * (k == sections) as u8 as f64);
        let chord =
            (root_chord * (1.0 - (y / half).powi(2)).max(1.0e-6).sqrt()).max(0.02 * root_chord);
        xsecs.push(WingXSec::new(
            [0.25 * (root_chord - chord), y, 0.0],
            chord,
            0.0,
            naca(foil),
        ));
    }
    airplane(Wing::new("Main Wing", xsecs, true))
}

/// Ideal linear thin-airfoil table `cl = 2 pi alpha`, constant `cd`.
pub fn linear_table(name: &str, cd: f64) -> SectionModel {
    let mut text = String::from("alpha,cl,cd,cm,re\n");
    for a in (-25..=25).step_by(5) {
        let alpha = f64::from(a);
        text.push_str(&format!(
            "{alpha},{:.9},{cd},0.0,300000\n",
            std::f64::consts::TAU * alpha.to_radians()
        ));
    }
    SectionModel::from_table(PolarTable::parse(name, &text, None).expect("table"))
}

pub fn neuralfoil(foil: &str, n_crit: f64) -> SectionModel {
    SectionModel::neuralfoil(&naca(foil), n_crit).expect("neuralfoil section")
}
