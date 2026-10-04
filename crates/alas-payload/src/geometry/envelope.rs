// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wall-inset elliptical envelope containment checks for [`super::CabinGeometry`].

use super::{CabinGeometry, InteriorEnvelopeError};

/// Numerical allowance used when testing a point against the inner ellipse.
const ENVELOPE_TOLERANCE: f64 = 1e-9;

/// Maximum longitudinal spacing between strict envelope evaluations.
const MAX_CONTAINMENT_STEP_M: f64 = 0.10;

impl CabinGeometry {
    /// Actual semi-axes of the wall-inset elliptical envelope at `x`.
    ///
    /// Unlike [`Self::internal_half_height`], this strict geometry does not
    /// invent space in a tapered section. `None` means the lining consumes
    /// the complete local section.
    pub fn inner_semi_axes(&self, x: f64) -> Option<(f64, f64)> {
        let half_width = self.width_at(x) * 0.5 - self.wall;
        let half_height = self.height_at(x) * 0.5 - self.wall;
        if half_width > 0.0 && half_height > 0.0 {
            Some((half_width, half_height))
        } else {
            None
        }
    }

    /// Internal fuselage width available at an absolute vertical station.
    ///
    /// The conceptual fuselage sections are elliptical, so crown furniture
    /// cannot reuse the floor chord without protruding through the sidewall.
    pub fn usable_width_at_z(&self, x: f64, z: f64) -> f64 {
        let Some((half_width, half_height)) = self.inner_semi_axes(x) else {
            return 0.0;
        };
        let normalized_z = (z - self.zc_at(x)) / half_height.max(1e-6);
        if normalized_z.abs() >= 1.0 {
            return 0.0;
        }
        2.0 * half_width * (1.0 - normalized_z * normalized_z).sqrt()
    }

    /// Whether a point lies in the station-dependent wall-inset ellipse.
    pub fn contains_point(&self, x: f64, y: f64, z: f64) -> bool {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() || x < self.x_min || x > self.x_max {
            return false;
        }
        let Some((half_width, half_height)) = self.inner_semi_axes(x) else {
            return false;
        };
        let normalized_y = y / half_width;
        let normalized_z = (z - self.zc_at(x)) / half_height;
        normalized_y.mul_add(normalized_y, normalized_z * normalized_z) <= 1.0 + ENVELOPE_TOLERANCE
    }

    /// Check a constant cross-section polygon throughout a longitudinal span.
    ///
    /// `vertices_yz` are absolute `(y, z)` coordinates. The check includes
    /// both item ends and every fuselage definition station inside the span,
    /// so nose/tail taper and centreline upsweep cannot be skipped by a check
    /// performed only at the item's centre.
    pub fn check_polygon_containment(
        &self,
        x_start: f64,
        x_end: f64,
        vertices_yz: &[[f64; 2]],
    ) -> Result<(), InteriorEnvelopeError> {
        if !self.strict_envelope {
            return Ok(());
        }
        if !x_start.is_finite()
            || !x_end.is_finite()
            || x_start > x_end
            || vertices_yz.is_empty()
            || vertices_yz
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
        {
            return Err(InteriorEnvelopeError::InvalidExtent);
        }

        let breakpoints: Vec<f64> = std::iter::once(x_start)
            .chain(
                self.x_stations
                    .iter()
                    .copied()
                    .filter(|x| *x > x_start && *x < x_end),
            )
            .chain(std::iter::once(x_end))
            .collect();
        for interval in breakpoints.windows(2) {
            let interval_length = interval[1] - interval[0];
            let steps = (interval_length / MAX_CONTAINMENT_STEP_M).ceil().max(1.0) as usize;
            for step in 0..=steps {
                let x = interval[0] + interval_length * step as f64 / steps as f64;
                for &[y, z] in vertices_yz {
                    if !self.contains_point(x, y, z) {
                        return Err(InteriorEnvelopeError::OutsideEnvelope { x, y, z });
                    }
                }
            }
        }
        Ok(())
    }

    /// Check all eight corners of an axis-aligned rectangular installation.
    pub fn check_rectangular_prism(
        &self,
        x_center: f64,
        length: f64,
        y_center: f64,
        width: f64,
        z_bottom: f64,
        height: f64,
    ) -> Result<(), InteriorEnvelopeError> {
        if !x_center.is_finite()
            || !length.is_finite()
            || !y_center.is_finite()
            || !width.is_finite()
            || !z_bottom.is_finite()
            || !height.is_finite()
            || length < 0.0
            || width < 0.0
            || height < 0.0
        {
            return Err(InteriorEnvelopeError::InvalidExtent);
        }
        let half_width = width * 0.5;
        let vertices = [
            [y_center - half_width, z_bottom],
            [y_center + half_width, z_bottom],
            [y_center - half_width, z_bottom + height],
            [y_center + half_width, z_bottom + height],
        ];
        self.check_polygon_containment(x_center - length * 0.5, x_center + length * 0.5, &vertices)
    }
}
