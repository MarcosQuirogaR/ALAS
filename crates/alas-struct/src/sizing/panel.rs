// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Buckling-limited compressive allowable of the stiffened wing-box cover.
//!
//! A cover of thin skin spanning the box between spars, unstiffened, buckles
//! at a stress far below any metal allowable: a simply supported plate of
//! thickness t and width b fails at 3.62 E (t/b)^2 (Niu, *Airframe Structural
//! Design*, 1988, Eq. 5.7.1, book p.157). A 6 mm aluminium skin 2.7 m wide
//! carries about 1.3 MPa. Real covers are skin-stringer panels, and the
//! section model here already holds that material: the spar caps and the
//! cover skin are the smeared stiffened panel of Torenbeek, *Advanced Aircraft
//! Design*, 2013, Sec. 11.3.2 ("The upper and lower stiffened skin panels
//! including spar flanges ... resist bending", Eqs. 11.30-11.34, book p.331).
//! What the model lacked was the allowable that goes with it.
//!
//! # Allowable
//!
//! Torenbeek Eq. 11.71 (book p.351, verified on the page): the compressive
//! stress at failure of a stiffened panel is proportional to the square root
//! of the structural index,
//!
//! `sigma_c = k_F sqrt(E_t N / r)` for `sigma_c < 350 MPa`,
//!
//! with `N` the end load per unit panel width, `r` the rib pitch and `k_F`
//! Farrar's efficiency factor (0.75 for integrally stiffened panels with
//! unflanged stiffeners, 0.88 for Z, J and hat stringers, up to 1.15 for
//! optimised Y stringers; Niu Sec. 5.6-5.7 gives the same family with
//! `F = 0.81` for the optimum unflanged panel, Eq. 5.7.4). The curve
//! recommended for weight prediction (Fig. 11.11) uses `k_F = 0.85` in the
//! elastic range, levels off at 420 MPa for highly loaded panels (440 MPa
//! yield is the limit for avoiding skin buckling; 420 MPa keeps a 5 % reserve
//! for combined compression and shear; stated for transports above 50 t), and
//! is blended between 350 and 420 MPa.
//!
//! For a smeared cover of equivalent thickness `t_eq` (skin plus stringers
//! plus spar flanges) failing at `sigma`, `N = sigma t_eq`, so the relation is
//! closed: `sigma = k_F^2 E t_eq / r`. That is what is evaluated here; the
//! allowable then depends on the material actually installed at the station
//! and on the rib pitch, which are the two things Eq. 11.71 says it depends on.
//!
//! # Limits of this model
//!
//! * Fig. 11.11 is a chart for Al 7075-T6 (`E_t` = 71 GPa); the blend between
//!   350 and 420 MPa is not tabulated. The blend used here is
//!   `350 + 70 (1 - exp(-(s - 350) / 70))`, which is continuous with unit
//!   slope at 350 MPa and tends to 420 MPa. It is an interpolation choice, not
//!   read data. For CFRP proxies the same relation is used as a stiffened-panel
//!   proxy; their strain allowable ([`crate::allowables`]) is below it.
//! * `k_F` is a single number for a generic stiffener type. Stringer pitch,
//!   stiffener type and section are not designed here; the cover is smeared.
//! * Tabulated per-alloy `Ftu`, `Fty`, `Fcy` (MMPDS-2022, CMH-17) were not
//!   retrievable in this lane; `f_allow` of the material remains the other
//!   limit. Nothing here is a certified allowable.

use crate::allowables::bending_allowable_pa;
use alas_config::materials::MaterialSpec;

use super::WingboxSizing;

/// Farrar efficiency of the curve Torenbeek recommends for weight prediction
/// (2013, Fig. 11.11 and text, book p.351).
pub const FARRAR_EFFICIENCY: f64 = 0.85;

/// Stress above which the elastic structural-index relation stops applying,
/// Pa (Torenbeek 2013, book p.351).
pub const ELASTIC_LIMIT_PA: f64 = 350.0e6;

/// Upper limit of the compressive stress of a highly loaded transport panel,
/// Pa (Torenbeek 2013, book p.351: 420 MPa with a 5 % reserve).
pub const COMPRESSION_LIMIT_PA: f64 = 420.0e6;

/// Local buckling coefficient of a skin simply supported on its stiffener
/// lines, `f_b = 3.62 E (t/b)^2` (Niu 1988, Eq. 5.7.1, book p.157).
pub const SIMPLY_SUPPORTED_PLATE_COEFFICIENT: f64 = 3.62;

/// Elastic local buckling stress of a skin of thickness `thickness_m` between
/// supports `width_m` apart, Pa (Niu Eq. 5.7.1). Zero for non-physical input.
pub fn skin_buckling_stress_pa(e_pa: f64, thickness_m: f64, width_m: f64) -> f64 {
    if !(e_pa > 0.0 && thickness_m > 0.0 && width_m > 0.0) {
        return 0.0;
    }
    SIMPLY_SUPPORTED_PLATE_COEFFICIENT * e_pa * (thickness_m / width_m).powi(2)
}

