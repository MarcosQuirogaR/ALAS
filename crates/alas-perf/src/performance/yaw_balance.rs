// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One-engine-inoperative yaw balance: the trim state, the minimum control
//! speed and the crosswind rudder demand.
//!
//! Sources (read from the owner-supplied handbooks, SOURCES-R2 Sec. 4):
//!
//! - Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), Sec. 9.6.1,
//!   Eqs. (9-52) to (9-60): the thrust asymmetry `N_e = dT_e y_e`, the side
//!   force and yawing moment of the airframe without fin, the weight
//!   component `W sin(phi)` of at most 5 deg of bank, and the fin
//!   `Y_v = q S_v eta_v C_Ya,v (tau_v delta_r - (beta - sigma_v))` acting at the
//!   arm `l_v`; Eq. (9-59) is the moment balance, Fig. 9-22 the relief of the
//!   required fin by sideslip; Eq. (9-64) and (9-65) the fuselage `Cn_beta`;
//!   Sec. 9.6.3 Eq. (9-68) the crosswind check; Sec. 9.6.1(b) a rudder
//!   deflection limit of 25 to 30 deg.
//! - Roskam, *Airplane Design Part VII*, Sec. 2.3, Eqs. (2.10) to (2.12): the
//!   same force and moment equations with the aileron and roll balance, and
//!   the acceptability limits (bank at most 5 deg, `|beta|` below 12 deg
//!   before fin stall).
//! - CS 25.149 (14 CFR 25.149): `VMC` is the speed at which control is held
//!   after the critical engine fails, in straight flight with at most 5 deg of
//!   bank; `VMC` may not exceed 1.13 `VSR`.
//!
//! # Conventions
//!
//! SI units, angles in radians. Sideslip `beta` is positive when the relative
//! wind comes from the live-engine side (the nose is displaced toward the
//! failed engine); the fin then loads in the same sense as a rudder pushing
//! the tail toward the failed side. The rudder angle is positive in that
//! sense. The bank is toward the live engine, so its weight component
//! balances the aerodynamic side force that points toward the failed side.
//!
//! # Model and its limits
//!
//! Quasi-steady moment and side-force balance with linear fin aerodynamics.
//! The roll balance and the aileron (Roskam Eq. 2.11), the side force and
//! induced drag of the fuselage and nacelles in sideslip, the propeller
//! slipstream over the fin, the pedal force (hinge moment) and the
//! transient overshoot are not modelled. The rudder effectiveness is
//! interpolated from the published curve points of Sadraey Fig. 12.12 (see
//! `rudder::RUDDER_TAU_POINTS`), not measured on the fin. The fin effective aspect
//! ratio is the geometric one unless the caller supplies a larger value.

use std::f64::consts::PI;

use super::failed_engine_drag::YawBalanceGeometry;

mod rudder;
pub use rudder::rudder_effectiveness;

/// CS 25.149(b): the maximum angle of bank toward the live engine, rad.
pub const BANK_LIMIT_RAD: f64 = 5.0 * PI / 180.0;
/// Limit on the sideslip magnitude of the trim state, rad (5 deg): Roskam VII
/// Sec. 2.3 acceptability limit for drag (the fin-stall limit is 12 deg) and
/// the range of Torenbeek Fig. 9-22.
pub const SIDESLIP_LIMIT_RAD: f64 = 5.0 * PI / 180.0;
/// CS 25.149(c): `VMC` may not exceed 1.13 `VSR`.
pub const VMC_CEILING_OVER_VSR: f64 = 1.13;
/// Fin dynamic-pressure ratio `eta_v = (V_v / V)^2`, Torenbeek Eq. (9-60).
/// Source: Sadraey, *Aircraft Design: A Systems Engineering Approach* (Wiley,
/// 2013), book p. 276: the tail efficiency of a conventional tail
/// is 0.85 to 0.95 (1.0 for a T-tail, out of the wing and fuselage wake), and
/// the worked fin examples of Chapter 12 use 0.94 to 0.96 (pp. 705, 744-745).
/// 0.95 is the upper end of the conventional range, kept for a fin partly in
/// the fuselage boundary layer; a lower value is conservative for VMC.
pub const FIN_DYNAMIC_PRESSURE_RATIO: f64 = 0.95;
/// Minimum weathercock stability `Cn_beta` of fuselage plus fin, per rad:
/// positive (Sadraey p. 282; CS 25.177). The model term omits wing and
/// nacelle, so it reads 0.02 to 0.08 per rad on the registered transports
/// (probe: A320 0.053, A340 0.024, ATR72 0.078), below the +0.1 to +0.4 of
/// complete aircraft, which is therefore not usable as a floor here.
pub const MINIMUM_DIRECTIONAL_STABILITY_PER_RAD: f64 = 0.0;
/// Normalising scale of the `Cn_beta` shortfall, per rad: the lower end of the
/// typical band, so a shortfall of this size counts as one unit of violation.
pub const DIRECTIONAL_STABILITY_SCALE_PER_RAD: f64 = 0.10;
/// Floor on the net directional stability `Cn_beta` per rad used by the
/// crosswind check: the lower end of the 0.10 to 0.25 range of transports
/// (Torenbeek Sec. 9.6.2); Sadraey (book p. 282) puts the typical
/// value of `Cn_beta` of stable aircraft at +0.1 to +0.4 per rad. The
/// airframe-without-fin term here is the fuselage alone (Eq. 9-64) and
/// neglects the wing and interference terms, so a small or negative net value
/// is an artefact of the missing terms, not a prediction.
pub const NET_DIRECTIONAL_STABILITY_FLOOR: f64 = 0.10;
/// Speed bounds of the minimum-control-speed search, m/s.
const SEARCH_SPEED_RANGE_M_S: (f64, f64) = (5.0, 300.0);

