// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The throttled operating point of the cycle at a cruise condition.
//!
//! [`super::compute_turbofan_cycle`] evaluates one cycle at stated pressure
//! ratios and turbine inlet temperature. An engine at cruise is throttled
//! below its take-off rating: the temperature falls until the thrust equals
//! the cruise demand, and the pressure ratios and the air flow follow it,
//! because the turbines pass a fixed corrected flow. This module finds that
//! operating point from the engine's design (static take-off) cycle with the
//! two relations a conceptual design can state without component maps.
//!
//! **Turbine compatibility** (a fixed-geometry engine with choked turbine
//! nozzles, derived here from two equations, no fitted constant):
//!
//! 1. Choked flow through the high-pressure turbine nozzle:
//!    `m_core (1 + f) sqrt(Tt4) / pt4 = const`, with `pt4 = pt3 * eta_b` and
//!    `pt3 = pt2 * OPR`. With the nozzle choked the turbine pressure ratio and
//!    temperature ratio `tau_t` are then fixed as well (constant turbine
//!    efficiency).
//! 2. Work balance of the shafts: the high-pressure turbine drives the
//!    compressor, `cp (Tt3 - Tt25') = eta_m cp_h (1 + f) (1 - tau_t) Tt4`, and
//!    the low-pressure turbine drives the fan (flow `B`) and the booster in
//!    the same way, so every compression temperature rise is proportional to
//!    `Tt4`. Hence `(Tt3/Tt2 - 1)` and `(Tt13/Tt2 - 1)` scale with
//!    `(Tt4/Tt2) / (Tt4/Tt2)_design`: **the overall and fan pressure ratios
//!    fall as the engine is throttled** and rise on a cold day.
//!
//! The compression ratios are recovered from those temperature ratios with
//! the cycle's own polytropic relations, and the core mass flow from (1)
//! (Mattingly, *Elements of Propulsion*, 2006, the constant-turbine-flow
//! reference-point method; Walsh and Fletcher, *Gas Turbine Performance*,
//! 2004, the same compatibility argument). The design point is the static
//! sea-level take-off cycle at the engine's catalogue temperature, which also
//! anchors the static flow ([`super::anchor_mass_flow_kg_s`]). The thrust is
//! then matched by bisection on `Tt4` between the compressor discharge
//! temperature and the take-off temperature (monotone in `Tt4`).
//!
//! The booster is on the low-pressure spool, so its temperature rise would
//! scale with the fan's; the cycle (`compute_turbofan_cycle`) takes the
//! booster as a configured share of the overall pressure ratio, so the share
//! is held at its configured value here. Open: add a booster
//! pressure-ratio input to the cycle. Needs data: the Mattingly
//! (2006) and Walsh & Fletcher (2004) page checks are not on disk, so the
//! booster input is not yet sourced. It moves the OPR split, not the overall
//! ratio, which the work balance sets. The low-pressure turbine's temperature ratio is held at its
//! design value, which presumes a choked (or fixed-pressure-ratio) downstream
//! nozzle: at sea-level static the core nozzle of a high-bypass engine is not
//! choked, so the *design* point of this reference-point method carries that
//! approximation into every throttled point; the cruise point (choked core
//! nozzle at the cruise pressure ratio) is the case it is meant for.
//!
//! What is not modelled: component efficiencies vary with throttle (held at
//! their design values), the bypass ratio is held (it rises a few per cent at
//! part power on a real fan), and bleed, cooling and offtake flows are absent
//! as in the cycle itself.
//!
//! **Sources.** The two relations are the textbook constant-turbine-flow and
//! work-balance arguments (Mattingly 2006; Walsh and Fletcher 2004). Neither
//! book is in the project's reference libraries, so the citation is to the
//! method, not to a page this code was checked against; the derivation above
//! is self-contained and is what the tests exercise.

use alas_atmo::Atmosphere;
use alas_config::PropulsionCycleConfig;

use super::{
    anchor_mass_flow_kg_s, compute_turbofan_cycle, walk_cycle, TurbofanCycleInputs,
    TurbofanCycleResult,
};

/// Bisection tolerance on the turbine inlet temperature, K.
const TEMPERATURE_TOLERANCE_K: f64 = 1.0e-3;

/// A throttled cruise point of an engine.
#[derive(Debug, Clone, PartialEq)]
pub struct CruiseOperatingPoint {
    /// The cycle inputs at the matched operating point: the pressure ratios
    /// and the turbine inlet temperature that go with the cruise thrust.
    pub inputs: TurbofanCycleInputs,
    /// Total (core plus bypass) air mass flow of one engine, kg/s.
    pub mass_flow_kg_s: f64,
    /// Net thrust of one engine, N.
    pub thrust_n: f64,
    /// The cycle at those inputs.
    pub cycle: TurbofanCycleResult,
}

