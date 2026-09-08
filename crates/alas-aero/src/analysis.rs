// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/aerodynamics.py
// Reference: alas @ rust-port-baseline.

//! The hybrid aerodynamics engine: an inviscid vortex-lattice solve for what
//! potential flow can answer, plus semi-empirical estimates for what it
//! cannot.
//!
//! [`crate::vlm`] gives lift, induced drag and pitching moment, and is
//! blind to two things that decide a transonic transport's cruise efficiency:
//! skin friction and the shock rise. This module supplies both -- Raymer's
//! flat-plate component buildup in [`AeroAnalysis::parasite_drag`], the Korn
//! equation in [`AeroAnalysis::wave_drag`] -- and hands back a corrected drag
//! polar. Every empirical coefficient comes from `DragModelConfig`; the sweep
//! and the section thickness come from the geometry actually being analysed,
//! not from a constant.
//!
//! Despite the name, this is **not** `alas-aero::drag_buildup`, which is
//! mission analysis model's own buildup reached only from inside the mission network (P6).
//! This is this program's own code and needs nothing but a geometry and an
//! atmosphere.
//!
//! # The compressibility correction, and what it does not touch
//!
//! native aerodynamic model's vortex lattice is incompressible, so at transonic cruise it
//! under-predicts the lift-curve slope and therefore over-predicts the
//! geometric alpha a given `CL` needs -- a supercritical widebody reads 4-6
//! degrees where the real aircraft shows 1-4. [`compressible_report_alpha`]
//! compresses the *reported* angle toward the zero-lift angle by the
//! Prandtl-Glauert factor. Only the angle moves: the drag polar stays on the
//! physical `CL`, and the solve is still evaluated at the incompressible
//! alpha where its `CL` equals the target, so `L/D` is unchanged. Three of
//! this module's entry points apply it and all three do so at the last step,
//! which is what keeps that property visible.
//!
//! # Scope
//!
//! [`AeroAnalysis::trimmed_performance`] takes a `StabilityTrimResult`
//! upstream -- an `alas-stab` type, which is P7 and therefore above this row.
//! It reads exactly three fields off it, so it takes those three here as
//! [`TrimPoint`] and P7 converts. This is the scoping `alas-perf`'s
//! `build_vn_diagram` already uses for its airplane argument: a function that
//! reads three numbers off a type takes three numbers, rather than inverting
//! the phase order to name the type.

mod performance;
mod wetted;

use std::f64::consts::PI;

use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_config::geometry::GeometryConfig;
use alas_config::physics::DragModelConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;

pub use performance::{PolarSweep, QuickPerformance, TrimPoint, TrimmedPerformance};

/// The thickness-to-chord [`AeroAnalysis::section_thickness`] answers when
/// there is no root section to read one off.
///
/// Upstream reaches it through a bare `except` around
/// `plane.wings[0].xsecs[0].airfoil.max_thickness()`, so it is what an
/// airplane with no wings gets. That is not a configuration this program
/// builds, but the value is observable -- [`AeroAnalysis::wave_drag`] is the
/// one consumer of the section thickness that is not itself a sum over wings
/// -- so it is reproduced rather than turned into an error.
const SECTION_THICKNESS_FALLBACK: f64 = 0.12;

/// The number of chordwise stations `Airfoil.max_thickness` samples at, which
/// upstream carries as its `x_over_c_sample=np.linspace(0, 1, 101)` default.
const MAX_THICKNESS_SAMPLES: usize = 101;

/// The `M cos(sweep)` ceiling [`swept_pg_beta`] clamps to, just below the
/// singularity at 1.
const MACH_NORMAL_CEILING: f64 = 0.95;

/// Breakdown of the drag estimate at a single operating point --
/// `DragComponents`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragComponents {
    /// Viscous/parasite drag coefficient, Raymer buildup, viscous margin
    /// included.
    pub cd_parasite: f64,
    /// Lift-induced drag coefficient, as the vortex-lattice solve reported
    /// it.
    pub cd_induced: f64,
    /// Transonic wave-drag coefficient, Korn equation.
    pub cd_wave: f64,
}

