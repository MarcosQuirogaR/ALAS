// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff-rotation (nose-wheel lift-off) forward centre-of-gravity
//! boundary: its moment balance, its pitch inertia and the horizontal-tail
//! lift it can command.
//!
//! Frame and units: SI; x metres aft of the nose tip; heights metres above
//! the shared ground plane; moments about the main-gear ground contact `P`,
//! nose-up positive. The aircraft stands level on the ground plane, so the
//! fuselage reference line is at zero angle of attack in the ground roll.
//!
//! Source: M. H. Sadraey, *Aircraft Design: A Systems Engineering
//! Approach*, Wiley, 2012. Section 9.6.2 (eqs. 9.36-9.54a) states the
//! balance at the instant the nose wheel leaves the runway,
//!
//! ```text
//! I_P th'' = L_wf (x_P - x_ac) + M_ac,wf + L_h (x_P - x_h) - W (x_P - x_cg)
//!            - T h_T + D h_D + m a h_cg,      m a = T - D - mu (W - L),
//! I_P      = I_yy,cg + m [(x_P - x_cg)^2 + h_cg^2]            (eq. 9.53)
//! ```
//!
//! and section 12.6 (eqs. 12.55-12.76) gives the tail lift at rotation from
//! the tail geometry, `CL_h = a_h (alpha_h + tau_e delta_e)`,
//! `alpha_h = alpha + i_h - epsilon`.
//!
//! Aerodynamic drag is omitted: no takeoff-roll drag polar exists in the
//! configuration. Its net moment `D (h_D - h_cg)` is bounded by about 1 %MAC
//! (`D/W` near 0.04 at `V_R`, `|h_D - h_cg|` below about 1 m on the
//! single-aisle and turboprop presets, less in MAC on the widebodies), nose-up
//! (conservative to omit) when the drag line is above the CG and nose-down
//! (non-conservative) when it is below.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::inertia::RadiiOfGyration;

use super::tail_stall::{
    plain_flap_delta_cl_max, strip_area_fraction, symmetric_thickness,
    tail_section_chart_excursion, tail_section_cl_max, SEA_LEVEL_KINEMATIC_VISCOSITY_M2_S,
    SEA_LEVEL_SPEED_OF_SOUND_M_S,
};

/// Required pitch acceleration at rotation, deg/s^2, for a transport: the
/// midpoint of Sadraey's transport range. Sadraey, *Aircraft Design: A
/// Systems Engineering Approach*, Wiley, 2012, ch. 12, sec. 12.3 (elevator
/// design, takeoff rotation requirement): "in a transport aircraft, the
/// acceptable value for the take-off rotation time is 3-5 seconds. The
/// equivalent value for the angular rotation rate to achieve such
/// requirement is 4-6 deg/sec2". A class requirement [E], not an aircraft
/// value; the earlier 7 deg/s^2 (a recalled Torenbeek/Roskam 6-8 range) had
/// no retrieved source. Overridden by
/// `landing_gear.rotation_pitch_acceleration_deg_s2`.
pub(super) const ROTATION_PITCH_ACCELERATION_DEG_S2: f64 = 5.0;
/// Maximum elevator deflection at rotation, degrees, trailing edge up
/// (negative): Sadraey's typical maximum up-elevator deflection of a
/// transport, -25 deg (Sadraey 2012, sec. 12.6, Table 12.3). One class-generic
/// value for every aircraft [E]: no aircraft carries a published one.
pub(super) const MAX_ELEVATOR_UP_DEFLECTION_DEG: f64 = -25.0;

/// The full up-elevator deflection at rotation, deg, trailing edge up
/// (negative): `-landing_gear.elevator_up_travel_deg` when it is set
/// (finite and positive), else [`MAX_ELEVATOR_UP_DEFLECTION_DEG`].
#[must_use]
pub(super) fn elevator_up_deflection_deg(config: &AlasConfig) -> f64 {
    config
        .landing_gear
        .elevator_up_travel_deg
        .filter(|deg| deg.is_finite() && *deg > 0.0)
        .map_or(MAX_ELEVATOR_UP_DEFLECTION_DEG, |deg| -deg)
}

/// The required pitch acceleration at rotation, deg/s^2: the user's
/// `landing_gear.rotation_pitch_acceleration_deg_s2` when it is set (finite
/// and positive), else [`ROTATION_PITCH_ACCELERATION_DEG_S2`].
#[must_use]
pub(super) fn rotation_pitch_acceleration_deg_s2(config: &AlasConfig) -> f64 {
    config
        .landing_gear
        .rotation_pitch_acceleration_deg_s2
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(ROTATION_PITCH_ACCELERATION_DEG_S2)
}

