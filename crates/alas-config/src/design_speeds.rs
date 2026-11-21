// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Design cruising and design dive speeds, CS-25 Amendment 28, CS 25.335(a)-(b) and
//! CS 25.1505.
//!
//! # The rule
//!
//! CS 25.335: "The selected design airspeeds are equivalent airspeeds (EAS)."
//!
//! * **VC/MC.** CS 25.1505: VMO/MMO "must be established so that it is not greater than the
//!   design cruising speed VC". The least VC that admits the certified operating envelope is
//!   therefore VMO itself, converted to equivalent airspeed at each pressure altitude, and the
//!   least MC is MMO (CS 25.335(a)(3): "At altitudes where VD is limited by Mach number, VC may
//!   be limited to a selected Mach number").
//! * **VD/MD.** CS 25.335(b): "VD must be selected so that VC/MC is not greater than 0.8 VD/MD,
//!   or so that the minimum speed margin between VC/MC and VD/MD is the greater of" (b)(1) and
//!   (b)(2). The two branches are alternatives the applicant chooses between; the least VD the
//!   text admits is the smaller of `1.25 VC` and `VC + max(margin (b)(1), margin (b)(2))`.
//!   Transport aeroplanes are certified on the margin branch, which is the smaller one at their
//!   speeds; [`dive_speed_from_operating_limits`] takes the smaller of the two.
//!   * (b)(1)(i), the upset criterion: "From an initial condition of stabilised flight at
//!     VC/MC, the aeroplane is upset, flown for 20 seconds along a flight path 7.5 deg below
//!     the initial path, and then pulled up at a load factor of 1.5 g (0.5 g acceleration
//!     increment). The speed increase occurring in this manoeuvre may be calculated if reliable
//!     or conservative aerodynamic data is used. Power as specified in CS 25.175(b)(1)(iv) is
//!     assumed until the pullup is initiated, at which time power reduction and the use of pilot
//!     controlled drag devices may be assumed". CS 25.175(b)(1)(iv) is the maximum cruising
//!     power, "except that the power need not exceed that required at VMO/MMO": thrust equals
//!     the drag of level flight at VC.
//!   * (b)(1)(ii), "For aeroplanes equipped with a high speed protection function: In lieu of
//!     subparagraph (b)(1)(i), the speed increase above VC/MC resulting from the greater of the
//!     following manoeuvres must be established". It applies to a type whose certification
//!     basis carries the function ([`UpsetCriterion::HighSpeedProtection`]; the presets cite the
//!     special condition or CRI). (A): "upset so as to take up a new flight path 7.5 deg below
//!     the initial path. Control application, up to full authority, is made to try and maintain
//!     this new flight path. Twenty seconds after achieving the new flight path, manual recovery
//!     is made at a load factor of 1.5 g (0.5 g acceleration increment), or such greater load
//!     factor that is automatically applied by the system with the pilot's pitch control
//!     neutral". (B): "From a speed below VC/MC, with power to maintain stabilised level flight
//!     at this speed, the aeroplane is upset so as to accelerate through VC/MC at a flight path
//!     15 deg below the initial path (or at the steepest nose down attitude that the system will
//!     permit with full control authority if less than 15 deg). [...] Recovery may be initiated
//!     3 seconds after operation of high speed, attitude, or other alerting system by
//!     application of a load factor of 1.5 g". The alert is the CS 25.1303(c)(1) speed warning,
//!     "whenever the speed exceeds VMO plus 11.1 km/h (6 knots) or MMO + 0.01"; its latest
//!     permitted setting, VMO + 6 kt calibrated, is taken ([`OVERSPEED_WARNING_MARGIN_CAS_M_S`]).
//!     (B) is evaluated as written, at 15 deg, with the path held (the pitch-up the system applies
//!     with the stick neutral is not credited) and the least load factor. (A) cannot be evaluated
//!     without the control law: the special conditions only state that the function "limits
//!     nose-down pilot authority at speeds above VC/MC" (FAA 25-535-SC) and that it acts
//!     "automatically to initiate recovery before the end of the 20-second period" (FAA
//!     25-470-SC). Its recovery is taken to begin at the overspeed warning speed (the earliest a
//!     limiter may act: "must not impede normal attainment of speeds up to overspeed warning",
//!     FAA 25-536-SC), at 1.5 g. That is an assumption; it does not decide VD: (A) stays below
//!     (B) for any recovery that begins within 15 s of the upset (14.99 s at a VC of 300 kt,
//!     16.06 s at 350 kt), so VD is (B) unless the function lets the path be held for longer.
//!     The A320 basis says it does not: SC A-2.2.3 (Explanatory Note to TCDS EASA.A.064,
//!     Annex I Issue 25, p 14) states that the protection gives "an order in view to pull up"
//!     above a set speed and "cannot allow to maintain the 7,5 o slope during 20 seconds"; it
//!     gives no onset speed, so the 15 s bound itself is not checked from public text.
//!   * (b)(2), atmospheric variations: "The margin at altitude where MC is limited by
//!     compressibility effects must not be less than 0.07M unless a lower margin is determined
//!     using a rational analysis that includes the effects of any automatic systems. In any
//!     case, the margin may not be reduced to less than 0.05M." AMC 25.335(b)(2) 4.b: "At
//!     altitudes where speed is limited by Mach number, a speed margin of .07 Mach between MC
//!     and MD is considered sufficient without further investigation." No rational analysis
//!     is made here, so `MD = MMO + 0.07`; the 0.05 floor ([`RATIONAL_ANALYSIS_MACH_MARGIN`])
//!     is not reachable without one. Where the speed is limited by VMO, AMC 25.335(b)(2)
//!     4.a(1) prescribes a head-on horizontal gust of 15.2 m/s EAS up to 6,096 m, reduced
//!     linearly to 7.6 m/s EAS at 15,240 m and constant above; its speed increment is taken as
//!     the gust velocity ([`atmospheric_margin_eas_m_s`]). The jet-stream and wind-shear
//!     encounters of 4.a(2) need a trajectory simulation with pilot recovery and are not
//!     evaluated; at transport speeds the upset margin exceeds the gust margin by a factor of
//!     about two.
//!
//! # The upset kinematics
//!
//! "Conservative aerodynamic data": the drag growth above the level-flight drag at VC is not
//! credited during the 20 s (the thrust held at the VC power balances the VC drag, and the
//! extra drag of the higher speed would only slow the aeroplane), and neither the power
//! reduction nor drag devices are credited in the pull-up. The path is then a point mass under
//! gravity alone along the path, with closed forms:
//!
//! * upset, `t = 20 s` at `gamma0 = 7.5 deg`: `V1 = V0 + g t sin(gamma0)` (true airspeed),
//!   height lost `(V0 t + g t^2 sin(gamma0) / 2) sin(gamma0)`;
//! * pull-up at load factor `n = 1.5`: `dV/dgamma = -V sin(gamma) / (n - cos(gamma))`, so
//!   `V_end = V1 (n - cos(gamma0)) / (n - 1)` when the path is level again (the speed peak),
//!   and by energy the height lost is `V1^2 / (2 g) ((n - cos(gamma0))^2 / (n - 1)^2 - 1)`.
//!
//! The peak is an equivalent airspeed at the recovery altitude. A start so low that the
//! recovery would end below sea level is not a manoeuvre that can be flown: the lowest start
//! is the one whose recovery ends at sea level ([`sea_level_recovery_start_m`]). VD is the
//! largest `VC + margin` over the altitudes where VMO, not MMO, limits VC.
//!
//! Crediting a representative drag (drag-to-weight 0.08 to 0.12 at VC, parasite-dominated,
//! idle thrust in the pull-up) lowers the A320 sea-level margin from 67 kt to 55-58 kt EAS;
//! that needs the aeroplane's drag polar, which the configuration does not carry.
//!
//! # Where VC is needed from VD alone
//!
//! The V-n diagram, the gust envelope, the cruise ceiling of a profile without a VMO and the
//! configuration checks carry only `requirements.dive_speed_m_s`. They take VC as the inverse
//! of the same rule at the sea-level recovery, [`design_cruise_speed_eas_m_s`]: exact for a
//! preset whose VMO is one value; for a tabulated VMO governed above sea level it is the VC
//! whose sea-level margin gives the same VD (A220-300 322 kt against its 326 kt EAS at
//! 10,000 ft).
//!
//! Published VD/MD values (type certificate data sheets, weight and balance manuals) are
//! never inputs; they are checks in the tests.

