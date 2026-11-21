// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Downwash increment at the horizontal tail due to trailing-edge flaps.
//!
//! Roskam, *Airplane Design* Part VI (DARcorporation), sect. 8.1.6.1,
//! Eq. 8.52 (printed p. 277), and Fig. 8.70 "Incremental Downwash Angle at
//! the Horizontal Tail due to Flaps" (printed p. 279, copied there from the
//! USAF DATCOM). The chart gives
//!
//! `Delta eps_f A (b_f / b) / Delta C_L_w` (degrees)
//!
//! against the tail height over the wing semispan, `h_h / (b/2)`, with one
//! line for plain flaps and one for slotted flaps. The symbols are:
//! - `Delta C_L_w`: the wing lift increment due to the flap at the
//!   condition's angle of attack (sect. 8.1.4.1);
//! - `A`: the wing aspect ratio;
//! - `b_f / b`: the flapped span over the wing span;
//! - `h_h`: the height of the quarter-chord point of the tail's mean
//!   aerodynamic chord above the plane of the wing root chord, positive up
//!   (Fig. 8.66, printed p. 274).
//!
//! The chart is read between its end points (plain: `h_h/(b/2)` from -0.17
//! to 0.44; slotted: 0.01 to 0.46). A tail outside that range takes the end
//! value, which is the chart's own limit.
//!
//! Every flap family ALAS models (single-slotted Fowler, double and triple
//! slotted, [`alas_config::FlapSlotKind`]) reads the slotted line.

use alas_geom::aircraft::airplane::{Airplane, HORIZONTAL_STABILIZER_NAME};

use crate::trim::main_wing;

/// Which line of Fig. 8.70 to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlapDownwashLine {
    /// Plain (unslotted) flap.
    Plain,
    /// Slotted flap: single-slotted, Fowler, double and triple slotted.
    Slotted,
}

impl From<alas_config::FlapSlotKind> for FlapDownwashLine {
    fn from(_: alas_config::FlapSlotKind) -> Self {
        Self::Slotted
    }
}

/// Fig. 8.70 plain-flap line: `(h_h/(b/2), Delta eps_f A (b_f/b) / Delta
/// C_L_w` in degrees), digitised from the printed chart.
const PLAIN: [(f64, f64); 14] = [
    (-0.172, 28.46),
    (-0.115, 25.74),
    (-0.063, 23.32),
    (-0.010, 21.00),
    (0.016, 19.81),
    (0.042, 18.72),
    (0.089, 16.78),
    (0.141, 14.81),
    (0.193, 13.11),
    (0.240, 11.78),
    (0.292, 10.52),
    (0.344, 9.63),
    (0.391, 9.02),
    (0.438, 8.71),
];

/// Fig. 8.70 slotted-flap line, as [`PLAIN`].
const SLOTTED: [(f64, f64); 11] = [
    (0.010, 27.61),
    (0.042, 25.09),
    (0.094, 21.55),
    (0.141, 18.96),
    (0.193, 16.78),
    (0.240, 15.22),
    (0.292, 13.79),
    (0.344, 12.56),
    (0.391, 11.68),
    (0.438, 10.96),
    (0.464, 10.66),
];

/// The Fig. 8.70 ordinate `Delta eps_f A (b_f/b) / Delta C_L_w`, degrees, at
/// `h_h / (b/2)`, held at the chart's end values outside its range.
#[must_use]
pub fn flap_downwash_parameter_deg(tail_height_over_semispan: f64, line: FlapDownwashLine) -> f64 {
    let table: &[(f64, f64)] = match line {
        FlapDownwashLine::Plain => &PLAIN,
        FlapDownwashLine::Slotted => &SLOTTED,
    };
    let x = tail_height_over_semispan;
    let (first, last) = (table[0], table[table.len() - 1]);
    if x.is_nan() || x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    table
        .windows(2)
        .find(|pair| x <= pair[1].0)
        .map_or(last.1, |pair| {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            y0 + (y1 - y0) * (x - x0) / (x1 - x0)
        })
}

/// The downwash increment at the tail due to the flap, radians (positive:
/// more downwash, a lower tail angle of attack).
///
/// `delta_cl_wing` is the flap's wing lift increment at the condition,
/// `aspect_ratio` the wing's, `flap_span_fraction` the flapped span over the
/// wing span (`b_f / b`), `tail_height_over_semispan` the tail's `h_h /
/// (b/2)` (see the module doc). Zero for a non-positive or non-finite lift
/// increment, aspect ratio or flap span.
#[must_use]
pub fn flap_downwash_increment_rad(
    delta_cl_wing: f64,
    aspect_ratio: f64,
    flap_span_fraction: f64,
    tail_height_over_semispan: f64,
    line: FlapDownwashLine,
) -> f64 {
    if !(delta_cl_wing.is_finite()
        && delta_cl_wing > 0.0
        && aspect_ratio.is_finite()
        && aspect_ratio > 0.0
        && flap_span_fraction.is_finite()
        && flap_span_fraction > 0.0)
    {
        return 0.0;
    }
    let parameter_deg = flap_downwash_parameter_deg(tail_height_over_semispan, line);
    (parameter_deg * delta_cl_wing / (aspect_ratio * flap_span_fraction.min(1.0))).to_radians()
}

