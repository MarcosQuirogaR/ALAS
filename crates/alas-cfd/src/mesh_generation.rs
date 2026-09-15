// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    AirfoilSnapshot, AirfoilTopology, BoundaryLayerSizing, CfdStudyConfig, EdgeKind, GmshGeo,
    MeshError, MeshGeometryReport, BOUNDARY_LAYER_EXPANSION_RATIO, CLOSURE_TOLERANCE,
    EDGE_X_TOLERANCE, GMSH_TEMPLATE_VERSION, MAX_EDGE_POINTS,
};
#[path = "mesh_render.rs"]
mod render;
pub(super) use render::{characteristic_lengths, domain_bounds, leading_edge_size, wake_box};

/// Build an exact polygonal Gmsh source and evidence report.
///
/// The snapshot must have been resolved by [`crate::resolve_airfoil`] (or
/// carry the same FNV-1a coordinate hash).  The function rejects invalid
/// hashes, self-intersections, degenerate loops, and sections with more than
/// two distinct points at either chordwise extreme.  Those cases require a
/// different geometry contract and are never silently replaced by another
/// airfoil.
pub fn build_gmsh_geo(
    config: &CfdStudyConfig,
    airfoil: &AirfoilSnapshot,
) -> Result<GmshGeo, MeshError> {
    validate_mesh_geometry_controls(config)?;
    let points = validated_points(config, airfoil)?;
    let sizing = boundary_layer_sizing(config)?;
    let chord = config.chord_m;
    let (min_x, max_x, _min_y, _max_y) = bounds(&points);
    let signed_area_unit = signed_area(&points);
    let signed_area_m2 = signed_area_unit * chord * chord;
    let topology = classify_topology(&points, min_x, max_x)?;
    let (domain_min_x_m, domain_max_x_m, domain_min_y_m, domain_max_y_m) =
        render::domain_bounds(config);
    let extrusion_span_m =
        positive_finite(chord * crate::EXTRUSION_SPAN_TO_CHORD, "extrusion span")?;
    let (far_size_m, wake_size_m, surface_size_m) = render::characteristic_lengths(config, chord);

    let source = render::render_geo(
        config,
        airfoil,
        &points,
        signed_area_unit,
        &topology,
        &sizing,
        domain_min_x_m,
        domain_max_x_m,
        domain_min_y_m,
        domain_max_y_m,
        extrusion_span_m,
        far_size_m,
        wake_size_m,
        surface_size_m,
    );

    Ok(GmshGeo {
        source,
        report: MeshGeometryReport {
            template_version: GMSH_TEMPLATE_VERSION.to_owned(),
            airfoil_name: airfoil.name.clone(),
            coordinate_hash: airfoil.coordinate_hash.clone(),
            input_coordinate_count: airfoil.coordinates.len(),
            emitted_coordinate_count: points.len(),
            removed_closing_duplicate: airfoil.coordinates.len() != points.len(),
            signed_area_m2,
            chord_m: chord,
            domain_min_x_m,
            domain_max_x_m,
            domain_min_y_m,
            domain_max_y_m,
            extrusion_span_m,
            topology,
            exact_polygon_geometry: true,
            geometry_resampling_error_m: 0.0,
            boundary_layer: sizing,
        },
    })
}

/// Build only the Gmsh source text.
///
/// This convenience wrapper is useful for callers that do not need to retain
/// the report in memory.  Use [`build_gmsh_geo`] when writing provenance.
pub fn generate_gmsh_geo(
    config: &CfdStudyConfig,
    airfoil: &AirfoilSnapshot,
) -> Result<String, MeshError> {
    build_gmsh_geo(config, airfoil).map(|artifact| artifact.source)
}

/// Derive a first-cell-centre wall distance from the configured target y+.
///
/// The estimate uses `Cf = 0.026 Re^(-1/7)` and
/// `u_tau = U sqrt(Cf/2)`.  It is a mesh sizing estimate only; the achieved
/// y+ must be measured from the solved OpenFOAM wall distance and wall shear.
pub fn derive_first_layer_wall_distance_m(config: &CfdStudyConfig) -> Result<f64, MeshError> {
    let sizing = boundary_layer_sizing(config)?;
    Ok(sizing.derived_wall_distance_m)
}

