// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing forward-limit tail requirement: the most negative tail lift
//! coefficient the stabiliser may carry, trimmed (elevator neutral) at the
//! landing reference speed, and still keep its angle of attack above the
//! stall angle of a stabiliser with leading-edge ice when the aircraft is
//! then accelerated to the flap placard speed and pushed over.
//!
//! Criterion: E. Obert, *Aerodynamic Design of Transport Aircraft*, IOS
//! Press, 2009, ch. 31, p. 406, Fig. 31.22 (Fokker F28, from AGARD CP-160,
//! paper 10): the stabiliser set for zero stick force at a lower trim speed
//! in the landing configuration at forward CG; the speed increased to the
//! flap placard with the elevator, or combined with an `n = 0.5` push-over,
//! brings the stabiliser angle of attack to the maximum angle of attack of
//! the stabiliser with leading-edge ice. The pitching velocity of the
//! push-over adds negative tail angle of attack (p. 462). CS 25.143 and
//! 25.145 are the certification background.
//!
//! Frame, units and signs: angles in radians, tail angles of attack and
//! lift coefficients negative for download; lift coefficients on their own
//! reference areas. The stabiliser section is symmetric, so with the
//! elevator neutral `CL_h = a_h alpha_h`.
//!
//! Terms, each with its source:
//! - trim speed `V_REF = 1.23 V_S` (CS 25.125(b)(2), the landing reference
//!   speed), so `CL_trim = CL_max,land / 1.23^2`;
//! - placard speed `V_FE = 1.8 V_S`, the least design flap speed of the
//!   landing flap position at maximum landing weight (14 CFR 25.335(e)(3)
//!   (iii)); `V_FE` may not exceed `V_F` (14 CFR 25.1511). The F28 example
//!   has its placard at about 1.87 `V_S` (Fig. 31.22);
//! - wing angle-of-attack change `(CL_trim - CL_FE) / a_w`, less the
//!   downwash change `2 (CL_trim - CL_FE) / (pi A)` of the envelope's
//!   downwash model (`super::ths_trim`);
//! - the push-over at `n = 0.5` reaching `CL_FE` (Fig. 31.22) flies at
//!   `V_FE / sqrt 2`, pitching at `q = 0.5 g / V`; the tail at arm `l_h`
//!   sees `q l_h / V = g l_h / V_FE^2` more negative angle of attack;
//! - smooth stall angle of the tail: `CL_h,max / a_h` (Raymer, *Aircraft
//!   Design: A Conceptual Approach*, 6th ed., eq. 12.17, p. 407, symmetric
//!   section so no zero-lift term) plus the non-linear increment of Fig.
//!   12.11 (p. 408) against leading-edge sweep and the sharpness parameter
//!   `Delta y = 26 t/c` of a NACA four-digit section (Table 12.1, p. 405);
//!   `CL_h,max` is the clean (elevator-neutral) value of
//!   `super::rotation::tail_download_cap` at the placard speed;
//! - iced stall angle: the smooth one less the decrement of the ice accretion
//!   of the stabiliser's protection ([`IceAccretion`]), elevator neutral, from
//!   the published tailplane tests listed there.

// Why reported and not governing.
//
// Accretion by certification: CS-25 Appendix C Part II (a) asks for the most
// critical accretion of each flight phase; on an unprotected surface that is
// the holding ice of (a)(4) carried into the landing ice of (a)(6), on a
// protected one the accretion of normal system operation, and CS 25.143(i)(2)
// flies the push-over with it. [`IceAccretion::of`] follows that.
//
// Datum: the only published holding-ice stall-angle decrement is the DHC-6
// tailplane section's (9.5 deg, 1.45 m chord). An ice shape does not scale
// with the chord, so on a transport stabiliser of 3 to 7 m mean chord the same
// exposure is a several times smaller ice height over chord; no published
// datum gives the decrement at that scale (the commercial-transport tail
// section of NASA/TP-2000-210031 is a 2 to 18 min glaze exposure on a model,
// 1 to 8 deg over three sections; the Fokker F28 value, 3.5 to 4 deg, is
// simulated roughness).
//
// Fleet evidence that the check cannot govern as formulated (an 18-preset run
// with the check governing): with the 9.5 deg datum the unprotected
// A320-200 limit is 28 %MAC against a certified forward limit of 17 %MAC, and
// on nine of the eighteen registered types (every wide-body) the limit lies aft
// of the type's own aft limit, 34 to 52 %MAC. With the F28 roughness decrement
// the same balance leaves the 747-400 and 787-9 near 33 %MAC and the DC-10 near
// 19 %MAC against published forward limits of 8 %MAC. Certified types therefore
// do not meet this manoeuvre (trim at V_REF, acceleration to V_FE at constant
// stabiliser, 0.5 g push-over: the F28 design practice of Obert's figure) at
// their forward limits. Two reasons are known: the real stabilisers carry
// inverted camber the generic symmetric section here has not (Obert quotes a
// trimmed tail lift coefficient of -0.8 at the forward limit, p. 406, against
// the -0.34 to -0.57 this check allows), and the CS 25.143(i)(2) push-over is
// entered from trim at the test speed, not from the approach trim. The value
// stays printed beside the governing limit.

