// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Stage 1: Fast 2-D NeuralFoil surrogate scoring across the airfoil database.

use alas_aero::neuralfoil::{aero_from_airfoil, Conditions, ModelSize};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::builder::AircraftBuilder;
use alas_opt::wing_fuel_volume_m3;

use crate::types::AirfoilCandidateResult;

/// Piecewise linear interpolation matching `numpy.interp(x, xp, yp)` with sorted `xp`.
pub fn interp_linear(x: f64, xp: &[f64], yp: &[f64]) -> f64 {
    if xp.is_empty() || yp.is_empty() {
        return 0.0;
    }
    if x <= xp[0] {
        return yp[0];
    }
    if x >= xp[xp.len() - 1] {
        return yp[yp.len() - 1];
    }
    for i in 0..xp.len() - 1 {
        if x >= xp[i] && x <= xp[i + 1] {
            let dx = xp[i + 1] - xp[i];
            if dx.abs() < 1e-12 {
                return yp[i];
            }
            let t = (x - xp[i]) / dx;
            return yp[i] + t * (yp[i + 1] - yp[i]);
        }
    }
    yp[0]
}

/// Compute level-flight cruise condition: (mach, reynolds, cl_target, altitude_m).
pub fn cruise_condition(
    config: &AlasConfig,
    dv: &DesignVector,
) -> Result<(f64, f64, f64, f64), String> {
    let req = &config.requirements;
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let plane = builder.build(Some(dv), false).map_err(|e| e.to_string())?;
    if plane.wings.is_empty() {
        return Err("aircraft geometry has no wings".to_string());
    }
    let wing = &plane.wings[0];
    let mac = wing.mean_aerodynamic_chord();
    let s = wing.area();

    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let v = req.cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * v.powi(2);
    let weight_n = req.mtow_kg * req.gravity_m_s2;
    let cl_target = weight_n / (q * s);
    let reynolds = atmo.density() * v * mac / atmo.dynamic_viscosity();
    Ok((req.cruise_mach, reynolds, cl_target, req.cruise_altitude_m))
}

