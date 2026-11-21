// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static aeroelastic shift of the neutral point of a swept wing.
//!
//! The VLM span load at the analysed dynamic pressure loads the wing box
//! (bending moment `M` and torque `T` about the elastic axis, from the panel
//! forces). The streamwise incidence change of a swept elastic axis is
//! (Niu, *Airframe Structural Design*, 1st ed., 1988, section 3.2
//! "Aeroelasticity", swept wing, pp. 31-33, p. 32)
//!
//! `d alpha = theta cos(L) - phi sin(L)`, `theta = int T/(G J) dy`,
//! `phi = int M/(E I) dy`,
//!
//! with `L` the sweep of the elastic axis, `phi` the bending slope of the axis
//! and `theta` the torsional twist. That incidence is added to every panel of
//! the wing ([`alas_aero::vlm::VlmSystem::solve_with_incidence_offsets`]) and
//! the lattice is re-solved until the incidence stops changing. The neutral
//! point is formed from the flexible `alpha` probes exactly as the rigid one,
//! so the shift is the flexible minus the rigid neutral point.
//!
//! Frames: geometry axes (x aft, y right, z up), SI. `T` is positive nose-up
//! about the outboard-pointing axis, `M` positive when the load raises the
//! tip. `E I` is the spanwise-frame stiffness `EI_n cos^3 L`
//! (`w'' = M_x / EI`) and `G J` that of the normal-to-axis cut, both from the
//! sized wing box. The Goethert-stretched mesh carries x stretched by
//! `1/beta`; arms in x are scaled back by `beta`.
//!
//! Linear range: elastic incidence up to 0.173 rad (see `linear_range_rad`);
//! beyond it the result is reported as not converged.
//!
//! Empennage ([`crate::neutral_point::TailFlexibility`]): the tail load bends
//! the aft fuselage, `theta = int M/EI_f dx`, which pitches the tail nose-down
//! and cuts its incidence change with `alpha`, and the horizontal-tail box
//! deforms like the wing (Obert, *Aerodynamic Design of Transport Aircraft*,
//! 2009, p. 396 for the mechanism; the beam integral is Euler-Bernoulli, as
//! the wing's). The fuselage and tail stiffnesses are mass-smeared upper bounds
//! (see `alas-mass` `tail_flexibility`).
//!
//! Inertia relief (when [`crate::neutral_point::InertiaRelief`] is supplied)
//! uses the equilibrium load factor `L/(m g)` of each probe and the carried
//! mass of the sized box, its design-case fuel and its engines, on the elastic
//! axis; the non-box wing structure is not included.
//!
//! Not modelled: chordwise camber deformation, the bending-torsion cross term
//! of the beam frame, and the flexibility of the fuselage forward of the wing.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::strips::{wing_strips, WingStrip};
use alas_aero::vlm::{VlmError, VlmResult, VlmSystem};
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::{
    airframe_terms, np_from_shared_terms, np_shared_terms, resolution, zero_airframe_terms,
    HighLiftScaling, ProbeCondition,
};
use crate::neutral_point::surfaces::{beta_for_mach, goethert_stretch};
use crate::neutral_point::{ElasticNpReport, TailFlexibility, WingStiffness};
use crate::trim::{HSTAB_NAME, MAIN_WING_NAME};

/// Fixed-point stopping tolerance on the largest change of the elastic
/// incidence between iterations, rad (about 6e-6 deg).
const INCIDENCE_TOLERANCE_RAD: f64 = 1.0e-7;
/// Iteration cap of the fixed point.
const MAX_ITERATIONS: usize = 300;
/// Smallest relaxation of the fixed point.
const MIN_RELAXATION: f64 = 1.0 / 64.0;
/// Relative error accepted in the small-angle linearisation (`tan a = a`) the
/// method rests on.
const SMALL_ANGLE_TOLERANCE: f64 = 0.01;
/// Linear range of the method, rad: the streamwise-incidence relation of
/// Niu (1988, sec. 3.2, p. 32) and the lattice's linearised flow-tangency
/// condition are small-angle relations, `tan a = a (1 + a^2/3 + ...)`, so the
/// elastic incidence is valid while `a^2/3 <= SMALL_ANGLE_TOLERANCE`, i.e.
/// `|a| <= sqrt(3 * 0.01) = 0.173 rad` (9.9 deg). A fixed point beyond it, or
/// one that grows without bound (static divergence), is reported as not
/// converged.
fn linear_range_rad() -> f64 {
    (3.0 * SMALL_ANGLE_TOLERANCE).sqrt()
}