/// Stall-angle decrement of a stabiliser with inter-cycle ice, deg: 2.3 deg
/// (maximum lift -30 %), NASA/TM-1999-208901 (Ratvasky, Van Zante and Riley,
/// AIAA-99-0370, 1999), Table 1: the DHC-6 Twin Otter tailplane section,
/// full-scale two-dimensional model, ice cast from the Icing Research Tunnel
/// with the boot cycled every 3 min. The Fokker F28 stabiliser with simulated
/// ice roughness (Obert 2009, Fig. 31.24, p. 407: about -20 deg smooth, -16 to
/// -16.5 deg iced) is the same order, 3.5 to 4 deg.
const INTER_CYCLE_STALL_ANGLE_DECREMENT_DEG: f64 = 2.3;
/// Stall-angle decrement of a stabiliser with holding ice, deg: 9.5 deg
/// (maximum lift -50 %), the same table: the LEWICE shape of a 45 min exposure
/// at 120 kt, LWC 0.5 g/m^3, MVD 20 um, -4 C, and the 1984 natural-icing shape,
/// both 9.5 deg (NASA/CR-2000-209921 Vol. 1, p. 26, Fig. 14: -18.6 deg clean to
/// about -8.2 deg, 10.4 deg). The nearest published datum to CS-25 Appendix C
/// Part II holding ice; no holding-ice datum of a large-transport tail was found.
/// A commercial-transport tailplane section with glaze ice of 2 to 18 min loses
/// stall angle of the same order (NASA/TP-2000-210031, Addy, Fig. 15 (b), p. 20;
/// "1 to 8 deg" over its three models, p. 11).
const HOLDING_ICE_STALL_ANGLE_DECREMENT_DEG: f64 = 9.5;

/// The ice accretion a stabiliser is checked against, by its ice protection
/// (CS-25 Appendix C Part II): the accretion of normal system operation on a
/// protected surface, the critical accretion on an unprotected one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IceAccretion {
    /// Residual ice between the cycles of a protected leading edge.
    InterCycle,
    /// Ice accreted in the holding pattern on an unprotected leading edge.
    Holding,
}

impl IceAccretion {
    /// The accretion of a type's declared stabiliser protection; a type that
    /// declares none is checked as unprotected.
    pub(super) fn of(protection: Option<alas_config::TailIceProtection>) -> Self {
        match protection {
            Some(alas_config::TailIceProtection::Protected) => Self::InterCycle,
            Some(alas_config::TailIceProtection::Unprotected) | None => Self::Holding,
        }
    }

    /// Stall-angle decrement of the iced stabiliser against the smooth one,
    /// elevator neutral, rad.
    pub(super) fn stall_angle_decrement_rad(self) -> f64 {
        let deg = match self {
            Self::InterCycle => INTER_CYCLE_STALL_ANGLE_DECREMENT_DEG,
            Self::Holding => HOLDING_ICE_STALL_ANGLE_DECREMENT_DEG,
        };
        deg.to_radians()
    }
}
/// Landing reference speed over the stall speed, CS 25.125(b)(2).
pub(super) const REFERENCE_SPEED_OVER_STALL: f64 = 1.23;
/// Least design flap speed of the landing flap position over the stall
/// speed, 14 CFR 25.335(e)(3)(iii).
const PLACARD_SPEED_OVER_STALL: f64 = 1.8;
/// Push-over load factor of the manoeuvre, Obert 2009, p. 406.
const PUSH_OVER_LOAD_FACTOR: f64 = 0.5;

