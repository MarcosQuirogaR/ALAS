// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rib mass and the trapezoidal integration helpers of the wing centroid.

use alas_config::materials::MaterialSpec;
use alas_config::StructuresConfig;
use alas_geom::aircraft::wing::Wing;

use super::sections::{
    elliptic_cantilever_moment, full_span_box_limits, sample_station, section_thicknesses,
    DistributedMass, SparLayout,
};

// families together; bundling them would only hide this physical dependency.
#[allow(clippy::too_many_arguments)]
pub(super) fn rib_first_moment(
    wing: &Wing,
    cumulative_span: &[f64],
    semi_span: f64,
    structures: &StructuresConfig,
    layout: &SparLayout,
    supported_force_n: f64,
    skin_material: &MaterialSpec,
    rib_material: &MaterialSpec,
) -> (f64, [f64; 3]) {
    let (front, rear) = full_span_box_limits(layout);
    let root_thickness = wing.xsecs[0]
        .airfoil
        .local_thickness(&[0.5 * (front + rear)])[0]
        * wing.xsecs[0].chord;
    let root_moment = elliptic_cantilever_moment(0.0, semi_span, supported_force_n);
    let box_width = (rear - front) * wing.xsecs[0].chord;
    let membrane_force = root_moment / root_thickness.max(1e-6) / box_width.max(1e-6);
    let panel_stress = (membrane_force / structures.t_skin_min_m).max(1e6);
    let spacing = (structures.rib_radius_of_gyration_m
        * (structures.rib_buckling_coeff * std::f64::consts::PI.powi(2) * skin_material.e_pa
            / panel_stress)
            .sqrt())
    .max(0.5);
    let rib_count = structures
        .num_ribs_override
        .unwrap_or_else(|| ((semi_span / spacing).ceil() as i64 + 1).max(10))
        .max(2) as usize;
    let rib_spans = linspace(0.0, semi_span, rib_count);
    let chord_fractions = linspace(front, rear, 61);
    let thickness_by_xsec = section_thicknesses(wing, &chord_fractions);
    let mut total_mass = 0.0;
    let mut moment = [0.0; 3];
    for span in rib_spans {
        let station = sample_station(wing, cumulative_span, &thickness_by_xsec, span);
        let mut area = 0.0;
        let mut fraction_moment = 0.0;
        for index in 0..chord_fractions.len() - 1 {
            let dx_fraction = chord_fractions[index + 1] - chord_fractions[index];
            let thickness =
                0.5 * (station.thickness_to_chord[index] + station.thickness_to_chord[index + 1]);
            let area_slice = thickness * dx_fraction * station.chord_m.powi(2);
            area += area_slice;
            fraction_moment +=
                area_slice * 0.5 * (chord_fractions[index] + chord_fractions[index + 1]);
        }
        let rib_mass = area * structures.t_rib_m * rib_material.rho_kg_m3;
        let fraction_centroid = if area > 0.0 {
            fraction_moment / area
        } else {
            0.5 * (front + rear)
        };
        let xyz = [
            station.xyz_le[0] + fraction_centroid * station.chord_m,
            station.xyz_le[1],
            station.xyz_le[2],
        ];
        total_mass += rib_mass;
        for axis in 0..3 {
            moment[axis] += rib_mass * xyz[axis];
        }
    }
    (total_mass, moment)
}

pub(super) fn trapezoid_property(
    stations: &[f64],
    values: &[DistributedMass],
    property: impl Fn(&DistributedMass) -> f64,
) -> f64 {
    stations
        .windows(2)
        .zip(values.windows(2))
        .map(|(span, pair)| 0.5 * (span[1] - span[0]) * (property(&pair[0]) + property(&pair[1])))
        .sum()
}

pub(super) fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count <= 1 {
        return vec![start];
    }
    (0..count)
        .map(|index| start + (stop - start) * index as f64 / (count - 1) as f64)
        .collect()
}
