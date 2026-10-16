// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail sizing from volume coefficients, per topology, and the V-tail mixer.
//!
//! Sources (read on the page): Raymer, 6th ed., Eq. 6.26-6.29 and Table 6.4,
//! p. 159-160 (volume coefficients; T-tail, H-tail, all-moving and V-tail
//! rules in the text of p. 160); Sadraey Eq. 6.24 and 6.47, Examples 6.1 and
//! 6.2, pp. 301 and 331-335 (optimum tail arm and tail area).
//!
//! The V-tail is sized with effective areas `S cos^2(G)` (pitch stability)
//! and `S sin^2(G)` (yaw stability); their sum is the true area, which is
//! exactly Raymer's rule that the V surfaces carry "the same total surface
//! area as required for conventional tails" and that
//! `G = atan(sqrt(S_v / S_h))`, near 45 deg (p. 160).

use crate::spec::topology::TailKind;

use super::AirframeError;

/// Aircraft classes of Raymer Table 6.4 that are relevant to small UAVs plus
/// the rest of the table for completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AircraftClass {
    /// Sailplane: 0.50 / 0.02.
    Sailplane,
    /// Homebuilt: 0.50 / 0.04.
    Homebuilt,
    /// General aviation, single engine: 0.70 / 0.04.
    GaSingle,
    /// General aviation, twin engine: 0.80 / 0.07.
    GaTwin,
    /// Agricultural: 0.50 / 0.04.
    Agricultural,
    /// Twin turboprop: 0.90 / 0.08.
    TwinTurboprop,
    /// Flying boat: 0.70 / 0.06.
    FlyingBoat,
    /// Jet trainer: 0.70 / 0.06.
    JetTrainer,
    /// Jet fighter: 0.40 / 0.07 (table gives 0.07-0.12).
    JetFighter,
    /// Military cargo or bomber: 1.00 / 0.08.
    MilitaryCargo,
    /// Jet transport: 1.00 / 0.09.
    JetTransport,
}

/// Horizontal and vertical tail volume coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeCoefficients {
    /// `c_HT = L_HT S_HT / (c_w S_w)`.
    pub horizontal: f64,
    /// `c_VT = L_VT S_VT / (b_w S_w)`.
    pub vertical: f64,
}

impl VolumeCoefficients {
    /// Raymer Table 6.4 (p. 160).
    #[must_use]
    pub fn raymer(class: AircraftClass) -> Self {
        let (horizontal, vertical) = match class {
            AircraftClass::Sailplane => (0.50, 0.02),
            AircraftClass::Homebuilt => (0.50, 0.04),
            AircraftClass::GaSingle => (0.70, 0.04),
            AircraftClass::GaTwin => (0.80, 0.07),
            AircraftClass::Agricultural => (0.50, 0.04),
            AircraftClass::TwinTurboprop => (0.90, 0.08),
            AircraftClass::FlyingBoat => (0.70, 0.06),
            AircraftClass::JetTrainer => (0.70, 0.06),
            AircraftClass::JetFighter => (0.40, 0.07),
            AircraftClass::MilitaryCargo => (1.00, 0.08),
            AircraftClass::JetTransport => (1.00, 0.09),
        };
        Self {
            horizontal,
            vertical,
        }
    }

    /// Topology adjustment of Raymer p. 160: T-tail both coefficients -5 %
    /// (end-plate effect on the fin, clean air on the horizontal tail).
    #[must_use]
    pub fn for_topology(self, kind: TailKind) -> Self {
        match kind {
            TailKind::T => Self {
                horizontal: self.horizontal * 0.95,
                vertical: self.vertical * 0.95,
            },
            _ => self,
        }
    }

    /// All-moving tail: horizontal coefficient reduced by 10-15 % (Raymer
    /// p. 160); the mid value 12.5 % is used.
    #[must_use]
    pub fn all_moving(self) -> Self {
        Self {
            horizontal: self.horizontal * 0.875,
            ..self
        }
    }
}

/// Tail area from a volume coefficient: `S = c_V l_ref S_w / L`
/// (Raymer Eq. 6.28-6.29; Sadraey Eq. 6.24).  `ref_length_m` is the wing MAC
/// for the horizontal tail and the wing span for the vertical tail.
///
/// # Errors
///
/// Non-positive reference quantities.
pub fn area_from_volume(
    volume: f64,
    ref_length_m: f64,
    s_wing_m2: f64,
    arm_m: f64,
) -> Result<f64, AirframeError> {
    if !(ref_length_m > 0.0 && s_wing_m2 > 0.0 && arm_m > 0.0 && volume >= 0.0) {
        return Err(AirframeError::Invalid(
            "tail sizing needs positive reference length, wing area and arm".into(),
        ));
    }
    Ok(volume * ref_length_m * s_wing_m2 / arm_m)
}

/// Volume coefficient of an existing tail, `S L / (l_ref S_w)`.
#[must_use]
pub fn volume_coefficient(area_m2: f64, arm_m: f64, ref_length_m: f64, s_wing_m2: f64) -> f64 {
    area_m2 * arm_m / (ref_length_m * s_wing_m2)
}