impl DragComponents {
    /// The three terms summed, in upstream's own order.
    pub fn cd_total(&self) -> f64 {
        self.cd_parasite + self.cd_induced + self.cd_wave
    }
}

/// The Prandtl-Glauert compressibility factor `beta = sqrt(1 - (M cos(sweep))^2)`
/// on the Mach component normal to the sweep line.
///
/// `M cos(sweep)` is clamped at [`MACH_NORMAL_CEILING`] rather than allowed to
/// reach the singularity at 1: near drag divergence the linear correction has
/// stopped meaning anything, and the Korn rise in [`AeroAnalysis::wave_drag`]
/// is what governs drag there. Upstream also floors the radicand at `1e-3`,
/// which that clamp makes unreachable (the smallest radicand the clamp allows
/// is 0.0975); it is reproduced because removing it would be a change to
/// code, not to behaviour.
pub fn swept_pg_beta(mach: f64, sweep_deg: f64) -> f64 {
    let mach_normal = (mach.abs() * sweep_deg.to_radians().cos().abs()).min(MACH_NORMAL_CEILING);
    (1.0 - mach_normal * mach_normal).max(1e-3).sqrt()
}

/// Compressibility-correct a geometric angle of attack for reporting.
///
/// Prandtl-Glauert steepens the compressible lift curve by `1/beta` about the
/// zero-lift angle, so the angle a given `CL` needs compresses toward
/// `alpha_0l_deg` by `beta`. See the module doc for what this deliberately
/// leaves alone.
pub fn compressible_report_alpha(
    alpha_incompressible_deg: f64,
    alpha_0l_deg: f64,
    mach: f64,
    sweep_deg: f64,
) -> f64 {
    let beta = swept_pg_beta(mach, sweep_deg);
    alpha_0l_deg + beta * (alpha_incompressible_deg - alpha_0l_deg)
}

/// Hybrid vortex-lattice plus empirical-correction analysis of one aircraft
/// -- `AeroAnalysis`.
///
/// Borrows the airplane rather than owning it, because every call site
/// upstream builds one aircraft and analyses it from several angles; a
/// solve that needs a modified copy (only [`AeroAnalysis::trimmed_performance`]
/// does) makes one for the duration of that solve.
#[derive(Debug, Clone)]
pub struct AeroAnalysis<'a> {
    /// The aircraft being analysed.
    pub plane: &'a Airplane,
    /// Quarter-chord sweep of the main wing, in degrees. Supplied by the
    /// caller rather than measured off `plane`, as upstream does: it is the
    /// design variable, and the buildup wants the value that was asked for.
    pub sweep_deg: f64,
    /// Wetted-area factors and the reference fuselage and engine dimensions.
    pub geometry: GeometryConfig,
    /// The empirical drag coefficients.
    pub drag: DragModelConfig,
    /// Mesh resolutions and the probe and sweep angle schedules.
    pub analysis: AnalysisConfig,
    /// Whether to replay the frozen pre-product parasite-drag convention.
    ///
    /// Reference fixtures used one main-wing thickness and design sweep for
    /// every surface. Product analyses use each surface's own geometry.
    reference_compatibility: bool,
}

impl<'a> AeroAnalysis<'a> {
    /// A new analysis of `plane` at `sweep_deg`, each configuration group
    /// defaulting when `None` -- `AeroAnalysis.__init__`.
    pub fn new(
        plane: &'a Airplane,
        sweep_deg: f64,
        geometry: Option<GeometryConfig>,
        drag_model: Option<DragModelConfig>,
        analysis: Option<AnalysisConfig>,
    ) -> Self {
        Self {
            plane,
            sweep_deg,
            geometry: geometry.unwrap_or_default(),
            drag: drag_model.unwrap_or_default(),
            analysis: analysis.unwrap_or_default(),
            reference_compatibility: false,
        }
    }

    /// Construct an analysis that replays the frozen reference drag buildup.
    ///
    /// This seam is for parity fixtures only. Product callers should use
    /// [`Self::new`], which evaluates each wing with its own thickness and
    /// sweep geometry.
    pub fn new_reference_compatibility(
        plane: &'a Airplane,
        sweep_deg: f64,
        geometry: Option<GeometryConfig>,
        drag_model: Option<DragModelConfig>,
        analysis: Option<AnalysisConfig>,
    ) -> Self {
        let mut result = Self::new(plane, sweep_deg, geometry, drag_model, analysis);
        result.reference_compatibility = true;
        result
    }

