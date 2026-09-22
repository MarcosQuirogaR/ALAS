// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::{
    AirfoilSnapshot, AirfoilTopology, BoundaryLayerSizing, CfdStudyConfig, EdgeKind,
    GMSH_TEMPLATE_VERSION,
};
use crate::MeshPreset;
pub(super) fn render_geo(
    config: &CfdStudyConfig,
    airfoil: &AirfoilSnapshot,
    points: &[(f64, f64)],
    signed_area_unit: f64,
    topology: &AirfoilTopology,
    sizing: &BoundaryLayerSizing,
    domain_min_x_m: f64,
    domain_max_x_m: f64,
    domain_min_y_m: f64,
    domain_max_y_m: f64,
    extrusion_span_m: f64,
    far_size_m: f64,
    wake_size_m: f64,
    surface_size_m: f64,
) -> String {
    // Outer points are first so Gmsh's Extrude side-surface order is stable:
    // bottom, right, top, left, then the airfoil boundary lines.  These are
    // the IDs used by the physical patch declarations below.
    let mut out = String::new();
    out.push_str("// ALAS native Gmsh airfoil mesh\n");
    out.push_str(&format!("// template = {GMSH_TEMPLATE_VERSION}\n"));
    out.push_str(&format!("// airfoil = {}\n", airfoil.name));
    out.push_str(&format!(
        "// coordinate_hash = {}\n",
        airfoil.coordinate_hash
    ));
    out.push_str("// coordinates are exact snapshot (x/c,y/c) values scaled to metres\n");
    out.push_str("SetFactory(\"Built-in\");\n");
    out.push_str("Mesh.MshFileVersion = 2.2;\n");
    // Delaunay (algorithm 5) handles the very short straight edges produced
    // by closed/blunt trailing-edge coordinate sets better than the
    // Frontal-Delaunay variant (algorithm 6).  The distinction is material
    // after extrusion: algorithm 6 can leave high-skew TE side faces even
    // when the 2-D surface itself has no invalid cells.
    // Pin the meshing RNG so the only remaining source of run-to-run variation
    // is named rather than anonymous.  Measured on this host with Gmsh 4.15.2,
    // single-threaded, on a byte-identical `.geo` (internal CFD convergence
    // study, 2026-09-16, case q16): three runs gave 89 794 /
    // 89 732 / 89 902 nodes.  Pinning the seed does NOT remove that, and
    // neither does `Mesh.Optimize = 0`, `Mesh.OptimizeNetgen = 0`,
    // `Mesh.RandomFactor`, nor `Mesh.Algorithm = 6`.  Deleting
    // `BoundaryLayer Field` does: three runs then produce bit-identical meshes
    // (77 556 nodes each, identical md5).  The non-determinism is inside Gmsh's
    // `BoundaryLayer` field, so the seed is necessary hygiene and the real fix
    // is an explicit prism-layer construction that does not use that field.
    out.push_str("Mesh.RandomSeed = 1;\n");
    out.push_str("Mesh.Algorithm = 5;\n");
    out.push_str("Mesh.Algorithm3D = 1;\n");
    out.push_str("Mesh.ElementOrder = 1;\n");
    out.push_str("Mesh.Optimize = 1;\n");
    out.push_str("Mesh.OptimizeNetgen = 1;\n");
    out.push_str("Mesh.Smoothing = 5;\n");
    out.push_str("Mesh.CharacteristicLengthExtendFromBoundary = 0;\n");
    out.push_str("Mesh.CharacteristicLengthFromPoints = 1;\n");
    out.push_str("Mesh.CharacteristicLengthFromCurvature = 0;\n");
    out.push_str(&format!(
        "Mesh.CharacteristicLengthMax = {far_size_m:.16e};\n"
    ));
    out.push_str(&format!(
        "Mesh.CharacteristicLengthMin = {min:.16e};\n",
        // Do not clip the first Gmsh boundary-layer element with a global
        // minimum.  Gmsh's BoundaryLayer field is allowed to request a
        // smaller SI size than the outer isotropic background field.
        min = if sizing.enabled {
            (sizing.first_layer_thickness_m * 0.1).min(surface_size_m * 0.05)
        } else {
            surface_size_m * 0.05
        }
        .max(f64::MIN_POSITIVE)
    ));
    out.push_str("\n// Outer rectangle and exact airfoil polygon points\n");
    for (id, (x, y)) in [
        (1_i32, (domain_min_x_m, domain_min_y_m)),
        (2, (domain_max_x_m, domain_min_y_m)),
        (3, (domain_max_x_m, domain_max_y_m)),
        (4, (domain_min_x_m, domain_max_y_m)),
    ] {
        out.push_str(&format!(
            "Point({id}) = {{{x:.16e}, {y:.16e}, 0, {far_size_m:.16e}}};\n"
        ));
    }
    let point_offset = 5_i32;
    for (index, &(x, y)) in points.iter().enumerate() {
        let id = point_offset + index as i32;
        out.push_str(&format!(
            "Point({id}) = {{{x:.16e}, {y:.16e}, 0, {surface_size_m:.16e}}};\n",
            x = x * config.chord_m,
            y = y * config.chord_m,
        ));
    }
    out.push_str("\n// Outer loop is counter-clockwise; the airfoil hole is oriented opposite to its source area.\n");
    out.push_str("Line(1) = {1, 2};\nLine(2) = {2, 3};\nLine(3) = {3, 4};\nLine(4) = {4, 1};\n");
    let airfoil_line_offset = 5_i32;
    for index in 0..points.len() {
        let id = airfoil_line_offset + index as i32;
        let first = point_offset + index as i32;
        let second = point_offset + ((index + 1) % points.len()) as i32;
        out.push_str(&format!("Line({id}) = {{{first}, {second}}};\n"));
    }
    out.push_str("Curve Loop(1) = {1, 2, 3, 4};\n");
    out.push_str("Curve Loop(2) = {");
    let hole_sign = if signed_area_unit > 0.0 {
        -1_i32
    } else {
        1_i32
    };
    for index in 0..points.len() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!(
            "{}",
            hole_sign * (airfoil_line_offset + index as i32)
        ));
    }
    out.push_str("};\nPlane Surface(1) = {1, 2};\n\n");

    // A Distance/Threshold field refines the section outside the boundary
    // layer.  The wake Box is independently refined downstream; the Min
    // field combines both constraints.
    let last_airfoil_line = airfoil_line_offset + points.len() as i32 - 1;
    out.push_str("Field[2] = Distance;\n");
    out.push_str(&format!(
        "Field[2].CurvesList = {{{airfoil_line_offset}:{last_airfoil_line}}};\n"
    ));
    out.push_str("Field[2].Sampling = 200;\n");
    out.push_str("Field[3] = Threshold;\nField[3].InField = 2;\n");
    out.push_str(&format!(
        "Field[3].SizeMin = {size_min:.16e};\nField[3].SizeMax = {far_size_m:.16e};\nField[3].DistMin = {dist_min:.16e};\nField[3].DistMax = {dist_max:.16e};\n",
        // The BoundaryLayer field owns the first layers.  Outside that
        // field, retain the exact surface spacing as the lower background
        // limit; using a fraction of it over a whole chord would create an
        // unnecessarily large isotropic cloud in the wake.
        size_min = surface_size_m.max(sizing.first_layer_thickness_m * 2.0),
        dist_min = if sizing.enabled { sizing.total_thickness_m.max(config.chord_m * 0.005) } else { config.chord_m * 0.005 },
        dist_max = config.chord_m * 0.75,
    ));
    let (_, _, wake_y_min_m, wake_y_max_m) = wake_box(config);
    out.push_str("Field[4] = Box;\n");
    out.push_str(&format!(
        "Field[4].VIn = {wake_size_m:.16e};\nField[4].VOut = {far_size_m:.16e};\nField[4].XMin = 0;\nField[4].XMax = {domain_max_x_m:.16e};\nField[4].YMin = {y_min:.16e};\nField[4].YMax = {y_max:.16e};\nField[4].ZMin = 0;\nField[4].ZMax = {extrusion_span_m:.16e};\n",
        y_min = wake_y_min_m,
        y_max = wake_y_max_m,
    ));
    let le_size_m = leading_edge_size(config, surface_size_m);
    if config.mesh.leading_edge_refinement > 0 && le_size_m < surface_size_m {
        // Optional leading-edge refinement (template v2): a Distance/Threshold
        // pair around the minimum-x coordinate, combined through the same Min
        // field.  At level zero this block is absent and the source is
        // identical to template v1.
        let le_index = points
            .iter()
            .enumerate()
            .min_by(|left, right| left.1 .0.total_cmp(&right.1 .0))
            .map_or(0, |(index, _)| index);
        out.push_str("Field[6] = Distance;\n");
        out.push_str(&format!(
            "Field[6].PointsList = {{{}}};\n",
            point_offset + le_index as i32
        ));
        out.push_str("Field[7] = Threshold;\nField[7].InField = 6;\n");
        out.push_str(&format!(
            // `SizeMax` must be the FAR-FIELD size, not the surface size.
            // `Field[5]` is `Min{3, 4, 7}` and a `Threshold` returns its
            // `SizeMax` everywhere beyond `DistMax`, so a leading-edge field
            // that relaxes only to the surface size clamps the whole domain to
            // it: `Field[3]` and `Field[4]` both relax to `far_size_m`, and the
            // `Min` then throws that away.  Measured on this host with the
            // medium preset at level 2 — surface `1.0e-2 m`, far field
            // `1.667e-1 m`, domain 30 m by 20 m — the background cell size
            // outside the leading-edge ball fell by 16.7x, which is a 280x
            // area-density increase over roughly 600 m^2; Gmsh 4.15.2 reached
            // 2.6 GB of resident memory and had produced no mesh after eight
            // minutes, against ten seconds at level 0.  Relaxing to
            // `far_size_m` makes the field impose nothing outside its own ball,
            // which is what "leading-edge refinement" is supposed to mean.
            "Field[7].SizeMin = {le_size_m:.16e};\nField[7].SizeMax = {far_size_m:.16e};\nField[7].DistMin = {dist_min:.16e};\nField[7].DistMax = {dist_max:.16e};\n",
            dist_min = LEADING_EDGE_REFINEMENT_INNER_CHORDS * config.chord_m,
            dist_max = LEADING_EDGE_REFINEMENT_OUTER_CHORDS * config.chord_m,
        ));
        out.push_str(
            "Field[5] = Min;\nField[5].FieldsList = {3, 4, 7};\nBackground Field = 5;\n\n",
        );
    } else {
        out.push_str("Field[5] = Min;\nField[5].FieldsList = {3, 4};\nBackground Field = 5;\n\n");
    }

    if sizing.enabled {
        out.push_str("// SI boundary-layer field: Size is first-cell thickness, twice wall-centre distance.\n");
        out.push_str("Field[1] = BoundaryLayer;\n");
        out.push_str(&format!(
            "Field[1].CurvesList = {{{airfoil_line_offset}:{last_airfoil_line}}};\n"
        ));
        out.push_str(&format!(
            "Field[1].Size = {size:.16e};\nField[1].Thickness = {thickness:.16e};\nField[1].Ratio = {ratio:.16e};\nField[1].NbLayers = {n_layers};\nField[1].Quads = 1;\n",
            size = sizing.first_layer_thickness_m,
            thickness = sizing.total_thickness_m,
            ratio = sizing.expansion_ratio,
            n_layers = sizing.n_layers,
        ));
        // Both trailing-edge corners are convex corners of the fluid domain, and
        // the prism stack has to turn through them.  Told to fan, Gmsh sweeps
        // the layers around the corner; left alone, it stitches the upper and
        // lower stacks together behind the section with whatever elements close
        // the gap, and those elements are the worst in the mesh.
        //
        // A blunt edge used to be excluded here, on the assumption that its two
        // separate endpoints would make fanned quads degenerate.  **Measured,
        // that assumption is wrong and the exclusion was the defect.** On
        // `n0012` (blunt), fanning both corners and changing nothing else:
        //
        // | preset | max non-orthogonality | max skewness |
        // |---|---|---|
        // | coarse | 37.891 -> 37.321 deg | 1.8138 -> 0.6441 |
        // | medium | 46.185 -> 50.235 deg | 1.8138 -> 0.5948 |
        // | **fine** | **71.369 -> 39.812 deg** | 1.8138 -> 0.6013 |
        //
        // The fine preset violated the declared `70 deg` limit before and clears
        // it by 30 deg after.  The identical `1.8138019812` skewness at all
        // three presets was the stitched region itself, and it is gone.  Cell
        // count and meshing time are unchanged to within 0.1 %, aspect ratio to
        // within 0.3 %, and `checkMesh` reports `Mesh OK` in every case.  No
        // size field, layer count, layer thickness, first-layer height or
        // coordinate changed: this is a local topology correction only.
        let fan_points: Vec<i32> = match topology.trailing_edge {
            // The closing point of a sharp edge is the single rearmost point.
            EdgeKind::Sharp => points
                .iter()
                .enumerate()
                .max_by(|left, right| left.1 .0.total_cmp(&right.1 .0))
                .map(|(index, _)| vec![point_offset + index as i32])
                .unwrap_or_default(),
            // A blunt edge is closed by the segment from the last point back to
            // the first, so its corners are exactly those two endpoints.
            EdgeKind::Blunt => (points.len() >= 2)
                .then(|| vec![point_offset, point_offset + points.len() as i32 - 1])
                .unwrap_or_default(),
        };
        if !fan_points.is_empty() {
            if topology.trailing_edge == EdgeKind::Sharp {
                out.push_str("Mesh.BoundaryLayerFanElements = 7;\n");
            }
            out.push_str(&format!(
                "Field[1].FanPointsList = {{{}}};\n",
                fan_points
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out.push_str("BoundaryLayer Field = 1;\n\n");
    }

    out.push_str(&format!(
        "out[] = Extrude {{0, 0, {extrusion_span_m:.16e}}} {{ Surface{{1}}; Layers{{1}}; Recombine; }};\n\n"
    ));
    out.push_str("// Physical groups are consumed by gmshToFoam.\n");
    out.push_str("Physical Surface(\"frontAndBack\") = {1, out[0]};\n");
    out.push_str("Physical Surface(\"farField\") = {out[2], out[4]};\n");
    out.push_str("Physical Surface(\"outlet\") = {out[3]};\n");
    out.push_str("Physical Surface(\"inlet\") = {out[5]};\n");
    out.push_str("Physical Surface(\"airfoil\") = {");
    for index in 0..points.len() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("out[{}]", 6 + index));
    }
    out.push_str("};\n");
    out.push_str("Physical Volume(\"fluid\") = {out[1]};\n");
    out.push_str("Mesh.SaveAll = 0;\n");
    out
}

