// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A mixture of domain coverage and nominal neighbourhoods, with wing area
//! sampled independently of aspect ratio instead of four independent lengths.

use super::planform_projection::PlanformProjection;

/// Keep every fourth point over the entire declared box; the others explore
/// a quarter of each nominal window. These are search distribution choices,
/// never feasibility tolerances or changes to the declared design envelope.
const LOCAL_WINDOW_FRACTION: f64 = 0.25;
const COVERAGE_PERIOD: usize = 4;

pub(super) fn apply(
    projection: &PlanformProjection,
    point: &mut [f64],
    bounds: &[(f64, f64)],
    nominal: &[f64],
    index: usize,
) {
    if point.len() != nominal.len() || point.len() != bounds.len() {
        projection.apply(point);
        return;
    }
    let local = index % COVERAGE_PERIOD != 0;
    let area_fraction = fraction(point[1], bounds[1]);
    let sweep_fraction = fraction(point[4], bounds[4]);
    if local {
        for ((value, &center), &(lo, hi)) in point.iter_mut().zip(nominal).zip(bounds) {
            let draw = fraction(*value, (lo, hi));
            let lower = (center - LOCAL_WINDOW_FRACTION * (center - lo)).clamp(lo, hi);
            let upper = (center + LOCAL_WINDOW_FRACTION * (hi - center)).clamp(lo, hi);
            *value = lower + draw * (upper - lower);
        }
        if let Some((lower, upper)) = projection.root_chord_interval(point) {
            point[1] = point[1].clamp(lower, upper);
        }
    } else {
        projection.apply(point);
        return;
    }
    // The reference MTOW and tank inventory require a wing comparable to
    // the reference. Retain that area while span varies, rather than drawing
    // wing loading and tank volume independently. Full physics checks the
    // resulting geometry, including fuel capacity and buffet, afterwards.
    let nominal_area = projection.area_m2(nominal);
    let current_area = projection.area_m2(point);
    let ceiling = projection.area_limit_m2();
    // The area spans the whole admissible interval, from the reference area
    // (tank volume at the declared MTOW) to the wing-area limit, rather than
    // the quarter window of the other coordinates: on registered aircraft
    // whose own reference fails a balance limit (forward CG at maximum fuel),
    // the feasible designs lie at larger chords, outside that window.
    let lower = nominal_area.min(ceiling);
    let upper = ceiling;
    let target = lower + area_fraction * (upper - lower).max(0.0);
    if current_area > 0.0 && target.is_finite() && target > 0.0 {
        let scale = target / current_area;
        for index in 1..=3 {
            point[index] = (point[index] * scale).clamp(bounds[index].0, bounds[index].1);
        }
        if let Some((lower, upper)) = projection.root_chord_interval(point) {
            point[1] = point[1].clamp(lower, upper);
        }
    }
    projection.repair_coupled(point, bounds, nominal, sweep_fraction);
}

fn fraction(value: f64, (lower, upper): (f64, f64)) -> f64 {
    if upper > lower {
        ((value - lower) / (upper - lower)).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

#[cfg(test)]
// A failed registered fixture or geometry build is the assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{search::screening, DesignOptimizer};
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn coupled_samples_keep_nominal_and_bounds_and_replay_as_a_prefix() {
        for preset in presets::registry() {
            let config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
                .expect("registered preset");
            let optimizer = DesignOptimizer::new(config.clone());
            let (bounds, nominal) = optimizer.anchored_search_space().expect("anchored box");
            let nominal = nominal.to_array();
            let projection = PlanformProjection::new(&config, &bounds).expect("planform");
            let points = screening::sample(&bounds, Some(&nominal), 256, 7, Some(&projection));
            assert_eq!(points[0], nominal);
            assert_eq!(
                points[..70],
                screening::sample(&bounds, Some(&nominal), 70, 7, Some(&projection))
            );
            for point in &points {
                for (&value, &(lo, hi)) in point.iter().zip(&bounds) {
                    assert!((lo..=hi).contains(&value), "{}: {value}", preset.name);
                }
            }
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .expect("reference builds");
            let area = projection.area_m2(&preset.design_vector.to_array());
            assert!(
                (area - plane.wings[0].projected_area()).abs() < 64.0 * f64::EPSILON * area,
                "{}: analytical {area}, built {}",
                preset.name,
                plane.wings[0].projected_area()
            );
        }
    }
}
