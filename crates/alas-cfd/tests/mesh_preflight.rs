// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Deterministic meshing preflight across representative database sections
//! and invalid inputs.  No mesher runs; nothing here is mesh-quality or CFD
//! evidence.

use alas_cfd::mesh::{
    audit_airfoil_geometry, domain_template, mesh_resolution, patch_contract, preset_catalogue,
    required_boundary_types, run_mesh_preflight, write_mesh_manifest, ClosureKind, EdgeKind,
    GeometryIssueCode, IssueSeverity, MeshError, MeshPreflight, PatchRole, PreflightStatus,
    TrailingEdgeMeshing, Winding, YPlusConsistency, DOMAIN_TEMPLATE_VERSION, GMSH_TEMPLATE_VERSION,
    MESH_MANIFEST_FILE, MESH_MANIFEST_VERSION,
};
use alas_cfd::{
    generate_case, resolve_airfoil, AirfoilSnapshot, CfdStudyConfig, MeshPreset, MeshSettings,
    TEMPLATE_VERSION,
};

/// Representative database fixtures: `(name, family, expected trailing edge)`.
const FIXTURES: &[(&str, &str, EdgeKind)] = &[
    ("SC2-0714", "default supercritical, blunt", EdgeKind::Blunt),
    ("naca0006", "thin symmetric, blunt", EdgeKind::Blunt),
    ("naca63206", "thin cambered, sharp", EdgeKind::Sharp),
    ("naca0024", "thick symmetric, blunt", EdgeKind::Blunt),
    ("fx77w343", "very thick, very blunt", EdgeKind::Blunt),
    ("s1223", "high camber, sharp", EdgeKind::Sharp),
    ("e387", "cambered low Reynolds, sharp", EdgeKind::Sharp),
];

fn config_for(name: &str) -> CfdStudyConfig {
    CfdStudyConfig {
        airfoil_name: name.to_owned(),
        ..CfdStudyConfig::default()
    }
}

fn snapshot(name: &str) -> AirfoilSnapshot {
    match resolve_airfoil(name) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("fixture {name}: {error}"),
    }
}

fn preflight(name: &str, config: &CfdStudyConfig) -> MeshPreflight {
    match run_mesh_preflight(config, &snapshot(name)) {
        Ok(bundle) => bundle.manifest,
        Err(error) => panic!("preflight {name}: {error}"),
    }
}

fn synthetic(name: &str, coordinates: Vec<(f64, f64)>) -> AirfoilSnapshot {
    // The audit recomputes the hash from name and bytes, so a synthetic
    // fixture needs the same hash the database resolver would produce.
    let hashed = resolve_hash(name, &coordinates);
    AirfoilSnapshot {
        name: name.to_owned(),
        coordinates,
        coordinate_hash: hashed,
    }
}

fn resolve_hash(name: &str, coordinates: &[(f64, f64)]) -> String {
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

/// A ten-point Selig-ordered symmetric section with a sharp trailing edge.
fn sharp_section() -> Vec<(f64, f64)> {
    vec![
        (1.0, 0.0),
        (0.75, 0.04),
        (0.5, 0.06),
        (0.25, 0.05),
        (0.1, 0.03),
        (0.0, 0.0),
        (0.1, -0.03),
        (0.25, -0.05),
        (0.5, -0.06),
        (0.75, -0.04),
    ]
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "alas-cfd-preflight-{label}-{}-{stamp}",
        std::process::id()
    ))
}

