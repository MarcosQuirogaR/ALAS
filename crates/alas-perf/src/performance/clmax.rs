// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Maximum lift coefficient of a swept wing with high-lift devices, from the
//! planform geometry (AERL-01).
//!
//! Two forms are provided and kept apart on purpose.
//!
//! - The **absolute handbook forms** ([`raymer_clean_cl_max`],
//!   [`raymer_flap_increment`], [`roskam_flap_increment`],
//!   [`roskam_slat_increment`]) need airfoil section data (`cl_max`, the
//!   section increments of the devices) that the configuration does not
//!   carry. They are the closed forms the scaling below is derived from and
//!   are tested against the worked A320 example of the audit.
//! - The **reference-scaled model** ([`ClmaxReference::scale`]) anchors the
//!   three coefficients (clean, take-off, landing) of a registered aircraft
//!   at its declared values and moves them with the two geometry ratios the
//!   handbook forms share: `cos(Lambda_c/4)` for the clean wing and
//!   `(S_flapped/S) cos(Lambda_hinge)` for the high-lift increments. Every
//!   registered preset therefore keeps its calibrated value at its own
//!   geometry (the ratios are exactly 1), and a sweep, taper, span or flap
//!   change moves the coefficients in the handbook direction and magnitude.
//!
//! # Sources
//!
//! - D. P. Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., eq.
//!   12.15 (`CL_max = 0.9 cl_max cos Lambda_c/4`), eq. 12.21 (`dCL_max = 0.9
//!   dcl_max (S_flapped/S) cos Lambda_hinge`), Table 12.2 (section increments;
//!   take-off settings use 60-80 % of the landing value).
//! - J. Roskam, *Airplane Design Part VI*, eqs. 8.29 and 8.30 (also USAF
//!   DATCOM 6.1.4.3-a,b): `dCL_max = dcl_max (S_wf/S) K_Lambda` with
//!   `K_Lambda = (1 - 0.08 cos^2 Lambda_c/4) cos^(3/4) Lambda_c/4`; slats and
//!   Krueger flaps `dCL_max = 7.11 (c_f,lef/c)(b_lef/b_e)^2 cos^2 Lambda_c/4`.
//! - USAF DATCOM 4.1.3.4 (wing maximum lift) for the swept-wing clean value.
//!
//! # Validity
//!
//! Subsonic, moderate-to-high aspect ratio, `Lambda_c/4` below about 45 deg,
//! Reynolds number of the reference. Reynolds, Mach (above 0.2) and the slat
//! extension chord ratio are out of scope: the reference anchors their level.
//! The model has no dependence on mass.

/// Raymer eq. 12.15 for a wing of section maximum lift `cl_max_section` and
/// quarter-chord sweep `sweep_c4_deg`: `0.9 cl_max cos(Lambda_c/4)`.
pub fn raymer_clean_cl_max(cl_max_section: f64, sweep_c4_deg: f64) -> f64 {
    0.9 * cl_max_section * sweep_c4_deg.to_radians().cos()
}

/// Raymer eq. 12.21: the trailing-edge or leading-edge device increment
/// `0.9 dcl_max (S_flapped/S) cos(Lambda_hinge)`.
pub fn raymer_flap_increment(
    d_cl_max_section: f64,
    flapped_area_ratio: f64,
    hinge_sweep_deg: f64,
) -> f64 {
    0.9 * d_cl_max_section * flapped_area_ratio * hinge_sweep_deg.to_radians().cos()
}

/// The Roskam VI / DATCOM 6.1.4.3 sweep factor
/// `K_Lambda = (1 - 0.08 cos^2 Lambda_c/4) cos^(3/4) Lambda_c/4`
/// (0.868 at 25 deg).
pub fn roskam_k_lambda(sweep_c4_deg: f64) -> f64 {
    let cos = sweep_c4_deg.to_radians().cos();
    (1.0 - 0.08 * cos * cos) * cos.powf(0.75)
}

/// Roskam VI eq. 8.29: `dCL_max = dcl_max (S_wf/S) K_Lambda`.
pub fn roskam_flap_increment(
    d_cl_max_section: f64,
    flapped_area_ratio: f64,
    sweep_c4_deg: f64,
) -> f64 {
    d_cl_max_section * flapped_area_ratio * roskam_k_lambda(sweep_c4_deg)
}

/// Roskam VI eq. 8.30 (DATCOM 6.1.4.3-b) for slats and Krueger flaps:
/// `7.11 (c_f,lef/c) (b_lef/b_e)^2 cos^2(Lambda_c/4)`.
pub fn roskam_slat_increment(chord_ratio: f64, span_ratio: f64, sweep_c4_deg: f64) -> f64 {
    let cos = sweep_c4_deg.to_radians().cos();
    7.11 * chord_ratio * span_ratio * span_ratio * cos * cos
}