pub(in crate::mesh) fn characteristic_lengths(
    config: &CfdStudyConfig,
    chord: f64,
) -> (f64, f64, f64) {
    // Keep spacing tied to the chord and preset.  If the outer rectangle is
    // enlarged, the physical resolution must remain unchanged; deriving it
    // from domain width would silently coarsen the wake and far field.  The
    // coarse value is the historical 30c x 20c, 120 x 80 baseline.
    let far_size_fraction = match config.mesh.preset {
        MeshPreset::Coarse => 0.25,
        MeshPreset::Medium => 1.0 / 6.0,
        MeshPreset::Fine => 0.10714285714285714,
    };
    let far_size_m = (far_size_fraction * chord).clamp(0.02 * chord, 2.0 * chord);
    let wake_divisor = 2_f64.powi(config.mesh.wake_refinement as i32);
    let wake_size_m = (far_size_m / wake_divisor).max(0.01 * chord);
    let surface_fraction = match config.mesh.preset {
        MeshPreset::Coarse => 0.02,
        MeshPreset::Medium => 0.01,
        MeshPreset::Fine => 0.005,
    };
    let surface_size_m = (surface_fraction * chord).max(0.002 * chord);
    (far_size_m, wake_size_m, surface_size_m)
}

pub(in crate::mesh) fn domain_bounds(config: &CfdStudyConfig) -> (f64, f64, f64, f64) {
    let chord = config.chord_m;
    (
        -config.mesh.upstream_chords * chord,
        config.mesh.downstream_chords * chord,
        -config.mesh.half_height_chords * chord,
        config.mesh.half_height_chords * chord,
    )
}

