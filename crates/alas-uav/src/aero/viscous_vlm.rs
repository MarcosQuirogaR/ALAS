// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Viscous VLM: lattice circulation coupled to NeuralFoil section polars.
//!
//! # Method (modified-incidence fixed point, UAV-PLAN 3.1 items 1-4)
//!
//! The production lattice (`alas-aero::vlm`) gives the inviscid circulation of
//! the main wing.  For each spanwise strip `i` (chord `c_i`, width `w_i`):
//!
//! 1. the lattice strip lift gives `cl_vlm,i = L_i / (q_op c_i w_i)`;
//! 2. the physical effective angle of the strip (chord-relative, geometry plus
//!    induced, no correction) is
//!    `alpha_eff,i = alpha0L,i + cl_vlm,i / a_i - delta_i - d_alpha_slip,i`
//!    where `a_i`, `alpha0L,i` are the strip's discrete thin-airfoil section
//!    law (see `layout`), `delta_i` the incidence correction already applied and
//!    `d_alpha_slip,i` the slipstream flow-angle reduction;
//! 3. the section model returns `cl_2D,i(alpha_eff,i, Re_i, Mach_i)` at the
//!    local Reynolds number `Re_i = rho V k_i c_i / mu` (`k_i` the local speed
//!    ratio, 1 outside a slipstream); the target lift on the freestream
//!    dynamic pressure is `cl*_i = q_ratio,i cl_2D,i`;
//! 4. `delta_i <- delta_i + omega (cl*_i - cl_vlm,i) / a_i` with relaxation
//!    `omega` (0.5 default, halved when the residual grows), and the lattice is
//!    re-solved with `delta` as per-panel incidence offsets
//!    ([`alas_aero::vlm::VlmSystem::solve_with_incidence_offsets`]).
//!
//! Converged when `max_i |cl*_i - cl_vlm,i| < tolerance` (1e-3).  In the
//! two-dimensional limit (no induced angle) the first update is exact; with
//! finite span the induced-angle feedback is what the iteration resolves.  The
//! converged circulation gives `CL`, `CM` (lattice moment: the viscous section
//! moment is NOT coupled) and the Trefftz-plane induced drag.  Profile drag is
//! `sum_i q_ratio,i cd_2D,i c_i w_i / S_ref`.
//!
//! Only the main wing (wing index 0) is viscously corrected; other surfaces in
//! the airplane (tails) enter the lattice inviscidly.  Non-wing parasite drag
//! comes from [`super::drag_lowre`].
//!
//! # Validity (enforced, not only documented)
//!
//! Every strip query is classified by [`super::validity::Limits`] (section Re
//! `>= 8e4` ok, `2e4..8e4` warn, below refused; `|alpha_eff|` `<= 20 deg`;
//! Mach `<= 0.12`; network confidence).  With
//! [`ViscousConfig::refuse_outside_domain`] a refused strip returns
//! [`AeroError::OutsideDomain`]; otherwise the flag is carried in the
//! solution.  Not modelled: sweep corrections to the strip Re and angle
//! (use for sweep below about 15 deg), control-surface deflection, wing-body
//! interference, ground effect (apply [`super::ground_effect`] to the result),
//! swirl.  The converged state past stall can fail to converge; the flag
//! [`Flag::NotConverged`] is then set and `CL` must not be trusted beyond
//! the band stated for `CLmax` (+-0.1, plan 3.1, ESTIMATE).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use alas_aero::operating_point::OperatingPoint;
use alas_aero::strips::wing_strips;
use alas_aero::vlm::{VlmResult, VlmSystem};
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;

use super::air::AirState;
use super::section::{SectionModel, SectionPoint};
use super::slipstream::{Slipstream, StripFlow};
use super::validity::{Flag, Severity, Validity};
use super::AeroError;

mod layout;
mod sweep;
mod types;
use layout::StripGeom;
pub use types::{ClMax, FirstStall, StallCriterion, StripResult, ViscousSolution};

