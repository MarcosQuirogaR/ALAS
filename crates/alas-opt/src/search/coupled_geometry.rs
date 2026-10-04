// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-based sampling couplings, followed by the unchanged hard gates.
//! The tank-volume proxy and gross-mass Korn estimate guide draws only;
//! neither substitutes for the candidate's sized tank, trim or CG assessment.

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
        let nominal_volume = projection.chord_integrals(&self.nominal).2;
        let candidate_volume = projection.chord_integrals(point).2;
        let mut minimum_thickness = bounds[THICKNESS].0;
        if candidate_volume > 0.0 {
            let minimum = nominal_volume / candidate_volume * self.nominal[THICKNESS];
            minimum_thickness = minimum.clamp(bounds[THICKNESS].0, bounds[THICKNESS].1);
            point[THICKNESS] = point[THICKNESS]
                .max(minimum)
                .clamp(bounds[THICKNESS].0, bounds[THICKNESS].1);
        }
        if self.mach >= self.onset_mach && self.divergence(projection, point, point[4]) < self.mach
        {
            let (mut lower, mut upper) = (point[4], bounds[4].1);
            if self.divergence(projection, point, upper) < self.mach {
                let cos = self.sweep_cos(projection, point, upper);
                let cl = self.lift_area_m2 / projection.area_m2(point);
                let maximum = cos.powi(2)
                    * (self.kappa / cos - cl / (10.0 * cos.powi(3)) - self.mach)
                    / self.thickness
                    * self.nominal[THICKNESS];
                point[THICKNESS] = point[THICKNESS]
                    .min(maximum)
                    .clamp(minimum_thickness, bounds[THICKNESS].1);
            }
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

#[cfg(test)]
// Registered configuration and geometry failures are assertion failures.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::DesignOptimizer;

    #[test]
    fn volume_repair_changes_thickness_and_preserves_independent_camber() {
        for preset in presets::registry() {
            let config = AlasConfig::from_value(&serde_json::json!({"preset":preset.name}))
                .expect("registered preset");
            let optimizer = DesignOptimizer::new(config.clone());
            let (bounds, nominal) = optimizer.anchored_search_space().expect("anchored box");
            let nominal = nominal.to_array();
            let projection = PlanformProjection::new(&config, &bounds).expect("planform");
            let mut point = nominal.clone();
            for chord in &mut point[1..=3] {
                *chord *= 0.9;
            }
            point[THICKNESS] = bounds[THICKNESS].0;
            point[11] = bounds[11].0;
            let camber_bits = point[11].to_bits();
            let minimum = (projection.chord_integrals(&nominal).2
                / projection.chord_integrals(&point).2
                * nominal[THICKNESS])
                .clamp(bounds[THICKNESS].0, bounds[THICKNESS].1);
            assert!(minimum > point[THICKNESS], "{} volume deficit", preset.name);
            projection.repair_coupled(&mut point, &bounds, &nominal, 0.5);
            assert!(point[THICKNESS] >= minimum, "{} thickness", preset.name);
            assert_eq!(point[11].to_bits(), camber_bits, "{} camber", preset.name);
        }
    }
}
