// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/geometry/fuselage.py
// Upstream: reference geometry 4.2.8, MIT.

//! reference geometry's `Fuselage` and `FuselageXSec`, scoped to the surface
//! `alas-geom::builder` and `alas-mass::torenbeek` (both later modules)
//! actually call: the `xsecs`/`name` fields, `FuselageXSec.xyz_c`/`.width`/
//! `.height`, `.translate(...)` on both classes, and `Fuselage.area_wetted()`.
//! `docs/PORTING.md` records the scoping decision and a prior grep across all
//! of `alas/`, not just `alas/geometry/`, for what reaches these two classes,
//! both `FuselageXSec`'s `radius=` and `width=`/`height=` constructor
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

use std::sync::Arc;

use super::vector3::add3;
use crate::loft::Loft;

/// The default `shape` a `FuselageXSec` is constructed with when nothing
/// overrides it: `FuselageXSec.__init__`'s `shape: float = 2.0`, a
/// superellipse exponent of 2 being an ordinary ellipse (a circle, when
/// `width == height`).
pub const DEFAULT_SHAPE: f64 = 2.0;

/// Axial run of a closing disc in [`Fuselage::closed_stations`], m: one
/// millimetre, as the other steps of the station view, so stations keep
/// strictly increasing in x.
pub const END_DISC_RUN_M: f64 = 1.0e-3;
/// Stations of a rounded fuselage tail end.
const END_DOME_STATIONS: usize = 8;
/// A forward end ring below this fraction of the widest section is a spinner
/// tip. Inlet highlights are 0.7 to 0.9, spinner tips below 0.2.
const SPINNER_WIDTH_FRACTION: f64 = 0.5;
/// A section thinner than this, m, is already a point.
const POINT_SIZE_M: f64 = 1.0e-9;

/// [`FuselageXSec::new`] rejects specifying both `radius` and
/// (`width`, `height`), and rejects specifying neither: the same two
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
/// shape exponent: `FuselageXSec`, scoped to the fields this program uses
/// (no normal vector, no analysis-specific options; see the module doc).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FuselageXSec {
    /// The center of this cross-section, m in geometry axes.
    pub xyz_c: [f64; 3],
    /// The cross-section's width, m (its extent along the local Y axis).
    pub width: f64,
    /// The cross-section's height, m (its extent along the local Z axis).
    pub height: f64,
    /// The superellipse shape exponent. Stored but not otherwise used here,
    /// see the module doc for what queries it (none, in this program's
    /// current scope).
    pub shape: f64,
}

impl FuselageXSec {
    /// A new cross-section at `xyz_c`, sized by either `radius` (which sets
    /// `width = height = 2 * radius`) or by `width` and `height` directly:
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

    /// A copy of this cross-section translated by `xyz`: `FuselageXSec.translate`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            xyz_c: add3(self.xyz_c, xyz),
            ..*self
        }
    }

    /// Whether this section has no open ring to close.
    fn is_point(&self) -> bool {
        self.width <= POINT_SIZE_M || self.height <= POINT_SIZE_M
    }

    /// The zero-size section on this one's station.
    fn as_point(&self) -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            ..*self
        }
    }

    /// This cross-section's perimeter ("circumference" for a circular
    /// section): `FuselageXSec.xsec_perimeter`, a closed-form
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

/// A scarfed inlet: the rings of the lip lean, the upper lip forward, and come
/// upright at the crest. Only a surface that is drawn or exported sees it; the
/// sections the analyses read stay normal to the axis.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct LipScarf {
    /// Station of the inlet ring, m.
    pub highlight_x_m: f64,
    /// Axial run over which the lean fades to zero, m.
    pub blend_m: f64,
    /// Axial shift per metre below the top of the inlet ring.
    pub slope: f64,
}

impl LipScarf {
    /// The lean of the ring at station `x`, axial metres per metre of height.
    #[must_use]
    pub fn lean_at(&self, x: f64) -> f64 {
        self.slope * (1.0 - (x - self.highlight_x_m) / self.blend_m).clamp(0.0, 1.0)
    }
}