/// Raymer 6th ed., Fig. 12.11 (p. 408): `Delta alpha_CLmax`, deg, against
/// leading-edge sweep, deg, for `Delta y` = 1.2 (and below), 2, 3 and 4
/// (and above), `0.2 <= M <= 0.6`; digitised from the page, about +-0.2 deg.
const FIG_12_11_SWEEP_DEG: [f64; 12] = [
    0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0,
];
const FIG_12_11_DELTA_Y: [f64; 4] = [1.2, 2.0, 3.0, 4.0];
const FIG_12_11_DEG: [[f64; 12]; 4] = [
    [
        1.82, 1.90, 2.19, 2.67, 3.33, 4.13, 5.07, 6.17, 7.37, 8.72, 10.17, 11.78,
    ],
    [
        0.12, 0.53, 1.05, 1.64, 2.36, 3.09, 3.90, 4.78, 5.69, 6.66, 7.67, 8.75,
    ],
    [
        1.25, 1.46, 1.73, 2.02, 2.36, 2.78, 3.23, 3.71, 4.22, 4.77, 5.36, 5.99,
    ],
    [
        2.20, 2.07, 2.02, 2.04, 2.08, 2.16, 2.27, 2.39, 2.54, 2.70, 2.88, 3.08,
    ],
];

use super::chart::interpolate;

/// Raymer's `Delta alpha_CLmax`, rad, at leading-edge sweep `sweep_le_deg`
/// for a NACA four-digit section of thickness ratio `thickness`; `None`
/// (an unrecognised section) takes the smallest curve value at that sweep,
/// so it can only under-credit the tail.
#[must_use]
pub(super) fn stall_angle_increment_rad(thickness: Option<f64>, sweep_le_deg: f64) -> f64 {
    let sweep = if sweep_le_deg.is_finite() {
        sweep_le_deg.abs()
    } else {
        0.0
    };
    let at_sweep: Vec<f64> = FIG_12_11_DEG
        .iter()
        .map(|curve| interpolate(&FIG_12_11_SWEEP_DEG, curve, sweep))
        .collect();
    let deg = match thickness.filter(|t| t.is_finite() && *t > 0.0) {
        Some(t) => interpolate(&FIG_12_11_DELTA_Y, &at_sweep, 26.0 * t),
        None => at_sweep.iter().copied().fold(f64::INFINITY, f64::min),
    };
    deg.to_radians()
}

/// Everything [`landing_trim_tail_lift_coefficient`] needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct IcedTailInput {
    /// Landing maximum lift coefficient (1 g).
    pub cl_max_landing: f64,
    /// Wing lift-curve slope, per rad.
    pub wing_lift_curve_slope_per_rad: f64,
    /// Wing aspect ratio on the reference area.
    pub wing_aspect_ratio: f64,
    /// Tail lift-curve slope, per rad.
    pub tail_lift_curve_slope_per_rad: f64,
    /// Smooth, elevator-neutral tail maximum lift coefficient (magnitude)
    /// at the placard speed.
    pub tail_cl_max_clean: f64,
    /// Raymer's stall-angle increment of the tail, rad.
    pub tail_stall_angle_increment_rad: f64,
    /// Stall-angle decrement of the iced stabiliser, rad
    /// ([`IceAccretion::stall_angle_decrement_rad`]).
    pub iced_stall_angle_decrement_rad: f64,
    /// Tail arm, m (tail aerodynamic centre aft of the wing-body one).
    pub tail_arm_m: f64,
    /// Placard speed, m/s.
    pub placard_speed_m_s: f64,
    /// Gravitational acceleration, m/s^2.
    pub gravity_m_s2: f64,
}

/// The placard speed `1.8 V_S`, m/s, at mass `mass_kg` on `wing_area_m2` at
/// `cl_max_landing` (ISA sea level).
#[must_use]
pub(super) fn placard_speed_m_s(
    mass_kg: f64,
    gravity_m_s2: f64,
    wing_area_m2: f64,
    cl_max_landing: f64,
) -> f64 {
    PLACARD_SPEED_OVER_STALL
        * super::rotation_thrust::rotation_speed_m_s(
            mass_kg * gravity_m_s2,
            wing_area_m2,
            cl_max_landing,
        )
}

