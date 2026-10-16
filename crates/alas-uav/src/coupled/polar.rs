// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission `DragPolar` from the viscous VLM and the drag build-up.
//!
//! The mission model needs `CD(CL)` without a speed argument.  For a fixed
//! weight the 1 g lift coefficient fixes the speed (`V = sqrt(2 W / rho S CL)`),
//! so the table is built along that line: for each angle of attack the wing is
//! solved at the Reynolds number of the speed that angle needs at 1 g (two
//! fixed-point passes), and the non-wing parasite drag is evaluated at the same
//! speed.  `cd_parts(cl)` returns
//!
//! * profile drag = wing strip profile drag + component build-up (+ the
//!   excrescence allowance), at the 1 g speed of that `CL`;
//! * induced drag = Trefftz-plane induced drag of the lattice, tabulated as
//!   `k(CL) = CDi / CL^2`.
//!
//! [`WingPolar::with_trim`] adds the trim drag of the tail
//! ([`super::trim_drag`]) to the induced part.
//!
//! `cl_max` is the maximum of the coupled alpha sweep at the Reynolds number
//! of the resulting stall speed (also a fixed point).  No flap is modelled, so
//! `cl_max` is the clean-wing value.
//!
//! Limits: turns and any load factor `n > 1` reuse the 1 g Reynolds number;
//! the ground-roll speeds (below the stall speed) reuse the lowest tabulated
//! point; wing-body interference beyond the `Q` factors is not modelled.  The
//! tail's trim drag is added by [`WingPolar::with_trim`] when requested; the
//! propeller-wash drag increment lives in the powertrain adapter
//! ([`super::slipstream_drag`]), so the wing lift in the prop wash is that of
//! free air.

use crate::aero::air::AirState;
use crate::aero::drag_lowre::{build_up, Component};
use crate::aero::viscous_vlm::{ViscousConfig, ViscousSolution, ViscousVlm};
use crate::perf::DragPolar;

use super::geometry::WingBuild;
use super::trim_drag::TrimDrag;
use super::CoupledError;

/// Build settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarSettings {
    /// Air density, kg/m3.
    pub rho: f64,
    /// Air temperature, K.
    pub t_k: f64,
    /// Aircraft weight used to tie speed to lift coefficient, N.
    pub weight_n: f64,
    /// Number of angles in the polar table.
    pub n_alpha: usize,
    /// Chordwise lattice panels per strip.
    pub chordwise: usize,
    /// Excrescence allowance (fraction of the component sum).
    pub excrescence_fraction: f64,
}

impl PolarSettings {
    /// Defaults for a given density and weight (15 deg C, 9 angles, 4 chordwise
    /// panels, 10 % excrescence).
    pub fn new(rho: f64, weight_n: f64) -> Self {
        Self {
            rho,
            t_k: 288.15,
            weight_n,
            n_alpha: 9,
            chordwise: 4,
            excrescence_fraction: crate::aero::drag_lowre::DEFAULT_EXCRESCENCE_FRACTION,
        }
    }
}

/// Summary numbers of the polar, for reports.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarInfo {
    /// Lift-curve slope of the wing, 1/rad.
    pub cl_alpha_per_rad: f64,
    /// Span efficiency at mid-table (`CL^2/(pi A CDi)`).
    pub span_efficiency: f64,
    /// Zero-lift parasite drag coefficient (profile at `CL ~ 0.2`), -.
    pub cd0: f64,
    /// Wing-alone profile drag coefficient at the same point (wing-area
    /// basis, without the component build-up), -.
    pub wing_cd0: f64,
    /// Maximum `CL/CD` over the table.
    pub l_over_d_max: f64,
    /// Stall speed at 1 g, m/s.
    pub stall_speed_m_s: f64,
    /// Where the clean wing first stalls (`y` of the first strip, m), if found.
    pub first_stall_y_m: Option<f64>,
    /// Table lift coefficients, -.
    pub cl_table: Vec<f64>,
    /// Aspect ratio.
    pub aspect_ratio: f64,
    /// Findings of the section validity guard (Re range, alpha, confidence).
    pub validity_notes: Vec<String>,
    /// Component build-up lines `(name, CD0 contribution)` at cruise CL.
    pub build_up: Vec<(String, f64)>,
}