/// Linear interpolation of `values` on `x`, clamped at the ends.
fn interp(x: &[f64], values: &[f64], at: f64) -> f64 {
    if at <= x[0] {
        return values[0];
    }
    let last = x.len() - 1;
    if at >= x[last] {
        return values[last];
    }
    let upper = x.partition_point(|&s| s <= at).clamp(1, last);
    let t = (at - x[upper - 1]) / (x[upper] - x[upper - 1]);
    values[upper - 1] + t * (values[upper] - values[upper - 1])
}

/// One wing half: strip indices ordered root to tip with the elastic-axis
/// geometry (physical x) at each.
struct Half {
    strips: Vec<WingStrip>,
    /// Elastic-axis x at each strip, m.
    x_axis: Vec<f64>,
    /// `sin L`, `cos L` of the elastic axis at each strip.
    sin_l: Vec<f64>,
    cos_l: Vec<f64>,
}

impl Half {
    fn new(mut strips: Vec<WingStrip>, axis_fraction: f64, beta: f64) -> Self {
        strips.sort_by(|a, b| a.y_m.abs().total_cmp(&b.y_m.abs()));
        let x_axis: Vec<f64> = strips
            .iter()
            .map(|s| {
                beta * (s.leading_edge_m[0]
                    + axis_fraction * (s.trailing_edge_m[0] - s.leading_edge_m[0]))
            })
            .collect();
        let n = strips.len();
        let (mut sin_l, mut cos_l) = (vec![0.0; n], vec![1.0; n]);
        for j in 0..n {
            let (a, b) = (j.saturating_sub(1), (j + 1).min(n - 1));
            let dy = strips[b].y_m.abs() - strips[a].y_m.abs();
            if dy > 0.0 {
                let angle = ((x_axis[b] - x_axis[a]) / dy).atan();
                sin_l[j] = angle.sin();
                cos_l[j] = angle.cos();
            }
        }
        Self {
            strips,
            x_axis,
            sin_l,
            cos_l,
        }
    }
}

/// Elastic streamwise incidence at every strip of `half`, rad (nose-up
/// positive), from the loads of `result`.
fn elastic_incidence(
    half: &Half,
    result: &VlmResult,
    stiffness: &WingStiffness,
    beta: f64,
) -> Vec<f64> {
    let n = half.strips.len();
    let y: Vec<f64> = half.strips.iter().map(|s| s.y_m.abs()).collect();
    // Panel loads of the strips, root to tip: (Fz, x, |y|).
    let mut loads: Vec<Vec<(f64, f64, f64)>> = half
        .strips
        .iter()
        .map(|s| {
            (s.first_panel..s.first_panel + s.panel_count)
                .map(|p| {
                    let c = result.panels[p].vortex_center;
                    (result.panel_forces_geometry[p][2], beta * c[0], c[1].abs())
                })
                .collect()
        })
        .collect();
    // Inertia relief: with the aircraft in equilibrium the load factor is
    // `L / (m g)`, so a carried mass `dm` on the elastic axis pulls down with
    // `(L / m) dm`. Point masses go to the nearest strip.
    if let Some(relief) = &stiffness.inertia_relief {
        let acceleration = result.lift / relief.aircraft_mass_kg;
        for (k, strip) in half.strips.iter().enumerate() {
            let mut mass = interp(&stiffness.y_m, &relief.carried_kg_m, y[k]) * strip.width_m;
            for &(station, kg) in &relief.point_masses_kg {
                let nearest = (0..y.len())
                    .min_by(|&a, &b| (y[a] - station).abs().total_cmp(&(y[b] - station).abs()));
                if nearest == Some(k) {
                    mass += kg;
                }
            }
            loads[k].push((-acceleration * mass, half.x_axis[k], y[k]));
        }
    }
    // Integrand `(M/EI, T/GJ)` at a station: loads on strips from `from` out.
    let integrand = |at_y: f64, x_axis: f64, sin_l: f64, cos_l: f64, from: usize| {
        let (mut m, mut t) = (0.0, 0.0);
        for strip in &loads[from..] {
            for &(fz, x, yp) in strip {
                m += fz * (yp - at_y);
                t += fz * (sin_l * (yp - at_y) - cos_l * (x - x_axis));
            }
        }
        let ei = interp(&stiffness.y_m, &stiffness.ei_nm2, at_y);
        let gj = interp(&stiffness.y_m, &stiffness.gj_nm2, at_y);
        (m / ei, t / gj)
    };
    let y0 = stiffness.y_m[0];
    let mut alpha = vec![0.0; n];
    let (mut phi, mut theta) = (0.0, 0.0);
    let mut previous: Option<(f64, f64, f64)> = None; // (y, M/EI, T/GJ)
    for j in 0..n {
        if y[j] <= y0 {
            continue;
        }
        if previous.is_none() {
            // The clamp: start at the box root with the load outboard of it.
            let first = (0..n).find(|&k| y[k] > y0).unwrap_or(j);
            let (m0, t0) = integrand(
                y0,
                half.x_axis[first],
                half.sin_l[first],
                half.cos_l[first],
                first,
            );
            previous = Some((y0, m0, t0));
        }
        let (m_j, t_j) = integrand(y[j], half.x_axis[j], half.sin_l[j], half.cos_l[j], j);
        if let Some((y_prev, m_prev, t_prev)) = previous {
            let dy = y[j] - y_prev;
            phi += 0.5 * (m_prev + m_j) * dy;
            theta += 0.5 * (t_prev + t_j) * dy;
        }
        previous = Some((y[j], m_j, t_j));
        alpha[j] = theta * half.cos_l[j] - phi * half.sin_l[j];
    }
    alpha
}

