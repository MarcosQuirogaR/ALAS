// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary transport-planform packaging metrics for the optimizer.
//!
//! A planform needs more than area and span to be a plausible airliner wing:
//! the inboard box carries bending load and fuel, and the trailing edge must
//! retain room for high-lift devices. These metrics are intentionally simple
//! geometric guards, not substitutes for detailed structures, fuel-system, or
//! landing-gear design. The physical rationale and source boundary are in
//! `docs/PHYSICS_SOLVER_FLOW.md`.

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

/// Cost contribution from a transport-planform assessment and trimmed body angle.
///
/// Returns `None` when the configured limits are not internally consistent or
/// an input is non-finite, so the objective can reject the candidate loudly.
pub fn transport_planform_penalty(
    assessment: TransportPlanformAssessment,
    geometric_body_alpha_deg: f64,
    required_fuel_kg: f64,
    fuel_density_kg_m3: f64,
    usable_fuel_fraction: f64,
    weights: &ObjectiveWeights,
) -> Option<f64> {
    let mut penalty = angle_bound_penalty(
        geometric_body_alpha_deg,
        weights.geometric_body_alpha_min_deg,
        weights.geometric_body_alpha_max_deg,
        weights.geometric_body_alpha_penalty_scale,
    )?;

    if weights.transport_shape_priors_enabled {
        for (available, required) in [
            (
                assessment.root_wingbox_depth_m,
                weights.min_root_wingbox_depth_m,
            ),
            (
                assessment.kink_wingbox_depth_m,
                weights.min_break_wingbox_depth_m,
            ),
            (
                assessment.kink_wingbox_width_m,
                weights.min_break_wingbox_width_m,
            ),
        ] {
            penalty +=
                deficit_penalty(available, required, weights.wingbox_packaging_penalty_scale)?;
        }

        penalty += deficit_penalty(
            assessment.flap_area_fraction,
            weights.min_flap_area_fraction,
            weights.flap_area_penalty_scale,
        )?;
        penalty += excess_penalty(
            assessment.root_bending_box_slenderness,
            weights.max_root_bending_box_slenderness,
            weights.bending_slenderness_penalty_scale,
        )?;
        penalty += angle_bound_penalty(
            assessment.inboard_trailing_edge_sweep_deg,
            weights.min_inboard_te_sweep_deg,
            weights.max_inboard_te_sweep_deg,
            weights.te_root_angle_penalty_scale,
        )?;
        penalty += bounded_penalty(
            assessment.break_root_chord_ratio,
            weights.min_break_root_chord_ratio,
            weights.max_break_root_chord_ratio,
            weights.taper_realism_penalty_scale,
        )?;
        penalty += deficit_penalty(
            assessment.tip_root_chord_ratio,
            weights.min_tip_root_chord_ratio,
            weights.taper_realism_penalty_scale,
        )?;
    }

    if !required_fuel_kg.is_finite()
        || !fuel_density_kg_m3.is_finite()
        || !usable_fuel_fraction.is_finite()
        || required_fuel_kg < 0.0
        || fuel_density_kg_m3 <= 0.0
        || !(0.0..=1.0).contains(&usable_fuel_fraction)
        || !weights.fuel_volume_penalty_scale.is_finite()
        || weights.fuel_volume_penalty_scale < 0.0
    {
        return None;
    }
    if required_fuel_kg > 0.0 {
        let tank_capacity_kg =
            assessment.tankable_wingbox_volume_m3 * usable_fuel_fraction * fuel_density_kg_m3;
        penalty += deficit_penalty(
            tank_capacity_kg,
            required_fuel_kg,
            weights.fuel_volume_penalty_scale,
        )?;
    }
    Some(penalty)
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

fn deficit_penalty(available: f64, required: f64, scale: f64) -> Option<f64> {
    if !available.is_finite()
        || !required.is_finite()
        || !scale.is_finite()
        || required <= 0.0
        || scale < 0.0
    {
        return None;
    }
    let deficit = ((required - available) / required).max(0.0);
    Some(deficit.powi(2) * scale)
}

fn excess_penalty(actual: f64, maximum: f64, scale: f64) -> Option<f64> {
    if !actual.is_finite()
        || !maximum.is_finite()
        || !scale.is_finite()
        || maximum <= 0.0
        || scale < 0.0
    {
        return None;
    }
    let excess = ((actual - maximum) / maximum).max(0.0);
    Some(excess.powi(2) * scale)
}

fn bounded_penalty(value: f64, minimum: f64, maximum: f64, scale: f64) -> Option<f64> {
    if !value.is_finite()
        || !minimum.is_finite()
        || !maximum.is_finite()
        || !scale.is_finite()
        || minimum >= maximum
        || scale < 0.0
    {
        return None;
    }
    let exceedance = if value < minimum {
        (minimum - value) / (maximum - minimum)
    } else if value > maximum {
        (value - maximum) / (maximum - minimum)
    } else {
        0.0
    };
    Some(exceedance.powi(2) * scale)
}

fn angle_bound_penalty(value: f64, minimum: f64, maximum: f64, scale: f64) -> Option<f64> {
    if !value.is_finite()
        || !minimum.is_finite()
        || !maximum.is_finite()
        || !scale.is_finite()
        || minimum >= maximum
        || scale < 0.0
    {
        return None;
    }
    let exceedance_deg = if value < minimum {
        minimum - value
    } else if value > maximum {
        value - maximum
    } else {
        0.0
    };
    if exceedance_deg > 0.0 {
        // A stated attitude or angular feasibility boundary is not a scalar
        // preference the aerodynamic reward may buy through. The fixed term
        // gives feasible candidates priority; the quadratic term still tells
        // an all-infeasible population which direction approaches feasibility.
        Some((1.0 + exceedance_deg.powi(2)) * scale)
    } else {
        Some(0.0)
    }
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

    #[test]
    fn the_geometric_body_alpha_window_uses_the_raw_trim_angle() {
        let (assessment, weights) = nominal_assessment();
        let compliant = transport_planform_penalty(assessment, 3.0, 1.0, 800.0, 0.85, &weights)
            .expect("a compliant raw body angle is evaluable");
        let low_attitude = transport_planform_penalty(assessment, 1.0, 1.0, 800.0, 0.85, &weights)
            .expect("an off-window raw body angle is evaluable");

        assert!(low_attitude > compliant);
    }

    #[test]
    fn a_poorly_packaged_wing_accumulates_more_penalty_than_the_nominal_one() {
        let (nominal, mut weights) = nominal_assessment();
        weights.transport_shape_priors_enabled = true;
        let constrained = TransportPlanformAssessment {
            root_wingbox_depth_m: 0.5 * weights.min_root_wingbox_depth_m,
            kink_wingbox_depth_m: 0.5 * weights.min_break_wingbox_depth_m,
            kink_wingbox_width_m: 0.5 * weights.min_break_wingbox_width_m,
            tankable_wingbox_volume_m3: 0.1,
            flap_area_fraction: 0.5 * weights.min_flap_area_fraction,
            root_bending_box_slenderness: 2.0 * weights.max_root_bending_box_slenderness,
            inboard_trailing_edge_sweep_deg: weights.max_inboard_te_sweep_deg + 10.0,
            break_root_chord_ratio: 0.5 * weights.min_break_root_chord_ratio,
            tip_root_chord_ratio: 0.5 * weights.min_tip_root_chord_ratio,
        };
        let nominal_penalty = transport_planform_penalty(nominal, 3.0, 1.0, 800.0, 0.85, &weights)
            .expect("the nominal planform is evaluable");
        let constrained_penalty =
            transport_planform_penalty(constrained, 3.0, 10_000.0, 800.0, 0.85, &weights)
                .expect("the constrained planform is evaluable");

        assert!(constrained_penalty > nominal_penalty);
    }

    #[test]
    fn a_forward_exposed_trailing_edge_receives_a_dominant_penalty() {
        let (mut assessment, mut weights) = nominal_assessment();
        weights.transport_shape_priors_enabled = true;
        assessment.inboard_trailing_edge_sweep_deg = -1.0;

        let penalty = transport_planform_penalty(assessment, 3.0, 1.0, 800.0, 0.85, &weights)
            .expect("the finite assessment is evaluable");

        assert!(penalty >= weights.te_root_angle_penalty_scale);
    }

    #[test]
    fn subjective_shape_priors_do_not_condition_the_default_product_search() {
        let (nominal, weights) = nominal_assessment();
        let unconventional = TransportPlanformAssessment {
            root_wingbox_depth_m: 0.5 * weights.min_root_wingbox_depth_m,
            kink_wingbox_depth_m: 0.5 * weights.min_break_wingbox_depth_m,
            kink_wingbox_width_m: 0.5 * weights.min_break_wingbox_width_m,
            flap_area_fraction: 0.5 * weights.min_flap_area_fraction,
            root_bending_box_slenderness: 2.0 * weights.max_root_bending_box_slenderness,
            inboard_trailing_edge_sweep_deg: weights.max_inboard_te_sweep_deg + 10.0,
            break_root_chord_ratio: 0.5 * weights.min_break_root_chord_ratio,
            tip_root_chord_ratio: 0.5 * weights.min_tip_root_chord_ratio,
            ..nominal
        };

        let nominal_penalty = transport_planform_penalty(nominal, 3.0, 0.0, 800.0, 0.85, &weights)
            .expect("the nominal planform is evaluable");
        let unconventional_penalty =
            transport_planform_penalty(unconventional, 3.0, 0.0, 800.0, 0.85, &weights)
                .expect("the unconventional planform is evaluable");

        assert_eq!(nominal_penalty, unconventional_penalty);
    }
}
