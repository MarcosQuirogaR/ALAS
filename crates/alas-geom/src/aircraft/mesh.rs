// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/geometry/wing.py, `Wing.mesh_thin_surface` and
// `Wing.mesh_line`.
// Upstream: reference geometry 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! [`Wing::mesh_thin_surface`] and [`Wing::mesh_line`]: meshing the mean
//! camber surface of a wing into the `(points, faces)` format a vortex
//! lattice solve reads its panels from.
//!
//! # Why this is in scope at all
//!
//! Nothing in `alas/`'s own source calls either method before P11 (Figures).
//! But reference geometry's own `VortexLatticeMethod.run()` calls
//! `wing.mesh_thin_surface(method="quad", chordwise_resolution=...,
//! chordwise_spacing_function=..., add_camber=True)` on every wing, every
//! time it runs, and `alas/physics/aerodynamics.py`'s `_run_vlm` constructs
//! and runs exactly that solver. That makes this row a P5 prerequisite (of
//! `alas-aero::vlm`, which will call [`Wing::mesh_thin_surface`] the same
//! way), not a P11-only row -- see `docs/PORTING.md`'s corrected paragraph
//! for this crate's Geometry section.
//!
//! # Scoped to `method="quad"`
//!
//! Upstream's `mesh_thin_surface` takes a `method: "tri" | "quad"`
//! parameter; `add_face`'s `"tri"` branch splits each quadrilateral into two
//! triangles. `VortexLatticeMethod.run()` -- the only caller this crate's
//! inputs reach -- always passes `method="quad"`, so [`Wing::mesh_thin_surface`]
//! has no `method` parameter at all rather than one nothing ever sets to
//! `"tri"`.
//!
//! # `mesh_line`'s per-cross-section station, and a bug not reproduced
//!
//! Upstream's `x_nondim`/`z_nondim` can be a single value shared by every
//! cross-section, or one value per cross-section (`Union[float,
//! List[float]]`); [`XsecStation`] carries that same choice. For each
//! cross-section `i`, upstream resolves `xsec_x_nondim = x_nondim[i]` (or
//! falls back to the bare `x_nondim` if that indexing fails, i.e. `x_nondim`
//! was a scalar) -- and then, when `add_camber` is set, looks the camber up
//! with `xsec.airfoil.local_camber(x_over_c=x_nondim)`, the *un-indexed*
//! outer parameter, not `xsec_x_nondim`. At `mesh_thin_surface`'s one call
//! site `x_nondim` is always a bare scalar, so `x_nondim == xsec_x_nondim`
//! there and the two reads are indistinguishable -- `CLAUDE.md` already
//! flags this as "harmless where it is called." [`Wing::mesh_line`] here uses
//! the correct per-cross-section value (`xsec_x_nondim`, called `x` in the
//! loop below) for the camber lookup rather than the outer parameter. This is
//! a translate-the-fix decision, not a `deviation-candidate`: no input this
//! program constructs reaches a difference in output, since the two values
//! are always equal at the one call site. It matters only if `mesh_line` is
//! ever handed a real per-cross-section [`XsecStation::PerXsec`] with
//! `add_camber` set, which is exactly the case
//! `mesh_line_uses_each_cross_sections_own_x_nondim_for_its_camber_lookup`
//! (below) constructs to pin the fixed behavior down, since no fixture case
//! exercises it (`mesh_thin_surface`'s scalar station never can).
//!
//! # Face-vertex ordering contract
//!
//! From upstream's own docstring, reproduced exactly:
//!
//! * Faces proceed in chordwise strips: for a fixed pair of adjacent
//!   spanwise stations, every chordwise panel between them is emitted before
//!   moving to the next pair of spanwise stations. The first face is nearest
//!   the leading edge of the wing root.
//! * Each face's four vertices are ordered front-left, back-left,
//!   back-right, front-right (chord-forward is "front"; root-to-tip is
//!   "right" on the wing this loop builds directly).
//! * If [`Wing::symmetric`], the mirrored (left) wing's faces follow all of
//!   the right wing's, built from a second, appended copy of every point
//!   with `y` negated, and wound in the opposite spanwise order
//!   (front-left/back-left/back-right/front-right at spanwise indices
//!   `i+1`/`i+1`/`i`/`i` rather than `i`/`i`/`i+1`/`i+1`) so the face normal
//!   still points outward from the mirrored surface.
//!
//! `points` is laid out one spanwise strip (one cross-section's worth of
//! chordwise-mesh points, in root-to-tip order) per chordwise station, in
//! increasing-chordwise-station order -- point `i + j * num_xsecs` is
//! cross-section `i`'s point at chordwise station `j`. `tests/parity_geom_aircraft_mesh.rs`
//! and this file's own unit tests both pin this layout down directly against
//! computed index arrays, not just against the fixture's specific numbers.

