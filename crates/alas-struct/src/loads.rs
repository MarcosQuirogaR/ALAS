// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_loads.py
// Reference: alas @ rust-port-baseline.

//! Shared spanwise load model for the wingbox.
//!
//! One elliptic-lift (+ optional inertial-relief) distributed load, integrated
//! to shear and bending moment via a cantilever (tip -> root) numerical
//! integral. Used by BOTH the strength-sizing model (load only, **no** relief
//! -- the conservative choice, matching the reference's own `00_sizing.py`)
//! and the analytical deflection estimate (**with** relief -- matching the
//! reference's `05_validation.py`, which added relief specifically to get a
//! closer match to real NASTRAN deflections). Keeping one shared
//! load-integration primitive is what guarantees sizing, the analytical
//! solver, and the NASTRAN BDF FORCE cards never disagree about the load
//! model.

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig};

/// The near-zero threshold below which a spanwise engine station is treated as
/// centerline-mounted (loading neither semi-wing). Upstream's literal `1e-6`.
const CENTERLINE_Y_THRESHOLD_M: f64 = 1e-6;

/// One structural design load case for the semi-wing.
///
/// Frozen upstream (`@dataclass(frozen=True)`); every field is a scalar or a
/// static name, so `Copy` is the faithful analogue of that immutability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadCase {
    /// `"pull-up"`, `"push-down"` or `"level"`.
    pub name: &'static str,
    /// The signed ultimate load factor n -- already including any additional
    /// safety factor.
    pub load_factor: f64,
    /// The signed total aerodynamic force on this semi-wing,
    /// `n * mtow_kg * g / 2`.
    pub total_force_n: f64,
}

/// Pull-up ultimate / push-down ultimate / 1g level.
///
/// Reuses [`DesignRequirements`]' own `ultimate_load_factor` /
/// `limit_load_factor_neg` fields with *exactly* the same derivation
/// `alas/physics/performance.py`'s V-n diagram uses (`n_ult_pos =
/// ultimate_load_factor`, `n_ult_neg = limit_load_factor_neg * 1.5`) -- so the
/// structural loads always match the V-n diagram shown elsewhere in the app,
/// not a second, independently-tuned load case. (`performance.py` is not yet
/// ported; the derivation is reproduced here from the requirement fields
/// directly, exactly as upstream does.)
pub fn load_cases(req: &DesignRequirements, additional_safety_factor: f64) -> [LoadCase; 3] {
    let g = req.gravity_m_s2;
    let w_n = req.mtow_kg * g;
    let n_ult_pos = req.ultimate_load_factor * additional_safety_factor;
    let n_ult_neg = req.limit_load_factor_neg * 1.5 * additional_safety_factor;
    [
        LoadCase {
            name: "pull-up",
            load_factor: n_ult_pos,
            total_force_n: n_ult_pos * w_n / 2.0,
        },
        LoadCase {
            name: "push-down",
            load_factor: n_ult_neg,
            total_force_n: n_ult_neg * w_n / 2.0,
        },
        LoadCase {
            name: "level",
            load_factor: 1.0,
            total_force_n: 1.0 * w_n / 2.0,
        },
    ]
}

/// Half-elliptic spanwise load distribution [N/m], integrating to
/// `total_force_n` over `[0, semi_span]`.
///
/// A classic, well-precedented preliminary-design simplification for wing
/// structural loads (the same one the reference scripts used, validated there
/// to <20% vs. real NASTRAN deformations). The `clamp(0.0, 1.0)` reproduces
/// upstream's `np.clip`, guarding the `sqrt` argument against going negative
/// at a station just past the tip.
pub fn elliptic_distributed_load(y: &[f64], semi_span: f64, total_force_n: f64) -> Vec<f64> {
    let q0 = 4.0 * total_force_n / (std::f64::consts::PI * semi_span);
    y.iter()
        .map(|&yi| {
            let arg = (1.0 - (yi / semi_span).powi(2)).clamp(0.0, 1.0);
            q0 * arg.sqrt()
        })
        .collect()
}