    /// Compressible turbulent flat-plate skin friction, Prandtl-Schlichting.
    fn turbulent_cf(reynolds: f64, mach: f64) -> f64 {
        0.455 / (reynolds.log10().powf(2.58) * (1.0 + 0.144 * mach * mach).powf(0.65))
    }

    /// The real maximum thickness-to-chord of the morphed root section, or
    /// [`SECTION_THICKNESS_FALLBACK`] where there is no section to read.
    pub fn section_thickness(&self) -> f64 {
        self.plane
            .wings
            .first()
            .map_or(SECTION_THICKNESS_FALLBACK, Self::wing_section_thickness)
    }

    /// The maximum thickness-to-chord of a surface's root section.
    ///
    /// Parasite drag is accumulated surface by surface. A tail can therefore
    /// not inherit the main wing's section thickness merely because the
    /// latter is the first surface in the airplane. Empty surfaces retain the
    /// same observable fallback as [`Self::section_thickness`].
    fn wing_section_thickness(wing: &Wing) -> f64 {
        wing.xsecs
            .first()
            .map_or(SECTION_THICKNESS_FALLBACK, |xsec| {
                xsec.airfoil
                    .max_thickness(&linspace(0.0, 1.0, MAX_THICKNESS_SAMPLES))
            })
    }

    /// Sweep used by the surface parasite form factor.
    ///
    /// The configured sweep is the design variable for the main wing and is
    /// retained for that surface. Tail surfaces have no corresponding design
    /// variable, so their own quarter-chord geometry supplies the sweep.
    fn wing_sweep_deg(&self, index: usize, wing: &Wing) -> f64 {
        if index == 0 || wing.xsecs.len() < 2 {
            self.sweep_deg
        } else {
            wing.mean_sweep_angle(0.25)
        }
    }

    /// Raymer's component buildup for the total parasite drag coefficient.
    ///
    /// `atmosphere` and `section_thickness` let a caller that has already
    /// computed either pass it in rather than have this rebuild it --
    /// [`AeroAnalysis::drag_components`] runs once per candidate evaluation,
    /// or once per angle in a sweep, and would otherwise construct the same
    /// atmosphere twice each time. A direct caller passes `None` for both and
    /// gets the same numbers.
    ///
    /// `cl` is accepted and unused, exactly as upstream accepts and ignores
    /// it: a flat-plate buildup is lift-independent, and the argument is
    /// there so the three drag terms read alike at their call sites. A unit
    /// test states that independence, since a reader will go looking for a
    /// lift-dependent term and should find the absence recorded rather than
    /// have to infer it.
    pub fn parasite_drag(
        &self,
        mach: f64,
        altitude_m: f64,
        cl: f64,
        atmosphere: Option<&Atmosphere>,
        section_thickness: Option<f64>,
    ) -> f64 {
        let _ = cl;
        let owned;
        let atmosphere = match atmosphere {
            Some(atmosphere) => atmosphere,
            None => {
                owned = Atmosphere::new(altitude_m);
                &owned
            }
        };
        let velocity = mach * atmosphere.speed_of_sound();
        let density = atmosphere.density();
        let viscosity = atmosphere.dynamic_viscosity();
        let s_ref = self.plane.s_ref;
        let main_thickness = section_thickness.unwrap_or_else(|| self.section_thickness());
        let x_over_c = self.drag.max_thickness_chordwise_loc;

        let mut cd0 = 0.0;

        for (index, wing) in self.plane.wings.iter().enumerate() {
            let mac = wing.mean_aerodynamic_chord();
            let reynolds = density * velocity * mac / viscosity;
            let cf = Self::turbulent_cf(reynolds, mach);
            let thickness = if self.reference_compatibility || index == 0 {
                main_thickness
            } else {
                Self::wing_section_thickness(wing)
            };
            let sweep_deg = if self.reference_compatibility {
                self.sweep_deg
            } else {
                self.wing_sweep_deg(index, wing)
            };
            let sweep = sweep_deg.to_radians();
            let form_factor = (1.0 + 0.6 / x_over_c * thickness + 100.0 * thickness.powf(4.0))
                * (1.34 * mach.powf(0.18) * sweep.cos().powf(0.28));
            // Wetted area counts exposed skin; the center section inside
            // the fuselage contributes no skin friction. The coefficient
            // still uses the gross projected aircraft reference area.
            let buried = if index == 0
                && !self.reference_compatibility
                && self.drag.exclude_buried_main_wing_area
            {
                self.plane
                    .fuselages
                    .first()
                    .map_or(0.0, |body| wetted::buried_main_wing_area(wing, body))
            } else {
                0.0
            };
            let wetted =
                (wing.unfolded_area() - buried).max(0.0) * self.geometry.wing_wetted_area_factor;
            cd0 += cf * form_factor * self.drag.interference_factor_wing * (wetted / s_ref);
        }

        // The primary body: length from its end stations, diameter from the
        // configuration rather than from the built cross-sections. No form
        // factor is applied to it, nor to the nacelles below -- upstream
        // carries the slenderness effect inside its wetted-area factors.
        if let Some(fuselage) = self.plane.fuselages.first() {
            let length = Self::body_length(fuselage);
            let diameter = self.geometry.fuselage.diameter_m;
            let wetted = PI * diameter * length * self.geometry.fuselage_wetted_factor;
            let reynolds = density * velocity * length / viscosity;
            let cf = Self::turbulent_cf(reynolds, mach);
            cd0 += cf * self.drag.interference_factor_fuselage * (wetted / s_ref);
        }

        for nacelle in self.plane.fuselages.iter().skip(1) {
            let length = Self::body_length(nacelle);
            // A cylinder of the configured nacelle radius, not the built
            // silhouette's own stations.
            let diameter = 2.0 * self.geometry.engine.radius_scale_m;
            let wetted = PI * diameter * length;
            let reynolds = density * velocity * length / viscosity;
            let cf = Self::turbulent_cf(reynolds, mach);
            cd0 += cf * self.drag.interference_factor_nacelle * (wetted / s_ref);
        }

        cd0 * self.drag.viscous_margin
    }

