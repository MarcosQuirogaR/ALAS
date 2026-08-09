// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from aerosandbox/geometry/wing.py
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! AeroSandbox's `Wing` and `WingXSec`, scoped to the surface
//! `alas-geom::builder` and `alas-mass::torenbeek` (both later modules)
//! actually call: `.translate(...)`, `.subdivide_sections(...)`, `.area()`,
//! `.span()`, `.mean_aerodynamic_chord()`, `.aerodynamic_center()` (indexed
//! `[0]` afterward), `.taper_ratio()`, `.mean_sweep_angle(x_nondim)`,
//! `.control_surface_area()`, the `xsecs`/`symmetric`/`name` fields, and
//! `WingXSec.twist` mutated in place by the later trim phase.
//! `docs/PORTING.md` records the scoping decision and a prior grep across all
//! of `alas/`, not just `alas/geometry/`, for what reaches these two classes.
//!
//! Left untranslated, because nothing in this program's inputs reaches them:
//! `aspect_ratio`, `is_entirely_symmetric`, `mean_geometric_chord`,
//! `mean_twist_angle`, `mean_dihedral_angle`, `volume`, every other
//! control-surface method (`get_control_surface_names`,
//! `set_control_surface_deflections`), `mesh_body`, `mesh_thin_surface`,
//! `mesh_line`, `draw*`, `_compute_frame_of_section` (meshing only),
//! `xsec_area`, and every non-default argument of the methods that are
//! translated (`type=`, `_sectional=` as a public parameter,
//! `include_centerline_distance=`, `control_surface_area`'s `by_name=`).
//!
//! [`Wing::control_surface_area`] always returns `0.0`: `WingXSec` here has
//! no `control_surfaces` field at all (see above), so the loop upstream sums
//! over is always empty on every wing this program builds -- the same result
//! upstream's formula produces for a wing with no control surfaces defined
//! on any section.
//!
//! # Airfoil identity vs. structural equality
//!
//! [`Wing::subdivide_sections`] branches on whether two adjacent `WingXSec`s
//! share the same airfoil, which upstream tests with Python's default
//! `__eq__` -- object identity, true exactly when the same `Airfoil` object
//! was passed to both constructors. This crate's `Airfoil` values are owned,
//! not shared references, so there is no identity to compare; this port uses
//! [`Airfoil`]'s `#[derive(PartialEq)]` (structural equality: same name, same
//! coordinates) instead. That is a faithful translation of the *reachable*
//! behavior, not a `deviation-candidate`: every call site this program has
//! either passes the literal same `Airfoil` value to both `WingXSec`s (the
//! main wing's root/break interval, and both ends of the hstab/vstab) or two
//! airfoils that are never coordinate-identical (the main wing's break/tip
//! interval), so structural and identity equality agree on every input this
//! program ever constructs.
//!
//! # `aerodynamic_center`'s un-rotated chordwise offset
//!
//! [`Wing::aerodynamic_center`] adds `chord_fraction * section_MAC_length`
//! straight onto the X axis without rotating it by the section's twist --
//! upstream's own `# TODO rotate this vector by the local twist angle`.
//! Reproduced exactly; `docs/PORTING.md`'s Geometry section already records
//! this as a `deviation-candidate`.

use std::f64::consts::PI;

use super::airfoil::Airfoil;
use super::spacing::linspace;
use super::vector3::{
    add3, blend3, cross3, dot3, matvec3, norm3, project_to_yz_and_normalize, rotation_matrix_3d,
    scale3, sub3,
};

/// `blend_with_another_airfoil`'s `n_points_per_side`, upstream's default and
/// the only value [`Wing::subdivide_sections`] ever calls it with.
const SUBDIVIDE_BLEND_N_POINTS_PER_SIDE: usize = 100;

/// A wing cross-section: leading-edge position, chord, twist and airfoil --
/// `WingXSec`, scoped to the fields this program uses (no control surfaces,
/// no analysis-specific options; see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct WingXSec {
    /// Leading-edge coordinates, in geometry axes.
    pub xyz_le: [f64; 3],
    /// Chord at this cross-section.
    pub chord: f64,
    /// Twist angle in degrees, about the leading edge. Mutated in place by
    /// the trim phase (a later module); an ordinary public field, not a
    /// derived quantity.
    pub twist: f64,
    /// The airfoil section at this cross-section.
    pub airfoil: Airfoil,
}

