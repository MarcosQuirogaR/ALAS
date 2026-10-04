// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turns a `(DesignVector, GeometryConfig)` pair into an [`Airplane`]:
//! `AircraftBuilder`. This is ALAS's own assembly logic, not a translation of
//! a third-party library, which is why it lives here rather than under
//! aircraft model: every geometry decision reads from the config objects, and the
//! only literals in this module are structural (which xsec gets which
//! offset, in what order the wings and fuselages are assembled), not tunable
//! values.
//!
//! [`AircraftBuilder::build`] is where every other module in this crate
//! meets: [`crate::airfoil_library::AirfoilLibrary::get`] resolves the three
//! configured airfoil names (through all four of its branches on the
//! default aircraft, see `docs/PORTING.md`'s Geometry section),
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
//! [`sinspace`] lives here rather than beside `aircraft::spacing::linspace`
//! and `cosspace` because the builder is its only caller; it is built on that
//! module's `linspace`.

mod custom_sections;
mod empennage;
mod error;
mod fin_root;
mod mesh;
mod spacing;
mod tail_attachment;
#[cfg(test)]
mod tests;

use crate::aircraft::spacing::linspace;
pub use error::BuildError;
use spacing::{n_subdivisions_usize, sinspace};

use alas_config::{DesignVector, GeometryConfig, TransportPlanform, WingHeights, WingShape};

use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::airplane::Airplane;
use crate::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use crate::aircraft::wing::{Wing, WingXSec};
use crate::airfoil_library::{build_section, AirfoilLibrary};

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

/// Builds parametric aircraft from design variables and a geometry scaffold:
/// `AircraftBuilder`.
pub struct AircraftBuilder {
    /// The geometry scaffold every build reads from.
    pub geometry: GeometryConfig,
    geometry_contract: GeometryContract,
}

impl AircraftBuilder {
    /// A new builder over `geometry`, defaulting to [`GeometryConfig::default`]
    /// when `None`: `AircraftBuilder.__init__`.
    ///
    /// Product geometry consumes the live engine configuration verbatim.
    /// Engine selection is resolved when a preset is selected; reapplying the
    /// database entry here would silently erase later engine-designer edits.
    pub fn new(geometry: Option<GeometryConfig>) -> Self {
        let geometry = geometry.unwrap_or_default();
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
        // The frozen builder lofts one wing shape, the configured heights.
        geometry.wing.flight_tip_rise_semispan_fraction = None;
        // And the tailcone loft: it has no belly upsweep.
        geometry.fuselage.belly_upsweep_length_m = None;
        // And its spanwise mesh; see `mesh`.
        mesh::restore_reference_ratios(&mut geometry);
        Self {
            geometry,
            geometry_contract: GeometryContract::ReferenceCompatibility,
        }
    }

    /// Assemble the full aircraft for `dv`, its main wing in the 1 g flight
    /// shape: `AircraftBuilder.build`.
    ///
    /// `dv = None` builds the nominal reference aircraft
    /// ([`DesignVector::default`]). `include_engines` controls whether the
    /// nacelles from [`Self::build_engines`] are appended to the fuselage
    /// list. The flight shape is the one every aerodynamic, stability and
    /// layout consumer reads; see [`WingShape`].
    ///
    /// # Errors
    ///
    /// See [`BuildError`].
    pub fn build(
        &self,
        dv: Option<&DesignVector>,
        include_engines: bool,
    ) -> Result<Airplane, BuildError> {
        self.build_shape(dv, include_engines, WingShape::Flight)
    }