#[test]
fn every_fixture_preflights_ready_with_its_exact_identity() {
    for (name, family, trailing_edge) in FIXTURES {
        let source = snapshot(name);
        let config = config_for(name);
        let bundle = match run_mesh_preflight(&config, &source) {
            Ok(bundle) => bundle,
            Err(error) => panic!("{name} ({family}): {error}"),
        };
        let manifest = &bundle.manifest;
        assert_eq!(
            manifest.status,
            PreflightStatus::Ready,
            "{name} ({family}): {}",
            manifest.blocking_summary()
        );
        assert!(manifest.geometry.passed(), "{name} ({family})");
        assert_eq!(manifest.airfoil.name, source.name, "{name}");
        assert_eq!(
            manifest.airfoil.coordinate_hash, source.coordinate_hash,
            "{name}"
        );
        assert_eq!(
            manifest.airfoil.coordinate_count,
            source.coordinates.len(),
            "{name}"
        );
        let edge = manifest
            .geometry
            .trailing_edge
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no trailing edge"));
        assert_eq!(edge.kind, *trailing_edge, "{name} ({family})");
        let expected_meshing = match trailing_edge {
            EdgeKind::Sharp => TrailingEdgeMeshing::FanAtSharpEdge,
            EdgeKind::Blunt => TrailingEdgeMeshing::ResolvedBluntFace,
        };
        assert_eq!(edge.meshing, expected_meshing, "{name}");
        assert_eq!(
            manifest.geometry.winding,
            Some(Winding::CounterClockwise),
            "{name}"
        );
        assert_eq!(manifest.geometry.upper_surface_first, Some(true), "{name}");
        let geo = bundle
            .geo
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no GEO"));
        assert_eq!(
            manifest.gmsh.as_ref().map(|report| report.topology),
            Some(geo.report.topology),
            "{name}"
        );
        assert_eq!(
            manifest.geometry.topology,
            Some(geo.report.topology),
            "{name}"
        );
        assert!(
            manifest
                .gmsh_source_hash
                .as_deref()
                .is_some_and(|hash| hash.starts_with("fnv1a64-")),
            "{name}"
        );
        for patch in &manifest
            .domain
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no domain"))
            .patches
        {
            assert!(
                geo.source
                    .contains(&format!("Physical Surface(\"{}\")", patch.name)),
                "{name}: patch {}",
                patch.name
            );
        }
        assert_eq!(manifest.manifest_version, MESH_MANIFEST_VERSION);
        assert_eq!(manifest.case_template_version, TEMPLATE_VERSION);
        assert_eq!(manifest.gmsh_template_version, GMSH_TEMPLATE_VERSION);
        assert_eq!(manifest.domain_template_version, DOMAIN_TEMPLATE_VERSION);
        assert!(!manifest.toolchain.executed);
    }
}

#[test]
fn preflight_is_deterministic_and_round_trips_through_json() {
    for (name, _, _) in FIXTURES {
        let config = config_for(name);
        let first = preflight(name, &config);
        let second = preflight(name, &config);
        assert_eq!(first, second, "{name}");
        let json = first
            .to_json_pretty()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            json,
            second
                .to_json_pretty()
                .unwrap_or_else(|error| panic!("{name}: {error}"))
        );
        let parsed: MeshPreflight =
            serde_json::from_str(&json).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(parsed, first, "{name}");
        assert!(
            !json.contains("NaN") && !json.contains("null,\n"),
            "{name}: {json}"
        );
    }
}

#[test]
fn geometry_metrics_distinguish_thin_thick_and_cambered_fixtures() {
    let metric = |name: &str| {
        let manifest = preflight(name, &config_for(name));
        manifest
            .geometry
            .metrics
            .unwrap_or_else(|| panic!("{name}: no metrics"))
    };
    let thin = metric("naca0006");
    assert!(
        (thin.max_thickness_ratio - 0.06).abs() < 0.006,
        "{}",
        thin.max_thickness_ratio
    );
    assert!(
        thin.max_camber_ratio.abs() < 0.002,
        "{}",
        thin.max_camber_ratio
    );
    let thick = metric("naca0024");
    assert!(
        (thick.max_thickness_ratio - 0.24).abs() < 0.01,
        "{}",
        thick.max_thickness_ratio
    );
    let very_thick = metric("fx77w343");
    assert!(
        very_thick.max_thickness_ratio > 0.33,
        "{}",
        very_thick.max_thickness_ratio
    );
    let cambered = metric("s1223");
    assert!(
        cambered.max_camber_ratio > 0.08,
        "{}",
        cambered.max_camber_ratio
    );
    assert!(cambered.max_camber_x > 0.2 && cambered.max_camber_x < 0.8);
    for metrics in [&thin, &thick, &very_thick, &cambered] {
        assert!(metrics.leading_edge.0.abs() < 1.0e-3);
        assert!(
            (metrics.trailing_edge_x - 1.0).abs() < 2.0e-3,
            "{}",
            metrics.trailing_edge_x
        );
        assert!(metrics.perimeter_chord > 2.0 && metrics.perimeter_chord < 2.6);
        assert!(metrics.signed_area_chord2 > 0.0);
        assert!(metrics.min_segment_chord > 0.0);
    }
    let blunt = preflight("fx77w343", &config_for("fx77w343"));
    let edge = blunt
        .geometry
        .trailing_edge
        .unwrap_or_else(|| panic!("no edge"));
    assert!(
        edge.gap_chord > 0.04 && edge.gap_chord < 0.05,
        "{}",
        edge.gap_chord
    );
    assert_eq!(edge.gap_m, Some(edge.gap_chord * blunt.chord_m));
}

