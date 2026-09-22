// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (_stability_scalars)
// Reference: alas @ rust-port-baseline.

//! Shared side-view/metrics scalar quantities, computed once from the
//! [`AnalysisReport`] (no new VLM runs) so [`super::side_view`] and
//! [`super::metrics`] stay numerically consistent with each other:
//! `_stability_scalars`.

use alas_geom::aircraft::wing::Wing;
use alas_pipeline::full_analysis::AnalysisReport;

/// The chord fraction [`Wing::aerodynamic_center`] is read at: the
/// quarter-chord native aerodynamic model's own default (`Wing.aerodynamic_center()` with
/// no argument) uses. Same value `alas-stab::trim`'s own `AC_CHORD_FRACTION`
/// carries.
pub(super) const AC_CHORD_FRACTION: f64 = 0.25;

/// The static margin `_stability_scalars` substitutes when
/// `report.static_margin` is `NaN`.
const FALLBACK_STATIC_MARGIN: f64 = 0.10;

/// Scalar geometry and stability quantities shared by
/// [`super::side_view::figure_stability_side_view`] and
/// [`super::metrics::figure_stability_metrics`]: the dict `_stability_scalars`
/// returns, as a struct.
pub struct StabilityScalars<'a> {
    /// The main wing: by name (`"Main Wing"`), falling back to the first wing.
    pub wing: &'a Wing,
    /// The horizontal stabilizer, if present: by name
    /// (`"Horizontal Stabilizer"`), falling back to the second wing.
    pub hstab: Option<&'a Wing>,
    /// Reference chord, m.
    pub c_ref: f64,
    /// Aerodynamic-moment-reference (`xyz_ref`) longitudinal position, m.
    pub x_cg_aero: f64,
    /// Physical (mass-weighted) center-of-gravity longitudinal position, m.
    pub x_cg_phys: f64,
    /// Main wing aerodynamic-center longitudinal position, m.
    pub x_wing_ac: f64,
    /// Static margin, fraction of MAC (`0.10` fallback if `report.static_margin` is `NaN`).
    pub sm: f64,
    /// Neutral-point longitudinal position, m.
    pub x_np: f64,
    /// Main wing planform area, m^2.
    pub s_wing: f64,
    /// Horizontal-stabilizer planform area, m^2 (`0.0` with no stabilizer).
    pub s_tail: f64,
    /// Horizontal-stabilizer aerodynamic-center longitudinal position, m
    /// (`x_np + 2.0` with no stabilizer, matching upstream's fallback).
    pub x_hstab_ac: f64,
    /// Tail moment arm, wing AC to h-stab AC, m.
    pub l_t: f64,
    /// Horizontal tail volume coefficient.
    pub v_h: f64,
    /// Longitudinal position of the leading edge of the mean aerodynamic
    /// chord (wing AC sits at 25% of the MAC), m.
    pub x_lemac: f64,
}

impl StabilityScalars<'_> {
    /// Convert a longitudinal position `x` to percent MAC from LEMAC: the
    /// `pct` closure both figures define locally.
    pub fn pct(&self, x: f64) -> f64 {
        (x - self.x_lemac) / self.c_ref * 100.0
    }
}

