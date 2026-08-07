// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from aerosandbox/geometry/fuselage.py
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! AeroSandbox's `Fuselage` and `FuselageXSec`, scoped to the surface
//! `alas-geom::builder` and `alas-mass::torenbeek` (both later modules)
//! actually call: the `xsecs`/`name` fields, `FuselageXSec.xyz_c`/`.width`/
//! `.height`, `.translate(...)` on both classes, and `Fuselage.area_wetted()`.
//! `docs/PORTING.md` records the scoping decision and a prior grep across all
//! of `alas/`, not just `alas/geometry/`, for what reaches these two classes
//! -- both `FuselageXSec`'s `radius=` and `width=`/`height=` constructor
//! branches are reached (`_build_fuselage`'s circular and ovoid cases), and
//! both the main-fuselage build (no `.translate()` on the `Fuselage` itself)
//! and the per-nacelle build (`.translate([x, y, z])`, each nacelle a
//! separate small `Fuselage`) are reached.
//!
//! Left untranslated, because nothing in this program's inputs reaches them:
//! `area_projected`, `area_base`, `fineness_ratio`, `length`, `volume`,
//! `x_centroid_projected`, `mesh_body`, `mesh_line`, `draw*`,
//! `subdivide_sections`, `add_loft` (itself `raise NotImplementedError`
//! upstream), `FuselageXSec.xsec_area`, `.compute_frame`,
//! `.get_3D_coordinates`, `.equivalent_radius`, and the `xyz_normal`,
//! `color` and `analysis_specific_options` fields of either class.
//! `FuselageXSec.xsec_perimeter` is translated as a private helper of
//! [`Fuselage::area_wetted`], the one thing that reads it upstream too.

use super::vector3::add3;

/// The default `shape` a `FuselageXSec` is constructed with when nothing
/// overrides it -- `FuselageXSec.__init__`'s `shape: float = 2.0`, a
/// superellipse exponent of 2 being an ordinary ellipse (a circle, when
/// `width == height`).
pub const DEFAULT_SHAPE: f64 = 2.0;

/// [`FuselageXSec::new`] rejects specifying both `radius` and
/// (`width`, `height`), and rejects specifying neither -- the same two
/// conditions upstream's two `raise ValueError(...)`s guard, restated as a
/// typed error since this crate does not panic (`CONTRIBUTING.md`).
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum FuselageXSecError {
    /// Both `radius` and one or both of `width`/`height` were given.
    #[error(
        "cannot specify both `radius` and (`width`, `height`) parameters - \
         must be one or the other"
    )]
    RadiusAndWidthHeight,
    /// Neither `radius` nor both of `width` and `height` were given.
    #[error("must specify either `radius` or both (`width`, `height`) parameters")]
    NeitherRadiusNorWidthHeight,
}

/// A fuselage cross-section: center point, width, height and superellipse
/// shape exponent -- `FuselageXSec`, scoped to the fields this program uses
/// (no normal vector, no analysis-specific options; see the module doc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageXSec {
    /// The center of this cross-section, in geometry axes.
    pub xyz_c: [f64; 3],
    /// The cross-section's width (its extent along the local Y axis).
    pub width: f64,
    /// The cross-section's height (its extent along the local Z axis).
    pub height: f64,
    /// The superellipse shape exponent. Stored but not otherwise used here
    /// -- see the module doc for what queries it (none, in this program's
    /// current scope).
    pub shape: f64,
}

