// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physically-anchored longitudinal neutral point across conditions.
//!
//! [`trim::neutral_point`] and [`trim::stability_and_trim`] in their
//! `reference_compatibility` form apply the tail dynamic-pressure efficiency
//! `eta` to the whole wing+tail VLM offset about the *geometric* quarter-MAC
//! point and shift the neutral point forward by a fuselage term whose strip
//! split, forebody coverage and afterbody ramp direction differ from
//! Multhopp's method, with no nacelle term and no compressibility. That form
//! is kept only to replay the frozen fixtures.
//!
//! This module reads each wing's own force/moment contribution off the VLM
//! panel data directly ([`surfaces::per_surface_contributions`]), applies
//! `eta` to the tail's own lift only ([`surfaces::combine`]), corrects the
//! fuselage strip method ([`fuselage::fuselage_terms`]), adds a Torenbeek
//! nacelle term ([`nacelles::nacelle_terms`]), and evaluates the whole
//! pipeline at a stated Mach through a Goethert/Prandtl-Glauert stretch of
//! the lifting-surface mesh ([`surfaces::goethert_stretch`]).
//!
//! [`neutral_point_conditions`] is the public surface for evaluating every
//! condition at once: clean low-speed, clean cruise, a high-lift
//! downwash-increment estimate, and a declared elasticity uncertainty band,
//! reduced to the single most-forward (`critical`) station another lane
//! forms the aft CG limit from. [`trim::neutral_point`]'s *product* path
//! (not its `reference_compatibility` twin) returns this module's clean
//! neutral point at its own fixed analysis condition; see its doc comment.

mod fuselage;
mod nacelles;
mod pipeline;
mod surfaces;

use alas_aero::vlm::VlmError;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

pub use fuselage::FuselageTerms;
pub use nacelles::NacelleTerm;
pub use surfaces::{combine, goethert_stretch, per_surface_contributions, SurfaceContribution};

pub(crate) use pipeline::product_neutral_point;
use pipeline::{clean_np_at_condition, clean_np_at_conditions_sharing_probe};

/// Documented conditions and estimates [`neutral_point_conditions`]
/// evaluates. Every non-Mach field here is a declared estimate, not a
/// calibrated fit; see the module doc for the ones with a numeric source and
/// [`Default`] for the values used when a caller has none of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpConditionsInput {
    /// The clean low-speed evaluation Mach (typically an approach-speed
    /// order of magnitude, not a stall condition).
    pub low_speed_mach: f64,
    /// Altitude for the low-speed evaluation, m (sea level unless the
    /// caller has a specific low-speed condition).
    pub low_speed_altitude_m: f64,
    /// The clean cruise evaluation Mach.
    pub cruise_mach: f64,
    /// Altitude for the cruise evaluation, m.
    pub cruise_altitude_m: f64,
    /// The DATCOM-4.4.1-style downwash-gradient increment applied to the
    /// tail term for the high-lift (flaps/slats-down) condition, at the
    /// low-speed Mach/altitude: an *estimate* (+0.10 to +0.15 is a
    /// typical order of magnitude for a single-slotted
    /// flap at approach deflection), flagged as such; a caller with a
    /// flap-deflected VLM panel geometry should probe that directly instead
    /// of using this field.
    pub flap_deps_increment: f64,
    /// The fraction of the (cruise-rigid-to-critical-rigid) neutral-point
    /// travel [`Self::elastic_factor`] declares as an aeroelastic
    /// uncertainty band at the critical condition: an estimate (-3 to -8 %MAC
    /// is a typical order of magnitude for
    /// a transport wing at high dynamic pressure), not a calibrated
    /// stiffness model. `0.0` disables the band (rigid only).
    pub elastic_factor: f64,
}

impl Default for NpConditionsInput {
    fn default() -> Self {
        Self {
            low_speed_mach: 0.2,
            low_speed_altitude_m: 0.0,
            cruise_mach: 0.78,
            cruise_altitude_m: 11_000.0,
            flap_deps_increment: 0.10,
            elastic_factor: 0.05,
        }
    }
}

