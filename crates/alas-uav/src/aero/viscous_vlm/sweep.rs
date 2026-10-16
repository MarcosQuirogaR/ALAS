// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Angle sweeps: wing `CLmax`, the first stalled strip and the 2-D stall data
//! behind them.
//!
//! A strip is called stalled when its section lift reaches 97 % of the
//! section's own `cl_max` at the strip Reynolds number, or its effective angle
//! reaches the angle of that maximum.  The coupled fixed point loses its
//! solution (a fold) shortly after the first strips stall; if no strip met the
//! criterion before the fold, the strip closest to its `cl_max` at the last
//! solved point is reported with [`StallCriterion::Fold`].

use crate::aero::air::AirState;
use crate::aero::slipstream::Slipstream;
use crate::aero::AeroError;

use super::layout::StripGeom;
use super::types::{ClMax, FirstStall, StallCriterion, ViscousSolution};
use super::ViscousVlm;

/// Fraction of the 2-D `cl_max` at which a strip counts as stalled.
const STALL_UTILISATION: f64 = 0.97;

/// The strip closest to stall in one solution.
#[derive(Debug, Clone, Copy)]
struct Peak {
    y_m: f64,
    utilisation: f64,
    angle_reached: bool,
}

impl Peak {
    fn stalled(&self) -> bool {
        self.angle_reached || self.utilisation >= STALL_UTILISATION
    }
}

impl<'a> ViscousVlm<'a> {
    /// Two-dimensional `(cl_max, alpha_clmax)` of strip `index` at its Re
    /// (cached; blended between the two bracketing cross-sections).
    fn strip_cl_max(
        &self,
        g: &StripGeom,
        index: usize,
        re: f64,
        mach: f64,
    ) -> Result<(f64, f64), AeroError> {
        // Quantise Re to 1000 so repeated sweeps at one speed hit the cache.
        let key = (index, ((re * 1.0e-3).round() as u64).max(1));
        if let Ok(cache) = self.stall_cache.lock() {
            if let Some(v) = cache.get(&key) {
                return Ok(*v);
            }
        }
        let (c0, a0) = self.wing_sections[g.section].cl_max(re, mach)?;
        let (c1, a1) = if g.weight > 0.0 {
            self.wing_sections[g.section + 1].cl_max(re, mach)?
        } else {
            (c0, a0)
        };
        let t = g.weight;
        let value = ((1.0 - t) * c0 + t * c1, (1.0 - t) * a0 + t * a1);
        if let Ok(mut cache) = self.stall_cache.lock() {
            cache.insert(key, value);
        }
        Ok(value)
    }

    /// The most heavily loaded strip of `sol` relative to its own `cl_max`.
    fn stall_peak(&self, sol: &ViscousSolution, air: &AirState) -> Result<Option<Peak>, AeroError> {
        let geoms = self.layout.get().map_or(&[][..], Vec::as_slice);
        let mut peak: Option<Peak> = None;
        for (i, (s, g)) in sol.strips.iter().zip(geoms).enumerate() {
            let mach = air.mach() * s.q_ratio.sqrt();
            let (cl_max, a_max) = self.strip_cl_max(g, i, s.re, mach)?;
            let candidate = Peak {
                y_m: s.y_m,
                utilisation: s.cl_section / cl_max,
                angle_reached: s.alpha_eff_rad >= a_max,
            };
            let better = peak.is_none_or(|p| {
                (candidate.angle_reached, candidate.utilisation) > (p.angle_reached, p.utilisation)
            });
            if better {
                peak = Some(candidate);
            }
        }
        Ok(peak)
    }

    /// Sweep angle of attack upward from `start_rad` in steps of `step_rad`
    /// to `stop_rad` (warm-started), find the maximum `CL` and the first
    /// stalled strip.  The sweep ends early after `CL` has fallen 0.05 below
    /// its maximum for two consecutive steps, or when a strip leaves the
    /// section-model domain ([`ClMax::terminated_by`]).
    ///
    /// # Errors
    ///
    /// As [`Self::solve`]; `step_rad` must be positive and `stop_rad >=
    /// start_rad`.
    pub fn cl_max(
        &self,
        air: &AirState,
        slipstream: Option<&Slipstream>,
        start_rad: f64,
        stop_rad: f64,
        step_rad: f64,
    ) -> Result<ClMax, AeroError> {
        if !(step_rad > 0.0 && stop_rad >= start_rad) {
            return Err(AeroError::InvalidInput("bad angle sweep".into()));
        }
        let semispan = 0.5 * self.airplane.b_ref;
        let mut warm: Option<Vec<f64>> = None;
        let mut curve = Vec::new();
        let mut first_stall: Option<FirstStall> = None;
        let mut last_peak: Option<(f64, Peak)> = None;
        let mut best = (f64::NEG_INFINITY, start_rad, false);
        let mut below = 0;
        let mut terminated_by = None;
        let mut alpha = start_rad;
        while alpha <= stop_rad + 1e-12 {
            let sol = match self.solve_from(air, alpha, slipstream, warm.as_deref()) {
                Ok(sol) => sol,
                Err(AeroError::OutsideDomain(why)) if !curve.is_empty() => {
                    terminated_by = Some(why);
                    break;
                }
                Err(e) => return Err(e),
            };
            if sol.converged {
                if let Some(peak) = self.stall_peak(&sol, air)? {
                    if first_stall.is_none() && peak.stalled() {
                        first_stall = Some(FirstStall {
                            y_m: peak.y_m,
                            alpha_rad: alpha,
                            tip_stall: peak.y_m.abs() > 0.6 * semispan,
                            criterion: if peak.angle_reached {
                                StallCriterion::AngleOfClMax
                            } else {
                                StallCriterion::Utilisation
                            },
                        });
                    }
                    last_peak = Some((alpha, peak));
                }
            }
            curve.push((alpha, sol.cl, sol.converged));
            if sol.cl > best.0 {
                best = (sol.cl, alpha, sol.converged);
                below = 0;
            } else if sol.cl < best.0 - 0.05 {
                below += 1;
                if below >= 2 {
                    break;
                }
            }
            if sol.converged {
                warm = Some(sol.offsets_rad);
            }
            alpha += step_rad;
        }
        if first_stall.is_none() && (terminated_by.is_some() || below > 0) {
            if let Some((a, peak)) = last_peak {
                first_stall = Some(FirstStall {
                    y_m: peak.y_m,
                    alpha_rad: a,
                    tip_stall: peak.y_m.abs() > 0.6 * semispan,
                    criterion: StallCriterion::Fold,
                });
            }
        }
        Ok(ClMax {
            cl_max: best.0,
            alpha_rad: best.1,
            band: 0.1,
            first_stall,
            converged_at_max: best.2,
            curve,
            terminated_by,
        })
    }
}
