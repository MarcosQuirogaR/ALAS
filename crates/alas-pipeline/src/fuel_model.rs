// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel model the pipeline prices every fuel quantity with.
//!
//! There is one: the optimizer's segment mission model
//! ([`alas_opt::SegmentMissionModel`]) flown on the candidate's trimmed drag
//! table and propulsion deck ([`alas_opt::CandidateFuelArtifacts`]). A report
//! bound to a sized candidate carries the artifacts that candidate was sized
//! on, so its dispatch, payload-range corners and band check price exactly the
//! trips the sizing closure priced; nothing here re-derives a polar from the
//! report's sweep. A report with no sized candidate (a baseline analysis)
//! builds the artifacts once, from the sizing closure of the declared aircraft
//! over its route ([`alas_opt::mdo::baseline_fuel_artifacts`]), so the route is
//! priced on the drag table that closure trimmed at its converged mass and
//! centre of gravity.

use std::sync::{Arc, OnceLock};

use alas_config::{AlasConfig, DesignVector};
use alas_opt::mdo::CandidateFuelArtifacts;
use alas_opt::{SegmentMissionModel, SizedCandidate};

use crate::full_analysis::AnalysisReport;

pub use alas_opt::mdo::candidate_mission_model;

/// What a sized candidate's closure carried into its report.
#[derive(Debug, Clone, PartialEq)]
pub struct SizedFuel {
    /// The drag, deck and frozen plan the closure flew.
    pub artifacts: Arc<CandidateFuelArtifacts>,
    /// Takeoff mass of the closure's dispatch solution, kg.
    pub takeoff_mass_kg: f64,
    /// Fuel at brake release of the closure's plan, kg.
    pub takeoff_fuel_kg: f64,
    /// Trip fuel of the closure's plan, kg.
    pub trip_fuel_kg: f64,
    /// Design-mission range the closure was flown over, m.
    pub design_range_m: f64,
}

impl SizedFuel {
    /// The fuel record of `sized`.
    pub fn of(sized: &SizedCandidate) -> Self {
        Self {
            artifacts: sized.fuel_artifacts.clone(),
            takeoff_mass_kg: sized.dispatch.takeoff_mass_kg,
            takeoff_fuel_kg: sized.design_mission_fuel_kg,
            trip_fuel_kg: sized.design_mission_trip_fuel_kg,
            design_range_m: sized.design_range_m,
        }
    }
}

/// The fuel artifacts of one report: carried from its sized candidate, or
/// built once on first use for a baseline report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReportFuel {
    sized: Option<SizedFuel>,
    artifacts: OnceLock<Result<Arc<CandidateFuelArtifacts>, String>>,
}

impl ReportFuel {
    /// The fuel of a report bound to a sized candidate.
    pub fn sized(sized: SizedFuel) -> Self {
        Self {
            sized: Some(sized),
            artifacts: OnceLock::new(),
        }
    }

    /// The sized candidate's record, when the report carries one.
    pub fn sized_fuel(&self) -> Option<&SizedFuel> {
        self.sized.as_ref()
    }

    /// The same aircraft's artifacts without its closure's dispatch plan,
    /// for a load case the plan does not describe (a payload override).
    pub fn without_dispatch_plan(&self) -> Self {
        let artifacts = OnceLock::new();
        if let Some(sized) = &self.sized {
            let _ = artifacts.set(Ok(sized.artifacts.clone()));
        } else if let Some(cached) = self.artifacts.get() {
            let _ = artifacts.set(cached.clone());
        }
        Self {
            sized: None,
            artifacts,
        }
    }

    /// The artifacts the report's fuel is priced on: the carried ones, else
    /// those of the baseline aircraft `design` under `config`, built on the
    /// first call and reused after it.
    ///
    /// # Errors
    ///
    /// Why the baseline artifacts could not be built.
    pub fn artifacts(
        &self,
        config: &AlasConfig,
        design: &DesignVector,
    ) -> Result<Arc<CandidateFuelArtifacts>, String> {
        if let Some(sized) = &self.sized {
            return Ok(sized.artifacts.clone());
        }
        self.artifacts
            .get_or_init(|| {
                alas_opt::mdo::baseline_fuel_artifacts(config, design)
                    .map(Arc::new)
                    .map_err(|error| format!("fuel model artifacts unavailable: {error}"))
            })
            .clone()
    }
}

/// The mission model of `report`'s aircraft under `config`
/// ([`candidate_mission_model`] on [`ReportFuel::artifacts`]).
///
/// # Errors
///
/// Why the artifacts or the model could not be built.
pub fn report_mission_model(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<SegmentMissionModel, String> {
    let artifacts = report.fuel.artifacts(config, &report.design)?;
    candidate_mission_model(config, &artifacts)
        .map_err(|error| format!("segment fuel model is not usable: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_mass::fuel_plan::FuelBurnModel;

    /// The model a sized report prices its design mission with is the model
    /// the sizing closure priced it with: the report's trip at the closure's
    /// takeoff mass and range is the closure's trip to the bit.
    #[test]
    fn a_sized_report_prices_the_closure_trip_to_the_bit() {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let assessment = alas_opt::assess_product_candidate(&config, &design)
            .unwrap_or_else(|error| panic!("assessment: {error}"));
        let sized = &assessment.sized;
        let report = FullAnalysis::new(config.clone())
            .run_sized_candidate(&assessment.resolved.design, sized)
            .unwrap_or_else(|error| panic!("report: {error}"));
        let model =
            report_mission_model(&config, &report).unwrap_or_else(|error| panic!("model: {error}"));
        let trip = model
            .trip(sized.dispatch.takeoff_mass_kg, sized.design_range_m)
            .unwrap_or_else(|error| panic!("trip: {error}"));
        assert_eq!(
            trip.fuel_kg.to_bits(),
            sized.design_mission_trip_fuel_kg.to_bits(),
            "report trip {} kg against closure trip {} kg",
            trip.fuel_kg,
            sized.design_mission_trip_fuel_kg
        );
    }
}