    /// [`Self::build`] with the main wing and its nacelles in `shape`: the
    /// static ground shape is the one ground clearance and nacelle strike
    /// are measured on.
    ///
    /// # Errors
    ///
    /// See [`BuildError`].
    pub fn build_shape(
        &self,
        dv: Option<&DesignVector>,
        include_engines: bool,
        shape: WingShape,
    ) -> Result<Airplane, BuildError> {
        let default_dv = DesignVector::default();
        let dv = dv.unwrap_or(&default_dv);
        let g = &self.geometry;

        let root_airfoil = Self::resolve(&g.wing.root_airfoil)?;
        let root_section = build_section(dv, &root_airfoil.coordinates)?;
        let tip_airfoil = Self::resolve(&g.wing.tip_airfoil)?;
        let tail_airfoil = Self::resolve(&g.empennage.tail_airfoil)?;
        let planform = g.wing.transport_planform(dv)?;
        let heights = g.wing.heights(shape, &planform)?;

        let main_wing =
            self.build_main_wing(dv, &planform, &heights, &root_section, &tip_airfoil)?;
        let fuselage = self.build_fuselage(dv)?;
        let seat = self.fin_seat(dv, &fuselage)?;
        let wings = vec![
            main_wing,
            self.build_hstab(dv, &tail_airfoil, true)?,
            self.build_vstab(dv, &tail_airfoil, true, &seat)?,
        ];

        let mut fuselages = vec![fuselage];
        if include_engines {
            fuselages.extend(self.build_engines(dv, shape)?);
        }

        let main_wing = &wings[0];
        // Projected (XY) MAC, matching `s_ref`/`b_ref` below; the
        // reference-compatibility contract keeps the unfolded chord
        // instead, since its golden fixture pins Python's own unfolded
        // `airplane.c_ref` (see that fixture's generator).
        let mac = match self.geometry_contract {
            GeometryContract::Product => main_wing.mac_station().chord_m,
            GeometryContract::ReferenceCompatibility => main_wing.mean_aerodynamic_chord(),
        };
        // Initial CG seed (~quarter-MAC); refined later by the autobalance step.
        let x_wing_global = g.wing.root_datum_x_m + dv.wing_x_shift_m;
        let x_cg_seed = x_wing_global + planform.kink.leading_edge_x_m + 0.25 * mac;

        Ok(Airplane {
            name: "ALAS Aircraft".to_owned(),
            xyz_ref: [x_cg_seed, 0.0, 0.0],
            // Aircraft reference quantities use one convention (product
            // path): planform, span and chord projected onto the aircraft
            // XY reference plane, the design-vector/preset convention and
            // the reference-plane convention audit S-0025 notes; audit
            // F-0030 measures the nominal dihedral mismatch at about 0.9%.
            // The wing model still exposes `area()`/`span()`/
            // `mean_aerodynamic_chord()` as unfolded compatibility
            // quantities for frozen translation fixtures.
            s_ref: main_wing.reference_area(),
            c_ref: mac,
            b_ref: main_wing.reference_span(),
            wings,
            fuselages,
        })
    }

    /// [`AirfoilLibrary::get`], turning `None` into [`BuildError::UnresolvedAirfoil`].
    fn resolve(name: &str) -> Result<Airfoil, BuildError> {
        AirfoilLibrary::get(name).ok_or_else(|| BuildError::UnresolvedAirfoil(name.to_owned()))
    }