/// Pitch radius of gyration about the centre of gravity, m.
///
/// The user's `landing_gear.pitch_radius_of_gyration_frac_mac` times the MAC
/// when it is set (finite and positive); otherwise
/// `sqrt(I_yy,cg / m)` from the mass ledger's takeoff tensor when it is
/// finite and positive; otherwise Raymer's jet-transport radius,
/// `0.38 L / 2` (Raymer, *Aircraft Design: A Conceptual Approach*,
/// table 16.1, half-length definition; [`RadiiOfGyration`]).
#[must_use]
pub(super) fn pitch_radius_of_gyration_m(
    override_frac_mac: Option<f64>,
    mac_m: f64,
    takeoff_pitch_inertia_kg_m2: f64,
    takeoff_mass_kg: f64,
    span_m: f64,
    fuselage_length_m: f64,
) -> f64 {
    if let Some(fraction) = override_frac_mac.filter(|f| f.is_finite() && *f > 0.0) {
        if mac_m.is_finite() && mac_m > 0.0 {
            return fraction * mac_m;
        }
    }
    if takeoff_pitch_inertia_kg_m2.is_finite()
        && takeoff_pitch_inertia_kg_m2 > 0.0
        && takeoff_mass_kg.is_finite()
        && takeoff_mass_kg > 0.0
    {
        (takeoff_pitch_inertia_kg_m2 / takeoff_mass_kg).sqrt()
    } else {
        RadiiOfGyration::JET_TRANSPORT.radii_m(span_m, fuselage_length_m)[1]
    }
}

/// Wieselsberger's ground-effect factor on induced flow,
/// `sigma = (16 h/b)^2 / (1 + (16 h/b)^2)`, in `[0, 1]`; `1.0` (no ground
/// effect) for a non-finite or non-positive height or span.
#[must_use]
pub(super) fn wieselsberger_factor(height_m: f64, span_m: f64) -> f64 {
    if !height_m.is_finite() || !span_m.is_finite() || height_m <= 0.0 || span_m <= 0.0 {
        return 1.0;
    }
    let ratio = 16.0 * height_m / span_m;
    (ratio * ratio / (1.0 + ratio * ratio)).clamp(0.0, 1.0)
}

/// Thin-airfoil angle-of-attack effectiveness of a plain flap of chord
/// fraction `c_f / c` (Glauert; Abbott and von Doenhoff, *Theory of Wing
/// Sections*, sec. 8.4): `tau = 1 - (theta_f - sin theta_f) / pi`,
/// `cos theta_f = 2 c_f / c - 1`. Sadraey's Fig. 12.12 plots the same
/// parameter. Thin-airfoil theory carries no viscous or large-deflection
/// loss, so it is an upper bound on the real effectiveness. `NaN` outside
/// `[0, 1]`.
#[must_use]
pub(super) fn elevator_effectiveness(chord_fraction: f64) -> f64 {
    if !(0.0..=1.0).contains(&chord_fraction) {
        return f64::NAN;
    }
    let theta = (2.0 * chord_fraction - 1.0).acos();
    1.0 - (theta - theta.sin()) / std::f64::consts::PI
}

/// Empirical large-deflection correction `K'` to the plain-flap (elevator)
/// lift effectiveness at trailing-edge deflection `|delta|` in degrees, for a
/// flap (elevator) of chord fraction `c_f / c`: USAF DATCOM Fig.
/// 6.1.1.1-40, as digitised in
/// [`alas_perf::performance::yaw_balance::large_deflection_effectiveness_factor`],
/// which the rudder of the yaw balance shares.
#[must_use]
pub(super) fn large_deflection_effectiveness_factor(
    deflection_deg: f64,
    chord_fraction: f64,
) -> f64 {
    alas_perf::performance::yaw_balance::large_deflection_effectiveness_factor(
        deflection_deg,
        chord_fraction,
    )
}