/// Outcome of the fixed point at one angle of attack.
struct FixedPoint {
    result: VlmResult,
    iterations: usize,
    residual_rad: f64,
    converged: bool,
    tip_incidence_rad: f64,
}

/// Halves of wing `wing_index` with at least two strips.
fn halves_of(
    rigid: &VlmResult,
    op: &OperatingPoint,
    wing_index: usize,
    axis_fraction: f64,
    beta: f64,
) -> Vec<Half> {
    let strips = wing_strips(rigid, op, wing_index);
    let (right, left): (Vec<_>, Vec<_>) = strips.into_iter().partition(|s| s.y_m >= 0.0);
    [right, left]
        .into_iter()
        .filter(|h| h.len() >= 2)
        .map(|h| Half::new(h, axis_fraction, beta))
        .collect()
}

/// Rotation of the horizontal-tail root, rad (positive: aft end rising, i.e.
/// the tail pitching nose-down), from the tail load `result` puts on the aft
/// fuselage: `theta = int M/EI_f dx` with `M(x) = F_t (x_F - x)` between the
/// wing-body junction and the tail root (Euler-Bernoulli cantilever from the
/// junction; Obert, *Aerodynamic Design of Transport Aircraft*, 2009, p. 396).
fn fuselage_tail_rotation(
    result: &VlmResult,
    tail_index: usize,
    tail: &TailFlexibility,
    beta: f64,
) -> f64 {
    let (mut force, mut moment) = (0.0, 0.0);
    for (panel, f) in result.panels.iter().zip(&result.panel_forces_geometry) {
        if panel.wing_index == tail_index {
            force += f[2];
            moment += f[2] * beta * panel.vortex_center[0];
        }
    }
    let x = &tail.fuselage_x_m;
    let x_force = if force.abs() > 1.0e-9 {
        moment / force
    } else {
        x[x.len() - 1]
    };
    // The moment of the load vanishes at and beyond its own station.
    let integrand = |k: usize| (x_force - x[k]).max(0.0) / tail.fuselage_ei_nm2[k];
    force
        * (1..x.len())
            .map(|k| 0.5 * (integrand(k - 1) + integrand(k)) * (x[k] - x[k - 1]))
            .sum::<f64>()
}