/// Every intermediate term [`neutral_point_conditions`] and
/// [`clean_np_at_condition`] compute, for reporting and cross-checking.
#[derive(Debug, Clone, PartialEq)]
pub struct NpDiagnostics {
    /// The main wing's own implied aerodynamic centre from the two-alpha
    /// VLM probe (`eta`-independent; not the geometric quarter-MAC point),
    /// geometry axes, m.
    pub x_wing_alone_ac: f64,
    /// The per-surface neutral point before any fuselage/nacelle
    /// correction (F1), geometry axes, m.
    pub x_np_surfaces: f64,
    /// The Mach-consistent total lift-curve slope this evaluation used,
    /// per rad.
    pub cl_alpha_total: f64,
    /// The VLM-implied downwash gradient at the tail
    /// (`1 - a_t,in_presence / a_t,isolated`), used by the fuselage
    /// afterbody factor; `0.0` with no tail.
    pub deps_dalpha_vlm: f64,
    /// Primary and cross-check fuselage terms (F2).
    pub fuselage: FuselageTerms,
    /// Every nacelle's own Torenbeek term (F3); empty with no podded
    /// fuselage.
    pub nacelle_terms: Vec<NacelleTerm>,
    /// Sum of [`Self::nacelle_terms`], m (negative: forward).
    pub nacelle_shift_m: f64,
}

/// The full set of conditions [`neutral_point_conditions`] evaluates, plus
/// the single most-forward (`critical`) station another lane forms the aft
/// CG limit from.
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralPointConditions {
    /// Clean, rigid neutral point at [`NpConditionsInput::low_speed_mach`].
    pub clean_low_speed: f64,
    /// Clean, rigid neutral point at [`NpConditionsInput::cruise_mach`].
    pub cruise: f64,
    /// Clean-geometry neutral point at the low-speed condition with
    /// [`NpConditionsInput::flap_deps_increment`] applied to the tail term
    /// (an estimate of the flaps/slats-down condition; see that field's
    /// doc).
    pub high_lift: f64,
    /// `(forward, rigid)` aeroelastic uncertainty band at the most-forward
    /// rigid condition among the three above, from
    /// [`NpConditionsInput::elastic_factor`]. An estimate, not a calibrated
    /// stiffness model; see that field's doc.
    pub elastic_band: (f64, f64),
    /// `min` over every condition above, including the forward edge of
    /// [`Self::elastic_band`]: the station a critical-condition aft CG
    /// limit should be formed from.
    pub critical: f64,
    /// Diagnostics from whichever condition produced [`Self::critical`].
    pub diagnostics: NpDiagnostics,
}

