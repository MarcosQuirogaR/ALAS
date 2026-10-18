// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact reuse across related lattices, including a rotating stabilizer.
//!
//! A changed panel invalidates its entire AIC row and column and its entire
//! near-field row and column. Every other entry uses the identical geometry
//! and is retained. The complete updated AIC still uses the ordinary LU and
//! its original conditioning gate. No incidence is rounded or linearized.

use std::sync::Arc;

use super::fast_kernel;
use super::schur::{AicFactorization, FrozenLattice};
use super::{build_kernel_cache, mesh_wing_panels, Panel, VlmError, VlmSystem, MAX_PIVOT_RATIO};
use alas_geom::aircraft::airplane::Airplane;
use alas_math::block_lu::BlockFactorization;
use alas_math::linalg::DenseMatrix;

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
    aic: Arc<Vec<f64>>,
    factorization: Option<Arc<AicFactorization>>,
    /// Frozen-panel base for the Schur update, see [`super::schur`].
    frozen: Option<FrozenLattice>,
    /// Whether the previous assembly was an incremental update.
    incremental_before: bool,
    /// Per-cache override of [`crate::vlm::speed::schur_lu`].
    schur_override: Option<bool>,
    kernel: Option<Arc<Vec<[f64; 3]>>>,
    trace: Vec<(usize, usize, [u64; 4])>,
    trefftz: Arc<std::sync::OnceLock<Option<super::super::trefftz::TrefftzOperator>>>,
}

impl VlmGeometryCache {
    fn with_schur_update_override(schur_override: Option<bool>) -> Self {
        Self {
            schur_override,
            ..Self::default()
        }
    }

    /// A cache that uses the Schur factorization update (`true`) or always
    /// the ordinary LU (`false`), regardless of [`crate::vlm::speed`]: for
    /// bit-diff comparisons and tests.
    pub fn with_schur_update(use_schur: bool) -> Self {
        Self::with_schur_update_override(Some(use_schur))
    }

    /// The block factorization of the current `aic`, when the frozen-panel
    /// base applies (or can now be built because the previous assembly was
    /// also an incremental update). `None` selects the ordinary LU.
    fn block_factorization(
        &mut self,
        panels: &[Panel],
        changed: &[bool],
    ) -> Option<BlockFactorization> {
        if !self.frozen.as_ref().is_some_and(|f| f.applies_to(panels)) {
            self.frozen = None;
            let moving = changed.iter().filter(|&&c| c).count();
            if self.incremental_before && moving > 0 && 2 * moving <= panels.len() {
                self.frozen = FrozenLattice::build(&self.aic, panels, changed);
            }
        }
        self.frozen
            .as_ref()?
            .factor(self.aic.clone(), MAX_PIVOT_RATIO)
    }