impl WingXSec {
    /// A new cross-section at `xyz_le`, `chord`, `twist` (degrees) and
    /// `airfoil`.
    pub fn new(xyz_le: [f64; 3], chord: f64, twist: f64, airfoil: Airfoil) -> Self {
        Self {
            xyz_le,
            chord,
            twist,
            airfoil,
        }
    }

    /// A copy of this cross-section translated by `xyz`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            xyz_le: add3(self.xyz_le, xyz),
            ..self.clone()
        }
    }
}

/// A wing: a name, an ordered list of cross-sections, and whether it is
/// mirrored about the XZ plane -- `Wing`, scoped to the fields and methods
/// this program uses (see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Wing {
    /// The wing's name, e.g. `"Main Wing"`.
    pub name: String,
    /// Cross-sections from root to tip. Lofted linearly between adjacent
    /// pairs.
    pub xsecs: Vec<WingXSec>,
    /// Whether the wing is mirrored about the XZ plane. If `true`, every
    /// quantity this module computes accounts for both halves.
    pub symmetric: bool,
}

/// [`Wing::subdivide_sections`] rejects a ratio less than 2 -- the same
/// condition upstream's `raise ValueError("`ratio` must be an integer
/// greater than or equal to 2.")` guards, restated as a typed error since
/// this crate does not panic (`CONTRIBUTING.md`). The "integer" half of
/// upstream's check has no counterpart here: `ratio`'s type already
/// guarantees that.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum SubdivideSectionsError {
    /// `ratio` was less than 2.
    #[error("`ratio` must be greater than or equal to 2, got {0}")]
    RatioTooSmall(usize),
    /// Blending two distinct airfoils at a subdivision boundary failed to
    /// repanel -- see [`Airfoil::blend_with_another_airfoil`].
    #[error(transparent)]
    Blend(#[from] alas_math::CubicSplineError),
}

impl Wing {
    /// A new wing named `name`, holding `xsecs`, mirrored about the XZ plane
    /// iff `symmetric`.
    pub fn new(name: impl Into<String>, xsecs: Vec<WingXSec>, symmetric: bool) -> Self {
        Self {
            name: name.into(),
            xsecs,
            symmetric,
        }
    }

