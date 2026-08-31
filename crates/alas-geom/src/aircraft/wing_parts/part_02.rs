// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    /// `ratio` smaller ones, by inserting cross-sections interpolated along
    /// each interval at the stations `spacing_function` gives -- `Wing.
    /// subdivide_sections`. See the module doc for which callers use which
    /// [`SpacingFunction`].
    ///
    /// # Errors
    ///
    /// [`SubdivideSectionsError::RatioTooSmall`] if `ratio < 2`.
    /// [`SubdivideSectionsError::Blend`] if a subdivision boundary needs two
    /// structurally distinct airfoils blended together and that blend's
    /// `repanel` step fails.
    pub fn subdivide_sections(
        &self,
        ratio: usize,
        spacing_function: SpacingFunction,
    ) -> Result<Self, SubdivideSectionsError> {
        if ratio < 2 {
            return Err(SubdivideSectionsError::RatioTooSmall(ratio));
        }

        // `spacing_function(0, 1, ratio + 1)[:-1]`: `ratio` fractions
        // covering [0, 1) -- the final fraction (exactly 1) is dropped
        // because the outer xsec it would reproduce is appended separately,
        // unchanged, after the loop.
        let full = spacing_function.spaced(0.0, 1.0, ratio + 1);
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
    ///
    /// `pub(crate)` rather than private: [`super::mesh`] reuses this exact
    /// coordinate computation for `mesh_line` rather than duplicating it.
    pub(crate) fn xyz_of_xsec(&self, index: usize, x_nondim: f64, z_nondim: f64) -> [f64; 3] {
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
    ///
    /// `pub(crate)`, for the same reason as [`Wing::xyz_of_xsec`], which is
    /// built on this.
    pub(crate) fn frame_of_xsec(&self, index: usize) -> ([f64; 3], [f64; 3], [f64; 3]) {
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

    /// The wing's unfolded geometric span, root to tip, doubled if
    /// [`Wing::symmetric`] -- the compatibility `Wing.span()` quantity at its
    /// defaults (`type="yz"`, no centerline distance, not sectional).
    ///
    /// This is the distance along the loft's YZ quarter-chord path.  It is
    /// retained for translation/parity callers, but it is not the aircraft
    /// reference span.  Product/reference normalization uses
    /// [`Self::reference_span`] instead.
    pub fn span(&self) -> f64 {
        self.unfolded_span()
    }

    /// Unfolded span along the loft's YZ quarter-chord path.
    ///
    /// This named form makes the legacy compatibility quantity explicit at
    /// call sites that intentionally replay the upstream geometry API.
    pub fn unfolded_span(&self) -> f64 {
        let half_span: f64 = self.sectional_spans_yz().iter().sum();
        if self.symmetric {
            2.0 * half_span
        } else {
            half_span
        }
    }

    /// Wing span projected onto the aircraft lateral axis.
    ///
    /// Manufacturer three-views and reference-plane definitions quote this
    /// span.  Unlike [`Self::span`], it does not grow with dihedral.
    pub fn projected_span(&self) -> f64 {
        let half_span: f64 = self
            .xsecs
            .windows(2)
            .map(|pair| (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs())
            .sum();
        if self.symmetric {
            2.0 * half_span
        } else {
            half_span
        }
    }

    /// Authoritative aircraft reference span.
    ///
    /// ALAS uses the lateral (XY-plane) projected span for `Airplane::b_ref`,
    /// aerodynamic coefficient normalization, and source/reference
    /// comparisons.  [`Self::unfolded_span`] remains available only for the
    /// frozen compatibility geometry path.
    pub fn reference_span(&self) -> f64 {
        self.projected_span()
    }

    /// Reference-plane area projected onto the aircraft XY plane.
    ///
    /// This deliberately ignores dihedral rather than changing the legacy
    /// [`Self::area`] quantity, whose `yz` convention is pinned to the
    /// reference geometry parity fixture.
    pub fn projected_area(&self) -> f64 {
        let half_area: f64 = self
            .xsecs
            .windows(2)
            .map(|pair| {
                let span = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
                span * (pair[0].chord + pair[1].chord) / 2.0
            })
            .sum();
        if self.symmetric {
            2.0 * half_area
        } else {
            half_area
        }
    }

    /// Authoritative aircraft reference area.
    ///
    /// ALAS uses the wing planform projected onto the aircraft XY reference
    /// plane for `Airplane::s_ref`, aerodynamic coefficient normalization, and
    /// manufacturer/source comparisons.  [`Self::unfolded_area`] remains
    /// available only for the frozen compatibility geometry path.
    pub fn reference_area(&self) -> f64 {
        self.projected_area()
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

    /// The wing's unfolded planform area, doubled if [`Wing::symmetric`] --
    /// compatibility `Wing.area()` at its defaults (`type="planform"`, no
    /// centerline distance, not sectional).
    ///
    /// This integrates the YZ quarter-chord path and therefore includes the
    /// dihedral-induced unfolding.  It is retained for translation/parity
    /// callers; product/reference normalization uses [`Self::reference_area`].
    pub fn area(&self) -> f64 {
        self.unfolded_area()
    }

    /// Unfolded planform area measured on the loft's YZ path.
    pub fn unfolded_area(&self) -> f64 {
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

    /// The geometric aspect ratio, `span^2 / area` -- `Wing.aspect_ratio` at
    /// its default `type="geometric"`; the `effective` branch is unreached and
    /// untranslated, like every other non-default argument here.
    pub fn aspect_ratio(&self) -> f64 {
        self.span() * self.span() / self.area()
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