/// Compute all first-layer and total-layer dimensions used by the GEO.
pub fn boundary_layer_sizing(config: &CfdStudyConfig) -> Result<BoundaryLayerSizing, MeshError> {
    positive_finite(config.chord_m, "chord")?;
    let density = positive_finite(config.density_kg_m3, "density")?;
    let dynamic_viscosity = positive_finite(config.dynamic_viscosity_pa_s, "dynamic viscosity")?;
    let speed = positive_finite(config.effective_speed_m_s(), "effective speed")?;
    let reynolds = positive_finite(config.effective_reynolds(), "effective Reynolds number")?;
    let target_y_plus = positive_finite(config.mesh.target_y_plus, "target y+")?;
    let requested_wall_distance_m = positive_finite(
        config.mesh.first_layer_height_m,
        "first-layer wall distance",
    )?;
    if config.mesh.boundary_layers && !(1..=30).contains(&config.mesh.n_layers) {
        return Err(MeshError::InvalidInput(
            "boundary-layer count must be between 1 and 30".to_owned(),
        ));
    }

    let kinematic_viscosity_m2_s =
        positive_finite(dynamic_viscosity / density, "kinematic viscosity")?;
    let estimated_skin_friction_coefficient = positive_finite(
        0.026 / reynolds.powf(1.0 / 7.0),
        "estimated skin-friction coefficient",
    )?;
    let friction_velocity_m_s = speed * (estimated_skin_friction_coefficient / 2.0).sqrt();
    if !friction_velocity_m_s.is_finite() || friction_velocity_m_s <= 0.0 {
        return Err(MeshError::InvalidInput(
            "the estimated friction velocity is not finite and positive".to_owned(),
        ));
    }
    let derived_wall_distance_m = positive_finite(
        target_y_plus * kinematic_viscosity_m2_s / friction_velocity_m_s,
        "derived first-layer wall distance",
    )?;
    let selected_wall_distance_m = requested_wall_distance_m;
    let first_layer_thickness_m =
        positive_finite(2.0 * selected_wall_distance_m, "first-layer thickness")?;
    let n_layers = if config.mesh.boundary_layers {
        config.mesh.n_layers
    } else {
        0
    };
    let total_thickness_m = if n_layers == 0 {
        0.0
    } else {
        positive_finite(
            geometric_layer_sum(
                first_layer_thickness_m,
                BOUNDARY_LAYER_EXPANSION_RATIO,
                n_layers,
            ),
            "boundary-layer total thickness",
        )?
    };
    let estimated_y_plus = positive_finite(
        selected_wall_distance_m * friction_velocity_m_s / kinematic_viscosity_m2_s,
        "estimated y+",
    )?;
    Ok(BoundaryLayerSizing {
        enabled: config.mesh.boundary_layers,
        n_layers,
        expansion_ratio: BOUNDARY_LAYER_EXPANSION_RATIO,
        requested_wall_distance_m,
        derived_wall_distance_m,
        selected_wall_distance_m,
        first_layer_thickness_m,
        total_thickness_m,
        friction_velocity_m_s,
        estimated_y_plus,
        target_y_plus,
        kinematic_viscosity_m2_s,
        estimated_skin_friction_coefficient,
    })
}
fn validated_points(
    config: &CfdStudyConfig,
    airfoil: &AirfoilSnapshot,
) -> Result<Vec<(f64, f64)>, MeshError> {
    if airfoil.name.trim().is_empty() {
        return Err(MeshError::InvalidInput(
            "airfoil snapshot name is empty".to_owned(),
        ));
    }
    if airfoil.coordinate_hash != coordinate_hash(&airfoil.name, &airfoil.coordinates) {
        return Err(MeshError::InvalidInput(format!(
            "coordinate hash for '{}' does not match the exact snapshot",
            airfoil.name
        )));
    }
    let chord = positive_finite(config.chord_m, "chord")?;
    let mut points = airfoil.coordinates.clone();
    // A database section may carry an explicit closing copy of its first
    // coordinate.  The parent validator treats that copy as a zero-length
    // final edge and can consequently report false self-intersections.  Keep
    // the raw coordinate list (and its hash) for provenance, but validate the
    // actual polygon loop after removing only that exact closure copy.
    while points.len() >= 2
        && distance_sq(points[0], points[points.len() - 1]) <= CLOSURE_TOLERANCE * CLOSURE_TOLERANCE
    {
        points.pop();
    }
    if points.len() < 8 {
        return Err(MeshError::UnsupportedGeometry(
            "removing a repeated closing coordinate leaves fewer than eight section points"
                .to_owned(),
        ));
    }
    crate::validate_coordinates(&points).map_err(MeshError::InvalidInput)?;
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(MeshError::InvalidInput(
            "airfoil coordinates must be finite".to_owned(),
        ));
    }
    if points
        .iter()
        .any(|(x, y)| !(*x * chord).is_finite() || !(*y * chord).is_finite())
    {
        return Err(MeshError::InvalidInput(
            "scaled airfoil coordinates are not finite for the configured chord".to_owned(),
        ));
    }
    let (min_x, max_x, _, _) = bounds(&points);
    let span_x = max_x - min_x;
    if span_x < 0.5 {
        return Err(MeshError::UnsupportedGeometry(
            "airfoil chord span is too small after validation".to_owned(),
        ));
    }
    let tolerance = EDGE_X_TOLERANCE.max(1.0e-12 * span_x);
    let min_count = points
        .iter()
        .filter(|(x, _)| (*x - min_x).abs() <= tolerance)
        .count();
    let max_count = points
        .iter()
        .filter(|(x, _)| (*x - max_x).abs() <= tolerance)
        .count();
    if min_count == 0 || max_count == 0 {
        return Err(MeshError::UnsupportedGeometry(
            "the section has no identifiable leading or trailing edge".to_owned(),
        ));
    }
    if min_count > MAX_EDGE_POINTS {
        return Err(MeshError::UnsupportedGeometry(format!(
            "leading-edge extreme contains {min_count} points; at most two are supported"
        )));
    }
    if max_count > MAX_EDGE_POINTS {
        return Err(MeshError::UnsupportedGeometry(format!(
            "trailing-edge extreme contains {max_count} points; at most two are supported"
        )));
    }
    let area = signed_area(&points);
    if !area.is_finite() || area.abs() <= 1.0e-12 * span_x * span_x {
        return Err(MeshError::UnsupportedGeometry(
            "airfoil loop encloses no measurable area".to_owned(),
        ));
    }
    let scaled_area = area * chord * chord;
    if !scaled_area.is_finite() {
        return Err(MeshError::InvalidInput(
            "scaled airfoil area is not finite".to_owned(),
        ));
    }
    for (index, pair) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if distance_sq(*index, *pair) <= f64::EPSILON {
            return Err(MeshError::UnsupportedGeometry(
                "airfoil loop contains a zero-length edge".to_owned(),
            ));
        }
    }
    Ok(points)
}