/// A fuselage (or other slender body: pod, fuel tank, nacelle): a name and an
/// ordered list of cross-sections: `Fuselage`, scoped to the fields and
/// methods this program uses (see the module doc).
///
/// A body built from a [`Loft`] also carries it: `xsecs` is then the dense
/// legacy view of the loft ([`Loft::sample_xsecs`]) that consumers which read
/// stations keep using, and `loft` serves the exact queries.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct Fuselage {
    /// The fuselage's name, e.g. `"Fuselage"` or `"Nacelle R"`: later
    /// modules distinguish a nacelle from the main fuselage by substring
    /// matching on this field.
    pub name: String,
    /// Cross-sections from nose to tail. Lofted linearly between adjacent
    /// pairs.
    pub xsecs: Vec<FuselageXSec>,
    /// The lofted surface the stations were sampled from, if any.
    pub loft: Option<Arc<Loft>>,
    /// A separate small cone attached behind the lofted hull (the stepped APU
    /// exhaust cone), its own body of two stations; the hull curves stay smooth
    /// and the stations of this cone are also the last two of `xsecs`.
    pub apu_cone: Option<Arc<Fuselage>>,
    /// The scarfed inlet of a nacelle, if any.
    pub lip_scarf: Option<LipScarf>,
    /// A nacelle hung from the fuselage (a rear-fuselage pair or a tail-centre
    /// pod) rather than from a wing, as the builder placed it. False for the
    /// main fuselage, a wing pod and a body read back without the attribute.
    /// Not part of the `Debug` layout: it is a placement attribute, not geometry.
    pub fuselage_mounted: bool,
}

/// The derived layout, with `apu_cone` shown only when there is one: bodies without
/// an APU cone print exactly as before the field existed (geometry hashes of
/// the registered presets are taken over this text).
impl std::fmt::Debug for Fuselage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Fuselage");
        s.field("name", &self.name)
            .field("xsecs", &self.xsecs)
            .field("loft", &self.loft);
        if let Some(cone) = &self.apu_cone {
            s.field("apu_cone", cone);
        }
        if let Some(scarf) = &self.lip_scarf {
            s.field("lip_scarf", scarf);
        }
        s.finish()
    }
}

impl Fuselage {
    /// A new fuselage named `name`, holding `xsecs` and no loft.
    pub fn new(name: impl Into<String>, xsecs: Vec<FuselageXSec>) -> Self {
        Self {
            name: name.into(),
            xsecs,
            loft: None,
            apu_cone: None,
            lip_scarf: None,
            fuselage_mounted: false,
        }
    }

