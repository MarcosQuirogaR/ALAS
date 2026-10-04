// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The physical CG limits that apply to one checked loading point, by the
//! flight phase the point stands for.
//!
//! Each named loading state of the model CG assessment carries its own
//! [`alas_opt::envelope::PhysicalCgLimits`], scoped to the state's own phase.
//! A point of the loading potato or the fuel vector is not one of those
//! states: it is looked up here by interpolating, linearly in mass, the
//! limits of the named states re-scoped to the phase of the point. The
//! interpolation is a coarse, documented approximation (no `h_cg` recomputed
//! at an arbitrary sequence point) and clamps outside the bracket instead of
//! extrapolating.

use alas_opt::envelope::{
    AftLimitGovernance, ForwardLimitGovernance, ModelCgEnvelopeAssessment, PhaseLimits,
};

/// The phase a checked point stands for, which selects the CG-limit
/// mechanisms that apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckedPhase {
    /// Partial loading, DOW to ZFW (boarding, cargo, baggage): ground
    /// mechanisms only.
    Loading,
    /// Zero-fuel mass, the start of fuelling: landing trim forward and the
    /// static-margin floor aft, like the ZFW named state.
    ZeroFuel,
    /// Fuel loaded up to takeoff: rotation forward at takeoff, plus the
    /// ground mechanisms.
    Takeoff,
    /// The flown landing point: landing trim forward, plus the ground
    /// mechanisms.
    Landing,
}

impl CheckedPhase {
    /// The mechanisms that apply in this phase.
    #[must_use]
    pub const fn limits(self) -> PhaseLimits {
        match self {
            Self::Loading => PhaseLimits::GROUND,
            Self::ZeroFuel => PhaseLimits::FLIGHT,
            Self::Takeoff => PhaseLimits::TAKEOFF,
            Self::Landing => PhaseLimits::LANDING,
        }
    }

    /// Stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Loading => "partial loading",
            Self::ZeroFuel => "zero fuel",
            Self::Takeoff => "fuel loading to takeoff",
            Self::Landing => "landing",
        }
    }
}

/// Stable report label of a forward-limit mechanism.
#[must_use]
pub const fn forward_mechanism_label(governance: ForwardLimitGovernance) -> &'static str {
    match governance {
        ForwardLimitGovernance::MaxNoseLoadHandling => "maximum nose-gear load",
        ForwardLimitGovernance::ScissorPlotEstimate => "scissor-plot estimate",
        ForwardLimitGovernance::RotationNoseWheelLiftoff => "nose-wheel liftoff (rotation)",
        ForwardLimitGovernance::LandingTrimGroundEffect => "landing trim in ground effect",
    }
}

/// Stable report label of an aft-limit mechanism.
#[must_use]
pub const fn aft_mechanism_label(governance: AftLimitGovernance) -> &'static str {
    match governance {
        AftLimitGovernance::Aerodynamic => "static-margin floor",
        AftLimitGovernance::GroundMinimumNoseLoad => "minimum nose-gear load",
        AftLimitGovernance::TipBack => "tip-back",
    }
}

/// The limits at one mass in one phase, with the governing mechanisms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitsAtMass {
    /// Forward limit, percent MAC; `NaN` when no state supplies a finite one.
    pub fwd_pct_mac: f64,
    /// Mechanism governing [`Self::fwd_pct_mac`].
    pub fwd_governance: ForwardLimitGovernance,
    /// Aft limit, percent MAC; `NaN` when no state supplies a finite one.
    pub aft_pct_mac: f64,
    /// Mechanism governing [`Self::aft_pct_mac`].
    pub aft_governance: AftLimitGovernance,
}

/// Interpolate `(mass, value, tag)` nodes at `mass_kg`; the tag comes from the
/// nearer bracketing node. `None` without a finite node.
fn interpolate<T: Copy>(mut nodes: Vec<(f64, f64, T)>, mass_kg: f64) -> Option<(f64, T)> {
    nodes.retain(|(mass, value, _)| mass.is_finite() && value.is_finite());
    nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let first = *nodes.first()?;
    let last = *nodes.last()?;
    if mass_kg <= first.0 {
        return Some((first.1, first.2));
    }
    if mass_kg >= last.0 {
        return Some((last.1, last.2));
    }
    nodes.windows(2).find_map(|pair| {
        let (lo, hi) = (pair[0], pair[1]);
        (mass_kg >= lo.0 && mass_kg <= hi.0).then(|| {
            let fraction = (mass_kg - lo.0) / (hi.0 - lo.0).max(1.0e-9);
            let tag = if fraction < 0.5 { lo.2 } else { hi.2 };
            (lo.1 + fraction * (hi.1 - lo.1), tag)
        })
    })
}

/// The limits at `mass_kg` for `phase`, interpolated between the named
/// loading states' own limits re-scoped to that phase.
#[must_use]
pub fn limits_at_mass(
    model_cg: &ModelCgEnvelopeAssessment,
    mass_kg: f64,
    phase: CheckedPhase,
) -> LimitsAtMass {
    let scoped: Vec<_> = model_cg
        .loading_states
        .iter()
        .map(|state| (state.mass_kg, state.physical_limits.scoped(phase.limits())))
        .collect();
    let fwd = interpolate(
        scoped
            .iter()
            .map(|(mass, limits)| (*mass, limits.fwd_limit_pct_mac, limits.fwd_limit_governance))
            .collect(),
        mass_kg,
    );
    let aft = interpolate(
        scoped
            .iter()
            .map(|(mass, limits)| (*mass, limits.aft_limit_pct_mac, limits.aft_limit_governance))
            .collect(),
        mass_kg,
    );
    LimitsAtMass {
        fwd_pct_mac: fwd.map_or(f64::NAN, |(value, _)| value),
        fwd_governance: fwd.map_or(ForwardLimitGovernance::MaxNoseLoadHandling, |(_, tag)| tag),
        aft_pct_mac: aft.map_or(f64::NAN, |(value, _)| value),
        aft_governance: aft.map_or(AftLimitGovernance::TipBack, |(_, tag)| tag),
    }
}
