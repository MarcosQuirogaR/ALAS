// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drag of the failed engine and of the asymmetric-thrust trim.
//!
//! Sources, read from the page images of the owner-supplied handbooks:
//!
//! - Windmilling turbofan: Roskam, *Airplane Design Part VI*, Sec. 4.5.3,
//!   Eq. (4.67), and the same equation in Torenbeek, *Synthesis of Subsonic
//!   Airplane Design* (1982), App. G-8.1, Eq. (G-82):
//!   `d(CD S)_wm = 0.0785 d_inl^2 + 2/(1 + 0.16 M^2) * sum A_N (V_N/U)(1 - V_N/U)`.
//!   The first term is `0.1 (pi/4) d_inl^2` of external spillage (G-80); the
//!   second is the internal momentum loss of each exhaust stream.
//! - Stopped propeller: Roskam VI Eq. (4.69) and Torenbeek Eq. (G-83),
//!   `d(CD S) = 0.00125 n_p D_p^2`; Torenbeek Sec. 5.4.3(d) states that a
//!   feathered propeller adds about 4 percent to CD0, used here because the
//!   engine catalogue carries no blade count.
//! - Asymmetric-thrust trim at zero sideslip: Torenbeek App. G-8.3,
//!   Eqs. (G-84) to (G-89): fin induced drag plus rudder profile drag of the
//!   fin side force that balances the yawing moment of the live-engine
//!   thrust and the failed-engine drag.
//!
//! All areas are drag areas `CD * S` in m^2; divide by the reference wing
//! area for a coefficient. The trim value is an upper bound for a pilot who
//! may use sideslip and up to 5 deg of bank (CS 25.149(b)); the handbook
//! gives no relief formula.

use std::f64::consts::PI;

/// External spillage coefficient of Torenbeek (G-80): `0.1 (pi/4) = 0.0785`.
pub const WINDMILL_SPILLAGE_COEFFICIENT: f64 = 0.0785;
/// Torenbeek (G-83) / Roskam (4.69) stopped-propeller coefficient.
pub const STOPPED_PROPELLER_COEFFICIENT: f64 = 0.00125;
/// Torenbeek Sec. 5.4.3(d): a feathered propeller adds about 4 percent to CD0.
pub const FEATHERED_PROPELLER_CD0_FRACTION: f64 = 0.04;
/// Nozzle flow velocity over flight speed, high-bypass fan stream
/// (Torenbeek Sec. G-8.1).
pub const VN_OVER_U_FAN_STREAM: f64 = 0.92;
/// Nozzle flow velocity over flight speed, high-bypass primary (core) stream
/// (Torenbeek Sec. G-8.1).
pub const VN_OVER_U_CORE_STREAM: f64 = 0.12;
/// Fan-stream nozzle area over inlet area for a CFM56-class high-bypass
/// engine: 1.45 m^2 over `pi/4 * 1.735^2` m^2. An assumed class geometry (the
/// catalogue carries no nozzle areas); it enters only the internal term,
/// which is about 55 percent of the windmilling drag area.
pub const FAN_NOZZLE_OVER_INLET_AREA: f64 = 0.613;
/// Core-stream nozzle area over inlet area for the same class geometry:
/// 0.40 m^2 over the 2.364 m^2 inlet.
pub const CORE_NOZZLE_OVER_INLET_AREA: f64 = 0.169;

/// One exhaust stream of a windmilling engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExhaustStream {
    /// Nozzle exit area of the stream, m^2.
    pub nozzle_area_m2: f64,
    /// Nozzle flow velocity over flight speed, `V_N/U` [-].
    pub velocity_ratio: f64,
}

/// Windmilling drag area `CD S` [m^2] of one jet engine, Roskam VI Eq. (4.67)
/// = Torenbeek Eq. (G-82).
///
/// `inlet_diameter_m` is the inlet (fan) diameter; `streams` lists each
/// exhaust stream with its nozzle area and `V_N/U`; `mach` is the flight Mach
/// number. Returns `None` for non-finite or negative inputs.
pub fn windmilling_jet_drag_area_m2(
    inlet_diameter_m: f64,
    streams: &[ExhaustStream],
    mach: f64,
) -> Option<f64> {
    let valid = inlet_diameter_m.is_finite()
        && inlet_diameter_m > 0.0
        && mach.is_finite()
        && mach >= 0.0
        && streams.iter().all(|stream| {
            stream.nozzle_area_m2.is_finite()
                && stream.nozzle_area_m2 >= 0.0
                && stream.velocity_ratio.is_finite()
                && (0.0..=1.0).contains(&stream.velocity_ratio)
        });
    if !valid {
        return None;
    }
    let internal: f64 = streams
        .iter()
        .map(|stream| stream.nozzle_area_m2 * stream.velocity_ratio * (1.0 - stream.velocity_ratio))
        .sum();
    Some(
        WINDMILL_SPILLAGE_COEFFICIENT * inlet_diameter_m * inlet_diameter_m
            + 2.0 / (1.0 + 0.16 * mach * mach) * internal,
    )
}