    /// A copy of this fuselage translated by `xyz`: `Fuselage.translate`.
    /// The loft, if any, is translated with it.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            name: self.name.clone(),
            xsecs: self.xsecs.iter().map(|xsec| xsec.translate(xyz)).collect(),
            loft: self
                .loft
                .as_ref()
                .map(|loft| Arc::new(loft.translated(xyz))),
            apu_cone: self
                .apu_cone
                .as_ref()
                .map(|cone| Arc::new(cone.translate(xyz))),
            lip_scarf: self.lip_scarf.map(|scarf| LipScarf {
                highlight_x_m: scarf.highlight_x_m + xyz[0],
                ..scarf
            }),
            fuselage_mounted: self.fuselage_mounted,
        }
    }

    /// This body marked as hung from the fuselage.
    #[must_use]
    pub fn mounted_on_fuselage(mut self) -> Self {
        self.fuselage_mounted = true;
        self
    }

    /// The lean of `station`'s ring, axial metres per metre of height; zero
    /// without a scarf.
    #[must_use]
    pub fn ring_lean(&self, station: &FuselageXSec) -> f64 {
        self.lip_scarf
            .map_or(0.0, |scarf| scarf.lean_at(station.xyz_c[0]))
    }

    /// Axial shift, m, of the surface point at height `z` of `station`'s ring:
    /// the top of the ring stays on the station.
    #[must_use]
    pub fn ring_x_shift_m(&self, station: &FuselageXSec, z: f64) -> f64 {
        self.ring_lean(station) * (station.xyz_c[2] + 0.5 * station.height - z)
    }

    /// [`Self::closed_stations`] with each ring's centre where the scarf puts
    /// it, and the ring's tilt about the +y axis, degrees: for whatever places
    /// whole sections, an export. Negative tilt moves the top of the ring
    /// forward (rotation from +z towards +x is positive).
    #[must_use]
    pub fn posed_closed_stations(&self) -> Vec<(FuselageXSec, f64)> {
        self.closed_stations()
            .into_iter()
            .map(|mut station| {
                let lean = self.ring_lean(&station);
                let tilt_deg = -lean.atan().to_degrees();
                station.xyz_c[0] += lean * 0.5 * station.height;
                (station, tilt_deg)
            })
            .collect()
    }

    /// The stations of the closed outer surface, for whatever draws or exports
    /// a solid: `xsecs` with the open end rings closed, at the same overall
    /// length. `xsecs` and the loft stay the open body the analyses read, so the
    /// base area remains a base and no wetted area, volume or ground clearance
    /// moves with this closure.
    ///
    /// * The aft end of a fuselage is rounded: a quarter ellipse over the last
    ///   half of the smaller end dimension, ending in a point on the same
    ///   station.
    /// * The aft end of a nacelle is a flat disc [`END_DISC_RUN_M`] long: a
    ///   nozzle or plug is cut square, its size is engine data.
    /// * A forward end smaller than half the widest section is a spinner tip and
    ///   gets the same disc. A larger one is an inlet lip and stays open: the
    ///   duct is not modelled, and it now looks onto the closed aft end.
    /// * The inlet lip of a nacelle on the plane of symmetry, a trijet's centre
    ///   pod, gets the same disc too, a fan face on the lip: the fin root stands
    ///   inside that shell, and an open lip would show it through the duct.
    #[must_use]
    pub fn closed_stations(&self) -> Vec<FuselageXSec> {
        let mut stations = self.xsecs.clone();
        if stations.len() < 2 {
            return stations;
        }
        let widest = stations.iter().map(|s| s.width).fold(0.0, f64::max);
        let first = stations[0];
        let centre_pod = self.name.contains("Nacelle") && first.xyz_c[1].abs() <= POINT_SIZE_M;
        if !first.is_point() && (first.width < SPINNER_WIDTH_FRACTION * widest || centre_pod) {
            let run = END_DISC_RUN_M.min(0.25 * (stations[1].xyz_c[0] - first.xyz_c[0]));
            stations[0].xyz_c[0] += run;
            stations.insert(0, first.as_point());
        }
        let last = stations[stations.len() - 1];
        if last.is_point() {
            return stations;
        }
        if self.name.contains("Nacelle") {
            let before = stations[stations.len() - 2].xyz_c[0];
            let run = END_DISC_RUN_M.min(0.25 * (last.xyz_c[0] - before));
            if let Some(ring) = stations.last_mut() {
                ring.xyz_c[0] -= run;
            }
            stations.push(last.as_point());
            return stations;
        }
        rounded_aft_end(&stations)
    }

    /// Outer skin chord (full width, m) at station `x` and height `z` (geometry axes): the
    /// single outer-section source of every consumer that places something inside the body
    /// (payload lining = this chord minus twice the drawn lining offset). `None` without a loft.
    #[must_use]
    pub fn skin_chord_m(&self, x: f64, z: f64) -> Option<f64> {
        self.loft
            .as_ref()
            .map(|loft| 2.0 * loft.half_width_at(x, z))
    }

    /// Half-width of the outer skin at station `x` and height `z` (geometry axes), m:
    /// the loft's when there is one, else the superellipse of the stations either side,
    /// interpolated linearly; zero outside the body.
    #[must_use]
    pub fn half_width_at(&self, x: f64, z: f64) -> f64 {
        if let Some(loft) = &self.loft {
            return loft.half_width_at(x, z);
        }
        let Some(pair) = self.xsecs.windows(2).find(|pair| {
            pair[0].xyz_c[0] <= x && x <= pair[1].xyz_c[0] && pair[1].xyz_c[0] > pair[0].xyz_c[0]
        }) else {
            return 0.0;
        };
        let t = (x - pair[0].xyz_c[0]) / (pair[1].xyz_c[0] - pair[0].xyz_c[0]);
        let lerp = |a: f64, b: f64| a + t * (b - a);
        let (width, height) = (
            lerp(pair[0].width, pair[1].width),
            lerp(pair[0].height, pair[1].height),
        );
        let shape = lerp(pair[0].shape, pair[1].shape);
        let u = (2.0 * (z - lerp(pair[0].xyz_c[2], pair[1].xyz_c[2])) / height).abs();
        if !(width > 0.0 && height > 0.0 && shape > 0.0 && u < 1.0) {
            return 0.0;
        }
        0.5 * width * (1.0 - u.powf(shape)).powf(1.0 / shape)
    }

    /// The wetted area of the lofted surface when there is one (exact skin
    /// integral, excluding base faces) plus the separate APU cone, else
    /// [`Self::area_wetted`].
    pub fn area_wetted_lofted(&self) -> f64 {
        let cone = self
            .apu_cone
            .as_ref()
            .map_or(0.0, |cone| cone.area_wetted());
        self.loft
            .as_ref()
            .map_or_else(|| self.area_wetted(), |loft| loft.wetted_area() + cone)
    }

    /// The fuselage's wetted area: each adjacent pair of stations'
    /// perimeters, trapezoidally integrated along X: `Fuselage.area_wetted`.
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