fn geo_assignment(source: &str, prefix: &str) -> f64 {
    source
        .lines()
        .find(|line| line.starts_with(prefix))
        .and_then(|line| line.split('=').nth(1))
        .and_then(|value| value.trim().trim_end_matches(';').parse::<f64>().ok())
        .unwrap_or_else(|| panic!("GEO assignment {prefix}"))
}

fn geo_point(source: &str, id: i32) -> (f64, f64, f64) {
    let line = source
        .lines()
        .find(|line| line.starts_with(&format!("Point({id}) =")))
        .unwrap_or_else(|| panic!("Point({id})"));
    let fields = line
        .split('{')
        .nth(1)
        .map(|value| value.trim_end_matches("};"))
        .unwrap_or_else(|| panic!("Point({id}) body"))
        .split(',')
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("Point({id}) field"))
        })
        .collect::<Vec<_>>();
    (fields[0], fields[1], fields[3])
}

#[test]
fn manifest_numbers_match_the_generated_gmsh_source() {
    let name = "SC2-0714";
    let mut config = config_for(name);
    config.chord_m = 0.35;
    config.mesh.preset = MeshPreset::Fine;
    config.mesh.wake_refinement = 3;
    let bundle =
        run_mesh_preflight(&config, &snapshot(name)).unwrap_or_else(|error| panic!("{error}"));
    let manifest = bundle.manifest;
    let source = bundle
        .geo
        .map(|geo| geo.source)
        .unwrap_or_else(|| panic!("no GEO"));
    let resolution = manifest
        .resolution
        .unwrap_or_else(|| panic!("no resolution"));
    let domain = manifest.domain.unwrap_or_else(|| panic!("no domain"));
    assert_eq!(
        geo_assignment(&source, "Mesh.CharacteristicLengthMax"),
        resolution.refinement.far_field_size_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[4].VIn"),
        resolution.refinement.wake_size_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[4].YMin"),
        domain.wake_box.min_y_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[4].YMax"),
        domain.wake_box.max_y_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[4].XMax"),
        domain.wake_box.max_x_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[1].Size"),
        resolution.inflation.first_layer_thickness_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[1].Thickness"),
        resolution.inflation.total_thickness_m
    );
    assert_eq!(
        geo_assignment(&source, "Field[1].NbLayers"),
        f64::from(resolution.inflation.n_layers)
    );
    assert_eq!(
        geo_point(&source, 1),
        (
            domain.extents.min_x_m,
            domain.extents.min_y_m,
            resolution.refinement.far_field_size_m
        )
    );
    assert_eq!(
        geo_point(&source, 3),
        (
            domain.extents.max_x_m,
            domain.extents.max_y_m,
            resolution.refinement.far_field_size_m
        )
    );
    let (x5, _, size5) = geo_point(&source, 5);
    assert_eq!(size5, resolution.refinement.surface_size_m);
    assert!(
        (x5 - config.chord_m).abs() < 1.0e-12,
        "first airfoil point is the trailing edge"
    );
    assert_eq!(
        domain.extents.span_m,
        geo_assignment(&source, "Field[4].ZMax")
    );
    assert_eq!(resolution.refinement.wake_refinement_level, 3);
    assert_eq!(resolution.refinement.leading_edge_refinement_level, 0);
    assert_eq!(
        resolution.refinement.leading_edge_size_m,
        resolution.refinement.surface_size_m
    );
    assert!(!source.contains("Field[6]"));
    assert!(source.contains("Field[5].FieldsList = {3, 4};"));
    assert_eq!(
        resolution
            .refinement
            .estimated_surface_segments
            .map(|n| n > 300),
        Some(true)
    );
}