/// Tail angle-of-attack decrease of the manoeuvre, rad (positive).
#[must_use]
pub(super) fn manoeuvre_tail_alpha_decrease_rad(input: &IcedTailInput) -> f64 {
    let cl_trim = input.cl_max_landing / REFERENCE_SPEED_OVER_STALL.powi(2);
    let cl_placard = input.cl_max_landing / PLACARD_SPEED_OVER_STALL.powi(2);
    let wing = (cl_trim - cl_placard)
        * (1.0 / input.wing_lift_curve_slope_per_rad
            - 2.0 / (std::f64::consts::PI * input.wing_aspect_ratio));
    // `q l_h / V` at `V = V_FE sqrt(n)` with `q = (1 - n) g / V`.
    let pitching = (1.0 - PUSH_OVER_LOAD_FACTOR) * input.gravity_m_s2 * input.tail_arm_m
        / (PUSH_OVER_LOAD_FACTOR * input.placard_speed_m_s.powi(2));
    wing + pitching
}

/// The iced stabiliser's stall angle of attack, rad (magnitude).
#[must_use]
pub(super) fn iced_stall_angle_rad(input: &IcedTailInput) -> f64 {
    (input.tail_cl_max_clean / input.tail_lift_curve_slope_per_rad
        + input.tail_stall_angle_increment_rad
        - input.iced_stall_angle_decrement_rad)
        .max(0.0)
}

/// The most negative tail lift coefficient the stabiliser may be trimmed to
/// at `V_REF`, elevator neutral: `-a_h (alpha_ice - Delta alpha_h)`; positive
/// (an upload) when the manoeuvre alone reaches the iced stall angle. `NaN`
/// for invalid inputs.
#[must_use]
pub(super) fn landing_trim_tail_lift_coefficient(input: &IcedTailInput) -> f64 {
    if !(input.wing_lift_curve_slope_per_rad > 0.0
        && input.tail_lift_curve_slope_per_rad > 0.0
        && input.wing_aspect_ratio > 0.0
        && input.placard_speed_m_s > 0.0)
    {
        return f64::NAN;
    }
    -input.tail_lift_curve_slope_per_rad
        * (iced_stall_angle_rad(input) - manoeuvre_tail_alpha_decrease_rad(input))
}

/// The wing-body of the landing flaps (extended chord, [`super::extended_chord`])
/// at the wing-body aerodynamic centre `x_ac_m`: the one the landing balance
/// ([`super::landing_download`]) uses. `None` without wing-body terms or a frame.
fn flapped_wing_body(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &alas_config::AlasConfig,
    s_ref: f64,
    x_ac_m: f64,
) -> Option<super::extended_chord::FlappedWingBody> {
    let terms = super::wing_body::wing_body_terms(plane, config)?;
    let moment = super::support::landing_pitching_moment(plane, config, s_ref)?;
    Some(super::extended_chord::FlappedWingBody::new(
        terms.cl_alpha_wf,
        terms.cl_alpha_wing,
        x_ac_m,
        plane.mac_frame()?.chord_m,
        terms.zero_alpha_lift(&moment),
        super::extended_chord::extended_lift(
            plane,
            config,
            super::extended_chord::landing_flap_deg(config),
        ),
    ))
}