/// The two exhaust streams of a high-bypass turbofan scaled from its inlet
/// diameter with the class nozzle-area ratios above.
pub fn high_bypass_streams(inlet_diameter_m: f64) -> [ExhaustStream; 2] {
    let inlet_area = PI / 4.0 * inlet_diameter_m * inlet_diameter_m;
    [
        ExhaustStream {
            nozzle_area_m2: FAN_NOZZLE_OVER_INLET_AREA * inlet_area,
            velocity_ratio: VN_OVER_U_FAN_STREAM,
        },
        ExhaustStream {
            nozzle_area_m2: CORE_NOZZLE_OVER_INLET_AREA * inlet_area,
            velocity_ratio: VN_OVER_U_CORE_STREAM,
        },
    ]
}

/// Drag area `CD S` [m^2] of a stopped propeller with `blade_count` blades of
/// disc diameter `diameter_m`: Roskam VI Eq. (4.69) = Torenbeek Eq. (G-83).
pub fn stopped_propeller_drag_area_m2(blade_count: f64, diameter_m: f64) -> Option<f64> {
    (blade_count.is_finite() && blade_count > 0.0 && diameter_m.is_finite() && diameter_m > 0.0)
        .then_some(STOPPED_PROPELLER_COEFFICIENT * blade_count * diameter_m * diameter_m)
}

/// Drag coefficient of one feathered propeller as a fraction of the clean
/// zero-lift drag coefficient (Torenbeek Sec. 5.4.3(d)).
pub fn feathered_propeller_cd(clean_cd0: f64) -> Option<f64> {
    (clean_cd0.is_finite() && clean_cd0 >= 0.0)
        .then_some(FEATHERED_PROPELLER_CD0_FRACTION * clean_cd0)
}

/// Fin, rudder and engine geometry for the asymmetric-thrust balance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YawBalanceGeometry {
    /// Lateral arm of the failed (critical) engine, m.
    pub engine_arm_m: f64,
    /// Fin side-force arm: distance from the centre of gravity to the fin
    /// aerodynamic centre along the body axis, m.
    pub fin_arm_m: f64,
    /// Fin planform area, m^2.
    pub fin_area_m2: f64,
    /// Effective fin aspect ratio `A_v,eff`.
    pub fin_effective_aspect_ratio: f64,
    /// Rudder area, m^2.
    pub rudder_area_m2: f64,
    /// Fin quarter-chord sweep, rad.
    pub fin_sweep_rad: f64,
}

/// Drag area `CD S` [m^2] of the asymmetric-thrust trim at zero sideslip,
/// Torenbeek Eqs. (G-84) to (G-86) for a jet aircraft (no sidewash term).
///
/// `asymmetric_force_n` is the live-engine thrust plus the failed engine's
/// windmilling drag, N; `dynamic_pressure_pa` the dynamic pressure at the
/// fin. Returns `(fin induced, rudder profile)` drag areas, or `None` for
/// invalid inputs.
pub fn asymmetric_trim_drag_areas_m2(
    geometry: YawBalanceGeometry,
    asymmetric_force_n: f64,
    dynamic_pressure_pa: f64,
) -> Option<(f64, f64)> {
    let g = geometry;
    if !(g.engine_arm_m.is_finite()
        && g.engine_arm_m > 0.0
        && g.fin_arm_m.is_finite()
        && g.fin_arm_m > 0.0)
    {
        return None;
    }
    // Eq. (G-85): the fin side force that balances the asymmetric moment.
    fin_force_drag_areas_m2(
        g,
        asymmetric_force_n * g.engine_arm_m / g.fin_arm_m,
        dynamic_pressure_pa,
    )
}

