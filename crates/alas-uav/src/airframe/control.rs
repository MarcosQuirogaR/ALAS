// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Control-surface geometry, hinge moment and the servo torque check.
//!
//! Hinge moment (Sadraey, Eq. 12.147-12.150, p. 714, read on the page):
//!
//! ```text
//! H  = 1/2 rho V^2 S_cs c_cs Ch,     Ch = Ch0 + Ch_alpha alpha_LS + Ch_delta delta
//! ```
//!
//! with `S_cs` the control-surface planform area, `c_cs` its mean chord,
//! `alpha_LS` the angle of attack of the host lifting surface and `delta` the
//! deflection.  Sadraey gives `Ch_alpha` about -0.1 and `Ch_delta` about -0.3
//! per radian as typical values and refers to DATCOM/Roskam for charts.  Both
//! derivatives are ESTIMATES with the plan's +/-30 % uncertainty
//! ([`CH_UNCERTAINTY`]); a measured or chart value must replace them for a
//! margin claim.  `H` is the aerodynamic moment about the hinge line, N m, in
//! the sense that opposes positive (trailing-edge-down) deflection when
//! negative.  Only its magnitude matters for the servo.
//!
//! Servo torque: the horn of length `r_h` (hinge to attachment) turns the
//! servo arm of radius `r_s` through the linkage, so the servo torque is
//! `T = |H| r_s / (r_h eta_link) + friction`; the usable servo torque is
//! 50 % of the stall torque at the actual supply voltage (component-data.md;
//! plan 3.4), and the check applies a design margin on top of `T`.

use alas_uav_db::parts::ServoSpec;

use super::geometry::{AirframeGeometry, Planform};
use super::AirframeError;
use crate::spec::master::{ControlSurface, HostSurface};

/// Typical `Ch_alpha`, 1/rad (Sadraey p. 714).  ESTIMATE.
pub const CH_ALPHA_TYPICAL: f64 = -0.1;
/// Typical `Ch_delta`, 1/rad (Sadraey p. 714).  ESTIMATE.
pub const CH_DELTA_TYPICAL: f64 = -0.3;
/// Relative uncertainty on hinge-moment derivatives from charts (plan 3.4).
pub const CH_UNCERTAINTY: f64 = 0.30;
/// Fraction of the stall torque that is considered usable (component-data.md).
pub const USABLE_STALL_FRACTION: f64 = 0.5;

/// Geometry of a hinged surface on one side of the host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlGeometry {
    /// Planform area of one surface, m^2.
    pub area_m2: f64,
    /// Mean chord of the surface (`area / span`), m.
    pub mean_chord_m: f64,
    /// Span of one surface, m.
    pub span_m: f64,
    /// Centroid x of the surface planform, m.
    pub centroid_x_m: f64,
}

/// Area, mean chord and centroid of a hinged control surface on one half of
/// `host`, between span fractions `span_start` and `span_end` of the half span
/// and with hinged chord fraction `chord_fraction` (the aft part of the chord).
///
/// # Errors
///
/// Fractions outside the valid ranges.
pub fn control_geometry(
    host: &Planform,
    span_start: f64,
    span_end: f64,
    chord_fraction: f64,
) -> Result<ControlGeometry, AirframeError> {
    if !(0.0..1.0).contains(&span_start)
        || !(span_start < span_end && span_end <= 1.0)
        || !(chord_fraction > 0.0 && chord_fraction <= 0.7)
    {
        return Err(AirframeError::Invalid(
            "control surface needs 0 <= span_start < span_end <= 1 and 0 < chord_fraction <= 0.7"
                .into(),
        ));
    }
    let half = host.half_span_m();
    let (s0, s1) = (span_start * half, span_end * half);
    let n = 200;
    let ds = (s1 - s0) / f64::from(n);
    let (mut area, mut mx) = (0.0, 0.0);
    for i in 0..n {
        let s = s0 + (f64::from(i) + 0.5) * ds;
        let (c, x_le) = host
            .chord_and_le_x_at(s)
            .ok_or_else(|| AirframeError::Invalid("control surface beyond the tip".into()))?;
        let cs_chord = chord_fraction * c;
        let x_c = x_le + c - 0.5 * cs_chord;
        area += cs_chord * ds;
        mx += cs_chord * ds * x_c;
    }
    let span = s1 - s0;
    Ok(ControlGeometry {
        area_m2: area,
        mean_chord_m: area / span,
        span_m: span,
        centroid_x_m: mx / area,
    })
}