#[test]
fn leading_edge_refinement_adds_a_threshold_field_only_when_requested() {
    let name = "naca0024";
    let mut config = config_for(name);
    config.mesh.leading_edge_refinement = 2;
    let bundle =
        run_mesh_preflight(&config, &snapshot(name)).unwrap_or_else(|error| panic!("{error}"));
    let source = bundle
        .geo
        .map(|geo| geo.source)
        .unwrap_or_else(|| panic!("no GEO"));
    let resolution = bundle
        .manifest
        .resolution
        .unwrap_or_else(|| panic!("no resolution"));
    assert_eq!(
        resolution.refinement.leading_edge_size_m,
        resolution.refinement.surface_size_m / 4.0
    );
    assert_eq!(
        geo_assignment(&source, "Field[7].SizeMin"),
        resolution.refinement.leading_edge_size_m
    );
    // The leading-edge field must relax to the FAR-FIELD size, not the surface
    // size.  `Field[5]` is `Min{3, 4, 7}` and a `Threshold` returns `SizeMax`
    // everywhere beyond `DistMax`, so relaxing only to the surface size clamps
    // the entire domain to it and the mesh explodes: measured at 2.6 GB of Gmsh
    // memory with no mesh after eight minutes, against ten seconds at level 0.
    assert_eq!(
        geo_assignment(&source, "Field[7].SizeMax"),
        resolution.refinement.far_field_size_m
    );
    assert!(
        resolution.refinement.far_field_size_m > resolution.refinement.surface_size_m,
        "the far-field size is the coarse one, so this assertion has teeth"
    );
    assert!(source.contains("Field[6] = Distance;"));
    assert!(source.contains("Field[5].FieldsList = {3, 4, 7};"));
    let le_point = source
        .lines()
        .find(|line| line.starts_with("Field[6].PointsList"))
        .and_then(|line| line.split(['{', '}']).nth(1))
        .and_then(|id| id.parse::<i32>().ok())
        .unwrap_or_else(|| panic!("leading-edge point id"));
    let (x, _, _) = geo_point(&source, le_point);
    assert!(
        x.abs() < 1.0e-9,
        "the refined point is the leading edge, x = {x}"
    );
    config.mesh.leading_edge_refinement = 0;
    let plain =
        run_mesh_preflight(&config, &snapshot(name)).unwrap_or_else(|error| panic!("{error}"));
    let plain_source = plain
        .geo
        .map(|geo| geo.source)
        .unwrap_or_else(|| panic!("no GEO"));
    assert!(!plain_source.contains("Field[6]"));
    assert_ne!(
        plain.manifest.gmsh_source_hash,
        bundle.manifest.gmsh_source_hash
    );
}

