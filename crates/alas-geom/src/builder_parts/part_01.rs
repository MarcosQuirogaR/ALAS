// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
            // Aircraft reference quantities use one convention: planform and
            // span projected onto the aircraft XY reference plane.  This is
            // the design-vector/preset convention (published planform area
            // calibration) and the standard reference-plane convention noted
            // by the NASA VLM evidence in audit S-0025; audit F-0030 measures
            // the nominal dihedral mismatch at about 0.9%.  The wing model
            // still exposes `area()`/`span()` as named unfolded compatibility
            // quantities for frozen translation fixtures, but they must not
            // feed `s_ref`/`b_ref`.
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