/// The tail height over the wing semispan, `h_h / (b/2)`, of a built
/// airplane: the quarter-MAC point of the horizontal stabilizer above the
/// plane of the main wing's root chord (Fig. 8.66), positive up. The root
/// chord plane passes through the root leading edge and falls aft with the
/// root incidence. `None` without a main wing, a horizontal stabilizer or a
/// positive reference span.
#[must_use]
pub fn tail_height_over_semispan(airplane: &Airplane) -> Option<f64> {
    let wing = main_wing(airplane)?;
    let tail = airplane
        .wings
        .iter()
        .find(|w| w.name == HORIZONTAL_STABILIZER_NAME)?;
    let root = wing.xsecs.first()?;
    let tail_c4 = tail.aerodynamic_center(0.25);
    let chord_plane_z =
        root.xyz_le[2] - (tail_c4[0] - root.xyz_le[0]) * root.twist.to_radians().tan();
    let semispan = 0.5 * airplane.b_ref;
    (semispan.is_finite() && semispan > 0.0).then(|| (tail_c4[2] - chord_plane_z) / semispan)
}

/// [`flap_downwash_increment_rad`] for a built airplane: the aspect ratio
/// from its reference span and area and the tail height from
/// [`tail_height_over_semispan`]. `None` where that height is undefined.
#[must_use]
pub fn airplane_flap_downwash_increment_rad(
    airplane: &Airplane,
    delta_cl_wing: f64,
    flap_span_fraction: f64,
    line: FlapDownwashLine,
) -> Option<f64> {
    let h = tail_height_over_semispan(airplane)?;
    let s_ref = airplane.s_ref;
    if !(s_ref.is_finite() && s_ref > 0.0) {
        return None;
    }
    let aspect_ratio = airplane.b_ref * airplane.b_ref / s_ref;
    Some(flap_downwash_increment_rad(
        delta_cl_wing,
        aspect_ratio,
        flap_span_fraction,
        h,
        line,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chart_reproduces_its_digitised_points_and_falls_with_tail_height() {
        // Points read off the printed grid (2 deg, 0.05 spacing).
        assert!((flap_downwash_parameter_deg(0.0, FlapDownwashLine::Plain) - 20.4).abs() < 0.3);
        assert!((flap_downwash_parameter_deg(0.2, FlapDownwashLine::Slotted) - 16.5).abs() < 0.3);
        for line in [FlapDownwashLine::Plain, FlapDownwashLine::Slotted] {
            let mut previous = f64::INFINITY;
            for i in 0..=50 {
                let value = flap_downwash_parameter_deg(-0.2 + 0.014 * f64::from(i), line);
                assert!(value <= previous && value > 8.0, "{line:?} {i} {value}");
                previous = value;
            }
        }
        // The slotted flap turns the flow more at every tail height.
        for h in [0.05, 0.15, 0.3, 0.44] {
            assert!(
                flap_downwash_parameter_deg(h, FlapDownwashLine::Slotted)
                    > flap_downwash_parameter_deg(h, FlapDownwashLine::Plain)
            );
        }
    }

    #[test]
    fn the_increment_scales_with_lift_over_aspect_ratio_and_flap_span() {
        // A 9.5 wing with 0.75 flapped span, a tail 0.15 semispans up, and a
        // landing flap lift increment of 1.0: about 2.6 deg.
        let eps = flap_downwash_increment_rad(1.0, 9.5, 0.75, 0.15, FlapDownwashLine::Slotted)
            .to_degrees();
        let chart = flap_downwash_parameter_deg(0.15, FlapDownwashLine::Slotted);
        assert!((eps - chart / (9.5 * 0.75)).abs() < 1e-12);
        assert!(eps > 2.0 && eps < 3.2, "{eps}");
        let half = flap_downwash_increment_rad(0.5, 9.5, 0.75, 0.15, FlapDownwashLine::Slotted);
        assert!((half.to_degrees() - 0.5 * eps).abs() < 1e-12);
        assert_eq!(
            flap_downwash_increment_rad(0.0, 9.5, 0.75, 0.15, FlapDownwashLine::Slotted),
            0.0
        );
    }
}