    /// The main wing: root/break/tip cross-sections, translated to the wing's
    /// fuselage-station datum and subdivided, `_build_main_wing`.
    fn build_main_wing(
        &self,
        dv: &DesignVector,
        planform: &TransportPlanform,
        heights: &WingHeights,
        root_section: &Airfoil,
        tip_airfoil: &Airfoil,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.wing;
        let x_wing_global = g.root_datum_x_m + dv.wing_x_shift_m;
        let mut xsecs = vec![WingXSec::new(
            [
                planform.root.leading_edge_x_m,
                planform.root.y_m,
                heights.root_z_m,
            ],
            planform.root.chord_m,
            g.root_twist_deg,
            root_section.clone(),
        )];
        // A derived side-of-body station lies on the straight root-to-kink
        // panel (its trailing-edge clip is millimetres), and meshing it only
        // when that clip happens to be active made the lattice section count
        // depend on the design vector: adjacent candidates differed by eight
        // sections and by fifteen percent in induced drag. Only an explicitly
        // cranked station changes the loft, so only that one is meshed.
        if let Some(side_of_body) = planform
            .side_of_body
            .filter(|_| g.side_of_body_chord_ratio.is_some())
        {
            let root_to_kink_fraction = side_of_body.y_m / planform.kink.y_m;
            let side_of_body_z_m =
                heights.root_z_m + root_to_kink_fraction * (heights.break_z_m - heights.root_z_m);
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
                heights.break_z_m,
            ],
            planform.kink.chord_m,
            g.break_twist_deg,
            root_section.clone(),
        ));
        xsecs.push(WingXSec::new(
            [
                planform.tip.leading_edge_x_m,
                planform.tip.y_m,
                heights.tip_z_m,
            ],
            planform.tip.chord_m,
            dv.tip_twist_deg,
            tip_airfoil.clone(),
        ));
        self.append_custom_wing_sections(planform, heights, &mut xsecs)?;

        let wing = Wing::new("Main Wing", xsecs, true);
        let wing = mesh::for_contract(
            self.geometry_contract,
            &wing.translate([x_wing_global, 0.0, 0.0]),
            g.n_subdivisions,
        )?;
        Ok(wing)
    }

    /// The fuselage body of revolution (or ovoid): `_build_fuselage`.
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
        g.validate_generated_sections()?;
        let radius = g.diameter_m / 2.0;
        let fus_len = dv.fuselage_length_m;
        let cabin_end = g.aft_body_start_m(fus_len);

        let local_height_scale = g.height_m.map_or(1.0, |height_m| height_m / g.diameter_m);
        let mut generated_index = 0usize;
        let mut make_xsec = |x_val: f64,
                             z_val: f64,
                             width_m: f64,
                             height_m: f64|
         -> Result<FuselageXSec, BuildError> {
            let override_section = g.generated_sections.get(generated_index);
            let (z_m, width_m, height_m, shape) =
                override_section.map_or((z_val, width_m, height_m, DEFAULT_SHAPE), |section| {
                    (
                        section.z_m,
                        section.width_m,
                        section.height_m,
                        section.shape,
                    )
                });
            generated_index += 1;
            Ok(FuselageXSec::new(
                [x_val, 0.0, z_m],
                None,
                Some(width_m),
                Some(height_m),
                shape,
            )?)
        };

        let mut stations = Vec::new();
        // Exclude the last point: it is the cabin start, added below.
        let x_nose = sinspace(0.0, 1.0, 10);
        for &xi in &x_nose[..x_nose.len() - 1] {
            let z_val = g.cabin_z_m + (g.nose_z_m - g.cabin_z_m) * (1.0 - xi).powi(2);
            let r_val = radius * (1.0 - (1.0 - xi).powi(2)).sqrt();
            stations.push(make_xsec(
                xi * g.cabin_start_x_m,
                z_val,
                r_val * 2.0,
                r_val * 2.0 * local_height_scale,
            )?);
        }

        stations.push(make_xsec(
            g.cabin_start_x_m,
            g.cabin_z_m,
            radius * 2.0,
            radius * 2.0 * local_height_scale,
        )?);
        stations.push(make_xsec(
            cabin_end,
            g.cabin_z_m,
            radius * 2.0,
            radius * 2.0 * local_height_scale,
        )?);

        // Exclude the first point: it is the cabin end, added above.
        let x_tail = linspace(0.0, 1.0, 10);
        for &xi in &x_tail[1..] {
            let station = g.aft_body_station(xi, fus_len);
            stations.push(make_xsec(
                station.x_m,
                station.z_m,
                station.width_m,
                station.height_m,
            )?);
        }
        self.append_custom_fuselage_sections(fus_len, &mut stations)?;

        Ok(Fuselage::new("Fuselage", stations))
    }

    /// The podded engines, one small [`Fuselage`] per spanwise position:
    /// `_build_engines`. See the module doc for the two placement branches.
    fn build_engines(
        &self,
        dv: &DesignVector,
        shape: WingShape,
    ) -> Result<Vec<Fuselage>, BuildError> {
        let g = &self.geometry.engine;
        let x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        let wing = &self.geometry.wing;
        let planform = wing.transport_planform(dv)?;
        let heights = wing.heights(shape, &planform)?;

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

                // The local leading-edge height of the wing shape being
                // built, so the nacelle hangs under the wing its pylon is
                // on; then the local Z offset relative to that leading edge.
                let z_wing = heights.at(&planform, y_pos);
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