/// Build a wing with `name` as root airfoil and score it using the 2-D NeuralFoil surrogate.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate(
    name: &str,
    config: &AlasConfig,
    dv: &DesignVector,
    mach: f64,
    reynolds: f64,
    cl_target: f64,
    usable_fraction: f64,
    alphas_deg: &[f64],
    model_size: ModelSize,
    min_tc: f64,
    max_tc: f64,
    cl_band: f64,
) -> AirfoilCandidateResult {
    let mut cfg2 = config.clone();
    cfg2.geometry.wing.root_airfoil = name.to_string();

    let builder = AircraftBuilder::new(Some(cfg2.geometry.clone()));
    let plane = match builder.build(Some(dv), false) {
        Ok(p) => p,
        Err(e) => {
            return AirfoilCandidateResult {
                name: name.to_string(),
                status: "error".to_string(),
                error: Some(e.to_string()),
                ..Default::default()
            };
        }
    };

    if plane.wings.is_empty() || plane.wings[0].xsecs.is_empty() {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some("wing has no cross sections".to_string()),
            ..Default::default()
        };
    }

    let wing = &plane.wings[0];
    let airfoil = &wing.xsecs[0].airfoil;

    let mut cl_vec = Vec::with_capacity(alphas_deg.len());
    let mut cd_vec = Vec::with_capacity(alphas_deg.len());

    for &alpha in alphas_deg {
        let cond = Conditions::new(alpha, reynolds);
        match aero_from_airfoil(airfoil, &cond, mach, model_size) {
            Ok(aero) => {
                cl_vec.push(aero.cl);
                cd_vec.push(aero.cd);
            }
            Err(e) => {
                return AirfoilCandidateResult {
                    name: name.to_string(),
                    status: "error".to_string(),
                    error: Some(e.to_string()),
                    ..Default::default()
                };
            }
        }
    }

    let mut indexed: Vec<(f64, f64, f64)> = cl_vec
        .iter()
        .zip(&cd_vec)
        .zip(alphas_deg)
        .map(|((&cl, &cd), &alpha)| (cl, cd, alpha))
        .collect();

    indexed.sort_by(|a, b| a.0.total_cmp(&b.0));

    let cl_sorted: Vec<f64> = indexed.iter().map(|p| p.0).collect();
    let cd_sorted: Vec<f64> = indexed.iter().map(|p| p.1).collect();
    let alpha_sorted: Vec<f64> = indexed.iter().map(|p| p.2).collect();

    if cl_target < cl_sorted[0] || cl_target > cl_sorted[cl_sorted.len() - 1] {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some(format!(
                "target CL {:.3} outside this airfoil's swept range [{:.3}, {:.3}] (alpha {:.1}..{:.1} deg)",
                cl_target, cl_sorted[0], cl_sorted[cl_sorted.len() - 1], alphas_deg[0], alphas_deg[alphas_deg.len() - 1]
            )),
            ..Default::default()
        };
    }

    let cd_at_target = interp_linear(cl_target, &cl_sorted, &cd_sorted);
    let alpha_at_target = interp_linear(cl_target, &cl_sorted, &alpha_sorted);

    if cd_at_target <= 0.0 {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some("non-physical CD <= 0 at target CL".to_string()),
            ..Default::default()
        };
    }

    let sample = linspace(0.0, 1.0, 101);
    let max_t = airfoil.max_thickness(&sample);
    let tank_vol = wing_fuel_volume_m3(wing, usable_fraction);
    let tank_cap = tank_vol * config.mass_model.fuel_density_kg_m3;
    let l_over_d = cl_target / cd_at_target;

    let values = [
        l_over_d,
        cl_target,
        cd_at_target,
        alpha_at_target,
        max_t,
        tank_vol,
        tank_cap,
    ];
    if !values.iter().all(|v| v.is_finite()) {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some("non-finite result (NaN/inf)".to_string()),
            ..Default::default()
        };
    }

    if !(0.005..=0.30).contains(&max_t) {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some(format!(
                "implausible t/c={:.1}% (outside 0.5-30% realistic range) -- likely a multi-element/degenerate database entry, not a usable wing section",
                max_t * 100.0
            )),
            ..Default::default()
        };
    }

    if !(min_tc..=max_tc).contains(&max_t) {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some(format!(
                "t/c={:.1}% is outside the requested thickness window [{:.1}%, {:.1}%]",
                max_t * 100.0,
                min_tc * 100.0,
                max_tc * 100.0
            )),
            ..Default::default()
        };
    }

    if !(0.0..=0.30).contains(&cd_at_target) || l_over_d > 150.0 {
        return AirfoilCandidateResult {
            name: name.to_string(),
            status: "error".to_string(),
            error: Some(format!(
                "implausible 2-D result at target CL (CD={:.4}, L/D={:.1}) -- likely a NeuralFoil CST-fit breakdown for this coordinate set, not a real polar",
                cd_at_target, l_over_d
            )),
            ..Default::default()
        };
    }

    // Off-design robustness evaluation
    let cl_lo = (cl_sorted[0]).max(cl_target - cl_band);
    let cl_hi = (cl_sorted[cl_sorted.len() - 1]).min(cl_target + cl_band);
    let cd_lo = interp_linear(cl_lo, &cl_sorted, &cd_sorted);
    let cd_hi = interp_linear(cl_hi, &cl_sorted, &cd_sorted);
    let mut robustness = None;
    if cd_lo > 0.0 && cd_hi > 0.0 {
        let ld_off = 0.5 * (cl_lo / cd_lo + cl_hi / cd_hi);
        let ratio = ld_off / l_over_d;
        if ratio.is_finite() {
            robustness = Some(ratio.clamp(0.0, 1.5));
        }
    }

    AirfoilCandidateResult {
        name: name.to_string(),
        status: "ok".to_string(),
        l_over_d: Some(l_over_d),
        cl: Some(cl_target),
        cd: Some(cd_at_target),
        alpha_deg: Some(alpha_at_target),
        max_thickness_frac: Some(max_t),
        tank_volume_m3: Some(tank_vol),
        tank_capacity_kg: Some(tank_cap),
        robustness,
        ..Default::default()
    }
}