/// Largest stiffener pitch at which the skin still reaches `stress_pa` without
/// buckling, m: `b = t sqrt(3.62 E / sigma)`. A skin wider than this between
/// stiffeners is not fully effective at that stress.
pub fn maximum_stiffener_pitch_m(e_pa: f64, thickness_m: f64, stress_pa: f64) -> f64 {
    if !(e_pa > 0.0 && thickness_m > 0.0 && stress_pa > 0.0) {
        return 0.0;
    }
    thickness_m * (SIMPLY_SUPPORTED_PLATE_COEFFICIENT * e_pa / stress_pa).sqrt()
}

/// Compressive stress at failure of a smeared stiffened cover, Pa, from
/// Torenbeek Eq. 11.71 with `N = sigma t_eq` (see the module documentation).
///
/// `smeared_thickness_m` is the cover cross-section area per unit width
/// (skin, stringers and spar flanges), `rib_pitch_m` the rib spacing. Returns
/// zero for non-physical input and never exceeds
/// [`COMPRESSION_LIMIT_PA`]. Monotone increasing in the smeared thickness and
/// in `E`, monotone decreasing in the rib pitch.
pub fn stiffened_cover_allowable_pa(e_pa: f64, smeared_thickness_m: f64, rib_pitch_m: f64) -> f64 {
    if !(e_pa > 0.0 && smeared_thickness_m > 0.0 && rib_pitch_m > 0.0)
        || !e_pa.is_finite()
        || !smeared_thickness_m.is_finite()
        || !rib_pitch_m.is_finite()
    {
        return 0.0;
    }
    let elastic = FARRAR_EFFICIENCY * FARRAR_EFFICIENCY * e_pa * smeared_thickness_m / rib_pitch_m;
    if elastic <= ELASTIC_LIMIT_PA {
        elastic
    } else {
        let span = COMPRESSION_LIMIT_PA - ELASTIC_LIMIT_PA;
        ELASTIC_LIMIT_PA + span * (1.0 - (-(elastic - ELASTIC_LIMIT_PA) / span).exp())
    }
}

/// Smeared cover thickness, m: the skin plus the area of the flanges and
/// stringers that act with it, spread over the box width (Torenbeek's panel
/// area `A(y)` divided by the width it covers).
pub fn smeared_cover_thickness_m(skin_m: f64, flange_area_m2: f64, box_width_m: f64) -> f64 {
    if box_width_m > 0.0 {
        skin_m + flange_area_m2 / box_width_m
    } else {
        skin_m
    }
}

/// Cover geometry that does not change while the caps of one station are
/// searched: the box width between the outermost active spars and the installed
/// rib pitch.
#[derive(Debug, Clone, Copy)]
pub(super) struct CoverPanel {
    box_width_m: f64,
    rib_pitch_m: f64,
    skin_m: f64,
}

impl CoverPanel {
    pub(super) fn new(sizing: &WingboxSizing, station: usize) -> Self {
        let mut active = sizing.spars.iter().filter(|spar| spar.h[station] > 0.0);
        let first = active.next().map(|spar| spar.chord_fraction);
        let last = sizing
            .spars
            .iter()
            .rev()
            .find(|spar| spar.h[station] > 0.0)
            .map(|spar| spar.chord_fraction);
        let box_width_m = match (first, last) {
            (Some(front), Some(rear)) => (rear - front) * sizing.chord[station],
            _ => 0.0,
        };
        let installed = sizing.installed_rib_spacing_m();
        let rib_pitch_m = if installed.is_finite() && installed > 0.0 {
            installed
        } else {
            sizing.rib_spacing_m
        };
        Self {
            box_width_m,
            rib_pitch_m,
            skin_m: sizing.t_skin,
        }
    }

    /// Compressive allowable of the cover for a material of modulus `e_pa` and
    /// material limit `material_limit_pa`, Pa, given the total flange area (all
    /// active spars, one cover) installed at the station: the lesser of the
    /// material limit and the stiffened-panel curve.
    pub(super) fn allowable_for_pa(
        self,
        e_pa: f64,
        material_limit_pa: f64,
        flange_area_m2: f64,
    ) -> f64 {
        if !(self.rib_pitch_m.is_finite() && self.rib_pitch_m > 0.0 && self.box_width_m > 0.0) {
            return material_limit_pa;
        }
        let t_eq = smeared_cover_thickness_m(self.skin_m, flange_area_m2, self.box_width_m);
        material_limit_pa.min(stiffened_cover_allowable_pa(e_pa, t_eq, self.rib_pitch_m))
    }

    /// [`Self::allowable_for_pa`] for a registered material and its product
    /// allowable.
    pub(super) fn allowable_pa(self, material: &MaterialSpec, flange_area_m2: f64) -> f64 {
        self.allowable_for_pa(
            material.e_pa,
            bending_allowable_pa(material),
            flange_area_m2,
        )
    }
}