/// The horizontal-tail lift at rotation and the terms it is built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RotationTailLift {
    /// Tail lift-curve slope, per radian (DATCOM, incompressible).
    pub(super) lift_curve_slope_per_rad: f64,
    /// Tail incidence at rotation, rad: the trimmable stabiliser's takeoff
    /// setting when it gives more download than the built incidence, else
    /// the built incidence.
    pub(super) incidence_rad: f64,
    /// Chord-weighted mean incidence of the built tail, rad.
    pub(super) built_incidence_rad: f64,
    /// Downwash at the tail in ground effect, rad.
    pub(super) downwash_rad: f64,
    /// Elevator effectiveness times its span fraction.
    pub(super) elevator_effectiveness: f64,
    /// Elevator term of the tail angle of attack at full up-elevator,
    /// `tau_e delta_e`, rad (negative).
    pub(super) elevator_term_rad: f64,
    /// Total tail lift coefficient at rotation, on the tail's own area;
    /// negative (download), limited in magnitude to
    /// [`Self::section_cap`].
    pub(super) lift_coefficient: f64,
    /// The linear (uncapped) tail lift coefficient, negative.
    pub(super) linear_lift_coefficient: f64,
    /// Largest download coefficient the tail supports, `0.9 [c_l,max
    /// cos(Lambda_c/4) + Delta c_l,max (S_e / S_h) cos(Lambda_HL)]`, positive.
    pub(super) section_cap: f64,
    /// The clean-section part of [`Self::section_cap`] (before the elevator
    /// increment), positive.
    pub(super) clean_section_cap: f64,
}

/// The horizontal tail's lift coefficient at rotation with full up-elevator,
/// derived from the built geometry (Sadraey 2012, sec. 12.6):
///
/// `CL_h = a_h [alpha + i_h - epsilon + tau_e (b_e / b_h) delta_e,max]`
///
/// - `alpha = 0`: the fuselage stands level in the ground roll;
/// - `i_h`: the tail incidence at rotation. A fixed stabiliser holds its
///   built chord-weighted mean incidence (its section twist). A trimmable
///   stabiliser is set nose-up for the takeoff CG before the roll, so it
///   holds `-landing_gear.takeoff_stabilizer_nose_up_deg` whenever that is
///   the more nose-up (more download) of the two;
/// - `epsilon = sigma 2 CL_g / (pi A)`: the lifting-line downwash of the wing
///   at its ground-roll lift coefficient `CL_g` (the same `2 CL / (pi A)`
///   gradient model `alas_stab` uses), reduced by Wieselsberger's
///   ground-effect factor at the wing's own height and span (the induced
///   angle in ground effect scales with the induced drag; an approximation
///   [E] that ignores the tail's own image);
/// - `a_h`: DATCOM lift-curve slope of the tail at zero Mach, about 2 %
///   below its value at the rotation Mach (conservative);
/// - `tau_e`: [`elevator_effectiveness`] of the configured elevator chord
///   fraction, times [`large_deflection_effectiveness_factor`] at
///   `delta_e,max` (USAF DATCOM section 6.1.1.1 / Raymer plain-flap
///   large-deflection correction), times the elevator's span fraction of
///   the tail (a strip approximation [E]);
/// - `delta_e,max`: [`MAX_ELEVATOR_UP_DEFLECTION_DEG`], a documented
///   class-generic assumption.
///
/// The linear result is then limited to the tail's stall: the download
/// cannot exceed `CL_h,max = 0.9 [c_l,max cos(Lambda_c/4) + Delta c_l,max
/// (S_e / S_h) cos(Lambda_HL)]` (Raymer, *Aircraft Design: A Conceptual
/// Approach*, eq. 12.21, the finite-wing maximum lift from the section
/// maximum and the flap increment). `c_l,max` and `Delta c_l,max` (the
/// elevator's own plain-flap increment) are sourced in [`super::tail_stall`]
/// at the rotation Mach and tail Reynolds numbers of `rotation_speed_m_s`
/// (ISA sea level). `None` with no second (tail) wing.
#[must_use]
pub(super) fn rotation_tail_lift(
    plane: &Airplane,
    config: &AlasConfig,
    ground_z_m: f64,
    cl_ground_attitude: f64,
    rotation_speed_m_s: f64,
) -> Option<RotationTailLift> {
    let wing = plane.wings.first()?;
    let tail = plane.wings.get(1)?;
    let chord_sum: f64 = tail.xsecs.iter().map(|section| section.chord).sum();
    if tail.xsecs.is_empty() || chord_sum <= 0.0 {
        return None;
    }
    let incidence_deg = tail
        .xsecs
        .iter()
        .map(|section| section.chord * section.twist)
        .sum::<f64>()
        / chord_sum;
    // The handbook slope and the Reynolds length are stated on the theoretical planform.
    let reference = tail.reference_planform();
    let lift_curve_slope_per_rad = alas_stab::static_stability::datcom(
        reference.aspect_ratio(),
        tail.mean_sweep_angle(0.25).to_radians(),
        tail.taper_ratio(),
        0.0,
    );
    let wing_aspect_ratio = plane.b_ref * plane.b_ref / plane.s_ref;
    let ground_effect = wieselsberger_factor(
        wing.carried_box().aerodynamic_center(0.25)[2] - ground_z_m,
        plane.b_ref,
    );
    let downwash_rad =
        ground_effect * 2.0 * cl_ground_attitude / (std::f64::consts::PI * wing_aspect_ratio);
    let controls = &config.control_surfaces;
    let span_fraction =
        (controls.elevator_span_end_frac - controls.elevator_span_start_frac).clamp(0.0, 1.0);
    let elevator_deg = elevator_up_deflection_deg(config);
    let effectiveness = elevator_effectiveness(controls.elevator_chord_fraction)
        * large_deflection_effectiveness_factor(elevator_deg, controls.elevator_chord_fraction)
        * span_fraction;
    let built_incidence_rad = incidence_deg.to_radians();
    let incidence_rad = takeoff_incidence_rad(
        built_incidence_rad,
        config.landing_gear.takeoff_stabilizer_nose_up_deg,
    );
    let linear_lift_coefficient = tail_lift_coefficient(
        lift_curve_slope_per_rad,
        incidence_rad,
        downwash_rad,
        effectiveness,
        elevator_deg.to_radians(),
    );
    let (clean_section_cap, section_cap) = tail_download_cap(tail, config, rotation_speed_m_s);
    let lift_coefficient = linear_lift_coefficient.max(-section_cap);
    Some(RotationTailLift {
        lift_curve_slope_per_rad,
        incidence_rad,
        built_incidence_rad,
        downwash_rad,
        elevator_effectiveness: effectiveness,
        elevator_term_rad: effectiveness * elevator_deg.to_radians(),
        lift_coefficient,
        linear_lift_coefficient,
        section_cap,
        clean_section_cap,
    })
}