/// Hinge-moment model of one control surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeMoment {
    /// Surface planform area `S_cs`, m^2.
    pub s_cs_m2: f64,
    /// Mean chord `c_cs`, m.
    pub c_cs_m: f64,
    /// `Ch0` (zero for a symmetric section), 1.
    pub ch0: f64,
    /// `Ch_alpha`, 1/rad.
    pub ch_alpha: f64,
    /// `Ch_delta`, 1/rad.
    pub ch_delta: f64,
}

impl HingeMoment {
    /// Model with the typical derivatives of Sadraey p. 714 (ESTIMATE).
    #[must_use]
    pub fn typical(geometry: &ControlGeometry) -> Self {
        Self {
            s_cs_m2: geometry.area_m2,
            c_cs_m: geometry.mean_chord_m,
            ch0: 0.0,
            ch_alpha: CH_ALPHA_TYPICAL,
            ch_delta: CH_DELTA_TYPICAL,
        }
    }

    /// Hinge moment `H`, N m, at airspeed `v` (m/s), density `rho` (kg/m^3),
    /// host angle of attack `alpha` and deflection `delta` (rad).
    #[must_use]
    pub fn moment_nm(&self, rho: f64, v: f64, alpha_rad: f64, delta_rad: f64) -> f64 {
        0.5 * rho
            * v
            * v
            * self.s_cs_m2
            * self.c_cs_m
            * (self.ch0 + self.ch_alpha * alpha_rad + self.ch_delta * delta_rad)
    }

    /// Magnitude of `H` evaluated with the derivatives scaled by
    /// `1 + CH_UNCERTAINTY` (the conservative bound on the magnitude).
    #[must_use]
    pub fn moment_upper_nm(&self, rho: f64, v: f64, alpha_rad: f64, delta_rad: f64) -> f64 {
        let scaled = Self {
            ch0: self.ch0 * (1.0 + CH_UNCERTAINTY),
            ch_alpha: self.ch_alpha * (1.0 + CH_UNCERTAINTY),
            ch_delta: self.ch_delta * (1.0 + CH_UNCERTAINTY),
            ..*self
        };
        scaled.moment_nm(rho, v, alpha_rad, delta_rad).abs()
    }
}

/// A flight condition for the hinge-moment sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct HingeCase {
    /// Name (for example `dive`, `stall`).
    pub name: String,
    /// True airspeed, m/s.
    pub speed_ms: f64,
    /// Air density, kg/m^3.
    pub rho: f64,
    /// Host-surface angle of attack, rad.
    pub alpha_rad: f64,
    /// Control deflection, rad.
    pub delta_rad: f64,
}

/// Worst case `(index, |H| with derivative uncertainty, N m)` over the cases.
///
/// # Errors
///
/// Empty case list.
pub fn worst_hinge_case(
    model: &HingeMoment,
    cases: &[HingeCase],
) -> Result<(usize, f64), AirframeError> {
    cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                i,
                model.moment_upper_nm(c.rho, c.speed_ms, c.alpha_rad, c.delta_rad),
            )
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .ok_or_else(|| AirframeError::Invalid("no hinge-moment cases".into()))
}

/// Linkage and margin inputs of the servo check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServoLoad {
    /// Hinge moment to be reacted, N m (per surface).
    pub hinge_moment_nm: f64,
    /// Servo arm radius, m.
    pub servo_arm_m: f64,
    /// Control horn radius (hinge line to attachment), m.
    pub horn_m: f64,
    /// Linkage efficiency (0, 1].
    pub linkage_efficiency: f64,
    /// Added friction and binding torque at the servo, N m.
    pub friction_nm: f64,
    /// Design margin on the required torque (1.5 = 50 % margin).
    pub margin_factor: f64,
    /// Servos sharing the surface.
    pub n_servos: u32,
    /// Maximum deflection to reach, rad.
    pub delta_max_rad: f64,
    /// Available servo travel, rad, total.
    pub servo_travel_rad: f64,
    /// Supply voltage, V.
    pub voltage_v: f64,
}

