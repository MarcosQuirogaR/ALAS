// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    AirfoilSnapshot, AirfoilTopology, BoundaryLayerSizing, CfdStudyConfig, EdgeKind, GmshGeo,
    MeshError, MeshGeometryReport, BOUNDARY_LAYER_COVERAGE_FACTOR, BOUNDARY_LAYER_EXPANSION_RATIO,
    CLOSURE_TOLERANCE, EDGE_X_TOLERANCE, GMSH_TEMPLATE_VERSION, MAX_DERIVED_BOUNDARY_LAYERS,
    MAX_EDGE_POINTS,
};
#[path = "mesh_render.rs"]
mod render;
pub(super) use render::{characteristic_lengths, domain_bounds, leading_edge_size, wake_box};
#[path = "mesh_layers.rs"]
mod layers;
pub(super) use layers::geometric_layer_sum;
use layers::{layers_reaching_size, layers_spanning_thickness};

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
    let topology = classify_topology(&points, min_x, max_x)?;
    let snapshot_count = points.len();
    let (points, curve_bulge_unit) = smooth_facets(&points);
    let signed_area_unit = signed_area(&points);
    let signed_area_m2 = signed_area_unit * chord * chord;
    let (domain_min_x_m, domain_max_x_m, domain_min_y_m, domain_max_y_m) =
        render::domain_bounds(config);
    let extrusion_span_m =
        positive_finite(chord * crate::EXTRUSION_SPAN_TO_CHORD, "extrusion span")?;
    let (far_size_m, wake_size_m, surface_size_m) = render::characteristic_lengths(config, chord);

    let source = render::render_geo(
        config,
        airfoil,
        &render::PolygonGeometry {
            points: &points,
            signed_area_unit,
            topology: &topology,
        },
        &sizing,
        &render::DomainGeometry {
            min_x_m: domain_min_x_m,
            max_x_m: domain_max_x_m,
            min_y_m: domain_min_y_m,
            max_y_m: domain_max_y_m,
            extrusion_span_m,
        },
        &render::MeshSizes {
            far_size_m,
            wake_size_m,
            surface_size_m,
        },
    );

    Ok(GmshGeo {
        source,
        report: MeshGeometryReport {
            template_version: GMSH_TEMPLATE_VERSION.to_owned(),
            airfoil_name: airfoil.name.clone(),
            coordinate_hash: airfoil.coordinate_hash.clone(),
            input_coordinate_count: airfoil.coordinates.len(),
            emitted_coordinate_count: points.len(),
            inserted_curve_points: points.len() - snapshot_count,
            removed_closing_duplicate: airfoil.coordinates.len() != snapshot_count,
            signed_area_m2,
            chord_m: chord,
            domain_min_x_m,
            domain_max_x_m,
            domain_min_y_m,
            domain_max_y_m,
            extrusion_span_m,
            topology,
            exact_polygon_geometry: true,
            geometry_resampling_error_m: curve_bulge_unit * chord,
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
    // The first cell governs the wall treatment, so it has to be a function of
    // the flow state.  The configured length stays available as an explicit
    // override and both values stay in the report.
    let selected_wall_distance_m = if config.mesh.derive_first_layer_from_target_y_plus {
        derived_wall_distance_m
    } else {
        requested_wall_distance_m
    };
    let first_layer_thickness_m =
        positive_finite(2.0 * selected_wall_distance_m, "first-layer thickness")?;
    // Flat-plate turbulent estimate of the layer the stack has to span.  The
    // stack is the only graded region of the mesh; anything the stack does not
    // cover falls to isotropic background cells that are typically an order of
    // magnitude larger than the outermost prism.
    let estimated_boundary_layer_thickness_m = positive_finite(
        0.37 * config.chord_m / reynolds.powf(0.2),
        "estimated boundary-layer thickness",
    )?;
    let target_total_thickness_m = positive_finite(
        BOUNDARY_LAYER_COVERAGE_FACTOR * estimated_boundary_layer_thickness_m,
        "target boundary-layer stack thickness",
    )?;
    let configured_n_layers = if config.mesh.boundary_layers {
        config.mesh.n_layers
    } else {
        0
    };
    let n_layers = if configured_n_layers == 0 {
        0
    } else {
        // The configured count is a floor, never a ceiling: a user asking for
        // more layers than the estimate needs still gets them.
        // The stack also grows until its outermost layer reaches the
        // isotropic near-wall size, so the graded region hands over to the
        // background mesh without a jump in normal spacing.  At high lift the
        // suction-side layer near the trailing edge is several times the
        // flat-plate estimate (NACA 0012, alpha 10 deg, Re 6e6: delta* 1.65 %
        // c at x/c 0.99 against a 1.6 % c flat-plate delta), and the outer
        // part of it lives on these cells.
        let (_, _, surface_size_m) = render::characteristic_lengths(config, config.chord_m);
        layers_spanning_thickness(
            first_layer_thickness_m,
            BOUNDARY_LAYER_EXPANSION_RATIO,
            target_total_thickness_m,
        )
        .max(layers_reaching_size(
            first_layer_thickness_m,
            BOUNDARY_LAYER_EXPANSION_RATIO,
            surface_size_m,
        ))
        .max(configured_n_layers)
        .min(MAX_DERIVED_BOUNDARY_LAYERS.max(configured_n_layers))
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
    let boundary_layer_coverage_ratio = total_thickness_m / estimated_boundary_layer_thickness_m;
    let estimated_y_plus = positive_finite(
        selected_wall_distance_m * friction_velocity_m_s / kinematic_viscosity_m2_s,
        "estimated y+",
    )?;
    Ok(BoundaryLayerSizing {
        enabled: config.mesh.boundary_layers,
        n_layers,
        configured_n_layers,
        expansion_ratio: BOUNDARY_LAYER_EXPANSION_RATIO,
        requested_wall_distance_m,
        derived_wall_distance_m,
        selected_wall_distance_m,
        first_layer_thickness_m,
        total_thickness_m,
        estimated_boundary_layer_thickness_m,
        target_total_thickness_m,
        boundary_layer_coverage_ratio,
        friction_velocity_m_s,
        estimated_y_plus,
        target_y_plus,
        kinematic_viscosity_m2_s,
        estimated_skin_friction_coefficient,
    })
}
/// Largest change of surface direction between consecutive emitted facets on
/// a smooth part of the section, degrees.
const MAX_FACET_TURN_DEG: f64 = 2.0;
/// A snapshot vertex whose facets turn by more than this is a corner (a
/// trailing edge) and is kept sharp.
const CORNER_TURN_DEG: f64 = 45.0;

/// Direction change at `b` between facets `a-b` and `b-c`, degrees.
fn facet_turn_deg(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
    (u.0 * v.1 - u.1 * v.0)
        .atan2(u.0 * v.0 + u.1 * v.1)
        .abs()
        .to_degrees()
}

/// Point at parameter `t` of the centripetal Catmull-Rom segment between `p1`
/// and `p2` (Barry-Goldman evaluation; C. Yuksel, S. Schaefer and J. Keyser,
/// "Parameterization and applications of Catmull-Rom curves", Computer-Aided
/// Design 43(7), 2011, pp. 747-755: the centripetal form has no cusps or
/// self-intersections within a segment).
fn catmull_rom(p: [(f64, f64); 4], fraction: f64) -> (f64, f64) {
    let knot = |a: (f64, f64), b: (f64, f64)| distance_sq(a, b).sqrt().sqrt().max(1.0e-300);
    let t0 = 0.0;
    let t1 = t0 + knot(p[0], p[1]);
    let t2 = t1 + knot(p[1], p[2]);
    let t3 = t2 + knot(p[2], p[3]);
    let t = t1 + fraction * (t2 - t1);
    let lerp = |a: (f64, f64), b: (f64, f64), ta: f64, tb: f64| {
        let (wa, wb) = ((tb - t) / (tb - ta), (t - ta) / (tb - ta));
        (wa * a.0 + wb * b.0, wa * a.1 + wb * b.1)
    };
    let a1 = lerp(p[0], p[1], t0, t1);
    let a2 = lerp(p[1], p[2], t1, t2);
    let a3 = lerp(p[2], p[3], t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

/// Subdivide the facets of the snapshot polygon on a smooth curve through
/// its own coordinates, so no smooth part turns by more than
/// [`MAX_FACET_TURN_DEG`] between emitted facets.
///
/// Every snapshot coordinate stays a vertex, in order; points are only
/// inserted between two consecutive ones, never across the closing (trailing
/// edge) segment and never next to a corner, so the edge topology and the
/// first and last points are unchanged.  A 131-point NACA 0012 snapshot
/// turns 15.6 deg at its leading-edge vertex and 10-15 deg at each of its
/// neighbours; the mesh resolves those kinks as surface corners, which raised
/// the 10 deg drag by about a sixth over the NASA Turbulence Modeling
/// Resource value, and refining the leading-edge cells made it worse.
///
/// Returns the points and the largest distance of an inserted point from the
/// snapshot facet it subdivides, in chord units.
fn smooth_facets(points: &[(f64, f64)]) -> (Vec<(f64, f64)>, f64) {
    let n = points.len();
    if n < 4 {
        return (points.to_vec(), 0.0);
    }
    // The chain is open: its two ends meet the closing segment and count as
    // corners.
    let turn: Vec<f64> = (0..n)
        .map(|i| {
            if i == 0 || i == n - 1 {
                f64::INFINITY
            } else {
                facet_turn_deg(points[i - 1], points[i], points[i + 1])
            }
        })
        .collect();
    // A corner turns far more than its neighbours (a slope discontinuity of
    // the section, such as a wedge shoulder); a sampled smooth curve turns by
    // about as much at a vertex as at the next.  Corners stay sharp.
    let corner: Vec<bool> = (0..n)
        .map(|i| {
            if i == 0 || i == n - 1 || turn[i] >= CORNER_TURN_DEG {
                return true;
            }
            let neighbours = [turn[i - 1], turn[i + 1]]
                .into_iter()
                .filter(|value| value.is_finite())
                .fold(0.0_f64, f64::max);
            turn[i] > 2.0 * neighbours + MAX_FACET_TURN_DEG
        })
        .collect();
    let mut out = Vec::with_capacity(n);
    let mut bulge = 0.0_f64;
    for i in 0..n - 1 {
        out.push(points[i]);
        if corner[i] || corner[i + 1] || i == 0 || i + 2 >= n {
            continue;
        }
        let worst = turn[i].max(turn[i + 1]);
        let pieces = (worst / MAX_FACET_TURN_DEG).ceil().max(1.0) as usize;
        let (a, b) = (points[i], points[i + 1]);
        let length = distance_sq(a, b).sqrt();
        for piece in 1..pieces {
            let q = catmull_rom(
                [points[i - 1], a, b, points[i + 2]],
                piece as f64 / pieces as f64,
            );
            let offset = ((b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0)).abs()
                / length.max(f64::MIN_POSITIVE);
            bulge = bulge.max(offset);
            out.push(q);
        }
    }
    out.push(points[n - 1]);
    (out, bulge)
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
