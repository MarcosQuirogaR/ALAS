// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reducing a `PBARL` I-section to the `PBAR` constants a 1995 solver wants.
//!
//! NASTRAN-95 has no `PBARL`: it cannot be handed a named cross-section and
//! asked to work out the beam constants itself. A modern solver does exactly
//! that internally, so the spar caps this mesh describes as `PBARL,...,I` have
//! to arrive here as an explicit area and three moments. This is the one place
//! the row does arithmetic a modern solver hides, and getting it wrong makes the
//! two decks model different beams, so the numbers are held to a modern
//! solver's own reduction rather than assumed.
//!
//! The mesh writes the six I dimensions in pyNastran's order,
//! `[H, W_bottom, W_top, t_web, t_flange_bottom, t_flange_top]`, and every cap
//! it builds has the two flanges equal (`W_bottom == W_top`,
//! `t_flange_bottom == t_flange_top`), which is the symmetric doubly-flanged I
//! the closed forms below assume. The bending constants were checked against MSC
//! Nastran: a cantilever of these caps reports the same tip *rotation*: the
//! pure `M L / (E I)` quantity, with no shear in it, to five figures, which is
//! the statement that `I1` here equals the modern solver's own. The tip
//! *deflection* differs at the fourth figure, which is transverse shear: `PBARL`
//! gives its I-section a shear area and a bare `PBAR` has none. That difference
//! is a formulation choice, secondary on a shell-dominated wingbox, and is left
//! for the convergence tier to absorb rather than papered over with a shear-area
//! guess this solver would apply differently anyway.

/// A symmetric I-section's beam constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarConstants {
    /// Cross-sectional area.
    pub area: f64,
    /// Second moment about axis 1 (the strong axis, in the plane of the web).
    pub i1: f64,
    /// Second moment about axis 2 (the weak axis, through the web).
    pub i2: f64,
    /// Torsional constant, the thin-open-section estimate.
    pub j: f64,
}

/// The `PBAR` constants for a `PBARL` I-section given by
/// `[height, flange_width, flange_width, web_thickness, flange_thickness,
/// flange_thickness]`.
///
/// # Panics
///
/// Never in this program: the mesh only ever emits the six-dimension symmetric
/// I. A section with a different dimension count or unequal flanges returns
/// `None` rather than guessing, since the closed forms below are only the
/// symmetric ones.
pub fn i_section(dim: &[f64]) -> Option<BarConstants> {
    let &[height, width_bottom, width_top, web, flange_bottom, flange_top] = dim else {
        return None;
    };
    if (width_bottom - width_top).abs() > f64::EPSILON
        || (flange_bottom - flange_top).abs() > f64::EPSILON
    {
        return None;
    }
    let d = height;
    let b = width_bottom;
    let t = web;
    let s = flange_bottom;
    // Inside height: the web between the two flanges.
    let h = d - 2.0 * s;

    let area = 2.0 * b * s + h * t;
    // Strong axis: the full rectangle b*d less the two side voids (b - t) wide
    // and h tall, all about the centroid: the standard doubly-symmetric I.
    let i1 = (b * d.powi(3) - (b - t) * h.powi(3)) / 12.0;
    // Weak axis: two flanges b wide and s thick, plus the thin web t wide.
    let i2 = (2.0 * s * b.powi(3) + h * t.powi(3)) / 12.0;
    // Thin open section: the sum of the three rectangles' b*t^3/3 estimate.
    let j = (2.0 * b * s.powi(3) + h * t.powi(3)) / 3.0;

    Some(BarConstants { area, i1, i2, j })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The section the MSC calibration ran on: d=0.1, b=0.05, s=0.01, t=0.005.
    #[test]
    fn the_calibration_section_matches_the_hand_computed_constants() {
        let bar = i_section(&[0.1, 0.05, 0.05, 0.005, 0.01, 0.01]).unwrap();
        assert!((bar.area - 0.0014).abs() < 1e-12);
        assert!((bar.i1 - 2.246_666_666_666_667e-6).abs() < 1e-18);
        assert!((bar.i2 - 2.091_666_666_666_667e-7).abs() < 1e-19);
        assert!((bar.j - 3.666_666_666_666_667e-8).abs() < 1e-20);
    }

    #[test]
    fn a_solid_rectangle_is_the_degenerate_i_with_no_voids() {
        // Flanges as wide as the web (b == t) and web the full height leaves a
        // solid bar: area b*d, I1 = b*d^3/12.
        let b = 0.02;
        let d = 0.1;
        let bar = i_section(&[d, b, b, b, d / 2.0, d / 2.0]).unwrap();
        assert!((bar.area - b * d).abs() < 1e-12);
        assert!((bar.i1 - b * d.powi(3) / 12.0).abs() < 1e-14);
    }

    #[test]
    fn unequal_flanges_are_refused_rather_than_guessed() {
        assert!(i_section(&[0.1, 0.05, 0.06, 0.005, 0.01, 0.01]).is_none());
        assert!(i_section(&[0.1, 0.05, 0.05, 0.005, 0.01, 0.02]).is_none());
    }

    #[test]
    fn a_wrong_dimension_count_is_refused() {
        assert!(i_section(&[0.1, 0.05, 0.05]).is_none());
        assert!(i_section(&[]).is_none());
    }
}
