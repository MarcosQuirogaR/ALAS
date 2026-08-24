// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/geometry/aircraft_builder.py
// Reference: alas @ rust-port-baseline.

//! Turns a `(DesignVector, GeometryConfig)` pair into an [`Airplane`] --
//! `AircraftBuilder`. This is ALAS's own assembly logic, not a translation of
//! a third-party library, which is why it lives here rather than under
//! aircraft model: every geometry decision reads from the config objects, and the
//! only literals in this module are structural (which xsec gets which
//! offset, in what order the wings and fuselages are assembled), not tunable
//! values.
//!
//! [`AircraftBuilder::build`] is where every other module in this crate
//! meets: [`crate::airfoil_library::AirfoilLibrary::get`] resolves the three
//! configured airfoil names (through all three of its branches on the
//! default aircraft -- see `docs/PORTING.md`'s Geometry section),
//! [`crate::airfoil_library::build_section`] shapes the root section from the
//! design vector, and [`crate::aircraft::wing::Wing`] /
//! [`crate::aircraft::fuselage::Fuselage`] loft the results into the returned
//! [`Airplane`].
//!
//! # Engine placement's two branches
//!
//! [`AircraftBuilder::build_engines`] has two cases: a centerline
//! tail-mounted engine (`y_pos == 0.0`, e.g. a trijet's tail engine) and a
//! wing-mounted engine, whose Z placement interpolates the wing's dihedral
//! between the root, break and tip stations. `alas-config::geometry`'s
//! default [`alas_config::EngineConfig`] carries two wing-mounted positions
//! (`9.8`, `-9.8`) and no centerline one, so this module's fixture
//! (`golden/geom/builder.json`) exercises only the wing-mounted branch; the
//! tail-mounted branch is covered by a unit test built on a synthetic
//! `y_pos == 0.0` position instead, since no default configuration reaches
//! it.
//!
//! # `sinspace`
//!
//! [`sinspace`] is duplicated here rather than shared with
//! `aircraft::spacing::linspace`/`cosspace`: that module is private to the aircraft model, and
//! `crate::airfoil_library` and `crate::wing_structure::support` already
//! establish the pattern of a small private copy per consumer rather than
//! widening the aircraft model's visibility for one helper (see either module's own
//! `linspace` for the precedent).

use alas_config::{DesignVector, GeometryConfig, TransportPlanform, TransportPlanformError};
use alas_math::CubicSplineError;

use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::airplane::Airplane;
use crate::aircraft::fuselage::{Fuselage, FuselageXSec, FuselageXSecError, DEFAULT_SHAPE};
use crate::aircraft::wing::{SpacingFunction, SubdivideSectionsError, Wing, WingXSec};
use crate::airfoil_library::{build_section, AirfoilLibrary};

