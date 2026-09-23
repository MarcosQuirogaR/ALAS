// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Stage 1: Fast 2-D NeuralFoil surrogate scoring across the airfoil database.

use alas_aero::neuralfoil::{Conditions, ModelSize, PreparedAirfoil};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::builder::AircraftBuilder;
use alas_opt::{wing_fuel_volume_m3, wing_fuel_volume_m3_reference_compatibility};

use crate::types::{AirfoilCandidateResult, MIN_NEURALFOIL_ANALYSIS_CONFIDENCE};

/// Geometry contract used by a screening evaluation.
///
/// Frozen screening fixtures replay the historical root/break/tip planform;
/// product callers retain the explicit transport-planform builder. Keeping the
/// choice at the builder boundary prevents a parity fixture from silently
/// changing the aircraft used by the product path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreeningGeometry {
    ReferenceCompatibility,
    Product,
}

fn builder_for_geometry(config: &AlasConfig, geometry: ScreeningGeometry) -> AircraftBuilder {
    match geometry {
        ScreeningGeometry::ReferenceCompatibility => {
            AircraftBuilder::new_reference_compatibility(Some(config.geometry.clone()))
        }
        ScreeningGeometry::Product => AircraftBuilder::new(Some(config.geometry.clone())),
    }
}

/// A candidate the screen could not score, with the reason it gives.
fn rejected(name: &str, error: impl Into<String>) -> AirfoilCandidateResult {
    AirfoilCandidateResult {
        name: name.to_string(),
        status: "error".to_string(),
        error: Some(error.into()),
        ..Default::default()
    }
}

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
    cruise_condition_with_geometry(config, dv, ScreeningGeometry::Product)
}

/// Compute the frozen-reference screening condition for translated fixtures.
pub fn cruise_condition_reference_compatibility(
    config: &AlasConfig,
    dv: &DesignVector,
) -> Result<(f64, f64, f64, f64), String> {
    cruise_condition_with_geometry(config, dv, ScreeningGeometry::ReferenceCompatibility)
}

pub(crate) fn cruise_condition_with_geometry(
    config: &AlasConfig,
    dv: &DesignVector,
    geometry: ScreeningGeometry,
) -> Result<(f64, f64, f64, f64), String> {
    let req = &config.requirements;
    let builder = builder_for_geometry(config, geometry);
    let plane = builder.build(Some(dv), false).map_err(|e| e.to_string())?;
    if plane.wings.is_empty() {
        return Err("aircraft geometry has no wings".to_string());
    }
    let wing = &plane.wings[0];
    let mac = wing.mean_aerodynamic_chord();
    let s = match geometry {
        ScreeningGeometry::Product => wing.reference_area(),
        // The compatibility scorer intentionally replays the historical
        // unfolded planform used by the frozen screening fixture.
        ScreeningGeometry::ReferenceCompatibility => wing.unfolded_area(),
    };

    let atmo = Atmosphere::new(req.cruise_altitude_m);
    let v = req.cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * v.powi(2);
    let weight_n = req.mtow_kg * req.gravity_m_s2;
    let cl_target = weight_n / (q * s);
    let reynolds = atmo.density() * v * mac / atmo.dynamic_viscosity();
    Ok((req.cruise_mach, reynolds, cl_target, req.cruise_altitude_m))
}

/// Build a wing with `name` as root airfoil and score it using the 2-D NeuralFoil surrogate.
// Same argument list as `score_candidate_with_geometry`, whose reason applies.
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
    score_candidate_with_geometry(
        name,
        config,
        dv,
        mach,
        reynolds,
        cl_target,
        usable_fraction,
        alphas_deg,
        model_size,
        min_tc,
        max_tc,
        cl_band,
        ScreeningGeometry::Product,
    )
}

/// Score a candidate using the frozen reference geometry for parity fixtures.
// Same argument list as `score_candidate_with_geometry`, whose reason applies.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate_reference_compatibility(
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
    score_candidate_with_geometry(
        name,
        config,
        dv,
        mach,
        reynolds,
        cl_target,
        usable_fraction,
        alphas_deg,
        model_size,
        min_tc,
        max_tc,
        cl_band,
        ScreeningGeometry::ReferenceCompatibility,
    )
}