/// Drag areas `CD S` [m^2] of a fin carrying the side force `fin_force_n` [N]
/// at dynamic pressure `dynamic_pressure_pa`: `(induced, rudder profile)`,
/// Torenbeek Eqs. (G-84) to (G-86) with the fin side-force coefficient
/// `C_Yv = fin_force_n / (q S_v)` of Eq. (G-85).
///
/// The side force is whatever the trim state loads on the fin: all of the
/// asymmetric moment over the arm at zero sideslip, or less when the
/// airframe supplies part of the balance. `None` for invalid inputs.
pub fn fin_force_drag_areas_m2(
    geometry: YawBalanceGeometry,
    fin_force_n: f64,
    dynamic_pressure_pa: f64,
) -> Option<(f64, f64)> {
    let g = geometry;
    let valid = [
        g.fin_area_m2,
        g.fin_effective_aspect_ratio,
        g.rudder_area_m2,
        fin_force_n,
        dynamic_pressure_pa,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0)
        && g.fin_sweep_rad.is_finite()
        && g.fin_sweep_rad.abs() < 1.4
        && g.rudder_area_m2 <= g.fin_area_m2;
    if !valid {
        return None;
    }
    let c_yv = fin_force_n / (dynamic_pressure_pa * g.fin_area_m2);
    let induced = c_yv * c_yv * g.fin_area_m2 / (PI * g.fin_effective_aspect_ratio);
    // Eq. (G-86): rudder profile drag.
    let rudder = (2.3 / PI)
        * (g.rudder_area_m2 * g.fin_area_m2).sqrt()
        * g.fin_effective_aspect_ratio.powf(-4.0 / 3.0)
        * g.fin_sweep_rad.cos().powf(1.0 / 3.0)
        * c_yv
        * c_yv;
    (induced.is_finite() && rudder.is_finite()).then_some((induced, rudder))
}