/// [`landing_trim_tail_lift_coefficient`] of `plane` at landing mass
/// `mass_kg`, the stabiliser checked against `ice`: the wing-body slope with
/// the landing flaps out and the tail arm from its flapped aerodynamic centre
/// (the manoeuvre is flown with the flaps down; the clean wing's slope and the
/// clean arm overstate the angle-of-attack change of the speed increase), the
/// wing's aspect ratio for the downwash, the tail slope and clean maximum lift
/// of the rotation tail (`super::rotation`) from the wing-body aerodynamic
/// centre `x_ac_wb_m` to the tail's `x_h_ac_m`. `NaN` with no tail.
#[must_use]
pub(super) fn landing_trim_cl_h(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &alas_config::AlasConfig,
    s_ref: f64,
    x_ac_wb_m: f64,
    x_h_ac_m: f64,
    mass_kg: f64,
    ice: IceAccretion,
) -> f64 {
    let (Some(wing), Some(tail)) = (plane.wings.first(), plane.wings.get(1)) else {
        return f64::NAN;
    };
    let cl_max_landing = config.performance.cl_max_land;
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    let wing_aspect_ratio = plane.b_ref * plane.b_ref / s_ref;
    let reference = tail.reference_planform();
    let tail_slope = alas_stab::static_stability::datcom(
        reference.aspect_ratio(),
        tail.mean_sweep_angle(0.25).to_radians(),
        tail.taper_ratio(),
        0.0,
    );
    let placard = placard_speed_m_s(mass_kg, gravity_m_s2, s_ref, cl_max_landing);
    let (tail_cl_max_clean, _) = super::rotation::tail_download_cap(tail, config, placard);
    let flapped = flapped_wing_body(plane, config, s_ref, x_ac_wb_m);
    let input = IcedTailInput {
        cl_max_landing,
        wing_lift_curve_slope_per_rad: flapped.map_or_else(
            || {
                alas_stab::static_stability::datcom(
                    wing_aspect_ratio,
                    wing.mean_sweep_angle(0.25).to_radians(),
                    wing.taper_ratio(),
                    0.2,
                )
            },
            |flapped| flapped.slope,
        ),
        wing_aspect_ratio,
        tail_lift_curve_slope_per_rad: tail_slope,
        tail_cl_max_clean,
        tail_stall_angle_increment_rad: stall_angle_increment_rad(
            super::tail_stall::symmetric_thickness(&config.geometry.empennage.tail_airfoil),
            tail.mean_sweep_angle(0.0),
        ),
        iced_stall_angle_decrement_rad: ice.stall_angle_decrement_rad(),
        tail_arm_m: x_h_ac_m - x_ac_wb_m - flapped.map_or(0.0, |flapped| flapped.ac_shift_m),
        placard_speed_m_s: placard,
        gravity_m_s2,
    };
    landing_trim_tail_lift_coefficient(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> IcedTailInput {
        IcedTailInput {
            cl_max_landing: 2.6,
            wing_lift_curve_slope_per_rad: 5.0,
            wing_aspect_ratio: 9.0,
            tail_lift_curve_slope_per_rad: 3.6,
            tail_cl_max_clean: 1.2,
            tail_stall_angle_increment_rad: 3.0_f64.to_radians(),
            iced_stall_angle_decrement_rad: 4.0_f64.to_radians(),
            tail_arm_m: 15.0,
            placard_speed_m_s: 110.0,
            gravity_m_s2: 9.806_65,
        }
    }

    #[test]
    fn the_stall_increment_reads_raymer_fig_12_11() {
        // Delta y = 26 * 0.12 = 3.12, between the 3 and 4 curves.
        let at_30 = stall_angle_increment_rad(Some(0.12), 30.0).to_degrees();
        assert!((at_30 - (3.23 + 0.12 * (2.27 - 3.23))).abs() < 1.0e-9);
        // Delta y below 1.2 and above 4 clamp to the end curves.
        assert!((stall_angle_increment_rad(Some(0.02), 0.0).to_degrees() - 1.82).abs() < 1.0e-9);
        assert!((stall_angle_increment_rad(Some(0.2), 60.0).to_degrees() - 3.08).abs() < 1.0e-9);
        // An unrecognised section takes the smallest value.
        assert!((stall_angle_increment_rad(None, 0.0).to_degrees() - 0.12).abs() < 1.0e-9);
    }

    #[test]
    fn the_trim_requirement_is_the_iced_stall_angle_less_the_manoeuvre() {
        let i = input();
        let cl_trim = 2.6 / 1.23_f64.powi(2);
        let cl_fe = 2.6 / 1.8_f64.powi(2);
        let wing = (cl_trim - cl_fe) * (1.0 / 5.0 - 2.0 / (std::f64::consts::PI * 9.0));
        let pitching = 9.806_65 * 15.0 / 110.0_f64.powi(2);
        let iced = 1.2 / 3.6 + 3.0_f64.to_radians() - 4.0_f64.to_radians();
        let expected = -3.6 * (iced - wing - pitching);
        assert!((landing_trim_tail_lift_coefficient(&i) - expected).abs() < 1.0e-12);
        assert!(expected < 0.0);
        // A faster placard (less pitching term) allows more download.
        let mut faster = i;
        faster.placard_speed_m_s = 150.0;
        assert!(
            landing_trim_tail_lift_coefficient(&faster) < landing_trim_tail_lift_coefficient(&i)
        );
        let mut bad = i;
        bad.tail_lift_curve_slope_per_rad = 0.0;
        assert!(landing_trim_tail_lift_coefficient(&bad).is_nan());
    }
}