use crate::MissionProfileConfig;

/// Flight-path angle of the upset below the initial path, deg (CS 25.335(b)(1)(i)).
pub const UPSET_PATH_ANGLE_DEG: f64 = 7.5;
/// Duration of the upset before the pull-up, s (CS 25.335(b)(1)(i)).
pub const UPSET_DURATION_S: f64 = 20.0;
/// Load factor of the pull-up, g (CS 25.335(b)(1)(i)).
pub const PULL_UP_LOAD_FACTOR: f64 = 1.5;
/// Flight-path angle of the (b)(1)(ii)(B) upset below the initial path, deg.
pub const STEEP_UPSET_PATH_ANGLE_DEG: f64 = 15.0;
/// Delay of the (b)(1)(ii)(B) recovery after the alert, s.
pub const ALERT_RECOVERY_DELAY_S: f64 = 3.0;
/// Speed above VMO at which the CS 25.1303(c)(1) speed warning operates at the latest, m/s
/// calibrated (6 kt).
pub const OVERSPEED_WARNING_MARGIN_CAS_M_S: f64 = 6.0 * 1_852.0 / 3_600.0;
/// Largest VC/VD of the ratio branch of CS 25.335(b).
pub const CRUISE_TO_DIVE_SPEED_RATIO: f64 = 0.8;
/// MD - MC at altitudes where MC is limited by compressibility, without a rational analysis
/// (CS 25.335(b)(2); AMC 25.335(b)(2) 4.b).
pub const COMPRESSIBILITY_MACH_MARGIN: f64 = 0.07;
/// Least MD - MC a rational analysis may establish (CS 25.335(b)(2)). Not used: no rational
/// analysis is made.
pub const RATIONAL_ANALYSIS_MACH_MARGIN: f64 = 0.05;

