// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Low-speed field polar of an aircraft's drag model.
//!
//! The jet field reads the airport-base drag table itself at every lift
//! coefficient it flies ([`FieldPolar::lift_drag`]), the same function the
//! second-segment assessor evaluates at its one `CL` (the
//! three-point parabola this replaced, extrapolated to `CL` 1.9, read 0.002
//! of gradient more drag than the table at the same point). The quadratic
//! `(cd0, k)` is kept for the propeller estimate and for callers that need a
//! parabola.

/// Low-speed field polar: a parabola and, when built from a drag table, the
/// table's lift-dependent drag on a lift grid.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPolar {
    /// Parabola `cd0 + k CL^2` fitted to three table samples: a positive
    /// linear lift term is bounded conservatively over `0..CLmax_TO`, a
    /// negative one receives no credit.
    pub cd0: f64,
    /// Induced factor of that parabola.
    pub k: f64,
    /// Table drag at zero lift.
    zero_lift: f64,
    /// Lift-grid spacing of `lift_drag`.
    step: f64,
    /// Table drag minus `zero_lift` on the grid `0, step, 2 step ..`; empty
    /// for a pure parabola.
    lift_drag: Vec<f64>,
}

impl FieldPolar {
    /// A pure parabola `cd0 + k CL^2`.
    pub fn parabola(cd0: f64, k: f64) -> Self {
        Self {
            cd0,
            k,
            zero_lift: cd0,
            step: 1.0,
            lift_drag: Vec::new(),
        }
    }

    /// The parabola `(cd0, k)` with the table's zero-lift drag and its
    /// lift-dependent drag `CD(CL) - CD(0)` sampled every `step` from zero.
    pub fn from_table(cd0: f64, k: f64, zero_lift: f64, step: f64, lift_drag: Vec<f64>) -> Self {
        Self {
            cd0,
            k,
            zero_lift,
            step,
            lift_drag,
        }
    }

    /// Table drag at zero lift (the parabola's `cd0` for a pure parabola).
    pub fn zero_lift(&self) -> f64 {
        self.zero_lift
    }

    /// Lift-dependent drag `CD(CL) - CD(0)` in free air: linear
    /// interpolation of the table samples (error at most k step^2 / 4 = 1e-4 of CD for a
    /// 0.04 CL^2 curve at a 0.1 spacing), the parabola beyond the grid.
    pub fn lift_drag(&self, cl: f64) -> f64 {
        let top = self.step * (self.lift_drag.len().saturating_sub(1)) as f64;
        if self.lift_drag.len() < 2 || !(0.0..=top).contains(&cl) {
            return self.k * cl * cl;
        }
        let x = cl / self.step;
        let i = (x.floor() as usize).min(self.lift_drag.len() - 2);
        let w = x - i as f64;
        self.lift_drag[i] * (1.0 - w) + self.lift_drag[i + 1] * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_polar_interpolates_and_a_parabola_is_exact() {
        let step = 0.1;
        let samples: Vec<f64> = (0..=30).map(|i| 0.04 * (i as f64 * step).powi(2)).collect();
        let polar = FieldPolar::from_table(0.02, 0.04, 0.02, step, samples);
        for cl in [0.0, 0.33, 1.0, 1.88, 2.99] {
            assert!(
                (polar.lift_drag(cl) - 0.04 * cl * cl).abs() < 1.01e-4,
                "{cl}"
            );
        }
        // Beyond the grid the parabola takes over.
        assert!((polar.lift_drag(3.5) - 0.04 * 3.5 * 3.5).abs() < 1e-12);
        let pure = FieldPolar::parabola(0.02, 0.05);
        assert_eq!(pure.zero_lift(), 0.02);
        assert!((pure.lift_drag(1.5) - 0.05 * 2.25).abs() < 1e-15);
    }
}
