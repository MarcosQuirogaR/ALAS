// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ground, takeoff, flight, landing and en-route-above-MLW limit sets of
//! the load-and-trim sheet, from the CG gate's per-state physical-limit
//! diagnostics.
//!
//! Every loading state carries each mechanism's own boundary (see
//! `alas_opt::envelope::PhysicalCgLimits`). A set takes, at each state's
//! mass, the governing forward and aft boundary among the mechanisms its
//! `PhaseLimits` scope admits, so each phase is drawn from its own
//! mechanisms and not from the mix of scopes the gate applies per state.
//! The flight set ends at the maximum landing mass: above it an en-route
//! state cannot land and the gate drops landing trim
//! (`PhaseLimits::EN_ROUTE_ABOVE_MLW`), so those states are a set of their
//! own, from the MLW to the heaviest of them, keyed by the forward limit the
//! gate found for them.

use alas_opt::envelope::{ModelCgLoadingAssessment, PhaseLimits};

use super::{LimitVertex, Mechanism};

/// The scoped limit sets, ascending mass.
#[derive(Debug, Default)]
pub(super) struct LimitSets {
    pub(super) ground: Vec<LimitVertex>,
    pub(super) takeoff: Vec<LimitVertex>,
    pub(super) flight: Vec<LimitVertex>,
    pub(super) landing: Vec<LimitVertex>,
    pub(super) en_route_above_mlw: Option<AboveMlwLimits>,
}

/// The limits of the en-route states the gate scoped above the maximum
/// landing mass ([`PhaseLimits::EN_ROUTE_ABOVE_MLW`]: no landing trim, no
/// rotation, the static-margin floor aft).
#[derive(Debug, Clone, PartialEq)]
pub struct AboveMlwLimits {
    /// Forward and aft limit, ascending mass, from the MLW (held at the
    /// lightest such state's values) to the heaviest such state.
    pub vertices: Vec<LimitVertex>,
    /// The mechanism of the forward limit the gate reported for the heaviest
    /// such state (its `fwd_limit_governance`), read from its verdict.
    pub forward_bound: Mechanism,
}

/// Masses that bound the bands, kg.
#[derive(Debug, Clone, Copy)]
pub(super) struct Bands {
    /// Zero-fuel mass: the lightest mass a flight limit applies at.
    pub(super) zfw_kg: f64,
    /// Heaviest takeoff (ramp) mass.
    pub(super) takeoff_kg: f64,
    /// Maximum landing mass: the landing limit runs up to it and the flight
    /// limit ends at it. The gate's own MLW, so the split matches its scoping.
    pub(super) landing_kg: f64,
}

fn vertex(state: &ModelCgLoadingAssessment, phase: PhaseLimits) -> Option<LimitVertex> {
    let fwd = state.physical_limits.fwd_for(phase).0;
    let aft = state.physical_limits.aft_for(phase).0;
    (state.mass_kg.is_finite() && fwd.is_finite() && aft.is_finite()).then_some(LimitVertex {
        mass_kg: state.mass_kg,
        fwd_pct_mac: fwd,
        aft_pct_mac: aft,
    })
}

/// The vertices of `phase` at every state whose mass lies in `[lo, hi]`,
/// ascending, with the last vertex repeated at `extend_to_kg` when that is
/// heavier (limits are held beyond the last state).
fn set_over(
    states: &[&ModelCgLoadingAssessment],
    phase: PhaseLimits,
    (lo, hi): (f64, f64),
    extend_to_kg: f64,
) -> Vec<LimitVertex> {
    let mut vertices: Vec<LimitVertex> = states
        .iter()
        .filter(|s| s.mass_kg >= lo - 1.0 && s.mass_kg <= hi + 1.0)
        .filter_map(|s| vertex(s, phase))
        .collect();
    vertices.sort_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg));
    vertices.dedup_by(|a, b| (a.mass_kg - b.mass_kg).abs() < 1.0);
    if let Some(last) = vertices.last().copied() {
        if extend_to_kg > last.mass_kg + 1.0 {
            vertices.push(LimitVertex {
                mass_kg: extend_to_kg,
                ..last
            });
        }
    }
    vertices
}

/// The en-route-above-MLW set of `states`, `None` when the gate scoped no
/// state above the maximum landing mass `mlw_kg`.
fn above_mlw(states: &[&ModelCgLoadingAssessment], mlw_kg: f64) -> Option<AboveMlwLimits> {
    let scope = PhaseLimits::EN_ROUTE_ABOVE_MLW;
    let above: Vec<_> = states
        .iter()
        .copied()
        .filter(|s| s.scope == scope)
        .collect();
    let heaviest = above
        .iter()
        .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))?;
    let mut vertices = set_over(&above, scope, (f64::NEG_INFINITY, f64::INFINITY), 0.0);
    let lightest = vertices.first().copied()?;
    if mlw_kg.is_finite() && lightest.mass_kg > mlw_kg + 1.0 {
        vertices.insert(
            0,
            LimitVertex {
                mass_kg: mlw_kg,
                ..lightest
            },
        );
    }
    Some(AboveMlwLimits {
        vertices,
        forward_bound: Mechanism::of_forward_governance(
            heaviest.physical_limits.fwd_limit_governance,
        ),
    })
}

/// The limit sets from the gate's loading `states`.
pub(super) fn limit_sets(states: &[&ModelCgLoadingAssessment], bands: Bands) -> LimitSets {
    let heaviest = states
        .iter()
        .map(|s| s.mass_kg)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut landing = set_over(
        states,
        PhaseLimits::LANDING,
        (bands.zfw_kg, bands.landing_kg),
        bands.landing_kg,
    );
    if landing.is_empty() {
        // No state inside the landing band: hold the state nearest to the
        // landing mass across it.
        if let Some(nearest) = states.iter().min_by(|a, b| {
            (a.mass_kg - bands.landing_kg)
                .abs()
                .total_cmp(&(b.mass_kg - bands.landing_kg).abs())
        }) {
            if let Some(v) = vertex(nearest, PhaseLimits::LANDING) {
                landing = [bands.zfw_kg, bands.landing_kg]
                    .into_iter()
                    .map(|mass_kg| LimitVertex { mass_kg, ..v })
                    .collect();
            }
        }
    }
    LimitSets {
        ground: set_over(
            states,
            PhaseLimits::GROUND,
            (f64::NEG_INFINITY, f64::INFINITY),
            bands.takeoff_kg,
        ),
        takeoff: set_over(
            states,
            PhaseLimits::TAKEOFF,
            (bands.zfw_kg, f64::INFINITY),
            bands.takeoff_kg,
        ),
        flight: set_over(
            states,
            PhaseLimits::FLIGHT,
            (bands.zfw_kg, bands.landing_kg.min(heaviest)),
            bands.landing_kg.min(heaviest),
        ),
        landing,
        en_route_above_mlw: above_mlw(states, bands.landing_kg),
    }
}