impl FuselageXSec {
    /// A new cross-section at `xyz_c`, sized by either `radius` (which sets
    /// `width = height = 2 * radius`) or by `width` and `height` directly --
    /// `FuselageXSec.__init__`'s `radius`/`width`/`height` validation.
    ///
    /// # Errors
    ///
    /// [`FuselageXSecError::RadiusAndWidthHeight`] if `radius` is `Some` and
    /// either `width` or `height` is also `Some`.
    /// [`FuselageXSecError::NeitherRadiusNorWidthHeight`] if `radius` is
    /// `None` and `width`/`height` are not both `Some`.
    pub fn new(
        xyz_c: [f64; 3],
        radius: Option<f64>,
        width: Option<f64>,
        height: Option<f64>,
        shape: f64,
    ) -> Result<Self, FuselageXSecError> {
        match (radius, width, height) {
            (Some(r), None, None) => Ok(Self {
                xyz_c,
                width: 2.0 * r,
                height: 2.0 * r,
                shape,
            }),
            (Some(_), _, _) => Err(FuselageXSecError::RadiusAndWidthHeight),
            (None, Some(w), Some(h)) => Ok(Self {
                xyz_c,
                width: w,
                height: h,
                shape,
            }),
            (None, _, _) => Err(FuselageXSecError::NeitherRadiusNorWidthHeight),
        }
    }

    /// A copy of this cross-section translated by `xyz` -- `FuselageXSec.translate`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            xyz_c: add3(self.xyz_c, xyz),
            ..*self
        }
    }

    /// This cross-section's perimeter ("circumference" for a circular
    /// section) -- `FuselageXSec.xsec_perimeter`, a closed-form
    /// symbolic-regression fit to the exact (infinite-series) superellipse
    /// perimeter. See upstream's docstring for the fit's derivation and
    /// stated error bounds (at most 0.2%, over `1 <= shape < infinity`).
    fn perimeter(&self) -> f64 {
        if self.width == 0.0 {
            return 2.0 * self.height;
        }
        if self.height == 0.0 {
            return 2.0 * self.width;
        }

        let s = self.shape;
        let h = ((self.width + 1e-16) / (self.height + 1e-16))
            .max((self.height + 1e-16) / (self.width + 1e-16));

        let term = (s - 0.884_870_77) * h + 0.258_857_4 / h;
        let exponent = (s / -0.900_692_05).exp();
        let nondim_quadrant_perimeter =
            h + (term.powf(exponent) + h + 0.099_197_85).powf(-1.481_229_3 / s);

        2.0 * nondim_quadrant_perimeter * self.width.min(self.height)
    }
}

/// A fuselage (or other slender body: pod, fuel tank, nacelle): a name and an
/// ordered list of cross-sections -- `Fuselage`, scoped to the fields and
/// methods this program uses (see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Fuselage {
    /// The fuselage's name, e.g. `"Fuselage"` or `"Nacelle R"` -- later
    /// modules distinguish a nacelle from the main fuselage by substring
    /// matching on this field.
    pub name: String,
    /// Cross-sections from nose to tail. Lofted linearly between adjacent
    /// pairs.
    pub xsecs: Vec<FuselageXSec>,
}

impl Fuselage {
    /// A new fuselage named `name`, holding `xsecs`.
    pub fn new(name: impl Into<String>, xsecs: Vec<FuselageXSec>) -> Self {
        Self {
            name: name.into(),
            xsecs,
        }
    }