    /// A copy of this wing translated by `xyz` -- `Wing.translate`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            name: self.name.clone(),
            xsecs: self.xsecs.iter().map(|xsec| xsec.translate(xyz)).collect(),
            symmetric: self.symmetric,
        }
    }

    /// A new wing splitting each of this wing's `n - 1` lofted sections into
    /// `ratio` smaller ones, by inserting linearly interpolated cross-sections
    /// -- `Wing.subdivide_sections`, hardcoded to `spacing_function=np.linspace`
    /// (upstream's default and this program's only caller).
    ///
    /// # Errors
    ///
    /// [`SubdivideSectionsError::RatioTooSmall`] if `ratio < 2`.
    /// [`SubdivideSectionsError::Blend`] if a subdivision boundary needs two
    /// structurally distinct airfoils blended together and that blend's
    /// `repanel` step fails.
    pub fn subdivide_sections(&self, ratio: usize) -> Result<Self, SubdivideSectionsError> {
        if ratio < 2 {
            return Err(SubdivideSectionsError::RatioTooSmall(ratio));
        }

        // `linspace(0, 1, ratio + 1)[:-1]`: `ratio` fractions covering
        // [0, 1) -- the final fraction (exactly 1) is dropped because the
        // outer xsec it would reproduce is appended separately, unchanged,
        // after the loop.
        let full = linspace(0.0, 1.0, ratio + 1);
        let span_fractions = &full[..full.len() - 1];

        let mut new_xsecs = Vec::new();
        for pair in self.xsecs.windows(2) {
            let (xsec_a, xsec_b) = (&pair[0], &pair[1]);
            for &s in span_fractions {
                let a_weight = 1.0 - s;
                let b_weight = s;

                // Upstream's `xsec_a.airfoil == xsec_b.airfoil` and
                // `a_weight == 1` branches both resolve to `xsec_a.airfoil`
                // unchanged, so they are merged here (clippy's
                // `if_same_then_else` would otherwise flag the duplicate);
                // the two conditions stay logically distinct in the
                // module doc's discussion of what upstream's identity
                // check reaches on this program's inputs.
                let airfoil = if xsec_a.airfoil == xsec_b.airfoil || a_weight == 1.0 {
                    xsec_a.airfoil.clone()
                } else if b_weight == 1.0 {
                    xsec_b.airfoil.clone()
                } else {
                    xsec_a.airfoil.blend_with_another_airfoil(
                        &xsec_b.airfoil,
                        b_weight,
                        SUBDIVIDE_BLEND_N_POINTS_PER_SIDE,
                    )?
                };

                new_xsecs.push(WingXSec {
                    xyz_le: blend3(xsec_a.xyz_le, xsec_b.xyz_le, a_weight, b_weight),
                    chord: xsec_a.chord * a_weight + xsec_b.chord * b_weight,
                    twist: xsec_a.twist * a_weight + xsec_b.twist * b_weight,
                    airfoil,
                });
            }
        }

        if let Some(last) = self.xsecs.last() {
            new_xsecs.push(last.clone());
        }

        Ok(Self {
            name: self.name.clone(),
            xsecs: new_xsecs,
            symmetric: self.symmetric,
        })
    }

    /// The quarter-chord point of every cross-section, root to tip --
    /// `Wing._compute_xyz_of_WingXSec(i, x_nondim=0.25, z_nondim=0)` for each
    /// `i`, factored out because [`Wing::span`], [`Wing::area`],
    /// [`Wing::mean_aerodynamic_chord`] and [`Wing::aerodynamic_center`] all
    /// build on the same per-section spans this produces.
    fn quarter_chord_points(&self) -> Vec<[f64; 3]> {
        (0..self.xsecs.len())
            .map(|index| self.xyz_of_xsec(index, 0.25, 0.0))
            .collect()
    }

    /// The point at `x_nondim` chord fraction and `z_nondim` (airfoil-frame)
    /// height of cross-section `index` -- `Wing._compute_xyz_of_WingXSec`.
    fn xyz_of_xsec(&self, index: usize, x_nondim: f64, z_nondim: f64) -> [f64; 3] {
        let (xg_local, _yg_local, zg_local) = self.frame_of_xsec(index);
        let origin = self.xsecs[index].xyz_le;
        let chord = self.xsecs[index].chord;
        add3(
            origin,
            add3(
                scale3(xg_local, x_nondim * chord),
                scale3(zg_local, z_nondim * chord),
            ),
        )
    }

    /// The local `(xg, yg, zg)` reference frame of cross-section `index`, in
    /// geometry axes -- `Wing._compute_frame_of_WingXSec`.
    ///
    /// The root and tip cross-sections take their spanwise (`yg`) direction
    /// from the one adjacent segment they have; an interior cross-section
    /// averages its two adjacent segments and scales `zg` by
    /// `sqrt(2 / (1 + cos(angle between them)))` so the frame remains
    /// consistent across a change in local sweep or dihedral. Both then twist
    /// about `yg` by the cross-section's own twist angle.
    fn frame_of_xsec(&self, index: usize) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let last = self.xsecs.len() - 1;

        let (yg_local, z_scale) = if index == 0 {
            let vector = sub3(self.xsecs[1].xyz_le, self.xsecs[0].xyz_le);
            (project_to_yz_and_normalize(vector), 1.0)
        } else if index == last {
            let vector = sub3(self.xsecs[last].xyz_le, self.xsecs[last - 1].xyz_le);
            (project_to_yz_and_normalize(vector), 1.0)
        } else {
            let vector_before = project_to_yz_and_normalize(sub3(
                self.xsecs[index].xyz_le,
                self.xsecs[index - 1].xyz_le,
            ));
            let vector_after = project_to_yz_and_normalize(sub3(
                self.xsecs[index + 1].xyz_le,
                self.xsecs[index].xyz_le,
            ));
            let span_vector = scale3(add3(vector_before, vector_after), 0.5);
            let yg_local = scale3(span_vector, 1.0 / norm3(span_vector));
            let cos_vectors = dot3(vector_before, vector_after);
            let z_scale = (2.0 / (cos_vectors + 1.0)).sqrt();
            (yg_local, z_scale)
        };

        let xg_local: [f64; 3] = [1.0, 0.0, 0.0];
        let zg_local = scale3(cross3(xg_local, yg_local), z_scale);

        let rotation = rotation_matrix_3d(self.xsecs[index].twist * PI / 180.0, yg_local);
        let xg_local = matvec3(rotation, xg_local);
        let zg_local = matvec3(rotation, zg_local);

        (xg_local, yg_local, zg_local)
    }

    /// Each lofted section's span, projected onto the YZ plane -- the
    /// internal `_sectional=True, type="yz"` path both [`Wing::span`] and
    /// [`Wing::area`] read.
    fn sectional_spans_yz(&self) -> Vec<f64> {
        self.quarter_chord_points()
            .windows(2)
            .map(|pair| {
                let dy = pair[1][1] - pair[0][1];
                let dz = pair[1][2] - pair[0][2];
                (dy * dy + dz * dz).sqrt()
            })
            .collect()
    }

    /// The wing's span, root to tip, doubled if [`Wing::symmetric`] --
    /// `Wing.span()` at its defaults (`type="yz"`, no centerline distance,
    /// not sectional).
    pub fn span(&self) -> f64 {
        let half_span: f64 = self.sectional_spans_yz().iter().sum();
        if self.symmetric {
            2.0 * half_span
        } else {
            half_span
        }
    }

    /// Each lofted section's planform area -- the internal
    /// `_sectional=True, type="planform"` path [`Wing::mean_aerodynamic_chord`]
    /// and [`Wing::aerodynamic_center`] both read.
    fn sectional_areas(&self) -> Vec<f64> {
        let spans = self.sectional_spans_yz();
        let chords: Vec<f64> = self.xsecs.iter().map(|xsec| xsec.chord).collect();
        spans
            .iter()
            .enumerate()
            .map(|(index, &span)| span * (chords[index] + chords[index + 1]) / 2.0)
            .collect()
    }

    /// The wing's planform area, doubled if [`Wing::symmetric`] --
    /// `Wing.area()` at its defaults (`type="planform"`, no centerline
    /// distance, not sectional).
    pub fn area(&self) -> f64 {
        let half_area: f64 = self.sectional_areas().iter().sum();
        if self.symmetric {
            2.0 * half_area
        } else {
            half_area
        }
    }

    /// Each lofted section's mean-aerodynamic-chord length, from its taper
    /// ratio -- the shared computation inside `Wing.mean_aerodynamic_chord`
    /// and `Wing.aerodynamic_center`.
    fn sectional_mac_lengths(&self) -> Vec<f64> {
        self.xsecs
            .windows(2)
            .map(|pair| {
                let taper = pair[1].chord / pair[0].chord;
                (2.0 / 3.0) * pair[0].chord * (1.0 + taper + taper * taper) / (1.0 + taper)
            })
            .collect()
    }

    /// The area-weighted mean aerodynamic chord length of the wing --
    /// `Wing.mean_aerodynamic_chord`. See upstream's cited methodology,
    /// <https://core.ac.uk/download/pdf/79175663.pdf>.
    pub fn mean_aerodynamic_chord(&self) -> f64 {
        let areas = self.sectional_areas();
        let macs = self.sectional_mac_lengths();
        let numerator: f64 = macs
            .iter()
            .zip(&areas)
            .map(|(&mac, &area)| mac * area)
            .sum();
        let denominator: f64 = areas.iter().sum();
        numerator / denominator
    }

    /// The area-weighted aerodynamic center of the wing, at `chord_fraction`
    /// of each section's local MAC -- `Wing.aerodynamic_center`. See the
    /// module doc for the un-rotated chordwise offset this reproduces
    /// faithfully from upstream.
    pub fn aerodynamic_center(&self, chord_fraction: f64) -> [f64; 3] {
        let areas = self.sectional_areas();
        let macs = self.sectional_mac_lengths();

        let sectional_acs: Vec<[f64; 3]> = self
            .xsecs
            .windows(2)
            .zip(&macs)
            .map(|(pair, &mac_length)| {
                let taper = pair[1].chord / pair[0].chord;
                let fraction = (1.0 + 2.0 * taper) / (3.0 + 3.0 * taper);
                let mac_le = add3(
                    pair[0].xyz_le,
                    scale3(sub3(pair[1].xyz_le, pair[0].xyz_le), fraction),
                );
                // Upstream's `# TODO rotate this vector by the local twist
                // angle`: the chordwise offset stays on the X axis rather
                // than being turned by the section's twist. See module doc.
                add3(mac_le, [chord_fraction * mac_length, 0.0, 0.0])
            })
            .collect();

        let total_area: f64 = areas.iter().sum();
        let mut center = [0.0; 3];
        for (ac, &area) in sectional_acs.iter().zip(&areas) {
            center = add3(center, scale3(*ac, area));
        }
        center = scale3(center, 1.0 / total_area);

        if self.symmetric {
            center[1] = 0.0;
        }
        center
    }

    /// The ratio of the tip chord to the root chord -- `Wing.taper_ratio`.
    /// Only meaningful for a trapezoidal wing, as upstream notes.
    pub fn taper_ratio(&self) -> f64 {
        let last = self.xsecs.len() - 1;
        self.xsecs[last].chord / self.xsecs[0].chord
    }

    /// The mean sweep angle (in degrees) of the `x_nondim` chordwise station
    /// from root to tip, relative to the X axis -- `Wing.mean_sweep_angle`.
    /// Positive is swept back. Measured directly from the root and tip
    /// cross-sections only, with no regard for the sweep of any
    /// cross-section in between.
    pub fn mean_sweep_angle(&self, x_nondim: f64) -> f64 {
        let last = self.xsecs.len() - 1;
        let root = self.xyz_of_xsec(0, x_nondim, 0.0);
        let tip = self.xyz_of_xsec(last, x_nondim, 0.0);
        let vector = sub3(tip, root);
        let vector_norm = scale3(vector, 1.0 / norm3(vector));
        // `vector_norm[0]` is the sine of the sweep angle, being the dot
        // product of the unit vector with the X axis.
        vector_norm[0].asin().to_degrees()
    }

    /// The total area of the wing's control surfaces -- see the module doc
    /// for why this is always `0.0` in this crate.
    pub fn control_surface_area(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn two_xsec_wing(symmetric: bool) -> Wing {
        Wing::new(
            "Probe",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    #[test]
    fn taper_ratio_is_tip_chord_over_root_chord() {
        let wing = two_xsec_wing(false);
        assert!((wing.taper_ratio() - 0.5).abs() < 1e-15);
    }

    #[test]
    fn subdivide_sections_rejects_a_ratio_below_two() {
        let wing = two_xsec_wing(false);
        assert_eq!(
            wing.subdivide_sections(1),
            Err(SubdivideSectionsError::RatioTooSmall(1))
        );
        assert_eq!(
            wing.subdivide_sections(0),
            Err(SubdivideSectionsError::RatioTooSmall(0))
        );
    }

    #[test]
    fn subdivide_sections_produces_ratio_times_n_minus_one_new_sections_plus_the_tip() {
        let wing = Wing::new(
            "Three",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 5.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([2.0, 10.0, 0.0], 1.0, 0.0, naca("naca2412")),
            ],
            false,
        );
        let subdivided = wing.subdivide_sections(4).expect("valid ratio");
        // Two lofted sections, 4 new xsecs each, plus the unchanged tip.
        assert_eq!(subdivided.xsecs.len(), 2 * 4 + 1);
        assert_eq!(subdivided.xsecs.last(), wing.xsecs.last());
    }

    #[test]
    fn subdivide_sections_reuses_the_shared_airfoil_without_blending() {
        // Same coordinates, same name, but two distinct `Airfoil` values --
        // exercising structural equality rather than a shared reference.
        let a = naca("naca0012");
        let b = naca("naca0012");
        assert_eq!(a, b);
        assert!(!std::ptr::eq(&a, &b));

        let wing = Wing::new(
            "Shared",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, a.clone()),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, b),
            ],
            false,
        );
        let subdivided = wing.subdivide_sections(3).expect("valid ratio");
        for xsec in &subdivided.xsecs {
            assert_eq!(xsec.airfoil, a);
        }
    }

    #[test]
    fn subdivide_sections_first_new_xsec_at_each_boundary_is_the_inner_airfoil_unblended() {
        // span_fractions_along_section[0] is exactly 0 (linspace's forced
        // endpoint), so a_weight == 1 there for every section -- the first
        // new cross-section after each original one always reuses the
        // inner airfoil verbatim, even when the two ends differ.
        let wing = Wing::new(
            "Distinct",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, 0.0, naca("naca2412")),
            ],
            false,
        );
        let subdivided = wing.subdivide_sections(3).expect("valid ratio");
        assert_eq!(subdivided.xsecs[0].airfoil, naca("naca0012"));
    }

    #[test]
    fn span_of_a_straight_symmetric_wing_doubles_the_half_span() {
        let wing = two_xsec_wing(true);
        // Root and tip share Y and Z with the quarter-chord line, so the
        // untwisted span is just the tip's Y coordinate.
        assert!((wing.span() - 20.0).abs() < 1e-9, "span={}", wing.span());
    }

    #[test]
    fn span_of_an_asymmetric_wing_is_the_half_span() {
        let wing = two_xsec_wing(false);
        assert!((wing.span() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn area_scales_with_span_and_average_chord() {
        let wing = two_xsec_wing(false);
        // Untwisted, unswept, no dihedral: sectional span equals the Y
        // separation, so area is span * mean chord.
        let expected = 10.0 * (2.0 + 1.0) / 2.0;
        assert!(
            (wing.area() - expected).abs() < 1e-9,
            "area={}",
            wing.area()
        );
    }

    #[test]
    fn taper_ratio_of_one_leaves_mean_aerodynamic_chord_equal_to_the_chord() {
        let wing = Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!((wing.mean_aerodynamic_chord() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn aerodynamic_center_of_a_symmetric_wing_has_zero_y() {
        let wing = two_xsec_wing(true);
        let ac = wing.aerodynamic_center(0.25);
        assert_eq!(ac[1], 0.0);
    }

    #[test]
    fn aerodynamic_center_y_is_nonzero_for_an_asymmetric_wing_off_the_centerline() {
        let wing = two_xsec_wing(false);
        let ac = wing.aerodynamic_center(0.25);
        assert!(ac[1] > 0.0);
    }

    #[test]
    fn mean_sweep_angle_is_zero_for_an_unswept_wing() {
        // A rectangular wing (constant chord, aligned leading edges) has
        // every chordwise station's root-to-tip vector pointing straight
        // along Y, at any x_nondim.
        let wing = Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 10.0, 0.0], 2.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!(wing.mean_sweep_angle(0.25).abs() < 1e-9);
    }

    #[test]
    fn mean_sweep_angle_is_positive_for_a_wing_swept_aft() {
        let wing = Wing::new(
            "Swept",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, naca("naca0012")),
                WingXSec::new([5.0, 10.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            false,
        );
        assert!(wing.mean_sweep_angle(0.25) > 0.0);
    }

    #[test]
    fn control_surface_area_is_always_zero() {
        let wing = two_xsec_wing(true);
        assert_eq!(wing.control_surface_area(), 0.0);
    }

    #[test]
    fn translate_shifts_every_xsec_leading_edge() {
        let wing = two_xsec_wing(false);
        let translated = wing.translate([5.0, -2.0, 1.0]);
        for (original, moved) in wing.xsecs.iter().zip(&translated.xsecs) {
            assert_eq!(moved.xyz_le, add3(original.xyz_le, [5.0, -2.0, 1.0]));
            assert_eq!(moved.chord, original.chord);
            assert_eq!(moved.twist, original.twist);
        }
    }
}