/// Solver settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViscousConfig {
    /// Target number of spanwise strips per half-span.  Each pair of adjacent
    /// wing cross-sections is subdivided `round(target / pairs)` times
    /// (at least once, cosine spaced), so a wing described by many
    /// cross-sections is not over-refined.  Induced drag converges as
    /// roughly `1/N`: for a rectangular `A = 8` wing the default 24 changes
    /// `CL` by 0.5 % and `CD` by 1.7 % against 48 (test
    /// `mesh_refinement_...`); the lattice `e` of a rectangular wing
    /// converges slowly above `A` ~ 15 (tip loading), so expect `CDi`
    /// errors of 10-20 % there until the strips are refined.
    pub spanwise_strips: usize,
    /// Chordwise panels per strip.
    pub chordwise_resolution: usize,
    /// Initial relaxation factor of the incidence update.
    pub relaxation: f64,
    /// Convergence tolerance on `max |cl* - cl_vlm|`.
    pub tolerance: f64,
    /// Iteration limit.
    pub max_iterations: usize,
    /// Return an error when a strip leaves the section-model domain by more
    /// than a warning.
    pub refuse_outside_domain: bool,
}

impl Default for ViscousConfig {
    fn default() -> Self {
        Self {
            spanwise_strips: 24,
            chordwise_resolution: 4,
            relaxation: 0.5,
            tolerance: 1.0e-3,
            max_iterations: 120,
            refuse_outside_domain: true,
        }
    }
}

/// The coupled solver for one wing.
pub struct ViscousVlm<'a> {
    wing_sections: Vec<SectionModel>,
    config: ViscousConfig,
    airplane: &'a Airplane,
    system: VlmSystem<'a>,
    layout: OnceLock<Vec<StripGeom>>,
    stall_cache: Mutex<HashMap<(usize, u64), (f64, f64)>>,
}

fn lattice(e: alas_aero::vlm::VlmError) -> AeroError {
    AeroError::Lattice(e.to_string())
}

impl<'a> ViscousVlm<'a> {
    /// Assemble the lattice of `airplane` (the main wing is `wings[0]`;
    /// `sections` holds one section model per cross-section of that wing).
    ///
    /// # Errors
    ///
    /// [`AeroError::InvalidInput`] for a section-count mismatch or bad
    /// settings, [`AeroError::Lattice`] if the lattice cannot be assembled.
    pub fn new(
        airplane: &'a Airplane,
        sections: Vec<SectionModel>,
        config: ViscousConfig,
    ) -> Result<Self, AeroError> {
        let wing = airplane
            .wings
            .first()
            .ok_or_else(|| AeroError::InvalidInput("the airplane has no wing".into()))?;
        if sections.len() != wing.xsecs.len() {
            return Err(AeroError::InvalidInput(format!(
                "{} section models for {} wing cross-sections",
                sections.len(),
                wing.xsecs.len()
            )));
        }
        let ok = config.spanwise_strips >= 1
            && config.chordwise_resolution >= 1
            && config.relaxation > 0.0
            && config.relaxation <= 1.0
            && config.tolerance > 0.0
            && config.max_iterations >= 1;
        if !ok {
            return Err(AeroError::InvalidInput(format!("bad settings {config:?}")));
        }
        let pairs = wing.xsecs.len().saturating_sub(1).max(1);
        let subdivisions = ((config.spanwise_strips as f64 / pairs as f64).round() as usize).max(1);
        let system = VlmSystem::assemble(airplane, subdivisions, config.chordwise_resolution)
            .map_err(lattice)?;
        Ok(Self {
            wing_sections: sections,
            config,
            airplane,
            system,
            layout: OnceLock::new(),
            stall_cache: Mutex::new(HashMap::new()),
        })
    }

    fn op_point(air: &AirState, alpha_rad: f64) -> OperatingPoint {
        // Coefficients depend on the velocity direction only; the sea-level
        // atmosphere merely supplies a reference density for the lattice.
        OperatingPoint::new(
            Atmosphere::new(0.0),
            air.v_m_s,
            alpha_rad.to_degrees(),
            0.0,
            0.0,
            0.0,
            0.0,
        )
    }