/// Shear `V(y)` and bending moment `M(y)` for a cantilever beam (free at the
/// tip, fixed at the root) under a net distributed load `q_net` [N/m] sampled
/// at `y`, via cumulative trapezoidal integration from tip to root.
///
/// The root reaction is never referenced directly -- V/M at `y = 0` fall out
/// of the integral, matching the reference's own approach. The accumulation
/// runs from the last segment down to the first, reproducing the summation
/// order of NumPy's `np.cumsum(seg[::-1])[::-1]` exactly, which is what keeps
/// this at the `closed` tier over a dense (FEM-resolution) station vector.
pub fn cantilever_shear_moment(y: &[f64], q_net: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = y.len();
    let mut v = vec![0.0; n];
    let mut m = vec![0.0; n];
    // With fewer than two stations there are no segments to integrate; upstream
    // leaves `v`/`m` at their `np.zeros(n)` fill, which `vec![0.0; n]` already
    // matches (an empty vector for `n == 0`, a single zero for `n == 1`).
    if n < 2 {
        return (v, m);
    }

    // Trapezoidal segment loads, then a suffix sum so that `v[i]` carries every
    // segment outboard of station `i`. The last segment is added first, matching
    // the reversed-cumsum order upstream uses.
    let mut segment = vec![0.0; n - 1];
    for i in 0..n - 1 {
        let dy = y[i + 1] - y[i];
        segment[i] = 0.5 * (q_net[i] + q_net[i + 1]) * dy;
    }
    let mut running = 0.0;
    for i in (0..n - 1).rev() {
        running += segment[i];
        v[i] = running;
    }

    // Integrate the shear the same way to get the bending moment.
    for i in 0..n - 1 {
        let dy = y[i + 1] - y[i];
        segment[i] = 0.5 * (v[i] + v[i + 1]) * dy;
    }
    let mut running = 0.0;
    for i in (0..n - 1).rev() {
        running += segment[i];
        m[i] = running;
    }

    (v, m)
}