/// Why [`AircraftBuilder::build`] could not assemble an [`Airplane`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BuildError {
    /// [`AirfoilLibrary::get`] did not resolve a name the geometry
    /// configuration names. Every airfoil `alas-config::geometry` can name is
    /// checked (`docs/PORTING.md`, Geometry) to resolve through one of that
    /// method's three branches, so this is not reached by this program's own
    /// configuration -- it exists because the lookup is fallible, not
    /// because a real input takes it.
    #[error("airfoil {0:?} did not resolve")]
    UnresolvedAirfoil(String),
    /// Shaping a wing section (`build_section`'s `repanel` step) failed.
    #[error(transparent)]
    Section(#[from] CubicSplineError),
    /// Subdividing a wing's cross-sections failed: an `n_subdivisions` below
    /// 2, or a blend between two distinct airfoils that failed to repanel.
    #[error(transparent)]
    Subdivide(#[from] SubdivideSectionsError),
    /// A fuselage cross-section's radius/width/height combination was
    /// invalid. [`AircraftBuilder`] always supplies exactly one of the two
    /// forms, so this is not reachable from this module's own calls; see
    /// [`crate::aircraft::fuselage::FuselageXSecError`].
    #[error(transparent)]
    FuselageXSec(#[from] FuselageXSecError),
    /// The configured transport planform has invalid stations, chords, or
    /// sweep angles.
    #[error(transparent)]
    Planform(#[from] TransportPlanformError),
}

/// Geometry behavior selected by an [`AircraftBuilder`] construction path.
///
/// [`Self::Product`] places an outboard nacelle on the outboard segment of a
/// cranked wing. [`Self::ReferenceCompatibility`] retains the frozen Python
/// inlet station, which extrapolates the inboard sweep beyond the break and
/// is needed only when replaying reference artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeometryContract {
    Product,
    ReferenceCompatibility,
}

/// Builds parametric aircraft from design variables and a geometry scaffold
/// -- `AircraftBuilder`.
pub struct AircraftBuilder {
    /// The geometry scaffold every build reads from.
    pub geometry: GeometryConfig,
    geometry_contract: GeometryContract,
}

impl AircraftBuilder {
    /// A new builder over `geometry`, defaulting to [`GeometryConfig::default`]
    /// when `None` -- `AircraftBuilder.__init__`.
    ///
    /// Applies the engine spec once here, not on every [`Self::build`] call,
    /// which would repeat the engine-table lookup on every evaluation.
    pub fn new(geometry: Option<GeometryConfig>) -> Self {
        let mut geometry = geometry.unwrap_or_default();
        geometry.engine.apply_engine_spec();
        Self {
            geometry,
            geometry_contract: GeometryContract::Product,
        }
    }

    /// Construct a builder that replays frozen Python geometry artifacts.
    ///
    /// This restores the frozen root/break/tip planform and historical
    /// outboard-nacelle X station. Product builders use [`Self::new`] and the
    /// active side-of-body/kink transport planform instead.
    pub fn new_reference_compatibility(geometry: Option<GeometryConfig>) -> Self {
        let mut geometry = geometry.unwrap_or_default();
        geometry.engine.apply_engine_spec();
        // The frozen Python builder has only root/break/tip stations and
        // derives outboard sweep from the decrement. Reference artifacts must
        // retain that family even though product defaults use the explicit
        // transport planform.
        geometry.wing.side_of_body_span_fraction = None;
        geometry.wing.side_of_body_chord_ratio = None;
        geometry.wing.kink_span_fraction = None;
        geometry.wing.outboard_le_sweep_deg = None;
        Self {
            geometry,
            geometry_contract: GeometryContract::ReferenceCompatibility,
        }
    }

    /// Assemble the full aircraft for `dv` -- `AircraftBuilder.build`.
    ///
    /// `dv = None` builds the nominal reference aircraft
    /// ([`DesignVector::default`]). `include_engines` controls whether the
    /// nacelles from [`Self::build_engines`] are appended to the fuselage
    /// list.
    ///
    /// # Errors
    ///
    /// See [`BuildError`].
    pub fn build(
        &self,
        dv: Option<&DesignVector>,
        include_engines: bool,
    ) -> Result<Airplane, BuildError> {
        let default_dv = DesignVector::default();
        let dv = dv.unwrap_or(&default_dv);
        let g = &self.geometry;

        let root_airfoil = Self::resolve(&g.wing.root_airfoil)?;
        let root_section = build_section(dv, &root_airfoil.coordinates)?;
        let tip_airfoil = Self::resolve(&g.wing.tip_airfoil)?;
        let tail_airfoil = Self::resolve(&g.empennage.tail_airfoil)?;
        let planform = g.wing.transport_planform(dv)?;

        let wings = vec![
            self.build_main_wing(dv, &planform, &root_section, &tip_airfoil)?,
            self.build_hstab(dv, &tail_airfoil)?,
            self.build_vstab(dv, &tail_airfoil)?,
        ];

        let mut fuselages = vec![self.build_fuselage(dv)?];
        if include_engines {
            fuselages.extend(self.build_engines(dv)?);
        }

        let main_wing = &wings[0];
        let mac = main_wing.mean_aerodynamic_chord();
        // Initial CG seed (~quarter-MAC); refined later by the autobalance step.
        let x_wing_global = g.wing.root_datum_x_m + dv.wing_x_shift_m;
        let x_cg_seed = x_wing_global + planform.kink.leading_edge_x_m + 0.25 * mac;

        Ok(Airplane {
            name: "ALAS Aircraft".to_owned(),
            xyz_ref: [x_cg_seed, 0.0, 0.0],
            s_ref: main_wing.area(),
            c_ref: mac,
            b_ref: dv.span_m,
            wings,
            fuselages,
        })
    }

    /// [`AirfoilLibrary::get`], turning `None` into [`BuildError::UnresolvedAirfoil`].
    fn resolve(name: &str) -> Result<Airfoil, BuildError> {
        AirfoilLibrary::get(name).ok_or_else(|| BuildError::UnresolvedAirfoil(name.to_owned()))
    }

    /// The main wing: root/break/tip cross-sections, translated to the wing's
    /// fuselage-station datum and subdivided -- `_build_main_wing`.
    fn build_main_wing(
        &self,
        dv: &DesignVector,
        planform: &TransportPlanform,
        root_section: &Airfoil,
        tip_airfoil: &Airfoil,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.wing;
        let x_wing_global = g.root_datum_x_m + dv.wing_x_shift_m;
        let mut xsecs = vec![WingXSec::new(
            [
                planform.root.leading_edge_x_m,
                planform.root.y_m,
                g.root_z_m,
            ],
            planform.root.chord_m,
            g.root_twist_deg,
            root_section.clone(),
        )];
        if let Some(side_of_body) = planform
            .side_of_body
            .filter(|station| side_of_body_changes_loft(planform, *station))
        {
            let root_to_kink_fraction = side_of_body.y_m / planform.kink.y_m;
            let side_of_body_z_m = g.root_z_m + root_to_kink_fraction * (g.break_z_m - g.root_z_m);
            let side_of_body_twist_deg =
                g.root_twist_deg + root_to_kink_fraction * (g.break_twist_deg - g.root_twist_deg);
            xsecs.push(WingXSec::new(
                [
                    side_of_body.leading_edge_x_m,
                    side_of_body.y_m,
                    side_of_body_z_m,
                ],
                side_of_body.chord_m,
                side_of_body_twist_deg,
                root_section.clone(),
            ));
        }
        xsecs.push(WingXSec::new(
            [
                planform.kink.leading_edge_x_m,
                planform.kink.y_m,
                g.break_z_m,
            ],
            planform.kink.chord_m,
            g.break_twist_deg,
            root_section.clone(),
        ));
        xsecs.push(WingXSec::new(
            [planform.tip.leading_edge_x_m, planform.tip.y_m, g.tip_z_m],
            planform.tip.chord_m,
            dv.tip_twist_deg,
            tip_airfoil.clone(),
        ));

        let wing = Wing::new("Main Wing", xsecs, true);
        let wing = wing
            .translate([x_wing_global, 0.0, 0.0])
            .subdivide_sections(
                n_subdivisions_usize(g.n_subdivisions),
                SpacingFunction::Linspace,
            )?;
        Ok(wing)
    }

    /// The horizontal stabilizer: root/tip cross-sections at the design
    /// vector's tail scale, translated aft to the tail datum --
    /// `_build_hstab`.
    fn build_hstab(&self, dv: &DesignVector, tail_airfoil: &Airfoil) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale;
        let x_hstab = (dv.fuselage_length_m - g.hstab_offset_from_tail_m) + dv.tail_x_shift_m;
        let (tip_x, tip_y, tip_z) = g.hstab_tip_le_m;

        let wing = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new(
                    [0.0, 0.0, 0.0],
                    g.hstab_root_chord_m * ts,
                    g.hstab_root_twist_deg,
                    tail_airfoil.clone(),
                ),
                // Only the in-plane (x, y) tip offset scales with the tail
                // scale; the vertical placement does not -- `tip_le[2]`
                // reproduced unscaled from the Python source.
                WingXSec::new(
                    [tip_x * ts, tip_y * ts, tip_z],
                    g.hstab_tip_chord_m * ts,
                    g.hstab_tip_twist_deg,
                    tail_airfoil.clone(),
                ),
            ],
            true,
        );
        let wing = wing
            .translate([x_hstab, 0.0, g.hstab_z_m])
            .subdivide_sections(
                n_subdivisions_usize(g.n_subdivisions),
                SpacingFunction::Linspace,
            )?;
        Ok(wing)
    }

    /// The vertical stabilizer: root/tip cross-sections at the design
    /// vector's tail scale, translated aft to the tail datum --
    /// `_build_vstab`.
    fn build_vstab(&self, dv: &DesignVector, tail_airfoil: &Airfoil) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale;
        let x_vstab = (dv.fuselage_length_m - g.vstab_offset_from_tail_m) + dv.tail_x_shift_m;
        let (tip_x, tip_y, tip_z) = g.vstab_tip_le_m;

        let wing = Wing::new(
            "Vertical Stabilizer",
            vec![
                WingXSec::new(
                    [0.0, 0.0, 0.0],
                    g.vstab_root_chord_m * ts,
                    0.0,
                    tail_airfoil.clone(),
                ),
                // Only the in-plane (x, z) tip offset scales with the tail
                // scale; the spanwise placement does not -- `tip_le[1]`
                // reproduced unscaled from the Python source (the fin grows
                // in Z, not Y).
                WingXSec::new(
                    [tip_x * ts, tip_y, tip_z * ts],
                    g.vstab_tip_chord_m * ts,
                    0.0,
                    tail_airfoil.clone(),
                ),
            ],
            false,
        );
        let wing = wing
            .translate([x_vstab, 0.0, g.vstab_z_m])
            .subdivide_sections(
                n_subdivisions_usize(g.n_subdivisions),
                SpacingFunction::Linspace,
            )?;
        Ok(wing)
    }

    /// The fuselage body of revolution (or ovoid) -- `_build_fuselage`.
    ///
    /// Ten `sinspace`-spaced nose stations resolving the curved ellipsoid
    /// rounding, two cabin stations, and ten `linspace`-spaced tailcone
    /// stations resolving the taper and upsweep. `is_ovoid` (a declared
    /// height that differs from the diameter, e.g. an A380-style double
    /// decker) switches every station between a circular radius and a
    /// width/height pair scaled proportionally from the same equivalent
    /// circular radius.
    fn build_fuselage(&self, dv: &DesignVector) -> Result<Fuselage, BuildError> {
        let g = &self.geometry.fuselage;
        let radius = g.diameter_m / 2.0;
        let fus_len = dv.fuselage_length_m;
        let cabin_end = fus_len - g.tailcone_length_m;

        let make_xsec = |x_val: f64, z_val: f64, r_val: f64| -> Result<FuselageXSec, BuildError> {
            let xsec = match g.height_m {
                Some(height_m) if height_m != g.diameter_m => {
                    let local_width = r_val * 2.0;
                    let local_height = r_val * 2.0 * (height_m / g.diameter_m);
                    FuselageXSec::new(
                        [x_val, 0.0, z_val],
                        None,
                        Some(local_width),
                        Some(local_height),
                        DEFAULT_SHAPE,
                    )
                }
                _ => FuselageXSec::new([x_val, 0.0, z_val], Some(r_val), None, None, DEFAULT_SHAPE),
            }?;
            Ok(xsec)
        };

        let mut stations = Vec::new();
        // Exclude the last point: it is the cabin start, added below.
        let x_nose = sinspace(0.0, 1.0, 10);
        for &xi in &x_nose[..x_nose.len() - 1] {
            let z_val = g.cabin_z_m + (g.nose_z_m - g.cabin_z_m) * (1.0 - xi).powi(2);
            let r_val = radius * (1.0 - (1.0 - xi).powi(2)).sqrt();
            stations.push(make_xsec(xi * g.cabin_start_x_m, z_val, r_val)?);
        }

        stations.push(make_xsec(g.cabin_start_x_m, g.cabin_z_m, radius)?);
        stations.push(make_xsec(cabin_end, g.cabin_z_m, radius)?);

        // Exclude the first point: it is the cabin end, added above.
        let x_tail = linspace(0.0, 1.0, 10);
        for &xi in &x_tail[1..] {
            let z_val = g.cabin_z_m + (g.tail_z_m - g.cabin_z_m) * xi.powf(1.5);
            let r_val = radius * (1.0 - xi.powf(1.5));
            stations.push(make_xsec(
                cabin_end + xi * g.tailcone_length_m,
                z_val,
                r_val,
            )?);
        }

        Ok(Fuselage::new("Fuselage", stations))
    }

    /// The podded engines, one small [`Fuselage`] per spanwise position --
    /// `_build_engines`. See the module doc for the two placement branches.
    fn build_engines(&self, dv: &DesignVector) -> Result<Vec<Fuselage>, BuildError> {
        let g = &self.geometry.engine;
        let x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        let wing = &self.geometry.wing;
        let planform = wing.transport_planform(dv)?;

        let mut nacelles = Vec::with_capacity(g.spanwise_positions_m.len());
        for &y_pos in &g.spanwise_positions_m {
            let (name, x_inlet, z_nacelle) = if y_pos == 0.0 {
                // Centerline tail-mounted engine (e.g. a trijet's tail
                // engine), placed on top of the tailcone at the base of the
                // vstab.
                let x_inlet = dv.fuselage_length_m - g.nacelle_length_m();
                let z_nacelle = self.geometry.fuselage.tail_z_m + g.radius_scale_m + 0.2;
                ("Nacelle Center".to_owned(), x_inlet, z_nacelle)
            } else {
                let name = if y_pos > 0.0 {
                    "Nacelle R"
                } else {
                    "Nacelle L"
                }
                .to_owned();
                let leading_edge_offset = match self.geometry_contract {
                    GeometryContract::Product => planform.leading_edge_x_at(y_pos.abs())?,
                    GeometryContract::ReferenceCompatibility => {
                        y_pos.abs() * dv.sweep_deg.to_radians().tan()
                    }
                };
                let x_inlet = x_wing_global + leading_edge_offset - g.inlet_x_offset_m;

                // Interpolate the local wing Z-height for dihedral-aware
                // placement, then apply the local Z offset relative to the
                // wing LE. The `1e-9` denominators guard a zero-length root-
                // or tip-side interval (`y_break == 0` or
                // `semi_span == y_break`), not an incidental epsilon.
                let semi_span = planform.tip.y_m;
                let y_break = planform.kink.y_m;
                let y_abs = y_pos.abs();
                let z_wing = if y_abs <= y_break {
                    wing.root_z_m + (wing.break_z_m - wing.root_z_m) * (y_abs / (y_break + 1e-9))
                } else {
                    wing.break_z_m
                        + (wing.tip_z_m - wing.break_z_m)
                            * ((y_abs - y_break) / (semi_span - y_break + 1e-9))
                };
                (name, x_inlet, z_wing + g.z_m)
            };

            let mut xsecs = Vec::with_capacity(g.nacelle_profile.len());
            for &(x, r) in &g.nacelle_profile {
                xsecs.push(FuselageXSec::new(
                    [x, 0.0, 0.0],
                    Some(g.radius_scale_m * r),
                    None,
                    None,
                    DEFAULT_SHAPE,
                )?);
            }
            nacelles.push(Fuselage::new(name, xsecs).translate([x_inlet, y_pos, z_nacelle]));
        }
        Ok(nacelles)
    }
}