    fn layout(
        &self,
        result: &VlmResult,
        op: &OperatingPoint,
    ) -> Result<&Vec<StripGeom>, AeroError> {
        if let Some(layout) = self.layout.get() {
            return Ok(layout);
        }
        let strips = wing_strips(result, op, 0);
        let built = layout::build(&self.airplane.wings[0], &strips, &result.panels)?;
        Ok(self.layout.get_or_init(|| built))
    }

    /// Inviscid reference solution (no section coupling) at `alpha_rad`.
    ///
    /// # Errors
    ///
    /// [`AeroError::Lattice`].
    pub fn solve_inviscid(&self, air: &AirState, alpha_rad: f64) -> Result<VlmResult, AeroError> {
        self.system
            .solve(&Self::op_point(air, alpha_rad))
            .map_err(lattice)
    }

    fn blend(
        &self,
        g: &StripGeom,
        alpha: f64,
        re: f64,
        mach: f64,
    ) -> Result<SectionPoint, AeroError> {
        let lo = self.wing_sections[g.section].eval(alpha, re, mach)?;
        if g.weight <= 0.0 {
            return Ok(lo);
        }
        let hi = self.wing_sections[g.section + 1].eval(alpha, re, mach)?;
        let mut validity = lo.validity.clone();
        validity.extend(&hi.validity);
        let t = g.weight;
        Ok(SectionPoint {
            cl: (1.0 - t) * lo.cl + t * hi.cl,
            cd: (1.0 - t) * lo.cd + t * hi.cd,
            cm: (1.0 - t) * lo.cm + t * hi.cm,
            validity,
        })
    }

    /// Solve at `alpha_rad` from zero incidence offsets.
    ///
    /// # Errors
    ///
    /// [`AeroError::OutsideDomain`] when a strip is refused (and the setting
    /// is on), [`AeroError::Lattice`], [`AeroError::Section`].
    pub fn solve(
        &self,
        air: &AirState,
        alpha_rad: f64,
        slipstream: Option<&Slipstream>,
    ) -> Result<ViscousSolution, AeroError> {
        self.solve_from(air, alpha_rad, slipstream, None)
    }

