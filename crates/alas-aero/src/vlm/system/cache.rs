// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact reuse across related lattices, including a rotating stabilizer.
//!
//! A changed panel invalidates its entire AIC row and column and its entire
//! near-field row and column. Every other entry uses the identical geometry
//! and is retained. The complete updated AIC still uses the ordinary LU and
//! its original conditioning gate. No incidence is rounded or linearized.

use std::sync::Arc;

use super::{
    build_kernel_cache, inter_surface_core, mesh_wing_panels, Panel, VlmError, VlmSystem,
    TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS,
};
use crate::singularities::calculate_induced_velocity_horseshoe_cored;
use crate::vector3::dot3;
use alas_geom::aircraft::airplane::Airplane;
use alas_math::linalg::{DenseMatrix, LuFactorization};

/// Geometry-only work retained within one candidate evaluation.
///
/// Panel equality is exact, including normals, horseshoe vertices and core
/// radii. Wake reuse separately checks actual strip leading-edge coordinates
/// and panel grouping: imported sections need not have zero leading-edge
/// camber. A change of mesh or unrelated geometry safely invalidates entries.
#[derive(Debug, Default)]
pub struct VlmGeometryCache {
    wings: Vec<alas_geom::aircraft::wing::Wing>,
    resolution: (usize, usize),
    panels: Vec<Panel>,
    aic: Vec<f64>,
    factorization: Option<Arc<LuFactorization>>,
    kernel: Option<Arc<Vec<[f64; 3]>>>,
    trace: Vec<(usize, usize, [u64; 4])>,
    trefftz: Arc<std::sync::OnceLock<Option<super::super::trefftz::TrefftzOperator>>>,
}