/// `stations` with the part aft of `length - d` scaled by the quarter ellipse
/// `sqrt(1 - t^2)`, `d` being half the smaller end dimension: the end keeps its
/// station and its centre line, and its corners take the radius the end section
/// itself allows.
fn rounded_aft_end(stations: &[FuselageXSec]) -> Vec<FuselageXSec> {
    let Some(&last) = stations.last() else {
        return Vec::new();
    };
    let end_x = last.xyz_c[0];
    let depth = (0.5 * last.width.min(last.height)).min(end_x - stations[0].xyz_c[0]);
    let start_x = end_x - depth;
    let outline_at = |x: f64| -> FuselageXSec {
        for pair in stations.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let span = b.xyz_c[0] - a.xyz_c[0];
            if x <= b.xyz_c[0] && span > 0.0 {
                let t = ((x - a.xyz_c[0]) / span).clamp(0.0, 1.0);
                let lerp = |p: f64, q: f64| p + t * (q - p);
                return FuselageXSec {
                    xyz_c: [
                        x,
                        lerp(a.xyz_c[1], b.xyz_c[1]),
                        lerp(a.xyz_c[2], b.xyz_c[2]),
                    ],
                    width: lerp(a.width, b.width),
                    height: lerp(a.height, b.height),
                    shape: lerp(a.shape, b.shape),
                };
            }
        }
        last
    };
    let mut out: Vec<FuselageXSec> = stations
        .iter()
        .copied()
        .filter(|s| s.xyz_c[0] < start_x)
        .collect();
    for k in 0..=END_DOME_STATIONS {
        // Equal turning angle along the quarter ellipse.
        let angle = std::f64::consts::FRAC_PI_2 * k as f64 / END_DOME_STATIONS as f64;
        let (t, scale) = (angle.sin(), angle.cos());
        let outline = outline_at(start_x + depth * t);
        out.push(if k == END_DOME_STATIONS {
            last.as_point()
        } else {
            FuselageXSec {
                width: outline.width * scale,
                height: outline.height * scale,
                ..outline
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tube(name: &str, first: f64, last: f64) -> Fuselage {
        let ring = |x: f64, d: f64| {
            FuselageXSec::new([x, 1.0, 2.0], Some(0.5 * d), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            name,
            vec![ring(0.0, first), ring(5.0, 2.0), ring(10.0, last)],
        )
    }

    #[test]
    fn a_fuselage_tail_ends_in_a_rounded_point_at_its_own_length() {
        let body = tube("Fuselage", 1.9, 1.0);
        let closed = body.closed_stations();
        let end = closed.last().expect("stations");
        assert_eq!(end.xyz_c, [10.0, 1.0, 2.0]);
        assert_eq!((end.width, end.height), (0.0, 0.0));
        assert!(closed.windows(2).all(|p| p[0].xyz_c[0] < p[1].xyz_c[0]));
        // The outline is untouched ahead of the half-dimension depth.
        let start = closed
            .iter()
            .position(|s| s.xyz_c[0] >= 9.5)
            .expect("dome start");
        assert_eq!(&closed[..start], &body.xsecs[..start]);
        assert!((closed[start].xyz_c[0] - 9.5).abs() < 1e-12);
    }

    #[test]
    fn a_nacelle_ends_in_a_disc_at_its_own_length_and_its_inlet_stays_open() {
        let body = tube("Nacelle R", 1.8, 0.5);
        let closed = body.closed_stations();
        assert_eq!(closed[0], body.xsecs[0]);
        let end = closed[closed.len() - 1];
        let ring = closed[closed.len() - 2];
        assert_eq!(end.xyz_c[0], 10.0);
        assert_eq!((end.width, end.height), (0.0, 0.0));
        assert_eq!(ring.width, 0.5);
        assert!((ring.xyz_c[0] - (10.0 - END_DISC_RUN_M)).abs() < 1e-12);
    }

    #[test]
    fn a_centre_pod_inlet_is_closed_on_its_lip() {
        let mut pod = tube("Nacelle Center", 1.8, 0.5);
        for xsec in &mut pod.xsecs {
            xsec.xyz_c[1] = 0.0;
        }
        let closed = pod.closed_stations();
        assert_eq!((closed[0].width, closed[0].xyz_c[0]), (0.0, 0.0));
        assert_eq!(closed[1].width, 1.8);
        assert!((closed[1].xyz_c[0] - END_DISC_RUN_M).abs() < 1e-12);
        assert!(closed.windows(2).all(|p| p[0].xyz_c[0] < p[1].xyz_c[0]));
    }

    #[test]
    fn a_spinner_tip_is_closed_and_a_point_is_left_alone() {
        let spinner = tube("Nacelle L", 0.2, 0.0).closed_stations();
        assert_eq!((spinner[0].width, spinner[0].xyz_c[0]), (0.0, 0.0));
        assert_eq!(spinner[1].width, 0.2);
        assert_eq!(spinner.len(), 4);
    }

    #[test]
    fn closing_leaves_the_open_body_the_analyses_read() {
        let body = tube("Fuselage", 0.0, 1.0);
        let before = body.xsecs.clone();
        let _ = body.closed_stations();
        assert_eq!(body.xsecs, before);
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
        // circle), not exact (see the method's doc comment) so this
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