    /// A copy of this fuselage translated by `xyz` -- `Fuselage.translate`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            name: self.name.clone(),
            xsecs: self.xsecs.iter().map(|xsec| xsec.translate(xyz)).collect(),
        }
    }

    /// The fuselage's wetted area: each adjacent pair of stations'
    /// perimeters, trapezoidally integrated along X -- `Fuselage.area_wetted`.
    pub fn area_wetted(&self) -> f64 {
        let perimeters: Vec<f64> = self.xsecs.iter().map(FuselageXSec::perimeter).collect();
        let mut area = 0.0;
        for i in 0..self.xsecs.len().saturating_sub(1) {
            let x_separation = self.xsecs[i + 1].xyz_c[0] - self.xsecs[i].xyz_c[0];
            area += (perimeters[i] + perimeters[i + 1]) / 2.0 * x_separation;
        }
        area
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_radius_sets_width_and_height_to_the_diameter() {
        let xsec = FuselageXSec::new([1.0, 2.0, 3.0], Some(1.5), None, None, DEFAULT_SHAPE)
            .expect("radius alone is valid");
        assert_eq!(xsec.width, 3.0);
        assert_eq!(xsec.height, 3.0);
    }

    #[test]
    fn from_width_and_height_keeps_them_distinct() {
        let xsec = FuselageXSec::new([0.0, 0.0, 0.0], None, Some(4.0), Some(2.5), DEFAULT_SHAPE)
            .expect("width and height together are valid");
        assert_eq!(xsec.width, 4.0);
        assert_eq!(xsec.height, 2.5);
    }

    #[test]
    fn rejects_radius_together_with_width_or_height() {
        assert_eq!(
            FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), Some(2.0), None, DEFAULT_SHAPE),
            Err(FuselageXSecError::RadiusAndWidthHeight)
        );
        assert_eq!(
            FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, Some(2.0), DEFAULT_SHAPE),
            Err(FuselageXSecError::RadiusAndWidthHeight)
        );
        assert_eq!(
            FuselageXSec::new(
                [0.0, 0.0, 0.0],
                Some(1.0),
                Some(2.0),
                Some(3.0),
                DEFAULT_SHAPE
            ),
            Err(FuselageXSecError::RadiusAndWidthHeight)
        );
    }

    #[test]
    fn rejects_neither_radius_nor_a_complete_width_height_pair() {
        assert_eq!(
            FuselageXSec::new([0.0, 0.0, 0.0], None, None, None, DEFAULT_SHAPE),
            Err(FuselageXSecError::NeitherRadiusNorWidthHeight)
        );
        assert_eq!(
            FuselageXSec::new([0.0, 0.0, 0.0], None, Some(2.0), None, DEFAULT_SHAPE),
            Err(FuselageXSecError::NeitherRadiusNorWidthHeight)
        );
        assert_eq!(
            FuselageXSec::new([0.0, 0.0, 0.0], None, None, Some(2.0), DEFAULT_SHAPE),
            Err(FuselageXSecError::NeitherRadiusNorWidthHeight)
        );
    }

    #[test]
    fn translating_an_xsec_twice_equals_translating_by_the_sum() {
        let xsec = FuselageXSec::new([1.0, -1.0, 0.5], Some(2.0), None, None, DEFAULT_SHAPE)
            .expect("radius alone is valid");
        let twice = xsec.translate([1.0, 2.0, 3.0]).translate([4.0, -1.0, 0.5]);
        let once = xsec.translate([5.0, 1.0, 3.5]);
        assert_eq!(twice.xyz_c, once.xyz_c);
        assert_eq!(twice.width, once.width);
        assert_eq!(twice.height, once.height);
    }

    #[test]
    fn area_wetted_of_a_uniform_cylinder_is_close_to_circumference_times_length() {
        // `xsec_perimeter`'s fit is stated accurate to -0.1% at shape=2 (a
        // circle), not exact -- see the method's doc comment -- so this
        // checks the port lands within that stated bound rather than
        // asserting bit-exact agreement with the closed-form circumference.
        let fuselage = Fuselage::new(
            "Cylinder",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
                FuselageXSec::new([10.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        );
        let expected = 2.0 * std::f64::consts::PI * 1.0 * 10.0;
        let relative_error = (fuselage.area_wetted() - expected).abs() / expected;
        assert!(
            relative_error < 0.002,
            "area_wetted={}, expected={expected}",
            fuselage.area_wetted()
        );
    }

    #[test]
    fn area_wetted_of_a_single_station_is_zero() {
        let fuselage = Fuselage::new(
            "Point",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        );
        assert_eq!(fuselage.area_wetted(), 0.0);
    }

    #[test]
    fn translating_a_fuselage_shifts_every_xsec_center_and_preserves_extent() {
        let fuselage = Fuselage::new(
            "Nacelle R",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(1.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
                FuselageXSec::new([2.0, 0.0, 0.0], Some(0.8), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        );
        let translated = fuselage.translate([10.0, -5.0, 3.0]);
        assert_eq!(translated.name, fuselage.name);
        for (original, moved) in fuselage.xsecs.iter().zip(&translated.xsecs) {
            assert_eq!(moved.xyz_c, add3(original.xyz_c, [10.0, -5.0, 3.0]));
            assert_eq!(moved.width, original.width);
            assert_eq!(moved.height, original.height);
        }
    }
}