/// Horizontal gust of AMC 25.335(b)(2) 4.a(1), m/s EAS, up to [`GUST_TAPER_START_M`].
const GUST_LOW_EAS_M_S: f64 = 15.2;
/// The same gust at and above [`GUST_TAPER_END_M`], m/s EAS.
const GUST_HIGH_EAS_M_S: f64 = 7.6;
/// 20,000 ft, m.
const GUST_TAPER_START_M: f64 = 6_096.0;
/// 50,000 ft, m.
const GUST_TAPER_END_M: f64 = 15_240.0;
/// Altitude step of the search over the VMO-limited band, m.
const ALTITUDE_STEP_M: f64 = 25.0;
/// Steps of the search: 600 x 25 m = 15,000 m.
const ALTITUDE_STEPS: u32 = 600;
/// Fixed-point passes for the sea-level recovery start (the height lost depends on the start
/// only through the density ratio; four passes settle it to below a millimetre).
const START_PASSES: usize = 8;
/// Bisection passes of the inverse on VC.
const INVERSE_PASSES: usize = 60;

/// ISA sea-level density, kg/m^3: the definition of equivalent airspeed.
const SEA_LEVEL_DENSITY_KG_M3: f64 = alas_atmo::SEA_LEVEL_DENSITY_ICAO_KG_M3;

/// Which speed-margin manoeuvres of CS 25.335(b)(1) apply: a property of the certification
/// basis, never of the aeroplane's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpsetCriterion {
    /// (b)(1)(i): no high speed protection function in the certification basis.
    #[default]
    Unprotected,
    /// (b)(1)(ii) (or the special condition it codifies): the basis carries a high speed
    /// protection function.
    HighSpeedProtection,
}

impl UpsetCriterion {
    /// The criterion of a configuration that declares (`true`) or not a high speed protection
    /// function.
    #[must_use]
    pub fn from_protection(high_speed_protection: bool) -> Self {
        if high_speed_protection {
            Self::HighSpeedProtection
        } else {
            Self::Unprotected
        }
    }
}

/// The end of the CS 25.335(b)(1)(i) manoeuvre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpsetRecovery {
    /// Highest equivalent airspeed of the manoeuvre (at the end of the pull-up), m/s.
    pub peak_eas_m_s: f64,
    /// Pressure altitude where the path is level again, m (below zero for a start lower than
    /// [`sea_level_recovery_start_m`]; its density is then taken at sea level).
    pub recovery_altitude_m: f64,
}

/// The design speeds the operating limits of a profile imply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignDiveSpeed {
    /// VD, m/s EAS.
    pub dive_eas_m_s: f64,
    /// The largest VC (VMO in EAS) of the VMO-limited band, m/s.
    pub max_cruise_eas_m_s: f64,
    /// Pressure altitude of the start that governs VD, m.
    pub governing_altitude_m: f64,
}

fn density_kg_m3(altitude_m: f64) -> Option<f64> {
    alas_atmo::us1976_try_compute_values(altitude_m.max(0.0), 0.0)
        .ok()
        .map(|values| values.density_kg_m3)
        .filter(|rho| rho.is_finite() && *rho > 0.0)
}

/// `(n - cos(gamma0)) / (n - 1)`: true airspeed at the end of the pull-up over true airspeed
/// at its start.
#[must_use]
pub fn pull_up_speed_ratio() -> f64 {
    (PULL_UP_LOAD_FACTOR - UPSET_PATH_ANGLE_DEG.to_radians().cos()) / (PULL_UP_LOAD_FACTOR - 1.0)
}

/// The CS 25.335(b)(1)(i) manoeuvre from stabilised flight at `cruise_eas_m_s` (VC, m/s EAS)
/// at pressure altitude `start_altitude_m`, with the conservative kinematics of the module
/// documentation. `None` for a speed that is not finite and positive or an altitude outside
/// the standard atmosphere.
#[must_use]
pub fn upset_recovery(cruise_eas_m_s: f64, start_altitude_m: f64) -> Option<UpsetRecovery> {
    held_path_recovery(
        cruise_eas_m_s,
        start_altitude_m,
        UPSET_PATH_ANGLE_DEG,
        |_, _| Some(UPSET_DURATION_S),
    )
}

/// A path held `path_deg` below level from VC `cruise_eas_m_s` (m/s EAS) at `start_altitude_m`
/// for the time `hold_s(start true airspeed m/s, sqrt(rho0 / rho) of the start)` returns, then
/// the pull-up at [`PULL_UP_LOAD_FACTOR`] to level flight, with the conservative kinematics of
/// the module documentation.
fn held_path_recovery(
    cruise_eas_m_s: f64,
    start_altitude_m: f64,
    path_deg: f64,
    hold_s: impl Fn(f64, f64) -> Option<f64>,
) -> Option<UpsetRecovery> {
    if !(cruise_eas_m_s.is_finite() && cruise_eas_m_s > 0.0 && start_altitude_m.is_finite()) {
        return None;
    }
    let g = alas_atmo::STANDARD_GRAVITY;
    let path_rad = path_deg.to_radians();
    let sin_path = path_rad.sin();
    let true_per_eas = (SEA_LEVEL_DENSITY_KG_M3 / density_kg_m3(start_altitude_m)?).sqrt();
    let start_true_m_s = cruise_eas_m_s * true_per_eas;
    let duration_s = hold_s(start_true_m_s, true_per_eas)?.max(0.0);
    let gain_m_s = g * duration_s * sin_path;
    let pull_up_start_m_s = start_true_m_s + gain_m_s;
    let upset_drop_m = (start_true_m_s * duration_s + 0.5 * gain_m_s * duration_s) * sin_path;
    let ratio = (PULL_UP_LOAD_FACTOR - path_rad.cos()) / (PULL_UP_LOAD_FACTOR - 1.0);
    let pull_up_drop_m = pull_up_start_m_s * pull_up_start_m_s / (2.0 * g) * (ratio * ratio - 1.0);
    let recovery_altitude_m = start_altitude_m - upset_drop_m - pull_up_drop_m;
    let peak_true_m_s = pull_up_start_m_s * ratio;
    let peak_eas_m_s =
        peak_true_m_s * (density_kg_m3(recovery_altitude_m)? / SEA_LEVEL_DENSITY_KG_M3).sqrt();
    Some(UpsetRecovery {
        peak_eas_m_s,
        recovery_altitude_m,
    })
}

