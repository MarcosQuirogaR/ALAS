// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded stiffness sizing for the production linear structural model.
//!
//! Cap area is a physical design variable: increasing it changes EI, structural
//! mass and inertial relief together. This is a feasible-section construction,
//! not a minimum-mass structural optimizer. Every returned box needs an
//! explicit mass inventory: added stiffness changes the material mass and
//! inertia, but disagreement with an empirical estimate is diagnostic only.
//! Neither inventory silently substitutes for the authoritative mass ledger.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::analytical::{analyze_structure_with_running_mass, StructuralAnalysisReport};
use crate::feasibility::{assess, LinearModelLimits, StructuralFeasibility};

use super::WingboxSizing;

/// A physical section and its re-evaluated response under one design state.
#[derive(Debug, Clone, PartialEq)]
pub struct StiffnessSizingResult {
    /// Final semi-wing geometry and mass, including every stiffness addition.
    pub sizing: WingboxSizing,
    /// Response using the final section's own mass and supplied fuel state.
    pub response: StructuralAnalysisReport,
    /// Strength, ribs, finiteness and supported linear-domain metrics.
    pub assessment: StructuralFeasibility,
    /// All available checks passed and cap areas/mass stabilized within the
    /// bounded sizing budget (relative allocation tolerance 1e-4).
    pub converged: bool,
    /// Number of response evaluations, including the original section.
    pub iterations: usize,
}

/// Strength-sized wingbox with additional cap area when the 1 g flight shape
/// leaves the linear-model budget of [`crate::feasibility`]. The ultimate
/// manoeuvre deflection is reported there but never bought down here.
///
/// All inputs are SI. `initial` must be a converged strength-sized box at the
/// same requirements, fuel and mounted-mass state. Cap sections and any
/// required web gauges increase;
/// existing local geometric limits (half chord, one fifth of spar depth) and
/// manufacturing floors apply. Each iteration evaluates *its own* structural
/// weight, so this solve has no hidden self-weight fixed point. Rib capacity
/// is retained conservatively only while its original root bending envelope
/// is not exceeded. A clipped or unachievable layout remains explicitly
/// infeasible with its last graded assessment.
#[allow(clippy::too_many_arguments)] // Explicit geometry, material, load-state and model-domain inputs.
pub fn size_for_linear_model(
    wsg: &WingStructureGeometry,
    initial: WingboxSizing,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    fuel_kg_m: &[f64],
    point_masses: &[(f64, f64)],
    limits: LinearModelLimits,
) -> StiffnessSizingResult {
    const MAX_PASSES: usize = 32;
    let floor = initial.clone();
    let mut sizing = initial;
    let mut initial_root_moment = None;
    let mut relative_allocation_change = f64::INFINITY;
    for pass in 1..=MAX_PASSES {
        super::section::update_moment_fractions(&mut sizing, skin, web, cap);
        let running_mass = super::arc_mass::update(wsg, &mut sizing, web, cap);
        let response = analyze_structure_with_running_mass(
            wsg,
            &sizing,
            cfg,
            req,
            engine_cfg,
            mass_cfg,
            skin,
            web,
            cap,
            fuel_kg_m,
            point_masses,
            Some(&running_mass),
        );
        // Added caps change both the clear web panel and its bending stress.
        // Any required web addition must enter mass, relief and EI before a
        // final response can be accepted.
        let count = sizing.y_stations.len();
        let moments: Vec<_> = (0..count)
            .map(|station| {
                response
                    .load_cases
                    .iter()
                    .map(|case| case.moment_nm[station].abs())
                    .fold(0.0_f64, f64::max)
            })
            .collect();
        let shears: Vec<_> = (0..count)
            .map(|station| {
                response
                    .load_cases
                    .iter()
                    .map(|case| case.shear_n[station].abs())
                    .fold(0.0_f64, f64::max)
            })
            .collect();
        if pass < MAX_PASSES
            && super::web::ensure_thickness(&mut sizing, &moments, &shears, skin, web, cap)
        {
            continue;
        }
        // Publish the final response margins on the final section rather
        // than stale margins belonging to its strength-only predecessor.
        for (spar_index, spar) in sizing.spars.iter_mut().enumerate() {
            for (station, margin) in spar.margin_of_safety.iter_mut().enumerate() {
                *margin = response
                    .load_cases
                    .iter()
                    .filter_map(|case| {
                        case.spar_stress
                            .get(spar_index)
                            .and_then(|result| result.margin_of_safety.get(station))
                    })
                    .copied()
                    .fold(f64::INFINITY, |a, b| {
                        if a.is_nan() || b.is_nan() {
                            f64::NAN
                        } else {
                            a.min(b)
                        }
                    });
            }
        }
        let root_moment = response
            .load_cases
            .iter()
            .filter_map(|case| case.moment_nm.first())
            .map(|value| value.abs())
            .fold(0.0_f64, f64::max);
        let original = *initial_root_moment.get_or_insert(root_moment);
        let ribs_remain_conservative =
            root_moment <= original * (1.0 + super::MARGIN_NUMERICAL_ZERO);
        let assessment = assess(&sizing, &response, limits);
        let done = assessment.passes() && ribs_remain_conservative;
        let converged = done && (pass == 1 || relative_allocation_change <= 1.0e-4);
        if converged
            || !assessment.input_valid
            || !ribs_remain_conservative
            || pass == MAX_PASSES
            || assessment.cap_packaging_ratio > 1.0 + super::MARGIN_NUMERICAL_ZERO
        {
            return StiffnessSizingResult {
                sizing,
                response,
                assessment,
                converged,
                iterations: pass,
            };
        }
        let target_slope =
            ((1.0 + limits.max_curvature_relative_error).powf(2.0 / 3.0) - 1.0).sqrt();
        let previous = sizing.clone();
        // Allocate stiffness where it reduces compliance per kilogram, with
        // original strength sections as lower bounds. The actual nonlinear
        // section geometry participates in the stationwise dual search.
        let changed = super::stiffness_distribution::allocate(
            wsg,
            &mut sizing,
            &floor,
            &response,
            cap,
            cfg,
            target_slope * (1.0 - 1.0e-10),
        );
        if !changed {
            return StiffnessSizingResult {
                sizing,
                response,
                assessment,
                converged: done,
                iterations: pass,
            };
        }
        // Added cap mass relieves the next load snapshot. Reallocate from
        // the strength floor rather than accepting a heavy first feasible
        // iterate; half relaxation closes that coupled correction smoothly.
        if pass > 1 {
            for (spar, old) in sizing.spars.iter_mut().zip(&previous.spars) {
                for j in 0..spar.a_cap.len() {
                    spar.w_cap[j] = 0.5 * (spar.w_cap[j] + old.w_cap[j]);
                    spar.t_cap[j] = 0.5 * (spar.t_cap[j] + old.t_cap[j]);
                    spar.a_cap[j] = spar.w_cap[j] * spar.t_cap[j];
                }
            }
        }
        super::arc_mass::update(wsg, &mut sizing, web, cap);
        relative_allocation_change =
            (sizing.total_mass_kg - previous.total_mass_kg).abs() / previous.total_mass_kg.max(1.0);
        for (spar, old) in sizing.spars.iter().zip(&previous.spars) {
            for (area, old_area) in spar.a_cap.iter().zip(&old.a_cap) {
                relative_allocation_change = relative_allocation_change
                    .max((area - old_area).abs() / old_area.abs().max(1.0e-12));
            }
        }
    }
    unreachable!("the last bounded sizing pass always returns")
}