/// Whether a side-of-body station changes the physical loft rather than only
/// naming a point on the straight root-to-kink panel.
///
/// Keeping a collinear bookkeeping station out of the VLM mesh avoids adding
/// an entire extra subdivision block to every candidate and every live-preview
/// camera update, while a genuinely cranked body fairing is still retained.
fn side_of_body_changes_loft(
    planform: &TransportPlanform,
    side_of_body: alas_config::MainWingStation,
) -> bool {
    let fraction = side_of_body.y_m / planform.kink.y_m;
    let interpolated_le = planform.root.leading_edge_x_m
        + fraction * (planform.kink.leading_edge_x_m - planform.root.leading_edge_x_m);
    let interpolated_chord =
        planform.root.chord_m + fraction * (planform.kink.chord_m - planform.root.chord_m);
    (side_of_body.leading_edge_x_m - interpolated_le).abs() > 1e-10
        || (side_of_body.chord_m - interpolated_chord).abs() > 1e-10
}

/// `n_subdivisions` clamped to `usize`, so a negative or overflowing
/// configuration value becomes `0` -- which [`Wing::subdivide_sections`]
/// rejects with [`SubdivideSectionsError::RatioTooSmall`], the same outcome
/// Python's `ratio < 2` guard produces for a negative `ratio` -- rather than
/// wrapping to a huge unsigned value on the `as` cast.
fn n_subdivisions_usize(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// Evenly spaced points from `start` to `stop`, inclusive -- NumPy's
/// `linspace(start, stop, num, endpoint=True)`. Duplicated from
/// `aircraft::spacing::linspace`; see the module doc.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// Sine-spaced points from `start` to `stop`, bunched near `start` --
/// `native aerodynamic model.numpy.spacing.sinspace` at its default `reverse_spacing =
/// False`: `start + (stop - start) * (1 - cos(linspace(0, pi/2, num)))`, with
/// both endpoints then forced exact to correct the trigonometric round trip,
/// exactly as upstream's own endpoint fixup does.
fn sinspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    let mut spaced: Vec<f64> = linspace(0.0, std::f64::consts::FRAC_PI_2, num)
        .into_iter()
        .map(|t| start + (stop - start) * (1.0 - t.cos()))
        .collect();
    spaced[0] = start;
    let last = spaced.len() - 1;
    spaced[last] = stop;
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluding_engines_leaves_exactly_one_fuselage() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let airplane = builder
            .build(None, false)
            .expect("the default aircraft builds cleanly");
        assert_eq!(airplane.fuselages.len(), 1);
        assert_eq!(airplane.fuselages[0].name, "Fuselage");
    }

    #[test]
    fn including_engines_appends_one_nacelle_per_spanwise_position() {
        let geometry = GeometryConfig::default();
        let expected_engine_count = geometry.engine.spanwise_positions_m.len();
        let builder = AircraftBuilder::new(Some(geometry));
        let airplane = builder
            .build(None, true)
            .expect("the default aircraft builds cleanly");
        assert_eq!(airplane.fuselages.len(), 1 + expected_engine_count);
    }

    #[test]
    fn a_centerline_engine_is_named_and_placed_on_the_tailcone() {
        // No default configuration reaches `y_pos == 0.0` -- see the module
        // doc -- so this is exercised directly on a synthetic geometry.
        let mut geometry = GeometryConfig::default();
        geometry.engine.spanwise_positions_m = vec![0.0];
        let builder = AircraftBuilder::new(Some(geometry));
        let dv = DesignVector::default();

        let nacelles = builder
            .build_engines(&dv)
            .expect("a single centerline engine builds cleanly");
        assert_eq!(nacelles.len(), 1);
        assert_eq!(nacelles[0].name, "Nacelle Center");

        let expected_z =
            builder.geometry.fuselage.tail_z_m + builder.geometry.engine.radius_scale_m + 0.2;
        // The nacelle's own xsecs are all at local x/y/z == 0 before
        // translation, so its first xsec's absolute Z is the translation.
        assert!((nacelles[0].xsecs[0].xyz_c[2] - expected_z).abs() < 1e-9);
    }

    #[test]
    fn wing_mounted_engines_are_named_by_which_side_they_are_on() {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let dv = DesignVector::default();
        let nacelles = builder
            .build_engines(&dv)
            .expect("the default two wing-mounted engines build cleanly");
        assert_eq!(nacelles.len(), 2);
        assert_eq!(nacelles[0].name, "Nacelle R");
        assert_eq!(nacelles[1].name, "Nacelle L");
    }

    #[test]
    fn an_outboard_nacelle_follows_the_continuous_leading_edge_sweep() {
        let mut geometry = GeometryConfig::default();
        let dv = DesignVector::default();
        let semi_span = dv.span_m / 2.0;
        let y_outboard = 0.8 * semi_span;
        geometry.engine.spanwise_positions_m = vec![y_outboard];
        let builder = AircraftBuilder::new(Some(geometry));
        let planform = builder
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the default planform is valid");

        let nacelles = builder
            .build_engines(&dv)
            .expect("outboard nacelle placement is valid");
        let expected = builder.geometry.wing.root_datum_x_m
            + planform
                .leading_edge_x_at(y_outboard)
                .expect("the nacelle is inside the planform")
            - builder.geometry.engine.inlet_x_offset_m;
        let inboard_extrapolation = builder.geometry.wing.root_datum_x_m
            + y_outboard * dv.sweep_deg.to_radians().tan()
            - builder.geometry.engine.inlet_x_offset_m;

        assert!(y_outboard > planform.kink.y_m);
        assert!((nacelles[0].xsecs[0].xyz_c[0] - expected).abs() < 1e-12);
        assert!((expected - inboard_extrapolation).abs() < 1e-12);
    }

    #[test]
    fn explicit_side_of_body_station_is_lofted_into_the_main_wing() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.side_of_body_span_fraction = Some(0.10);
        geometry.wing.side_of_body_chord_ratio = Some(0.90);
        geometry.wing.kink_span_fraction = Some(0.40);
        geometry.wing.outboard_le_sweep_deg = Some(28.0);
        geometry.wing.n_subdivisions = 2;
        let builder = AircraftBuilder::new(Some(geometry));
        let dv = DesignVector::default();
        let planform = builder
            .geometry
            .wing
            .transport_planform(&dv)
            .expect("the transport planform is valid");

        let airplane = builder
            .build(Some(&dv), false)
            .expect("the explicit transport planform builds");
        let main_wing = &airplane.wings[0];

        // Three original lofted panels at a subdivision ratio of two yield
        // six sections plus the unchanged tip. The side-of-body station is
        // the first section of the second panel.
        assert_eq!(main_wing.xsecs.len(), 7);
        assert!((main_wing.xsecs[2].xyz_le[1] - planform.side_of_body.unwrap().y_m).abs() < 1e-12);
        assert!((main_wing.xsecs[2].chord - planform.side_of_body.unwrap().chord_m).abs() < 1e-12);
        assert!((main_wing.xsecs[4].xyz_le[1] - planform.kink.y_m).abs() < 1e-12);
        assert!(
            (main_wing.xsecs[6].xyz_le[0]
                - (builder.geometry.wing.root_datum_x_m + planform.tip.leading_edge_x_m))
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn a_collinear_side_of_body_station_does_not_add_a_vlm_subdivision_block() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.n_subdivisions = 2;
        let builder = AircraftBuilder::new(Some(geometry));

        let airplane = builder
            .build(Some(&DesignVector::default()), false)
            .expect("the default transport planform builds");

        // Root/kink/tip is two physical panels. The derived side-of-body
        // station lies on the inboard panel and therefore adds no mesh block.
        assert_eq!(airplane.wings[0].xsecs.len(), 5);
    }

    #[test]
    fn reference_compatibility_retains_the_frozen_nacelle_station() {
        let dv = DesignVector {
            span_m: 35.8,
            root_chord_m: 6.5,
            break_chord_m: 4.2,
            tip_chord_m: 1.8,
            sweep_deg: 25.0,
            fuselage_length_m: 37.5,
            ..DesignVector::default()
        };
        let reference =
            AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
        let y_outboard = reference.geometry.engine.spanwise_positions_m[0].abs();
        let reference_inlet = reference
            .build_engines(&dv)
            .expect("reference nacelles build")[0]
            .xsecs[0]
            .xyz_c[0];
        let frozen_inlet = reference.geometry.wing.root_datum_x_m
            + y_outboard * dv.sweep_deg.to_radians().tan()
            - reference.geometry.engine.inlet_x_offset_m;

        assert!((reference_inlet - frozen_inlet).abs() < 1e-12);
    }

    #[test]
    fn sinspace_bunches_points_near_the_start() {
        let values = sinspace(0.0, 1.0, 11);
        assert_eq!(values[0], 0.0);
        assert_eq!(values[10], 1.0);
        let first_gap = values[1] - values[0];
        let last_gap = values[10] - values[9];
        assert!(first_gap < last_gap, "{first_gap} >= {last_gap}");
    }

    #[test]
    fn n_subdivisions_usize_clamps_a_negative_configuration_value_to_zero() {
        assert_eq!(n_subdivisions_usize(-1), 0);
        assert_eq!(n_subdivisions_usize(8), 8);
    }

    #[test]
    fn a_ratio_below_two_from_configuration_is_a_build_error_not_a_panic() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.n_subdivisions = 1;
        let builder = AircraftBuilder::new(Some(geometry));
        let error = builder
            .build(None, false)
            .expect_err("n_subdivisions=1 must be rejected, not silently truncated");
        assert!(matches!(error, BuildError::Subdivide(_)));
    }
}