/// Seconds to accelerate along `path_deg` from true airspeed `start_true_m_s` to the warning
/// `warning_eas_m_s` (m/s EAS) taken at the start density (`true_per_eas`): the true-airspeed
/// increment is largest there, so the warning comes no earlier than in the descending flight.
fn time_to_warning_s(
    start_true_m_s: f64,
    warning_eas_m_s: f64,
    true_per_eas: f64,
    path_deg: f64,
) -> Option<f64> {
    let rate = alas_atmo::STANDARD_GRAVITY * path_deg.to_radians().sin();
    let time_s = (warning_eas_m_s * true_per_eas - start_true_m_s) / rate;
    time_s.is_finite().then_some(time_s.max(0.0))
}

/// CS 25.335(b)(1)(ii)(B) from VC `cruise_eas_m_s` (m/s EAS) at `start_altitude_m`: the path is
/// 15 deg below level through VC, the speed warning operates at `warning_eas_m_s` (m/s EAS),
/// recovery at 1.5 g begins [`ALERT_RECOVERY_DELAY_S`] later. Neither drag growth, power
/// reduction, drag devices nor the system's own pitch-up is credited.
#[must_use]
pub fn steep_upset_recovery(
    cruise_eas_m_s: f64,
    warning_eas_m_s: f64,
    start_altitude_m: f64,
) -> Option<UpsetRecovery> {
    held_path_recovery(
        cruise_eas_m_s,
        start_altitude_m,
        STEEP_UPSET_PATH_ANGLE_DEG,
        |v0, root| {
            Some(
                time_to_warning_s(v0, warning_eas_m_s, root, STEEP_UPSET_PATH_ANGLE_DEG)?
                    + ALERT_RECOVERY_DELAY_S,
            )
        },
    )
}

/// CS 25.335(b)(1)(ii)(A) from VC `cruise_eas_m_s` (m/s EAS) at `start_altitude_m`, with the
/// recovery taken to begin when the speed reaches the warning `warning_eas_m_s` (m/s EAS), and
/// no later than [`UPSET_DURATION_S`]: the assumption of the module documentation.
#[must_use]
pub fn protected_upset_recovery(
    cruise_eas_m_s: f64,
    warning_eas_m_s: f64,
    start_altitude_m: f64,
) -> Option<UpsetRecovery> {
    held_path_recovery(
        cruise_eas_m_s,
        start_altitude_m,
        UPSET_PATH_ANGLE_DEG,
        |v0, root| {
            Some(
                time_to_warning_s(v0, warning_eas_m_s, root, UPSET_PATH_ANGLE_DEG)?
                    .min(UPSET_DURATION_S),
            )
        },
    )
}

/// The lowest start, m, from which `manoeuvre(start)` recovers at or above sea level.
fn lowest_start_m(manoeuvre: impl Fn(f64) -> Option<UpsetRecovery>) -> Option<f64> {
    let mut start_m = 0.0_f64;
    for _ in 0..START_PASSES {
        start_m = (start_m - manoeuvre(start_m)?.recovery_altitude_m).max(0.0);
    }
    Some(start_m)
}

/// Largest EAS gain, m/s, of `manoeuvre(start)` from a start at `altitude_m` or, when that is
/// lower, at the lowest start that recovers at sea level.
fn manoeuvre_margin_eas_m_s(
    cruise_eas_m_s: f64,
    altitude_m: f64,
    manoeuvre: impl Fn(f64) -> Option<UpsetRecovery>,
) -> Option<f64> {
    let start_m = altitude_m.max(lowest_start_m(&manoeuvre)?);
    Some(manoeuvre(start_m)?.peak_eas_m_s - cruise_eas_m_s)
}

/// The lowest start, m, from which the manoeuvre at `cruise_eas_m_s` (m/s EAS) recovers at or
/// above sea level: the start whose recovery ends at sea level, 0 if none is lower.
#[must_use]
pub fn sea_level_recovery_start_m(cruise_eas_m_s: f64) -> Option<f64> {
    lowest_start_m(|start_m| upset_recovery(cruise_eas_m_s, start_m))
}

