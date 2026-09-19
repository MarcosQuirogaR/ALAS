// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Versioned chord-scaled two-dimensional external-flow domain.
//!
//! The domain is a rectangle around a unit-chord section scaled to the
//! configured chord, extruded one cell in `+z`.  Patch names are the contract
//! shared by the Gmsh physical groups, the converted `polyMesh/boundary`
//! and the `0/*` field dictionaries; they never change with the airfoil.

use serde::{Deserialize, Serialize};

use super::generation::{domain_bounds, validate_mesh_geometry_controls, wake_box};
use super::{required_boundary_types, MeshError};
use crate::{CfdStudyConfig, EXTRUSION_SPAN_TO_CHORD};

/// Version of the domain contract recorded in every manifest.
pub const DOMAIN_TEMPLATE_VERSION: &str = "alas-airfoil-2d-external-domain-v1";
/// Extents at or above which the template's default is considered
/// far-field-independent for attached flow; smaller values are accepted and
/// flagged, never validated.
pub const RECOMMENDED_UPSTREAM_CHORDS: f64 = 10.0;
/// See [`RECOMMENDED_UPSTREAM_CHORDS`].
pub const RECOMMENDED_DOWNSTREAM_CHORDS: f64 = 20.0;
/// See [`RECOMMENDED_UPSTREAM_CHORDS`].
pub const RECOMMENDED_HALF_HEIGHT_CHORDS: f64 = 10.0;
/// Blockage (maximum thickness over domain height) above which a warning is
/// recorded.
pub const MAX_RECOMMENDED_BLOCKAGE: f64 = 0.02;

/// Role of a named patch in the study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchRole {
    /// The section surface (no-slip wall).
    Airfoil,
    /// Upstream boundary at minimum `x`.
    Inlet,
    /// Downstream boundary at maximum `x`.
    Outlet,
    /// Upper and lower boundaries at plus or minus half height.
    FarField,
    /// The two extrusion planes carrying the `empty` constraint.
    FrontAndBack,
}

/// One named patch of the contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchSpec {
    /// Exact patch name used by Gmsh, `polyMesh/boundary` and `0/*`.
    pub name: String,
    /// Role.
    pub role: PatchRole,
    /// OpenFOAM patch type the converted boundary must carry.
    pub openfoam_type: String,
    /// Where the patch lies in the chord frame.
    pub placement: String,
}

/// Rectangle extents in metres and chords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainExtents {
    /// Chord in metres.
    pub chord_m: f64,
    /// Upstream extent in chords (inlet at `-upstream_chords * c`).
    pub upstream_chords: f64,
    /// Downstream extent in chords (outlet at `+downstream_chords * c`).
    pub downstream_chords: f64,
    /// Half height in chords.
    pub half_height_chords: f64,
    /// Inlet plane, metres.
    pub min_x_m: f64,
    /// Outlet plane, metres.
    pub max_x_m: f64,
    /// Lower far-field plane, metres.
    pub min_y_m: f64,
    /// Upper far-field plane, metres.
    pub max_y_m: f64,
    /// Extrusion span, metres.
    pub span_m: f64,
    /// Total width, metres.
    pub width_m: f64,
    /// Total height, metres.
    pub height_m: f64,
}

/// Wake refinement box in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WakeBox {
    /// Upstream face (the trailing-edge plane).
    pub min_x_m: f64,
    /// Downstream face (the outlet).
    pub max_x_m: f64,
    /// Lower face.
    pub min_y_m: f64,
    /// Upper face.
    pub max_y_m: f64,
}

/// Frame statement recorded with the domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameStatement {
    /// Chord axis.
    pub chord_axis: String,
    /// Section normal axis.
    pub normal_axis: String,
    /// Extrusion axis.
    pub extrusion_axis: String,
    /// Origin statement.
    pub origin: String,
}

/// The complete domain contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainTemplate {
    /// Contract version.
    pub version: String,
    /// Shape identifier.
    pub shape: String,
    /// Frame statement.
    pub frame: FrameStatement,
    /// Extents.
    pub extents: DomainExtents,
    /// Named patches in contract order.
    pub patches: Vec<PatchSpec>,
    /// Wake refinement box.
    pub wake_box: WakeBox,
    /// Maximum section thickness over domain height, when the thickness is known.
    pub blockage_ratio: Option<f64>,
    /// Advisory findings (never blocking).
    pub warnings: Vec<String>,
}

