// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Topology-aware entry points for the preliminary fixed-wing search.
//!
//! The existing sampler has explicit equations for a horizontal tail,
//! vertical tail, elevator, and three primary actuators. Those equations are
//! cannot represent a T-tail junction or its mass, a V-tail mixer, or an
//! elevon/reflex model by changing a label, so those cases stop here with a
//! typed reason.

use std::fmt;

use crate::topology::{
    TopologyAvailability, TopologyUnavailableReason, UavAnalysisPath, UavTopology,
};

use super::{
    optimize, optimize_with_control, OptimizationError, OptimizationProblem, OptimizationProgress,
    OptimizedUav,
};

/// A preliminary optimization result together with the selected arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct TopologyOptimizedUav {
    /// Arrangement that selected this otherwise unchanged preliminary search.
    pub topology: UavTopology,
    /// Component, sizing, and feasibility result from the deterministic search.
    pub optimized: OptimizedUav,
}

impl TopologyOptimizedUav {
    /// Borrow the underlying preliminary result.
    pub const fn preliminary(&self) -> &OptimizedUav {
        &self.optimized
    }

    /// Consume the wrapper and return the historical preliminary result.
    pub fn into_preliminary(self) -> OptimizedUav {
        self.optimized
    }
}

/// Failure to start or complete a topology-aware preliminary search.
#[derive(Debug, Clone, PartialEq)]
pub enum TopologyOptimizationError {
    /// The selected arrangement needs an omitted sizing/control model.
    UnsupportedTopology {
        /// Requested arrangement.
        topology: UavTopology,
        /// Physical model that is not represented by the preliminary search.
        reason: TopologyUnavailableReason,
    },
    /// Existing deterministic search failure after topology admission.
    Optimization(OptimizationError),
}

impl fmt::Display for TopologyOptimizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTopology { topology, reason } => write!(
                formatter,
                "{} is unavailable for preliminary optimization: {}",
                topology.label(),
                reason.description()
            ),
            Self::Optimization(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TopologyOptimizationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedTopology { .. } => None,
            Self::Optimization(error) => Some(error),
        }
    }
}

/// Run a deterministic preliminary search for a selected arrangement.
///
/// Only arrangements whose sizing, mass, and control equations are explicitly
/// represented enter the deterministic search.
pub fn optimize_for_topology(
    topology: UavTopology,
    problem: &OptimizationProblem<'_>,
) -> Result<TopologyOptimizedUav, TopologyOptimizationError> {
    ensure_optimizer_support(topology)?;
    Ok(TopologyOptimizedUav {
        topology,
        optimized: optimize(problem).map_err(TopologyOptimizationError::Optimization)?,
    })
}

/// Run a topology-aware preliminary search with progress and cancellation.
///
/// Cancellation, sampling order, and ranking exactly follow
/// [`super::optimize_with_control`] for the conventional-tail path.
pub fn optimize_for_topology_with_control(
    topology: UavTopology,
    problem: &OptimizationProblem<'_>,
    on_progress: impl FnMut(OptimizationProgress),
    should_cancel: impl FnMut() -> bool,
) -> Result<TopologyOptimizedUav, TopologyOptimizationError> {
    ensure_optimizer_support(topology)?;
    Ok(TopologyOptimizedUav {
        topology,
        optimized: optimize_with_control(problem, on_progress, should_cancel)
            .map_err(TopologyOptimizationError::Optimization)?,
    })
}

fn ensure_optimizer_support(topology: UavTopology) -> Result<(), TopologyOptimizationError> {
    match topology.availability(UavAnalysisPath::PreliminaryOptimization) {
        TopologyAvailability::Available => Ok(()),
        TopologyAvailability::Unavailable(reason) => {
            Err(TopologyOptimizationError::UnsupportedTopology { topology, reason })
        }
    }
}
