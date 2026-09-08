// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Geometry seams for the clean-sheet wing inventory: the FLOPS and
// non-box-planform inputs read off the built wing, and the configured
// control-surface areas and centroids the inventory items are placed at.

/// FLOPS Eqs. 33-38 inputs read off the built wing.
///
/// `movable_surface_area_m2` is left at zero: [`build_wing_inventory`]
/// overwrites it with the enumerated movable areas so the two cannot disagree.
/// `thickness_to_chord` is the chord-weighted mean section thickness, which is
/// the `TCA` the equations ask for.
fn flops_wing_inputs(config: &AlasConfig, wing: &Wing) -> FlopsWingInputs {
    FlopsWingInputs {
        design_gross_mass_kg: config.requirements.mtow_kg,
        wing_area_m2: wing.reference_area(),
        wing_span_m: wing.reference_span(),
        taper_ratio: wing.taper_ratio(),
        quarter_chord_sweep_deg: wing.mean_sweep_angle(0.25),
        thickness_to_chord: chord_weighted_thickness_to_chord(wing),
        movable_surface_area_m2: 0.0,
        ultimate_load_factor: config.requirements.ultimate_load_factor,
        composite_utilization: 0.0,
        aeroelastic_tailoring: 0.0,
        strut_bracing: 0.0,
        wing_load_fraction: 1.0,
        fuselage_count: 1,
        variable_sweep_penalty: 0.0,
        wing_mounted_engine_count: config.geometry.engine.spanwise_positions_m.len(),
        bending: WingBendingFactor::Simplified,
    }
}

/// The chord-weighted mean of the section maximum thickness ratios.
fn chord_weighted_thickness_to_chord(wing: &Wing) -> f64 {
    let sample: Vec<f64> = (0..101).map(|index| index as f64 / 100.0).collect();
    let (moment, weight) = wing
        .xsecs
        .iter()
        .fold((0.0, 0.0), |(moment, weight), xsec| {
            let thickness = xsec.airfoil.max_thickness(&sample);
            (moment + thickness * xsec.chord, weight + xsec.chord)
        });
    if weight > 0.0 && moment.is_finite() {
        moment / weight
    } else {
        0.0
    }
}

/// The wing planform outside the structural box.
///
/// Only full-span spars bound the box over the whole semispan, so a partial
/// centre spar does not move the boundary. The chordwise fraction is the strip
/// forward of the front spar plus the strip aft of the rear spar; the centroid
/// is the area-weighted mid-chord point of those two strips.
fn fixed_non_box_structure(config: &AlasConfig, wing: &Wing) -> FixedNonBoxStructure {
    let (fractions, full_span) = config.structures.resolved_spars();
    let bounding: Vec<f64> = fractions
        .iter()
        .zip(full_span.iter())
        .filter_map(|(fraction, spans)| {
            (*spans && fraction.is_finite() && (0.0..=1.0).contains(fraction)).then_some(*fraction)
        })
        .collect();
    let front = bounding.iter().copied().fold(f64::INFINITY, f64::min);
    let rear = bounding.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if bounding.len() < 2
        || front.partial_cmp(&rear) != Some(std::cmp::Ordering::Less)
    {
        return FixedNonBoxStructure {
            chord_fraction_outside_box: 0.0,
            centroid_m: surface_centroid(wing, 0.0, 1.0, 0.5),
        };
    }
    let forward_share = front;
    let aft_share = 1.0 - rear;
    let outside = forward_share + aft_share;
    let centroid = weighted_centroid(
        forward_share,
        surface_centroid(wing, 0.0, 1.0, 0.5 * front),
        aft_share,
        surface_centroid(wing, 0.0, 1.0, 0.5 * (1.0 + rear)),
        outside,
    )
    .unwrap_or_else(|| surface_centroid(wing, 0.0, 1.0, 0.5));
    FixedNonBoxStructure {
        chord_fraction_outside_box: outside,
        centroid_m: centroid,
    }
}

