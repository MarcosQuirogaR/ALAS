// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Grid construction and interpolation kernels of [`super::TrimmedDragTable`].

use super::{
    DragTableError, CL_STEP, MACH_ABOVE_CRUISE, MACH_COARSE_STEP, MACH_FINE_FROM, MACH_FINE_STEP,
    MACH_MIN,
};

/// CL nodes from [`CL_STEP`] to `cl_max`, the last one exactly `cl_max`,
/// with the divergence law's curvature break `lift_break`
/// (`alas_aero::analysis::DivergenceLaw::lift_slope_break`) as a node when it
/// lies inside: the cubic Hermite cell across it would otherwise carry an
/// `h^2` error that no Mach refinement reduces.
pub(super) fn cl_grid(cl_max: f64, lift_break: Option<f64>) -> Result<Vec<f64>, DragTableError> {
    if !cl_max.is_finite() || cl_max < 2.0 * CL_STEP {
        return Err(DragTableError::NonPhysical("clean CLmax"));
    }
    let mut cl: Vec<f64> = (1..)
        .map(|n| n as f64 * CL_STEP)
        .take_while(|&c| c < cl_max - 1e-9)
        .collect();
    cl.push(cl_max);
    if let Some(at) = lift_break.filter(|&at| at.is_finite() && at > cl[0] && at < cl_max) {
        cl.push(at);
        cl.sort_by(f64::total_cmp);
        cl.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    }
    Ok(cl)
}

/// Mach nodes per the module doc, with the wave-onset Mach as a node.
pub(super) fn mach_grid(cruise_mach: f64, onset_mach: f64) -> Vec<f64> {
    let top = cruise_mach + MACH_ABOVE_CRUISE;
    let mut mach: Vec<f64> = (0..)
        .map(|n| MACH_MIN + n as f64 * MACH_COARSE_STEP)
        .take_while(|&m| m < MACH_FINE_FROM.min(top) - 1e-9)
        .collect();
    mach.extend(
        (0..)
            .map(|n| MACH_FINE_FROM + n as f64 * MACH_FINE_STEP)
            .take_while(|&m| m < top - 1e-9),
    );
    mach.push(top);
    if onset_mach > MACH_MIN && onset_mach < top {
        mach.push(onset_mach);
    }
    mach.sort_by(f64::total_cmp);
    mach.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    mach
}

/// Index of the cell holding `x` and the linear weight in it, clamped.
pub(super) fn cell(nodes: &[f64], x: f64) -> (usize, f64) {
    let last = nodes.len() - 2;
    let x = x.clamp(nodes[0], nodes[last + 1]);
    let index = nodes
        .partition_point(|&node| node <= x)
        .saturating_sub(1)
        .min(last);
    let weight = (x - nodes[index]) / (nodes[index + 1] - nodes[index]);
    (index, weight)
}

pub(super) fn lerp(a: f64, b: f64, weight: f64) -> f64 {
    a + weight * (b - a)
}

/// Cubic Hermite value and derivative on `[x0, x1]` from end values and slopes.
pub(super) fn hermite(
    x0: f64,
    x1: f64,
    (f0, s0): (f64, f64),
    (f1, s1): (f64, f64),
    x: f64,
) -> (f64, f64) {
    let h = x1 - x0;
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    let value = (2.0 * t3 - 3.0 * t2 + 1.0) * f0
        + (t3 - 2.0 * t2 + t) * h * s0
        + (-2.0 * t3 + 3.0 * t2) * f1
        + (t3 - t2) * h * s1;
    let slope = (6.0 * t2 - 6.0 * t) / h * f0
        + (3.0 * t2 - 4.0 * t + 1.0) * s0
        + (6.0 * t - 6.0 * t2) / h * f1
        + (3.0 * t2 - 2.0 * t) * s1;
    (value, slope)
}

#[cfg(test)]
mod tests {
    // The law and grids are built from literals, so a failed `expect` or
    // `unwrap` is the assertion failing rather than a library invariant
    // being broken.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::mdo::drag_table::CD_ERROR_BOUND;
    use alas_aero::analysis::DivergenceLaw;

    /// The default (Shevell-Kroo) law of a supercritical wing whose
    /// quarter-chord sweep (19.5 deg) and mean t/c (0.14) put the chart
    /// break `cos^2 L = 0.8886` inside the 0.85-0.90 CL cell.
    fn shevell_law() -> DivergenceLaw {
        serde_json::from_value(serde_json::json!({
            "relation": { "Shevell": { "increment": 0.035 } },
            "cos_sweep": 19.5_f64.to_radians().cos(),
            "thickness": 0.14,
            "offset": 0.0,
        }))
        .expect("a Shevell-Kroo divergence law")
    }

    /// Largest `|Hermite - law|` wave drag at the CL cell centres of `cl`,
    /// at Mach nodes (the Mach direction exact), as `cell_error` measures it.
    fn worst_cl_cell_error(law: &DivergenceLaw, cl: &[f64], mach: &[f64]) -> f64 {
        let node = |m: f64, c: f64| {
            let value = law.wave_cd(m, c, 20.0);
            (value, law.wave_cd_lift_slope(m, c, value))
        };
        let mut worst = 0.0_f64;
        for &m in mach {
            for pair in cl.windows(2) {
                let centre = 0.5 * (pair[0] + pair[1]);
                let (table, _) =
                    hermite(pair[0], pair[1], node(m, pair[0]), node(m, pair[1]), centre);
                worst = worst.max((table - law.wave_cd(m, centre, 20.0)).abs());
            }
        }
        worst
    }

    #[test]
    fn the_divergence_law_break_is_a_cl_node() {
        let law = shevell_law();
        let at = law.lift_slope_break().expect("the Shevell chart break");
        let cl = cl_grid(1.5, Some(at)).expect("a CL grid");
        assert!(cl.iter().any(|&c| (c - at).abs() < 1e-12));
        assert!(cl.windows(2).all(|pair| pair[1] > pair[0]));
        assert_eq!(cl.last(), Some(&1.5));
        // A break outside the grid, or on a node, adds nothing.
        assert_eq!(
            cl_grid(1.5, Some(2.0)).unwrap(),
            cl_grid(1.5, None).unwrap()
        );
        assert_eq!(
            cl_grid(1.5, Some(0.5)).unwrap(),
            cl_grid(1.5, None).unwrap()
        );
    }

    #[test]
    fn a_cl_cell_across_the_chart_break_misses_the_bound_that_the_break_node_meets() {
        // Cruise Mach 0.84 (the default requirement) plus the grid's 0.06:
        // the Mach nodes 0.70..0.90 where the wave column is tabulated.
        let mach: Vec<f64> = (0..=20).map(|k| 0.70 + 0.01 * f64::from(k)).collect();
        let law = shevell_law();
        let plain = worst_cl_cell_error(&law, &cl_grid(1.5, None).unwrap(), &mach);
        let with_break =
            worst_cl_cell_error(&law, &cl_grid(1.5, law.lift_slope_break()).unwrap(), &mach);
        // The straddling cell: 1.56e-5 at CL 0.875, Mach 0.89, which no Mach
        // halving reduces (the defect behind 'drag-table error 1.297e-5').
        assert!(plain > CD_ERROR_BOUND, "plain grid {plain:.3e}");
        // With the break a node the smooth h^4 term is left: about 8e-8.
        assert!(
            with_break < 0.1 * CD_ERROR_BOUND,
            "break node {with_break:.3e}"
        );
    }
}
