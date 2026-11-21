// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trim as a bracketed root when a Newton step cannot find it.
//!
//! The lattice is linear in the circulation, but the stabiliser incidence moves
//! its panels. Where a wing wake leg lies close to the tail, the moment against
//! the incidence is not monotonic: it has a short stretch of the wrong slope,
//! and a Newton step that lands there is thrown away from the solution. The
//! trim still exists on either side of that stretch.
//!
//! So: at each incidence the angle of attack is solved for the target lift
//! (the lift is almost linear in it), which leaves the moment as a function of
//! the incidence alone. The incidences around the start are walked outward
//! until the moment changes sign the way an aft tail makes it change, falling
//! as the incidence rises, and that bracket is closed with the Illinois
//! variant of regula falsi (Dowell and Jarratt, BIT 11, 1971). The stretch of
//! the wrong slope has the opposite sign change and is never accepted.
//!
//! Nothing here loosens a tolerance: the root returned meets the same residual
//! the Newton iteration has to meet, or none is returned.

/// Spacing of the walk over the incidence, degrees.
const SEARCH_STEP_DEG: f64 = 1.0;
/// How far the walk goes on each side of the start, degrees.
pub const SEARCH_RANGE_DEG: f64 = 15.0;
/// Corrections of the angle of attack at one incidence.
const LIFT_ITERATIONS: usize = 12;
/// Finite step of the lift slope, degrees.
const LIFT_SLOPE_STEP_DEG: f64 = 0.1;
/// Regula falsi steps closing one bracket.
const BRACKET_ITERATIONS: usize = 60;

/// One incidence with the angle of attack that gives the target lift.
#[derive(Clone, Copy)]
struct Level {
    incidence_deg: f64,
    alpha_deg: f64,
    cm: f64,
}

/// The trim `(alpha, incidence)` in degrees nearest to the start with the
/// moment falling through zero as the incidence rises, with both lift and moment
/// residuals within `tolerance`; `None` when there is none within
/// [`SEARCH_RANGE_DEG`] of `start_incidence_deg`.
///
/// `lattice(alpha, incidence)` returns `(CL, Cm)`, `Cm` including whatever
/// moment the caller's trim condition adds to the lattice's own.
///
/// # Errors
///
/// What `lattice` returns.
pub fn bracketed_trim<E>(
    mut lattice: impl FnMut(f64, f64) -> Result<(f64, f64), E>,
    cl_target: f64,
    start_alpha_deg: f64,
    start_incidence_deg: f64,
    tolerance: f64,
) -> Result<Option<(f64, f64)>, E> {
    if !(start_alpha_deg.is_finite() && start_incidence_deg.is_finite() && cl_target.is_finite()) {
        return Ok(None);
    }
    let mut level_at = |incidence_deg: f64, alpha_guess: f64| -> Result<Option<Level>, E> {
        let mut alpha_deg = alpha_guess;
        let (mut cl, mut cm) = lattice(alpha_deg, incidence_deg)?;
        let (above, _) = lattice(alpha_deg + LIFT_SLOPE_STEP_DEG, incidence_deg)?;
        let slope = (above - cl) / LIFT_SLOPE_STEP_DEG;
        if !(slope.is_finite() && slope > 0.0) {
            return Ok(None);
        }
        for _ in 0..LIFT_ITERATIONS {
            if !(cl.is_finite() && cm.is_finite()) {
                return Ok(None);
            }
            if (cl - cl_target).abs() <= tolerance {
                return Ok(Some(Level {
                    incidence_deg,
                    alpha_deg,
                    cm,
                }));
            }
            alpha_deg += (cl_target - cl) / slope;
            (cl, cm) = lattice(alpha_deg, incidence_deg)?;
        }
        Ok(None)
    };

    let Some(start) = level_at(start_incidence_deg, start_alpha_deg)? else {
        return Ok(None);
    };
    if start.cm.abs() <= tolerance {
        return Ok(Some((start.alpha_deg, start.incidence_deg)));
    }
    let steps = (SEARCH_RANGE_DEG / SEARCH_STEP_DEG).round() as usize;
    let (mut above, mut below) = (Some(start), Some(start));
    for step in 1..=steps {
        let offset = step as f64 * SEARCH_STEP_DEG;
        if let Some(low) = above {
            let next = level_at(start_incidence_deg + offset, low.alpha_deg)?;
            if let Some(high) = next.filter(|high| low.cm > 0.0 && high.cm <= 0.0) {
                if let Some(root) = close_bracket(&mut level_at, low, high, tolerance)? {
                    return Ok(Some(root));
                }
            }
            above = next;
        }
        if let Some(high) = below {
            let next = level_at(start_incidence_deg - offset, high.alpha_deg)?;
            if let Some(low) = next.filter(|low| low.cm > 0.0 && high.cm <= 0.0) {
                if let Some(root) = close_bracket(&mut level_at, low, high, tolerance)? {
                    return Ok(Some(root));
                }
            }
            below = next;
        }
    }
    Ok(None)
}

