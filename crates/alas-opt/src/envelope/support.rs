// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small pure helpers that `assess_model_cg_envelope` needs for the physical
//! CG-limits helper.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};

use alas_config::AlasConfig;

/// Wing aerodynamic centre as a fraction of MAC aft of LEMAC: the
/// quarter-chord point, the fallback of [`wing_body_ac_frac`] without a
/// finite set of wing/fuselage terms.
pub(super) const SCISSOR_X_AC_WB_FRAC: f64 = 0.25;

/// Wing-body aerodynamic centre, fraction of MAC aft of LEMAC, for the
/// landing-trim, rotation and takeoff-trim balances. The "wing" of those
/// balances is the airplane less its horizontal tail, fuselage and nacelles
/// included (Torenbeek, *Advanced Aircraft Design*, Wiley, 2013, sec. 6.3.1,
/// p. 165): the wing's quarter chord moved by the fuselage, the carry-over
/// lift loss, the nacelles and the propellers of Torenbeek's *Synthesis of
/// Subsonic Airplane Design*, App. E ([`super::wing_body`]). The centre is a
/// station of the airplane: the wing's own lattice aerodynamic centre (the
/// geometric quarter chord of its MAC when the lattice gives none) and the
/// shifts in metres, converted into the reference frame of
/// [`Airplane::mac_frame`] last, so a published chord or LEMAC moves the
/// fraction and never the station. [`SCISSOR_X_AC_WB_FRAC`] without a frame
/// or a finite set of terms.
pub(super) fn wing_body_ac_frac(plane: &Airplane, config: &AlasConfig) -> f64 {
    let (Some(frame), Some(terms)) = (
        plane.mac_frame(),
        super::wing_body::wing_body_terms(plane, config),
    ) else {
        return SCISSOR_X_AC_WB_FRAC;
    };
    let x_ac_wing_m =
        super::wing_body::lattice_wing_ac_m(plane, config).unwrap_or(terms.x_c4_mac_m);
    let fraction = (terms.ac_x_m(x_ac_wing_m) - frame.x_lemac_m) / frame.chord_m;
    if fraction.is_finite() {
        fraction
    } else {
        SCISSOR_X_AC_WB_FRAC
    }
}

/// The reference-frame scales of the wing's own moment terms:
/// `(own MAC / reference chord, drawn wing area / reference area)`, so a
/// coefficient on the wing's own chord and area, `C_m S_w c_w`, is the same
/// moment as `C_m' S_ref c_ref`. `(1, 1)` without a wing, a frame or a finite
/// ratio.
fn own_to_reference_scales(plane: &Airplane) -> (f64, f64) {
    let Some((wing, frame)) = plane.wings.first().zip(plane.mac_frame()) else {
        return (1.0, 1.0);
    };
    let chord = wing.mean_aerodynamic_chord() / frame.chord_m;
    let area = wing.projected_area() / plane.s_ref;
    let finite = |value: f64| {
        if value.is_finite() && value > 0.0 {
            value
        } else {
            1.0
        }
    };
    (finite(chord), finite(area))
}

/// The wing-body pitching moment about [`wing_body_ac_frac`] with the flap
/// setting of `wing`, the wing's own terms ([`landing_pitching_moment`],
/// [`takeoff_pitching_moment`]): `(C_m,ac)_wf = (C_m,ac)_w + Delta_f C_m,ac`
/// (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, App. E,
/// eq. E-39 and E-40, printed p. 480), the fuselage term with the clean lift
/// at zero fuselage angle of attack: the flap's moment, fuselage included, is
/// already the wing's `-0.3 Delta C_L0` term. Every term is on the wing's own
/// MAC; the section-moment integral is also on the drawn wing's own area,
/// the flap and fuselage terms on the reference area (their lift and
/// eq. E-40 are normalised by it). The sum is carried onto the reference
/// chord and area ([`own_to_reference_scales`]). The wing moment alone
/// without a finite set of fuselage terms, and `None` without the wing's.
fn wing_body_cm_ac(
    plane: &Airplane,
    config: &AlasConfig,
    wing: Option<LandingPitchingMoment>,
) -> Option<f64> {
    let wing = wing?;
    let (chord_scale, area_scale) = own_to_reference_scales(plane);
    let fuselage = super::wing_body::wing_body_terms(plane, config)
        .map(|terms| terms.fuselage_cm_ac(&wing))
        .filter(|cm| cm.is_finite())
        .unwrap_or(0.0);
    let cm_ac =
        (wing.clean_cm_ac * area_scale + (wing.cm_ac - wing.clean_cm_ac) + fuselage) * chord_scale;
    Some(if cm_ac.is_finite() { cm_ac } else { wing.cm_ac })
}

