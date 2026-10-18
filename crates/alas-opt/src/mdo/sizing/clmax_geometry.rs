// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The geometry side of the CLmax model (AERL-01): what a candidate's built
//! wing says about its maximum lift, relative to the registered geometry the
//! declared coefficients belong to.
//!
//! The scaling itself is `alas_perf::performance::clmax`. This module only
//! measures the wing: the quarter-chord sweep (the same mean line the Korn
//! relation reads), the wing area spanned by the trailing-edge flap (full chord
//! integrated over the flap span stations of
//! `control_surfaces`), and the sweep of the hinge line. The reference
//! geometry is the one the configured nominal design builds, so a candidate
//! at that design returns `None` and keeps the declared values bit for bit.
//!
//! The reference coefficients are the configuration's own
//! `performance.cl_max_{clean,to,land}`;
//! `PerformanceConfig::geometry_scaled_clmax` switches the model off.

use alas_aero::analysis::AeroAnalysis;
use alas_config::control_surfaces::ControlSurfacesConfig;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_perf::performance::clmax::{ClmaxReference, ClmaxSet, WingLiftGeometry};

/// Linear interpolation of `(x_le, chord)` at spanwise station `y` along the
/// wing's sections (root to tip, increasing `y`).
fn section_at(wing: &Wing, y: f64) -> Option<(f64, f64)> {
    wing.xsecs.windows(2).find_map(|pair| {
        let (y0, y1) = (pair[0].xyz_le[1], pair[1].xyz_le[1]);
        (y1 > y0 && y >= y0 - 1e-12 && y <= y1 + 1e-12).then(|| {
            let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
            let lerp = |a: f64, b: f64| a + t * (b - a);
            (
                lerp(pair[0].xyz_le[0], pair[1].xyz_le[0]),
                lerp(pair[0].chord, pair[1].chord),
            )
        })
    })
}

/// `integral c dy` over `[y_in, y_out]` for the piecewise-linear chord.
fn chord_integral(wing: &Wing, y_in: f64, y_out: f64) -> f64 {
    let mut area = 0.0;
    for pair in wing.xsecs.windows(2) {
        let (y0, y1) = (pair[0].xyz_le[1], pair[1].xyz_le[1]);
        let (lo, hi) = (y0.max(y_in), y1.min(y_out));
        if y1 > y0 && hi > lo {
            let chord = |y: f64| {
                let t = (y - y0) / (y1 - y0);
                pair[0].chord + t * (pair[1].chord - pair[0].chord)
            };
            area += 0.5 * (chord(lo) + chord(hi)) * (hi - lo);
        }
    }
    area
}

/// The wing quantities the CLmax scaling reads, `None` for a wing the
/// measurement cannot be made on.
pub(super) fn measure(
    plane: &Airplane,
    controls: &ControlSurfacesConfig,
) -> Option<WingLiftGeometry> {
    let wing = plane.wings.first().filter(|wing| wing.xsecs.len() >= 2)?;
    let semi_span = wing
        .xsecs
        .iter()
        .map(|xsec| xsec.xyz_le[1])
        .fold(0.0, f64::max);
    let (start, end) = (controls.flap_span_start_frac, controls.flap_span_end_frac);
    let chord_fraction = controls.flap_chord_fraction;
    if !(semi_span > 0.0
        && plane.s_ref > 0.0
        && (0.0..1.0).contains(&start)
        && end > start
        && end <= 1.0
        && chord_fraction > 0.0
        && chord_fraction < 1.0)
    {
        return None;
    }
    let (y_in, y_out) = (start * semi_span, end * semi_span);
    // Both semispans when the wing is mirrored.
    let sides = if wing.symmetric { 2.0 } else { 1.0 };
    let flapped_area = sides * chord_integral(wing, y_in, y_out);
    let (x_in, c_in) = section_at(wing, y_in)?;
    let (x_out, c_out) = section_at(wing, y_out)?;
    let hinge = |x_le: f64, chord: f64| x_le + (1.0 - chord_fraction) * chord;
    let hinge_sweep_deg = (hinge(x_out, c_out) - hinge(x_in, c_in))
        .atan2(y_out - y_in)
        .to_degrees();
    let geometry = WingLiftGeometry {
        sweep_c4_deg: AeroAnalysis::quarter_chord_sweep_deg(plane, f64::NAN),
        flapped_area_ratio: flapped_area / plane.s_ref,
        hinge_sweep_deg,
    };
    (geometry.sweep_c4_deg.is_finite() && geometry.hinge_sweep_deg.is_finite()).then_some(geometry)
}