/// Sadraey Eq. 6.47: tail arm that minimises drag for a given volume
/// coefficient, `l_opt = K_c sqrt(4 c S V_H / (pi D_f))`.  `kc` is the
/// fuselage correction factor (1.4 for a twin-seat GA aircraft, Example 6.1;
/// 1.2 for the motor glider of Example 6.2).
#[must_use]
pub fn optimum_tail_arm(kc: f64, mac_m: f64, s_wing_m2: f64, v_h: f64, d_fus_m: f64) -> f64 {
    kc * (4.0 * mac_m * s_wing_m2 * v_h / (std::f64::consts::PI * d_fus_m)).sqrt()
}

/// Dihedral of a V-tail from the required effective areas,
/// `atan(sqrt(S_v / S_h))`, rad (Raymer p. 160).
#[must_use]
pub fn v_tail_dihedral_rad(s_h_eff_m2: f64, s_v_eff_m2: f64) -> f64 {
    (s_v_eff_m2 / s_h_eff_m2).sqrt().atan()
}

/// Sized tail areas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TailAreas {
    /// Effective horizontal-tail area, m^2 (`S cos^2 G` for a V pair).
    pub horizontal_eff_m2: f64,
    /// Effective vertical-tail area, m^2 (total of all fins).
    pub vertical_eff_m2: f64,
    /// True area of the V pair, m^2 (`None` for other topologies).
    pub v_pair_area_m2: Option<f64>,
    /// V-pair half-dihedral, rad (`None` for other topologies).
    pub v_dihedral_rad: Option<f64>,
}

/// Size the tails of a topology from volume coefficients.
///
/// `arm_h_m` and `arm_v_m` are the distances between the wing and tail
/// aerodynamic centres (quarter chords).  For a V-tail `v_dihedral_rad`
/// fixes `G` (the true area then satisfies both requirements), or `None`
/// takes Raymer's `atan(sqrt(S_v/S_h))`.  A flying wing (`TailKind::None`)
/// has no tail.
///
/// # Errors
///
/// As [`area_from_volume`].
#[allow(clippy::too_many_arguments)]
pub fn size_from_volume(
    kind: TailKind,
    coeffs: VolumeCoefficients,
    s_wing_m2: f64,
    mac_m: f64,
    span_m: f64,
    arm_h_m: f64,
    arm_v_m: f64,
    v_dihedral_rad: Option<f64>,
) -> Result<TailAreas, AirframeError> {
    if matches!(kind, TailKind::None) {
        return Ok(TailAreas {
            horizontal_eff_m2: 0.0,
            vertical_eff_m2: 0.0,
            v_pair_area_m2: None,
            v_dihedral_rad: None,
        });
    }
    let c = coeffs.for_topology(kind);
    let s_h = area_from_volume(c.horizontal, mac_m, s_wing_m2, arm_h_m)?;
    let s_v = area_from_volume(c.vertical, span_m, s_wing_m2, arm_v_m)?;
    if matches!(kind, TailKind::V { .. } | TailKind::InvV { .. }) {
        let g = v_dihedral_rad.map_or_else(|| v_tail_dihedral_rad(s_h, s_v), f64::abs);
        let need_h = s_h / g.cos().powi(2);
        let need_v = if g.sin() > 1e-9 {
            s_v / g.sin().powi(2)
        } else {
            0.0
        };
        let area = need_h.max(need_v);
        return Ok(TailAreas {
            horizontal_eff_m2: area * g.cos().powi(2),
            vertical_eff_m2: area * g.sin().powi(2),
            v_pair_area_m2: Some(area),
            v_dihedral_rad: Some(g),
        });
    }
    Ok(TailAreas {
        horizontal_eff_m2: s_h,
        vertical_eff_m2: s_v,
        v_pair_area_m2: None,
        v_dihedral_rad: None,
    })
}

/// V-tail control mixer, derived from the surface normal forces.
///
/// With `G` the half-dihedral from horizontal (negative for an inverted V),
/// the right surface (y > 0) has normal `(y, z) = (-sin G, cos G)` and the
/// left `(+sin G, cos G)`.  A deflection `d_i` (trailing edge down positive)
/// produces a normal force `N_i = q (S/2) CLa tau d_i`, hence
///
/// ```text
/// F_z = cos G (N_R + N_L) = q S CLa tau cos G * d_pitch,   d_pitch = (d_L + d_R)/2
/// F_y = sin G (N_L - N_R) = q S CLa tau sin G * d_yaw,     d_yaw   = (d_L - d_R)/2
/// ```
///
/// `F_z > 0` is up; `F_y > 0` pushes the tail to starboard (nose to port in
/// x-aft, z-up axes).  The surface that carries the worst combined demand
/// deflects `|d_pitch| + |d_yaw|`.  (INFERENCE: derived here from first
/// principles and checked by algebra tests; not taken from a text.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VTailMixer {
    /// Half-dihedral from horizontal, rad.
    pub gamma_rad: f64,
}