/// Per-engine `(y_position_m, dry_mass_kg)` for every WING-mounted engine on
/// the modeled (positive-Y, right) semi-wing.
///
/// `spanwise_positions_m` lists BOTH wings' engines for the full aircraft (a
/// symmetric twin is `(9.8, -9.8)`); since the FEM only models one semi-wing,
/// only `y > 0` stations are returned -- otherwise a symmetric pair would
/// double-count one engine's mass onto a single semi-wing (both `+9.8` and
/// `-9.8` are the same distance from the root). A `y == 0` entry is a
/// centerline/tail-mounted engine, which loads neither wing and is skipped the
/// same way.
///
/// Reuses the same per-engine dry-mass formula as `alas-mass::breakdown`'s
/// `m_prop` (thrust / TWR / g, scaled by the installation factor). The two are
/// kept as deliberate duplicates rather than a shared helper: this one is
/// per-engine while `breakdown`'s is aggregated across the engine count, and
/// the shared quantity is the one-engine dry mass, which is short enough that
/// factoring it out would couple two crates for one multiplication. The intent
/// that they stay in sync is what this comment records.
pub fn engine_point_loads_n(
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    req: &DesignRequirements,
) -> Vec<(f64, f64)> {
    let thrust_n = engine_cfg.thrust_kn * 1000.0;
    if thrust_n <= 0.0 {
        return Vec::new();
    }
    let m_engine = (thrust_n / (mass_cfg.propulsion_twr_factor * req.gravity_m_s2))
        * mass_cfg.propulsion_installation_factor;
    engine_cfg
        .spanwise_positions_m
        .iter()
        .filter(|&&y_pos| y_pos > CENTERLINE_Y_THRESHOLD_M)
        .map(|&y_pos| (y_pos, m_engine))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirements() -> DesignRequirements {
        DesignRequirements::default()
    }

    #[test]
    fn load_cases_reproduce_the_v_n_diagram_derivation() {
        let req = DesignRequirements {
            mtow_kg: 100_000.0,
            gravity_m_s2: 9.81,
            ultimate_load_factor: 3.75,
            limit_load_factor_neg: -1.0,
            ..Default::default()
        };

        let cases = load_cases(&req, 1.0);
        let w_n = 100_000.0 * 9.81;

        assert_eq!(cases[0].name, "pull-up");
        assert_eq!(cases[0].load_factor, 3.75);
        assert_eq!(cases[0].total_force_n, 3.75 * w_n / 2.0);

        assert_eq!(cases[1].name, "push-down");
        // limit_load_factor_neg * 1.5, with the ultimate factor already folded in.
        let n_ult_neg = req.limit_load_factor_neg * 1.5;
        assert_eq!(cases[1].load_factor, n_ult_neg);
        assert_eq!(cases[1].total_force_n, n_ult_neg * w_n / 2.0);

        assert_eq!(cases[2].name, "level");
        assert_eq!(cases[2].load_factor, 1.0);
        assert_eq!(cases[2].total_force_n, w_n / 2.0);
    }

    #[test]
    fn additional_safety_factor_scales_both_ultimate_cases_but_not_the_level_case() {
        let req = requirements();
        let base = load_cases(&req, 1.0);
        let scaled = load_cases(&req, 1.5);
        assert!((scaled[0].load_factor - base[0].load_factor * 1.5).abs() < 1e-12);
        assert!((scaled[1].load_factor - base[1].load_factor * 1.5).abs() < 1e-12);
        // The 1g level case is a literal 1.0, untouched by the safety factor.
        assert_eq!(scaled[2].load_factor, 1.0);
    }

    #[test]
    fn the_elliptic_load_integrates_to_the_requested_total() {
        let semi_span = 30.0;
        let total = 800_000.0;
        let y: Vec<f64> = (0..=2000).map(|i| i as f64 * semi_span / 2000.0).collect();
        let q = elliptic_distributed_load(&y, semi_span, total);
        // Trapezoidal integral of the sampled distribution over [0, semi_span].
        let mut integral = 0.0;
        for i in 0..y.len() - 1 {
            integral += 0.5 * (q[i] + q[i + 1]) * (y[i + 1] - y[i]);
        }
        assert!((integral - total).abs() / total < 1e-4);
    }

    #[test]
    fn a_station_past_the_tip_is_clipped_to_zero_load_not_a_nan() {
        let q = elliptic_distributed_load(&[10.0, 10.000_001], 10.0, 100_000.0);
        assert_eq!(q[0], 0.0);
        assert_eq!(q[1], 0.0);
        assert!(q.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn shear_and_moment_vanish_at_the_tip_and_peak_at_the_root() {
        // A uniform load over a cantilever: shear and moment are largest at the
        // fixed root (y = 0) and zero at the free tip.
        let y: Vec<f64> = (0..=8).map(|i| i as f64 * 2.0).collect();
        let q_net = vec![5_000.0; y.len()];
        let (v, m) = cantilever_shear_moment(&y, &q_net);
        let last = y.len() - 1;
        assert_eq!(v[last], 0.0);
        assert_eq!(m[last], 0.0);
        assert!(v[0] > v[1]);
        assert!(m[0] > m[1]);
        // Root shear equals the whole integrated load: q * span.
        let span = y[last];
        assert!((v[0] - 5_000.0 * span).abs() / (5_000.0 * span) < 1e-12);
    }

    #[test]
    fn a_degenerate_single_station_integrates_to_zero() {
        let (v, m) = cantilever_shear_moment(&[3.0], &[1_000.0]);
        assert_eq!(v, vec![0.0]);
        assert_eq!(m, vec![0.0]);
    }

    #[test]
    fn zero_thrust_loads_nothing_regardless_of_positions() {
        let engine = EngineConfig {
            thrust_kn: 0.0,
            spanwise_positions_m: vec![9.8, -9.8],
            ..Default::default()
        };
        let loads = engine_point_loads_n(&engine, &MassModelConfig::default(), &requirements());
        assert!(loads.is_empty());
    }

    #[test]
    fn only_starboard_wing_stations_survive_the_semi_wing_filter() {
        // A symmetric pair, a centerline engine, and a below-threshold station.
        let engine = EngineConfig {
            thrust_kn: 350.0,
            spanwise_positions_m: vec![9.8, -9.8, 0.0, 1e-7, 22.5],
            ..Default::default()
        };
        let loads = engine_point_loads_n(&engine, &MassModelConfig::default(), &requirements());
        let positions: Vec<f64> = loads.iter().map(|&(y, _)| y).collect();
        assert_eq!(positions, vec![9.8, 22.5]);
        // Every surviving engine carries the same one-engine dry mass.
        assert_eq!(loads[0].1, loads[1].1);
        assert!(loads[0].1 > 0.0);
    }
}