/// The horizontal tail's maximum download coefficient (a magnitude) with
/// full up-elevator at the sea-level speed `speed_m_s`, and the same without
/// the elevator's increment: `CL_h,max = 0.9 [c_l,max cos(Lambda_c/4) +
/// Delta c_l,max (S_e / S_h) cos(Lambda_HL)]` (Raymer, *Aircraft Design: A
/// Conceptual Approach*, eq. 12.21), the section maximum and the elevator's
/// plain-flap increment of [`super::tail_stall`] at the Mach and tail
/// Reynolds numbers of that speed (ISA sea level), the elevator at
/// [`elevator_up_deflection_deg`]. The rotation's transient maximum-elevator
/// limit only. It is optimistic: a smooth, sealed-gap, ice-free tail at
/// full deflection, with no margin for recovery. The landing-trim balance
/// takes Torenbeek's design value [`super::support::SCISSOR_CL_H_MAX`]
/// instead, which keeps that margin.
#[must_use]
pub(super) fn tail_download_cap(
    tail: &alas_geom::aircraft::wing::Wing,
    config: &AlasConfig,
    speed_m_s: f64,
) -> (f64, f64) {
    let controls = &config.control_surfaces;
    let elevator_deg = elevator_up_deflection_deg(config);
    let mach = speed_m_s / SEA_LEVEL_SPEED_OF_SOUND_M_S;
    let reynolds = speed_m_s * tail.reference_planform().mean_aerodynamic_chord()
        / SEA_LEVEL_KINEMATIC_VISCOSITY_M2_S;
    let airfoil = &config.geometry.empennage.tail_airfoil;
    let quarter_chord_cos = tail.mean_sweep_angle(0.25).to_radians().cos().abs();
    if let Some((mach_end, reynolds_end)) = tail_section_chart_excursion(mach, reynolds) {
        tracing::debug!(
            mach,
            reynolds,
            ?mach_end,
            ?reynolds_end,
            "tail section c_l,max is held at the end of its chart, not read from it"
        );
    }
    let clean_section_cap = 0.9 * tail_section_cl_max(airfoil, mach, reynolds) * quarter_chord_cos;
    let hinge_cos = tail
        .mean_sweep_angle(1.0 - controls.elevator_chord_fraction)
        .to_radians()
        .cos()
        .abs();
    let elevator_area_fraction = strip_area_fraction(
        tail.taper_ratio(),
        controls.elevator_span_start_frac,
        controls.elevator_span_end_frac,
    );
    let elevator_increment = symmetric_thickness(airfoil).map_or(0.0, |thickness| {
        plain_flap_delta_cl_max(thickness, controls.elevator_chord_fraction, elevator_deg)
    });
    let section_cap =
        clean_section_cap + 0.9 * elevator_increment * elevator_area_fraction * hinge_cos;
    (clean_section_cap, section_cap)
}