/// The wing/fuselage lift coefficient at the ground attitude before rotation,
/// for the rotation and takeoff-trim balances: the lift at zero fuselage angle
/// of attack with the takeoff flap (Torenbeek, *Synthesis of Subsonic Airplane
/// Design*, 1982, App. E, eq. E-29 at `alpha_f = 0`), times its gain in
/// ground effect at that angle ([`super::wing_body::ground_effect_lift_ratio`],
/// eq. G-76, MAC heights over the model's ground plane).
///
/// Assumption, the same for every type: the fuselage reference line (the
/// body `x` axis) is level on the ground plane at the ground attitude, so the
/// body angle of attack in the roll is zero. No preset carries a published
/// ground attitude; a nose-down or nose-up static attitude would move the
/// lift by `(C_L,alpha)_wf` per radian of it.
///
/// Falls back to the configured fraction of `CL_max,TO` without a finite
/// estimate.
pub(super) fn takeoff_ground_lift(plane: &Airplane, config: &AlasConfig, s_ref: f64) -> f64 {
    let fallback =
        config.performance.cl_max_to * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
    let Some(wing) = takeoff_pitching_moment(plane, config, s_ref) else {
        return fallback;
    };
    let ground_z = plane
        .fuselages
        .first()
        .map(|fuselage| ground_z_m(fuselage, config));
    super::wing_body::wing_body_terms(plane, config)
        .map(|terms| {
            let free_air = terms.zero_alpha_lift(&wing);
            let ratio = match (ground_z, plane.wings.first()) {
                (Some(ground_z), Some(main)) => {
                    let mac = main.carried_box();
                    super::wing_body::ground_effect_lift_ratio(
                        plane,
                        &terms,
                        free_air,
                        mac.aerodynamic_center(0.25)[2] - ground_z,
                        mac.aerodynamic_center(1.0)[2] - ground_z,
                    )
                }
                _ => 1.0,
            };
            free_air * ratio
        })
        .filter(|lift| lift.is_finite() && *lift > 0.0)
        .unwrap_or(fallback)
}
/// Landing-configuration wing-body pitching moment about its own AC, the
/// fallback of [`landing_cm_ac_wb`] when the wing gives no finite estimate,
/// dimensionless, nose-down.
pub(super) const SCISSOR_CM_AC_WB_LANDING: f64 = -0.20;
/// Ratio of the flap/slat pitching-moment increment about the aerodynamic
/// centre to the flap/slat lift increment at zero angle of attack,
/// `Delta C_M,ac = -0.3 Delta C_L(alpha=0)`: Torenbeek, *Advanced Aircraft
/// Design*, Wiley, 2013, sec. 6.3.3, p. 168, typical input of the
/// forward-limit trim balance (eq. 6.2) with flaps and slats down. The flap
/// increment acts 0.3 of the chord aft of the centre.
///
/// Independent check on the landing flap, not a calibration: Fokker 100
/// tail-off wind-tunnel data (Obert, *Aerodynamic Design of Transport
/// Aircraft*, 2009, Fig. 27.25, p. 357; flaps 42 deg, M 0.19): `C_m` about
/// 0.30 MAC is about -0.24 at `C_L` 1.8 with the test aerodynamic centre near
/// 0.20 MAC, so `C_m,ac` is about -0.42 (the line's value at `C_L = 0`), which
/// is `-0.3 Delta C_L0` for `Delta C_L0` near 1.4.
pub(super) const LANDING_CM_AC_PER_ZERO_ALPHA_LIFT: f64 = -0.3;
/// Two-dimensional change of the zero-lift angle of a trailing-edge flap at
/// the takeoff setting, deg: Raymer, *Aircraft Design: A Conceptual
/// Approach*, 6th ed., sec. 12.6, "-10 deg at the takeoff setting", scaled to
/// the wing by eq. 12.22.
pub(super) const TAKEOFF_FLAP_ZERO_LIFT_SHIFT_DEG: f64 = -10.0;

/// The terms of the landing-configuration wing-body pitching moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LandingPitchingMoment {
    /// Wing lift-curve slope, per radian (DATCOM, Mach 0.2).
    pub(super) lift_curve_slope_per_rad: f64,
    /// Clean wing pitching moment about its aerodynamic centre, from the
    /// section camber lines.
    pub(super) clean_cm_ac: f64,
    /// Flap lift increment at zero angle of attack.
    pub(super) flap_zero_alpha_lift: f64,
    /// Clean wing lift at zero body angle of attack: `a_w` times the
    /// chord-weighted mean of the section incidence less the thin-airfoil
    /// zero-lift angle.
    pub(super) clean_zero_alpha_lift: f64,
    /// The part of [`Self::clean_zero_alpha_lift`] from the root section's
    /// incidence alone, `a_w (i_w - alpha_l0,r)` (Torenbeek 1982, eq. E-29).
    pub(super) root_zero_alpha_lift: f64,
    /// `C_M,ac` with landing flaps, nose-down negative.
    pub(super) cm_ac: f64,
}

