// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary transport-planform packaging metrics for the optimizer.
//!
//! A planform needs more than area and span to be a plausible airliner wing:
//! the inboard box carries bending load and fuel, and the trailing edge must
//! retain room for high-lift devices. These metrics are intentionally simple
//! geometric guards, not substitutes for detailed structures, fuel-system, or
//! landing-gear design.

use alas_config::{
    AlasConfig, ControlSurfacesConfig, DesignVector, MainWingStation, ObjectiveWeights,
    StructuresConfig, TransportPlanform,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};

const WINGBOX_THICKNESS_SAMPLES: usize = 41;

/// Geometric quantities used by the transport-planform objective constraints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportPlanformAssessment {
    /// Maximum available depth in the root spar box, in metres.
    pub root_wingbox_depth_m: f64,
    /// Maximum available depth in the kink spar box, in metres.
    pub kink_wingbox_depth_m: f64,
    /// Front-to-rear spar separation at the kink, in metres.
    pub kink_wingbox_width_m: f64,
    /// Volume in the configured tankable spar-box span, before usable fraction.
    pub tankable_wingbox_volume_m3: f64,
    /// Physical configured flap area divided by projected wing area.
    pub flap_area_fraction: f64,
    /// Projected-span-squared divided by root box width times depth.
    pub root_bending_box_slenderness: f64,
    /// Trailing-edge sweep of the inboard panel ending at the kink, in degrees.
    pub inboard_trailing_edge_sweep_deg: f64,
    /// Kink chord divided by root chord.
    pub break_root_chord_ratio: f64,
    /// Tip chord divided by root chord.
    pub tip_root_chord_ratio: f64,
}

/// Assess the built wing against transport-specific packaging geometry.
///
/// Returns `None` when no finite spar box, station, or span interval can be
/// derived. The caller must treat that as an invalid candidate rather than
/// assigning it a favorable zero penalty.
pub fn assess_transport_planform(
    wing: &Wing,
    planform: &TransportPlanform,
    structures: &StructuresConfig,
    controls: &ControlSurfacesConfig,
    weights: &ObjectiveWeights,
) -> Option<TransportPlanformAssessment> {
    let (front_spar_x_over_c, rear_spar_x_over_c) = spar_box_limits(structures)?;
    let root = nearest_section(wing, planform.root)?;
    let kink = nearest_section(wing, planform.kink)?;
    let root_metrics = wingbox_section_metrics(root, front_spar_x_over_c, rear_spar_x_over_c)?;
    let kink_metrics = wingbox_section_metrics(kink, front_spar_x_over_c, rear_spar_x_over_c)?;
    let tankable_wingbox_volume_m3 = wingbox_volume_m3(
        wing,
        front_spar_x_over_c,
        rear_spar_x_over_c,
        weights.tankable_span_start_fraction,
        weights.tankable_span_end_fraction,
        planform.root.y_m,
        planform.tip.y_m,
    )?;
    let flap_area_fraction =
        flap_area_fraction(wing, controls, planform.root.y_m, planform.tip.y_m)?;
    let projected_span_m = wing.projected_span();
    let root_bending_box_slenderness =
        projected_span_m.powi(2) / (root_metrics.width_m * root_metrics.depth_m);
    let inboard_trailing_edge_sweep_deg = planform
        .panels()
        .into_iter()
        .find(|panel| panel.outboard.kind == alas_config::MainWingStationKind::Kink)?
        .trailing_edge_sweep_deg;

    let assessment = TransportPlanformAssessment {
        root_wingbox_depth_m: root_metrics.depth_m,
        kink_wingbox_depth_m: kink_metrics.depth_m,
        kink_wingbox_width_m: kink_metrics.width_m,
        tankable_wingbox_volume_m3,
        flap_area_fraction,
        root_bending_box_slenderness,
        inboard_trailing_edge_sweep_deg,
        break_root_chord_ratio: planform.kink.chord_m / planform.root.chord_m,
        tip_root_chord_ratio: planform.tip.chord_m / planform.root.chord_m,
    };
    assessment_is_finite(assessment).then_some(assessment)
}

