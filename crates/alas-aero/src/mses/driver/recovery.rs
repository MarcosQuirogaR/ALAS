// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Requested results indexed by schedule position, including exact requested
/// angles recovered later while bridging across an earlier failed point.
pub(super) struct RequestedPolarRecovery {
    pub(super) alphas: Vec<f64>,
    pub(super) points: Vec<Option<RecoveredPolarPoint>>,
    pub(super) visited_count: usize,
}

pub(super) struct RecoveredPolarPoint {
    pub(super) summary: HashMap<String, f64>,
    pub(super) solver_output: String,
    pub(super) checkpoint: Option<MsesConvergedCheckpoint>,
}

impl RequestedPolarRecovery {
    pub(super) fn new(alphas: &[f64]) -> Self {
        Self {
            alphas: alphas.to_vec(),
            points: (0..alphas.len()).map(|_| None).collect(),
            visited_count: 0,
        }
    }

    pub(super) fn missing_requested_index(&self, alpha: f64) -> Option<usize> {
        self.alphas
            .iter()
            .take(self.visited_count)
            .enumerate()
            .position(|(index, requested)| {
                // Arithmetic roundoff from repeated half-degree bridge steps;
                // this is not an aerodynamic incidence acceptance tolerance.
                (requested - alpha).abs() <= 32.0 * f64::EPSILON * requested.abs().max(1.0)
                    && self.points[index].is_none()
            })
    }

    pub(super) fn finish(self, outcome: &mut SweepOutcome) {
        for (index, point) in self.points.into_iter().enumerate() {
            let Some(point) = point else { continue };
            if let Some(diagnostic) = outcome.point_diagnostics.get_mut(index) {
                diagnostic.status = MsesPolarPointStatus::Converged;
                diagnostic.solver_output = point.solver_output;
            }
            for (key, value) in point.summary {
                outcome.accumulated.entry(key).or_default().push(value);
            }
            if let Some(checkpoint) = point.checkpoint {
                outcome.checkpoints.push(checkpoint);
            }
        }
    }
}