/// The wing pitching moment about its aerodynamic centre with flaps, from the
/// built wing (the takeoff setting; the landing setting replaces the flap
/// terms, [`landing_pitching_moment`]):
/// `C_M,ac = C_M,ac,clean - 0.3 Delta C_L,flap(alpha=0)` (Torenbeek 2013,
/// sec. 6.3.3, p. 168, for the flap/slat term):
///
/// - `C_M,ac,clean = A cos^2(Lambda) / (A + 2 cos(Lambda)) c_m,sections`
///   (Sadraey, *Aircraft Design: A Systems Engineering Approach*, Wiley,
///   2013, sec. 6.2.1, eq. 6.26, p. 275, without its `+0.01 alpha_t` twist
///   term, see below), `A` the
///   reference aspect ratio, `Lambda` the quarter-chord sweep and
///   `c_m,sections = integral(c^2 c_m,c/4 dy) / integral(c^2 dy)` the
///   strip-theory section moment normalised by the mean aerodynamic chord,
///   `c_m,c/4` the thin-airfoil moment about the quarter chord. Both are
///   independent of the angle of attack, so a rigid rotation of the wing
///   leaves the clean moment unchanged;
/// - `Delta C_L,flap(alpha=0) = a_w Delta alpha_0L,flap`, with
///   `Delta alpha_0L,flap = 10 deg (S_flapped / S) cos(Lambda_HL)` (Raymer
///   6th ed., eq. 12.22, takeoff setting), `S_flapped` the straight-tapered
///   wing area spanned by the flap and `Lambda_HL` its hinge-line sweep;
/// - `a_w`: DATCOM slope of the reference wing at Mach 0.2.
///
/// Sadraey's twist term is left out on physical grounds. With its sign
/// convention (`alpha_t` in degrees, washout negative, p. 275 and sec. 5.10,
/// p. 223) washout makes the moment more nose-down. At zero wing lift a
/// washed-out wing carries upload inboard and download at the tips; on an
/// aft-swept wing the tips lie aft, so that couple is nose-up, the opposite
/// sign; on an unswept wing it is near zero. The term is worth 0.6-0.9 %MAC
/// on the transports' limits.
///
/// `None` with no wing or a non-finite term. Thin-airfoil theory and the
/// 2D flap shift carry no Fowler chord extension: a first-order estimate.
pub(super) fn takeoff_pitching_moment(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
) -> Option<LandingPitchingMoment> {
    flapped_pitching_moment(plane, config, s_ref, TAKEOFF_FLAP_ZERO_LIFT_SHIFT_DEG)
}

/// The wing/fuselage lift coefficient at zero fuselage angle of attack with
/// the takeoff flap, out of ground effect: [`takeoff_ground_lift`] without the
/// ground's gain, for the climb-trim balance. `None` without finite terms.
pub(super) fn takeoff_free_air_lift(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
) -> Option<f64> {
    let wing = takeoff_pitching_moment(plane, config, s_ref)?;
    super::wing_body::wing_body_terms(plane, config)
        .map(|terms| terms.zero_alpha_lift(&wing))
        .filter(|lift| lift.is_finite() && *lift > 0.0)
}

/// The take-off flap's lift increment at zero angle of attack on the
/// reference area, by the landing flap's method (Torenbeek, *Synthesis of
/// Subsonic Airplane Design*, 1982, App. G, eqs. G-3/G-4, G-8, G-9 and G-21,
/// [`super::landing_flap`]) at the take-off setting: the declared take-off
/// deflection (`performance.takeoff_flap_deflection_deg`), else
/// [`alas_config::presets::DEFAULT_TAKEOFF_FLAP_DEG`] (the deflection the
/// take-off flap drag is evaluated at), with the chord extension of that
/// deflection on the same Jenkinson Fig. 8.11 curve as the landing one
/// ([`alas_config::presets::flap_extension_at`]). `None` with no wing or a
/// non-finite term.
pub(super) fn takeoff_flap_zero_alpha_lift(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
) -> Option<f64> {
    let clean = flapped_pitching_moment(plane, config, s_ref, 0.0)?;
    let main = plane.wings.first()?;
    let landing = &config.performance.high_lift_chord_extension;
    let deflection = config
        .performance
        .takeoff_flap_deflection_deg
        .unwrap_or(alas_config::presets::DEFAULT_TAKEOFF_FLAP_DEG);
    let takeoff = alas_config::HighLiftChordExtension {
        flap: alas_config::presets::flap_extension_at(landing, deflection),
        flap_landing_deg: deflection,
        ..*landing
    };
    let inputs = super::landing_flap::WingInputs {
        aspect_ratio: plane.b_ref * plane.b_ref / s_ref,
        taper_ratio: main.reference_planform().taper_ratio(),
        lift_curve_slope: clean.lift_curve_slope_per_rad,
        section_zero_alpha_lift: section_zero_alpha_lift(main)?,
    };
    super::landing_flap::flap_zero_alpha_lift(&inputs, &takeoff, config)
}

/// The wing pitching moment with the configured landing flap
/// (`config.performance.high_lift_chord_extension`), about the wing-body
/// aerodynamic centre ([`wing_body_ac_frac`] on the wing's own MAC):
///
/// `C_M,ac = C_M,ac,clean + (-0.3) Delta_f C_L0`
///
/// ([`LANDING_CM_AC_PER_ZERO_ALPHA_LIFT`], the take-off flap's estimate with the
/// flap lift of [`super::landing_flap`], eq. G-21). The clean moment is
/// [`takeoff_pitching_moment`]'s section term; `None` with no wing or a
/// non-finite term.
///
/// A section-by-section moment (Torenbeek 1982, App. G, eqs. G-35 to G-39, and
/// DATCOM's slat term) stood here and is retired: it came out at 1.7 to 2 times
/// the lumped estimate on every type and the F100 tail-off data, with no
/// definition difference found (moment centre, `Delta C_L0` at constant angle of
/// attack, flap chord and span inputs), so the lumped estimate serves every type.
pub(super) fn landing_pitching_moment(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
) -> Option<LandingPitchingMoment> {
    let clean = flapped_pitching_moment(plane, config, s_ref, 0.0)?;
    let main = plane.wings.first()?;
    let inputs = super::landing_flap::WingInputs {
        aspect_ratio: plane.b_ref * plane.b_ref / s_ref,
        taper_ratio: main.reference_planform().taper_ratio(),
        lift_curve_slope: clean.lift_curve_slope_per_rad,
        section_zero_alpha_lift: section_zero_alpha_lift(main)?,
    };
    let flap_zero_alpha_lift = super::landing_flap::flap_zero_alpha_lift(
        &inputs,
        &config.performance.high_lift_chord_extension,
        config,
    )?;
    let cm_ac = clean.clean_cm_ac + LANDING_CM_AC_PER_ZERO_ALPHA_LIFT * flap_zero_alpha_lift;
    cm_ac.is_finite().then_some(LandingPitchingMoment {
        flap_zero_alpha_lift,
        cm_ac,
        ..clean
    })
}