impl VlmGeometryCache {
    pub(super) fn assemble<'a>(
        &mut self,
        airplane: &'a Airplane,
        spanwise: usize,
        chordwise: usize,
    ) -> Result<VlmSystem<'a>, VlmError> {
        let mut panels = Vec::with_capacity(self.panels.len());
        for (index, wing) in airplane.wings.iter().enumerate() {
            if self.resolution == (spanwise, chordwise) && self.wings.get(index) == Some(wing) {
                panels.extend(
                    self.panels
                        .iter()
                        .filter(|panel| panel.wing_index == index)
                        .cloned(),
                );
            } else {
                mesh_wing_panels(wing, index, spanwise, chordwise, &mut panels)?;
            }
        }
        let n = panels.len();
        let same_size = n == self.panels.len() && self.factorization.is_some();
        let changed: Vec<bool> = panels
            .iter()
            .enumerate()
            .map(|(i, panel)| !same_size || *panel != self.panels[i])
            .collect();
        if !same_size {
            self.aic.resize(n * n, 0.0);
        }
        if changed.iter().any(|&change| change) || self.factorization.is_none() {
            for (i, field) in panels.iter().enumerate() {
                for (j, source) in panels.iter().enumerate() {
                    if changed[i] || changed[j] {
                        let induced = calculate_induced_velocity_horseshoe_cored(
                            field.collocation_point,
                            source.left_vortex_vertex,
                            source.right_vortex_vertex,
                            TRAILING_VORTEX_DIRECTION,
                            1.0,
                            VORTEX_CORE_RADIUS,
                            inter_surface_core(field.wing_index, source),
                        );
                        self.aic[i * n + j] = dot3(induced, field.normal_direction);
                    }
                }
            }
            let factorization = match DenseMatrix::from_row_major(n, &self.aic).factor() {
                Ok(factorization) => factorization,
                Err(pivot) => {
                    // The AIC has already been updated; discard it rather
                    // than leave entries paired with the previous mesh.
                    *self = Self::default();
                    return Err(VlmError::SingularAic(pivot));
                }
            };
            self.factorization = Some(Arc::new(factorization));
            if same_size && self.kernel.is_some() {
                if let Some(kernel) = self.kernel.as_mut() {
                    let entries = Arc::make_mut(kernel);
                    for (i, field) in panels.iter().enumerate() {
                        for (j, source) in panels.iter().enumerate() {
                            if changed[i] || changed[j] {
                                entries[i * n + j] = calculate_induced_velocity_horseshoe_cored(
                                    field.vortex_center,
                                    source.left_vortex_vertex,
                                    source.right_vortex_vertex,
                                    TRAILING_VORTEX_DIRECTION,
                                    4.0 * std::f64::consts::PI,
                                    VORTEX_CORE_RADIUS,
                                    inter_surface_core(field.wing_index, source),
                                );
                            }
                        }
                    }
                }
            } else {
                let centers: Vec<_> = panels.iter().map(|panel| panel.vortex_center).collect();
                self.kernel = build_kernel_cache(&centers, &panels).map(Arc::new);
            }
        }
        let mut start = 0;
        let trace: Vec<_> = panels
            .iter()
            .enumerate()
            .filter_map(|(i, panel)| {
                if !panel.is_trailing_edge {
                    return None;
                }
                let leading = &panels[start];
                let entry = (
                    i + 1,
                    panel.wing_index,
                    [
                        leading.front_left[1].to_bits(),
                        leading.front_left[2].to_bits(),
                        leading.front_right[1].to_bits(),
                        leading.front_right[2].to_bits(),
                    ],
                );
                start = i + 1;
                Some(entry)
            })
            .collect();
        if trace != self.trace {
            self.trefftz = Arc::default();
            self.trace = trace;
        }
        self.panels.clone_from(&panels);
        if self.wings.len() != airplane.wings.len() {
            self.wings.clone_from(&airplane.wings);
        } else {
            for (old, wing) in self.wings.iter_mut().zip(&airplane.wings) {
                if old != wing {
                    old.clone_from(wing);
                }
            }
        }
        self.resolution = (spanwise, chordwise);
        // The factorization is installed above even for an empty lattice.
        let factorization = self
            .factorization
            .as_ref()
            .ok_or(VlmError::NonFiniteResult)?
            .clone();
        Ok(VlmSystem {
            airplane,
            panels,
            factorization,
            kernel_cache: std::sync::OnceLock::from(self.kernel.clone()),
            trefftz: self.trefftz.clone(),
            solves: std::sync::atomic::AtomicUsize::new(1),
        })
    }
}

