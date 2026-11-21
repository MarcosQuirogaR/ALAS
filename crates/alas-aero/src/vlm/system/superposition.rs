// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Repeated solves of one lattice as combinations of a few basis solves.
//!
//! Without rotation rates the right-hand side of the flow-tangency system is
//! `b_i = -V . n_i`, linear in the three freestream components `V_j`:
//! `b = sum_j V_j e_j` with `e_j = -n_j` the meshed normals' components.
//! A surface pitched by `theta` about the y axis
//! ([`super::incidence::NormalChange::SurfacePitch`]) has the normal
//! `[n_x cos + n_z sin, n_y, -n_x sin + n_z cos]`, which adds
//! `d_x p_x + d_z p_z` with `p_j = -n_j` on that surface's panels (zero
//! elsewhere), `d_x = V_x (cos - 1) - V_z sin` and
//! `d_z = V_x sin + V_z (cos - 1)`. The circulation `A^-1 b` and the
//! velocity it induces at the vortex centers (the kernel product) are linear
//! in `b`, so both are the same combinations of the solutions of `e_j` and
//! `p_j` and of their kernel products: five vectors per lattice, solved and
//! multiplied once in two blocked substitutions, after which a solve costs
//! `O(n)` instead of the substitution, the residual product and the kernel
//! product, each `O(n^2)`. The superposition principle of the linear
//! potential-flow model (Katz and Plotkin, *Low-Speed Aerodynamics*, 2nd
//! ed., sec. 3.1); only the floating-point rounding differs from a direct
//! solve. The residual is linear too, so the combined residual vector
//! reports the direct solve's evidence
//! ([`alas_math::linalg::SolvedColumns`]).
//!
//! The basis is a function of the lattice alone (its factorization, kernel
//! and normals) and is shared like the kernel by every system of the
//! lattice; each group of columns is always solved as the same batch, so
//! which thread or solve built it cannot change a result.

use std::sync::{Arc, Mutex, OnceLock};

use alas_math::linalg::{SolveDiagnostics, SolvedColumns};

use super::incidence::NormalChange;
use super::kernel_cache::induced_columns_from_kernel;
use super::schur::AicFactorization;
use super::Panel;
use crate::operating_point::OperatingPoint;

/// The basis solves of one lattice, shared by its systems.
pub(super) type BasisSlot = Arc<Basis>;

/// The level columns `e_x, e_y, e_z`, and the pitch columns `p_x, p_z` of
/// each set of pitched surfaces asked for.
#[derive(Default)]
pub(super) struct Basis {
    level: OnceLock<Option<Batch>>,
    pitched: Mutex<Vec<(Vec<usize>, BatchSlot)>>,
}

/// A batch built on first use; `None` if its solves were not finite.
type BatchSlot = Arc<OnceLock<Option<Batch>>>;

impl std::fmt::Debug for Basis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Basis").finish_non_exhaustive()
    }
}

/// One batch of basis columns: their solutions and residuals, and the
/// velocity each solution induces at the vortex centers.
struct Batch {
    solved: SolvedColumns,
    induced: Vec<Vec<[f64; 3]>>,
}

impl Batch {
    fn build(
        factorization: &AicFactorization,
        kernel: &[[f64; 3]],
        columns: &[Vec<f64>],
    ) -> Option<Self> {
        let solved = factorization.solve_with_residuals(columns);
        let n = columns.first().map_or(0, Vec::len);
        let finite = solved
            .solutions
            .iter()
            .chain(&solved.residuals)
            .all(|column| column.iter().all(|value| value.is_finite()));
        if !finite {
            return None;
        }
        let induced = induced_columns_from_kernel(kernel, n, &solved.solutions);
        Some(Self { solved, induced })
    }
}

/// A solve assembled from the basis.
struct Superposed {
    vortex_strengths: Vec<f64>,
    diagnostics: SolveDiagnostics,
    /// The induced velocity at each vortex center, without the far field.
    induced: Vec<[f64; 3]>,
}