/// The full corrected neutral point: clean low-speed, clean cruise, an estimated
/// high-lift, a declared elastic band, and the most-forward (`critical`)
/// station across all of them. See [`NpConditionsInput`] for every
/// estimate's basis and [`NeutralPointConditions`] for the fields.
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point_conditions(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    input: &NpConditionsInput,
) -> Result<NeutralPointConditions, VlmError> {
    // The low-speed and high-lift conditions share Mach, altitude and probe
    // angles, differing only in the downwash increment applied to the tail
    // term; the underlying VLM probe (stretched-mesh assembly, alpha pair,
    // isolated-tail downwash solve) is therefore identical between them and
    // is paid for once.
    let ((low, low_diag), (high_lift, high_lift_diag)) = clean_np_at_conditions_sharing_probe(
        airplane,
        analysis,
        input.low_speed_mach,
        input.low_speed_altitude_m,
        0.0,
        input.flap_deps_increment,
    )?;
    let (cruise, cruise_diag) = clean_np_at_condition(
        airplane,
        analysis,
        input.cruise_mach,
        input.cruise_altitude_m,
        0.0,
    )?;

    let candidates = [
        (low, &low_diag),
        (cruise, &cruise_diag),
        (high_lift, &high_lift_diag),
    ];
    let (rigid_critical, rigid_diag) = candidates
        .iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(x, d)| (*x, (*d).clone()))
        .unwrap_or((low, low_diag.clone()));

    let elastic_shift_m = input.elastic_factor.max(0.0) * airplane.c_ref.max(0.1);
    let elastic_forward = rigid_critical - elastic_shift_m;
    let elastic_band = (elastic_forward, rigid_critical);
    let critical = rigid_critical.min(elastic_forward);

    Ok(NeutralPointConditions {
        clean_low_speed: low,
        cruise,
        high_lift,
        elastic_band,
        critical,
        diagnostics: rigid_diag,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn probe_fuselage() -> Fuselage {
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.6),
                station(3.0, 1.6),
                station(16.0, 1.6),
                station(22.0, 0.7),
            ],
        )
    }

    fn probe_airplane() -> Airplane {
        probe_airplane_with_tail_scale(1.0)
    }

    /// The probe with both horizontal-tail chords scaled by `tail_scale`, so
    /// the tail area and tail volume scale by the same factor.
    fn probe_airplane_with_tail_scale(tail_scale: f64) -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([8.0, 0.0, 0.0], 4.0, 0.0, naca("naca0012")),
                WingXSec::new([9.5, 12.0, 0.0], 1.5, 0.0, naca("naca0012")),
            ],
            true,
        );
        let hstab = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([20.0, 0.0, 0.0], 1.8 * tail_scale, -2.0, naca("naca0012")),
                WingXSec::new([20.6, 4.0, 0.0], 1.0 * tail_scale, -2.0, naca("naca0012")),
            ],
            true,
        );
        let s_ref = main.reference_area();
        let b_ref = main.reference_span();
        let c_ref = main.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [9.0, 0.0, 0.0],
            wings: vec![main, hstab],
            fuselages: vec![probe_fuselage()],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn neutral_point_conditions_reports_a_critical_no_more_aft_than_every_rigid_condition() {
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput::default();
        let result = neutral_point_conditions(&plane, &analysis, &input)
            .expect("the probe meshes and solves");
        assert!(result.critical <= result.clean_low_speed + 1e-9);
        assert!(result.critical <= result.cruise + 1e-9);
        assert!(result.critical <= result.high_lift + 1e-9);
        assert!(result.elastic_band.0 <= result.elastic_band.1);
    }

    /// Physical properties of the neutral point, in metres aft of the nose
    /// tip: a tailed aircraft has its neutral point aft of the wing-alone
    /// aerodynamic centre (the tail adds a stabilizing, aft-acting lift-curve
    /// slope; Raymer, Aircraft Design: A Conceptual Approach, 6th ed.,
    /// section 16.3), and a larger tail moves it further aft (Etkin and Reid,
    /// Dynamics of Flight, 3rd ed., section 2.5: the tail contribution grows
    /// with tail volume).
    #[test]
    fn the_neutral_point_is_aft_of_the_wing_alone_centre_and_grows_with_tail_volume() {
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput::default();
        let mut previous: Option<f64> = None;
        for tail_scale in [0.5, 1.0, 1.5, 2.0] {
            let plane = probe_airplane_with_tail_scale(tail_scale);
            let result = neutral_point_conditions(&plane, &analysis, &input)
                .expect("the probe meshes and solves");
            assert!(
                result.clean_low_speed.is_finite()
                    && result.diagnostics.x_wing_alone_ac.is_finite()
            );
            // The wing-alone centre is the reference; the fuselage term can
            // move the wing-body centre forward, so the claim is on the
            // tailed neutral point against the wing alone.
            assert!(
                result.clean_low_speed > result.diagnostics.x_wing_alone_ac,
                "tail scale {tail_scale}: neutral point {:.3} m not aft of the wing-alone \
                 centre {:.3} m",
                result.clean_low_speed,
                result.diagnostics.x_wing_alone_ac
            );
            if let Some(before) = previous {
                assert!(
                    result.clean_low_speed > before,
                    "tail scale {tail_scale}: neutral point {:.3} m did not move aft of {before:.3} m",
                    result.clean_low_speed
                );
            }
            previous = Some(result.clean_low_speed);
        }
    }

    #[test]
    fn high_lift_is_no_more_aft_than_clean_low_speed() {
        // A positive downwash increment can only de-weight the tail's aft
        // aerodynamic centre, never move it aft of the clean case.
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput {
            flap_deps_increment: 0.10,
            ..NpConditionsInput::default()
        };
        let result = neutral_point_conditions(&plane, &analysis, &input)
            .expect("the probe meshes and solves");
        assert!(result.high_lift <= result.clean_low_speed + 1e-6);
    }

    #[test]
    fn zero_elastic_factor_collapses_the_band_to_the_rigid_station() {
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput {
            elastic_factor: 0.0,
            ..NpConditionsInput::default()
        };
        let result = neutral_point_conditions(&plane, &analysis, &input)
            .expect("the probe meshes and solves");
        assert!((result.elastic_band.0 - result.elastic_band.1).abs() < 1e-9);
    }
}