/// Total asymmetric-trim drag area `CD S` [m^2]: fin induced plus rudder
/// profile drag, see [`asymmetric_trim_drag_areas_m2`].
pub fn asymmetric_trim_drag_area_m2(
    geometry: YawBalanceGeometry,
    asymmetric_force_n: f64,
    dynamic_pressure_pa: f64,
) -> Option<f64> {
    asymmetric_trim_drag_areas_m2(geometry, asymmetric_force_n, dynamic_pressure_pa)
        .map(|(induced, rudder)| induced + rudder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CFM56-class worked example of SOURCES-R2 Sec. 1.5: fan inlet 1.735 m,
    /// fan nozzle 1.45 m^2, core nozzle 0.40 m^2, M 0.22.
    fn a320_streams() -> [ExhaustStream; 2] {
        [
            ExhaustStream {
                nozzle_area_m2: 1.45,
                velocity_ratio: 0.92,
            },
            ExhaustStream {
                nozzle_area_m2: 0.40,
                velocity_ratio: 0.12,
            },
        ]
    }

    #[test]
    fn the_windmilling_formula_reproduces_the_handbook_worked_example() {
        let area = windmilling_jet_drag_area_m2(1.735, &a320_streams(), 0.22).unwrap_or(f64::NAN);
        // spillage 0.0785 * 1.735^2 = 0.23630; streams
        // 2/(1+0.16*0.0484) * (1.45*0.92*0.08 + 0.40*0.12*0.88) = 0.29562.
        assert!((area - 0.5319).abs() < 5e-4, "{area}");
        // CD = area / S for S = 122.4 m^2: the Roskam/Torenbeek 0.0043.
        assert!((area / 122.4 - 0.00435).abs() < 5e-5);
    }

    #[test]
    fn windmilling_drag_has_the_limiting_behaviour_of_the_formula() {
        // No exhaust streams: only the spillage term remains.
        let spill = windmilling_jet_drag_area_m2(2.0, &[], 0.2).unwrap_or(f64::NAN);
        assert!((spill - 0.0785 * 4.0).abs() < 1e-15);
        // A nozzle at U or at rest produces no internal loss.
        for ratio in [0.0, 1.0] {
            let stream = ExhaustStream {
                nozzle_area_m2: 1.0,
                velocity_ratio: ratio,
            };
            let area = windmilling_jet_drag_area_m2(2.0, &[stream], 0.2).unwrap_or(f64::NAN);
            assert!((area - spill).abs() < 1e-15);
        }
        // Mach reduces the internal term only through 2/(1 + 0.16 M^2).
        let slow = windmilling_jet_drag_area_m2(1.735, &a320_streams(), 0.0).unwrap_or(f64::NAN);
        let fast = windmilling_jet_drag_area_m2(1.735, &a320_streams(), 0.8).unwrap_or(f64::NAN);
        assert!(fast < slow);
        // Invalid inputs are rejected.
        assert!(windmilling_jet_drag_area_m2(-1.0, &[], 0.2).is_none());
        assert!(windmilling_jet_drag_area_m2(1.7, &[], f64::NAN).is_none());
        let bad = ExhaustStream {
            nozzle_area_m2: 1.0,
            velocity_ratio: 1.5,
        };
        assert!(windmilling_jet_drag_area_m2(1.7, &[bad], 0.2).is_none());
    }

    #[test]
    fn the_class_stream_geometry_reproduces_the_worked_inlet_ratios() {
        let streams = high_bypass_streams(1.735);
        assert!((streams[0].nozzle_area_m2 - 1.45).abs() < 0.01);
        assert!((streams[1].nozzle_area_m2 - 0.40).abs() < 0.01);
        let area = windmilling_jet_drag_area_m2(1.735, &streams, 0.22).unwrap_or(f64::NAN);
        assert!((area - 0.532).abs() < 0.01, "{area}");
    }

    #[test]
    fn the_stopped_propeller_follows_the_blade_count_and_diameter_square() {
        // Six blades of 3.93 m: 0.00125 * 6 * 15.44 = 0.1158 m^2.
        let area = stopped_propeller_drag_area_m2(6.0, 3.93).unwrap_or(f64::NAN);
        assert!((area - 0.1158).abs() < 1e-3);
        let doubled = stopped_propeller_drag_area_m2(6.0, 7.86).unwrap_or(f64::NAN);
        assert!((doubled / area - 4.0).abs() < 1e-12);
        assert!(stopped_propeller_drag_area_m2(0.0, 3.0).is_none());
        // Torenbeek 5.4.3(d): 4 percent of CD0.
        assert!((feathered_propeller_cd(0.030).unwrap_or(f64::NAN) - 0.0012).abs() < 1e-15);
    }

    /// SOURCES-R2 Sec. 1.6 worked example: T_e 100 kN at q 3445 Pa, engine arm
    /// 5.75 m, fin arm 15 m, fin 21.5 m^2, A_v,eff 1.6, rudder 5.5 m^2, sweep
    /// 35 deg, with the windmilling drag area 0.532 m^2 added to the force.
    fn a320_yaw() -> YawBalanceGeometry {
        YawBalanceGeometry {
            engine_arm_m: 5.75,
            fin_arm_m: 15.0,
            fin_area_m2: 21.5,
            fin_effective_aspect_ratio: 1.6,
            rudder_area_m2: 5.5,
            fin_sweep_rad: 35.0_f64.to_radians(),
        }
    }

    #[test]
    fn the_asymmetric_trim_drag_reproduces_the_handbook_worked_example() {
        let q = 3445.0;
        let force = 100_000.0 + 0.532 * q;
        let (induced, rudder) =
            asymmetric_trim_drag_areas_m2(a320_yaw(), force, q).unwrap_or((f64::NAN, f64::NAN));
        assert!((induced - 1.19).abs() < 0.01, "{induced}");
        assert!((rudder - 1.11).abs() < 0.01, "{rudder}");
        let total = asymmetric_trim_drag_area_m2(a320_yaw(), force, q).unwrap_or(f64::NAN);
        assert!((total / 122.4 - 0.0187).abs() < 3e-4);
    }

    #[test]
    fn the_trim_drag_scales_with_the_square_of_the_asymmetry() {
        let q = 3445.0;
        let single = asymmetric_trim_drag_area_m2(a320_yaw(), 50_000.0, q).unwrap_or(f64::NAN);
        let double = asymmetric_trim_drag_area_m2(a320_yaw(), 100_000.0, q).unwrap_or(f64::NAN);
        assert!((double / single - 4.0).abs() < 1e-12);
        // A longer fin arm or a larger fin reduces it.
        let long_arm = YawBalanceGeometry {
            fin_arm_m: 20.0,
            ..a320_yaw()
        };
        let longer = asymmetric_trim_drag_area_m2(long_arm, 100_000.0, q).unwrap_or(f64::NAN);
        assert!(longer < double);
    }

    #[test]
    fn invalid_yaw_balance_inputs_are_rejected() {
        let q = 3445.0;
        assert!(asymmetric_trim_drag_area_m2(a320_yaw(), 0.0, q).is_none());
        assert!(asymmetric_trim_drag_area_m2(a320_yaw(), 1.0e5, 0.0).is_none());
        let rudder_larger_than_fin = YawBalanceGeometry {
            rudder_area_m2: 30.0,
            ..a320_yaw()
        };
        assert!(asymmetric_trim_drag_area_m2(rudder_larger_than_fin, 1.0e5, q).is_none());
    }
}