/// The circulation of `rhs` (the right-hand side of `op_point` with
/// `normal_change`), its diagnostics and, when it was superposed, the
/// velocity it induces at the vortex centers: superposed for a repeated
/// solve (`kernel` present) without rotation rates while
/// [`crate::vlm::speed::superposition`] is on, direct otherwise.
pub(super) fn strengths(
    basis: &Basis,
    factorization: &AicFactorization,
    kernel: Option<&[[f64; 3]]>,
    panels: &[Panel],
    op_point: &OperatingPoint,
    normal_change: &NormalChange<'_>,
    rhs: &[f64],
) -> (Vec<f64>, SolveDiagnostics, Option<Vec<[f64; 3]>>) {
    let level = op_point.p == 0.0 && op_point.q == 0.0 && op_point.r == 0.0;
    let superposed = kernel
        .filter(|_| level && crate::vlm::speed::superposition())
        .and_then(|kernel| {
            solve(
                basis,
                factorization,
                kernel,
                panels,
                op_point.freestream_velocity_geometry_axes(),
                normal_change,
                rhs,
            )
        });
    match superposed {
        Some(solved) => (
            solved.vortex_strengths,
            solved.diagnostics,
            Some(solved.induced),
        ),
        None => {
            let (strengths, diagnostics) = factorization.solve_vector(rhs);
            (strengths, diagnostics, None)
        }
    }
}

/// The solve of `rhs` (the right-hand side of `velocity` with
/// `normal_change` and no rotation rates) from `basis`, building the columns
/// it needs on first use; `None` where the basis does not apply (incidence
/// offsets) or did not solve to finite values, and the caller solves
/// directly.
fn solve(
    basis: &Basis,
    factorization: &AicFactorization,
    kernel: &[[f64; 3]],
    panels: &[Panel],
    velocity: [f64; 3],
    normal_change: &NormalChange<'_>,
    rhs: &[f64],
) -> Option<Superposed> {
    let pitch = match *normal_change {
        NormalChange::None | NormalChange::SurfacePitch(_, 0.0) => None,
        NormalChange::SurfacePitch(surfaces, angle) => Some((surfaces, angle)),
        NormalChange::Offsets(_) => return None,
    };
    let level = basis
        .level
        .get_or_init(|| {
            let columns: Vec<Vec<f64>> = (0..3)
                .map(|j| panels.iter().map(|p| -p.normal_direction[j]).collect())
                .collect();
            Batch::build(factorization, kernel, &columns)
        })
        .as_ref()?;
    let mut terms: Vec<(f64, &Batch, usize)> = (0..3).map(|j| (velocity[j], level, j)).collect();
    let pitched_slot;
    if let Some((surfaces, angle)) = pitch {
        pitched_slot = {
            let mut held = basis.pitched.lock().ok()?;
            match held.iter().find(|(set, _)| set.as_slice() == surfaces) {
                Some((_, slot)) => slot.clone(),
                None => {
                    let slot = Arc::default();
                    held.push((surfaces.to_vec(), Arc::clone(&slot)));
                    slot
                }
            }
        };
        let pitched = pitched_slot
            .get_or_init(|| {
                let columns: Vec<Vec<f64>> = [0, 2]
                    .iter()
                    .map(|&j| {
                        panels
                            .iter()
                            .map(|p| {
                                if surfaces.contains(&p.wing_index) {
                                    -p.normal_direction[j]
                                } else {
                                    0.0
                                }
                            })
                            .collect()
                    })
                    .collect();
                Batch::build(factorization, kernel, &columns)
            })
            .as_ref()?;
        let sin = angle.sin();
        // cos - 1 without the cancellation: -2 sin^2(theta / 2).
        let half = (0.5 * angle).sin();
        let cos_minus_one = -2.0 * half * half;
        terms.push((velocity[0] * cos_minus_one - velocity[2] * sin, pitched, 0));
        terms.push((velocity[0] * sin + velocity[2] * cos_minus_one, pitched, 1));
    }
    if !terms
        .iter()
        .all(|(coefficient, _, _)| coefficient.is_finite())
    {
        return None;
    }
    let n = panels.len();
    let mut vortex_strengths = vec![0.0_f64; n];
    let mut residual = vec![0.0_f64; n];
    let mut induced = vec![[0.0_f64; 3]; n];
    for &(coefficient, batch, column) in &terms {
        let solution = &batch.solved.solutions[column];
        let column_residual = &batch.solved.residuals[column];
        let column_induced = &batch.induced[column];
        for i in 0..n {
            vortex_strengths[i] += coefficient * solution[i];
            residual[i] += coefficient * column_residual[i];
            for (sum, value) in induced[i].iter_mut().zip(column_induced[i]) {
                *sum += coefficient * value;
            }
        }
    }
    let diagnostics = level.solved.diagnostics(&residual, &vortex_strengths, rhs);
    Some(Superposed {
        vortex_strengths,
        diagnostics,
        induced,
    })
}