// Each scalar and geometry argument is independently reported by screening;
// bundling them would hide which physical constraint produced a score.
#[allow(clippy::too_many_arguments)]
pub(crate) fn score_candidate_with_geometry(
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
    geometry: ScreeningGeometry,
) -> AirfoilCandidateResult {
    let mut cfg2 = config.clone();
    cfg2.geometry.wing.root_airfoil = name.to_string();

    let builder = builder_for_geometry(&cfg2, geometry);
    let plane = match builder.build(Some(dv), false) {
        Ok(p) => p,
        Err(e) => {
            return rejected(name, e.to_string());
        }
    };

    if plane.wings.is_empty() || plane.wings[0].xsecs.is_empty() {
        return rejected(name, "wing has no cross sections".to_string());
    }

    let wing = &plane.wings[0];
    let airfoil = &wing.xsecs[0].airfoil;

    let mut cl_vec = Vec::with_capacity(alphas_deg.len());
    let mut cd_vec = Vec::with_capacity(alphas_deg.len());
    let mut confidence_min = f64::INFINITY;

    // Normalize and fit the section once; the sweep only changes the flight
    // condition. Same numbers as fitting inside the loop, at a fraction of
    // the cost.
    let prepared = match PreparedAirfoil::prepare(airfoil) {
        Ok(prepared) => prepared,
        Err(e) => {
            return rejected(name, e.to_string());
        }
    };
    // One batched network pass over the whole angle schedule: the same
    // numbers as one call per angle, with every weight read once per layer.
    let conditions: Vec<Conditions> = alphas_deg
        .iter()
        .map(|&alpha| Conditions::new(alpha, reynolds))
        .collect();
    let aeros = match prepared.aero_sweep(&conditions, mach, model_size) {
        Ok(aeros) => aeros,
        Err(e) => {
            return rejected(name, e.to_string());
        }
    };
    for aero in aeros {
        if !aero.analysis_confidence.is_finite() {
            return AirfoilCandidateResult {
                analysis_confidence: Some(aero.analysis_confidence),
                ..rejected(
                    name,
                    "NeuralFoil returned non-finite analysis confidence".to_string(),
                )
            };
        }
        confidence_min = confidence_min.min(aero.analysis_confidence);
        cl_vec.push(aero.cl);
        cd_vec.push(aero.cd);
    }

    if !confidence_min.is_finite() || confidence_min < MIN_NEURALFOIL_ANALYSIS_CONFIDENCE {
        return AirfoilCandidateResult {
            analysis_confidence: Some(confidence_min),
            ..rejected(
                name,
                format!(
                    "NeuralFoil analysis confidence {:.3e} is below the screening floor {:.3e}",
                    confidence_min, MIN_NEURALFOIL_ANALYSIS_CONFIDENCE
                ),
            )
        };
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
        return rejected(name, format!(
                "target CL {:.3} outside this airfoil's swept range [{:.3}, {:.3}] (alpha {:.1}..{:.1} deg)",
                cl_target, cl_sorted[0], cl_sorted[cl_sorted.len() - 1], alphas_deg[0], alphas_deg[alphas_deg.len() - 1]
            ));
    }

    let cd_at_target = interp_linear(cl_target, &cl_sorted, &cd_sorted);
    let alpha_at_target = interp_linear(cl_target, &cl_sorted, &alpha_sorted);

    if cd_at_target <= 0.0 {
        return rejected(name, "non-physical CD <= 0 at target CL".to_string());
    }

    let sample = linspace(0.0, 1.0, 101);
    let max_t = airfoil.max_thickness(&sample);
    let tank_vol = match geometry {
        ScreeningGeometry::Product => wing_fuel_volume_m3(wing, usable_fraction),
        ScreeningGeometry::ReferenceCompatibility => {
            wing_fuel_volume_m3_reference_compatibility(wing, usable_fraction)
        }
    };
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
        return rejected(name, "non-finite result (NaN/inf)".to_string());
    }

    if !(0.005..=0.30).contains(&max_t) {
        return rejected(name, format!(
                "implausible t/c={:.1}% (outside 0.5-30% realistic range), likely a multi-element/degenerate database entry, not a usable wing section",
                max_t * 100.0
            ));
    }

    if !(min_tc..=max_tc).contains(&max_t) {
        return rejected(
            name,
            format!(
                "t/c={:.1}% is outside the requested thickness window [{:.1}%, {:.1}%]",
                max_t * 100.0,
                min_tc * 100.0,
                max_tc * 100.0
            ),
        );
    }

    if !(0.0..=0.30).contains(&cd_at_target) || l_over_d > 150.0 {
        return rejected(name, format!(
                "implausible 2-D result at target CL (CD={:.4}, L/D={:.1}), likely a NeuralFoil CST-fit breakdown for this coordinate set, not a real polar",
                cd_at_target, l_over_d
            ));
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
        analysis_confidence: Some(confidence_min),
        max_thickness_frac: Some(max_t),
        tank_volume_m3: Some(tank_vol),
        tank_capacity_kg: Some(tank_cap),
        robustness,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_cruise_cl_uses_the_projected_reference_area() {
        let config = AlasConfig::default();
        let dv = DesignVector::default();
        let (_, _, cl_product, _) =
            cruise_condition_with_geometry(&config, &dv, ScreeningGeometry::Product)
                .expect("default product screening geometry builds");
        let (_, _, cl_compatibility, _) =
            cruise_condition_with_geometry(&config, &dv, ScreeningGeometry::ReferenceCompatibility)
                .expect("default compatibility screening geometry builds");

        assert!(
            (cl_product - cl_compatibility).abs() > 1e-6,
            "dihedral must distinguish projected product and unfolded parity CL"
        );
    }
}