    pub(super) fn assemble<'a>(
        &mut self,
        airplane: &'a Airplane,
        spanwise: usize,
        chordwise: usize,
    ) -> Result<VlmSystem<'a>, VlmError> {
        let _profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmAssembly);
        alas_math::stage_profile::count(alas_math::stage_profile::Counter::VlmAssemblies, 1);
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
            Arc::make_mut(&mut self.aic).resize(n * n, 0.0);
            self.frozen = None;
        }
        if changed.iter().any(|&change| change) || self.factorization.is_none() {
            use alas_math::stage_profile::{count, scope, Counter, Stage};
            count(Counter::VlmRefactorizations, 1);
            let (fill_stage, factor_stage) = if same_size {
                (Stage::VlmFill, Stage::VlmFactor)
            } else {
                count(Counter::VlmColdAssemblies, 1);
                (Stage::VlmColdFill, Stage::VlmColdFactor)
            };
            let fill_profile = scope(fill_stage);
            let aic = Arc::make_mut(&mut self.aic);
            let fast = crate::vlm::speed::fast_kernel();
            // A same-size update refreshes the cached kernel in the same
            // pass; otherwise it is rebuilt whole below.
            let kernel_entries = if same_size {
                self.kernel
                    .as_mut()
                    .map(|kernel| Arc::make_mut(kernel).as_mut_slice())
            } else {
                None
            };
            let update_kernel = kernel_entries.is_some();
            fast_kernel::fill_changed(&panels, &changed, fast, aic, kernel_entries);
            drop(fill_profile);
            let factor_profile = scope(factor_stage);
            let block = if same_size
                && self
                    .schur_override
                    .unwrap_or_else(crate::vlm::speed::schur_lu)
            {
                self.block_factorization(&panels, &changed)
            } else {
                None
            };
            let factorization = if let Some(block) = block {
                AicFactorization::Block(block)
            } else {
                match DenseMatrix::from_row_major(n, &self.aic).factor() {
                    Ok(factorization) => AicFactorization::Full(factorization),
                    Err(pivot) => {
                        // The AIC has already been updated; discard it rather
                        // than leave entries paired with the previous mesh.
                        *self = Self::with_schur_update_override(self.schur_override);
                        return Err(VlmError::SingularAic(pivot));
                    }
                }
            };
            drop(factor_profile);
            self.incremental_before = same_size;
            self.factorization = Some(Arc::new(factorization));
            if !update_kernel {
                let _kernel_profile = scope(fill_stage);
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
    use crate::vector3::dot3;
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
        let mut cache = VlmGeometryCache::with_schur_update(false);
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
    fn fast_fill_is_bit_identical_to_the_ordinary_kernel_fill() {
        let mut plane = airplane();
        // A tail pitched into the wing's near field exercises the cores.
        for section in &mut plane.wings[1].xsecs {
            section.twist = 4.0;
            section.xyz_le[2] = 0.05;
        }
        let panels = super::super::mesh_panels(&plane, 3, 4).unwrap();
        let n = panels.len();
        let tail_only: Vec<bool> = panels.iter().map(|p| p.wing_index == 1).collect();
        for mask in [vec![true; n], tail_only] {
            let mut results = Vec::new();
            for fast in [false, true] {
                let mut aic = vec![0.0; n * n];
                let mut kernel = vec![[0.0; 3]; n * n];
                super::fast_kernel::fill_changed(&panels, &mask, fast, &mut aic, Some(&mut kernel));
                let aic_bits: Vec<u64> = aic.iter().map(|v| v.to_bits()).collect();
                let kernel_bits: Vec<u64> = kernel.iter().flatten().map(|v| v.to_bits()).collect();
                results.push((aic_bits, kernel_bits));
            }
            assert!(
                results[0] == results[1],
                "fast fill differs from the ordinary kernel"
            );
        }
    }

    #[test]
    fn schur_update_matches_fresh_solve_to_rounding_and_is_taken() {
        let mut plane = airplane();
        let mut cache = VlmGeometryCache::with_schur_update(true);
        let mut taken = 0;
        for (index, incidence) in [-4.0, 2.0, 3.5, -1.0, 0.5, 5.0].into_iter().enumerate() {
            for section in &mut plane.wings[1].xsecs {
                section.twist = incidence;
            }
            plane.xyz_ref[0] += 0.01;
            let point =
                OperatingPoint::new(Atmosphere::new(9000.0), 180.0, 3.0, 0.0, 0.0, 0.0, 0.0);
            let fresh = VlmSystem::assemble(&plane, 2, 4).unwrap();
            let reused = VlmSystem::assemble_cached(&plane, 2, 4, &mut cache).unwrap();
            taken += usize::from(reused.factorization.is_block());
            let (expected, actual) = (fresh.solve(&point).unwrap(), reused.solve(&point).unwrap());
            let scale = expected
                .vortex_strengths
                .iter()
                .fold(0.0_f64, |m, g| m.max(g.abs()));
            for (a, e) in actual
                .vortex_strengths
                .iter()
                .zip(&expected.vortex_strengths)
            {
                assert!((a - e).abs() <= 1.0e-11 * scale, "step {index}: {a} vs {e}");
            }
            for (a, e) in [
                (actual.cl_lift, expected.cl_lift),
                (actual.cd_drag, expected.cd_drag),
                (actual.cm_pitch, expected.cm_pitch),
            ] {
                assert!((a - e).abs() <= 1.0e-11 * e.abs().max(1.0e-3), "{a} vs {e}");
            }
        }
        // Cold, one ordinary incremental LU to arm the base, then blocks.
        assert_eq!(taken, 4);
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