use super::spacing::cosspace;
use super::wing::Wing;

/// The value [`Wing::mesh_line`] places at every cross-section along one
/// coordinate: a single value shared by every cross-section, or one value
/// per cross-section -- upstream's `Union[float, List[float]]`.
#[derive(Debug, Clone, PartialEq)]
pub enum XsecStation {
    /// The same value at every cross-section.
    Scalar(f64),
    /// One value per cross-section, in root-to-tip order. Must have exactly
    /// as many entries as the wing has cross-sections; see [`MeshLineError`].
    PerXsec(Vec<f64>),
}

/// [`Wing::mesh_line`] rejects a [`XsecStation::PerXsec`] whose length does
/// not match the wing's cross-section count -- the same condition upstream's
/// `raise ValueError("If \`x_nondim\` is an iterable, it should be the same
/// length as \`Wing.xsecs\`...")` guards, restated as a typed error since
/// this crate does not panic (`CONTRIBUTING.md`).
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum MeshLineError {
    /// `x_nondim` was [`XsecStation::PerXsec`] with the wrong length.
    #[error("`x_nondim` has {0} entries but the wing has {1} cross-sections")]
    XNondimLengthMismatch(usize, usize),
    /// `z_nondim` was [`XsecStation::PerXsec`] with the wrong length.
    #[error("`z_nondim` has {0} entries but the wing has {1} cross-sections")]
    ZNondimLengthMismatch(usize, usize),
}

/// Resolve an [`XsecStation`] into one value per cross-section, or an error
/// if a [`XsecStation::PerXsec`] does not have exactly `len` entries.
fn resolve_station(
    station: &XsecStation,
    len: usize,
    mismatch: impl FnOnce(usize, usize) -> MeshLineError,
) -> Result<Vec<f64>, MeshLineError> {
    match station {
        XsecStation::Scalar(value) => Ok(vec![*value; len]),
        XsecStation::PerXsec(values) => {
            if values.len() != len {
                return Err(mismatch(values.len(), len));
            }
            Ok(values.clone())
        }
    }
}

impl Wing {
    /// A point through each of this wing's cross-sections, root to tip, at
    /// `x_nondim` chord fraction and `z_nondim` (airfoil-frame) height, each
    /// optionally offset by that cross-section's own mean camber --
    /// `Wing.mesh_line`. Ignores wing symmetry, giving only the one side this
    /// wing's own cross-sections describe, exactly as upstream does.
    ///
    /// See the module doc for the camber-lookup fix this makes relative to
    /// upstream when `x_nondim` is [`XsecStation::PerXsec`].
    ///
    /// # Errors
    ///
    /// [`MeshLineError`] if `x_nondim` or `z_nondim` is
    /// [`XsecStation::PerXsec`] with a length other than the wing's
    /// cross-section count.
    pub fn mesh_line(
        &self,
        x_nondim: XsecStation,
        z_nondim: XsecStation,
        add_camber: bool,
    ) -> Result<Vec<[f64; 3]>, MeshLineError> {
        let len = self.xsecs.len();
        let x_values = resolve_station(&x_nondim, len, MeshLineError::XNondimLengthMismatch)?;
        let z_values = resolve_station(&z_nondim, len, MeshLineError::ZNondimLengthMismatch)?;
        Ok(self.mesh_line_resolved(&x_values, &z_values, add_camber))
    }