/// Atmospheric margin of CS 25.335(b)(2) where VMO limits VC, m/s EAS: the horizontal gust
/// velocity of AMC 25.335(b)(2) 4.a(1) at pressure altitude `altitude_m`.
#[must_use]
pub fn atmospheric_margin_eas_m_s(altitude_m: f64) -> f64 {
    if altitude_m <= GUST_TAPER_START_M {
        GUST_LOW_EAS_M_S
    } else if altitude_m >= GUST_TAPER_END_M {
        GUST_HIGH_EAS_M_S
    } else {
        GUST_LOW_EAS_M_S
            + (GUST_HIGH_EAS_M_S - GUST_LOW_EAS_M_S) * (altitude_m - GUST_TAPER_START_M)
                / (GUST_TAPER_END_M - GUST_TAPER_START_M)
    }
}

/// `VC + max(margin (b)(1), margin (b)(2))`, m/s EAS, for VC `cruise_eas_m_s` (m/s EAS) at
/// pressure altitude `altitude_m`; a start below [`sea_level_recovery_start_m`] is flown from
/// there. The margin branch of CS 25.335(b) only, without the 0.8 alternative.
#[must_use]
pub fn margin_dive_speed_eas_m_s(cruise_eas_m_s: f64, altitude_m: f64) -> Option<f64> {
    let start_m = altitude_m.max(sea_level_recovery_start_m(cruise_eas_m_s)?);
    let upset = upset_recovery(cruise_eas_m_s, start_m)?.peak_eas_m_s - cruise_eas_m_s;
    Some(cruise_eas_m_s + upset.max(atmospheric_margin_eas_m_s(altitude_m)))
}

/// `VC + max(margin (b)(1)(ii)(A), margin (b)(1)(ii)(B), margin (b)(2))`, m/s EAS, for VC
/// `cruise_eas_m_s` and speed warning `warning_eas_m_s` (both m/s EAS) at pressure altitude
/// `altitude_m`; each manoeuvre is flown from `altitude_m` or its lowest start that recovers at
/// sea level, whichever is higher. The margin branch only.
#[must_use]
pub fn protected_margin_dive_speed_eas_m_s(
    cruise_eas_m_s: f64,
    warning_eas_m_s: f64,
    altitude_m: f64,
) -> Option<f64> {
    let hold = manoeuvre_margin_eas_m_s(cruise_eas_m_s, altitude_m, |start_m| {
        protected_upset_recovery(cruise_eas_m_s, warning_eas_m_s, start_m)
    })?;
    let steep = manoeuvre_margin_eas_m_s(cruise_eas_m_s, altitude_m, |start_m| {
        steep_upset_recovery(cruise_eas_m_s, warning_eas_m_s, start_m)
    })?;
    Some(cruise_eas_m_s + hold.max(steep).max(atmospheric_margin_eas_m_s(altitude_m)))
}

/// The margin branch of `criterion` for VC `cruise_eas_m_s` and warning `warning_eas_m_s` (m/s
/// EAS; read by the protected criterion only) at `altitude_m`.
#[must_use]
pub fn margin_dive_speed_for(
    criterion: UpsetCriterion,
    cruise_eas_m_s: f64,
    warning_eas_m_s: f64,
    altitude_m: f64,
) -> Option<f64> {
    match criterion {
        UpsetCriterion::Unprotected => margin_dive_speed_eas_m_s(cruise_eas_m_s, altitude_m),
        UpsetCriterion::HighSpeedProtection => {
            protected_margin_dive_speed_eas_m_s(cruise_eas_m_s, warning_eas_m_s, altitude_m)
        }
    }
}

/// Equivalent airspeed of calibrated airspeed `cas_m_s` at pressure altitude `altitude_m`,
/// m/s; `None` where it is supersonic or the altitude is outside the atmosphere.
fn equivalent_from_calibrated(cas_m_s: f64, altitude_m: f64) -> Option<f64> {
    let ambient = alas_atmo::us1976_try_compute_values(altitude_m, 0.0).ok()?;
    let mach = alas_atmo::airspeed::mach_from_calibrated(cas_m_s, ambient.pressure_pa).ok()?;
    let true_m_s = alas_atmo::airspeed::true_from_mach(mach, ambient.temperature_k).ok()?;
    alas_atmo::airspeed::equivalent_from_true(true_m_s, ambient.pressure_pa, ambient.temperature_k)
        .ok()
}

/// VD and MD from the VMO and MMO a mission profile declares, with VC = VMO and MC = MMO
/// (CS 25.1505) and VD the least value CS 25.335(b) admits: the smaller of the ratio branch
/// (`1.25` times the largest VC) and the margin branch (the largest `VC + margin` over the
/// pressure altitudes, 0 to 15,000 m in 25 m steps, where VMO limits VC). `None` when the
/// profile declares no VMO.
#[must_use]
pub fn dive_speed_from_operating_limits(profile: &MissionProfileConfig) -> Option<DesignDiveSpeed> {
    dive_speed_from_operating_limits_for(profile, UpsetCriterion::Unprotected)
}

