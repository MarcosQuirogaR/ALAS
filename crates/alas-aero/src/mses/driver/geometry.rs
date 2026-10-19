// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airfoil clean-up applied before the MSES decks are written.

use alas_geom::aircraft::airfoil::Airfoil;

/// Close a trailing edge whose lower point lies above its upper point.
///
/// Selig order starts at the upper trailing edge and ends at the lower one, so
/// `y_first < y_last` at `x` ~ 1 is a negative trailing-edge gap: the two
/// surfaces cross just before the end. The `dsma523a` library file carries this
/// (its last point (1.000000, 0.000498) is above its first (1.000000,
/// 0.000308)) and so does every section morphed from it (B747-400 and DC-10
/// root, kink and tip). MSET meshes such a section, but MSES then fails to
/// converge even at M 0.3 and Re 5e6 (B747-400 root, 0 of 9 sweeps converged
/// before; all of them converge with the end points set to their mean). Both
/// end points are replaced by their mean `y`, a zero-gap closed edge a few
/// 1e-4 chord away from the stored points. Sections with a normal (open or
/// closed) trailing edge are returned unchanged, so reference fixtures are
/// untouched.
pub(super) fn repair_inverted_trailing_edge(mut airfoil: Airfoil) -> (Airfoil, Option<String>) {
    let (Some(&first), Some(&last)) = (airfoil.coordinates.first(), airfoil.coordinates.last())
    else {
        return (airfoil, None);
    };
    let at_trailing_edge = (first.0 - last.0).abs() < 1.0e-6 && first.0 > 0.99;
    if !at_trailing_edge || last.1.is_nan() || first.1.is_nan() || last.1 <= first.1 + 1.0e-9 {
        return (airfoil, None);
    }
    let mean = 0.5 * (first.1 + last.1);
    let count = airfoil.coordinates.len();
    airfoil.coordinates[0].1 = mean;
    airfoil.coordinates[count - 1].1 = mean;
    let note = format!(
        "[ALAS: inverted trailing edge repaired: lower end point y={:.6} was above upper end point y={:.6}; both set to {mean:.6}]",
        last.1, first.1
    );
    (airfoil, Some(note))
}