    /// A body's streamwise length, from the x of its first station to the x
    /// of its last.
    fn body_length(fuselage: &Fuselage) -> f64 {
        match (fuselage.xsecs.first(), fuselage.xsecs.last()) {
            (Some(first), Some(last)) => last.xyz_c[0] - first.xyz_c[0],
            // Upstream indexes both ends unguarded and would raise here; a
            // body with no stations has no length, and nothing this program
            // builds produces one.
            _ => 0.0,
        }
    }

    /// The Korn-equation transonic wave-drag estimate.
    ///
    /// Zero below the configured onset Mach, and zero again above it while
    /// the drag-divergence Mach the section, sweep and lift coefficient set
    /// has not been passed -- which is where the nominal cruise point of this
    /// program's own default aircraft actually sits.
    pub fn wave_drag(&self, mach: f64, cl: f64, section_thickness: Option<f64>) -> f64 {
        if mach < self.drag.wave_drag_onset_mach {
            return 0.0;
        }
        let thickness = section_thickness.unwrap_or_else(|| self.section_thickness());
        let cos_sweep = self.sweep_deg.to_radians().cos();
        let kappa = self.drag.korn_technology_factor;
        let mach_dd =
            kappa / cos_sweep - thickness / cos_sweep.powf(2.0) - cl / (10.0 * cos_sweep.powf(3.0));
        if mach > mach_dd {
            self.drag.wave_drag_coefficient * (mach - mach_dd).powf(4.0)
        } else {
            0.0
        }
    }

    /// The three drag terms at one operating point, with `cd_induced` as the
    /// vortex-lattice solve reported it -- `drag_components`.
    ///
    /// Resolves the atmosphere and the section thickness once and passes both
    /// down, which is the whole reason the two arguments exist.
    pub fn drag_components(
        &self,
        mach: f64,
        altitude_m: f64,
        cl: f64,
        cd_induced: f64,
        atmosphere: Option<&Atmosphere>,
    ) -> DragComponents {
        let owned;
        let atmosphere = match atmosphere {
            Some(atmosphere) => atmosphere,
            None => {
                owned = Atmosphere::new(altitude_m);
                &owned
            }
        };
        let thickness = self.section_thickness();
        DragComponents {
            cd_parasite: self.parasite_drag(
                mach,
                altitude_m,
                cl,
                Some(atmosphere),
                Some(thickness),
            ),
            cd_induced,
            cd_wave: self.wave_drag(mach, cl, Some(thickness)),
        }
    }
}

