// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native mission lift training and the shared candidate drag adapter.

use std::sync::Arc;

use alas_aero::lift_surrogate::{LiftSurrogate, TrainingGrid};
use alas_aero::vorlax::{VlmGeometry, VlmSettings};
use alas_config::AlasConfig;
use alas_mission::segments::{MissionDragCoefficients, MissionDragModel, MissionDragSource};
use alas_opt::mdo::CandidateDrag;

use crate::full_analysis::AnalysisReport;

struct CandidateMissionDrag {
    drag: CandidateDrag,
}

impl CandidateMissionDrag {
    fn from_report(config: &AlasConfig, report: &AnalysisReport) -> Result<Self, String> {
        let artifacts = report.fuel.artifacts(config, &report.design)?;
        Ok(Self {
            drag: artifacts.drag.clone(),
        })
    }
}

impl MissionDragModel for CandidateMissionDrag {
    fn coefficients(
        &self,
        lift_coefficient: f64,
        mach: f64,
        reynolds_number_per_m: f64,
    ) -> MissionDragCoefficients {
        match &self.drag {
            CandidateDrag::Table(table) => MissionDragCoefficients {
                parasite: table.cd0_at_reynolds_per_m(mach, reynolds_number_per_m),
                induced: table.induced_cd(lift_coefficient),
                wave: table.wave_cd(lift_coefficient, mach),
            },
            // External-solver candidates carry one measured polar instead of
            // a native buildup; reuse that same polar without inventing one.
            CandidateDrag::External(polar) => MissionDragCoefficients {
                parasite: polar.cd0,
                induced: polar.induced_factor_k * lift_coefficient * lift_coefficient,
                wave: polar.wave_cd_at(mach),
            },
        }
    }
}

pub(super) fn candidate_drag_source(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<MissionDragSource, String> {
    Ok(MissionDragSource::SharedCandidate(Arc::new(
        CandidateMissionDrag::from_report(config, report)?,
    )))
}

/// Train lift with the existing vortex lattice. Its induced-drag tables are
/// consumed only by frozen parity; product drag comes from the candidate.
pub(super) fn mission_lift_surrogate(geometry: &VlmGeometry) -> Result<LiftSurrogate, String> {
    LiftSurrogate::train(geometry, &VlmSettings::default(), &TrainingGrid::default())
        .map_err(|error| format!("mission lift surrogate failed: {error}"))
}

#[cfg(test)]
mod tests;
