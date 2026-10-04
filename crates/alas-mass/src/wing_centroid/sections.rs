// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Station sampling, spar layout and per-station structural mass for the centroid integration.

use alas_config::materials::{self, MaterialSpec};
use alas_config::StructuresConfig;
use alas_geom::aircraft::wing::Wing;

use super::WingCentroidError;

pub(super) const EFFECTIVE_CAP_DEPTH_FACTOR: f64 = 0.85;

#[derive(Debug, Clone)]
pub(super) struct SparLayout {
    pub(super) fractions: Vec<f64>,
    pub(super) full_span: Vec<bool>,
}

#[derive(Debug, Clone)]
pub(super) struct Station {
    pub(super) xyz_le: [f64; 3],
    pub(super) chord_m: f64,
    pub(super) thickness_to_chord: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct DistributedMass {
    pub(super) mass_kg_m: f64,
    pub(super) x_moment_kg: f64,
    pub(super) y_moment_kg: f64,
    pub(super) z_moment_kg: f64,
    pub(super) caps_kg_m: f64,
    pub(super) webs_kg_m: f64,
    pub(super) skins_kg_m: f64,
}

/// Integrate a preliminary wingbox's structural mass centroid.
///
/// The load is an elliptical ultimate maneuver load over one semispan. The
/// absolute modeled mass is not substituted for the empirical aircraft wing
/// mass; it only supplies physically coupled weights for the first moment.
pub(super) fn material(name: &str) -> Result<&'static MaterialSpec, WingCentroidError> {
    materials::get(name).map_err(|_| WingCentroidError::UnknownMaterial(name.to_owned()))
}

pub(super) fn resolved_spar_layout(
    structures: &StructuresConfig,
) -> Result<SparLayout, WingCentroidError> {
    let (fractions, full_span) = structures.resolved_spars();
    let mut pairs: Vec<(f64, bool)> = fractions
        .into_iter()
        .zip(full_span)
        .filter(|(fraction, _)| fraction.is_finite() && (0.0..=1.0).contains(fraction))
        .collect();
    pairs.sort_by(|left, right| left.0.total_cmp(&right.0));
    if pairs.iter().filter(|(_, full)| *full).count() < 2 {
        return Err(WingCentroidError::InvalidSparLayout);
    }
    Ok(SparLayout {
        fractions: pairs.iter().map(|pair| pair.0).collect(),
        full_span: pairs.iter().map(|pair| pair.1).collect(),
    })
}

pub(super) fn cumulative_semispan(wing: &Wing) -> Vec<f64> {
    let mut cumulative = Vec::with_capacity(wing.xsecs.len());
    cumulative.push(0.0);
    for pair in wing.xsecs.windows(2) {
        let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
        let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
        let next = cumulative.last().copied().unwrap_or(0.0) + dy.hypot(dz);
        cumulative.push(next);
    }
    cumulative
}

pub(super) fn section_thicknesses(wing: &Wing, spar_fractions: &[f64]) -> Vec<Vec<f64>> {
    wing.xsecs
        .iter()
        .map(|section| section.airfoil.local_thickness(spar_fractions))
        .collect()
}

pub(super) fn sample_station(
    wing: &Wing,
    cumulative_span: &[f64],
    thickness_by_xsec: &[Vec<f64>],
    span_m: f64,
) -> Station {
    let mut panel = cumulative_span.len().saturating_sub(2);
    for index in 0..cumulative_span.len().saturating_sub(1) {
        if span_m <= cumulative_span[index + 1] {
            panel = index;
            break;
        }
    }
    let panel_span = (cumulative_span[panel + 1] - cumulative_span[panel]).max(1e-12);
    let blend = ((span_m - cumulative_span[panel]) / panel_span).clamp(0.0, 1.0);
    let root = &wing.xsecs[panel];
    let tip = &wing.xsecs[panel + 1];
    let mut xyz_le = [0.0; 3];
    for (axis, coordinate) in xyz_le.iter_mut().enumerate() {
        *coordinate = root.xyz_le[axis] * (1.0 - blend) + tip.xyz_le[axis] * blend;
    }
    Station {
        xyz_le,
        chord_m: root.chord * (1.0 - blend) + tip.chord * blend,
        thickness_to_chord: thickness_by_xsec[panel]
            .iter()
            .zip(&thickness_by_xsec[panel + 1])
            .map(|(&root_value, &tip_value)| root_value * (1.0 - blend) + tip_value * blend)
            .collect(),
    }
}

pub(super) fn active_spar(
    index: usize,
    span_fraction: f64,
    break_fraction: f64,
    layout: &SparLayout,
) -> bool {
    layout.full_span[index] || span_fraction <= break_fraction + 1e-12
}