/// The tail incidence at rotation, rad (negative is leading edge down, more
/// download): the trimmable stabiliser's takeoff setting `-nose_up_deg` when
/// it is finite, positive and more nose-up than the built incidence, else the
/// built incidence. The crew never sets a takeoff trim that removes download
/// a forward CG needs, so the setting can only add tail authority.
#[must_use]
pub(super) fn takeoff_incidence_rad(built_incidence_rad: f64, nose_up_deg: Option<f64>) -> f64 {
    nose_up_deg
        .filter(|deg| deg.is_finite() && *deg > 0.0)
        .map_or(built_incidence_rad, |deg| {
            built_incidence_rad.min(-deg.to_radians())
        })
}

/// `CL_h = a_h (alpha + i_h - epsilon + tau delta_e)` with the fuselage at
/// zero angle of attack (Sadraey 2012, sec. 12.6); angles in radians,
/// `delta_e` negative for trailing edge up.
#[must_use]
pub(super) fn tail_lift_coefficient(
    lift_curve_slope_per_rad: f64,
    incidence_rad: f64,
    downwash_rad: f64,
    elevator_effectiveness: f64,
    elevator_deflection_rad: f64,
) -> f64 {
    lift_curve_slope_per_rad
        * (incidence_rad - downwash_rad + elevator_effectiveness * elevator_deflection_rad)
}

/// The thrust and rolling-friction moment about the main-gear contact at
/// nose-wheel lift-off, over `W` (m), nose-up positive.
///
/// Thrust `T` acts forward at `h_T` (moment `-T h_T`). The runway friction
/// `mu (W - L)` acts in the contact plane (no moment). The inertial reaction
/// of the forward acceleration, `m a = T - mu (W - L)`, acts aft at the CG
/// (moment `+m a h_cg`). So the moment over `W` is
/// `T/W (h_cg - h_T) - mu (1 - L/W) h_cg`, with the wheel load clamped at
/// zero. A thrust line below the CG is nose-up; friction is always
/// nose-down. A non-finite or non-positive `h_cg_m` returns `0.0` (term not
/// evaluated); a non-finite thrust or thrust-line height drops only the
/// thrust part.
#[must_use]
pub(super) fn longitudinal_force_moment_m(
    h_cg_m: f64,
    thrust_to_weight: f64,
    thrust_line_height_m: f64,
    rolling_friction_coefficient: f64,
    lift_over_weight: f64,
) -> f64 {
    if !h_cg_m.is_finite() || h_cg_m <= 0.0 {
        return 0.0;
    }
    let thrust_moment = if thrust_to_weight.is_finite() && thrust_line_height_m.is_finite() {
        thrust_to_weight * (h_cg_m - thrust_line_height_m)
    } else {
        0.0
    };
    let wheel_load_fraction = (1.0 - lift_over_weight).max(0.0);
    thrust_moment - rolling_friction_coefficient * wheel_load_fraction * h_cg_m
}

/// The most-forward centre of gravity that still rotates, as its distance
/// ahead of the main-gear contact, `d = x_P - x_cg`, m.
///
/// `available_m` is every moment about the contact except the weight's,
/// over `W`. With `kappa = th'' / g` the balance over `W` is
///
/// `available_m - d = kappa (k_y^2 + h_cg^2 + d^2)`,
///
/// a parabola in `d` whose admissible set (moment available at least the
/// moment required) lies between its two roots; the forward boundary is the
/// larger root, `d = 2 r / (1 + sqrt(1 + 4 kappa r))` with
/// `r = available_m - kappa (k_y^2 + h_cg^2)`, the conjugate form that is
/// exact at `kappa = 0` and free of cancellation. With no real root
/// (`1 + 4 kappa r < 0`) no centre of gravity rotates; the parabola's vertex
/// `d = -1 / (2 kappa)`, the station closest to rotating, is returned, which
/// lies far aft of every physical CG so the boundary rejects every state.
/// A non-finite or non-positive `h_cg_m` contributes no `h_cg^2` term.
#[must_use]
pub(super) fn rotation_cg_offset_m(
    available_m: f64,
    pitch_radius_of_gyration_m: f64,
    h_cg_m: f64,
    pitch_acceleration_deg_s2: f64,
    gravity_m_s2: f64,
) -> f64 {
    let kappa = pitch_acceleration_deg_s2.to_radians() / gravity_m_s2;
    let height_m = if h_cg_m.is_finite() && h_cg_m > 0.0 {
        h_cg_m
    } else {
        0.0
    };
    let r = available_m - kappa * (pitch_radius_of_gyration_m.powi(2) + height_m * height_m);
    let discriminant = 1.0 + 4.0 * kappa * r;
    if discriminant < 0.0 {
        return -0.5 / kappa;
    }
    2.0 * r / (1.0 + discriminant.sqrt())
}