/// Every documented refinement level must emit a GEO Gmsh can actually mesh.
///
/// Levels 0..=6 were run through Gmsh 4.15.2 on this host after the `SizeMax`
/// fix: level 0 gives 193 302 nodes, level 2 194 360, level 4 198 370 in 10.6 s
/// with no errors, and levels 5 and 6 saturate at the chord clamp and give
/// 199 578 nodes in 12.3 s.  This test pins the two properties that make that
/// true — the far-field relaxation and the size floor — so the range stays
/// meshable; level 7 is rejected in preflight instead (see
/// `preflight_rejects_impossible_configurations`).
#[test]
fn every_documented_leading_edge_level_stays_meshable() {
    let name = "naca0024";
    let mut previous = f64::INFINITY;
    for level in 0..=6_u32 {
        let mut config = config_for(name);
        config.mesh.leading_edge_refinement = level;
        config
            .validate()
            .unwrap_or_else(|errors| panic!("level {level}: {errors:?}"));
        let bundle =
            run_mesh_preflight(&config, &snapshot(name)).unwrap_or_else(|error| panic!("{error}"));
        let resolution = bundle
            .manifest
            .resolution
            .unwrap_or_else(|| panic!("no resolution"));
        let le = resolution.refinement.leading_edge_size_m;
        // The floor is what keeps levels 5 and 6 from asking for a cell Gmsh
        // cannot fill: at 5e-4 chords they are the same mesh, not a finer one.
        assert!(
            le >= 5.0e-4 * config.chord_m * (1.0 - 1.0e-12),
            "level {level}: leading-edge size {le:.6e} m is below the chord floor"
        );
        assert!(le <= previous, "level {level}: size must not grow");
        previous = le;
        let Some(geo) = bundle.geo else {
            continue;
        };
        if level == 0 {
            assert!(!geo.source.contains("Field[7]"));
            continue;
        }
        assert_eq!(geo_assignment(&geo.source, "Field[7].SizeMin"), le);
        // Without this the `Min` field clamps the whole 30 m x 20 m domain to
        // the surface size and Gmsh never returns.
        assert_eq!(
            geo_assignment(&geo.source, "Field[7].SizeMax"),
            resolution.refinement.far_field_size_m
        );
    }
    // Levels 5 and 6 both sit on the floor, so the clamp is exercised above.
    assert!(previous > 0.0);
}

fn blocked_codes(name: &str, coordinates: Vec<(f64, f64)>) -> (MeshPreflight, Vec<String>) {
    let manifest =
        match run_mesh_preflight(&CfdStudyConfig::default(), &synthetic(name, coordinates)) {
            Ok(bundle) => {
                assert!(
                    bundle.geo.is_none(),
                    "{name}: a blocked preflight must not build a GEO"
                );
                bundle.manifest
            }
            Err(error) => panic!("{name}: {error}"),
        };
    assert_eq!(manifest.status, PreflightStatus::Blocked, "{name}");
    assert_eq!(
        manifest.airfoil.name, name,
        "{name}: identity must be preserved, never substituted"
    );
    let codes = manifest
        .errors()
        .map(|issue| issue.code.clone())
        .collect::<Vec<_>>();
    (manifest, codes)
}

#[test]
fn invalid_geometry_is_reported_with_actionable_codes_and_never_substituted() {
    let mut crossing = sharp_section();
    crossing.swap(2, 8);
    let (manifest, codes) = blocked_codes("crossing", crossing);
    assert!(
        codes.iter().any(|code| code == "SelfIntersection"),
        "{codes:?}"
    );
    assert!(manifest.errors().all(|issue| !issue.remedy.is_empty()));

    let mut nan = sharp_section();
    nan[3].1 = f64::NAN;
    let (manifest, codes) = blocked_codes("nan", nan);
    assert_eq!(codes, vec!["NonFinite".to_owned()]);
    assert_eq!(manifest.geometry.issues[0].indices, vec![3]);

    let mut duplicate = sharp_section();
    duplicate[4] = duplicate[3];
    let (_, codes) = blocked_codes("duplicate", duplicate);
    assert!(
        codes.iter().any(|code| code == "DuplicatePoint"),
        "{codes:?}"
    );

    let mut rotated = sharp_section();
    rotated.rotate_left(5);
    let (_, codes) = blocked_codes("leading-edge-first", rotated);
    assert!(
        codes.iter().any(|code| code == "NotSingleLoop"),
        "{codes:?}"
    );

    let (_, codes) = blocked_codes("too-few", sharp_section()[..6].to_vec());
    assert_eq!(codes, vec!["TooFewPoints".to_owned()]);

    let mut wide_base = sharp_section();
    wide_base[0].1 = 0.08;
    wide_base.push((1.0, -0.08));
    let (manifest, codes) = blocked_codes("wide-base", wide_base);
    assert!(
        codes.iter().any(|code| code == "TrailingEdgeGapTooLarge"),
        "{codes:?}"
    );
    assert_eq!(
        manifest.geometry.trailing_edge.map(|edge| edge.kind),
        Some(EdgeKind::Blunt)
    );

    let plate = sharp_section()
        .into_iter()
        .map(|(x, y)| (x, y * 0.05))
        .collect();
    let (_, codes) = blocked_codes("plate", plate);
    assert!(
        codes.iter().any(|code| code == "ThicknessOutOfRange"),
        "{codes:?}"
    );

    let unnormalised = sharp_section()
        .into_iter()
        .map(|(x, y)| (x * 2.0, y * 2.0))
        .collect();
    let (_, codes) = blocked_codes("unnormalised", unnormalised);
    assert!(
        codes.iter().any(|code| code == "NotNormalized"),
        "{codes:?}"
    );

    let mut tampered = synthetic("tampered", sharp_section());
    tampered.coordinates[2].1 += 0.001;
    let bundle = run_mesh_preflight(&CfdStudyConfig::default(), &tampered)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(bundle.manifest.status, PreflightStatus::Blocked);
    assert!(bundle
        .manifest
        .errors()
        .any(|issue| issue.code == "HashMismatch"));
}