/// The five patches of the contract, in the order the GEO declares them.
pub fn patch_contract() -> Vec<PatchSpec> {
    let types = required_boundary_types();
    let spec = |name: &str, role: PatchRole, placement: &str| PatchSpec {
        name: name.to_owned(),
        role,
        openfoam_type: types.get(name).cloned().unwrap_or_default(),
        placement: placement.to_owned(),
    };
    vec![
        spec(
            "frontAndBack",
            PatchRole::FrontAndBack,
            "planes z = 0 and z = span",
        ),
        spec(
            "farField",
            PatchRole::FarField,
            "planes y = -half height and y = +half height",
        ),
        spec("outlet", PatchRole::Outlet, "plane x = +downstream extent"),
        spec("inlet", PatchRole::Inlet, "plane x = -upstream extent"),
        spec(
            "airfoil",
            PatchRole::Airfoil,
            "exact section polygon scaled to the chord",
        ),
    ]
}

/// Build the domain contract for a configuration.
///
/// `max_thickness_ratio` (from the geometry audit) feeds the blockage
/// estimate; `None` leaves it unreported.  Extents outside the supported
/// chord-scaled ranges are errors; extents below the template defaults are
/// accepted and recorded as warnings because far-field independence is a
/// separate study, not a property of this contract.
pub fn domain_template(
    config: &CfdStudyConfig,
    max_thickness_ratio: Option<f64>,
) -> Result<DomainTemplate, MeshError> {
    let chord_m = super::generation::positive_finite(config.chord_m, "chord")?;
    validate_mesh_geometry_controls(config)?;
    let (min_x_m, max_x_m, min_y_m, max_y_m) = domain_bounds(config);
    let (wake_min_x_m, wake_max_x_m, wake_min_y_m, wake_max_y_m) = wake_box(config);
    let height_m = max_y_m - min_y_m;
    let extents = DomainExtents {
        chord_m,
        upstream_chords: config.mesh.upstream_chords,
        downstream_chords: config.mesh.downstream_chords,
        half_height_chords: config.mesh.half_height_chords,
        min_x_m,
        max_x_m,
        min_y_m,
        max_y_m,
        span_m: chord_m * EXTRUSION_SPAN_TO_CHORD,
        width_m: max_x_m - min_x_m,
        height_m,
    };
    let blockage_ratio = max_thickness_ratio
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .map(|ratio| ratio * chord_m / height_m);
    let mut warnings = Vec::new();
    for (label, value, recommended) in [
        (
            "upstream",
            config.mesh.upstream_chords,
            RECOMMENDED_UPSTREAM_CHORDS,
        ),
        (
            "downstream",
            config.mesh.downstream_chords,
            RECOMMENDED_DOWNSTREAM_CHORDS,
        ),
        (
            "half-height",
            config.mesh.half_height_chords,
            RECOMMENDED_HALF_HEIGHT_CHORDS,
        ),
    ] {
        if value < recommended {
            warnings.push(format!(
                "{label} extent {value} c is below the {recommended} c template default; far-field independence has not been established for this domain"
            ));
        }
    }
    if let Some(blockage) = blockage_ratio {
        if blockage > MAX_RECOMMENDED_BLOCKAGE {
            warnings.push(format!(
                "blockage ratio {blockage:.4} exceeds {MAX_RECOMMENDED_BLOCKAGE}; increase the half height"
            ));
        }
    }
    // No separate lift-interference advisory is emitted.  A 10-chord versus
    // 25-chord pair at two angles moved the measured lift-curve slope by only
    // 0.7 % (0.12073 -> 0.11990 per degree; dispatch evidence
    // `.agent/opus-cfd-20260916`, cases E10/F02 against H10/H11), which agrees
    // with classical closed-boundary interference being sub-1 % at these
    // distances.  The template's residual lift excess is a viscous
    // trailing-edge effect, not a domain effect, so a domain-distance warning
    // would point the user at the wrong thing.
    Ok(DomainTemplate {
        version: DOMAIN_TEMPLATE_VERSION.to_owned(),
        shape: "rectangle".to_owned(),
        frame: FrameStatement {
            chord_axis: "+x".to_owned(),
            normal_axis: "+y".to_owned(),
            extrusion_axis: "+z".to_owned(),
            origin: "leading edge at (0, 0); trailing edge at (chord, ~0)".to_owned(),
        },
        extents,
        patches: patch_contract(),
        wake_box: WakeBox {
            min_x_m: wake_min_x_m,
            max_x_m: wake_max_x_m,
            min_y_m: wake_min_y_m,
            max_y_m: wake_max_y_m,
        },
        blockage_ratio,
        warnings,
    })
}