    /// Solve at `alpha_rad` starting from the offsets of a neighbouring
    /// solution (`warm`, one per strip) - used by sweeps.
    ///
    /// # Errors
    ///
    /// As [`Self::solve`].
    pub fn solve_from(
        &self,
        air: &AirState,
        alpha_rad: f64,
        slipstream: Option<&Slipstream>,
        warm: Option<&[f64]>,
    ) -> Result<ViscousSolution, AeroError> {
        if !alpha_rad.is_finite() {
            return Err(AeroError::InvalidInput("alpha must be finite".into()));
        }
        let op = Self::op_point(air, alpha_rad);
        let q_op = op.dynamic_pressure();
        let mut result = self.system.solve(&op).map_err(lattice)?;
        let geoms = self.layout(&result, &op)?;
        let n = geoms.len();
        let flows: Vec<StripFlow> = geoms
            .iter()
            .map(|g| match slipstream {
                Some(s) => s.strip_flow(
                    air,
                    g.quarter_chord[0],
                    g.strip.y_m,
                    g.quarter_chord[2],
                    g.strip.width_m,
                    alpha_rad,
                ),
                None => StripFlow::NONE,
            })
            .collect();
        let mut delta = match warm {
            Some(w) if w.len() == n => w.to_vec(),
            _ => vec![0.0; n],
        };
        let mut offsets = vec![0.0; self.system.panel_count()];
        let mut relax = self.config.relaxation;
        let mut last_residual = f64::INFINITY;
        let mut iterations = 0;
        let mut strips_out: Vec<StripResult> = Vec::new();
        let mut residual = f64::INFINITY;
        let mut converged = false;
        for it in 0..self.config.max_iterations {
            iterations = it + 1;
            if it > 0 || warm.is_some() {
                for (g, d) in geoms.iter().zip(&delta) {
                    for o in
                        &mut offsets[g.strip.first_panel..g.strip.first_panel + g.strip.panel_count]
                    {
                        *o = *d;
                    }
                }
                result = self
                    .system
                    .solve_with_incidence_offsets(&op, &offsets)
                    .map_err(lattice)?;
            }
            let strips = wing_strips(&result, &op, 0);
            strips_out.clear();
            let mut worst = 0.0_f64;
            let mut update = vec![0.0; n];
            for (i, g) in geoms.iter().enumerate() {
                let s = &strips[i];
                let cl_vlm = s.lift_n / (q_op * s.chord_m * s.width_m);
                let flow = flows[i];
                let alpha_eff = g.alpha0l + cl_vlm / g.a - delta[i] - flow.d_alpha_rad;
                let speed = flow.speed_ratio();
                let re = air.reynolds(s.chord_m) * speed;
                let mach = air.mach() * speed;
                let sec = self.blend(g, alpha_eff, re, mach)?;
                let target = flow.q_ratio * sec.cl;
                let err = target - cl_vlm;
                worst = worst.max(err.abs());
                update[i] = err / g.a;
                strips_out.push(StripResult {
                    y_m: s.y_m,
                    chord_m: s.chord_m,
                    width_m: s.width_m,
                    re,
                    alpha_eff_rad: alpha_eff,
                    cl_vlm,
                    cl_section: sec.cl,
                    cd_section: sec.cd,
                    cm_section: sec.cm,
                    q_ratio: flow.q_ratio,
                    coverage: flow.coverage,
                    validity: sec.validity,
                });
            }
            residual = worst;
            if !worst.is_finite() {
                return Err(AeroError::Lattice("non-finite strip state".into()));
            }
            if worst < self.config.tolerance {
                converged = true;
                break;
            }
            if worst > 1.5 * last_residual {
                relax = (0.5 * relax).max(0.05);
            }
            last_residual = worst;
            for (d, u) in delta.iter_mut().zip(&update) {
                *d += relax * u;
            }
        }
        self.finish(
            air, alpha_rad, &op, &result, strips_out, delta, iterations, residual, converged,
        )
    }

    // The nine arguments are the loop state handed over once, at the end of
    // the iteration; a carrier struct would only rename them.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        air: &AirState,
        alpha_rad: f64,
        op: &OperatingPoint,
        result: &VlmResult,
        strips: Vec<StripResult>,
        offsets_rad: Vec<f64>,
        iterations: usize,
        residual: f64,
        converged: bool,
    ) -> Result<ViscousSolution, AeroError> {
        let s_ref = self.airplane.s_ref;
        let cd_induced = self
            .system
            .trefftz_induced_drag_coefficient(result, op)
            .ok_or_else(|| {
                AeroError::Lattice("the Trefftz-plane drag operator is unavailable".into())
            })?;
        let cd_profile: f64 = strips
            .iter()
            .map(|s| s.q_ratio * s.cd_section * s.chord_m * s.width_m)
            .sum::<f64>()
            / s_ref;
        let mut validity = Validity::default();
        for s in &strips {
            validity.extend(&s.validity);
        }
        if !converged {
            validity.push(Severity::Warn, Flag::NotConverged(residual));
        }
        if self.config.refuse_outside_domain {
            if let Some((_, flag)) = validity
                .flags
                .iter()
                .find(|(sev, _)| *sev == Severity::Refused)
            {
                return Err(AeroError::OutsideDomain(flag.to_string()));
            }
        }
        let aspect = self.airplane.b_ref * self.airplane.b_ref / s_ref;
        let cl = result.cl_lift;
        let span_efficiency = (cl.abs() > 1e-3 && cd_induced > 1e-9)
            .then(|| cl * cl / (std::f64::consts::PI * aspect * cd_induced));
        Ok(ViscousSolution {
            alpha_rad,
            cl,
            cd_induced,
            cd_profile,
            cm: result.cm_pitch,
            span_efficiency,
            lift_n: cl * air.q_pa() * s_ref,
            strips,
            offsets_rad,
            iterations,
            residual,
            converged,
            validity,
        })
    }
}