#[test]
fn closure_and_surface_order_are_classified_without_blocking() {
    let mut closed = sharp_section();
    closed.push(closed[0]);
    let bundle = run_mesh_preflight(&CfdStudyConfig::default(), &synthetic("closed", closed))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        bundle.manifest.status,
        PreflightStatus::Ready,
        "{}",
        bundle.manifest.blocking_summary()
    );
    assert_eq!(
        bundle.manifest.geometry.closure,
        ClosureKind::RepeatedFirstPoint
    );
    assert_eq!(bundle.manifest.geometry.loop_point_count, 10);
    assert_eq!(bundle.manifest.airfoil.coordinate_count, 11);
    assert_eq!(
        bundle
            .manifest
            .gmsh
            .map(|report| report.removed_closing_duplicate),
        Some(true)
    );

    let mut lower_first = sharp_section();
    lower_first[1..].reverse();
    let bundle = run_mesh_preflight(
        &CfdStudyConfig::default(),
        &synthetic("lower-first", lower_first),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        bundle.manifest.status,
        PreflightStatus::Ready,
        "{}",
        bundle.manifest.blocking_summary()
    );
    assert_eq!(bundle.manifest.geometry.winding, Some(Winding::Clockwise));
    assert_eq!(bundle.manifest.geometry.upper_surface_first, Some(false));
    assert!(
        bundle
            .manifest
            .issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Warning
                && issue.code == "LowerSurfaceFirst")
    );
    let audit = audit_airfoil_geometry(
        &synthetic("lower-first-audit", {
            let mut points = sharp_section();
            points[1..].reverse();
            points
        }),
        1.0,
    );
    assert!(audit.passed());
    assert!(audit
        .issues
        .iter()
        .any(|issue| issue.code == GeometryIssueCode::LowerSurfaceFirst));
}