/// Fin, rudder, wing and airframe data of the yaw balance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YawControlGeometry {
    /// Engine arm, fin arm and area, effective aspect ratio, rudder area and
    /// quarter-chord sweep.
    pub balance: YawBalanceGeometry,
    /// Fin taper ratio `c_tip / c_root`.
    pub fin_taper_ratio: f64,
    /// Rudder chord over local fin chord.
    pub rudder_chord_ratio: f64,
    /// Largest rudder deflection, rad.
    pub rudder_max_deflection_rad: f64,
    /// Wing reference area, m^2.
    pub wing_area_m2: f64,
    /// Wing reference span, m.
    pub wing_span_m: f64,
    /// Yaw-moment slope of the airframe without the fin, per rad (negative
    /// when destabilizing), referred to `S b`.
    pub cn_beta_without_fin_per_rad: f64,
}

/// What the pilot (or the autopilot) holds while balancing the asymmetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YawTrimMode {
    /// Zero sideslip with the bank that balances the fin side force, when it
    /// is within the limit; otherwise the limit bank and the sideslip that
    /// completes the balance. The state of least fin side force, hence least
    /// trim drag.
    MinimumDrag,
    /// The state of least rudder demand at a bank up to the limit: the
    /// minimum-control-speed condition of CS 25.149(b).
    LeastRudder,
    /// Wings level, zero sideslip, the fin carries the whole moment (the
    /// ground-run balance without tyre side force).
    RudderOnly,
}

/// A solved engine-out trim state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YawTrim {
    /// Bank toward the live engine, rad (`NaN` when no weight was given).
    pub bank_rad: f64,
    /// Sideslip, positive with the wind from the live-engine side, rad.
    pub sideslip_rad: f64,
    /// Rudder deflection, rad (positive pushes the tail toward the failed side).
    pub rudder_rad: f64,
    /// Fin side force, N.
    pub fin_force_n: f64,
    /// Whether the bank limit, not the balance, set the bank.
    pub bank_limited: bool,
}

/// The factor that limits a minimum control speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlLimit {
    /// The rudder reaches its maximum deflection.
    Rudder,
    /// The sideslip reaches the fin-stall limit.
    Sideslip,
}

/// Minimum control speed and the trim state at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimumControlSpeed {
    /// True airspeed, m/s.
    pub tas_m_s: f64,
    /// The trim state at that speed.
    pub trim: YawTrim,
    /// What limits control there.
    pub limit: ControlLimit,
}

/// Fuselage data of the yaw-moment slope, Torenbeek Fig. 9-25.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageYawGeometry {
    /// Fuselage length, m.
    pub length_m: f64,
    /// Nose to centre of gravity, m.
    pub cg_from_nose_m: f64,
    /// Side projected area, m^2.
    pub side_area_m2: f64,
    /// Maximum height, m.
    pub max_height_m: f64,
    /// Height and width at a quarter of the length from the nose, m.
    pub height_quarter_m: f64,
    /// Width at a quarter of the length from the nose, m.
    pub width_quarter_m: f64,
    /// Height at three quarters of the length, m.
    pub height_three_quarter_m: f64,
    /// Width at three quarters of the length, m.
    pub width_three_quarter_m: f64,
}