// A test asserts on values it constructed here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const G: f64 = 9.806_65;

    /// `kappa = 5 deg/s^2 / g = 0.0088987 1/m`; with `k_y = 7 m`,
    /// `h_cg = 3 m` and 2.5 m of available moment over `W`,
    /// `r = 2.5 - 58 kappa = 1.983875` and the larger root of
    /// `kappa d^2 + d - r = 0` is `d = 1.95004 m`.
    #[test]
    fn the_offset_is_the_hand_computed_root_and_closes_the_dimensional_balance() {
        let (mass_kg, k_m, h_m, accel, available_m) = (60_000.0, 7.0, 3.0, 5.0, 2.5);
        let d = rotation_cg_offset_m(available_m, k_m, h_m, accel, G);
        assert!((d - 1.950_04).abs() < 1.0e-4, "{d}");
        let weight_n = mass_kg * G;
        let inertia_contact = mass_kg * k_m * k_m + mass_kg * (d * d + h_m * h_m);
        let required_nm = inertia_contact * accel.to_radians();
        let available_nm = weight_n * (available_m - d);
        assert!((required_nm - available_nm).abs() < 1.0e-9 * required_nm);
    }

    /// The inertia about the contact is the inertia of the mass
    /// distribution itself: two point masses `a` either side of the CG
    /// (`I_cg = m a^2`, `k_y = a`) summed directly about the contact equal
    /// `I_cg + m [(x_P - x_cg)^2 + h_cg^2]` at the solved offset.
    #[test]
    fn the_balance_uses_the_inertia_transferred_to_the_contact_point() {
        let (mass_kg, a_m, h_m, accel, available_m) = (20_000.0, 5.0, 2.5, 7.0, 3.0);
        let d = rotation_cg_offset_m(available_m, a_m, h_m, accel, G);
        // Contact at the origin, CG d ahead of it and h_cg above it.
        let direct = [-d - a_m, -d + a_m]
            .iter()
            .map(|x| 0.5 * mass_kg * (x * x + h_m * h_m))
            .sum::<f64>();
        let weight_n = mass_kg * G;
        let residual = weight_n * (available_m - d) - direct * accel.to_radians();
        assert!(residual.abs() < 1.0e-9 * weight_n);
        // Dropping the transfer (the inertia about the CG alone) gives the
        // linear boundary, which is further forward.
        let no_transfer = available_m - accel.to_radians() / G * a_m * a_m;
        assert!(no_transfer > d);
        // A taller CG adds m h^2 to the inertia and moves the boundary aft.
        assert!(rotation_cg_offset_m(available_m, a_m, 4.0, accel, G) < d);
    }

    #[test]
    fn no_pitch_acceleration_reduces_to_the_static_balance() {
        assert_eq!(rotation_cg_offset_m(1.7, 9.0, 3.0, 0.0, G), 1.7);
    }

    #[test]
    fn a_balance_with_no_real_root_returns_the_parabola_vertex_far_aft() {
        let kappa = 5.0_f64.to_radians() / G;
        let d = rotation_cg_offset_m(-100.0, 6.0, 3.0, 5.0, G);
        assert_eq!(d, -0.5 / kappa);
        assert!(d < -50.0);
    }

    /// `tau(0.25)`: `cos theta = -1/2`, `theta = 2 pi / 3`,
    /// `tau = 1 - (2 pi/3 - sqrt(3)/2) / pi = 1/3 + sqrt(3) / (2 pi)`.
    #[test]
    fn the_elevator_effectiveness_is_the_thin_airfoil_value() {
        let expected = 1.0 / 3.0 + 3.0_f64.sqrt() / (2.0 * std::f64::consts::PI);
        assert!((elevator_effectiveness(0.25) - expected).abs() < 1.0e-12);
        assert!(elevator_effectiveness(0.0).abs() < 1.0e-12);
        assert!((elevator_effectiveness(1.0) - 1.0).abs() < 1.0e-12);
        assert!(elevator_effectiveness(1.5).is_nan());
        let mut previous = 0.0;
        for fraction in [0.1, 0.2, 0.3, 0.4, 0.5] {
            assert!(elevator_effectiveness(fraction) > previous);
            previous = elevator_effectiveness(fraction);
        }
    }

    #[test]
    fn the_wieselsberger_factor_is_one_half_at_one_sixteenth_of_the_span() {
        assert!((wieselsberger_factor(2.0, 32.0) - 0.5).abs() < 1.0e-12);
        assert!(wieselsberger_factor(1.0e6, 30.0) > 0.999_999);
        assert_eq!(wieselsberger_factor(f64::NAN, 30.0), 1.0);
    }

    #[test]
    fn the_pitch_radius_comes_from_the_ledger_and_falls_back_to_raymer() {
        assert!(
            (pitch_radius_of_gyration_m(None, 4.0, 60_000.0 * 49.0, 60_000.0, 34.0, 37.6) - 7.0)
                .abs()
                < 1e-12
        );
        let fallback = pitch_radius_of_gyration_m(None, 4.0, f64::NAN, 60_000.0, 34.0, 37.6);
        assert!((fallback - 0.38 * 37.6 / 2.0).abs() < 1.0e-12);
    }

    /// The user's fraction of MAC wins over both the ledger and Raymer; an
    /// unusable override (non-positive, non-finite) falls through.
    #[test]
    fn the_pitch_radius_override_is_a_fraction_of_the_mac() {
        let ledger = 60_000.0 * 49.0;
        let overridden = pitch_radius_of_gyration_m(Some(0.3), 4.5, ledger, 60_000.0, 34.0, 37.6);
        assert!((overridden - 1.35).abs() < 1.0e-12);
        for unusable in [0.0, -0.3, f64::NAN] {
            let radius =
                pitch_radius_of_gyration_m(Some(unusable), 4.5, ledger, 60_000.0, 34.0, 37.6);
            assert!((radius - 7.0).abs() < 1.0e-12, "{unusable}");
        }
    }

    /// 5 deg/s^2 for every class unless the config overrides it.
    #[test]
    fn the_required_pitch_acceleration_is_five_unless_overridden() {
        for preset in ["ATR72-600", "A320-200", "B787-9"] {
            let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
                .expect("a registered preset configures");
            assert_eq!(rotation_pitch_acceleration_deg_s2(&config), 5.0, "{preset}");
            config.landing_gear.rotation_pitch_acceleration_deg_s2 = Some(6.0);
            assert_eq!(rotation_pitch_acceleration_deg_s2(&config), 6.0, "{preset}");
            config.landing_gear.rotation_pitch_acceleration_deg_s2 = Some(-1.0);
            assert_eq!(rotation_pitch_acceleration_deg_s2(&config), 5.0, "{preset}");
        }
    }

    #[test]
    fn the_large_deflection_factor_follows_the_digitised_datcom_figure() {
        // Tabulated points (DATCOM Fig. 6.1.1.1-40).
        assert_eq!(large_deflection_effectiveness_factor(-10.0, 0.30), 1.0);
        assert_eq!(large_deflection_effectiveness_factor(-25.0, 0.25), 0.74);
        assert_eq!(large_deflection_effectiveness_factor(-25.0, 0.30), 0.72);
        assert_eq!(large_deflection_effectiveness_factor(-25.0, 0.40), 0.68);
        // Linear between chords: c_f/c = 0.35 at 25 deg is halfway.
        assert!((large_deflection_effectiveness_factor(-25.0, 0.35) - 0.70).abs() < 1.0e-12);
        // Clamped outside the tabulated chord and deflection ranges.
        assert_eq!(large_deflection_effectiveness_factor(-25.0, 0.10), 0.74);
        assert_eq!(large_deflection_effectiveness_factor(-60.0, 0.30), 0.56);
        // Monotone non-increasing in |delta|, symmetric in sign, larger
        // chords lose more.
        for chord in [0.25, 0.3, 0.35, 0.4] {
            let mut previous = 1.0;
            for delta in 0..=60 {
                let factor = large_deflection_effectiveness_factor(f64::from(delta), chord);
                assert!(factor <= previous + 1.0e-15 && factor > 0.0);
                assert_eq!(
                    factor,
                    large_deflection_effectiveness_factor(-f64::from(delta), chord)
                );
                assert!(factor <= large_deflection_effectiveness_factor(f64::from(delta), 0.25));
                previous = factor;
            }
        }
    }

    #[test]
    fn a_trimmable_stabiliser_can_only_add_takeoff_download() {
        let built = (-2.0_f64).to_radians();
        // Fixed stabiliser, or a setting that is not a positive angle: the
        // built incidence holds.
        for setting in [None, Some(0.0), Some(-3.0), Some(f64::NAN)] {
            assert_eq!(takeoff_incidence_rad(built, setting), built);
        }
        // A nose-up setting beyond the built incidence replaces it.
        assert_eq!(
            takeoff_incidence_rad(built, Some(4.3)),
            -(4.3_f64.to_radians())
        );
        // A setting less nose-up than the built incidence never removes
        // download.
        assert_eq!(takeoff_incidence_rad(built, Some(1.0)), built);
    }

    /// The registered presets with a trimmable stabiliser get more takeoff
    /// download than their built incidence gives, and the fixed-stabiliser
    /// ATR keeps its built incidence.
    #[test]
    fn the_takeoff_trim_raises_the_rotation_download_of_the_stabiliser_presets() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let ground = super::super::support::ground_z_m(&plane.fuselages[0], &config);
            let cl_ground = config.performance.cl_max_to
                * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
            let tail =
                rotation_tail_lift(&plane, &config, ground, cl_ground, 75.0).expect("a tail");
            match config.landing_gear.takeoff_stabilizer_nose_up_deg {
                Some(setting) => {
                    assert!(tail.incidence_rad < tail.built_incidence_rad, "{name}");
                    assert!((tail.incidence_rad + setting.to_radians()).abs() < 1.0e-12);
                }
                None => assert_eq!(tail.incidence_rad, tail.built_incidence_rad, "{name}"),
            }
        }
        let atr = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .expect("the ATR configures");
        assert_eq!(atr.landing_gear.takeoff_stabilizer_nose_up_deg, None);
    }

    /// The registered presets' derived tail lift at rotation: the linear
    /// value keeps the thin-airfoil effectiveness times the large-deflection
    /// factor, never exceeds the thin-airfoil value of the previous model
    /// (Sadraey's -1 to -1.5 band), and is limited by the section cap.
    #[test]
    fn the_derived_tail_lift_is_corrected_for_deflection_and_capped_by_the_section() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let fuselage = &plane.fuselages[0];
            let ground = super::super::support::ground_z_m(fuselage, &config);
            let cl_ground = config.performance.cl_max_to
                * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
            let tail =
                rotation_tail_lift(&plane, &config, ground, cl_ground, 75.0).expect("a tail");
            let controls = &config.control_surfaces;
            let span_fraction = (controls.elevator_span_end_frac
                - controls.elevator_span_start_frac)
                .clamp(0.0, 1.0);
            let thin = elevator_effectiveness(controls.elevator_chord_fraction) * span_fraction;
            let deflection = elevator_up_deflection_deg(&config);
            let expected = thin
                * large_deflection_effectiveness_factor(
                    deflection,
                    controls.elevator_chord_fraction,
                );
            assert!(
                (tail.elevator_effectiveness - expected).abs() < 1.0e-12,
                "{name}"
            );
            let uncorrected = tail_lift_coefficient(
                tail.lift_curve_slope_per_rad,
                tail.incidence_rad,
                tail.downwash_rad,
                thin,
                deflection.to_radians(),
            );
            assert!(
                tail.linear_lift_coefficient > uncorrected,
                "{name}: the correction weakens the download"
            );
            assert!(
                tail.lift_coefficient >= -tail.section_cap - 1.0e-12,
                "{name}"
            );
            assert!(
                tail.lift_coefficient == tail.linear_lift_coefficient
                    || (tail.lift_coefficient + tail.section_cap).abs() < 1.0e-12,
                "{name}"
            );
            assert!(tail.lift_coefficient < 0.0, "{name}: {tail:?}");
        }
    }

    /// Probe: prints the rotation tail-lift terms of every registered preset.
    #[test]
    #[ignore = "diagnostic table of all registered presets; prints"]
    // A probe prints its numbers; it asserts nothing.
    #[allow(clippy::print_stdout)]
    fn registered_preset_tail_stall_table() {
        use alas_geom::builder::AircraftBuilder;
        for name in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let ground = super::super::support::ground_z_m(&plane.fuselages[0], &config);
            let cl_ground = config.performance.cl_max_to
                * config.landing_gear.cl_ground_attitude_frac_of_cl_max_to;
            if let Some(tail) = rotation_tail_lift(&plane, &config, ground, cl_ground, 78.0) {
                println!(
                    "{name}: linear {:.3} cap {:.3} (clean {:.3}) applied {:.3} incidence {:.2} deg",
                    tail.linear_lift_coefficient,
                    -tail.section_cap,
                    -tail.clean_section_cap,
                    tail.lift_coefficient,
                    tail.incidence_rad.to_degrees()
                );
            }
        }
    }
}
