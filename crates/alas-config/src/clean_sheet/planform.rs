// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Projected area and mean aerodynamic chord of the transport planform.
//!
//! The planform is the straight-tapered root, side-of-body, kink and tip
//! polyline [`crate::WingConfig::transport_planform`] builds, so on every
//! panel the chord and the leading-edge station vary linearly with the
//! spanwise coordinate and the integrals below are exact:
//!
//! - `S = 2 sum dy (c_i + c_o) / 2`
//! - `c_bar = (2 / S) sum dy (c_i^2 + c_i c_o + c_o^2) / 3`
//! - `x_LE,mac = (2 / S) sum dy (2 c_i x_i + c_i x_o + c_o x_i + 2 c_o x_o) / 6`
//!
//! all in metres on the projected XY reference plane, `x` positive aft from
//! the wing-root datum, matching the builder's reference quantities.

use crate::{DesignVector, WingConfig};

/// The planform quantities a clean-sheet sizing reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WingPlanformSummary {
    /// Projected span, m.
    pub span_m: f64,
    /// Projected reference area, both semispans, m^2.
    pub area_m2: f64,
    /// Mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Quarter-MAC station aft of the wing-root datum, m.
    pub quarter_mac_x_m: f64,
}

impl WingPlanformSummary {
    /// Measure `wing` drawn with `design`; `None` when it does not build or
    /// the integrals are not finite and positive.
    pub fn of(wing: &WingConfig, design: &DesignVector) -> Option<Self> {
        let planform = wing.transport_planform(design).ok()?;
        let stations = planform.stations();
        let (mut half_area, mut chord_sq, mut chord_x) = (0.0, 0.0, 0.0);
        for pair in stations.windows(2) {
            let (i, o) = (pair[0], pair[1]);
            let dy = o.y_m - i.y_m;
            half_area += dy * 0.5 * (i.chord_m + o.chord_m);
            chord_sq += dy * (i.chord_m.powi(2) + i.chord_m * o.chord_m + o.chord_m.powi(2)) / 3.0;
            chord_x += dy
                * (2.0 * i.chord_m * i.leading_edge_x_m
                    + i.chord_m * o.leading_edge_x_m
                    + o.chord_m * i.leading_edge_x_m
                    + 2.0 * o.chord_m * o.leading_edge_x_m)
                / 6.0;
        }
        let area_m2 = 2.0 * half_area;
        if !(area_m2.is_finite() && area_m2 > 0.0) {
            return None;
        }
        let mac_m = chord_sq / half_area;
        let leading_edge_mac_x_m = chord_x / half_area;
        let summary = Self {
            span_m: design.span_m,
            area_m2,
            mac_m,
            quarter_mac_x_m: leading_edge_mac_x_m + 0.25 * mac_m,
        };
        (summary.mac_m.is_finite() && summary.quarter_mac_x_m.is_finite()).then_some(summary)
    }
}