// A test constructs the geometry it asserts on directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::FuselageXSec;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    /// A minimal airplane: one wing, one body, one nacelle.
    fn probe() -> Airplane {
        let airfoil = Airfoil::from_name("naca2412").expect("a 4-digit NACA name");
        let wing = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, airfoil.clone()),
                WingXSec::new([1.0, 8.0, 0.0], 1.5, 0.0, airfoil),
            ],
            true,
        );
        let station = |x: f64, y: f64, radius: f64| {
            FuselageXSec::new([x, y, 0.0], Some(radius), None, None, 2.0)
                .expect("a radius with no width or height")
        };
        let body = Fuselage::new(
            "Fuselage",
            vec![station(0.0, 0.0, 2.0), station(20.0, 0.0, 2.0)],
        );
        let nacelle = Fuselage::new(
            "Nacelle",
            vec![station(4.0, 5.0, 1.0), station(8.0, 5.0, 1.0)],
        );
        Airplane {
            name: "probe".to_owned(),
            xyz_ref: [5.0, 0.0, 0.0],
            wings: vec![wing],
            fuselages: vec![body, nacelle],
            s_ref: 36.0,
            c_ref: 2.4,
            b_ref: 16.0,
        }
    }

    #[test]
    fn the_prandtl_glauert_radicand_floor_is_unreachable_under_the_mach_clamp() {
        // The 0.95 ceiling on M cos(sweep) leaves a radicand of at least
        // 0.0975, so upstream's max(1e-3, ...) never binds. Stated here
        // rather than given a fixture case that would only pretend to reach
        // it -- see swept_pg_beta's own doc.
        let smallest = (1.0 - MACH_NORMAL_CEILING * MACH_NORMAL_CEILING).sqrt();
        assert!(smallest > 1e-3_f64.sqrt());
        for mach in [1.0, 2.0, 50.0] {
            assert_eq!(swept_pg_beta(mach, 0.0), smallest);
        }
    }

    #[test]
    fn the_compressibility_factor_ignores_the_signs_of_mach_and_sweep() {
        assert_eq!(swept_pg_beta(-0.6, -25.0), swept_pg_beta(0.6, 25.0));
    }

    #[test]
    fn an_incompressible_report_leaves_the_angle_where_it_was() {
        // beta is exactly 1 at M = 0, so the correction is the identity.
        assert_eq!(compressible_report_alpha(6.0, -1.0, 0.0, 32.0), 6.0);
    }

    #[test]
    fn the_reported_angle_moves_toward_the_zero_lift_angle_and_not_past_it() {
        let alpha = compressible_report_alpha(5.0, -2.0, 0.82, 32.0);
        assert!(alpha < 5.0 && alpha > -2.0);
        // At the zero-lift angle itself there is nothing to compress.
        assert_eq!(compressible_report_alpha(-2.0, -2.0, 0.82, 32.0), -2.0);
    }

    #[test]
    fn an_airplane_with_no_wing_falls_back_to_a_twelve_percent_section() {
        let mut plane = probe();
        plane.wings.clear();
        let analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        assert_eq!(analysis.section_thickness(), SECTION_THICKNESS_FALLBACK);
    }

    #[test]
    fn a_nacelle_adds_parasite_drag_and_removing_it_takes_that_drag_away() {
        let plane = probe();
        let with_nacelle = AeroAnalysis::new(&plane, 32.0, None, None, None);
        let mut stripped = probe();
        stripped.fuselages.truncate(1);
        let without = AeroAnalysis::new(&stripped, 32.0, None, None, None);
        assert!(
            with_nacelle.parasite_drag(0.8, 10000.0, 0.5, None, None)
                > without.parasite_drag(0.8, 10000.0, 0.5, None, None)
        );
    }

    #[test]
    fn buried_wing_correction_is_selectable_without_changing_induced_or_wave_drag() {
        let plane = probe();
        let mut analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        analysis.drag.exclude_buried_main_wing_area = false;
        let gross = analysis.drag_components(0.8, 10_000.0, 0.5, 0.012, None);
        analysis.drag.exclude_buried_main_wing_area = true;
        let exposed = analysis.drag_components(0.8, 10_000.0, 0.5, 0.012, None);
        assert!(exposed.cd_parasite < gross.cd_parasite);
        assert_eq!(exposed.cd_induced, gross.cd_induced);
        assert_eq!(exposed.cd_wave, gross.cd_wave);
        let reference = AeroAnalysis::new_reference_compatibility(&plane, 32.0, None, None, None);
        assert_eq!(
            reference.parasite_drag(0.8, 10_000.0, 0.5, None, None),
            gross.cd_parasite
        );
    }

    #[test]
    fn the_parasite_estimate_does_not_depend_on_the_lift_coefficient() {
        let plane = probe();
        let analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        assert_eq!(
            analysis.parasite_drag(0.8, 10000.0, 0.0, None, None),
            analysis.parasite_drag(0.8, 10000.0, 1.4, None, None)
        );
    }

    fn tail(tip_x: f64, airfoil_name: &str) -> Wing {
        let airfoil = Airfoil::from_name(airfoil_name).expect("a four-digit NACA name");
        Wing::new(
            "Tail",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, airfoil.clone()),
                WingXSec::new([tip_x, 12.0, 0.0], 1.0, 0.0, airfoil),
            ],
            true,
        )
    }

    #[test]
    fn each_tail_surface_uses_its_own_thickness_and_sweep() {
        let make_plane = |tip_x: f64, airfoil_name: &str| {
            let mut plane = probe();
            plane.wings.push(tail(tip_x, airfoil_name));
            plane
        };
        let drag = |plane: &Airplane| {
            AeroAnalysis::new(plane, 32.0, None, None, None)
                .parasite_drag(0.8, 10_000.0, 0.5, None, None)
        };

        let thin_unswept = drag(&make_plane(0.0, "naca0006"));
        let thick_unswept = drag(&make_plane(0.0, "naca0018"));
        let thin_swept = drag(&make_plane(12.0, "naca0006"));

        assert!(
            thick_unswept > thin_unswept,
            "tail thickness must affect its parasite form factor"
        );
        assert!(
            (thin_swept - thin_unswept).abs() > 1e-10,
            "tail sweep must affect its parasite form factor"
        );
    }

    #[test]
    fn wave_drag_is_zero_below_the_onset_mach_whatever_the_lift() {
        let plane = probe();
        let analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        let onset = analysis.drag.wave_drag_onset_mach;
        assert_eq!(analysis.wave_drag(onset - 1e-9, 5.0, None), 0.0);
    }

    #[test]
    fn wave_drag_rises_with_lift_once_the_divergence_mach_is_passed() {
        let plane = probe();
        let analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        // More lift lowers the drag-divergence Mach, so the same flight Mach
        // sits further past it.
        let light = analysis.wave_drag(0.86, 0.4, None);
        let heavy = analysis.wave_drag(0.86, 0.9, None);
        assert!(heavy > light);
    }

    #[test]
    fn parasite_and_wave_drag_remain_finite_through_the_regime_boundary() {
        let plane = probe();
        let analysis = AeroAnalysis::new(&plane, 32.0, None, None, None);
        for mach in [0.2, 0.8, 0.99, 1.0, 1.2] {
            let parasite = analysis.parasite_drag(mach, 10_000.0, 0.5, None, None);
            let wave = analysis.wave_drag(mach, 0.5, None);
            assert!(parasite.is_finite(), "parasite drag at Mach {mach}");
            assert!(wave.is_finite() && wave >= 0.0, "wave drag at Mach {mach}");
        }
    }

    #[test]
    fn the_total_is_the_three_terms_and_nothing_else() {
        let components = DragComponents {
            cd_parasite: 0.017,
            cd_induced: 0.012,
            cd_wave: 0.002,
        };
        assert_eq!(components.cd_total(), 0.017 + 0.012 + 0.002);
    }
}