/// Fuselage yaw-moment slope `Cn_beta,f` per rad on `S b`, Torenbeek
/// Eqs. (9-64) and (9-65): `-K_beta (S_fs l_f / (S b)) (h_1/h_2)^(1/2)
/// (b_2/b_1)^(1/3)` with `K_beta = 0.3 l_cg/l_f + 0.75 h_max/l_f - 0.105`.
///
/// Eq. (9-65) is stated for `l_f/h_max >= 3.5`; `None` outside that range or
/// for invalid data. The wing, nacelle and interference terms are neglected
/// (Torenbeek: small for a moderately swept wing).
pub fn fuselage_cn_beta_per_rad(
    fuselage: FuselageYawGeometry,
    wing_area_m2: f64,
    wing_span_m: f64,
) -> Option<f64> {
    let f = fuselage;
    let positive = [
        f.length_m,
        f.side_area_m2,
        f.max_height_m,
        f.height_quarter_m,
        f.width_quarter_m,
        f.height_three_quarter_m,
        f.width_three_quarter_m,
        wing_area_m2,
        wing_span_m,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0);
    if !positive || !f.cg_from_nose_m.is_finite() || f.length_m / f.max_height_m < 3.5 {
        return None;
    }
    let k_beta = 0.3 * f.cg_from_nose_m / f.length_m + 0.75 * f.max_height_m / f.length_m - 0.105;
    let shape = (f.height_quarter_m / f.height_three_quarter_m).sqrt()
        * (f.width_three_quarter_m / f.width_quarter_m).cbrt();
    Some(-k_beta * f.side_area_m2 * f.length_m / (wing_area_m2 * wing_span_m) * shape)
}

/// Lift-curve slope per rad of a fin of effective aspect ratio
/// `aspect_ratio`, DATCOM / Helmbold form with a 2D slope of `2 pi / beta`:
/// `2 pi A / (2 + sqrt(4 + A^2 beta^2 (1 + tan^2(L_c/2) / beta^2)))`.
///
/// `sweep_quarter_rad` is converted to the half-chord sweep with the taper
/// ratio. `None` for invalid inputs or `mach >= 1`.
pub fn fin_lift_curve_slope_per_rad(
    aspect_ratio: f64,
    sweep_quarter_rad: f64,
    taper_ratio: f64,
    mach: f64,
) -> Option<f64> {
    let valid = aspect_ratio.is_finite()
        && aspect_ratio > 0.0
        && sweep_quarter_rad.is_finite()
        && sweep_quarter_rad.abs() < 1.4
        && taper_ratio.is_finite()
        && taper_ratio > 0.0
        && mach.is_finite()
        && (0.0..1.0).contains(&mach);
    if !valid {
        return None;
    }
    let beta_sq = 1.0 - mach * mach;
    let tan_half = sweep_quarter_rad.tan()
        - 4.0 / aspect_ratio * 0.25 * (1.0 - taper_ratio) / (1.0 + taper_ratio);
    let root = 4.0 + aspect_ratio * aspect_ratio * (beta_sq + tan_half * tan_half);
    Some(2.0 * PI * aspect_ratio / (2.0 + root.sqrt()))
}

/// Fin force coefficients at one Mach number.
struct FinAuthority {
    /// `S_v eta_v C_L,alpha`: side force per unit dynamic pressure per rad
    /// of fin angle of attack, m^2.
    area_slope_m2: f64,
    /// Rudder effectiveness.
    tau: f64,
}

impl YawControlGeometry {
    fn fin_authority(&self, mach: f64) -> Option<FinAuthority> {
        let g = &self.balance;
        let slope = fin_lift_curve_slope_per_rad(
            g.fin_effective_aspect_ratio,
            g.fin_sweep_rad,
            self.fin_taper_ratio,
            mach,
        )?;
        let tau = rudder_effectiveness(self.rudder_chord_ratio)?;
        Some(FinAuthority {
            area_slope_m2: g.fin_area_m2 * FIN_DYNAMIC_PRESSURE_RATIO * slope,
            tau,
        })
    }