/// [`dive_speed_from_operating_limits`] under `criterion`; the protected criterion takes the
/// speed warning at VMO + [`OVERSPEED_WARNING_MARGIN_CAS_M_S`] (calibrated) at each altitude.
#[must_use]
pub fn dive_speed_from_operating_limits_for(
    profile: &MissionProfileConfig,
    criterion: UpsetCriterion,
) -> Option<DesignDiveSpeed> {
    let declares_vmo = profile.max_operating_cas_m_s > 0.0 || !profile.vmo_schedule.is_empty();
    if !declares_vmo {
        return None;
    }
    let mut margin_dive_eas_m_s = 0.0_f64;
    let mut governing_altitude_m = 0.0;
    let mut max_cruise_eas_m_s = 0.0_f64;
    for step in 0..=ALTITUDE_STEPS {
        let altitude_m = f64::from(step) * ALTITUDE_STEP_M;
        let vmo_cas_m_s = profile.vmo_cas_m_s_at_altitude(altitude_m);
        if !(vmo_cas_m_s.is_finite() && vmo_cas_m_s > 0.0) {
            break;
        }
        let Some(ambient) = alas_atmo::us1976_try_compute_values(altitude_m, 0.0).ok() else {
            break;
        };
        // Above the VMO/MMO crossover MMO limits VC (and VMO may be supersonic there).
        let mmo = profile.mmo_at_altitude(altitude_m);
        let vmo_mach = alas_atmo::airspeed::mach_from_calibrated(vmo_cas_m_s, ambient.pressure_pa);
        match vmo_mach {
            Ok(mach) if mmo <= 0.0 || mach <= mmo => {}
            _ => break,
        }
        let Some(cruise_eas_m_s) = equivalent_from_calibrated(vmo_cas_m_s, altitude_m) else {
            break;
        };
        max_cruise_eas_m_s = max_cruise_eas_m_s.max(cruise_eas_m_s);
        let warning_eas_m_s =
            equivalent_from_calibrated(vmo_cas_m_s + OVERSPEED_WARNING_MARGIN_CAS_M_S, altitude_m)
                .unwrap_or(f64::NAN);
        if let Some(dive_eas_m_s) =
            margin_dive_speed_for(criterion, cruise_eas_m_s, warning_eas_m_s, altitude_m)
        {
            if dive_eas_m_s > margin_dive_eas_m_s {
                margin_dive_eas_m_s = dive_eas_m_s;
                governing_altitude_m = altitude_m;
            }
        }
    }
    if !(margin_dive_eas_m_s > 0.0 && max_cruise_eas_m_s > 0.0) {
        return None;
    }
    let ratio_dive_eas_m_s = max_cruise_eas_m_s / CRUISE_TO_DIVE_SPEED_RATIO;
    Some(DesignDiveSpeed {
        dive_eas_m_s: margin_dive_eas_m_s.min(ratio_dive_eas_m_s),
        max_cruise_eas_m_s,
        governing_altitude_m,
    })
}

/// MC, the largest MMO the profile declares (its constant MMO or the largest row of its MMO
/// schedule): the least MC CS 25.1505 admits. `None` where the profile declares no MMO.
///
/// MD is then [`dive_mach`] of it, the (b)(2) margin alone. The (b)(1)(i) upset from MC is not
/// evaluated: its speed gain above the drag-divergence Mach needs a compressible drag-rise
/// polar, which this crate does not carry, and without drag credit the closed forms above give
/// about 0.1 M, which is not a transonic result. The (b)(1) margin at MC is therefore taken to
/// be below 0.07 M, as the certified pairs on file show (A320 MMO 0.82, MD 0.89).
#[must_use]
pub fn declared_max_operating_mach(profile: &MissionProfileConfig) -> Option<f64> {
    let max_mmo = profile
        .mmo_schedule
        .iter()
        .map(|row| row.1)
        .fold(profile.max_operating_mach, f64::max);
    (max_mmo.is_finite() && max_mmo > 0.0).then_some(max_mmo)
}

/// MD for MC `cruise_mach`: `MC + 0.07` (CS 25.335(b)(2), AMC 25.335(b)(2) 4.b).
#[must_use]
pub fn dive_mach(cruise_mach: f64) -> f64 {
    cruise_mach + COMPRESSIBILITY_MACH_MARGIN
}

/// VC, m/s EAS, of design dive speed `dive_eas_m_s` (m/s EAS): the inverse of the rule at the
/// sea-level recovery, the larger of `0.8 VD` (the ratio branch) and the VC whose margin
/// branch gives VD. Returns `dive_eas_m_s` unchanged when it is not finite and positive, so a
/// caller's own guard still sees the invalid value.
#[must_use]
pub fn design_cruise_speed_eas_m_s(dive_eas_m_s: f64) -> f64 {
    design_cruise_speed_eas_m_s_for(dive_eas_m_s, UpsetCriterion::Unprotected)
}

/// [`design_cruise_speed_eas_m_s`] under `criterion` (at sea level the speed warning is VC + 6 kt,
/// calibrated and equivalent airspeeds being equal there).
#[must_use]
pub fn design_cruise_speed_eas_m_s_for(dive_eas_m_s: f64, criterion: UpsetCriterion) -> f64 {
    if !(dive_eas_m_s.is_finite() && dive_eas_m_s > 0.0) {
        return dive_eas_m_s;
    }
    // The mission evaluates its cruise ceiling once per rung with one VD: keep the last answer.
    thread_local! {
        static LAST: std::cell::Cell<(u64, UpsetCriterion, f64)> =
            const { std::cell::Cell::new((0, UpsetCriterion::Unprotected, 0.0)) };
    }
    let key = dive_eas_m_s.to_bits();
    if let Some(hit) = LAST.with(|last| {
        let (bits, kind, value) = last.get();
        (bits == key && kind == criterion).then_some(value)
    }) {
        return hit;
    }
    let cruise_eas_m_s = invert_dive_speed(dive_eas_m_s, criterion);
    LAST.with(|last| last.set((key, criterion, cruise_eas_m_s)));
    cruise_eas_m_s
}