/// The design-point temperature ratios the throttled point scales from.
struct Design {
    /// `Tt3 / Tt2 - 1` at the static take-off cycle.
    compression: f64,
    /// `Tt13 / Tt2 - 1` at the static take-off cycle.
    fan: f64,
    /// `Tt4 / Tt2` at the static take-off cycle.
    temperature_ratio: f64,
    /// `pt3` of the static cycle, Pa.
    pt3_pa: f64,
    /// Fuel-air ratio of the static cycle.
    fuel_air: f64,
    /// Core mass flow of the static cycle, kg/s.
    core_flow_kg_s: f64,
}

/// Inputs and flow of the throttled cycle at a trial turbine temperature.
fn throttled(
    design: &Design,
    design_inputs: &TurbofanCycleInputs,
    tit_k: f64,
    cfg: &PropulsionCycleConfig,
) -> Option<(TurbofanCycleInputs, f64)> {
    let gc = cfg.gamma_cold;
    let exponent = (gc - 1.0) / gc;
    // Inlet total temperature of the flight condition (the static point has no ram).
    let mach = design_inputs.mach.max(0.0);
    let tt2 = Atmosphere::new(design_inputs.altitude_m).temperature()
        * (1.0 + 0.5 * (gc - 1.0) * mach * mach);
    let scale = (tit_k / tt2) / design.temperature_ratio;
    // Fan: (Tt13/Tt2 - 1) scales with Tt4/Tt2; the polytropic relation gives FPR.
    let fan_ratio = 1.0 + scale * design.fan;
    let fpr = fan_ratio.powf(cfg.fan_polytropic_efficiency / exponent);
    // Core compression: the core stream crosses the fan hub (the fan's
    // temperature rise) and then the booster, whose configured pressure ratio
    // is held; the high-pressure compressor takes the rest of the rise.
    let total = 1.0 + scale * design.compression;
    let booster = cfg.lpc_pressure_ratio_split;
    let booster_rise = booster.powf(exponent / cfg.lpc_polytropic_efficiency);
    let hpc_rise = total / (fan_ratio * booster_rise);
    if !(hpc_rise > 1.0 && fan_ratio > 1.0) {
        return None;
    }
    let hpc = hpc_rise.powf(cfg.hpc_polytropic_efficiency / exponent);
    let inputs = TurbofanCycleInputs {
        overall_pressure_ratio: fpr * booster * hpc,
        fan_pressure_ratio: fpr,
        turbine_inlet_temperature_k: tit_k,
        ..*design_inputs
    };
    let throttled_walk = walk_cycle(&inputs, cfg).ok()?;
    // Choked high-pressure turbine nozzle: m (1 + f) sqrt(T4) / p3 is constant.
    let core_flow = design.core_flow_kg_s
        * (throttled_walk.pt3 / design.pt3_pa)
        * (design_inputs.turbine_inlet_temperature_k / tit_k).sqrt()
        * (1.0 + design.fuel_air)
        / (1.0 + throttled_walk.f_ratio);
    Some((inputs, core_flow * (1.0 + design_inputs.bypass_ratio)))
}

/// The static take-off design point the throttled points scale from.
fn design_of(
    design_inputs: &TurbofanCycleInputs,
    static_flow_kg_s: f64,
    cfg: &PropulsionCycleConfig,
) -> Result<Design, String> {
    let static_inputs = TurbofanCycleInputs {
        mach: 0.0,
        altitude_m: 0.0,
        ..*design_inputs
    };
    let static_walk = walk_cycle(&static_inputs, cfg)?;
    Ok(Design {
        compression: static_walk.tt3 / static_walk.tt2 - 1.0,
        fan: static_walk.tt13 / static_walk.tt2 - 1.0,
        temperature_ratio: design_inputs.turbine_inlet_temperature_k / static_walk.tt2,
        pt3_pa: static_walk.pt3,
        fuel_air: static_walk.f_ratio,
        core_flow_kg_s: static_flow_kg_s / (1.0 + design_inputs.bypass_ratio),
    })
}

