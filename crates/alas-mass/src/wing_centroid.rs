// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary main-wing structural mass centroid.
//!
//! A wing mass is not concentrated at its aerodynamic centre. The primary
//! structure is distributed through the wingbox, and bending material is
//! strongly biased inboard. This module integrates the first moment of four
//! explicit structural families: bending caps, spar webs, wingbox skins, and
//! ribs. Cap area follows the classical fully-stressed beam relation
//! `A = M / (sigma h)`; see A. Ning, *Flight Vehicle Design*, section 8.2,
//! equations 8.3--8.5:
//! <https://flowlab.groups.et.byu.net/me415/flight.pdf>. The stationwise
//! skin/web treatment is consistent with the preliminary wingbox procedure in
//! NASA TP-1158, pp. 8--12:
//! <https://ntrs.nasa.gov/api/citations/19780017136/downloads/19780017136.pdf>.
//!
//! This is a centroid model, not a replacement for the Torenbeek total wing
//! mass correlation. Its component masses only normalize the integrated first
//! moment; [`crate::breakdown::MassBreakdown::wing`] remains the mass carried
//! into aircraft weight and balance.

use std::fmt;

use alas_config::materials::{self, MaterialSpec};
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::aircraft::wing::Wing;

const MIN_STATIONS: usize = 41;
const MAX_STATIONS: usize = 1001;
const EFFECTIVE_CAP_DEPTH_FACTOR: f64 = 0.85;

/// Integrated wingbox centroid and the component masses used to normalize it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingStructuralCentroid {
    /// Structural mass centroid in aircraft geometry axes, metres.
    pub xyz_m: [f64; 3],
    /// Semi-wing cap mass represented by the integration, kilograms.
    pub cap_mass_kg: f64,
    /// Semi-wing spar-web mass represented by the integration, kilograms.
    pub web_mass_kg: f64,
    /// Semi-wing wingbox-skin mass represented by the integration, kilograms.
    pub skin_mass_kg: f64,
    /// Semi-wing rib mass represented by the integration, kilograms.
    pub rib_mass_kg: f64,
}

impl WingStructuralCentroid {
    /// Total modeled semi-wing mass used only to normalize the centroid.
    pub fn modeled_semiwing_mass_kg(self) -> f64 {
        self.cap_mass_kg + self.web_mass_kg + self.skin_mass_kg + self.rib_mass_kg
    }
}

/// Why a structural centroid could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WingCentroidError {
    /// A lofted wing needs at least a root and a tip section.
    InsufficientSections,
    /// The cross-sections do not define a positive physical semispan.
    DegenerateSpan,
    /// Fewer than two valid full-span spars remain after validation.
    InvalidSparLayout,
    /// A configured structural material is not registered.
    UnknownMaterial(String),
    /// The integrated structural mass is zero or non-finite.
    DegenerateMass,
}

impl fmt::Display for WingCentroidError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientSections => write!(formatter, "wing needs at least two sections"),
            Self::DegenerateSpan => write!(formatter, "wing semispan must be positive"),
            Self::InvalidSparLayout => {
                write!(
                    formatter,
                    "wingbox needs at least two valid full-span spars"
                )
            }
            Self::UnknownMaterial(name) => write!(formatter, "unknown structural material {name}"),
            Self::DegenerateMass => write!(formatter, "integrated wingbox mass is not positive"),
        }
    }
}

impl std::error::Error for WingCentroidError {}

#[derive(Debug, Clone)]
struct SparLayout {
    fractions: Vec<f64>,
    full_span: Vec<bool>,
}