// Tests construct valid lattices; a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use alas_atmo::Atmosphere;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    use crate::operating_point::OperatingPoint;
    use crate::vlm::{VlmGeometryCache, VlmResult, VlmSystem};

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
            name: "Superposition test".into(),
            xyz_ref: [0.5, 0.0, 0.0],
            s_ref: wing.reference_area(),
            b_ref: wing.reference_span(),
            c_ref: wing.mean_aerodynamic_chord(),
            wings: vec![wing, tail],
            fuselages: vec![],
        }
    }

    fn agree(superposed: &VlmResult, direct: &VlmResult) {
        let close = |a: f64, b: f64, what: &str| {
            assert!(
                (a - b).abs() <= 1.0e-12 * (1.0 + b.abs()),
                "{what}: superposed {a} against direct {b}"
            );
        };
        close(superposed.cl_lift, direct.cl_lift, "CL");
        close(superposed.cd_drag, direct.cd_drag, "CDi");
        close(superposed.cm_pitch, direct.cm_pitch, "Cm");
        for (a, b) in superposed
            .vortex_strengths
            .iter()
            .zip(&direct.vortex_strengths)
        {
            close(*a, *b, "gamma");
        }
        assert!(superposed.solve_diagnostics.residual_is_bounded());
        assert!(superposed.solve_diagnostics.normalized_residual < 1.0e-13);
    }

    #[test]
    fn repeated_solves_agree_with_a_direct_solve_to_rounding() {
        let plane = airplane();
        let mut cache = VlmGeometryCache::with_schur_update(false);
        // Each assembly is a new system: its first solve is direct, every
        // solve of a cached system (and every later one) is superposed.
        for (alpha, beta, pitch_deg) in [(2.0, 0.0, 0.0), (6.0, 0.0, 1.5), (-1.0, 3.0, -2.0)] {
            let point =
                OperatingPoint::new(Atmosphere::new(9000.0), 180.0, alpha, beta, 0.0, 0.0, 0.0);
            let fresh = VlmSystem::assemble(&plane, 1, 4).unwrap();
            let direct = fresh
                .solve_with_surface_pitch(&point, &[1], f64::to_radians(pitch_deg))
                .unwrap();
            let superposed = fresh
                .solve_with_surface_pitch(&point, &[1], f64::to_radians(pitch_deg))
                .unwrap();
            agree(&superposed, &direct);
            let cached = VlmSystem::assemble_cached(&plane, 1, 4, &mut cache).unwrap();
            let from_cache = cached
                .solve_with_surface_pitch(&point, &[1], f64::to_radians(pitch_deg))
                .unwrap();
            agree(&from_cache, &direct);
        }
    }

    #[test]
    fn rotation_rates_keep_the_direct_solve() {
        let plane = airplane();
        let point = OperatingPoint::new(Atmosphere::new(9000.0), 180.0, 3.0, 0.0, 0.1, 0.0, 0.0);
        let system = VlmSystem::assemble(&plane, 1, 4).unwrap();
        let first = system.solve(&point).unwrap();
        let second = system.solve(&point).unwrap();
        assert_eq!(first.vortex_strengths, second.vortex_strengths);
        assert_eq!(first.cl_lift.to_bits(), second.cl_lift.to_bits());
    }
}