    fn is_usable(&self) -> bool {
        let g = &self.balance;
        [
            g.engine_arm_m,
            g.fin_arm_m,
            g.fin_area_m2,
            self.wing_area_m2,
            self.wing_span_m,
            self.rudder_max_deflection_rad,
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
            && self.cn_beta_without_fin_per_rad.is_finite()
    }

    /// Yaw-moment slope of the whole airframe per rad of sideslip, `Cn_beta`
    /// of Torenbeek Eq. (9-62) (jet: no sidewash term). Positive is stable.
    pub fn cn_beta_total_per_rad(&self, mach: f64) -> Option<f64> {
        let authority = self.fin_authority(mach)?;
        Some(self.cn_beta_without_fin_per_rad + self.fin_moment_slope(&authority))
    }

    /// `S_v eta_v C_L,alpha l_v / (S b)`: the fin's own contribution to
    /// `Cn_beta`.
    fn fin_moment_slope(&self, authority: &FinAuthority) -> f64 {
        authority.area_slope_m2 * self.balance.fin_arm_m / (self.wing_area_m2 * self.wing_span_m)
    }

    /// Solve the engine-out trim at dynamic pressure `q_pa` and Mach `mach`.
    ///
    /// `asymmetric_force_n` is the live-engine thrust plus the failed
    /// engine's windmilling drag (the moment is that force times the engine
    /// arm), `weight_n` the aircraft weight. Without a weight only
    /// [`YawTrimMode::RudderOnly`] and the zero-sideslip state are
    /// available. `None` for unusable inputs.
    pub fn trim(
        &self,
        asymmetric_force_n: f64,
        q_pa: f64,
        mach: f64,
        weight_n: Option<f64>,
        mode: YawTrimMode,
    ) -> Option<YawTrim> {
        if !(self.is_usable() && asymmetric_force_n.is_finite() && asymmetric_force_n >= 0.0) {
            return None;
        }
        if !(q_pa.is_finite() && q_pa > 0.0) {
            return None;
        }
        let authority = self.fin_authority(mach)?;
        let g = &self.balance;
        let moment_nm = asymmetric_force_n * g.engine_arm_m;
        let fin_force_zero_sideslip = moment_nm / g.fin_arm_m;
        let k_n = q_pa * authority.area_slope_m2;
        let zero_sideslip = |bank_rad: f64| YawTrim {
            bank_rad,
            sideslip_rad: 0.0,
            rudder_rad: fin_force_zero_sideslip / (k_n * authority.tau),
            fin_force_n: fin_force_zero_sideslip,
            bank_limited: false,
        };
        let weight = weight_n.filter(|w| w.is_finite() && *w > 0.0);
        if mode == YawTrimMode::RudderOnly {
            return Some(zero_sideslip(0.0));
        }
        let Some(weight) = weight else {
            return (mode == YawTrimMode::MinimumDrag).then(|| zero_sideslip(f64::NAN));
        };
        let capacity_n = weight * BANK_LIMIT_RAD.sin();
        // Side-force balance: weight component = fin force (the airframe
        // side force is neglected). Moment balance fixes the sideslip.
        let denominator =
            q_pa * self.wing_area_m2 * self.wing_span_m * self.cn_beta_without_fin_per_rad;
        let banked = |fin_force_n: f64| -> Option<YawTrim> {
            if denominator.abs() < 1.0e-9 {
                return None;
            }
            let sideslip = (moment_nm - g.fin_arm_m * fin_force_n) / denominator;
            let rudder = (fin_force_n / k_n - sideslip) / authority.tau;
            Some(YawTrim {
                bank_rad: (fin_force_n / weight).asin(),
                sideslip_rad: sideslip,
                rudder_rad: rudder,
                fin_force_n,
                bank_limited: true,
            })
        };
        let at_limit = banked(capacity_n);
        match mode {
            YawTrimMode::MinimumDrag => {
                if fin_force_zero_sideslip <= capacity_n {
                    Some(zero_sideslip((fin_force_zero_sideslip / weight).asin()))
                } else {
                    at_limit
                }
            }
            YawTrimMode::LeastRudder => {
                // Bank toward the live engine lets the fin carry more than
                // the zero-sideslip force, at the price of sideslip with the
                // wind from the live side, which the destabilizing airframe
                // pays for. Between the zero-sideslip force and the bank
                // capacity the rudder demand falls and the sideslip demand
                // rises linearly with the fin force, so the least demand is
                // at an end or where the two cross. Less force than the
                // zero-sideslip value would lean on the destabilizing
                // airframe with the wind from the failed side, which is not
                // a state a pilot holds.
                let low = fin_force_zero_sideslip.min(capacity_n);
                let fraction = |trim: &YawTrim| {
                    (
                        trim.rudder_rad / self.rudder_max_deflection_rad,
                        trim.sideslip_rad / SIDESLIP_LIMIT_RAD,
                    )
                };
                let mut forces = vec![low, capacity_n];
                if let (Some(a), Some(b)) = (banked(low), banked(capacity_n)) {
                    let (ra, sa) = fraction(&a);
                    let (rb, sb) = fraction(&b);
                    let (ga, gb) = (ra - sa, rb - sb);
                    if ga * gb < 0.0 {
                        forces.push(low - ga * (capacity_n - low) / (gb - ga));
                    }
                }
                let mut best = (fin_force_zero_sideslip <= capacity_n)
                    .then(|| zero_sideslip((fin_force_zero_sideslip / weight).asin()));
                for force in forces {
                    let candidate = banked(force).map(|mut trim| {
                        trim.bank_limited = force >= capacity_n;
                        trim
                    });
                    if let Some(candidate) = candidate {
                        if best.is_none_or(|b| demand(&b, self) > demand(&candidate, self)) {
                            best = Some(candidate);
                        }
                    }
                }
                best
            }
            YawTrimMode::RudderOnly => None,
        }
    }

    /// Minimum control speed for the thrust asymmetry `asymmetric_force_n`:
    /// the largest true airspeed at which the rudder demand reaches its
    /// maximum deflection or the sideslip its limit.
    ///
    /// `rho_kg_m3` and `speed_of_sound_m_s` give the dynamic pressure and
    /// the Mach number of the fin; `mode` selects the balance (use
    /// [`YawTrimMode::LeastRudder`] for the airborne `VMCA` and
    /// [`YawTrimMode::RudderOnly`] for the ground estimate). `None` when the
    /// aircraft is not controllable even at the top of the search range or
    /// for unusable inputs.
    pub fn minimum_control_speed(
        &self,
        asymmetric_force_n: f64,
        rho_kg_m3: f64,
        speed_of_sound_m_s: f64,
        weight_n: Option<f64>,
        mode: YawTrimMode,
    ) -> Option<MinimumControlSpeed> {
        if !(rho_kg_m3.is_finite() && rho_kg_m3 > 0.0 && speed_of_sound_m_s > 0.0) {
            return None;
        }
        let state = |v: f64| -> Option<(YawTrim, f64)> {
            let q = 0.5 * rho_kg_m3 * v * v;
            let trim = self.trim(
                asymmetric_force_n,
                q,
                v / speed_of_sound_m_s,
                weight_n,
                mode,
            )?;
            Some((trim, demand(&trim, self)))
        };
        let (mut low, mut high) = SEARCH_SPEED_RANGE_M_S;
        // Demand falls with speed: at `high` it must be below one, at `low`
        // at least one.
        if state(high)?.1 >= 1.0 || state(low)?.1 < 1.0 {
            return None;
        }
        for _ in 0..80 {
            let mid = 0.5 * (low + high);
            if state(mid)?.1 >= 1.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        let (trim, _) = state(low)?;
        let limit = if trim.rudder_rad.abs() / self.rudder_max_deflection_rad
            >= trim.sideslip_rad.abs() / SIDESLIP_LIMIT_RAD
        {
            ControlLimit::Rudder
        } else {
            ControlLimit::Sideslip
        };
        Some(MinimumControlSpeed {
            tas_m_s: low,
            trim,
            limit,
        })
    }

    /// Rudder deflection that holds a steady sideslip `sideslip_rad` with no
    /// thrust asymmetry (the crosswind balance of Torenbeek Eq. 9-68): the
    /// net airframe yaw stiffness over the rudder moment slope, so the
    /// dynamic pressure cancels. The net stiffness is floored at
    /// [`NET_DIRECTIONAL_STABILITY_FLOOR`]. Positive opposes the sideslip.
    /// `None` for unusable inputs.
    pub fn crosswind_rudder_rad(&self, sideslip_rad: f64, mach: f64) -> Option<f64> {
        if !(self.is_usable() && sideslip_rad.is_finite()) {
            return None;
        }
        let authority = self.fin_authority(mach)?;
        let fin_slope = self.fin_moment_slope(&authority);
        let net =
            (self.cn_beta_without_fin_per_rad + fin_slope).max(NET_DIRECTIONAL_STABILITY_FLOOR);
        Some(net * sideslip_rad / (fin_slope * authority.tau))
    }
}

/// Control demand of a trim state: the larger of the rudder and sideslip
/// fractions of their limits; at or above one the aircraft is at its limit.
fn demand(trim: &YawTrim, geometry: &YawControlGeometry) -> f64 {
    (trim.rudder_rad.max(0.0) / geometry.rudder_max_deflection_rad)
        .max(trim.sideslip_rad.abs() / SIDESLIP_LIMIT_RAD)
}

#[cfg(test)]
mod tests;