/// The three maximum lift coefficients of a configuration set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClmaxSet {
    /// Clean wing, flaps and slats retracted.
    pub clean: f64,
    /// Take-off flap setting.
    pub takeoff: f64,
    /// Landing flap setting.
    pub landing: f64,
}

/// The wing quantities the reference scaling reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingLiftGeometry {
    /// Quarter-chord sweep of the lifting wing, degrees.
    pub sweep_c4_deg: f64,
    /// Area spanned by the trailing-edge flap over the reference area.
    pub flapped_area_ratio: f64,
    /// Sweep of the flap hinge line, degrees.
    pub hinge_sweep_deg: f64,
}

impl WingLiftGeometry {
    fn is_valid(&self) -> bool {
        self.sweep_c4_deg.is_finite()
            && self.sweep_c4_deg.abs() < 70.0
            && self.hinge_sweep_deg.is_finite()
            && self.hinge_sweep_deg.abs() < 70.0
            && self.flapped_area_ratio.is_finite()
            && self.flapped_area_ratio > 0.0
    }
}

/// A registered aircraft's declared coefficients and the geometry they
/// belong to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClmaxReference {
    /// The declared coefficients (calibration inputs of the preset).
    pub set: ClmaxSet,
    /// The geometry they were declared for.
    pub geometry: WingLiftGeometry,
}

