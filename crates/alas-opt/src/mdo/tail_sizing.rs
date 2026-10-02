// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail auto-sizing for a reference-aircraft adaptation.
//!
//! A registered aircraft's tails are sized by its stability and control
//! requirements, which scale with the wing they serve. When the search moves
//! the wing (area, mean chord, span) the tails therefore have to follow, or
//! the aircraft silently drifts to a different stability level and the
//! objective is free to exploit it. This module keeps the tail volume
//! coefficients at the registered aircraft's own values:
//!
//! - `V_H = S_H l_H / (S_w MAC)` and `V_V = S_V l_V / (S_w b)` (Raymer,
//!   *Aircraft Design: A Conceptual Approach*, AIAA, chapter 6 on tail volume
//!   coefficients; Sadraey, *Aircraft Design: A Systems Engineering
//!   Approach*, Wiley, 2013, ch. 6), evaluated by
//!   [`alas_stab::trim::tail_volume_coefficients`] on the built planforms.
//!   Arms `l_H` and `l_V` are quarter-MAC to quarter-MAC distances along x
//!   (metres aft of the nose tip).
//! - The targets are the coefficients of the registered preset's own
//!   geometry and design vector, not a class band.
//!
//! The design vector keeps one `tail_scale`. The fin is sized first, because
//! `V_V` does not depend on the tailplane; its absolute scale is carried as
//! `EmpennageConfig::vstab_scale_ratio` times `tail_scale`. The tailplane is
//! then solved with the fin scale held fixed, because on a T-tail the
//! tailplane root rides on the fin tip and its arm depends on the fin.
//! Each solve is a secant iteration on `ln V` against `ln scale`, where the
//! dependence is close to `V proportional to scale^2`.
//!
//! Only the planform area and quarter-MAC position enter the solve, so it
//! uses the unmeshed empennage build.

use std::sync::OnceLock;

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, EmpennageConfig, TailSizing};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_stab::trim::tail_volume_coefficients;

use super::types::CandidateFailure;

/// Relative convergence of each solved coefficient.
const VOLUME_TOLERANCE: f64 = 1.0e-10;
/// Secant iterations allowed per surface.
const MAX_ITERATIONS: usize = 40;
/// Smallest and largest tail scale the solve will consider.
const SCALE_RANGE: (f64, f64) = (0.05, 20.0);
/// Largest change of `ln scale` in one step, which is a factor of two.
const MAX_LOG_STEP: f64 = std::f64::consts::LN_2;

/// A `(V_H, V_V)` pair.
type TailVolumes = (f64, f64);

