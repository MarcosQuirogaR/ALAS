// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit mesh resolution presets, refinement and inflation controls.
//!
//! Every number here is the one the GEO renderer uses: the sizes come from
//! the same functions, so the manifest cannot drift from the generated
//! source.  Lengths are metres for the configured chord; the catalogue
//! reports chord fractions.  Nothing here is mesh-quality evidence: the
//! achieved sizes, layer heights and y+ must be measured on the generated
//! mesh and the solved fields.

use serde::{Deserialize, Serialize};

use super::generation::{characteristic_lengths, leading_edge_size};
use super::{boundary_layer_sizing, MeshError};
use crate::{CfdStudyConfig, MeshPreset, MeshSettings};

/// Version of the preset catalogue recorded in every manifest.
pub const PRESET_CATALOGUE_VERSION: &str = "alas-airfoil-mesh-presets-v1";
/// The estimated y+ is consistent with the target when their ratio lies
/// within `[1/band, band]`.
pub const Y_PLUS_CONSISTENCY_BAND: f64 = 2.0;
/// Total inflation thickness above this chord fraction is flagged.
pub const MAX_RECOMMENDED_INFLATION_CHORDS: f64 = 0.10;

/// One row of the preset catalogue (chord fractions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetSummary {
    /// Preset identifier.
    pub preset: MeshPreset,
    /// Short label.
    pub label: String,
    /// Far-field size as a chord fraction.
    pub far_field_fraction_of_chord: f64,
    /// Surface size as a chord fraction.
    pub surface_fraction_of_chord: f64,
    /// Wake size at the default wake refinement level, chord fraction.
    pub wake_fraction_of_chord: f64,
    /// Refinement level retained for exports.
    pub refinement_level: u32,
    /// Background block cells `(x, y, z)`.
    pub base_cells: (u32, u32, u32),
    /// Intended use.
    pub intent: String,
}

/// Explicit refinement sizes for one configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefinementSpec {
    /// Preset.
    pub preset: MeshPreset,
    /// Isotropic far-field size, metres.
    pub far_field_size_m: f64,
    /// Wake box size, metres.
    pub wake_size_m: f64,
    /// Surface (airfoil point) size, metres.
    pub surface_size_m: f64,
    /// Leading-edge size, metres (equal to the surface size at level zero).
    pub leading_edge_size_m: f64,
    /// Wake refinement level (each level halves the far-field size).
    pub wake_refinement_level: u32,
    /// Leading-edge refinement level (each level halves the surface size).
    pub leading_edge_refinement_level: u32,
    /// Estimated straight surface segments at the surface size, when the
    /// section perimeter is known.
    pub estimated_surface_segments: Option<u64>,
    /// Gmsh 2-D algorithm identifier written to the GEO (5 = Delaunay).
    pub gmsh_algorithm_2d: u32,
    /// Element order written to the GEO.
    pub element_order: u32,
}

/// Agreement between the configured first layer and the y+ target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YPlusConsistency {
    /// Estimated y+ within the band of the target.
    Consistent,
    /// Estimated y+ above the band: the first layer is too thick.
    TooCoarse,
    /// Estimated y+ below the band: the first layer is thinner than needed.
    TooFine,
    /// Inflation disabled: the target is not enforced by the mesh.
    NotEnforced,
}

/// Inflation (boundary-layer) controls in SI units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InflationSpec {
    /// Whether a boundary-layer field is emitted.
    pub enabled: bool,
    /// Number of layers.
    pub n_layers: u32,
    /// Geometric expansion ratio.
    pub expansion_ratio: f64,
    /// Configured first cell-centre wall distance, metres.
    pub wall_distance_m: f64,
    /// First-layer thickness sent to the mesher (twice the wall distance), metres.
    pub first_layer_thickness_m: f64,
    /// Total inflation thickness, metres.
    pub total_thickness_m: f64,
    /// Total inflation thickness as a chord fraction.
    pub total_thickness_chords: f64,
    /// Target y+.
    pub target_y_plus: f64,
    /// Estimated y+ at the configured wall distance (flat-plate estimate).
    pub estimated_y_plus: f64,
    /// Wall distance that would meet the target under the same estimate, metres.
    pub wall_distance_for_target_m: f64,
    /// `estimated_y_plus / target_y_plus`.
    pub y_plus_ratio: f64,
    /// Consistency verdict.
    pub consistency: YPlusConsistency,
    /// Friction velocity of the estimate, m/s.
    pub friction_velocity_m_s: f64,
    /// Kinematic viscosity, m^2/s.
    pub kinematic_viscosity_m2_s: f64,
    /// Skin-friction coefficient of the estimate.
    pub skin_friction_estimate: f64,
    /// Interpretation statement.
    pub interpretation: String,
}

/// Refinement plus inflation for one configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshResolutionSpec {
    /// Catalogue version.
    pub catalogue_version: String,
    /// Refinement sizes.
    pub refinement: RefinementSpec,
    /// Inflation controls.
    pub inflation: InflationSpec,
    /// Advisory findings (never blocking).
    pub warnings: Vec<String>,
}