impl ClmaxReference {
    /// The coefficients of a wing of `geometry`:
    ///
    /// ```text
    /// clean   = clean_ref * cos(L_c4) / cos(L_c4,ref)
    /// d_x     = (x_ref - clean_ref) * [(Sf/S) / (Sf/S)_ref] * [cos(L_h) / cos(L_h,ref)]
    /// x       = clean + d_x                         (x = take-off, landing)
    /// ```
    ///
    /// `None` when either geometry is invalid or when the reference
    /// increments are not positive (nothing to scale), and when `geometry`
    /// equals the reference exactly: the declared values stand untouched,
    /// bit for bit.
    pub fn scale(&self, geometry: &WingLiftGeometry) -> Option<ClmaxSet> {
        if !self.geometry.is_valid() || !geometry.is_valid() || geometry == &self.geometry {
            return None;
        }
        let d_takeoff = self.set.takeoff - self.set.clean;
        let d_landing = self.set.landing - self.set.clean;
        if !(self.set.clean > 0.0 && d_takeoff > 0.0 && d_landing > 0.0) {
            return None;
        }
        let sweep = |g: &WingLiftGeometry| g.sweep_c4_deg.to_radians().cos();
        let hinge = |g: &WingLiftGeometry| g.hinge_sweep_deg.to_radians().cos();
        let clean = self.set.clean * sweep(geometry) / sweep(&self.geometry);
        let device = geometry.flapped_area_ratio / self.geometry.flapped_area_ratio
            * hinge(geometry)
            / hinge(&self.geometry);
        let scaled = ClmaxSet {
            clean,
            takeoff: clean + d_takeoff * device,
            landing: clean + d_landing * device,
        };
        [scaled.clean, scaled.takeoff, scaled.landing]
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
            .then_some(scaled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audit's worked A320 (SOURCES-R2 section 2.3): clean airfoil
    /// `cl_max` 1.8, `Lambda_c/4` 25 deg; Fowler `c'/c` 1.20 over `S_wf/S`
    /// 0.60 with hinge sweep 20 deg; slats `c'/c` 1.10 over 0.75 of the wing,
    /// sweep 27 deg.
    #[test]
    fn the_worked_a320_example_reproduces_the_handbook_numbers() {
        let clean = raymer_clean_cl_max(1.8, 25.0);
        assert!((clean - 1.468).abs() < 1e-3, "{clean}");
        // Table 12.2: Fowler dcl_max 1.3 c'/c; slat 0.4 c'/c.
        let landing_flap = raymer_flap_increment(1.3 * 1.20, 0.60, 20.0);
        let slat = raymer_flap_increment(0.4 * 1.10, 0.75, 27.0);
        assert!((landing_flap - 0.79).abs() < 5e-3, "{landing_flap}");
        assert!((slat - 0.26).abs() < 5e-3, "{slat}");
        // Raymer: landing 2.52, take-off 2.29 slats out / 2.02 retracted.
        let landing = clean + landing_flap + slat;
        assert!((landing - 2.52).abs() < 0.01, "{landing}");
        let takeoff_flap = 0.7 * landing_flap;
        assert!((clean + takeoff_flap + slat - 2.29).abs() < 0.01);
        assert!((clean + takeoff_flap - 2.02).abs() < 0.01);
        // Roskam VI / DATCOM: clean 1.45, flap 0.59 (landing) / 0.30
        // (take-off), slat 0.56; landing 2.60, take-off 2.31.
        assert!((roskam_k_lambda(25.0) - 0.868).abs() < 1e-3);
        let flap_land = roskam_flap_increment(1.0 * 0.97 * 0.90 * 1.3, 0.60, 25.0);
        let flap_to = roskam_flap_increment(1.0 * 0.80 * 0.55 * 1.3, 0.60, 25.0);
        let slat_roskam = roskam_slat_increment(0.15, 0.8, 25.0);
        assert!((flap_land - 0.59).abs() < 0.01, "{flap_land}");
        assert!((flap_to - 0.30).abs() < 0.01, "{flap_to}");
        assert!((slat_roskam - 0.56).abs() < 0.01, "{slat_roskam}");
        assert!((1.45 + flap_land + slat_roskam - 2.60).abs() < 0.02);
        assert!((1.45 + flap_to + slat_roskam - 2.31).abs() < 0.02);
    }

    fn a320_reference() -> ClmaxReference {
        ClmaxReference {
            set: ClmaxSet {
                clean: 1.50,
                takeoff: 1.80,
                landing: 2.60,
            },
            geometry: WingLiftGeometry {
                sweep_c4_deg: 25.0,
                flapped_area_ratio: 0.60,
                hinge_sweep_deg: 20.0,
            },
        }
    }

    #[test]
    fn the_reference_geometry_returns_none_so_declared_values_stand_bitwise() {
        let reference = a320_reference();
        assert_eq!(reference.scale(&reference.geometry), None);
    }

    #[test]
    fn sweep_scales_the_clean_value_by_the_cosine_law() {
        let reference = a320_reference();
        // The audit's 31 -> 37 deg window: cos 37 / cos 31 = 0.9317 (-6.8 %).
        let at_31 = ClmaxReference {
            geometry: WingLiftGeometry {
                sweep_c4_deg: 31.0,
                ..reference.geometry
            },
            ..reference
        };
        let moved = WingLiftGeometry {
            sweep_c4_deg: 37.0,
            ..at_31.geometry
        };
        let scaled = at_31.scale(&moved).expect("moved geometry");
        let ratio = 37.0_f64.to_radians().cos() / 31.0_f64.to_radians().cos();
        assert!((ratio - 0.9317).abs() < 1e-3);
        assert!((scaled.clean - 1.50 * ratio).abs() < 1e-12);
        // Increments scale with the hinge sweep only, not the quarter chord.
        assert!(((scaled.takeoff - scaled.clean) - 0.30).abs() < 1e-12);
        assert!(((scaled.landing - scaled.clean) - 1.10).abs() < 1e-12);
    }

    #[test]
    fn flap_span_and_hinge_sweep_scale_the_increments_and_the_set_is_monotone() {
        let reference = a320_reference();
        let wider = WingLiftGeometry {
            flapped_area_ratio: 0.66,
            ..reference.geometry
        };
        let more_swept = WingLiftGeometry {
            hinge_sweep_deg: 30.0,
            ..reference.geometry
        };
        let a = reference.scale(&wider).unwrap();
        let b = reference.scale(&more_swept).unwrap();
        // +10 % flapped area gives +10 % increments at unchanged sweep.
        assert!((a.landing - a.clean - 1.10 * 1.10).abs() < 1e-12);
        assert!((a.takeoff - a.clean - 1.10 * 0.30).abs() < 1e-12);
        assert_eq!(a.clean, 1.50);
        // More hinge sweep, smaller increment, clean unchanged.
        let factor = 30.0_f64.to_radians().cos() / 20.0_f64.to_radians().cos();
        assert!((b.landing - b.clean - 1.10 * factor).abs() < 1e-12);
        for set in [a, b] {
            assert!(set.clean < set.takeoff && set.takeoff < set.landing);
        }
    }

    #[test]
    fn invalid_geometry_or_references_are_refused() {
        let reference = a320_reference();
        for bad in [
            WingLiftGeometry {
                sweep_c4_deg: f64::NAN,
                ..reference.geometry
            },
            WingLiftGeometry {
                flapped_area_ratio: 0.0,
                ..reference.geometry
            },
            WingLiftGeometry {
                sweep_c4_deg: 80.0,
                ..reference.geometry
            },
        ] {
            assert_eq!(reference.scale(&bad), None);
        }
        let no_increment = ClmaxReference {
            set: ClmaxSet {
                clean: 1.5,
                takeoff: 1.5,
                landing: 2.6,
            },
            ..reference
        };
        let moved = WingLiftGeometry {
            sweep_c4_deg: 30.0,
            ..reference.geometry
        };
        assert_eq!(no_increment.scale(&moved), None);
    }
}
