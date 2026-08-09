// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/performance.py
// Reference: alas @ rust-port-baseline.

//! The Breguet range equation and the CS-25 V-n flight envelope.

use alas_atmo::Atmosphere;
use alas_config::{DesignRequirements, PerformanceConfig};

use super::{linspace, G, RHO_SL};

/// Metres per second to knots as `build_vn_diagram` writes it locally (`_KT`),
/// a six-figure factor distinct from the module's five-figure `_MS_TO_KT`;
/// this is the one the envelope's speeds are reported in.
const KT: f64 = 1.943844;

/// Breguet range [m], SI throughout -- `breguet_range_m`.
///
/// ```text
/// R = (V / (tsfc_si * g)) * (L/D) * ln(W_start / W_end)
/// ```
///
/// `tsfc_si` is thrust-specific fuel consumption in kg/(N*s). Any physically
/// meaningless input -- a non-positive weight or TSFC, or a burn that gains
/// weight -- returns `0.0` rather than a negative or NaN range.
pub fn breguet_range_m(
    tas_m_s: f64,
    l_over_d: f64,
    tsfc_si: f64,
    w_start_kg: f64,
    w_end_kg: f64,
) -> f64 {
    if w_end_kg <= 0.0 || w_start_kg <= 0.0 || w_end_kg > w_start_kg || tsfc_si <= 0.0 {
        return 0.0;
    }
    (tas_m_s / (tsfc_si * G)) * l_over_d * (w_start_kg / w_end_kg).ln()
}

/// CS-25-style V-n (flight-envelope) diagram data -- `VnDiagramData`. Speeds
/// are equivalent airspeed [kt] at MTOW against sea-level density, the
/// convention V-n diagrams are plotted in.
#[derive(Debug, Clone, PartialEq)]
pub struct VnDiagramData {
    /// Velocity axis [kt], `0..~1.1*max(VD, cruise)`.
    pub v_kt: Vec<f64>,
    /// Positive stall boundary `n(v)`.
    pub n_stall_pos: Vec<f64>,
    /// Negative stall boundary `n(v)`.
    pub n_stall_neg: Vec<f64>,
    /// Positive limit load factor.
    pub n_lim_pos: f64,
    /// Negative limit load factor.
    pub n_lim_neg: f64,
    /// Positive ultimate load factor.
    pub n_ult_pos: f64,
    /// Negative ultimate load factor.
    pub n_ult_neg: f64,
    /// 1 g clean stall speed at MTOW [kt].
    pub v_s_kt: f64,
    /// Maneuvering speed [kt].
    pub v_a_kt: f64,
    /// Design cruise speed [kt] (derived `VD/1.25`).
    pub v_c_kt: f64,
    /// Design dive speed [kt].
    pub v_d_kt: f64,
    /// Actual operating cruise EAS [kt] (informational).
    pub v_cruise_op_kt: f64,
}

/// Build the CS-25-style V-n diagram -- `build_vn_diagram`.
///
/// Scoped to `s_ref` rather than a whole `Airplane`: upstream reads only
/// `plane.s_ref` off its airplane argument, so this crate takes the reference
/// area directly instead of a dependency on the geometry crate for one field.
/// `n_lim_pos = ultimate_load_factor / 1.5` (CS-25.303 factor of safety),
/// `VC = VD / 1.25` (CS-25.335(b) minimum margin); the stall boundaries use
/// the clean-configuration lift limits, distinct from the flaps-down maxima.
pub fn build_vn_diagram(
    s_ref: f64,
    req: &DesignRequirements,
    perf_cfg: &PerformanceConfig,
    cruise_alt_m: f64,
) -> VnDiagramData {
    let w = req.mtow_kg * G;
    let s = s_ref.max(1e-6);

    let n_ult_pos = req.ultimate_load_factor;
    let n_lim_pos = n_ult_pos / 1.5;
    let n_lim_neg = req.limit_load_factor_neg;
    let n_ult_neg = n_lim_neg * 1.5;

    let v_d = req.dive_speed_m_s;
    let v_c = v_d / 1.25;
    let v_s = (2.0 * w / (RHO_SL * s * perf_cfg.cl_max_clean)).sqrt();
    let v_a = (2.0 * n_lim_pos * w / (RHO_SL * s * perf_cfg.cl_max_clean)).sqrt();

    let atmo = Atmosphere::new(cruise_alt_m);
    let v_cruise_op = req.cruise_mach * atmo.speed_of_sound() * (atmo.density() / RHO_SL).sqrt();

    let v_max = v_d.max(v_cruise_op) * 1.15;
    let v_ms = linspace(0.0, v_max, 400);
    let n_stall_pos: Vec<f64> = v_ms
        .iter()
        .map(|&v| 0.5 * RHO_SL * v * v * s * perf_cfg.cl_max_clean / w)
        .collect();
    let n_stall_neg: Vec<f64> = v_ms
        .iter()
        .map(|&v| 0.5 * RHO_SL * v * v * s * perf_cfg.cl_min_clean / w)
        .collect();

    VnDiagramData {
        v_kt: v_ms.iter().map(|&v| v * KT).collect(),
        n_stall_pos,
        n_stall_neg,
        n_lim_pos,
        n_lim_neg,
        n_ult_pos,
        n_ult_neg,
        v_s_kt: v_s * KT,
        v_a_kt: v_a * KT,
        v_c_kt: v_c * KT,
        v_d_kt: v_d * KT,
        v_cruise_op_kt: v_cruise_op * KT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_physically_impossible_leg_has_zero_range() {
        // Each guard branch: end above start, non-positive weight, dead TSFC.
        assert_eq!(
            breguet_range_m(231.0, 18.5, 1.6e-5, 63_000.0, 79_000.0),
            0.0
        );
        assert_eq!(breguet_range_m(231.0, 18.5, 1.6e-5, 79_000.0, 0.0), 0.0);
        assert_eq!(breguet_range_m(231.0, 18.5, 0.0, 79_000.0, 63_000.0), 0.0);
    }

    #[test]
    fn burning_more_fuel_flies_further() {
        let less = breguet_range_m(231.0, 18.5, 1.6e-5, 79_000.0, 70_000.0);
        let more = breguet_range_m(231.0, 18.5, 1.6e-5, 79_000.0, 63_000.0);
        assert!(more > less && less > 0.0);
    }

    #[test]
    fn the_maneuvering_speed_sits_above_the_stall() {
        // v_a scales the stall speed by sqrt(n_lim_pos); with a limit above 1 g
        // it must exceed the 1 g stall.
        let data = build_vn_diagram(
            122.0,
            &DesignRequirements::default(),
            &PerformanceConfig::default(),
            10668.0,
        );
        assert!(data.v_a_kt > data.v_s_kt);
        assert!(data.n_lim_pos < data.n_ult_pos);
    }
}