/// Thin-airfoil section lift at zero angle of attack of the wing's camber,
/// `2 pi (-alpha_0l)` chord-weighted over its sections; `None` without a
/// finite zero-lift angle.
fn section_zero_alpha_lift(wing: &alas_geom::aircraft::wing::Wing) -> Option<f64> {
    let (mut weighted, mut chords) = (0.0, 0.0);
    for section in &wing.xsecs {
        let zero_lift =
            alas_aero::fourier_lifting_line::thin_airfoil_zero_lift_angle_rad(&section.airfoil)
                .ok()?;
        weighted += section.chord * -zero_lift;
        chords += section.chord;
    }
    let lift = 2.0 * std::f64::consts::PI * weighted / chords;
    lift.is_finite().then_some(lift)
}

/// The wing-body moment with a flap whose two-dimensional zero-lift angle
/// change is `flap_shift_deg`, see [`landing_pitching_moment`].
fn flapped_pitching_moment(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
    flap_shift_deg: f64,
) -> Option<LandingPitchingMoment> {
    let wing = plane.wings.first()?;
    if !(s_ref.is_finite() && s_ref > 0.0) {
        return None;
    }
    let reference = wing.reference_planform();
    let aspect_ratio = plane.b_ref * plane.b_ref / s_ref;
    let slope = alas_stab::static_stability::datcom(
        aspect_ratio,
        wing.mean_sweep_angle(0.25).to_radians(),
        reference.taper_ratio(),
        0.2,
    );
    // Simpson's rule on each segment: chord linear in span, the section
    // moment linear between its two sections.
    let mut chord_squared = 0.0;
    let mut moment = 0.0;
    let (mut area, mut incidence_area) = (0.0, 0.0);
    for pair in wing.xsecs.windows(2) {
        let span = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
        let section_moment = |section: &alas_geom::aircraft::wing::WingXSec| {
            alas_aero::fourier_lifting_line::thin_airfoil_quarter_chord_moment(&section.airfoil)
                .ok()
        };
        let (c0, c1) = (pair[0].chord, pair[1].chord);
        let (m0, m1) = (section_moment(&pair[0])?, section_moment(&pair[1])?);
        let (cm, mm) = (0.5 * (c0 + c1), 0.5 * (m0 + m1));
        chord_squared += span / 6.0 * (c0 * c0 + 4.0 * cm * cm + c1 * c1);
        moment += span / 6.0 * (c0 * c0 * m0 + 4.0 * cm * cm * mm + c1 * c1 * m1);
        let incidence = |section: &alas_geom::aircraft::wing::WingXSec| {
            alas_aero::fourier_lifting_line::thin_airfoil_zero_lift_angle_rad(&section.airfoil)
                .ok()
                .map(|zero_lift| section.twist.to_radians() - zero_lift)
        };
        let (i0, i1) = (incidence(&pair[0])?, incidence(&pair[1])?);
        area += span / 6.0 * (c0 + 4.0 * cm + c1);
        incidence_area += span / 6.0 * (c0 * i0 + 4.0 * cm * 0.5 * (i0 + i1) + c1 * i1);
    }
    if chord_squared <= 0.0 || area <= 0.0 {
        return None;
    }
    let clean_zero_alpha_lift = slope * incidence_area / area;
    let root = wing.xsecs.first()?;
    let root_zero_alpha_lift = slope
        * (root.twist.to_radians()
            - alas_aero::fourier_lifting_line::thin_airfoil_zero_lift_angle_rad(&root.airfoil)
                .ok()?);
    let controls = &config.control_surfaces;
    let flapped_fraction = super::tail_stall::strip_area_fraction(
        reference.taper_ratio(),
        controls.flap_span_start_frac,
        controls.flap_span_end_frac,
    );
    let hinge_cos = wing
        .mean_sweep_angle(1.0 - controls.flap_chord_fraction)
        .to_radians()
        .cos()
        .abs();
    let sweep_cos = wing.mean_sweep_angle(0.25).to_radians().cos();
    let planform_factor = aspect_ratio * sweep_cos * sweep_cos / (aspect_ratio + 2.0 * sweep_cos);
    let clean_cm_ac = planform_factor * moment / chord_squared;
    let flap_zero_alpha_lift =
        slope * (-flap_shift_deg).to_radians() * flapped_fraction * hinge_cos;
    let cm_ac = clean_cm_ac + LANDING_CM_AC_PER_ZERO_ALPHA_LIFT * flap_zero_alpha_lift;
    [
        slope,
        clean_cm_ac,
        flap_zero_alpha_lift,
        clean_zero_alpha_lift,
        root_zero_alpha_lift,
        cm_ac,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(LandingPitchingMoment {
        lift_curve_slope_per_rad: slope,
        clean_cm_ac,
        flap_zero_alpha_lift,
        clean_zero_alpha_lift,
        root_zero_alpha_lift,
        cm_ac,
    })
}

/// The landing wing-body `C_M,ac` about [`wing_body_ac_frac`]
/// ([`wing_body_cm_ac`] with the landing flap), or
/// [`SCISSOR_CM_AC_WB_LANDING`] when the wing gives no finite estimate.
pub(super) fn landing_cm_ac_wb(plane: &Airplane, config: &AlasConfig, s_ref: f64) -> f64 {
    wing_body_cm_ac(plane, config, landing_pitching_moment(plane, config, s_ref))
        .unwrap_or(SCISSOR_CM_AC_WB_LANDING)
}

/// The takeoff wing-body `C_M,ac` about [`wing_body_ac_frac`]
/// ([`wing_body_cm_ac`] with the takeoff flap), or
/// [`ROTATION_CM_AC_WB_TAKEOFF`] when the wing gives no finite estimate.
pub(super) fn takeoff_cm_ac_wb(plane: &Airplane, config: &AlasConfig, s_ref: f64) -> f64 {
    wing_body_cm_ac(plane, config, takeoff_pitching_moment(plane, config, s_ref))
        .unwrap_or(ROTATION_CM_AC_WB_TAKEOFF)
}

/// The clean wing-body `C_M,ac` about [`wing_body_ac_frac`] and its lift at
/// zero fuselage angle of attack ([`wing_body_cm_ac`] and
/// `super::wing_body::WingBodyTerms::clean_zero_alpha_lift` with no flap),
/// for the clean climb trim (`super::clean_trim`). `None` without finite
/// terms.
pub(super) fn clean_wing_body_terms(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
) -> Option<(f64, f64)> {
    let wing = flapped_pitching_moment(plane, config, s_ref, 0.0)?;
    let cm_ac = wing_body_cm_ac(plane, config, Some(wing))?;
    let lift = super::wing_body::wing_body_terms(plane, config)
        .map(|terms| terms.clean_zero_alpha_lift(&wing))?;
    (cm_ac.is_finite() && lift.is_finite()).then_some((cm_ac, lift))
}
/// Tail dynamic-pressure ratio `q_h / q` of a stabilizer mounted on the
/// fuselage, below the fin: Torenbeek, *Advanced Aircraft Design*, Wiley,
/// 2013, sec. 6.3.3, p. 168, typical input of the forward-limit trim balance
/// (eq. 6.2), paired with `C_L,h = -0.80` ([`SCISSOR_CL_H_MAX`]).
pub(super) const LOW_TAIL_EFFICIENCY: f64 = 0.85;
/// The same for a T-tail, its stabilizer on the fin: Torenbeek 2013,
/// sec. 6.3.3, p. 168.
pub(super) const T_TAIL_EFFICIENCY: f64 = 0.95;

/// Tail dynamic-pressure ratio `q_h / q` of the rotation, takeoff-trim and
/// landing-trim balances, dimensionless: Torenbeek's paired input by tail
/// type ([`T_TAIL_EFFICIENCY`] when the stabilizer shares a lattice
/// component with the fin, [`alas_aero::vlm::lattice_components`], else
/// [`LOW_TAIL_EFFICIENCY`]). The lattice keeps its own
/// `config.analysis.tail_efficiency` for the cruise tail.
pub(super) fn tail_efficiency(plane: &Airplane) -> f64 {
    if is_t_tail(plane) {
        T_TAIL_EFFICIENCY
    } else {
        LOW_TAIL_EFFICIENCY
    }
}

/// Whether the horizontal stabilizer sits on the fin: both in one lattice
/// component.
pub(super) fn is_t_tail(plane: &Airplane) -> bool {
    let component = alas_aero::vlm::lattice_components(&plane.wings);
    let index = |name: &str| plane.wings.iter().position(|wing| wing.name == name);
    match (
        index(alas_geom::aircraft::airplane::HORIZONTAL_STABILIZER_NAME),
        index(FIN_NAME),
    ) {
        (Some(stabilizer), Some(fin)) => component[stabilizer] == component[fin],
        _ => false,
    }
}

/// Name the builder gives the fin.
const FIN_NAME: &str = "Vertical Stabilizer";

/// Fallback of the landing-trim tail lift (the achievable download of
/// `super::landing_download` is used when it resolves). The design value:
/// tail lift coefficient of the landing-trim forward-limit balance at
/// `CL_max,land`: Torenbeek, *Advanced Aircraft Design*, Wiley, 2013,
/// sec. 6.3.3, pp. 167-168, eq. 6.2, the design value paired with
/// [`LOW_TAIL_EFFICIENCY`] and [`T_TAIL_EFFICIENCY`]. It is a design value,
/// not the tail's stall download: it keeps elevator margin for recovery.
/// Obert, *Aerodynamic Design of Transport Aircraft*, 2009, Fig. 31.11,
/// p. 400, takes the same `-0.8` at landing forward CG, and p. 406 keeps a
/// margin to tail stall with leading-edge ice through a speed increase to
/// the placard and a 0.5 g push-over. The rotation's transient
/// maximum-elevator capability is `super::rotation::tail_download_cap`.
pub(super) const SCISSOR_CL_H_MAX: f64 = -0.8;

/// Fallback wing-body pitching moment about its own AC in the takeoff
/// flap/slat configuration, dimensionless, for a wing without a finite
/// [`takeoff_pitching_moment`]: an order-of-magnitude estimate, smaller in
/// magnitude than [`SCISSOR_CM_AC_WB_LANDING`], not a measured value.
pub(super) const ROTATION_CM_AC_WB_TAKEOFF: f64 = -0.15;

/// This design's horizontal-tail reference area over the wing reference
/// area (`S_h / S`), for the rotation and landing-trim criteria's tail-force
/// (rather than tail-volume) bookkeeping: `0.0` with no second (tail) wing.
pub(super) fn tail_area_ratio(plane: &Airplane, s_ref: f64) -> f64 {
    let Some(h_stab) = plane.wings.get(1) else {
        return 0.0;
    };
    if s_ref <= 0.0 {
        0.0
    } else {
        h_stab.reference_area() / s_ref
    }
}

/// The horizontal tail's own aerodynamic-centre station, primary body
/// frame, m aft of the nose tip; `f64::NAN` with no second (tail) wing so a
/// caller that forgets to check can only ever disable the mechanism that
/// depends on it (matching the tip-back boundary's non-finite convention),
/// never silently substitute zero.
pub(super) fn horizontal_tail_ac_x_m(plane: &Airplane) -> f64 {
    plane.wings.get(1).map_or(f64::NAN, |h_stab| {
        h_stab.reference_planform().aerodynamic_center(0.25)[0]
    })
}

/// This design's mass-weighted vertical CG for the `OEW_KEYS` component
/// set, metres, mirroring `alas_payload::oew::oew_and_cg`'s `x` aggregation
/// on the `z` axis (needed for the tip-back boundary's `h_cg`, which
/// `oew_and_cg` does not itself carry).
pub(super) fn oew_cg_z(masses: &MassBreakdown, coords: &MassCoordinates) -> f64 {
    let mut total = 0.0;
    let mut moment = 0.0;
    for key in alas_mass::breakdown::OEW_KEYS {
        let mass = masses.get(key).unwrap_or(0.0).max(0.0);
        if mass <= 0.0 {
            continue;
        }
        if let Some((_, xyz)) = coords.as_pairs().into_iter().find(|&(name, _)| name == key) {
            total += mass;
            moment += mass * xyz[2];
        }
    }
    if total > 0.0 {
        moment / total
    } else {
        0.0
    }
}

/// The shared ground plane's `z`, metres: the mass model's own
/// (`alas_mass::stations::ground_plane_z_m`), so CG heights and gear
/// stations stand on one plane.
pub(super) fn ground_z_m(fuselage: &Fuselage, config: &AlasConfig) -> f64 {
    alas_mass::stations::ground_plane_z_m(fuselage, &config.geometry, &config.landing_gear)
}

/// The ground plane's `z` at one end of the documented clearance sample of the
/// aircraft (a declared clearance is the same plane at every end), metres.
pub(super) fn ground_z_on(
    basis: alas_mass::stations::GroundPlaneBasis,
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
) -> Option<f64> {
    let (fuselage, wing) = (plane.fuselages.first()?, plane.wings.first()?);
    let sample = alas_mass::stations::clearance_sample(
        wing,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    Some(alas_mass::stations::ground_plane_z_for(
        basis,
        sample,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    ))
}

/// Lower-contour samples of the airframe aft of `x_main_gear_m`, for the
/// tail-scrape (tail-down) angle: the keel `z_c - height/2` at each fuselage
/// cross-section aft of the main gear, and both edges of every built
/// ventral-fin section there. A ventral fin hangs below the tail cone, so the
/// fins touch the runway before the keel does: on the Learjet 60 the delta-fin
/// tips stand at 11.5 deg from the static main wheels against 15.4 deg for the
/// keel alone (FlightSafety Learjet 60 Pilot Training Manual, fig 1-2).
pub(crate) fn lower_contour_aft_of(
    plane: &Airplane,
    fuselage: &Fuselage,
    x_main_gear_m: f64,
) -> Vec<alas_perf::landing_gear::geometry::FuselageLowerPoint> {
    use alas_perf::landing_gear::geometry::FuselageLowerPoint;
    let keel = fuselage.xsecs.iter().map(|xsec| FuselageLowerPoint {
        x_m: xsec.xyz_c[0],
        z_bottom_m: xsec.xyz_c[2] - xsec.height / 2.0,
    });
    let fins = plane
        .wings
        .iter()
        .filter(|wing| wing.is_ventral_fin())
        .flat_map(|wing| wing.xsecs.iter())
        .flat_map(|section| {
            let twist = section.twist.to_radians();
            [
                FuselageLowerPoint {
                    x_m: section.xyz_le[0],
                    z_bottom_m: section.xyz_le[2],
                },
                FuselageLowerPoint {
                    x_m: section.xyz_le[0] + section.chord * twist.cos(),
                    z_bottom_m: section.xyz_le[2] - section.chord * twist.sin(),
                },
            ]
        });
    keel.chain(fins)
        .filter(|point| point.x_m > x_main_gear_m)
        .collect()
}

/// This design's horizontal-tail volume coefficient `S_h * l_h / (S * c)`
/// for the scissor-plot estimate, `0.0` with no second (tail) wing.
pub(super) fn tail_volume_coefficient(
    plane: &Airplane,
    s_ref: f64,
    mac: f64,
    x_ac_wb_m: f64,
) -> f64 {
    let Some(h_stab) = plane.wings.get(1) else {
        return 0.0;
    };
    // The arm of the moment balance is measured from the wing-body aerodynamic
    // centre the balance takes its moments about (Torenbeek 2013 eq. 6.2:
    // `C_L (x - x_ac) + C_m,ac = eta V_H C_L,h`), not from the wing quarter
    // chord: a different station by the fuselage, nacelle and lift-loss shifts.
    let l_h = (h_stab.reference_planform().aerodynamic_center(0.25)[0] - x_ac_wb_m).abs();
    let s_h = h_stab.reference_area();
    if s_ref <= 0.0 || mac <= 0.0 {
        0.0
    } else {
        s_h * l_h / (s_ref * mac)
    }
}

/// The most forward candidate that is not NaN, and its governance; all candidates
/// NaN falls back on the first one's governance. The aft candidates are closed-form
/// (no solve) and `pct_mac` maps a non-finite station to NaN, so none comes out
/// infinite today; were one to, `-inf` governs (fails closed) as in [`most_aft`].
pub(super) fn most_forward<G: Copy>(candidates: [(f64, G); 3]) -> (f64, G) {
    candidates
        .into_iter()
        .filter(|(value, _)| !value.is_nan())
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .unwrap_or((f64::NAN, candidates[0].1))
}

/// The most aft candidate that is not NaN, and its governance. An infinite
/// candidate is kept: `+inf` is the rotation boundary that could not be resolved,
/// which must govern (and so violate the forward-limit row), not be dropped.
pub(super) fn most_aft<G: Copy, const N: usize>(candidates: [(f64, G); N]) -> (f64, G) {
    candidates
        .into_iter()
        .filter(|(value, _)| !value.is_nan())
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .unwrap_or((f64::NAN, candidates[0].1))
}

/// The ceiling on the nose-gear share of weight at `mass_kg` that bounds the
/// forward ground limit: the class handling share
/// (`mass_model.pct_load_nlg_max_handling`), or the published maximum static
/// nose-gear load over the weight where that is lower
/// (`landing_gear.nose_gear_max_static_load_kg`).
pub(super) fn nose_handling_fraction(config: &AlasConfig, mass_kg: f64) -> f64 {
    let class = config.mass_model.pct_load_nlg_max_handling;
    config
        .landing_gear
        .nose_gear_max_static_load_kg
        .filter(|load| load.is_finite() && *load > 0.0 && mass_kg > 0.0)
        .map_or(class, |load| class.min(load / mass_kg))
}

// A test asserts on values it built here, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// The aft selection drops NaN only: an infinite aft boundary would govern.
    #[test]
    fn the_most_forward_selection_drops_nan_only() {
        let (value, tag) = most_forward([(f64::NAN, 0), (f64::NEG_INFINITY, 1), (30.0, 2)]);
        assert_eq!((value, tag), (f64::NEG_INFINITY, 1));
        let (value, tag) = most_forward([(f64::NAN, 0), (f64::NAN, 1), (f64::NAN, 2)]);
        assert!(value.is_nan());
        assert_eq!(tag, 0);
    }

    /// Torenbeek's paired tail efficiency by tail type: the stabilizers on
    /// the fin (ATR72-600, A400M, LJ60) read 0.95, every fuselage-mounted one
    /// 0.85.
    #[allow(clippy::expect_used)]
    #[test]
    fn the_tail_efficiency_is_torenbeeks_value_for_the_tail_type() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let t_tail = ["ATR72-600", "A400M", "LJ60"].contains(&name);
            assert_eq!(is_t_tail(&plane), t_tail, "{name}");
            let expected = if t_tail { 0.95 } else { 0.85 };
            assert_eq!(tail_efficiency(&plane), expected, "{name}");
        }
    }

    /// Every registered preset: the landing flap lift is Torenbeek's eq. G-21
    /// ([`super::super::landing_flap`]) and the take-off flap's Raymer's
    /// eq. 12.22 shift on the DATCOM slope; the moment of each is the clean
    /// section moment plus Torenbeek's -0.3 of the flap lift and nose-down, a
    /// rigid rotation of the wing leaves it unchanged, and the probe prints
    /// the terms.
    // A probe prints its numbers.
    #[allow(clippy::print_stdout, clippy::expect_used)]
    #[test]
    fn the_landing_moment_is_the_section_moment_and_torenbeeks_flap_fraction() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let wing = &plane.wings[0];
            let s_ref = wing.reference_area();
            let moment = landing_pitching_moment(&plane, &config, s_ref).expect("finite terms");
            let controls = &config.control_surfaces;
            let fraction = super::super::tail_stall::strip_area_fraction(
                wing.reference_planform().taper_ratio(),
                controls.flap_span_start_frac,
                controls.flap_span_end_frac,
            );
            let hinge = wing
                .mean_sweep_angle(1.0 - controls.flap_chord_fraction)
                .to_radians()
                .cos();
            let takeoff_flap =
                moment.lift_curve_slope_per_rad * 10.0_f64.to_radians() * fraction * hinge;
            let expected = moment.clean_cm_ac - 0.3 * moment.flap_zero_alpha_lift;
            assert!((moment.cm_ac - expected).abs() < 1.0e-12, "{name}");
            assert!(moment.clean_cm_ac <= 0.0, "{name}: {moment:?}");
            assert!(moment.cm_ac < 0.0, "{name}: {moment:?}");
            // The fuselage (Torenbeek App. E): the AC of eq. E-36 and the
            // moment of eq. E-40 with the clean lift at zero fuselage angle.
            let terms = super::super::wing_body::wing_body_terms(&plane, &config)
                .expect("finite wing/fuselage terms");
            // The centre is the lattice wing AC and the shifts in metres, in
            // the reference frame last; Cm on the wing's own MAC, carried onto
            // the reference chord.
            let frame = plane.mac_frame().expect("a frame");
            let x_ac_w = super::super::wing_body::lattice_wing_ac_m(&plane, &config)
                .expect("a lattice wing AC");
            let x_ac_wb = (terms.ac_x_m(x_ac_w) - frame.x_lemac_m) / frame.chord_m;
            assert!(
                (wing_body_ac_frac(&plane, &config) - x_ac_wb).abs() < 1.0e-12,
                "{name}"
            );
            let to_ref = terms.mac_m / frame.chord_m;
            let area = plane.wings[0].projected_area() / plane.s_ref;
            let zero_alpha_lift = terms.zero_alpha_lift(&moment);
            let clean_zero_alpha_lift = terms.k_ii * moment.root_zero_alpha_lift
                + terms.k_i * (moment.clean_zero_alpha_lift - moment.root_zero_alpha_lift)
                + terms.vertical_position_lift;
            assert_eq!(clean_zero_alpha_lift, terms.clean_zero_alpha_lift(&moment));
            let landing = landing_cm_ac_wb(&plane, &config, s_ref);
            assert!(
                (landing
                    - to_ref
                        * (moment.clean_cm_ac * area
                            + (moment.cm_ac - moment.clean_cm_ac)
                            + terms.fuselage_cm_ac_per_zero_alpha_lift * clean_zero_alpha_lift))
                    .abs()
                    < 1.0e-12,
                "{name}"
            );
            // The takeoff setting: the same section moment, Raymer's -10 deg
            // flap shift, a smaller flap lift than the landing flap's.
            let takeoff = takeoff_pitching_moment(&plane, &config, s_ref).expect("finite terms");
            assert_eq!(takeoff.clean_cm_ac, moment.clean_cm_ac, "{name}");
            assert!(
                (takeoff.flap_zero_alpha_lift - takeoff_flap).abs() < 1.0e-12,
                "{name}"
            );
            assert!(
                moment.flap_zero_alpha_lift > takeoff.flap_zero_alpha_lift,
                "{name}: {moment:?} {takeoff:?}"
            );
            assert!(
                moment.cm_ac < takeoff.cm_ac && takeoff.cm_ac < 0.0,
                "{name}: {takeoff:?}"
            );
            // The ground-attitude lift is the takeoff lift at zero fuselage
            // angle, the same `C_L0` the takeoff fuselage moment uses.
            // In ground effect (eq. G-76): a gain at these heights.
            let ground = takeoff_ground_lift(&plane, &config, s_ref);
            let free_air = clean_zero_alpha_lift + takeoff.flap_zero_alpha_lift;
            assert!(
                ground > free_air && ground < 1.3 * free_air,
                "{name}: {ground} {free_air}"
            );
            assert!(
                (takeoff_cm_ac_wb(&plane, &config, s_ref)
                    - to_ref
                        * (takeoff.clean_cm_ac * area
                            + (takeoff.cm_ac - takeoff.clean_cm_ac)
                            + terms.fuselage_cm_ac_per_zero_alpha_lift * clean_zero_alpha_lift))
                    .abs()
                    < 1.0e-12,
                "{name}"
            );
            let mut rotated = plane.clone();
            for section in &mut rotated.wings[0].xsecs {
                section.twist += 2.0;
            }
            let turned = landing_pitching_moment(&rotated, &config, s_ref).expect("finite terms");
            // The planform projection of the sweep and flap terms moves at second
            // order (the old -0.3 C_L(alpha=0) form moved 0.047 here).
            assert!(
                (turned.clean_cm_ac - moment.clean_cm_ac).abs() < 1.0e-4,
                "{name}"
            );
            assert!(
                (turned.cm_ac - moment.cm_ac).abs() < 1.0e-4,
                "{name}: {turned:?} vs {moment:?}"
            );
            println!(
                "{name}: a_w {:.3} Cm_ac clean {:.4} CL0 flap {:.3} Cm_ac {:.3} CL0 clean {:.3} \
                 x_ac,wf {:.4} CL0 land {:.3} Cm_ac,wf land {:.3} CL_g {:.3} (free air {:.3}) \
                 Cm_ac,wf to {:.3}",
                moment.lift_curve_slope_per_rad,
                moment.clean_cm_ac,
                moment.flap_zero_alpha_lift,
                moment.cm_ac,
                moment.clean_zero_alpha_lift,
                x_ac_wb,
                zero_alpha_lift,
                landing,
                ground,
                free_air,
                takeoff_cm_ac_wb(&plane, &config, s_ref)
            );
        }
    }
}