#[test]
fn preset_catalogue_and_patch_contract_are_explicit_and_ordered() {
    let catalogue = preset_catalogue();
    assert_eq!(
        catalogue.iter().map(|row| row.preset).collect::<Vec<_>>(),
        vec![MeshPreset::Coarse, MeshPreset::Medium, MeshPreset::Fine]
    );
    for pair in catalogue.windows(2) {
        assert!(pair[1].far_field_fraction_of_chord < pair[0].far_field_fraction_of_chord);
        assert!(pair[1].surface_fraction_of_chord < pair[0].surface_fraction_of_chord);
        assert!(pair[1].wake_fraction_of_chord <= pair[0].wake_fraction_of_chord);
        assert!(pair[1].base_cells.0 > pair[0].base_cells.0);
        assert!(pair[1].refinement_level > pair[0].refinement_level);
    }
    let patches = patch_contract();
    assert_eq!(patches.len(), 5);
    let required = required_boundary_types();
    for patch in &patches {
        assert_eq!(
            required.get(&patch.name),
            Some(&patch.openfoam_type),
            "{}",
            patch.name
        );
    }
    let roles = patches.iter().map(|patch| patch.role).collect::<Vec<_>>();
    for role in [
        PatchRole::Airfoil,
        PatchRole::Inlet,
        PatchRole::Outlet,
        PatchRole::FarField,
        PatchRole::FrontAndBack,
    ] {
        assert_eq!(
            roles.iter().filter(|candidate| **candidate == role).count(),
            1,
            "{role:?}"
        );
    }
    assert_eq!(
        patches
            .iter()
            .find(|patch| patch.role == PatchRole::Airfoil)
            .map(|patch| patch.openfoam_type.as_str()),
        Some("wall")
    );
    assert_eq!(
        patches
            .iter()
            .find(|patch| patch.role == PatchRole::FrontAndBack)
            .map(|patch| patch.openfoam_type.as_str()),
        Some("empty")
    );
}

#[test]
fn preset_and_domain_controls_are_validated_with_units() {
    let mut config = CfdStudyConfig::default();
    config.mesh.upstream_chords = 1.0;
    assert!(
        matches!(run_mesh_preflight(&config, &snapshot("e387")), Err(MeshError::InvalidInput(message)) if message.contains("Domain extents"))
    );
    assert!(domain_template(&config, None).is_err());
    let mut config = CfdStudyConfig::default();
    config.mesh.leading_edge_refinement = 7;
    assert!(
        matches!(mesh_resolution(&config, None), Err(MeshError::InvalidInput(message)) if message.contains("leading-edge refinement"))
    );
    assert!(config.validate().is_err());
    let mut config = CfdStudyConfig::default();
    config.mesh.n_layers = 0;
    assert!(
        matches!(mesh_resolution(&config, None), Err(MeshError::InvalidInput(message)) if message.contains("boundary-layer count"))
    );
    let mut config = CfdStudyConfig::default();
    config.mesh.first_layer_height_m = -1.0e-5;
    assert!(mesh_resolution(&config, None).is_err());

    let config = CfdStudyConfig::default();
    let resolution = mesh_resolution(&config, Some(2.05)).unwrap_or_else(|error| panic!("{error}"));
    // The default derives the wall distance from the y+ target, so the
    // reported distance is the derived one and the emitted first-layer
    // thickness is twice it under the cell-centre interpretation.
    assert!(config.mesh.derive_first_layer_from_target_y_plus);
    assert_eq!(
        resolution.inflation.first_layer_thickness_m,
        2.0 * resolution.inflation.wall_distance_m
    );
    assert_eq!(
        resolution.inflation.wall_distance_m,
        resolution.inflation.wall_distance_for_target_m
    );
    assert_ne!(
        resolution.inflation.wall_distance_m,
        config.mesh.first_layer_height_m
    );

    // With the override the literal configured length is used verbatim.
    let mut literal = CfdStudyConfig::default();
    literal.mesh.derive_first_layer_from_target_y_plus = false;
    let literal_resolution =
        mesh_resolution(&literal, Some(2.05)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        literal_resolution.inflation.wall_distance_m,
        literal.mesh.first_layer_height_m
    );
    assert_eq!(
        literal_resolution.inflation.first_layer_thickness_m,
        2.0 * literal.mesh.first_layer_height_m
    );
    assert!(
        (resolution.inflation.y_plus_ratio
            - resolution.inflation.estimated_y_plus / config.mesh.target_y_plus)
            .abs()
            < 1.0e-12
    );
    let expected = if resolution.inflation.y_plus_ratio > 2.0 {
        YPlusConsistency::TooCoarse
    } else if resolution.inflation.y_plus_ratio < 0.5 {
        YPlusConsistency::TooFine
    } else {
        YPlusConsistency::Consistent
    };
    assert_eq!(resolution.inflation.consistency, expected);
    assert_eq!(
        resolution.refinement.estimated_surface_segments,
        Some((2.05 * config.chord_m / resolution.refinement.surface_size_m).round() as u64)
    );

    // A too-coarse first cell is only reachable through the explicit override
    // now; the derived path cannot disagree with its own target.
    let mut coarse_wall = CfdStudyConfig::default();
    coarse_wall.mesh.derive_first_layer_from_target_y_plus = false;
    coarse_wall.mesh.first_layer_height_m = 2.0e-3;
    let resolution = mesh_resolution(&coarse_wall, None).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        resolution.inflation.consistency,
        YPlusConsistency::TooCoarse
    );
    assert!(resolution
        .warnings
        .iter()
        .any(|warning| warning.contains("would meet the target")));
    let mut no_layers = CfdStudyConfig::default();
    no_layers.mesh.boundary_layers = false;
    let resolution = mesh_resolution(&no_layers, None).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        resolution.inflation.consistency,
        YPlusConsistency::NotEnforced
    );
    assert_eq!(resolution.inflation.n_layers, 0);
    assert_eq!(resolution.inflation.total_thickness_m, 0.0);

    let small = CfdStudyConfig {
        chord_m: 2.0,
        mesh: MeshSettings {
            upstream_chords: 5.0,
            half_height_chords: 4.0,
            ..MeshSettings::default()
        },
        ..CfdStudyConfig::default()
    };
    let domain = domain_template(&small, Some(0.24)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(domain.extents.min_x_m, -10.0);
    assert_eq!(domain.extents.height_m, 16.0);
    assert_eq!(domain.extents.span_m, 0.02);
    assert_eq!(domain.blockage_ratio, Some(0.24 * 2.0 / 16.0));
    assert!(domain
        .warnings
        .iter()
        .any(|warning| warning.starts_with("upstream")));
    assert!(domain
        .warnings
        .iter()
        .any(|warning| warning.starts_with("half-height")));
    assert!(domain
        .warnings
        .iter()
        .any(|warning| warning.starts_with("blockage")));
    assert!(domain_template(&CfdStudyConfig::default(), Some(0.14))
        .map(|domain| domain.warnings.is_empty())
        .unwrap_or(false));
}