/// Radius (chords) inside which the leading-edge refinement size applies.
pub(super) const LEADING_EDGE_REFINEMENT_INNER_CHORDS: f64 = 0.02;
/// Radius (chords) beyond which the surface size applies again.
pub(super) const LEADING_EDGE_REFINEMENT_OUTER_CHORDS: f64 = 0.15;
/// Smallest leading-edge size, as a chord fraction.
pub(super) const MIN_LEADING_EDGE_SIZE_CHORDS: f64 = 0.0005;

/// Leading-edge target size in metres for the configured refinement level.
///
/// Level zero returns the surface size unchanged; each level halves it,
/// bounded below by [`MIN_LEADING_EDGE_SIZE_CHORDS`].
pub(in crate::mesh) fn leading_edge_size(config: &CfdStudyConfig, surface_size_m: f64) -> f64 {
    let divisor = 2_f64.powi(config.mesh.leading_edge_refinement as i32);
    (surface_size_m / divisor).max(MIN_LEADING_EDGE_SIZE_CHORDS * config.chord_m)
}

/// Wake refinement box `(x_min, x_max, y_min, y_max)` in metres: from the
/// trailing-edge plane to the outlet, two chords above and below the chord line.
pub(in crate::mesh) fn wake_box(config: &CfdStudyConfig) -> (f64, f64, f64, f64) {
    let (_, max_x, _, _) = domain_bounds(config);
    (0.0, max_x, -2.0 * config.chord_m, 2.0 * config.chord_m)
}