    /// [`Wing::mesh_line`]'s body, once `x_nondim`/`z_nondim` are already one
    /// value per cross-section -- shared with [`Wing::mesh_thin_surface`],
    /// which always calls with a scalar station broadcast to every
    /// cross-section and so cannot hit [`MeshLineError`].
    fn mesh_line_resolved(
        &self,
        x_values: &[f64],
        z_values: &[f64],
        add_camber: bool,
    ) -> Vec<[f64; 3]> {
        self.xsecs
            .iter()
            .enumerate()
            .map(|(index, xsec)| {
                let x = x_values[index];
                let mut z = z_values[index];
                if add_camber {
                    // Uses `x` (this cross-section's own station), not the
                    // outer, un-indexed parameter -- see the module doc.
                    z += xsec.airfoil.local_camber(&[x])[0];
                }
                self.xyz_of_xsec(index, x, z)
            })
            .collect()
    }

    /// Meshes the mean camber surface of the wing as a thin sheet of
    /// quadrilaterals -- `Wing.mesh_thin_surface(method="quad", ...)`, the
    /// only `method` this crate's one caller ever uses (see the module doc).
    ///
    /// `chordwise_resolution` chordwise panels are cut with
    /// [`cosspace`]-spaced stations (upstream's `chordwise_spacing_function`
    /// default and this program's only caller's choice), `add_camber`
    /// controls whether each station follows the local airfoil's mean camber
    /// line or the flat planform, and the wing is mirrored about the XZ plane
    /// first if [`Wing::symmetric`]. See the module doc for the exact
    /// face-vertex ordering this returns.
    pub fn mesh_thin_surface(
        &self,
        chordwise_resolution: usize,
        add_camber: bool,
    ) -> (Vec<[f64; 3]>, Vec<[usize; 4]>) {
        let x_nondim = cosspace(0.0, 1.0, chordwise_resolution + 1);
        let num_i = self.xsecs.len(); // spanwise: one point per cross-section
        let num_j = x_nondim.len(); // chordwise: one strip per station

        let z_values = vec![0.0; num_i];
        let mut points: Vec<[f64; 3]> = Vec::with_capacity(num_i * num_j);
        for &x in &x_nondim {
            let x_values = vec![x; num_i];
            points.extend(self.mesh_line_resolved(&x_values, &z_values, add_camber));
        }

        let index_of = |iloc: usize, jloc: usize| iloc + jloc * num_i;

        let mut faces: Vec<[usize; 4]> = Vec::new();
        for i in 0..num_i.saturating_sub(1) {
            for j in 0..num_j.saturating_sub(1) {
                faces.push([
                    index_of(i, j),         // front-left
                    index_of(i, j + 1),     // back-left
                    index_of(i + 1, j + 1), // back-right
                    index_of(i + 1, j),     // front-right
                ]);
            }
        }

        if self.symmetric {
            let index_offset = points.len();
            let mirrored: Vec<[f64; 3]> = points.iter().map(|&[x, y, z]| [x, -y, z]).collect();
            points.extend(mirrored);

            let index_of_mirrored = |iloc: usize, jloc: usize| index_offset + iloc + jloc * num_i;
            for i in 0..num_i.saturating_sub(1) {
                for j in 0..num_j.saturating_sub(1) {
                    faces.push([
                        index_of_mirrored(i + 1, j),     // front-left
                        index_of_mirrored(i + 1, j + 1), // back-left
                        index_of_mirrored(i, j + 1),     // back-right
                        index_of_mirrored(i, j),         // front-right
                    ]);
                }
            }
        }

        (points, faces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft::airfoil::Airfoil;
    use crate::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn two_xsec_wing(symmetric: bool) -> Wing {
        Wing::new(
            "Probe",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    #[test]
    fn mesh_thin_surface_face_indices_follow_the_chordwise_strip_then_spanwise_contract() {
        // 2 cross-sections (num_i=2), chordwise_resolution=2 -> 3 stations
        // (num_j=3): 1*2 = 2 faces on an asymmetric wing.
        let wing = two_xsec_wing(false);
        let (points, faces) = wing.mesh_thin_surface(2, false);

        assert_eq!(points.len(), 2 * 3);
        assert_eq!(
            faces,
            vec![
                [0, 2, 3, 1], // i=0, j=0: index_of(0,0), index_of(0,1), index_of(1,1), index_of(1,0)
                [2, 4, 5, 3], // i=0, j=1
            ]
        );
    }

    #[test]
    fn mesh_thin_surface_mirrors_points_and_appends_reversed_winding_faces_when_symmetric() {
        let wing = two_xsec_wing(true);
        let (points, faces) = wing.mesh_thin_surface(2, false);

        assert_eq!(
            points.len(),
            2 * 2 * 3,
            "points double for the mirrored side"
        );
        for i in 0..6 {
            let original = points[i];
            let mirrored = points[6 + i];
            assert_eq!(mirrored, [original[0], -original[1], original[2]]);
        }

        assert_eq!(
            faces,
            vec![
                [0, 2, 3, 1],
                [2, 4, 5, 3],
                // Mirrored side, index_offset=6, spanwise indices reversed
                // relative to the right wing's winding.
                [7, 9, 8, 6],
                [9, 11, 10, 8],
            ]
        );
    }

    #[test]
    fn mesh_thin_surface_without_camber_lies_on_the_flat_planform() {
        // With add_camber=false and z_nondim=0, every meshed point should
        // fall exactly on xyz_of_xsec's own quarter-of-chord-free output --
        // a property `local_camber`'s specific values cannot mask.
        let wing = two_xsec_wing(false);
        let (points, _) = wing.mesh_thin_surface(1, false);
        for (index, xsec_index) in [0usize, 1].into_iter().enumerate() {
            let expected = wing.xyz_of_xsec(xsec_index, 0.0, 0.0);
            assert_eq!(points[index], expected);
        }
    }

    #[test]
    fn mesh_line_rejects_a_per_xsec_station_of_the_wrong_length() {
        let wing = two_xsec_wing(false);
        let err = wing
            .mesh_line(
                XsecStation::PerXsec(vec![0.1, 0.2, 0.3]),
                XsecStation::Scalar(0.0),
                false,
            )
            .unwrap_err();
        assert_eq!(err, MeshLineError::XNondimLengthMismatch(3, 2));
    }

    #[test]
    fn mesh_line_uses_each_cross_sections_own_x_nondim_for_its_camber_lookup() {
        // naca4412 is asymmetrically cambered, so local_camber differs
        // meaningfully between x/c = 0.2 and x/c = 0.8 -- the fixed lookup
        // must read each cross-section's own station, not a shared one.
        let airfoil = naca("naca4412");
        let wing = Wing::new(
            "Probe",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, airfoil.clone()),
                WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, airfoil.clone()),
            ],
            false,
        );

        let camber_at_02 = airfoil.local_camber(&[0.2])[0];
        let camber_at_08 = airfoil.local_camber(&[0.8])[0];
        assert!(
            (camber_at_02 - camber_at_08).abs() > 1e-6,
            "camber must differ across x/c for this test to be meaningful"
        );

        let points = wing
            .mesh_line(
                XsecStation::PerXsec(vec![0.2, 0.8]),
                XsecStation::Scalar(0.0),
                true,
            )
            .expect("both entries match the wing's 2 cross-sections");

        assert_eq!(points[0], wing.xyz_of_xsec(0, 0.2, camber_at_02));
        assert_eq!(points[1], wing.xyz_of_xsec(1, 0.8, camber_at_08));

        // The upstream bug this port does not reproduce would instead look
        // camber up at the outer, un-indexed `x_nondim` for every
        // cross-section; had that happened here, xsec 1's z would have come
        // from `camber_at_02`, not `camber_at_08` -- these differ, so this
        // also confirms the fixed value was actually used.
        assert_ne!(points[1], wing.xyz_of_xsec(1, 0.8, camber_at_02));
    }

    #[test]
    fn mesh_line_scalar_station_broadcasts_to_every_cross_section() {
        let wing = two_xsec_wing(false);
        let points = wing
            .mesh_line(XsecStation::Scalar(0.25), XsecStation::Scalar(0.0), false)
            .expect("scalar stations never mismatch a length");
        assert_eq!(points[0], wing.xyz_of_xsec(0, 0.25, 0.0));
        assert_eq!(points[1], wing.xyz_of_xsec(1, 0.25, 0.0));
    }
}
