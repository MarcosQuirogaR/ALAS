// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// SPDX-License-Identifier: AGPL-3.0-or-later

use super::{SurfaceDistribution, SurfaceError, SurfaceOrder, SurfaceReference};
use std::fs;
use std::path::Path;

#[path = "surface_native_geometry.rs"]
mod geometry;
#[cfg(test)]
pub(crate) use geometry::integrate;
#[path = "surface_native_mesh.rs"]
mod mesh;
#[path = "surface_native_raw.rs"]
mod raw;

pub(crate) fn io(path: &Path, error: std::io::Error) -> SurfaceError {
    SurfaceError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

pub(crate) fn error(source: &str, message: impl Into<String>) -> SurfaceError {
    SurfaceError::Parse {
        source: source.to_owned(),
        message: message.into(),
    }
}

fn read(path: &Path) -> Result<String, SurfaceError> {
    fs::read_to_string(path).map_err(|e| io(path, e))
}

pub(crate) fn parse_case(
    case: &Path,
    time: &str,
    reference: &SurfaceReference,
) -> Result<SurfaceDistribution, SurfaceError> {
    let mesh_dir = case.join("constant/polyMesh");
    let patch = mesh::boundary(&read(&mesh_dir.join("boundary"))?, "airfoil")?;
    let points = mesh::points(&read(&mesh_dir.join("points"))?, "points")?;
    let faces = mesh::faces(&read(&mesh_dir.join("faces"))?, "faces")?;
    let owners = mesh::labels(&read(&mesh_dir.join("owner"))?, "owner")?;
    let end = patch
        .start
        .checked_add(patch.count)
        .ok_or_else(|| SurfaceError::Mismatch("airfoil face range overflows".to_owned()))?;
    if end > faces.len() || end > owners.len() {
        return Err(SurfaceError::Mismatch(
            "airfoil patch exceeds face/owner lists".to_owned(),
        ));
    }

    let time_dir = case.join(time);
    let pressure_path = time_dir.join("p");
    let shear_path = time_dir.join("wallShearStress");
    let (pressure, pressure_units) = mesh::pressure_field(&read(&pressure_path)?, "p")?;
    let (shear, shear_units) = mesh::wall_shear_field(&read(&shear_path)?, "wallShearStress")?;
    let pressures = mesh::scalar_patch(&pressure, &owners, patch.start, patch.count)?;
    let shears = mesh::vector_patch(&shear, &owners, patch.start, patch.count)?;

    let mut source_samples = Vec::with_capacity(patch.count);
    for local in 0..patch.count {
        let global = patch.start + local;
        let face = geometry::face(&points, &faces[global])?;
        // `SurfaceSample` stores the kinematic pressure convention used by
        // the incompressible parser.  Convert rhoSimpleFoam's absolute Pa
        // field by density before handing it to the shared integrator; the
        // pressure reference is then subtracted exactly once there.
        let pressure_kinematic = match pressure_units {
            mesh::PressureUnits::Kinematic => pressures[local],
            mesh::PressureUnits::Absolute => pressures[local] / reference.density_kg_m3,
        };
        let shear_kinematic = match shear_units {
            mesh::PressureUnits::Kinematic => shears[local],
            mesh::PressureUnits::Absolute => {
                let value = shears[local];
                [
                    value[0] / reference.density_kg_m3,
                    value[1] / reference.density_kg_m3,
                    value[2] / reference.density_kg_m3,
                ]
            }
        };
        source_samples.push(geometry::sample(
            local,
            global,
            face,
            pressure_kinematic,
            shear_kinematic,
            reference,
        ));
    }

    let (indices, arcs, topology) = geometry::order_faces(&source_samples, &faces, patch.start);
    if indices.len() != source_samples.len() {
        return Err(SurfaceError::Mismatch(
            "airfoil wall traversal did not include every face".to_owned(),
        ));
    }
    // The public samples are traversal ordered.  Keep the original patch/global
    // indices on each row so consumers can trace every value to OpenFOAM.
    let mut samples = Vec::with_capacity(source_samples.len());
    for (rank, index) in indices.iter().enumerate() {
        let mut sample = source_samples[*index].clone();
        sample.arc_length_m = arcs[rank];
        samples.push(sample);
    }
    let forces = geometry::integrate(&samples, reference)?;
    let time_value = time
        .parse()
        .map_err(|_| SurfaceError::InvalidInput("time must be numeric".to_owned()))?;
    Ok(SurfaceDistribution {
        time: time_value,
        patch_name: "airfoil".to_owned(),
        pressure_field: pressure_path.display().to_string(),
        wall_shear_field: shear_path.display().to_string(),
        order: if topology {
            SurfaceOrder::MeshTopology
        } else {
            SurfaceOrder::GeometricNearestNeighbour
        },
        samples,
        forces,
    })
}

pub(crate) fn raw_rows(
    text: &str,
    components: usize,
) -> Result<(Vec<([f64; 3], Vec<f64>)>, Option<usize>), SurfaceError> {
    raw::raw_rows(text, components)
}

pub(crate) fn count_check(
    actual: usize,
    declared: Option<usize>,
    source: &str,
) -> Result<(), SurfaceError> {
    raw::count_check(actual, declared, source)
}

pub(crate) fn parse_sampled(
    pressure_text: &str,
    shear_text: &str,
    reference: &SurfaceReference,
) -> Result<super::SampledSurfaceDistribution, SurfaceError> {
    raw::parse_sampled(pressure_text, shear_text, reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn field_parser_rejects_binary_or_dimensional_pressure() {
        let binary = r#"
            FoamFile { format binary; }
            dimensions [0 2 -2 0 0 0 0];
            internalField uniform 0;
        "#;
        assert!(mesh::field(binary, false, "p").is_err());

        let dimensional = r#"
            FoamFile { format ascii; }
            dimensions [1 -1 -2 0 0 0 0];
            internalField uniform 0;
        "#;
        assert!(mesh::field(dimensional, false, "p").is_err());
    }

    #[test]
    fn compressible_pressure_field_is_converted_from_absolute_pa() {
        let dimensional = r#"
            FoamFile { format ascii; }
            dimensions [1 -1 -2 0 0 0 0];
            internalField uniform 101325;
            boundaryField
            {
                airfoil { type zeroGradient; value uniform 101325; }
            }
        "#;
        let (field, units) = mesh::pressure_field(dimensional, "p").expect("absolute p");
        assert_eq!(units, mesh::PressureUnits::Absolute);
        let pressure = mesh::scalar_patch(&field, &[0], 0, 1).expect("wall p");
        let shear_text = r#"
            FoamFile { format ascii; }
            dimensions [1 -1 -2 0 0 0 0];
            internalField uniform (4 5 6);
            boundaryField
            {
                airfoil { type calculated; value uniform (4 5 6); }
            }
        "#;
        let (_, shear_units) =
            mesh::wall_shear_field(shear_text, "wallShearStress").expect("absolute shear");
        assert_eq!(shear_units, mesh::PressureUnits::Absolute);
        let reference = SurfaceReference {
            density_kg_m3: 1.225,
            pressure_reference_pa: 101325.,
            ..SurfaceReference::for_two_dimensional_chord(1., 1.)
        };
        let face = geometry::face(
            &[[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
            &[0, 1, 2, 3],
        )
        .expect("valid wall face");
        let sample = geometry::sample(
            0,
            0,
            face,
            pressure[0] / reference.density_kg_m3,
            [0.; 3],
            &reference,
        );
        assert!((sample.pressure_pa - 101325.).abs() < 1.0e-9);
        assert!(sample.cp.abs() < 1.0e-12);
    }

    #[test]
    fn shuffled_patch_faces_are_recovered_from_mesh_edges() {
        let points = vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [1., 0., 1.],
            [1., 1., 1.],
            [0., 1., 1.],
        ];
        // The four lateral faces are deliberately listed in a non-geometric
        // order, as gmshToFoam is free to order boundary faces this way.
        let faces = vec![
            vec![2, 3, 7, 6],
            vec![0, 1, 5, 4],
            vec![3, 0, 4, 7],
            vec![1, 2, 6, 5],
        ];
        let reference = SurfaceReference::for_two_dimensional_chord(1., 1.);
        let source = faces
            .iter()
            .enumerate()
            .map(|(i, face)| {
                geometry::sample(
                    i,
                    i,
                    geometry::face(&points, face).expect("valid lateral face"),
                    0.,
                    [0.; 3],
                    &reference,
                )
            })
            .collect::<Vec<_>>();
        let (indices, arcs, topology) = geometry::order_faces(&source, &faces, 0);
        assert!(topology);
        assert_eq!(indices.len(), source.len());
        assert_eq!(arcs.len(), source.len());
        assert!(arcs.windows(2).all(|pair| pair[1] > pair[0]));

        let ordered = indices
            .iter()
            .map(|i| source[*i].global_face_index)
            .collect::<Vec<_>>();
        assert_ne!(ordered, vec![0, 1, 2, 3]);
        assert_eq!(
            ordered.iter().copied().collect::<HashSet<_>>(),
            [0_usize, 1, 2, 3].into_iter().collect()
        );
    }

    #[test]
    fn fluid_wall_shear_uses_negative_skin_friction_sign() {
        let points = vec![[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]];
        let reference = SurfaceReference::for_two_dimensional_chord(1., 1.);
        let sample = geometry::sample(
            0,
            0,
            geometry::face(&points, &[0, 1, 2, 3]).expect("valid face"),
            0.,
            [1., 0., 0.],
            &reference,
        );
        assert!(sample.cf < 0.);
        assert!(sample.cf_magnitude > 0.);
    }
}
