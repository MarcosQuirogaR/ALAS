// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Share of the landing flap lift increment reached at a take-off flap
//! deflection, from measured transport-aircraft `CLmax,1g` against flap angle.
//!
//! Source: Obert, *Aerodynamic Design of Transport Aircraft* (2009), Ch. 26,
//! printed p. 324 (library PDF p. 343), Figs. 26.13 (A320-200), 26.14
//! (A330-200), 26.15 (737-800) and p. 325 (PDF p. 344), Fig. 26.16 (777-200):
//! flight-test `CLmax,1g` at sea level with slats out against flap angle. The
//! values below were read from the rendered plots (reading uncertainty about
//! +-0.02 in `CLmax`).
//!
//! For each aircraft the share is `r = (CL(d) - CL(0)) / (CL(d_land) -
//! CL(0))`, with `CL(0)` the slats-out, flaps-up point and `d_land` the
//! largest plotted flap angle, so `x = d / d_land` and `r(1) = 1`:
//!
//! | aircraft (d_land, CL(0), CL(d_land)) | d: CL(d) |
//! |---|---|
//! | A320-200 (40, 1.90, 2.59) | 10: 2.12; 15: 2.35; 20: 2.42 (gear down) |
//! | A330-200 (32, 1.75, 2.50) | 8: 1.99; 14: 2.25; 22: 2.38 |
//! | 737-800 (40, 1.96, 2.58) | 5: 2.06; 10: 2.28; 15: 2.35; 25: 2.40; 30: 2.45 (gear down above 15) |
//! | 777-200 (30, 1.70, 2.40) | 5: 1.94; 15: 2.04; 20: 2.13 |
//!
//! The 14 points scatter about the pooled curve by about 0.09 in `r` (r.m.s.),
//! which is about +-0.08 in `CLmax_TO` for a typical flap increment of 0.9. The
//! gear-down points above the take-off flap range are kept because they are
//! plotted for the same flap series; they bias `r` upward by the gear
//! increment (about 0.03 in `CL`).
//!
//! The pooled curve below is the piecewise-linear mean of those points in
//! four bins (`x` in 0.125-0.25, 0.375-0.5, 0.625-0.75, then 1 by definition).

/// Pooled share nodes `(x = d / d_land, r)`, monotone, `r(0) = 0`, `r(1) = 1`.
const NODES: [(f64, f64); 5] = [
    (0.0, 0.0),
    (0.21, 0.33),
    (0.44, 0.64),
    (0.69, 0.74),
    (1.0, 1.0),
];

/// Share `r` of the landing flap increment at flap fraction `x = d / d_land`.
///
/// `x` is clamped to `[0, 1]`; a non-finite `x` returns `None`.
pub fn increment_share(x: f64) -> Option<f64> {
    if !x.is_finite() {
        return None;
    }
    let x = x.clamp(0.0, 1.0);
    let upper = NODES.iter().position(|&(node, _)| node >= x)?;
    if upper == 0 {
        return Some(NODES[0].1);
    }
    let ((x0, r0), (x1, r1)) = (NODES[upper - 1], NODES[upper]);
    Some(r0 + (r1 - r0) * (x - x0) / (x1 - x0))
}

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_is_monotone_and_pinned_at_both_ends() {
        assert_eq!(increment_share(0.0), Some(0.0));
        assert_eq!(increment_share(1.0), Some(1.0));
        let mut last = 0.0;
        for i in 0..=100 {
            let r = increment_share(i as f64 / 100.0).unwrap();
            assert!(r >= last);
            last = r;
        }
        assert_eq!(increment_share(f64::NAN), None);
        assert_eq!(increment_share(2.0), Some(1.0));
    }

    /// The measured points of the four plots, as `(x, r)`.
    fn measured() -> Vec<(f64, f64)> {
        let family = |cl0: f64, cl_land: f64, d_land: f64, points: &[(f64, f64)]| {
            points
                .iter()
                .map(|&(d, cl)| (d / d_land, (cl - cl0) / (cl_land - cl0)))
                .collect::<Vec<_>>()
        };
        let mut all = family(
            1.90,
            2.59,
            40.0,
            &[(10.0, 2.12), (15.0, 2.35), (20.0, 2.42)],
        );
        all.extend(family(
            1.75,
            2.50,
            32.0,
            &[(8.0, 1.99), (14.0, 2.25), (22.0, 2.38)],
        ));
        all.extend(family(
            1.96,
            2.58,
            40.0,
            &[
                (5.0, 2.06),
                (10.0, 2.28),
                (15.0, 2.35),
                (25.0, 2.40),
                (30.0, 2.45),
            ],
        ));
        all.extend(family(
            1.70,
            2.40,
            30.0,
            &[(5.0, 1.94), (15.0, 2.04), (20.0, 2.13)],
        ));
        all
    }

    #[test]
    fn the_pooled_curve_reproduces_the_measured_points_within_their_scatter() {
        let points = measured();
        let rms = (points
            .iter()
            .map(|&(x, r)| (increment_share(x).unwrap() - r).powi(2))
            .sum::<f64>()
            / points.len() as f64)
            .sqrt();
        assert!(rms < 0.12, "r.m.s. {rms}");
    }

    #[test]
    fn the_mid_range_share_brackets_the_raymer_take_off_band() {
        // Raymer p. 415: take-off flap lift is 60-80 % of the landing value.
        let mid = increment_share(0.5).unwrap();
        assert!((0.6..=0.8).contains(&mid), "{mid}");
    }
}