fn invert_dive_speed(dive_eas_m_s: f64, criterion: UpsetCriterion) -> f64 {
    let ratio_cruise_m_s = CRUISE_TO_DIVE_SPEED_RATIO * dive_eas_m_s;
    let (mut low, mut high) = (0.5 * dive_eas_m_s, dive_eas_m_s);
    for _ in 0..INVERSE_PASSES {
        let middle = 0.5 * (low + high);
        let warning = middle + OVERSPEED_WARNING_MARGIN_CAS_M_S;
        match margin_dive_speed_for(criterion, middle, warning, 0.0) {
            Some(dive) if dive > dive_eas_m_s => high = middle,
            Some(_) => low = middle,
            None => return ratio_cruise_m_s,
        }
    }
    ratio_cruise_m_s.max(0.5 * (low + high))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const KT_M_S: f64 = 1_852.0 / 3_600.0;

    /// Hand values: `g t sin(7.5 deg) = 9.80665 x 20 x 0.130526 = 25.600494 m/s`,
    /// `(1.5 - cos(7.5 deg)) / 0.5 = 1.017110`; from sea level at 350 kt EAS
    /// (180.055556 m/s, true there) the peak is `(180.055556 + 25.600494) x 1.017110 =
    /// 209.1749 m/s` (density held at sea level), recovered 577.88 m below the start.
    #[test]
    fn the_upset_kinematics_match_the_closed_forms() {
        assert!((pull_up_speed_ratio() - 1.017_110_277).abs() < 1.0e-8);
        let recovery = upset_recovery(350.0 * KT_M_S, 0.0).unwrap();
        assert!(
            (recovery.peak_eas_m_s - 209.174_9).abs() < 1.0e-3,
            "{recovery:?}"
        );
        assert!(
            (recovery.recovery_altitude_m + 577.88).abs() < 0.01,
            "{recovery:?}"
        );
        // At 8,000 m, 300 kt EAS: 181.95 m/s EAS at 7,231 m (independent ISA evaluation).
        let high = upset_recovery(300.0 * KT_M_S, 8_000.0).unwrap();
        assert!((high.peak_eas_m_s - 181.954).abs() < 0.05, "{high:?}");
        assert!((high.recovery_altitude_m - 7_231.1).abs() < 1.0, "{high:?}");
    }

    /// The lowest flyable start at 350 kt EAS recovers at sea level (595.5 m) and gives the
    /// largest EAS gain: 214.52 m/s, 67.0 kt above VC.
    #[test]
    fn the_lowest_flyable_start_recovers_at_sea_level() {
        let vc = 350.0 * KT_M_S;
        let start_m = sea_level_recovery_start_m(vc).unwrap();
        assert!((start_m - 595.46).abs() < 1.0, "{start_m}");
        let recovery = upset_recovery(vc, start_m).unwrap();
        assert!(recovery.recovery_altitude_m.abs() < 0.01, "{recovery:?}");
        assert!(
            (recovery.peak_eas_m_s - 214.521).abs() < 0.05,
            "{recovery:?}"
        );
        let vd = margin_dive_speed_eas_m_s(vc, 0.0).unwrap();
        assert!((vd - recovery.peak_eas_m_s).abs() < 1.0e-9);
        // A higher start at the same VC gives less (214.12 m/s 500 m higher, 212.79 m/s at
        // 3,000 m).
        assert!(upset_recovery(vc, start_m + 500.0).unwrap().peak_eas_m_s < vd);
        assert!((margin_dive_speed_eas_m_s(vc, 3_000.0).unwrap() - 212.787).abs() < 0.05);
    }

    #[test]
    fn the_atmospheric_gust_tapers_between_20000_and_50000_ft() {
        assert_eq!(atmospheric_margin_eas_m_s(0.0), 15.2);
        assert_eq!(atmospheric_margin_eas_m_s(6_096.0), 15.2);
        assert!((atmospheric_margin_eas_m_s(10_668.0) - 11.4).abs() < 1.0e-12);
        assert_eq!(atmospheric_margin_eas_m_s(20_000.0), 7.6);
        assert!((dive_mach(0.82) - 0.89).abs() < 1.0e-12);
    }

    /// One VMO from sea level: the inverse returns VMO; a low VD is governed by the ratio
    /// branch (the margin branch would give a larger VD), so VC is `0.8 VD`.
    #[test]
    fn the_inverse_recovers_vc_and_falls_back_to_the_ratio_branch() {
        let vc = 350.0 * KT_M_S;
        let vd = margin_dive_speed_eas_m_s(vc, 0.0).unwrap();
        assert!((design_cruise_speed_eas_m_s(vd) - vc).abs() < 1.0e-6);
        assert!((design_cruise_speed_eas_m_s(130.0) - 104.0).abs() < 1.0e-12);
        assert!((design_cruise_speed_eas_m_s(175.0) - 143.186).abs() < 0.02);
        assert_eq!(design_cruise_speed_eas_m_s(0.0), 0.0);
    }

    /// Hand values of (b)(1)(ii)(B) from sea level (density held there) at VC 350 kt EAS,
    /// warning 356 kt: `g sin(15 deg) = 2.538088 m/s^2`, warning after `3.086667 / 2.538088 =
    /// 1.216110 s`, recovery at 4.216110 s at `180.055556 + 2.538088 x 4.216110 = 190.756666
    /// m/s`, `(1.5 - cos(15 deg)) / 0.5 = 1.068148`: peak 203.756417 m/s, 463.80 m lower. The
    /// lowest start that recovers at sea level (481.8 m) gives 208.3588 m/s, 55.0 kt above VC;
    /// (A) with recovery at the warning gives 14.1 kt; (i) gives 67.0 kt.
    #[test]
    fn the_protected_manoeuvres_match_the_closed_forms() {
        let vc = 350.0 * KT_M_S;
        let warning = vc + OVERSPEED_WARNING_MARGIN_CAS_M_S;
        let steep = steep_upset_recovery(vc, warning, 0.0).unwrap();
        assert!(
            (steep.peak_eas_m_s - 203.756_417).abs() < 1.0e-4,
            "{steep:?}"
        );
        assert!(
            (steep.recovery_altitude_m + 463.80).abs() < 0.01,
            "{steep:?}"
        );
        let vd = protected_margin_dive_speed_eas_m_s(vc, warning, 0.0).unwrap();
        assert!((vd - 208.358_8).abs() < 1.0e-3, "{vd}");
        let hold = manoeuvre_margin_eas_m_s(vc, 0.0, |start| {
            protected_upset_recovery(vc, warning, start)
        })
        .unwrap();
        assert!((hold / KT_M_S - 14.14).abs() < 0.01, "{hold}");
        assert!(vd < margin_dive_speed_eas_m_s(vc, 0.0).unwrap());
        let inverse = design_cruise_speed_eas_m_s_for(vd, UpsetCriterion::HighSpeedProtection);
        assert!((inverse - vc).abs() < 1.0e-6, "{inverse}");
    }

    /// The A320-200 preset (EASA TCDS EASA.A.064: VMO 350 kt, MMO 0.82; high speed protection
    /// in its certification basis, SC A-2.2.3) dives at 405.0 kt EAS ((b)(1)(ii)(B) governs)
    /// and MD 0.89. Check, not fit: the secondary-source VD/MD 381 kt / 0.89 is not yet tied to
    /// a primary document; VD lies 0 to 10 % above it and MD equals it. The excess is not
    /// attributed: no drag growth, power reduction or automatic pitch-up is credited.
    #[test]
    fn the_a320_rule_lands_at_or_above_the_published_vd_and_on_the_published_md() {
        let preset = crate::presets::get("A320-200").unwrap();
        assert!(preset.requirements.high_speed_protection);
        let vd = preset.requirements.dive_speed_m_s;
        assert!((vd - 208.359).abs() < 0.1, "{vd}");
        let excess = vd / (381.0 * KT_M_S) - 1.0;
        assert!((0.0..0.10).contains(&excess), "{excess}");
        let profile = preset.operational_mission_defaults().profile;
        let speeds =
            dive_speed_from_operating_limits_for(&profile, UpsetCriterion::HighSpeedProtection)
                .unwrap();
        assert_eq!(speeds.dive_eas_m_s.to_bits(), vd.to_bits());
        let mc = declared_max_operating_mach(&profile).unwrap();
        assert_eq!(preset.requirements.max_operating_mach, Some(mc));
        assert!((dive_mach(mc) - 0.89).abs() < 1.0e-12);
        assert!((preset.requirements.design_dive_mach() - 0.89).abs() < 1.0e-12);
        let vc = preset.requirements.design_cruise_speed_eas_m_s();
        assert!((vc - 350.0 * KT_M_S).abs() < 0.05, "{vc}");
    }

    /// Every preset whose profile declares a VMO dives at the rule of its certification basis;
    /// one without keeps its declared VD. Expected values from the independent Python mirrors
    /// (research notes vd-cs25335 `rule_final.py`, vd-followups/protected `protected_rule.py`).
    /// A340-300 checks against its FAA TCDS A43NM VD 365 kt: the rule gives 382.4 kt (+4.8 %).
    #[test]
    fn presets_with_a_vmo_take_the_rule_and_the_rest_keep_their_declared_vd() {
        for (name, vd_m_s) in [
            ("A320-200", 208.359),
            ("A220-300", 194.423),
            ("C295", 159.298),
            ("LJ60", 205.553),
            ("B747-400", 228.439),
            ("B787-9", 208.359),
            ("A340-300", 196.728),
            ("E195-E2", 188.754),
            ("ATR72-600", 159.523),
            ("DC-10", 224.046),
            ("A400M", 186.892),
            ("AVE", 227.301),
            ("A350-900", 202.535),
            ("A380-800", 202.535),
        ] {
            let dive = crate::presets::get(name)
                .unwrap()
                .requirements
                .dive_speed_m_s;
            assert!((dive - vd_m_s).abs() < 0.1, "{name}: {dive}");
        }
        // Hand-set: no C919 VMO is published at any rank.
        let dive = crate::presets::get("C919")
            .unwrap()
            .requirements
            .dive_speed_m_s;
        assert_eq!(dive, 180.0, "C919");
    }
}