#[derive(Debug, Clone)]
struct Station {
    xyz_le: [f64; 3],
    chord_m: f64,
    thickness_to_chord: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DistributedMass {
    mass_kg_m: f64,
    x_moment_kg: f64,
    y_moment_kg: f64,
    z_moment_kg: f64,
    caps_kg_m: f64,
    webs_kg_m: f64,
    skins_kg_m: f64,
}

/// Integrate a preliminary wingbox's structural mass centroid.
///
/// The load is an elliptical ultimate maneuver load over one semispan. The
/// absolute modeled mass is not substituted for the empirical aircraft wing
/// mass; it only supplies physically coupled weights for the first moment.
pub fn wing_structural_centroid(
    wing: &Wing,
    requirements: &DesignRequirements,
    structures: &StructuresConfig,
) -> Result<WingStructuralCentroid, WingCentroidError> {
    if wing.xsecs.len() < 2 {
        return Err(WingCentroidError::InsufficientSections);
    }
    let cumulative_span = cumulative_semispan(wing);
    let semi_span = *cumulative_span
        .last()
        .ok_or(WingCentroidError::DegenerateSpan)?;
    if !semi_span.is_finite() || semi_span <= 0.0 {
        return Err(WingCentroidError::DegenerateSpan);
    }

    let layout = resolved_spar_layout(structures)?;
    let skin_material = material(&structures.skin_material)?;
    let web_material = material(&structures.spar_web_material)?;
    let cap_material = material(&structures.spar_cap_material)?;
    let rib_material = material(&structures.rib_material)?;
    let station_count =
        (structures.spanwise_stations.max(0) as usize).clamp(MIN_STATIONS, MAX_STATIONS);
    let span_stations = linspace(0.0, semi_span, station_count);
    let thickness_by_xsec = section_thicknesses(wing, &layout.fractions);
    let stations: Vec<Station> = span_stations
        .iter()
        .map(|&span| sample_station(wing, &cumulative_span, &thickness_by_xsec, span))
        .collect();
    let break_fraction = if cumulative_span.len() > 2 {
        cumulative_span[1] / semi_span
    } else {
        1.0
    };

    let supported_force_n = requirements.mtow_kg
        * requirements.gravity_m_s2
        * requirements.ultimate_load_factor
        * structures.additional_safety_factor
        / if wing.symmetric { 2.0 } else { 1.0 };
    let web_thicknesses =
        sized_web_thicknesses(&stations[0], supported_force_n, structures, web_material);

    let distributed: Vec<DistributedMass> = span_stations
        .iter()
        .zip(&stations)
        .map(|(&span, station)| {
            distributed_station_mass(
                station,
                span / semi_span,
                semi_span,
                break_fraction,
                supported_force_n,
                &layout,
                &web_thicknesses,
                structures,
                skin_material,
                web_material,
                cap_material,
            )
        })
        .collect();

    let mut mass = trapezoid_property(&span_stations, &distributed, |value| value.mass_kg_m);
    let mut first_moment = [
        trapezoid_property(&span_stations, &distributed, |value| value.x_moment_kg),
        trapezoid_property(&span_stations, &distributed, |value| value.y_moment_kg),
        trapezoid_property(&span_stations, &distributed, |value| value.z_moment_kg),
    ];
    let cap_mass = trapezoid_property(&span_stations, &distributed, |value| value.caps_kg_m);
    let web_mass = trapezoid_property(&span_stations, &distributed, |value| value.webs_kg_m);
    let skin_mass = trapezoid_property(&span_stations, &distributed, |value| value.skins_kg_m);

    let (rib_mass, rib_moment) = rib_first_moment(
        wing,
        &cumulative_span,
        semi_span,
        structures,
        &layout,
        supported_force_n,
        skin_material,
        rib_material,
    );
    mass += rib_mass;
    for axis in 0..3 {
        first_moment[axis] += rib_moment[axis];
    }
    if !mass.is_finite() || mass <= 0.0 {
        return Err(WingCentroidError::DegenerateMass);
    }
    let mut xyz = [
        first_moment[0] / mass,
        first_moment[1] / mass,
        first_moment[2] / mass,
    ];
    if wing.symmetric {
        xyz[1] = 0.0;
    }

    Ok(WingStructuralCentroid {
        xyz_m: xyz,
        cap_mass_kg: cap_mass,
        web_mass_kg: web_mass,
        skin_mass_kg: skin_mass,
        rib_mass_kg: rib_mass,
    })
}

fn material(name: &str) -> Result<&'static MaterialSpec, WingCentroidError> {
    materials::get(name).map_err(|_| WingCentroidError::UnknownMaterial(name.to_owned()))
}