/// The three presets with their chord-fraction sizes at the default wake
/// refinement level.
pub fn preset_catalogue() -> Vec<PresetSummary> {
    [
        (
            MeshPreset::Coarse,
            "coarse",
            "fast preflight and topology checks; not for coefficients",
        ),
        (
            MeshPreset::Medium,
            "medium",
            "default engineering setup for attached flow",
        ),
        (
            MeshPreset::Fine,
            "fine",
            "sensitivity check against the medium preset",
        ),
    ]
    .into_iter()
    .map(|(preset, label, intent)| {
        let config = CfdStudyConfig {
            chord_m: 1.0,
            mesh: MeshSettings {
                preset,
                ..MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let (far, wake, surface) = characteristic_lengths(&config, 1.0);
        PresetSummary {
            preset,
            label: label.to_owned(),
            far_field_fraction_of_chord: far,
            surface_fraction_of_chord: surface,
            wake_fraction_of_chord: wake,
            refinement_level: preset.refinement_level(),
            base_cells: preset.base_cells(),
            intent: intent.to_owned(),
        }
    })
    .collect()
}

/// Explicit resolution for one configuration.
///
/// `perimeter_chord` (from the geometry audit) sizes the surface-segment
/// estimate; `None` leaves it unreported.  Invalid controls are errors with
/// the same messages the GEO builder would raise.
pub fn mesh_resolution(
    config: &CfdStudyConfig,
    perimeter_chord: Option<f64>,
) -> Result<MeshResolutionSpec, MeshError> {
    super::generation::validate_mesh_geometry_controls(config)?;
    let sizing = boundary_layer_sizing(config)?;
    let chord_m = config.chord_m;
    let (far_field_size_m, wake_size_m, surface_size_m) = characteristic_lengths(config, chord_m);
    let leading_edge_size_m = leading_edge_size(config, surface_size_m);
    let estimated_surface_segments = perimeter_chord
        .filter(|perimeter| perimeter.is_finite() && *perimeter > 0.0)
        .map(|perimeter| (perimeter * chord_m / surface_size_m).round() as u64);
    let refinement = RefinementSpec {
        preset: config.mesh.preset,
        far_field_size_m,
        wake_size_m,
        surface_size_m,
        leading_edge_size_m,
        wake_refinement_level: config.mesh.wake_refinement,
        leading_edge_refinement_level: config.mesh.leading_edge_refinement,
        estimated_surface_segments,
        gmsh_algorithm_2d: 5,
        element_order: 1,
    };
    let y_plus_ratio = sizing.estimated_y_plus / sizing.target_y_plus;
    let consistency = if !sizing.enabled {
        YPlusConsistency::NotEnforced
    } else if y_plus_ratio > Y_PLUS_CONSISTENCY_BAND {
        YPlusConsistency::TooCoarse
    } else if y_plus_ratio < 1.0 / Y_PLUS_CONSISTENCY_BAND {
        YPlusConsistency::TooFine
    } else {
        YPlusConsistency::Consistent
    };
    let total_thickness_chords = sizing.total_thickness_m / chord_m;
    let mut warnings = Vec::new();
    match consistency {
        YPlusConsistency::Consistent => {}
        YPlusConsistency::NotEnforced => warnings.push(
            "boundary layers are disabled; the y+ target is recorded but not enforced by the mesh"
                .to_owned(),
        ),
        YPlusConsistency::TooCoarse | YPlusConsistency::TooFine => warnings.push(format!(
            "first-layer wall distance {:.3e} m gives an estimated y+ of {:.2} against a target of {:.2}; {:.3e} m would meet the target under the same flat-plate estimate",
            sizing.selected_wall_distance_m,
            sizing.estimated_y_plus,
            sizing.target_y_plus,
            sizing.derived_wall_distance_m
        )),
    }
    if total_thickness_chords > MAX_RECOMMENDED_INFLATION_CHORDS {
        warnings.push(format!(
            "total inflation thickness {total_thickness_chords:.4} c exceeds {MAX_RECOMMENDED_INFLATION_CHORDS} c; reduce the layer count or the first-layer height"
        ));
    }
    if sizing.enabled && sizing.first_layer_thickness_m > surface_size_m {
        warnings.push(format!(
            "first-layer thickness {:.3e} m exceeds the surface size {:.3e} m; the inflation field is coarser than the surface spacing",
            sizing.first_layer_thickness_m, surface_size_m
        ));
    }
    Ok(MeshResolutionSpec {
        catalogue_version: PRESET_CATALOGUE_VERSION.to_owned(),
        refinement,
        inflation: InflationSpec {
            enabled: sizing.enabled,
            n_layers: sizing.n_layers,
            expansion_ratio: sizing.expansion_ratio,
            wall_distance_m: sizing.selected_wall_distance_m,
            first_layer_thickness_m: sizing.first_layer_thickness_m,
            total_thickness_m: sizing.total_thickness_m,
            total_thickness_chords,
            target_y_plus: sizing.target_y_plus,
            estimated_y_plus: sizing.estimated_y_plus,
            wall_distance_for_target_m: sizing.derived_wall_distance_m,
            y_plus_ratio,
            consistency,
            friction_velocity_m_s: sizing.friction_velocity_m_s,
            kinematic_viscosity_m2_s: sizing.kinematic_viscosity_m2_s,
            skin_friction_estimate: sizing.estimated_skin_friction_coefficient,
            interpretation: "wall distance is the first cell-centre distance; the mesher receives twice that as the first-layer thickness; y+ is a smooth flat-plate estimate (Cf = 0.026 Re^-1/7), not a solved value".to_owned(),
        },
        warnings,
    })
}