/// Total one-cover flange area at a station, m^2: the sum of every spar cap
/// area (`w_cap * t_cap`) installed there.
pub(super) fn flange_area_m2(sizing: &WingboxSizing, station: usize) -> f64 {
    sizing
        .spars
        .iter()
        .filter(|spar| spar.h[station] > 0.0)
        .map(|spar| spar.w_cap[station] * spar.t_cap[station])
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const E_AL: f64 = 71.0e9;

    #[test]
    fn plate_buckling_matches_the_closed_form_and_flags_the_unstiffened_cover() {
        // 6 mm aluminium skin spanning a 2.7 m box: about 1.3 MPa (audit STR-03).
        let unstiffened = skin_buckling_stress_pa(E_AL, 0.006, 2.7);
        assert!((unstiffened - 3.62 * E_AL * (0.006_f64 / 2.7).powi(2)).abs() < 1.0);
        assert!(unstiffened < 2.0e6);
        // That is two orders of magnitude below the 380 MPa allowable credited.
        assert!(380.0e6 / unstiffened > 100.0);
        // A 0.15 m stringer pitch lifts the skin above that allowable.
        assert!(skin_buckling_stress_pa(E_AL, 0.006, 0.15) > 380.0e6);
    }

    #[test]
    fn stiffener_pitch_for_a_given_stress_inverts_the_plate_formula() {
        let b = maximum_stiffener_pitch_m(E_AL, 0.006, 380.0e6);
        assert!((skin_buckling_stress_pa(E_AL, 0.006, b) - 380.0e6).abs() < 1.0);
        // The skin of an aluminium cover at 380 MPa needs a pitch of about 26 t.
        assert!((b / 0.006 - (3.62 * E_AL / 380.0e6).sqrt()).abs() < 1.0e-9);
    }

    #[test]
    fn the_stiffened_allowable_is_the_farrar_relation_below_the_elastic_limit() {
        // k_F^2 E t / r = 0.7225 * 71e9 * 0.004 / 1.0 = 205.2 MPa.
        let value = stiffened_cover_allowable_pa(E_AL, 0.004, 1.0);
        assert!((value - 0.85_f64.powi(2) * E_AL * 0.004).abs() < 1.0);
        // The same value follows from sigma = k_F sqrt(E N / r) with N = sigma t.
        let n = value * 0.004;
        assert!((value - 0.85 * (E_AL * n / 1.0).sqrt()).abs() < 1.0e-3 * value);
    }

    #[test]
    fn the_stiffened_allowable_is_monotone_continuous_and_capped() {
        let mut previous = 0.0;
        for step in 1..400 {
            let t = 1.0e-3 * step as f64 * 0.1;
            let value = stiffened_cover_allowable_pa(E_AL, t, 0.8);
            assert!(value >= previous, "monotone in thickness");
            assert!(value <= COMPRESSION_LIMIT_PA);
            previous = value;
        }
        // Continuous with unit slope at the elastic limit: no jump.
        let below =
            stiffened_cover_allowable_pa(E_AL, 0.999 * 350.0e6 * 0.8 / (0.7225 * E_AL), 0.8);
        let above =
            stiffened_cover_allowable_pa(E_AL, 1.001 * 350.0e6 * 0.8 / (0.7225 * E_AL), 0.8);
        assert!(above > below && above - below < 2.5e-3 * 350.0e6);
        // Longer rib pitch lowers the allowable.
        assert!(
            stiffened_cover_allowable_pa(E_AL, 0.004, 1.2)
                < stiffened_cover_allowable_pa(E_AL, 0.004, 0.8)
        );
        // Very heavily loaded panels approach 420 MPa.
        assert!(stiffened_cover_allowable_pa(E_AL, 1.0, 0.5) > 0.999 * COMPRESSION_LIMIT_PA);
    }

    #[test]
    fn non_physical_input_gives_no_allowable() {
        for (e, t, r) in [
            (0.0, 0.01, 1.0),
            (E_AL, 0.0, 1.0),
            (E_AL, 0.01, 0.0),
            (E_AL, f64::NAN, 1.0),
            (E_AL, 0.01, f64::INFINITY),
        ] {
            assert_eq!(stiffened_cover_allowable_pa(e, t, r), 0.0);
        }
        assert_eq!(skin_buckling_stress_pa(E_AL, -1.0, 1.0), 0.0);
        assert_eq!(maximum_stiffener_pitch_m(E_AL, 0.006, 0.0), 0.0);
    }

    #[test]
    fn smeared_thickness_adds_the_flange_area_over_the_box_width() {
        assert!((smeared_cover_thickness_m(0.006, 0.02, 4.0) - 0.011).abs() < 1.0e-15);
        assert_eq!(smeared_cover_thickness_m(0.006, 0.02, 0.0), 0.006);
    }
}