// Tests construct valid lattices and assert on their solves; a failed unwrap
// is a test assertion rather than a production recovery path.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::operating_point::OperatingPoint;
    use alas_atmo::Atmosphere;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn airplane() -> Airplane {
        let section =
            |xyz, chord| WingXSec::new(xyz, chord, 0.0, Airfoil::from_name("naca0012").unwrap());
        let wing = Wing::new(
            "Main Wing",
            vec![section([0.0; 3], 2.0), section([1.0, 6.0, 0.5], 1.0)],
            true,
        );
        let tail = Wing::new(
            "Horizontal Stabilizer",
            vec![section([8.0, 0.0, 0.5], 1.0), section([8.5, 2.0, 0.7], 0.7)],
            true,
        );
        Airplane {
            name: "Cache test".into(),
            xyz_ref: [0.5, 0.0, 0.0],
            s_ref: wing.reference_area(),
            b_ref: wing.reference_span(),
            c_ref: wing.mean_aerodynamic_chord(),
            wings: vec![wing, tail],
            fuselages: vec![],
        }
    }

    #[test]
    fn changing_incidence_cg_geometry_and_mesh_matches_fresh_solve_bitwise() {
        let mut plane = airplane();
        let mut cache = VlmGeometryCache::default();
        for (incidence, alpha, chordwise, tip_delta) in [
            (-4.0, -1.0, 4, 0.0),
            (2.0, 6.0, 4, 0.0),
            (2.0, 3.0, 4, 0.0),
            (0.5, 1.0, 4, 0.2),
            (1.0, 4.0, 8, 0.0),
        ] {
            for section in &mut plane.wings[1].xsecs {
                section.twist = incidence;
            }
            plane.xyz_ref[0] += 0.01;
            plane.wings[0].xsecs[1].xyz_le[1] += tip_delta;
            let point =
                OperatingPoint::new(Atmosphere::new(9000.0), 180.0, alpha, 0.0, 0.0, 0.0, 0.0);
            let fresh = VlmSystem::assemble(&plane, 1, chordwise).unwrap();
            let reused = VlmSystem::assemble_cached(&plane, 1, chordwise, &mut cache).unwrap();
            let expected = fresh.solve(&point).unwrap();
            let actual = reused.solve(&point).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                reused.trefftz_induced_drag_coefficient(&actual, &point),
                fresh.trefftz_induced_drag_coefficient(&expected, &point)
            );
        }
    }

    #[test]
    fn identical_mesh_reuses_lu_and_incidence_only_reuses_checked_wake() {
        let plane = airplane();
        let mut cache = VlmGeometryCache::default();
        let old = VlmSystem::assemble_cached(&plane, 1, 4, &mut cache).unwrap();
        let again = VlmSystem::assemble_cached(&plane, 1, 4, &mut cache).unwrap();
        assert!(Arc::ptr_eq(&old.factorization, &again.factorization));
        drop(again);
        let mut rotated_plane = plane.clone();
        rotated_plane.wings[1]
            .xsecs
            .iter_mut()
            .for_each(|section| section.twist = 3.0);
        let rotated = VlmSystem::assemble_cached(&rotated_plane, 1, 4, &mut cache).unwrap();
        assert!(!Arc::ptr_eq(&old.factorization, &rotated.factorization));
        assert!(Arc::ptr_eq(&old.trefftz, &rotated.trefftz));
        drop(rotated);
        rotated_plane.wings[1].xsecs[1].xyz_le[2] += 0.1;
        let moved = VlmSystem::assemble_cached(&rotated_plane, 1, 4, &mut cache).unwrap();
        assert!(!Arc::ptr_eq(&old.trefftz, &moved.trefftz));
    }

    #[test]
    fn frozen_matrix_normalwash_fails_the_tight_induced_drag_bound() {
        let mut plane = airplane();
        plane.wings.truncate(1);
        plane.wings[0].symmetric = false;
        plane.wings[0].xsecs[0].xyz_le = [0.0; 3];
        plane.wings[0].xsecs[1].xyz_le = [0.0, 10.0, 0.0];
        for section in &mut plane.wings[0].xsecs {
            section.chord = 1.0;
        }
        let fixed = VlmSystem::assemble(&plane, 1, 1).unwrap();
        let mut rotated = plane.clone();
        for section in &mut rotated.wings[0].xsecs {
            section.twist = 0.5;
        }
        let full = VlmSystem::assemble(&rotated, 1, 1).unwrap();
        let point = OperatingPoint::new(Atmosphere::new(0.0), 50.0, 5.0, 0.0, 0.0, 0.0, 0.0);
        let expected = full.solve(&point).unwrap();
        let onset = point.freestream_velocity_geometry_axes();
        let rhs: Vec<_> = full
            .panels
            .iter()
            .map(|panel| -dot3(onset, panel.normal_direction))
            .collect();
        let mut frozen = expected.clone();
        frozen.vortex_strengths = fixed.factorization.solve_vector(&rhs).0;
        let expected_cd = full
            .trefftz_induced_drag_coefficient(&expected, &point)
            .unwrap();
        let frozen_cd = full
            .trefftz_induced_drag_coefficient(&frozen, &point)
            .unwrap();
        let error = (frozen_cd / expected_cd - 1.0).abs();
        assert!(error > 1.0e-6, "frozen AIC relative CDi error {error:e}");
        assert!((error - 8.4149e-6).abs() < 1.0e-9);
    }
}
