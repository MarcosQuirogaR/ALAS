// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Derived queries of the trimmed drag table: its tangent parabola and the
//! lift coefficient of maximum `CL / CD`.

use super::TrimmedDragTable;

/// Which side of a solved interval a query fell on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSide {
    /// Below the lowest solved node.
    Below,
    /// Above the highest solved node.
    Above,
    /// The query is `NaN`.
    NotANumber,
}

impl RangeSide {
    fn of(value: f64, (low, high): (f64, f64)) -> Option<Self> {
        if value.is_nan() {
            Some(Self::NotANumber)
        } else if value < low {
            Some(Self::Below)
        } else if value > high {
            Some(Self::Above)
        } else {
            None
        }
    }
}

/// The intervals of lift coefficient and Mach number a
/// [`TrimmedDragTable`] was solved on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragTableRange {
    /// Lowest and highest trimmed lift coefficient of the induced-drag
    /// curve. Outside it the end quadratic is extrapolated.
    pub induced_cl: (f64, f64),
    /// Lowest and highest lift coefficient of the wave-drag grid. Outside it
    /// the wave drag is held at the edge value.
    pub wave_cl: (f64, f64),
    /// Lowest and highest Mach node. Outside it the Mach columns are held at
    /// the edge value.
    pub mach: (f64, f64),
}

/// How a `(CL, Mach)` query left the solved range of a
/// [`TrimmedDragTable`]; every field `None` is not representable (an
/// in-range query has no value of this type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragTableOutOfRange {
    /// The lift coefficient against the trimmed induced-drag nodes.
    pub induced_cl: Option<RangeSide>,
    /// The lift coefficient against the wave-drag grid.
    pub wave_cl: Option<RangeSide>,
    /// The Mach number against the Mach nodes.
    pub mach: Option<RangeSide>,
}

impl TrimmedDragTable {
    /// The lift-coefficient and Mach intervals the table was solved on.
    pub fn solved_range(&self) -> DragTableRange {
        let induced_cl = self
            .induced
            .ranges()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), piece| {
                (low.min(piece.0), high.max(piece.1))
            });
        let ends = |nodes: &[f64]| {
            (
                nodes.first().copied().unwrap_or(f64::NAN),
                nodes.last().copied().unwrap_or(f64::NAN),
            )
        };
        DragTableRange {
            induced_cl,
            wave_cl: ends(&self.cl),
            mach: ends(&self.mach),
        }
    }

    /// Whether, and how, `(cl, mach)` lies outside [`Self::solved_range`].
    ///
    /// [`Self::cd`] still returns a number there: the induced drag from the
    /// end quadratic of the trimmed curve, the wave drag and the Mach columns
    /// held at the grid edge. That number is an extrapolation, not a solved
    /// point, and a caller that reports it must say so.
    pub fn out_of_range(&self, cl: f64, mach: f64) -> Option<DragTableOutOfRange> {
        let range = self.solved_range();
        let excursion = DragTableOutOfRange {
            induced_cl: RangeSide::of(cl, range.induced_cl),
            wave_cl: RangeSide::of(cl, range.wave_cl),
            mach: RangeSide::of(mach, range.mach),
        };
        (excursion.induced_cl.is_some() || excursion.wave_cl.is_some() || excursion.mach.is_some())
            .then_some(excursion)
    }

    /// [`Self::cd`] with the out-of-range state of the query.
    pub fn cd_checked(
        &self,
        cl: f64,
        mach: f64,
        altitude_m: f64,
    ) -> (f64, Option<DragTableOutOfRange>) {
        (self.cd(cl, mach, altitude_m), self.out_of_range(cl, mach))
    }

    /// The parabola `cd0 + k CL^2` tangent to the table at the design lift
    /// coefficient, Mach `mach` and the reference altitude: it reproduces
    /// the drag and its CL slope there, wave drag included.
    pub fn parabolic_equivalent(&self, mach: f64) -> (f64, f64) {
        self.parabolic_equivalent_at(self.design_cl, mach, self.reference_altitude_m)
    }

    /// The parabola `cd0 + k CL^2` tangent to the shared table at positive
    /// `cl`, `mach` and `altitude_m`, retaining trim and wave drag's local slope.
    pub fn parabolic_equivalent_at(&self, cl: f64, mach: f64, altitude_m: f64) -> (f64, f64) {
        let cd = self.cd(cl, mach, altitude_m);
        let cd0 = self.cd0(mach, altitude_m);
        let slope = self.induced_at(cl, mach).1
            + self.upsweep_attitude_value_and_slope(cl, mach).1
            + self.wave_value_and_slope(cl, mach).1
            + 2.0 * self.viscous_lift_fraction * cd0 * cl;
        let k = slope / (2.0 * cl);
        (cd - k * cl * cl, k)
    }

    /// Lift coefficient of maximum `CL/CD` (minimum drag force) at `mach`
    /// and `altitude_m`, by golden-section search over the CL grid.
    pub fn min_drag_cl(&self, mach: f64, altitude_m: f64) -> f64 {
        let ratio = |cl: f64| cl / self.cd(cl, mach, altitude_m);
        let golden = 0.5 * (5.0_f64.sqrt() - 1.0);
        let cl_min = self.cl[0];
        let cl_max = self.cl[self.cl.len() - 1];
        self.induced
            .ranges()
            .filter_map(|(low, high)| {
                let (mut low, mut high) = (low.max(cl_min), high.min(cl_max));
                if low >= high {
                    return None;
                }
                // 60 contractions by 0.618 shrink the bracket below 1e-12 of its size.
                for _ in 0..60 {
                    let a = high - golden * (high - low);
                    let b = low + golden * (high - low);
                    if ratio(a) < ratio(b) {
                        low = a;
                    } else {
                        high = b;
                    }
                }
                Some(0.5 * (low + high))
            })
            .max_by(|left, right| ratio(*left).total_cmp(&ratio(*right)))
            .unwrap_or(cl_min)
    }
}