pub(super) fn validate_mesh_geometry_controls(config: &CfdStudyConfig) -> Result<(), MeshError> {
    for (name, value) in [
        ("upstream domain extent", config.mesh.upstream_chords),
        ("downstream domain extent", config.mesh.downstream_chords),
        ("half-height domain extent", config.mesh.half_height_chords),
    ] {
        positive_finite(value, name)?;
    }
    if !(2.0..=100.0).contains(&config.mesh.upstream_chords)
        || !(5.0..=200.0).contains(&config.mesh.downstream_chords)
        || !(2.0..=100.0).contains(&config.mesh.half_height_chords)
    {
        return Err(MeshError::InvalidInput(
            "domain extents are outside the supported chord-scaled range".to_owned(),
        ));
    }
    if config.mesh.wake_refinement > 6 {
        return Err(MeshError::InvalidInput(
            "wake refinement must be between zero and six".to_owned(),
        ));
    }
    if config.mesh.leading_edge_refinement > 6 {
        return Err(MeshError::InvalidInput(
            "leading-edge refinement must be between zero and six".to_owned(),
        ));
    }
    Ok(())
}

fn classify_topology(
    points: &[(f64, f64)],
    min_x: f64,
    max_x: f64,
) -> Result<AirfoilTopology, MeshError> {
    let tolerance = EDGE_X_TOLERANCE.max(1.0e-12 * (max_x - min_x));
    let leading = points
        .iter()
        .filter(|(x, _)| (*x - min_x).abs() <= tolerance)
        .map(|(_, y)| *y)
        .collect::<Vec<_>>();
    let trailing = points
        .iter()
        .filter(|(x, _)| (*x - max_x).abs() <= tolerance)
        .map(|(_, y)| *y)
        .collect::<Vec<_>>();
    if leading.is_empty() || trailing.is_empty() {
        return Err(MeshError::UnsupportedGeometry(
            "cannot classify leading/trailing edge".to_owned(),
        ));
    }
    let leading_gap = vertical_gap(&leading);
    let trailing_gap = vertical_gap(&trailing);
    Ok(AirfoilTopology {
        leading_edge: if leading_gap <= 1.0e-8 {
            EdgeKind::Sharp
        } else {
            EdgeKind::Blunt
        },
        trailing_edge: if trailing_gap <= 1.0e-8 {
            EdgeKind::Sharp
        } else {
            EdgeKind::Blunt
        },
        leading_edge_points: leading.len(),
        trailing_edge_points: trailing.len(),
    })
}

pub(super) fn geometric_layer_sum(first: f64, ratio: f64, n_layers: u32) -> f64 {
    if n_layers == 0 {
        return 0.0;
    }
    first * (ratio.powi(n_layers as i32) - 1.0) / (ratio - 1.0)
}

pub(super) fn positive_finite(value: f64, name: &str) -> Result<f64, MeshError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(MeshError::InvalidInput(format!(
            "{name} must be finite and greater than zero"
        )))
    }
}

pub(super) fn bounds(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), &(x, y)| {
            (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y))
        },
    )
}

pub(super) fn vertical_gap(values: &[f64]) -> f64 {
    let Some(minimum) = values.iter().copied().reduce(f64::min) else {
        return 0.0;
    };
    let Some(maximum) = values.iter().copied().reduce(f64::max) else {
        return 0.0;
    };
    maximum - minimum
}

pub(super) fn signed_area(points: &[(f64, f64)]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(&(x0, y0), &(x1, y1))| x0.mul_add(y1, -x1 * y0))
        .sum::<f64>()
        * 0.5
}

pub(super) fn distance_sq(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).mul_add(a.0 - b.0, (a.1 - b.1) * (a.1 - b.1))
}

pub(super) fn coordinate_hash(name: &str, coordinates: &[(f64, f64)]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for &(x, y) in coordinates {
        for byte in x.to_le_bytes().into_iter().chain(y.to_le_bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("fnv1a64-{hash:016x}")
}