#[test]
fn generate_case_writes_the_manifest_and_blocked_manifests_still_serialise() {
    let dir = temp_dir("case");
    let config = CfdStudyConfig {
        mesh: MeshSettings {
            preset: MeshPreset::Coarse,
            ..MeshSettings::default()
        },
        ..CfdStudyConfig::default()
    };
    let generated = generate_case(&config, &dir).unwrap_or_else(|error| panic!("{error}"));
    let path = dir.join(MESH_MANIFEST_FILE);
    assert!(
        generated.files.iter().any(|file| file == &path),
        "{:?}",
        generated.files
    );
    let written: MeshPreflight = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(written.status, PreflightStatus::Ready);
    assert_eq!(
        written.airfoil.coordinate_hash,
        generated.airfoil.coordinate_hash
    );
    assert_eq!(written.gmsh_template_version, GMSH_TEMPLATE_VERSION);
    let geo = std::fs::read_to_string(dir.join("system/airfoil.geo"))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(geo.contains(&format!("template = {GMSH_TEMPLATE_VERSION}")));
    let _ = std::fs::remove_dir_all(&dir);

    let mut crossing = sharp_section();
    crossing.swap(2, 8);
    let bundle = run_mesh_preflight(&CfdStudyConfig::default(), &synthetic("crossing", crossing))
        .unwrap_or_else(|error| panic!("{error}"));
    let dir = temp_dir("blocked");
    let path =
        write_mesh_manifest(&dir, &bundle.manifest).unwrap_or_else(|error| panic!("{error}"));
    let written: MeshPreflight = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(written.status, PreflightStatus::Blocked);
    assert!(written.gmsh.is_none() && written.gmsh_source_hash.is_none());
    assert!(!written.blocking_summary().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