fn validate_secondary_breakdown(
    breakdown: WingSecondaryMassBreakdown,
) -> Result<(), CandidateFailure> {
    let values = [
        breakdown.high_lift_devices_kg,
        breakdown.spoilers_and_speedbrakes_kg,
        breakdown.total_kg,
    ];
    if values
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
        && breakdown.total_kg > 0.0
        && (breakdown.total_kg
            - breakdown.high_lift_devices_kg
            - breakdown.spoilers_and_speedbrakes_kg)
            .abs()
            < 1.0e-9
    {
        Ok(())
    } else {
        Err(structural_failure())
    }
}

fn configured_surface_area(wing: &Wing, start: f64, end: f64, chord_fraction: f64) -> f64 {
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(0.0, 1.0);
    let chord_fraction = chord_fraction.clamp(0.0, 1.0);
    if end <= start || chord_fraction <= 0.0 {
        return 0.0;
    }
    let span = wing_span(wing);
    if !span.is_finite() || span <= 0.0 {
        return 0.0;
    }
    let samples = 64usize;
    let width = (end - start) / samples as f64;
    let area: f64 = (0..samples)
        .map(|index| {
            let fraction = start + (index as f64 + 0.5) * width;
            wing_position(wing, fraction).1 * chord_fraction * width * span
        })
        .sum();
    area * if wing.symmetric { 2.0 } else { 1.0 }
}

fn surface_centroid(wing: &Wing, start: f64, end: f64, chordwise_fraction: f64) -> [f64; 3] {
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(0.0, 1.0);
    if end <= start {
        return wing_position(wing, 0.5 * (start + end)).0;
    }
    let span = wing_span(wing);
    let samples = 64usize;
    let width = (end - start) / samples as f64;
    let mut moment = [0.0; 3];
    let mut area = 0.0;
    for index in 0..samples {
        let fraction = start + (index as f64 + 0.5) * width;
        let (leading_edge, chord) = wing_position(wing, fraction);
        let mass_area = chord * width * span;
        let point = [
            leading_edge[0] + chordwise_fraction * chord,
            leading_edge[1],
            leading_edge[2],
        ];
        for axis in 0..3 {
            moment[axis] += mass_area * point[axis];
        }
        area += mass_area;
    }
    if wing.symmetric {
        moment[1] = 0.0;
    }
    if area > 0.0 && area.is_finite() {
        moment.map(|value| value / area)
    } else {
        [0.0; 3]
    }
}

fn weighted_centroid(
    first_mass: f64,
    first: [f64; 3],
    second_mass: f64,
    second: [f64; 3],
    total: f64,
) -> Option<[f64; 3]> {
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let result =
        std::array::from_fn(|axis| (first_mass * first[axis] + second_mass * second[axis]) / total);
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}

fn wing_span(wing: &Wing) -> f64 {
    wing.xsecs
        .windows(2)
        .map(|pair| {
            let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
            let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
            dy.hypot(dz)
        })
        .sum()
}

fn wing_position(wing: &Wing, fraction: f64) -> ([f64; 3], f64) {
    let fraction = fraction.clamp(0.0, 1.0);
    let mut lengths = Vec::with_capacity(wing.xsecs.len().saturating_sub(1));
    let mut total = 0.0;
    for pair in wing.xsecs.windows(2) {
        let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
        let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
        let length = dy.hypot(dz);
        lengths.push(length);
        total += length;
    }
    if total <= 0.0 || !total.is_finite() || wing.xsecs.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let target = fraction * total;
    let mut travelled = 0.0;
    for (index, length) in lengths.iter().copied().enumerate() {
        if target <= travelled + length || index + 2 == wing.xsecs.len() {
            let t = if length > 0.0 {
                ((target - travelled) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let root = &wing.xsecs[index];
            let tip = &wing.xsecs[index + 1];
            let position =
                std::array::from_fn(|axis| root.xyz_le[axis] * (1.0 - t) + tip.xyz_le[axis] * t);
            let chord = root.chord * (1.0 - t) + tip.chord * t;
            return (position, chord);
        }
        travelled += length;
    }
    match wing.xsecs.last() {
        Some(last) => (last.xyz_le, last.chord),
        None => ([0.0; 3], 0.0),
    }
}