pub(super) fn sized_web_thicknesses(
    root: &Station,
    supported_force_n: f64,
    structures: &StructuresConfig,
    web_material: &MaterialSpec,
) -> Vec<f64> {
    // Every configured spar is active at the root. `full_span` controls only
    // whether `active_spar` retains it outboard of the kink, so root shear
    // sizes every web here and the station kernel suppresses partial webs
    // where they no longer exist.
    let heights: Vec<f64> = root
        .thickness_to_chord
        .iter()
        .map(|thickness| thickness * root.chord_m)
        .collect();
    let total_height = heights.iter().sum::<f64>().max(1e-9);
    let shear_allowable = web_material.f_allow_pa / (2.0 * 3.0_f64.sqrt());
    heights
        .iter()
        .map(|&height| {
            let share = height / total_height;
            structures.t_web_min_m.max(
                share * supported_force_n
                    / (shear_allowable * EFFECTIVE_CAP_DEPTH_FACTOR * height.max(1e-6)),
            )
        })
        .collect()
}

// The station kernel receives the resolved geometry, load, and four material
// families separately so no hidden mutable sizing state can alter a centroid.
#[allow(clippy::too_many_arguments)]
pub(super) fn distributed_station_mass(
    station: &Station,
    span_fraction: f64,
    semi_span_m: f64,
    break_fraction: f64,
    supported_force_n: f64,
    layout: &SparLayout,
    web_thicknesses: &[f64],
    structures: &StructuresConfig,
    skin_material: &MaterialSpec,
    web_material: &MaterialSpec,
    cap_material: &MaterialSpec,
) -> DistributedMass {
    let moment_nm = elliptic_cantilever_moment(span_fraction, semi_span_m, supported_force_n);
    let heights: Vec<f64> = station
        .thickness_to_chord
        .iter()
        .map(|thickness| (thickness * station.chord_m).max(0.0))
        .collect();
    let total_active_height = heights
        .iter()
        .enumerate()
        .filter(|(index, _)| active_spar(*index, span_fraction, break_fraction, layout))
        .map(|(_, height)| *height)
        .sum::<f64>()
        .max(1e-9);
    let mut result = DistributedMass::default();
    for (index, &fraction) in layout.fractions.iter().enumerate() {
        if !active_spar(index, span_fraction, break_fraction, layout) {
            continue;
        }
        let x = station.xyz_le[0] + fraction * station.chord_m;
        // Moment is allocated in proportion to spar height. Substituting
        // M_i = M h_i / sum(h) into A_i = M_i / (sigma 0.85 h_i)
        // cancels h_i, so every active spar has this same cap area. The two
        // caps form one force couple; summing A_i sigma 0.85 h_i recovers M,
        // rather than counting the total bending area once per spar.
        let cap_area = moment_nm
            / (cap_material.f_allow_pa * EFFECTIVE_CAP_DEPTH_FACTOR * total_active_height);
        let cap_line_mass = 2.0 * cap_area * cap_material.rho_kg_m3;
        let web_line_mass = web_thicknesses[index] * heights[index] * web_material.rho_kg_m3;
        add_line_mass(
            &mut result,
            cap_line_mass,
            x,
            station.xyz_le[1],
            station.xyz_le[2],
        );
        add_line_mass(
            &mut result,
            web_line_mass,
            x,
            station.xyz_le[1],
            station.xyz_le[2],
        );
        result.caps_kg_m += cap_line_mass;
        result.webs_kg_m += web_line_mass;
    }

    let (front, rear) = full_span_box_limits(layout);
    let skin_line_mass =
        2.0 * (rear - front) * station.chord_m * structures.t_skin_min_m * skin_material.rho_kg_m3;
    let skin_x = station.xyz_le[0] + 0.5 * (front + rear) * station.chord_m;
    add_line_mass(
        &mut result,
        skin_line_mass,
        skin_x,
        station.xyz_le[1],
        station.xyz_le[2],
    );
    result.skins_kg_m += skin_line_mass;
    result
}

pub(super) fn add_line_mass(result: &mut DistributedMass, mass: f64, x: f64, y: f64, z: f64) {
    result.mass_kg_m += mass;
    result.x_moment_kg += mass * x;
    result.y_moment_kg += mass * y;
    result.z_moment_kg += mass * z;
}

pub(super) fn full_span_box_limits(layout: &SparLayout) -> (f64, f64) {
    let mut values = layout
        .fractions
        .iter()
        .zip(&layout.full_span)
        .filter_map(|(&fraction, &full)| full.then_some(fraction));
    let first = values.next().unwrap_or(0.25);
    values.fold((first, first), |(minimum, maximum), value| {
        (minimum.min(value), maximum.max(value))
    })
}

pub(super) fn elliptic_cantilever_moment(
    span_fraction: f64,
    semi_span_m: f64,
    supported_force_n: f64,
) -> f64 {
    let eta = span_fraction.clamp(0.0, 1.0);
    let root = (1.0 - eta * eta).max(0.0).sqrt();
    let integral_load = std::f64::consts::FRAC_PI_4 - 0.5 * (eta * root + eta.asin());
    let integral_arm = (1.0 - eta * eta).max(0.0).powf(1.5) / 3.0;
    (4.0 * supported_force_n * semi_span_m / std::f64::consts::PI)
        * (integral_arm - eta * integral_load).max(0.0)
}