/// Outcome of the servo check.
#[derive(Debug, Clone, PartialEq)]
pub struct ServoCheck {
    /// Required servo torque per servo (with margin), N m.
    pub required_nm: f64,
    /// Stall torque at the supply voltage, N m.
    pub stall_nm: f64,
    /// Usable torque (50 % of stall), N m.
    pub usable_nm: f64,
    /// `required / usable` (above 1 fails).
    pub ratio: f64,
    /// Servo rotation needed for the maximum deflection, rad.
    pub travel_needed_rad: f64,
    /// Travel is available.
    pub travel_ok: bool,
    /// Stall current at the supply voltage, A, when the database has it.
    pub stall_current_a: Option<f64>,
    /// Torque check passes.
    pub torque_ok: bool,
}

/// Check a servo against a hinge load.
///
/// # Errors
///
/// A missing torque curve, a voltage outside it (no extrapolation) or
/// non-physical linkage values.
pub fn servo_check(
    servo: &ServoSpec,
    load: &ServoLoad,
    what: &str,
) -> Result<ServoCheck, AirframeError> {
    if !(load.servo_arm_m > 0.0
        && load.horn_m > 0.0
        && load.linkage_efficiency > 0.0
        && load.linkage_efficiency <= 1.0
        && load.n_servos >= 1
        && load.margin_factor >= 1.0)
    {
        return Err(AirframeError::Invalid(
            "servo check needs positive arm and horn, efficiency in (0, 1], >= 1 servo, margin >= 1"
                .into(),
        ));
    }
    let curve = servo
        .torque_vs_voltage
        .as_ref()
        .ok_or_else(|| AirframeError::Missing {
            what: what.to_owned(),
            field: "torque_vs_voltage",
        })?;
    let stall = curve.at(load.voltage_v).ok_or_else(|| {
        AirframeError::Invalid(format!(
            "{what}: {} V is outside the tabulated torque range",
            load.voltage_v
        ))
    })?;
    let per_servo = load.hinge_moment_nm.abs() / f64::from(load.n_servos);
    let required = (per_servo * load.servo_arm_m / (load.horn_m * load.linkage_efficiency)
        + load.friction_nm)
        * load.margin_factor;
    let usable = USABLE_STALL_FRACTION * stall;
    let travel = load.delta_max_rad * load.horn_m / load.servo_arm_m;
    let ratio = required / usable;
    Ok(ServoCheck {
        required_nm: required,
        stall_nm: stall,
        usable_nm: usable,
        ratio,
        travel_needed_rad: travel,
        travel_ok: travel <= load.servo_travel_rad,
        stall_current_a: servo
            .i_stall_vs_voltage
            .as_ref()
            .and_then(|c| c.at(load.voltage_v)),
        torque_ok: ratio <= 1.0,
    })
}

/// Geometry of one surface of a spec `ControlSurface` on the derived
/// airframe geometry (host wing, horizontal tail or V pair, or fin).
///
/// # Errors
///
/// The host surface does not exist in the geometry, or the spec fractions
/// are invalid.
pub fn geometry_of_control(
    geometry: &AirframeGeometry,
    control: &ControlSurface,
) -> Result<ControlGeometry, AirframeError> {
    let host = match control.host {
        HostSurface::Wing => &geometry.wing,
        HostSurface::HorizontalTail => {
            &geometry
                .horizontal
                .as_ref()
                .ok_or_else(|| AirframeError::UnknownName("horizontal tail".into()))?
                .planform
        }
        HostSurface::VerticalTail => {
            &geometry
                .vertical
                .as_ref()
                .ok_or_else(|| AirframeError::UnknownName("vertical tail".into()))?
                .planform
        }
    };
    control_geometry(
        host,
        control.span_start,
        control.span_end,
        control.chord_fraction,
    )
}