/// Assess the main wing built for a product optimization candidate.
///
/// This resolves the shared transport-planform geometry before sampling the
/// actual airfoil/wingbox sections, keeping the builder, objective, and
/// reports on one leading- and trailing-edge definition.
pub fn assess_product_transport_planform(
    plane: &Airplane,
    design: &DesignVector,
    config: &AlasConfig,
) -> Option<TransportPlanformAssessment> {
    let planform = config.geometry.wing.transport_planform(design).ok()?;
    let main_wing = plane.wings.first()?;
    assess_transport_planform(
        main_wing,
        &planform,
        &config.structures,
        &config.control_surfaces,
        &config.optimizer.weights,
    )
}

/// Angle of the built wing's exposed trailing edge, side-of-body to kink, to
/// the aft fuselage axis, degrees; `None` when the planform does not resolve.
///
/// 90 degrees is an unswept edge; more is an edge running forward. The inboard
/// end is the side-of-body section when the builder lofts one (a pinned
/// `side_of_body_chord_ratio`, the only case the built wing has that station)
/// and the centreline root otherwise, which is then the built inboard station.
/// It needs only the planform stations, not a built aircraft, so a pre-gate
/// can call it on a design vector and the wing configuration.
pub fn exposed_te_angle_deg(wing: &alas_config::WingConfig, design: &DesignVector) -> Option<f64> {
    let planform = wing.transport_planform(design).ok()?;
    let inboard = planform
        .side_of_body
        .filter(|_| wing.side_of_body_chord_ratio.is_some())
        .unwrap_or(planform.root);
    let te_x = |s: &MainWingStation| s.leading_edge_x_m + s.chord_m;
    let angle = (planform.kink.y_m - inboard.y_m)
        .atan2(te_x(&planform.kink) - te_x(&inboard))
        .to_degrees();
    angle.is_finite().then_some(angle)
}

#[derive(Debug, Clone, Copy)]
struct WingboxSectionMetrics {
    depth_m: f64,
    width_m: f64,
    cross_section_area_m2: f64,
}

fn spar_box_limits(structures: &StructuresConfig) -> Option<(f64, f64)> {
    let (front, rear) = structures
        .spar_chord_fractions
        .iter()
        .copied()
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
        .fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(front, rear), fraction| (front.min(fraction), rear.max(fraction)),
        );
    (front.is_finite() && rear.is_finite() && rear > front).then_some((front, rear))
}

fn nearest_section(wing: &Wing, station: MainWingStation) -> Option<&WingXSec> {
    wing.xsecs.iter().min_by(|left, right| {
        (left.xyz_le[1] - station.y_m)
            .abs()
            .total_cmp(&(right.xyz_le[1] - station.y_m).abs())
    })
}

fn wingbox_section_metrics(
    section: &WingXSec,
    front_spar_x_over_c: f64,
    rear_spar_x_over_c: f64,
) -> Option<WingboxSectionMetrics> {
    if !section.chord.is_finite() || section.chord <= 0.0 {
        return None;
    }
    let samples = linspace(
        front_spar_x_over_c,
        rear_spar_x_over_c,
        WINGBOX_THICKNESS_SAMPLES,
    );
    let thickness = section.airfoil.local_thickness(&samples);
    if thickness.len() != samples.len() || thickness.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let depth_m = thickness.iter().copied().fold(f64::NEG_INFINITY, f64::max) * section.chord;
    let thickness_integral = thickness
        .windows(2)
        .zip(samples.windows(2))
        .map(|(thickness_pair, x_pair)| {
            (thickness_pair[0] + thickness_pair[1]) * (x_pair[1] - x_pair[0]) / 2.0
        })
        .sum::<f64>();
    let width_m = (rear_spar_x_over_c - front_spar_x_over_c) * section.chord;
    let cross_section_area_m2 = thickness_integral * section.chord.powi(2);
    (depth_m.is_finite()
        && depth_m > 0.0
        && width_m.is_finite()
        && width_m > 0.0
        && cross_section_area_m2.is_finite()
        && cross_section_area_m2 > 0.0)
        .then_some(WingboxSectionMetrics {
            depth_m,
            width_m,
            cross_section_area_m2,
        })
}