/// [`solve_cruise_operating_point`], with a request outside the throttle
/// range held at its end: above what the cycle delivers at the take-off
/// temperature, the take-off-temperature point (the engine at its rating);
/// below the lowest feasible temperature of the 5 K march, that point.
///
/// # Errors
///
/// A reason when the static design cycle is infeasible or no throttled
/// point is feasible at the condition.
pub fn solve_operating_point_within_range(
    design_inputs: &TurbofanCycleInputs,
    rated_thrust_n: f64,
    thrust_n: f64,
    cfg: &PropulsionCycleConfig,
) -> Result<CruiseOperatingPoint, String> {
    let reason = match solve_cruise_operating_point(design_inputs, rated_thrust_n, thrust_n, cfg) {
        Ok(point) => return Ok(point),
        Err(reason) => reason,
    };
    let (static_flow_kg_s, _) = anchor_mass_flow_kg_s(
        rated_thrust_n / 1_000.0,
        design_inputs.overall_pressure_ratio,
        design_inputs.fan_pressure_ratio,
        design_inputs.bypass_ratio,
        design_inputs.turbine_inlet_temperature_k,
        cfg,
    );
    if !static_flow_kg_s.is_finite() {
        return Err(reason);
    }
    let design = design_of(design_inputs, static_flow_kg_s, cfg)?;
    let point_at = |tit_k: f64| {
        let (inputs, mass_flow_kg_s) = throttled(&design, design_inputs, tit_k, cfg)?;
        let cycle = compute_turbofan_cycle(&inputs, cfg);
        let thrust_n = mass_flow_kg_s * cycle.specific_thrust_ms;
        (cycle.cycle_feasible && thrust_n > 0.0).then_some(CruiseOperatingPoint {
            thrust_n,
            inputs,
            mass_flow_kg_s,
            cycle,
        })
    };
    let top_k = design_inputs.turbine_inlet_temperature_k;
    let full = point_at(top_k).ok_or(reason)?;
    if thrust_n >= full.thrust_n {
        return Ok(full);
    }
    let mut lowest = full;
    for step in 1..=400 {
        match point_at(top_k - f64::from(step) * 5.0) {
            Some(point) => lowest = point,
            None => break,
        }
    }
    Ok(lowest)
}