/// The scaled coefficients of the candidate wing `plane`, relative to the
/// registered geometry of `config`; `None` when the model is off, the
/// candidate is the registered geometry, or either wing cannot be measured.
pub(super) fn candidate_set(config: &AlasConfig, plane: &Airplane) -> Option<ClmaxSet> {
    let performance = &config.performance;
    if !performance.geometry_scaled_clmax {
        return None;
    }
    let reference_plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&config.configured_nominal_design()), true)
        .ok()?;
    let reference = ClmaxReference {
        set: ClmaxSet {
            clean: performance.cl_max_clean,
            takeoff: performance.cl_max_to,
            landing: performance.cl_max_land,
        },
        geometry: measure(&reference_plane, &config.control_surfaces)?,
    };
    reference.scale(&measure(plane, &config.control_surfaces)?)
}

/// The declared coefficients of `config`: the registered aircraft's.
pub(crate) fn declared_set(config: &AlasConfig) -> ClmaxSet {
    ClmaxSet {
        clean: config.performance.cl_max_clean,
        takeoff: config.performance.cl_max_to,
        landing: config.performance.cl_max_land,
    }
}

/// `config` with the scaled coefficients written into its performance group.
pub(crate) fn with_clmax(config: &AlasConfig, set: &ClmaxSet) -> AlasConfig {
    let mut scaled = config.clone();
    scaled.performance.cl_max_clean = set.clean;
    scaled.performance.cl_max_to = set.takeoff;
    scaled.performance.cl_max_land = set.landing;
    scaled
}

// A test builds its own configurations, so a failed unwrap or expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::DesignVector;

    fn built(config: &AlasConfig, x: &DesignVector) -> Airplane {
        AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(x), true)
            .expect("the registered preset builds")
    }

    #[test]
    fn a_registered_preset_at_its_own_design_keeps_its_declared_values() {
        for name in ["A320-200", "B787-9", "ATR72-600"] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
            let plane = built(&config, &config.configured_nominal_design());
            assert_eq!(candidate_set(&config, &plane), None, "{name}");
            let geometry = measure(&plane, &config.control_surfaces).expect("measurable");
            assert!(geometry.sweep_c4_deg.abs() < 45.0, "{name}");
            assert!(geometry.flapped_area_ratio > 0.2 && geometry.flapped_area_ratio < 0.9);
        }
    }

    #[test]
    fn more_sweep_lowers_the_clean_value_by_the_cosine_ratio_and_the_switch_disables_it() {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        let nominal = config.configured_nominal_design();
        let reference = built(&config, &nominal);
        let swept = DesignVector {
            sweep_deg: nominal.sweep_deg + 4.0,
            ..nominal
        };
        let plane = built(&config, &swept);
        let set = candidate_set(&config, &plane).expect("moved wing");
        let g0 = measure(&reference, &config.control_surfaces).unwrap();
        let g1 = measure(&plane, &config.control_surfaces).unwrap();
        assert!(g1.sweep_c4_deg > g0.sweep_c4_deg);
        let expected = config.performance.cl_max_clean * g1.sweep_c4_deg.to_radians().cos()
            / g0.sweep_c4_deg.to_radians().cos();
        assert!((set.clean - expected).abs() < 1e-12);
        assert!(set.clean < config.performance.cl_max_clean);
        assert!(set.clean < set.takeoff && set.takeoff < set.landing);
        config.performance.geometry_scaled_clmax = false;
        assert_eq!(candidate_set(&config, &plane), None);
    }
}