/// Compute [`StabilityScalars`] from `report`, or `None` if the airplane has
/// no wings at all. Upstream indexes `plane.wings[0]` unconditionally
/// (`next(..., plane.wings[0])`), which would raise `IndexError` on an
/// airplane with none; this port declines to render rather than panicking,
/// per `CONTRIBUTING.md`.
pub fn stability_scalars(report: &AnalysisReport) -> Option<StabilityScalars<'_>> {
    let plane = &report.airplane;
    let wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .or_else(|| plane.wings.first())?;
    let hstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Horizontal Stabilizer")
        .or_else(|| plane.wings.get(1));

    let c_ref = plane.c_ref;
    let x_cg_aero = plane.xyz_ref[0];
    let x_cg_phys = report.physical_cg[0];
    let x_wing_ac = wing.aerodynamic_center(AC_CHORD_FRACTION)[0];
    let sm = if report.static_margin.is_nan() {
        FALLBACK_STATIC_MARGIN
    } else {
        report.static_margin
    };
    let x_np = x_cg_aero + sm * c_ref;

    let s_wing = wing.reference_area();
    let s_tail = hstab.map_or(0.0, Wing::reference_area);
    let x_hstab_ac = hstab.map_or(x_np + 2.0, |h| h.aerodynamic_center(AC_CHORD_FRACTION)[0]);
    let l_t = x_hstab_ac - x_wing_ac;
    let v_h = if c_ref * s_wing > 0.0 {
        (l_t * s_tail) / (c_ref * s_wing)
    } else {
        0.0
    };

    let x_lemac = x_wing_ac - 0.25 * c_ref;

    Some(StabilityScalars {
        wing,
        hstab,
        c_ref,
        x_cg_aero,
        x_cg_phys,
        x_wing_ac,
        sm,
        x_np,
        s_wing,
        s_tail,
        x_hstab_ac,
        l_t,
        v_h,
        x_lemac,
    })
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_config::design_variables::DesignVector;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_pipeline::full_analysis::{DesignPoint, PolarFit, PolarFitStatus};
    use std::collections::HashMap;

    // Test geometry uses a known-valid airfoil name.
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn naca(name: &str) -> alas_geom::aircraft::airfoil::Airfoil {
        alas_geom::aircraft::airfoil::Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn simple_wing(name: &str, x0: f64, span: f64, chord: f64, symmetric: bool) -> Wing {
        use alas_geom::aircraft::wing::WingXSec;
        Wing::new(
            name,
            vec![
                WingXSec::new([x0, 0.0, 0.0], chord, 0.0, naca("naca0012")),
                WingXSec::new([x0, span, 0.0], chord, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    fn report_with(wings: Vec<Wing>, static_margin: f64) -> AnalysisReport {
        use alas_aero::analysis::PolarSweep;
        AnalysisReport {
            design: DesignVector::default(),
            airplane: Airplane {
                name: "Probe".to_owned(),
                xyz_ref: [5.0, 0.0, 0.0],
                s_ref: wings.first().map_or(1.0, Wing::area),
                c_ref: 2.0,
                b_ref: wings.first().map_or(1.0, Wing::span),
                wings,
                fuselages: Vec::new(),
            },
            polar: PolarSweep {
                alpha_deg: Vec::new(),
                geometric_alpha_deg: Vec::new(),
                cl: Vec::new(),
                cd: Vec::new(),
                cd_induced: Vec::new(),
                cd_wave: Vec::new(),
                cd_parasite: Vec::new(),
                cm: Vec::new(),
                l_over_d: Vec::new(),
            },
            design_point: DesignPoint {
                alpha_deg: 2.0,
                cl: 0.5,
                cd: 0.03,
                l_over_d: 16.0,
            },
            polar_fit: PolarFit {
                cd0: 0.02,
                k: 0.04,
                oswald_e: 0.85,
                aspect_ratio: 9.0,
                status: PolarFitStatus::Fitted,
            },
            static_margin,
            x_neutral_point: 0.0,
            geometry_summary: HashMap::new(),
            component_masses: HashMap::new(),
            flops_mass_buildup: None,
            mass_coordinates: HashMap::new(),
            physical_cg: [5.2, 0.0, 0.0],
            payload_layout: None,
            trimmed_design_point: None,
            cg_envelope_ok: None,
        }
    }

    #[test]
    fn no_wings_at_all_returns_none() {
        let report = report_with(Vec::new(), 0.1);
        assert!(stability_scalars(&report).is_none());
    }

    #[test]
    fn a_nan_static_margin_falls_back_to_the_ten_percent_default() {
        let report = report_with(
            vec![simple_wing("Main Wing", 5.0, 8.0, 2.0, true)],
            f64::NAN,
        );
        let s = stability_scalars(&report).expect("one wing is enough");
        assert_eq!(s.sm, FALLBACK_STATIC_MARGIN);
        assert!((s.x_np - (s.x_cg_aero + FALLBACK_STATIC_MARGIN * s.c_ref)).abs() < 1e-12);
    }

    #[test]
    fn with_no_stabilizer_the_tail_arm_and_volume_coefficient_are_a_stub() {
        let report = report_with(vec![simple_wing("Main Wing", 5.0, 8.0, 2.0, true)], 0.1);
        let s = stability_scalars(&report).expect("one wing is enough");
        assert!(s.hstab.is_none());
        assert_eq!(s.s_tail, 0.0);
        assert_eq!(s.v_h, 0.0);
        assert!((s.x_hstab_ac - (s.x_np + 2.0)).abs() < 1e-12);
    }

    #[test]
    fn a_stabilizer_further_aft_gives_a_larger_tail_volume_coefficient() {
        let near = report_with(
            vec![
                simple_wing("Main Wing", 0.0, 8.0, 2.0, true),
                simple_wing("Horizontal Stabilizer", 10.0, 2.0, 1.0, true),
            ],
            0.1,
        );
        let far = report_with(
            vec![
                simple_wing("Main Wing", 0.0, 8.0, 2.0, true),
                simple_wing("Horizontal Stabilizer", 20.0, 2.0, 1.0, true),
            ],
            0.1,
        );
        let s_near = stability_scalars(&near).expect("two wings");
        let s_far = stability_scalars(&far).expect("two wings");
        assert!(s_far.l_t > s_near.l_t);
        assert!(s_far.v_h > s_near.v_h);
    }

    #[test]
    fn pct_is_zero_at_lemac_and_a_hundred_a_full_mac_aft() {
        let report = report_with(vec![simple_wing("Main Wing", 0.0, 8.0, 4.0, true)], 0.1);
        let s = stability_scalars(&report).expect("one wing is enough");
        assert!((s.pct(s.x_lemac)).abs() < 1e-9);
        assert!((s.pct(s.x_lemac + s.c_ref) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn wing_and_stabilizer_are_selected_by_name_over_position() {
        // Deliberately out of the [0]/[1] order the fallback would use.
        let report = report_with(
            vec![
                simple_wing("Horizontal Stabilizer", 10.0, 2.0, 1.0, true),
                simple_wing("Main Wing", 0.0, 8.0, 2.0, true),
            ],
            0.1,
        );
        let s = stability_scalars(&report).expect("two wings");
        assert_eq!(s.wing.name, "Main Wing");
        assert_eq!(
            s.hstab.expect("named stabilizer present").name,
            "Horizontal Stabilizer"
        );
    }
}