/// Nominal `(V_H, V_V)` of every registered preset, built once.
fn nominal_volumes(preset: &str) -> Option<TailVolumes> {
    static NOMINALS: OnceLock<Vec<(&'static str, Option<TailVolumes>)>> = OnceLock::new();
    NOMINALS
        .get_or_init(|| {
            alas_config::presets::registry()
                .iter()
                .map(|registered| {
                    let plane = AircraftBuilder::new(Some(registered.geometry.clone()))
                        .build(Some(&registered.design_vector), false)
                        .ok();
                    let volumes = plane.and_then(|plane| match tail_volume_coefficients(&plane) {
                        (Some(vh), Some(vv)) if vh > 0.0 && vv > 0.0 => Some((vh, vv)),
                        _ => None,
                    });
                    (registered.name, volumes)
                })
                .collect()
        })
        .iter()
        .find(|(name, _)| *name == preset)
        .and_then(|(_, volumes)| *volumes)
}

/// Whether `config` asks for tail auto-sizing: a reference adaptation of a
/// registered aircraft whose tail volumes are known.
fn targets(config: &AlasConfig) -> Option<TailVolumes> {
    (config.optimizer.design_space.mode == DesignMode::ReferenceAdaptation)
        .then(|| nominal_volumes(&config.preset))
        .flatten()
}

/// The main wing and reference quantities the coefficients are measured
/// against, with placeholder tails that each trial replaces.
struct Frame {
    builder: AircraftBuilder,
    plane: Airplane,
}

impl Frame {
    fn volumes(&mut self, dv: &DesignVector, ratio: f64) -> Option<(f64, f64)> {
        self.builder.geometry.empennage.vstab_scale_ratio = ratio;
        let (hstab, vstab) = self.builder.build_empennage(dv).ok()?;
        self.plane.wings[1] = hstab;
        self.plane.wings[2] = vstab;
        match tail_volume_coefficients(&self.plane) {
            (Some(vh), Some(vv)) if vh.is_finite() && vv.is_finite() => Some((vh, vv)),
            _ => None,
        }
    }
}

/// Solve `f(scale) = target` for a positive increasing `f` by secant
/// iteration in log-log space, starting from `start`.
fn solve_log_secant(mut f: impl FnMut(f64) -> Option<f64>, start: f64, target: f64) -> Option<f64> {
    let residual = |value: f64| (value > 0.0).then(|| value.ln() - target.ln());
    let clamp = |x: f64| x.clamp(SCALE_RANGE.0.ln(), SCALE_RANGE.1.ln());
    let mut x0 = clamp(start.ln());
    let mut y0 = residual(f(x0.exp())?)?;
    if y0.abs() < VOLUME_TOLERANCE {
        return Some(x0.exp());
    }
    // A coefficient is an area times an arm over wing quantities, so its
    // log-log slope is close to two; that seeds the first step.
    let mut x1 = clamp(x0 - y0 / 2.0);
    for _ in 0..MAX_ITERATIONS {
        let y1 = residual(f(x1.exp())?)?;
        if y1.abs() < VOLUME_TOLERANCE {
            return Some(x1.exp());
        }
        let slope = (y1 - y0) / (x1 - x0);
        let slope = if slope.is_finite() && slope > 0.1 {
            slope
        } else {
            2.0
        };
        let step = (-y1 / slope).clamp(-MAX_LOG_STEP, MAX_LOG_STEP);
        (x0, y0) = (x1, y1);
        x1 = clamp(x1 + step);
    }
    None
}

/// The tail scales that restore `targets = (V_H, V_V)` on the wing built
/// from `dv` and `config_geometry`'s main wing.
fn solve(
    config: &AlasConfig,
    dv: &DesignVector,
    (target_h, target_v): (f64, f64),
) -> Option<TailSizing> {
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let mut reference = builder.build(Some(dv), false).ok()?;
    let main_wing = reference.wings.first()?.clone();
    reference.wings = vec![main_wing; 3];
    let mut frame = Frame {
        builder,
        plane: reference,
    };
    let start = if dv.tail_scale.is_finite() && dv.tail_scale > 0.0 {
        dv.tail_scale
    } else {
        1.0
    };
    // The fin first: its coefficient does not depend on the tailplane.
    let fin_scale = solve_log_secant(
        |scale| {
            let trial = DesignVector {
                tail_scale: scale,
                ..*dv
            };
            frame.volumes(&trial, 1.0).map(|(_, vv)| vv)
        },
        start,
        target_v,
    )?;
    // Then the tailplane, with the fin held at its solved absolute scale.
    let tail_scale = solve_log_secant(
        |scale| {
            let trial = DesignVector {
                tail_scale: scale,
                ..*dv
            };
            frame.volumes(&trial, fin_scale / scale).map(|(vh, _)| vh)
        },
        start,
        target_h,
    )?;
    Some(TailSizing {
        tail_scale,
        vstab_scale_ratio: fin_scale / tail_scale,
    })
}

/// Size the tails of a reference-adaptation candidate in place.
///
/// Writes the solved `tail_scale` into `dv` and the fin ratio into the
/// candidate's geometry. Any other mode, or a configuration that does not
/// name a registered aircraft, is left unchanged, and so is a candidate
/// whose geometry has no tails to size.
///
/// # Errors
///
/// `geometry_build` when the solve does not converge inside the scale range,
/// which means the candidate cannot carry the nominal tail volumes.
pub(crate) fn apply(
    config: &mut AlasConfig,
    dv: &mut DesignVector,
) -> Result<(), CandidateFailure> {
    config.geometry.empennage.vstab_scale_ratio = 1.0;
    let Some(volumes) = targets(config) else {
        return Ok(());
    };
    let scales = solve(config, dv, volumes).ok_or(CandidateFailure {
        reason: "geometry_build",
    })?;
    scales.apply_to(&mut config.geometry.empennage, dv);
    Ok(())
}

/// The fin scale ratio a built aircraft carries, recovered from its fin root
/// chord, for carrying a derived geometry into a replay.
///
/// Returns one when the aircraft has no fin or the tail scale is not
/// positive.
pub(crate) fn fin_scale_ratio(
    plane: &Airplane,
    empennage: &EmpennageConfig,
    dv: &DesignVector,
) -> f64 {
    let fin_root_chord = plane
        .wings
        .get(2)
        .and_then(|fin| fin.xsecs.first())
        .map(|section| section.chord);
    match fin_root_chord {
        Some(chord)
            if chord > 0.0
                && empennage.vstab_root_chord_m > 0.0
                && dv.tail_scale > 0.0
                && dv.tail_scale.is_finite() =>
        {
            chord / empennage.vstab_root_chord_m / dv.tail_scale
        }
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests;
