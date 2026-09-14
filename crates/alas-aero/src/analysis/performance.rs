// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/aerodynamics.py
// Reference: alas @ rust-port-baseline.

//! The three entry points that run the vortex lattice and correct what it
//! returns -- `AeroAnalysis`' "performance estimates" section.
//!
//! Each is a different budget. [`AeroAnalysis::quick_performance`] runs two
//! probe solves and linearizes between them, which is what the optimizer can
//! afford inside its loop. [`AeroAnalysis::trimmed_performance`] runs exactly
//! one solve, at an angle and a stabilizer incidence a trim solve has already
//! settled, so the induced drag it reports is the real trim drag rather than
//! an added-on correction. [`AeroAnalysis::run_sweep`] runs one solve per
//! angle across the whole schedule and is the reported drag polar.
//!
//! All three end by compressibility-correcting the angle they report and
//! nothing else; `crate::analysis`'s module doc says why.

use alas_atmo::Atmosphere;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_math::interp;

use crate::operating_point::OperatingPoint;
use crate::vlm::{self, VlmError, VlmResult};

use super::{compressible_report_alpha, swept_pg_beta, AeroAnalysis};

/// The name `trimmed_performance` looks the horizontal stabilizer up by, and
/// which `alas-geom::builder` gives it.
const HSTAB_NAME: &str = "Horizontal Stabilizer";

/// A lift-curve slope below this magnitude is treated as no slope at all, and
/// the angle is reported without a zero-lift reference to compress it toward.
const CL_ALPHA_FLOOR: f64 = 1e-9;

/// The three numbers [`AeroAnalysis::trimmed_performance`] reads off a trim
/// solve.
///
/// Upstream passes a whole `StabilityTrimResult`, which is an `alas-stab`
/// type and therefore P7 -- above this row. See `crate::analysis`'s module
/// doc for why this takes the fields instead of the type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimPoint {
    /// The trimmed angle of attack, in degrees.
    pub trim_alpha_deg: f64,
    /// The trimmed horizontal-stabilizer incidence, in degrees. A NaN means
    /// the trim solve did not settle one, and the stabilizer is left at the
    /// twist the geometry built it with.
    pub trim_ih_deg: f64,
    /// The lift-curve slope the trim solve used, per degree.
    pub cl_alpha: f64,
}

/// What the two-point cruise estimate reports -- `quick_performance`'s dict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuickPerformance {
    /// Lift-to-drag ratio at the target lift coefficient.
    pub l_over_d: f64,
    /// The compressibility-corrected angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Total drag coefficient, all three components.
    pub cd: f64,
    /// The target lift coefficient, returned unchanged.
    pub cl: f64,
}

/// What the single trimmed solve reports -- `trimmed_performance`'s dict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimmedPerformance {
    /// Lift-to-drag ratio at the trimmed condition.
    pub l_over_d: f64,
    /// The compressibility-corrected angle of attack, in degrees.
    pub alpha_deg: f64,
    /// The stabilizer incidence the trim solve settled, in degrees, passed
    /// straight through.
    pub incidence_deg: f64,
    /// Total drag coefficient, all three components.
    pub cd: f64,
    /// Parasite drag coefficient at the trimmed operating point.
    pub cd_parasite: f64,
    /// Induced drag coefficient returned by the vortex-lattice solve.
    pub cd_induced: f64,
    /// Wave drag coefficient from the configured compressibility correction.
    pub cd_wave: f64,
    /// Lift coefficient the solve produced at the trimmed angle.
    pub cl: f64,
    /// The pitching moment left over at that condition. Diagnostic: it should
    /// be near zero, and nothing penalizes it if it is not.
    pub cm_residual: f64,
}

/// The corrected drag polar and stability curve over the angle schedule --
/// `run_sweep`'s dict of arrays. Every field has one entry per angle, in
/// schedule order.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarSweep {
    /// The display/reporting angle of attack axis, in degrees, after the
    /// compressibility correction described below.
    pub alpha_deg: Vec<f64>,
    /// The geometric angle of attack used for each VLM solve, in degrees.
    ///
    /// The reporting axis above may be relabeled by the Prandtl--Glauert
    /// correction without recomputing the coefficients. External solvers and
    /// cross-model comparisons must use this native state axis.
    pub geometric_alpha_deg: Vec<f64>,
    /// Lift coefficient.
    pub cl: Vec<f64>,
    /// Total drag coefficient.
    pub cd: Vec<f64>,
    /// The vortex lattice's induced-drag term.
    pub cd_induced: Vec<f64>,
    /// The Korn wave-drag term.
    pub cd_wave: Vec<f64>,
    /// The Raymer parasite term.
    pub cd_parasite: Vec<f64>,
    /// Pitching-moment coefficient.
    pub cm: Vec<f64>,
    /// Lift-to-drag ratio.
    pub l_over_d: Vec<f64>,
}