impl VTailMixer {
    /// Surface deflections `(left, right)` for pitch and yaw commands.
    #[must_use]
    pub fn surface_deflections(&self, d_pitch: f64, d_yaw: f64) -> (f64, f64) {
        (d_pitch + d_yaw, d_pitch - d_yaw)
    }

    /// Pitch and yaw commands from the left and right deflections.
    #[must_use]
    pub fn commands(&self, d_left: f64, d_right: f64) -> (f64, f64) {
        (0.5 * (d_left + d_right), 0.5 * (d_left - d_right))
    }

    /// Forces per unit dynamic pressure `(F_z/q, F_y/q)`, m^2, for true area
    /// `s_true_m2`, surface lift slope `cl_alpha` (1/rad) and effectiveness
    /// `tau`.
    #[must_use]
    pub fn forces_per_q(
        &self,
        s_true_m2: f64,
        cl_alpha: f64,
        tau: f64,
        d_left: f64,
        d_right: f64,
    ) -> (f64, f64) {
        let n = |d: f64| 0.5 * s_true_m2 * cl_alpha * tau * d;
        let (nl, nr) = (n(d_left), n(d_right));
        (
            self.gamma_rad.cos() * (nr + nl),
            self.gamma_rad.sin() * (nl - nr),
        )
    }

    /// Deflections `(left, right)` that produce the demanded `(F_z/q, F_y/q)`.
    ///
    /// # Errors
    ///
    /// A pure fin (`G = 90 deg`) cannot make `F_z`, and `G = 0` cannot make
    /// `F_y`: a non-zero demand on the missing axis is rejected.
    pub fn deflections_for_forces(
        &self,
        s_true_m2: f64,
        cl_alpha: f64,
        tau: f64,
        fz_per_q: f64,
        fy_per_q: f64,
    ) -> Result<(f64, f64), AirframeError> {
        let k = s_true_m2 * cl_alpha * tau;
        let (cg, sg) = (self.gamma_rad.cos(), self.gamma_rad.sin());
        let d_pitch = if cg.abs() > 1e-9 {
            fz_per_q / (k * cg)
        } else if fz_per_q.abs() < 1e-12 {
            0.0
        } else {
            return Err(AirframeError::NoSolution(
                "a vertical fin pair cannot produce vertical force".into(),
            ));
        };
        let d_yaw = if sg.abs() > 1e-9 {
            fy_per_q / (k * sg)
        } else if fy_per_q.abs() < 1e-12 {
            0.0
        } else {
            return Err(AirframeError::NoSolution(
                "a horizontal pair cannot produce side force".into(),
            ));
        };
        Ok(self.surface_deflections(d_pitch, d_yaw))
    }

    /// Largest single-surface deflection for peak pitch and yaw commands.
    #[must_use]
    pub fn worst_surface_deflection(d_pitch_max: f64, d_yaw_max: f64) -> f64 {
        d_pitch_max.abs() + d_yaw_max.abs()
    }
}

/// Result of the T-tail wake screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WakeScreen {
    /// Height of the tail aerodynamic centre above the wake centre line, m
    /// (negative: below it).
    pub height_above_wake_m: f64,
    /// The tail lies within the assumed wake half-width: deep-stall risk.
    pub in_wake: bool,
}

/// Geometric deep-stall screen for a T-tail (plan 3.4: "deep-stall flag from
/// wake-line geometry").  INFERENCE, not validated, and only a screen.
///
/// The wake centre line leaves the wing trailing edge at the MAC station and
/// follows the relative wind deflected by the downwash at the tail: in x-aft,
/// z-up body axes, at body angle of attack `alpha` the line has slope
/// `tan(alpha - eps)` with `eps = (d eps/d alpha) alpha`, so
/// `z_wake(x) = z_te + (x - x_te) tan(alpha - eps)`.  The tail is flagged
/// when `|z_tail - z_wake| < half_width_chords * c` (default assumption: one
/// wing MAC, ESTIMATE).
///
/// # Errors
///
/// A tail not aft of the wing trailing edge.
pub fn t_tail_wake_screen(
    wing_te_m: [f64; 3],
    tail_ac_m: [f64; 3],
    alpha_rad: f64,
    downwash_gradient: f64,
    mac_m: f64,
    half_width_chords: f64,
) -> Result<WakeScreen, AirframeError> {
    let dx = tail_ac_m[0] - wing_te_m[0];
    if !(dx.is_finite() && dx > 0.0) {
        return Err(AirframeError::Invalid(
            "the tail must be aft of the wing trailing edge".into(),
        ));
    }
    let eps = downwash_gradient * alpha_rad;
    let z_wake = wing_te_m[2] + dx * (alpha_rad - eps).tan();
    let h = tail_ac_m[2] - z_wake;
    Ok(WakeScreen {
        height_above_wake_m: h,
        in_wake: h.abs() < half_width_chords * mac_m,
    })
}