/// Solve the flexible wing at `op`: iterate the elastic incidence of every
/// half of wing `wing_index` (and, with `tail`, of the horizontal tail: its
/// own box twist plus the aft-fuselage rotation) to a fixed point.
fn flexible_solve(
    system: &VlmSystem<'_>,
    op: &OperatingPoint,
    rigid: &VlmResult,
    wing_index: usize,
    stiffness: &WingStiffness,
    tail: Option<(usize, &TailFlexibility)>,
    beta: f64,
) -> Result<FixedPoint, VlmError> {
    let halves = halves_of(rigid, op, wing_index, stiffness.box_axis_fraction, beta);
    let tail_halves = tail.map_or_else(Vec::new, |(index, t)| {
        halves_of(rigid, op, index, t.stabilizer.box_axis_fraction, beta)
    });
    let panel_count = system.panel_count();
    let offsets_of = |result: &VlmResult| -> (Vec<f64>, f64) {
        let mut offsets = vec![0.0; panel_count];
        let mut tip = 0.0_f64;
        for half in &halves {
            let alpha = elastic_incidence(half, result, stiffness, beta);
            for (strip, a) in half.strips.iter().zip(&alpha) {
                for o in &mut offsets[strip.first_panel..strip.first_panel + strip.panel_count] {
                    *o = *a;
                }
            }
            if let Some(&last) = alpha.last() {
                if last.abs() > tip.abs() {
                    tip = last;
                }
            }
        }
        if let Some((tail_index, flex)) = tail {
            let fuselage = fuselage_tail_rotation(result, tail_index, flex, beta);
            // The whole tail pitches nose-down by the fuselage rotation.
            for (p, panel) in result.panels.iter().enumerate() {
                if panel.wing_index == tail_index {
                    offsets[p] = -fuselage;
                }
            }
            for half in &tail_halves {
                let alpha = elastic_incidence(half, result, &flex.stabilizer, beta);
                for (strip, a) in half.strips.iter().zip(&alpha) {
                    for o in &mut offsets[strip.first_panel..strip.first_panel + strip.panel_count]
                    {
                        *o = *a - fuselage;
                    }
                }
            }
        }
        (offsets, tip)
    };
    let (mut offsets, mut tip) = offsets_of(rigid);
    let mut result = rigid.clone();
    // Picard iteration with Irons-Tuck (vector Aitken) relaxation `omega`,
    // clamped to (0, 1]: a washout (negative-feedback) wing can overshoot with
    // gain beyond one and still have a stable fixed point that damping finds,
    // while a diverging (positive-feedback, past the static divergence
    // pressure) wing keeps growing at the smallest `omega` and is reported.
    let mut omega = 1.0_f64;
    let mut previous: Option<Vec<f64>> = None;
    let mut worsening = 0;
    let mut last_residual = f64::INFINITY;
    let finish = |result: VlmResult, iterations, residual, converged, tip| FixedPoint {
        result,
        iterations,
        residual_rad: residual,
        converged,
        tip_incidence_rad: tip,
    };
    for iteration in 1..=MAX_ITERATIONS {
        result = system.solve_with_incidence_offsets(op, &offsets)?;
        let (next, next_tip) = offsets_of(&result);
        let r: Vec<f64> = next.iter().zip(&offsets).map(|(a, b)| a - b).collect();
        let residual = r.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let largest = next.iter().fold(0.0_f64, |m, a| m.max(a.abs()));
        if !residual.is_finite() || !largest.is_finite() || largest > linear_range_rad() {
            return Ok(finish(result, iteration, residual, false, next_tip));
        }
        if residual < INCIDENCE_TOLERANCE_RAD {
            return Ok(finish(result, iteration, residual, true, next_tip));
        }
        if let Some(r_prev) = &previous {
            let (mut num, mut den) = (0.0, 0.0);
            for (a, b) in r_prev.iter().zip(&r) {
                num += a * (b - a);
                den += (b - a) * (b - a);
            }
            if den > 0.0 {
                omega = (-omega * num / den).clamp(MIN_RELAXATION, 1.0);
            }
        }
        worsening = if residual >= last_residual && omega <= 2.0 * MIN_RELAXATION {
            worsening + 1
        } else {
            0
        };
        if worsening >= 8 {
            return Ok(finish(result, iteration, residual, false, next_tip));
        }
        last_residual = residual;
        for (o, step) in offsets.iter_mut().zip(&r) {
            *o += omega * step;
        }
        previous = Some(r);
        tip = next_tip;
    }
    Ok(finish(result, MAX_ITERATIONS, last_residual, false, tip))
}