/// The throttled operating point that delivers `thrust_n` per engine at the
/// flight condition of `design`.
///
/// `design` carries the flight condition and the engine's design cycle
/// parameters (overall and fan pressure ratio, bypass ratio and, in
/// `turbine_inlet_temperature_k`, the take-off temperature, the upper end of
/// the throttle range); `rated_thrust_n` is its static sea-level thrust.
///
/// # Errors
///
/// A reason when the static design cycle is infeasible, or when the thrust is
/// outside what the cycle delivers between the compressor discharge
/// temperature and the take-off temperature.
pub fn solve_cruise_operating_point(
    design_inputs: &TurbofanCycleInputs,
    rated_thrust_n: f64,
    thrust_n: f64,
    cfg: &PropulsionCycleConfig,
) -> Result<CruiseOperatingPoint, String> {
    if !(thrust_n.is_finite() && thrust_n > 0.0 && rated_thrust_n.is_finite()) {
        return Err(format!("cruise thrust {thrust_n} N is not usable"));
    }
    let (static_flow_kg_s, static_cycle) = anchor_mass_flow_kg_s(
        rated_thrust_n / 1_000.0,
        design_inputs.overall_pressure_ratio,
        design_inputs.fan_pressure_ratio,
        design_inputs.bypass_ratio,
        design_inputs.turbine_inlet_temperature_k,
        cfg,
    );
    if !static_flow_kg_s.is_finite() {
        return Err(format!(
            "static design cycle infeasible: {}",
            static_cycle.infeasibility_reason
        ));
    }
    let design = design_of(design_inputs, static_flow_kg_s, cfg)?;
    let thrust_at = |tit: f64| -> Option<f64> {
        let (inputs, flow) = throttled(&design, design_inputs, tit, cfg)?;
        let cycle = compute_turbofan_cycle(&inputs, cfg);
        cycle
            .cycle_feasible
            .then_some(flow * cycle.specific_thrust_ms)
    };
    let mut high = design_inputs.turbine_inlet_temperature_k;
    let full = thrust_at(high).unwrap_or(f64::NAN);
    if !full.is_finite() || full < thrust_n {
        return Err(format!(
            "cruise thrust {thrust_n:.0} N exceeds the {full:.0} N the cycle delivers at its take-off temperature"
        ));
    }
    // March down from the take-off temperature until the thrust falls to the
    // request (the bracket), refusing a request below the feasible range.
    let mut low = high;
    let mut bracketed = false;
    for step in 1..=400 {
        let candidate = high - f64::from(step) * 5.0;
        match thrust_at(candidate) {
            Some(value) if value > 0.0 => {
                low = candidate;
                if value <= thrust_n {
                    bracketed = true;
                    break;
                }
                high = candidate;
            }
            _ => break,
        }
    }
    if !bracketed {
        return Err(format!(
            "cruise thrust {thrust_n:.0} N is below what the cycle delivers at {low:.0} K"
        ));
    }
    let (mut upper, mut lower) = (high, low);
    while upper - lower > TEMPERATURE_TOLERANCE_K {
        let middle = 0.5 * (upper + lower);
        if thrust_at(middle).unwrap_or(0.0) > thrust_n {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    let tit = 0.5 * (upper + lower);
    let (inputs, mass_flow_kg_s) = throttled(&design, design_inputs, tit, cfg)
        .ok_or_else(|| "the matched temperature has no feasible cycle".to_owned())?;
    let cycle = compute_turbofan_cycle(&inputs, cfg);
    Ok(CruiseOperatingPoint {
        thrust_n: mass_flow_kg_s * cycle.specific_thrust_ms,
        inputs,
        mass_flow_kg_s,
        cycle,
    })
}

// A failed unwrap in these tests is the assertion failing, not a library
// invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn a320_class() -> TurbofanCycleInputs {
        TurbofanCycleInputs {
            mach: 0.78,
            altitude_m: 10_668.0,
            bypass_ratio: 5.7,
            overall_pressure_ratio: 32.6,
            fan_pressure_ratio: 1.6,
            turbine_inlet_temperature_k: 1_600.0,
        }
    }

    #[test]
    fn the_matched_point_delivers_the_requested_thrust_below_take_off() {
        let cfg = PropulsionCycleConfig::default();
        let design = a320_class();
        let point = solve_cruise_operating_point(&design, 117_770.0, 20_000.0, &cfg)
            .unwrap_or_else(|reason| panic!("{reason}"));
        assert!(point.inputs.turbine_inlet_temperature_k < design.turbine_inlet_temperature_k);
        assert!(
            (point.thrust_n - 20_000.0).abs() < 1.0,
            "{}",
            point.thrust_n
        );
    }

    #[test]
    fn throttling_lowers_the_pressure_ratios_with_the_temperature_ratio() {
        // Turbine compatibility: (Tt3/Tt2 - 1) and (Tt13/Tt2 - 1) scale with
        // Tt4/Tt2, so a throttled engine runs at lower OPR and FPR than its
        // design cycle, and a lower thrust needs a lower temperature, OPR and FPR.
        let cfg = PropulsionCycleConfig::default();
        let design = a320_class();
        let high = solve_cruise_operating_point(&design, 117_770.0, 24_000.0, &cfg).unwrap();
        let low = solve_cruise_operating_point(&design, 117_770.0, 16_000.0, &cfg).unwrap();
        assert!(low.inputs.turbine_inlet_temperature_k < high.inputs.turbine_inlet_temperature_k);
        assert!(low.inputs.overall_pressure_ratio < high.inputs.overall_pressure_ratio);
        assert!(low.inputs.fan_pressure_ratio < high.inputs.fan_pressure_ratio);
        assert!(low.cycle.propulsive_efficiency > high.cycle.propulsive_efficiency);
        // The compatibility relation itself, from the cycle's own stations.
        let walk = |point: &CruiseOperatingPoint| walk_cycle(&point.inputs, &cfg).unwrap();
        let (a, b) = (walk(&high), walk(&low));
        let ratio = |w: &super::super::CycleWalk| (w.tt3 / w.tt2 - 1.0) / (w.tt4 / w.tt2);
        assert!((ratio(&a) - ratio(&b)).abs() < 1.0e-9 * ratio(&a));
        let fan = |w: &super::super::CycleWalk| (w.tt13 / w.tt2 - 1.0) / (w.tt4 / w.tt2);
        assert!((fan(&a) - fan(&b)).abs() < 1.0e-9 * fan(&a));
    }

    #[test]
    fn the_core_flow_obeys_the_choked_turbine_relation() {
        let cfg = PropulsionCycleConfig::default();
        let design = a320_class();
        let point = solve_cruise_operating_point(&design, 117_770.0, 20_000.0, &cfg).unwrap();
        let (static_flow, _) = anchor_mass_flow_kg_s(117.77, 32.6, 1.6, 5.7, 1_600.0, &cfg);
        let static_walk = walk_cycle(
            &TurbofanCycleInputs {
                mach: 0.0,
                altitude_m: 0.0,
                ..design
            },
            &cfg,
        )
        .unwrap();
        let walk = walk_cycle(&point.inputs, &cfg).unwrap();
        // m (1 + f) sqrt(T4) / p3 equal at the two points.
        let flow = |m_total: f64, w: &super::super::CycleWalk| {
            m_total / 6.7 * (1.0 + w.f_ratio) * w.tt4.sqrt() / w.pt3
        };
        let a = flow(static_flow, &static_walk);
        let b = flow(point.mass_flow_kg_s, &walk);
        assert!((a - b).abs() < 1.0e-9 * a, "{a} {b}");
        // The throttled engine passes a fraction of its static flow.
        assert!(point.mass_flow_kg_s < static_flow);
    }

    #[test]
    fn thrust_beyond_the_take_off_temperature_is_refused() {
        let cfg = PropulsionCycleConfig::default();
        let error =
            solve_cruise_operating_point(&a320_class(), 117_770.0, 200_000.0, &cfg).unwrap_err();
        assert!(error.contains("exceeds"), "{error}");
    }
}