/// The drag polar of a wing plus fuselage and tail.
#[derive(Debug, Clone)]
pub struct WingPolar {
    cl: Vec<f64>,
    cd_profile: Vec<f64>,
    k_induced: Vec<f64>,
    alpha_rad: Vec<f64>,
    cl_max: f64,
    /// Trim drag of the tail, when requested.
    pub trim: Option<TrimDrag>,
    /// Summary numbers.
    pub info: PolarInfo,
}

fn interp(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let n = xs.len();
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[n - 1] {
        return ys[n - 1];
    }
    let i = xs.partition_point(|&v| v <= x) - 1;
    ys[i] + (x - xs[i]) / (xs[i + 1] - xs[i]) * (ys[i + 1] - ys[i])
}

struct Point {
    alpha: f64,
    sol: ViscousSolution,
    v: f64,
}

impl WingPolar {
    /// Build the polar of `wing` with the non-wing `components`.
    ///
    /// # Errors
    ///
    /// [`CoupledError`] from the lattice, the section models or the build-up.
    pub fn build(
        wing: &WingBuild,
        components: &[Component],
        settings: &PolarSettings,
    ) -> Result<Self, CoupledError> {
        let cfg = ViscousConfig {
            spanwise_strips: wing.stations_y_m.len().saturating_sub(1).max(1),
            chordwise_resolution: settings.chordwise,
            refuse_outside_domain: false,
            ..ViscousConfig::default()
        };
        let vlm = ViscousVlm::new(&wing.airplane, wing.sections.clone(), cfg)?;
        let (rho, w, s) = (settings.rho, settings.weight_n, wing.s_ref_m2);
        let air_at = |v: f64| AirState::from_rho_t(v, rho, settings.t_k);
        let v_for = |cl: f64| (2.0 * w / (rho * s * cl.max(0.05))).sqrt();

        // CLmax at the Reynolds number of its own stall speed.
        let mut v_stall = v_for(1.2);
        let mut peak = None;
        for _ in 0..2 {
            let m = vlm.cl_max(
                &air_at(v_stall)?,
                None,
                0.0,
                24.0_f64.to_radians(),
                1.0_f64.to_radians(),
            )?;
            v_stall = v_for(m.cl_max);
            peak = Some(m);
        }
        let peak = peak.ok_or_else(|| CoupledError::Spec("no CLmax sweep".into()))?;
        let alpha_top = (peak.alpha_rad - 1.5_f64.to_radians()).max(2.0_f64.to_radians());
        let alpha_lo = -2.0_f64.to_radians();
        let n = settings.n_alpha.max(4);

        let mut pts: Vec<Point> = Vec::with_capacity(n);
        for k in 0..n {
            let alpha = alpha_lo + (alpha_top - alpha_lo) * k as f64 / (n - 1) as f64;
            let mut v = v_for(0.6);
            let mut sol = None;
            for _ in 0..3 {
                let sv = vlm.solve(&air_at(v)?, alpha, None)?;
                v = v_for(sv.cl).clamp(0.8 * v_stall, (6.0 * v_stall).min(40.0));
                sol = Some(sv);
            }
            let sol = sol.ok_or_else(|| CoupledError::Spec("no polar point".into()))?;
            pts.push(Point { alpha, sol, v });
        }
        pts.sort_by(|a, b| a.sol.cl.total_cmp(&b.sol.cl));
        let (mut cl, mut cdp, mut ki, mut al, mut cdw) = (vec![], vec![], vec![], vec![], vec![]);
        let mut notes: Vec<String> = Vec::new();
        for p in &pts {
            let body = build_up(components, &air_at(p.v)?, s, settings.excrescence_fraction)?;
            if cl.last().is_some_and(|&c| p.sol.cl <= c + 1e-6) {
                continue;
            }
            cl.push(p.sol.cl);
            cdp.push(p.sol.cd_profile + body.cd0_total);
            cdw.push(p.sol.cd_profile);
            ki.push(p.sol.cd_induced / (p.sol.cl * p.sol.cl).max(1e-4));
            al.push(p.alpha);
            for (_, flag) in &p.sol.validity.flags {
                let t = flag.to_string();
                if !notes.contains(&t) {
                    notes.push(t);
                }
            }
            if !p.sol.converged {
                notes.push(format!(
                    "polar point alpha {:.1} deg not converged",
                    p.alpha.to_degrees()
                ));
            }
        }
        if cl.len() < 3 {
            return Err(CoupledError::Spec(
                "the polar table collapsed (fewer than 3 distinct CL)".into(),
            ));
        }
        let aspect = wing.span_m * wing.span_m / s;
        let slope = (cl[cl.len() - 1] - cl[0]) / (al[al.len() - 1] - al[0]);
        let mid = cl.len() / 2;
        let e_mid = 1.0 / (std::f64::consts::PI * aspect * ki[mid]);
        let i02 = cl.partition_point(|&c| c < 0.2).min(cl.len() - 1);
        let ld = cl
            .iter()
            .zip(cdp.iter().zip(&ki))
            .map(|(&c, (&p, &k))| c / (p + k * c * c))
            .fold(0.0_f64, f64::max);
        let v_ref = v_for(0.6);
        let comp_lines = build_up(
            components,
            &air_at(v_ref)?,
            s,
            settings.excrescence_fraction,
        )?;
        let mut lines: Vec<(String, f64)> = comp_lines
            .lines
            .iter()
            .map(|l| (l.name.clone(), l.cd0))
            .collect();
        lines.push((
            comp_lines.excrescence.name.clone(),
            comp_lines.excrescence.cd0,
        ));
        let info = PolarInfo {
            cl_alpha_per_rad: slope,
            span_efficiency: e_mid,
            cd0: cdp[i02],
            wing_cd0: cdw[i02],
            l_over_d_max: ld,
            stall_speed_m_s: v_stall,
            first_stall_y_m: peak.first_stall.map(|f| f.y_m),
            cl_table: cl.clone(),
            aspect_ratio: aspect,
            validity_notes: notes,
            build_up: lines,
        };
        Ok(Self {
            cl,
            cd_profile: cdp,
            k_induced: ki,
            alpha_rad: al,
            cl_max: peak.cl_max,
            trim: None,
            info,
        })
    }