fn resolved_spar_layout(structures: &StructuresConfig) -> Result<SparLayout, WingCentroidError> {
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

fn cumulative_semispan(wing: &Wing) -> Vec<f64> {
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

fn section_thicknesses(wing: &Wing, spar_fractions: &[f64]) -> Vec<Vec<f64>> {
    wing.xsecs
        .iter()
        .map(|section| section.airfoil.local_thickness(spar_fractions))
        .collect()
}

fn sample_station(
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

fn active_spar(index: usize, span_fraction: f64, break_fraction: f64, layout: &SparLayout) -> bool {
    layout.full_span[index] || span_fraction <= break_fraction + 1e-12
}

fn sized_web_thicknesses(
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
fn distributed_station_mass(
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

fn add_line_mass(result: &mut DistributedMass, mass: f64, x: f64, y: f64, z: f64) {
    result.mass_kg_m += mass;
    result.x_moment_kg += mass * x;
    result.y_moment_kg += mass * y;
    result.z_moment_kg += mass * z;
}

fn full_span_box_limits(layout: &SparLayout) -> (f64, f64) {
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

fn elliptic_cantilever_moment(span_fraction: f64, semi_span_m: f64, supported_force_n: f64) -> f64 {
    let eta = span_fraction.clamp(0.0, 1.0);
    let root = (1.0 - eta * eta).max(0.0).sqrt();
    let integral_load = std::f64::consts::FRAC_PI_4 - 0.5 * (eta * root + eta.asin());
    let integral_arm = (1.0 - eta * eta).max(0.0).powf(1.5) / 3.0;
    (4.0 * supported_force_n * semi_span_m / std::f64::consts::PI)
        * (integral_arm - eta * integral_load).max(0.0)
}

// Ribs need the loft, span mapping, box layout, load, and two material
// families together; bundling them would only hide this physical dependency.
#[allow(clippy::too_many_arguments)]
fn rib_first_moment(
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

fn trapezoid_property(
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

fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count <= 1 {
        return vec![start];
    }
    (0..count)
        .map(|index| start + (stop - start) * index as f64 / (count - 1) as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

    fn rectangular_wing() -> Wing {
        let airfoil = Airfoil::from_name("naca0012").expect("NACA 0012 is registered");
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([2.0, 0.0, 0.0], 4.0, 0.0, airfoil.clone()),
                WingXSec::new([2.0, 10.0, 0.0], 4.0, 0.0, airfoil),
            ],
            true,
        )
    }

    #[test]
    fn spar_cap_allocation_recovers_the_applied_bending_moment_once() {
        let layout = SparLayout {
            fractions: vec![0.25, 0.70],
            full_span: vec![true, true],
        };
        let station = Station {
            xyz_le: [0.0, 0.0, 0.0],
            chord_m: 10.0,
            thickness_to_chord: vec![0.10, 0.08],
        };
        let structures = StructuresConfig::default();
        let cap_material = materials::get(&structures.spar_cap_material)
            .expect("the default cap material is registered");
        let skin_material =
            materials::get(&structures.skin_material).expect("the default skin is registered");
        let web_material = materials::get(&structures.spar_web_material)
            .expect("the default web material is registered");
        let supported_force_n = 1.0e6;
        let semi_span_m = 10.0;
        let web_thicknesses = vec![structures.t_web_min_m; 2];
        let distributed = distributed_station_mass(
            &station,
            0.0,
            semi_span_m,
            1.0,
            supported_force_n,
            &layout,
            &web_thicknesses,
            &structures,
            skin_material,
            web_material,
            cap_material,
        );

        let cap_area_each_m2 =
            distributed.caps_kg_m / (2.0 * layout.fractions.len() as f64 * cap_material.rho_kg_m3);
        let effective_height_sum_m = station.thickness_to_chord.iter().sum::<f64>()
            * station.chord_m
            * EFFECTIVE_CAP_DEPTH_FACTOR;
        let recovered_moment_nm =
            cap_area_each_m2 * cap_material.f_allow_pa * effective_height_sum_m;
        let applied_moment_nm = elliptic_cantilever_moment(0.0, semi_span_m, supported_force_n);

        assert!((recovered_moment_nm - applied_moment_nm).abs() / applied_moment_nm < 1e-12);
    }

    #[test]
    fn a_partial_span_spar_is_inactive_outboard_of_the_break() {
        let layout = SparLayout {
            fractions: vec![0.25, 0.50, 0.70],
            full_span: vec![true, false, true],
        };

        assert!(active_spar(1, 0.40, 0.40, &layout));
        assert!(!active_spar(1, 0.41, 0.40, &layout));
        assert!(active_spar(0, 1.0, 0.40, &layout));
        assert!(active_spar(2, 1.0, 0.40, &layout));
    }

    #[test]
    fn a_symmetric_wing_centroid_is_finite_and_on_the_centerline() {
        let centroid = wing_structural_centroid(
            &rectangular_wing(),
            &DesignRequirements::default(),
            &StructuresConfig::default(),
        )
        .expect("the default two-spar wingbox is valid");

        assert!(centroid.xyz_m.into_iter().all(f64::is_finite));
        assert_eq!(centroid.xyz_m[1], 0.0);
        assert!((3.0..=4.8).contains(&centroid.xyz_m[0]));
        assert!(centroid.modeled_semiwing_mass_kg() > 0.0);
    }

    #[test]
    fn an_invalid_spar_layout_is_reported_instead_of_using_a_fallback_point() {
        let structures = StructuresConfig {
            spar_chord_fractions: vec![0.25],
            ..Default::default()
        };
        let error = wing_structural_centroid(
            &rectangular_wing(),
            &DesignRequirements::default(),
            &structures,
        )
        .expect_err("one full-span spar cannot define a wingbox");

        assert_eq!(error, WingCentroidError::InvalidSparLayout);
    }
}
