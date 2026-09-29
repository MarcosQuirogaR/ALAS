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
//! equations 8.3 to 8.5:
//! <https://flowlab.groups.et.byu.net/me415/flight.pdf>. The stationwise
//! skin/web treatment is consistent with the preliminary wingbox procedure in
//! NASA TP-1158, pp. 8 to 12:
//! <https://ntrs.nasa.gov/api/citations/19780017136/downloads/19780017136.pdf>.
//!
//! This is a centroid model, not a replacement for the Torenbeek total wing
//! mass correlation. Its component masses only normalize the integrated first
//! moment; [`crate::breakdown::MassBreakdown::wing`] remains the mass carried
//! into aircraft weight and balance.

use std::fmt;

use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::aircraft::wing::Wing;

mod ribs;
mod sections;
#[cfg(test)]
mod tests;

const MIN_STATIONS: usize = 41;
const MAX_STATIONS: usize = 1001;

use ribs::{linspace, rib_first_moment, trapezoid_property};
use sections::{
    cumulative_semispan, distributed_station_mass, material, resolved_spar_layout, sample_station,
    section_thicknesses, sized_web_thicknesses, DistributedMass, Station,
};

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