    /// Add the trim drag of the tail to the induced part of the polar.
    pub fn with_trim(mut self, trim: Option<TrimDrag>) -> Self {
        self.trim = trim;
        self
    }

    /// Wing lift coefficient at angle of attack `alpha_rad` (linear in the
    /// table, linear extrapolation with the table slope outside it).
    pub fn cl_at_alpha(&self, alpha_rad: f64) -> f64 {
        let n = self.cl.len();
        let slope = (self.cl[n - 1] - self.cl[0]) / (self.alpha_rad[n - 1] - self.alpha_rad[0]);
        if alpha_rad <= self.alpha_rad[0] {
            self.cl[0] + slope * (alpha_rad - self.alpha_rad[0])
        } else if alpha_rad >= self.alpha_rad[n - 1] {
            self.cl[n - 1] + slope * (alpha_rad - self.alpha_rad[n - 1])
        } else {
            let a: Vec<f64> = self.alpha_rad.clone();
            interp(&a, &self.cl, alpha_rad)
        }
    }
}

impl DragPolar for WingPolar {
    fn cd_parts(&self, cl: f64) -> (f64, f64) {
        let c = cl.abs();
        let cdp = interp(&self.cl, &self.cd_profile, c);
        let k = interp(&self.cl, &self.k_induced, c);
        let trim = self.trim.as_ref().map_or(0.0, |t| t.delta_cdi(c, k));
        (cdp, k * cl * cl + trim)
    }

    fn cl_max(&self) -> f64 {
        self.cl_max
    }
}