fn wingbox_volume_m3(
    wing: &Wing,
    front_spar_x_over_c: f64,
    rear_spar_x_over_c: f64,
    start_span_fraction: f64,
    end_span_fraction: f64,
    root_y_m: f64,
    tip_y_m: f64,
) -> Option<f64> {
    if !start_span_fraction.is_finite()
        || !end_span_fraction.is_finite()
        || !(0.0..=1.0).contains(&start_span_fraction)
        || !(0.0..=1.0).contains(&end_span_fraction)
        || end_span_fraction <= start_span_fraction
        || !root_y_m.is_finite()
        || !tip_y_m.is_finite()
        || tip_y_m <= root_y_m
    {
        return None;
    }
    let span_m = tip_y_m - root_y_m;
    let start_y_m = root_y_m + start_span_fraction * span_m;
    let end_y_m = root_y_m + end_span_fraction * span_m;
    let mut half_volume_m3 = 0.0;
    for pair in wing.xsecs.windows(2) {
        let y_a = pair[0].xyz_le[1];
        let y_b = pair[1].xyz_le[1];
        let segment_start_y_m = y_a.min(y_b);
        let segment_end_y_m = y_a.max(y_b);
        let overlap_m = (segment_end_y_m.min(end_y_m) - segment_start_y_m.max(start_y_m)).max(0.0);
        if overlap_m == 0.0 {
            continue;
        }
        let inboard = wingbox_section_metrics(&pair[0], front_spar_x_over_c, rear_spar_x_over_c)?;
        let outboard = wingbox_section_metrics(&pair[1], front_spar_x_over_c, rear_spar_x_over_c)?;
        half_volume_m3 +=
            overlap_m * (inboard.cross_section_area_m2 + outboard.cross_section_area_m2) / 2.0;
    }
    let symmetry_factor = if wing.symmetric { 2.0 } else { 1.0 };
    let volume_m3 = symmetry_factor * half_volume_m3;
    (volume_m3.is_finite() && volume_m3 > 0.0).then_some(volume_m3)
}

fn flap_area_fraction(
    wing: &Wing,
    controls: &ControlSurfacesConfig,
    root_y_m: f64,
    tip_y_m: f64,
) -> Option<f64> {
    if !controls.flap_span_start_frac.is_finite()
        || !controls.flap_span_end_frac.is_finite()
        || !controls.flap_chord_fraction.is_finite()
        || !(0.0..=1.0).contains(&controls.flap_span_start_frac)
        || !(0.0..=1.0).contains(&controls.flap_span_end_frac)
        || !(0.0..=1.0).contains(&controls.flap_chord_fraction)
        || controls.flap_span_end_frac <= controls.flap_span_start_frac
        || tip_y_m <= root_y_m
    {
        return None;
    }
    let span_m = tip_y_m - root_y_m;
    let start_y_m = root_y_m + controls.flap_span_start_frac * span_m;
    let end_y_m = root_y_m + controls.flap_span_end_frac * span_m;
    let mut half_flap_area_m2 = 0.0;
    for pair in wing.xsecs.windows(2) {
        let y_a = pair[0].xyz_le[1];
        let y_b = pair[1].xyz_le[1];
        let overlap_m = (y_a.max(y_b).min(end_y_m) - y_a.min(y_b).max(start_y_m)).max(0.0);
        if overlap_m > 0.0 {
            half_flap_area_m2 +=
                overlap_m * (pair[0].chord + pair[1].chord) * controls.flap_chord_fraction / 2.0;
        }
    }
    let symmetry_factor = if wing.symmetric { 2.0 } else { 1.0 };
    let projected_wing_area_m2 = wing.projected_area();
    let area_fraction = symmetry_factor * half_flap_area_m2 / projected_wing_area_m2;
    (area_fraction.is_finite() && area_fraction >= 0.0).then_some(area_fraction)
}