impl AeroAnalysis<'_> {
    /// One vortex-lattice solve at `op_point` -- `_run_vlm`.
    fn run_vlm(&self, op_point: &OperatingPoint) -> Result<VlmResult, VlmError> {
        self.run_vlm_on(self.plane, op_point)
    }

    /// The same solve, on a specific airplane -- which is only ever a
    /// modified copy of `self.plane`, in [`Self::trimmed_performance`].
    fn run_vlm_on(
        &self,
        plane: &alas_geom::aircraft::airplane::Airplane,
        op_point: &OperatingPoint,
    ) -> Result<VlmResult, VlmError> {
        // `usize` rather than the configuration's `i64`: a negative or zero
        // resolution is not a mesh, and both fields are documented as
        // multipliers of at least one.
        vlm::run(
            plane,
            op_point,
            self.analysis.spanwise_resolution.max(1) as usize,
            self.analysis.chordwise_resolution.max(1) as usize,
        )
    }

    /// The mesh and factored influence matrix of `self.plane` at the
    /// configured resolution, assembled once so that a schedule of operating
    /// points pays the O(n^3) factorization a single time.
    fn system(&self) -> Result<vlm::VlmSystem<'_>, VlmError> {
        vlm::VlmSystem::assemble(
            self.plane,
            self.analysis.spanwise_resolution.max(1) as usize,
            self.analysis.chordwise_resolution.max(1) as usize,
        )
    }

    /// A level operating point at `alpha_deg`: no sideslip, no rotation
    /// rates, which is every state these three entry points construct.
    fn level_op_point(atmosphere: Atmosphere, velocity: f64, alpha_deg: f64) -> OperatingPoint {
        OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0)
    }

    /// The fast two-point cruise estimate the optimization loop runs --
    /// `quick_performance`.
    ///
    /// Two probe solves fix a lift-curve slope; the angle that reaches
    /// `cl_target` follows from it, and the induced drag is scaled off the
    /// low probe's own `CD`/`CL^2`. The empirical components are then added
    /// at the physical `cl_target`, not at either probe.
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    pub fn quick_performance(
        &self,
        cl_target: f64,
        mach: f64,
        altitude_m: f64,
    ) -> Result<QuickPerformance, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let alpha_low = self.analysis.probe_alpha_low_deg;
        let alpha_high = self.analysis.probe_alpha_high_deg;

        let system = self.system()?;
        let low = system.solve(&Self::level_op_point(atmosphere, velocity, alpha_low))?;
        let high = system.solve(&Self::level_op_point(atmosphere, velocity, alpha_high))?;

        let cl_alpha = (high.cl_lift - low.cl_lift) / (alpha_high - alpha_low);
        let alpha_required = alpha_low + (cl_target - low.cl_lift) / cl_alpha;
        let alpha_zero_lift = if cl_alpha.abs() > CL_ALPHA_FLOOR {
            alpha_low - low.cl_lift / cl_alpha
        } else {
            alpha_required
        };
        let alpha_report =
            compressible_report_alpha(alpha_required, alpha_zero_lift, mach, self.sweep_deg);

        // The 1e-9 keeps a probe that produced no lift from dividing by zero;
        // it is upstream's, and it is why this is a two-point estimate rather
        // than a polar fit.
        let k_induced = low.cd_drag / (low.cl_lift * low.cl_lift + 1e-9);
        let cd_induced = k_induced * cl_target * cl_target;
        let components =
            self.drag_components(mach, altitude_m, cl_target, cd_induced, Some(&atmosphere));
        Ok(QuickPerformance {
            l_over_d: cl_target / components.cd_total(),
            alpha_deg: alpha_report,
            cd: components.cd_total(),
            cl: cl_target,
        })
    }

    /// Evaluate the genuinely trimmed cruise condition a trim solve settled
    /// -- `trimmed_performance`.
    ///
    /// One solve, at `trim`'s angle and stabilizer incidence. Whatever tail
    /// lift or download the trim requires shows up in the solve's own induced
    /// drag, which *is* the trim drag -- no separate trim-drag formula is
    /// added on top.
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    pub fn trimmed_performance(
        &self,
        trim: &TrimPoint,
        mach: f64,
        altitude_m: f64,
    ) -> Result<TrimmedPerformance, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let op_point = Self::level_op_point(atmosphere, velocity, trim.trim_alpha_deg);

        // Upstream overwrites every stabilizer section's twist in place and
        // restores it in a `finally`. A copy for the duration of the solve
        // is the same thing without the window in which an exception would
        // leave the aircraft altered -- and it is also why the restore's own
        // quirk (it writes `xsecs[0]`'s twist back to all of them, losing any
        // spanwise variation) has nothing to reproduce here: no section of
        // `self.plane` is ever written to.
        let perturbed = self.with_stabilizer_incidence(trim.trim_ih_deg);
        let solved = match &perturbed {
            Some(plane) => self.run_vlm_on(plane, &op_point)?,
            None => self.run_vlm(&op_point)?,
        };

        let cl_trim = solved.cl_lift;
        let components =
            self.drag_components(mach, altitude_m, cl_trim, solved.cd_drag, Some(&atmosphere));

        // The solve ran at the incompressible trim angle, so CL and CD -- and
        // therefore L/D and the trim drag -- are already right; only the
        // angle is reported corrected.
        let alpha_report = if trim.cl_alpha.abs() > CL_ALPHA_FLOOR {
            let alpha_zero_lift = trim.trim_alpha_deg - cl_trim / trim.cl_alpha;
            compressible_report_alpha(trim.trim_alpha_deg, alpha_zero_lift, mach, self.sweep_deg)
        } else {
            trim.trim_alpha_deg
        };

        Ok(TrimmedPerformance {
            l_over_d: cl_trim / components.cd_total(),
            alpha_deg: alpha_report,
            incidence_deg: trim.trim_ih_deg,
            cd: components.cd_total(),
            cd_parasite: components.cd_parasite,
            cd_induced: components.cd_induced,
            cd_wave: components.cd_wave,
            cl: cl_trim,
            cm_residual: solved.cm_pitch,
        })
    }

    /// A copy of the aircraft with every horizontal-stabilizer section set to
    /// `incidence_deg`, or `None` when there is nothing to set: no stabilizer
    /// by that name, or an incidence the trim solve left as NaN.
    fn with_stabilizer_incidence(
        &self,
        incidence_deg: f64,
    ) -> Option<alas_geom::aircraft::airplane::Airplane> {
        if incidence_deg.is_nan() || !self.plane.wings.iter().any(|w| w.name == HSTAB_NAME) {
            return None;
        }
        let mut plane = self.plane.clone();
        for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
            set_twist(wing, incidence_deg);
        }
        Some(plane)
    }

    /// The full angle sweep: the corrected drag polar and the stability
    /// curve -- `run_sweep`.
    ///
    /// The reported angle axis is compressibility-corrected as a whole at the
    /// end, about the zero-lift angle read off the computed polar. That
    /// correction is skipped when the polar cannot supply a zero-lift
    /// angle -- fewer than two points, or a lift coefficient that does not
    /// rise across the schedule -- and then the geometric angles are reported
    /// as they were flown.
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    pub fn run_sweep(&self, mach: f64, altitude_m: f64) -> Result<PolarSweep, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let alphas = linspace(
            self.analysis.sweep_alpha_min_deg,
            self.analysis.sweep_alpha_max_deg,
            self.analysis.sweep_n_points.max(0) as usize,
        );

        let mut sweep = PolarSweep {
            alpha_deg: Vec::with_capacity(alphas.len()),
            geometric_alpha_deg: alphas.clone(),
            cl: Vec::with_capacity(alphas.len()),
            cd: Vec::with_capacity(alphas.len()),
            cd_induced: Vec::with_capacity(alphas.len()),
            cd_wave: Vec::with_capacity(alphas.len()),
            cd_parasite: Vec::with_capacity(alphas.len()),
            cm: Vec::with_capacity(alphas.len()),
            l_over_d: Vec::with_capacity(alphas.len()),
        };

        // The influence matrix depends on the geometry alone, so the whole
        // schedule shares one assembly and one factorization; each angle is
        // a new right-hand side.
        if !alphas.is_empty() {
            let system = self.system()?;
            for alpha in &alphas {
                let solved = system.solve(&Self::level_op_point(atmosphere, velocity, *alpha))?;
                let components = self.drag_components(
                    mach,
                    altitude_m,
                    solved.cl_lift,
                    solved.cd_drag,
                    Some(&atmosphere),
                );
                sweep.alpha_deg.push(*alpha);
                sweep.cl.push(solved.cl_lift);
                sweep.cm.push(solved.cm_pitch);
                sweep.cd.push(components.cd_total());
                sweep.cd_induced.push(components.cd_induced);
                sweep.cd_wave.push(components.cd_wave);
                sweep.cd_parasite.push(components.cd_parasite);
                sweep.l_over_d.push(solved.cl_lift / components.cd_total());
            }
        }

        let rises = sweep.cl.len() >= 2
            && sweep
                .cl
                .last()
                .zip(sweep.cl.first())
                .is_some_and(|(last, first)| last - first > 1e-6);
        if rises {
            let alpha_zero_lift = interp(0.0, &sweep.cl, &sweep.alpha_deg);
            let beta = swept_pg_beta(mach, self.sweep_deg);
            for alpha in &mut sweep.alpha_deg {
                *alpha = alpha_zero_lift + beta * (*alpha - alpha_zero_lift);
            }
        }

        Ok(sweep)
    }
}

/// Set every cross-section of `wing` to `twist_deg`.
///
/// A free function rather than a `Wing` method: `alas-geom::aircraft::wing`'s row
/// is scoped to what native aerodynamic model's own `Wing` offers, and upstream mutates
/// the sections from outside rather than through any such method.
fn set_twist(wing: &mut Wing, twist_deg: f64) {
    for xsec in &mut wing.xsecs {
        xsec.twist = twist_deg;
    }
}