/// Neutral point of a solved probe pair on the stretched lattice, geometry
/// axes: what `clean_np_at_condition` forms.
fn np_of_pair(
    airplane: &Airplane,
    stretched: &Airplane,
    analysis: &AnalysisConfig,
    condition: &ProbeCondition,
    beta: f64,
    lo: &VlmResult,
    hi: &VlmResult,
) -> Result<f64, VlmError> {
    let shared = np_shared_terms(stretched, condition, lo, hi)?;
    let (x_stretched, diag) = np_from_shared_terms(
        stretched,
        &shared,
        analysis.tail_efficiency_or(stretched.horizontal_tail_dynamic_pressure_ratio()),
        HighLiftScaling::NONE.stretched(beta),
        false,
        analysis.include_fuselage_stability,
    );
    let (fuselage_shift, nacelle_shift, _, _) = if analysis.include_fuselage_stability {
        airframe_terms(
            airplane,
            diag.cl_alpha_total,
            diag.wing_lift_fraction,
            diag.deps_dalpha_vlm,
        )
    } else {
        zero_airframe_terms(diag.deps_dalpha_vlm)
    };
    Ok(x_stretched * beta + fuselage_shift + nacelle_shift)
}

/// The flexible-minus-rigid clean neutral point at a stated Mach and altitude.
///
/// A non-converged fixed point is reported with `converged == false` and a
/// zero shift: the flexible state there does not exist (static divergence) or
/// the linear theory no longer holds.
///
/// # Errors
///
/// See [`VlmError`].
pub(crate) fn elastic_np_shift(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    mach: f64,
    altitude_m: f64,
    stiffness: &WingStiffness,
    tail: Option<&TailFlexibility>,
) -> Result<ElasticNpReport, VlmError> {
    let atmosphere = Atmosphere::new(altitude_m);
    let beta = beta_for_mach(mach).max(0.05);
    let condition = ProbeCondition {
        spanwise_resolution: resolution(analysis.spanwise_resolution),
        chordwise_resolution: resolution(analysis.chordwise_resolution),
        atmosphere,
        velocity: mach * atmosphere.speed_of_sound(),
        alpha_lo_deg: analysis.autobalance_alpha_low_deg,
        alpha_hi_deg: analysis.autobalance_alpha_high_deg,
    };
    let q_pa = 0.5 * atmosphere.density() * condition.velocity * condition.velocity;
    let rigid_report = |converged| ElasticNpReport {
        shift_m: 0.0,
        dynamic_pressure_pa: q_pa,
        converged,
        iterations: 0,
        residual_rad: 0.0,
        tip_incidence_deg: 0.0,
    };
    let Some(wing_index) = airplane.wings.iter().position(|w| w.name == MAIN_WING_NAME) else {
        return Ok(rigid_report(true));
    };
    if !stiffness.is_usable() {
        return Ok(rigid_report(true));
    }
    // The tail's flexibility needs its own lattice surface and a usable table.
    let tail = airplane
        .wings
        .iter()
        .position(|w| w.name == HSTAB_NAME)
        .zip(tail.filter(|t| t.is_usable()));
    let stretched = goethert_stretch(airplane, beta);
    let system = VlmSystem::assemble(
        &stretched,
        condition.spanwise_resolution,
        condition.chordwise_resolution,
    )?;
    let op = |alpha_deg| {
        OperatingPoint::new(
            atmosphere,
            condition.velocity,
            alpha_deg,
            0.0,
            0.0,
            0.0,
            0.0,
        )
    };
    let (op_lo, op_hi) = (op(condition.alpha_lo_deg), op(condition.alpha_hi_deg));
    let lo = system.solve(&op_lo)?;
    let hi = system.solve(&op_hi)?;
    let rigid = np_of_pair(airplane, &stretched, analysis, &condition, beta, &lo, &hi)?;
    let flex_lo = flexible_solve(&system, &op_lo, &lo, wing_index, stiffness, tail, beta)?;
    let flex_hi = flexible_solve(&system, &op_hi, &hi, wing_index, stiffness, tail, beta)?;
    let converged = flex_lo.converged && flex_hi.converged;
    let shift_m = if converged {
        np_of_pair(
            airplane,
            &stretched,
            analysis,
            &condition,
            beta,
            &flex_lo.result,
            &flex_hi.result,
        )? - rigid
    } else {
        0.0
    };
    Ok(ElasticNpReport {
        shift_m,
        dynamic_pressure_pa: q_pa,
        converged,
        iterations: flex_lo.iterations.max(flex_hi.iterations),
        residual_rad: flex_lo.residual_rad.max(flex_hi.residual_rad),
        tip_incidence_deg: flex_hi.tip_incidence_rad.to_degrees(),
    })
}