fn assessment_is_finite(assessment: TransportPlanformAssessment) -> bool {
    [
        assessment.root_wingbox_depth_m,
        assessment.kink_wingbox_depth_m,
        assessment.kink_wingbox_width_m,
        assessment.tankable_wingbox_volume_m3,
        assessment.flap_area_fraction,
        assessment.root_bending_box_slenderness,
        assessment.inboard_trailing_edge_sweep_deg,
        assessment.break_root_chord_ratio,
        assessment.tip_root_chord_ratio,
    ]
    .into_iter()
    .all(f64::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;

    fn nominal_assessment() -> (TransportPlanformAssessment, ObjectiveWeights) {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .expect("the default transport builds");
        let planform = config
            .geometry
            .wing
            .transport_planform(&design)
            .expect("the default planform is valid");
        let assessment = assess_transport_planform(
            &plane.wings[0],
            &planform,
            &config.structures,
            &config.control_surfaces,
            &config.optimizer.weights,
        )
        .expect("the default wing has a finite transport assessment");
        (assessment, config.optimizer.weights)
    }

    #[test]
    fn the_default_transport_wing_retains_a_finite_box_and_high_lift_footprint() {
        let (assessment, _) = nominal_assessment();

        assert!(assessment.root_wingbox_depth_m > assessment.kink_wingbox_depth_m);
        assert!(assessment.kink_wingbox_width_m > 0.0);
        assert!(assessment.tankable_wingbox_volume_m3 > 0.0);
        assert!(assessment.flap_area_fraction > 0.0);
        assert!(assessment.root_bending_box_slenderness > 0.0);
    }

    fn te_angle_of(preset: &str) -> f64 {
        let registered = alas_config::presets::get(preset).expect("preset");
        exposed_te_angle_deg(&registered.geometry.wing, &registered.design_vector)
            .expect("planform resolves")
    }

    #[test]
    fn the_exposed_edge_angle_is_the_side_of_body_to_kink_geometry_of_the_built_wing() {
        for name in ["A380-800", "ATR72-600", "A320-200", "B787-9"] {
            let registered = alas_config::presets::get(name).expect("preset");
            let plane = AircraftBuilder::new(Some(registered.geometry.clone()))
                .build(Some(&registered.design_vector), false)
                .expect("builds");
            let planform = registered
                .geometry
                .wing
                .transport_planform(&registered.design_vector)
                .expect("planform");
            let inboard_y = if registered.geometry.wing.side_of_body_chord_ratio.is_some() {
                planform.side_of_body.expect("pinned station").y_m
            } else {
                planform.root.y_m
            };
            let at = |y: f64| {
                plane.wings[0]
                    .xsecs
                    .iter()
                    .min_by(|a, b| (a.xyz_le[1] - y).abs().total_cmp(&(b.xyz_le[1] - y).abs()))
                    .expect("sections")
            };
            let (inboard, kink) = (at(inboard_y), at(planform.kink.y_m));
            let built = (kink.xyz_le[1] - inboard.xyz_le[1])
                .atan2((kink.xyz_le[0] + kink.chord) - (inboard.xyz_le[0] + inboard.chord))
                .to_degrees();
            let angle = te_angle_of(name);
            assert!((built - angle).abs() < 1e-6, "{name}: {built}");
            // Whether a preset's nominal meets the limit is a property of its
            // registered geometry, reported here rather than asserted.
            let verdict = if angle <= crate::mdo::TE_ANGLE_LIMIT_DEG {
                "within"
            } else {
                "beyond"
            };
            // The test log is where the verdict is reported.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{name}: exposed trailing edge {angle:.3} deg, {verdict} the limit");
            }
        }
    }
}