/// Close a bracket with `low.cm > 0 >= high.cm`; `None` when the moment jumps
/// across it instead of passing through zero.
fn close_bracket<E>(
    level_at: &mut impl FnMut(f64, f64) -> Result<Option<Level>, E>,
    mut low: Level,
    mut high: Level,
    tolerance: f64,
) -> Result<Option<(f64, f64)>, E> {
    let (mut low_cm, mut high_cm) = (low.cm, high.cm);
    let mut moved_low_last = false;
    for _ in 0..BRACKET_ITERATIONS {
        if high.cm.abs() <= tolerance {
            return Ok(Some((high.alpha_deg, high.incidence_deg)));
        }
        if low.cm.abs() <= tolerance {
            return Ok(Some((low.alpha_deg, low.incidence_deg)));
        }
        let fraction = low_cm / (low_cm - high_cm);
        let incidence_deg = low.incidence_deg + fraction * (high.incidence_deg - low.incidence_deg);
        let alpha_guess = low.alpha_deg + fraction * (high.alpha_deg - low.alpha_deg);
        let Some(next) = level_at(incidence_deg, alpha_guess)? else {
            return Ok(None);
        };
        // Illinois: when the same end moves twice running, the other one's
        // weight is halved so that it cannot hold the interval open.
        if next.cm > 0.0 {
            if moved_low_last {
                high_cm *= 0.5;
            }
            low = next;
            low_cm = next.cm;
            moved_low_last = true;
        } else {
            if !moved_low_last {
                low_cm *= 0.5;
            }
            high = next;
            high_cm = next.cm;
            moved_low_last = false;
        }
    }
    Ok(None)
}

// A test asserts on values it constructed here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// A lift linear in the attitude and a moment with a stretch of the wrong
    /// slope between -4.5 and -2.75 degrees of incidence.
    fn s_shaped(alpha: f64, ih: f64) -> Result<(f64, f64), ()> {
        let cl = 0.1 * alpha + 0.01 * ih;
        let bump = 0.6 * (-((ih + 3.6) / 0.6).powi(2)).exp();
        Ok((cl, -0.1 * (ih + 6.0) + bump + 0.001 * alpha))
    }

    #[test]
    fn the_root_beyond_a_stretch_of_the_wrong_slope_is_found_and_meets_the_tolerance() {
        let (alpha, ih) = bracketed_trim(s_shaped, 0.66, 7.0, -3.6, 1.0e-7)
            .unwrap()
            .expect("a root");
        let (cl, cm) = s_shaped(alpha, ih).unwrap();
        assert!((cl - 0.66).abs() <= 1.0e-7 && cm.abs() <= 1.0e-7);
        let slope = (s_shaped(alpha, ih + 1.0e-4).unwrap().1 - cm) / 1.0e-4;
        assert!(slope < 0.0, "a stable root, slope {slope}");
    }

    #[test]
    fn a_moment_that_never_crosses_zero_has_no_trim() {
        let never = |alpha: f64, _ih: f64| -> Result<(f64, f64), ()> { Ok((0.1 * alpha, 0.5)) };
        assert_eq!(bracketed_trim(never, 0.5, 5.0, 0.0, 1.0e-7), Ok(None));
    }

    #[test]
    fn the_wrong_way_crossing_is_not_a_trim() {
        let unstable =
            |alpha: f64, ih: f64| -> Result<(f64, f64), ()> { Ok((0.1 * alpha, 0.1 * ih)) };
        assert_eq!(bracketed_trim(unstable, 0.5, 5.0, -3.0, 1.0e-7), Ok(None));
    }
}
