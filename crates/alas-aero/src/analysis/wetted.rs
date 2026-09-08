// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main-wing exposed-area approximation for the parasite drag buildup.
//!
//! NASA NDARC Theory v1.6, wing drag model, uses `S_wet = 2(S - c*w_fus)`.
//! https://rotorcraft.arc.nasa.gov/Publications/files/NDARCTheory_v1_6_938.pdf
//! Here the chord is integrated piecewise for taper, and the fuselage width
//! is evaluated at root quarter chord and root height. This local extruded
//! cross-section approximation does not resolve fairings, wing thickness,
//! body curvature over the chord, or twist in the local area metric. It only applies to symmetric main wings;
//! tails and detached wings retain their existing surface estimates.

use alas_geom::aircraft::{fuselage::Fuselage, wing::Wing};

pub(super) fn buried_main_wing_area(wing: &Wing, body: &Fuselage) -> f64 {
    let Some(root) = wing.xsecs.first() else {
        return 0.0;
    };
    if !wing.symmetric {
        return 0.0;
    }
    let x = root.xyz_le[0] + 0.25 * root.chord;
    let Some(pair) = body
        .xsecs
        .windows(2)
        .find(|p| p[0].xyz_c[0] <= x && x <= p[1].xyz_c[0] && p[1].xyz_c[0] > p[0].xyz_c[0])
    else {
        return 0.0;
    };
    let t = (x - pair[0].xyz_c[0]) / (pair[1].xyz_c[0] - pair[0].xyz_c[0]);
    let lerp = |a: f64, b: f64| a + t * (b - a);
    let width = lerp(pair[0].width, pair[1].width);
    let height = lerp(pair[0].height, pair[1].height);
    let shape = lerp(pair[0].shape, pair[1].shape);
    let center_y = lerp(pair[0].xyz_c[1], pair[1].xyz_c[1]);
    let center_z = lerp(pair[0].xyz_c[2], pair[1].xyz_c[2]);
    if width <= 0.0 || height <= 0.0 || shape <= 0.0 || center_y.abs() > 1e-9 {
        return 0.0;
    }
    let z = (2.0 * (root.xyz_le[2] - center_z) / height).abs();
    if z >= 1.0 {
        return 0.0;
    }
    let half_width = 0.5 * width * (1.0 - z.powf(shape)).powf(1.0 / shape);
    let mut area = 0.0;
    for pair in wing.xsecs.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let dy = b.xyz_le[1] - a.xyz_le[1];
        if dy.abs() < 1e-12 {
            continue;
        }
        let t0 = ((-half_width - a.xyz_le[1]) / dy).min((half_width - a.xyz_le[1]) / dy);
        let t1 = ((-half_width - a.xyz_le[1]) / dy).max((half_width - a.xyz_le[1]) / dy);
        let lo = t0.max(0.0);
        let hi = t1.min(1.0);
        if hi <= lo {
            continue;
        }
        let chord_lo = a.chord + lo * (b.chord - a.chord);
        let chord_hi = a.chord + hi * (b.chord - a.chord);
        let dz = b.xyz_le[2] - a.xyz_le[2];
        area += dy.hypot(dz) * (hi - lo) * 0.5 * (chord_lo + chord_hi);
    }
    (2.0 * area).min(wing.unfolded_area())
}

// Test geometry construction failures are failed assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::{airfoil::Airfoil, fuselage::FuselageXSec, wing::WingXSec};

    fn wing(root_chord: f64, tip_chord: f64) -> Wing {
        let foil = Airfoil::from_name("naca0012").unwrap();
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], root_chord, 0.0, foil.clone()),
                WingXSec::new([0.0, 10.0, 0.0], tip_chord, 0.0, foil),
            ],
            true,
        )
    }

    fn body() -> Fuselage {
        Fuselage::new(
            "Fuselage",
            [-10.0, 20.0]
                .map(|x| FuselageXSec::new([x, 0.0, 0.0], Some(2.0), None, None, 2.0).unwrap())
                .to_vec(),
        )
    }

    #[test]
    fn rectangular_wing_recovers_ndarc_chord_times_body_width() {
        assert!((buried_main_wing_area(&wing(3.0, 3.0), &body()) - 12.0).abs() < 1e-12);
    }

    #[test]
    fn tapered_center_section_integrates_the_local_chord() {
        // c(y)=4-0.2*y; 2*integral(0..2)c(y)dy = 15.2 m2.
        assert!((buried_main_wing_area(&wing(4.0, 2.0), &body()) - 15.2).abs() < 1e-12);
    }

    #[test]
    fn body_width_respects_wing_height_and_detached_geometry() {
        let low = wing(3.0, 3.0).translate([0.0, 0.0, 1.0]);
        assert!((buried_main_wing_area(&low, &body()) - 6.0 * 3.0_f64.sqrt()).abs() < 1e-12);
        for offset in [[0.0, 0.0, 3.0], [50.0, 0.0, 0.0], [0.0, 4.0, 0.0]] {
            assert_eq!(
                buried_main_wing_area(&wing(3.0, 3.0).translate(offset), &body()),
                0.0
            );
        }
    }

    #[test]
    fn a_body_wider_than_the_wing_cannot_subtract_more_than_its_area() {
        let narrow = wing(3.0, 3.0);
        let huge = Fuselage::new(
            "Fuselage",
            [-10.0, 20.0]
                .map(|x| FuselageXSec::new([x, 0.0, 0.0], Some(30.0), None, None, 2.0).unwrap())
                .to_vec(),
        );
        assert_eq!(
            buried_main_wing_area(&narrow, &huge),
            narrow.unfolded_area()
        );
    }
}
