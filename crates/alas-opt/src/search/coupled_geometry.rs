// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-based sampling couplings, followed by the unchanged hard gates.
//! The gross-mass Korn estimate guides the sweep draw only; it does not
//! substitute for the candidate's sized tank, trim or CG assessment. Every
//! design mode holds the section thickness at its nominal, so the repair
//! moves sweep and the wing station, never the thickness.

use alas_aero::analysis::AeroAnalysis;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

use super::planform_projection::PlanformProjection;

const THICKNESS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CoupledGeometry {
    nominal: [f64; 16],
    thickness: f64,
    kappa: f64,
    mach: f64,
    onset_mach: f64,
    /// MTOW times gravity divided by sizing cruise dynamic pressure, m^2.
    lift_area_m2: f64,
}

impl CoupledGeometry {
    pub(super) fn new(config: &AlasConfig) -> Option<Self> {
        let design = presets::get(&config.preset).ok()?.design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        let wing = plane.wings.first()?;
        let req = &config.requirements;
        let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
        let speed = req.cruise_mach * atmosphere.speed_of_sound();
        let q_pa = 0.5 * atmosphere.density() * speed * speed;
        Some(Self {
            nominal: design.to_array().try_into().ok()?,
            thickness: AeroAnalysis::area_weighted_thickness(wing),
            kappa: config.geometry.wing.airfoil_class.korn_technology_factor(),
            mach: req.cruise_mach,
            onset_mach: config.drag_model.wave_drag_onset_mach,
            lift_area_m2: req.mtow_kg * req.gravity_m_s2 / q_pa,
        })
    }

    pub(super) fn repair(
        &self,
        projection: &PlanformProjection,
        point: &mut [f64],
        bounds: &[(f64, f64)],
        anchor: &[f64],
        sweep_fraction: f64,
    ) {
        if self.mach >= self.onset_mach && self.divergence(projection, point, point[4]) < self.mach
        {
            let (mut lower, mut upper) = (point[4], bounds[4].1);
            if self.divergence(projection, point, upper) >= self.mach {
                for _ in 0..48 {
                    let middle = 0.5 * (lower + upper);
                    if self.divergence(projection, point, middle) < self.mach {
                        lower = middle;
                    } else {
                        upper = middle;
                    }
                }
                point[4] = upper + sweep_fraction * (bounds[4].1 - upper);
            } else {
                point[4] = upper;
            }
        }
        // Preserve the anchor's quarter-chord station as sweep, span and
        // chords change. The residual CG draw remains a translation in +x
        // aft from the fuselage nose, m, within the declared coordinate box.
        let shift =
            projection.quarter_chord_station(anchor) - projection.quarter_chord_station(point);
        point[6] = (point[6] + shift).clamp(bounds[6].0, bounds[6].1);
    }

    fn divergence(&self, projection: &PlanformProjection, point: &[f64], sweep: f64) -> f64 {
        let thickness = self.thickness * point[THICKNESS] / self.nominal[THICKNESS];
        let cl = self.lift_area_m2 / projection.area_m2(point);
        let cos = self.sweep_cos(projection, point, sweep);
        // The existing Korn relation at gross-mass CL guides the draw;
        // full evaluation uses its own mid-cruise mass and
        // area-weighted morphed sections (analysis::wave).
        self.kappa / cos - thickness / cos.powi(2) - cl / (10.0 * cos.powi(3))
    }

    fn sweep_cos(&self, projection: &PlanformProjection, point: &[f64], sweep: f64) -> f64 {
        let quarter_slope = sweep.to_radians().tan()
            + 0.25 * (point[3] - point[2]) / projection.outboard_span_m(point);
        quarter_slope.atan().cos()
    }
}
