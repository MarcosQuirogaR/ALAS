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

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use alas_config::{AlasConfig, DesignVector};
use alas_opt::mdo::CandidateFuelArtifacts;
use alas_opt::{SegmentMissionModel, SizedCandidate};

use crate::full_analysis::AnalysisReport;

pub use alas_opt::mdo::candidate_mission_model;

/// What a sized candidate's closure carried into its report.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Destination landing mass of the closure's design mission, kg: the
    /// quantity the `landing_mass` residual and the design landing mass limit
    /// are built on.
    pub design_landing_mass_kg: f64,
    /// Physical maximum-fuel loading under Hard MTOW, separate from the
    /// required design-mission plan used by dispatch and mission replay.
    pub takeoff_loading: Option<alas_mass::loading::MtowFuelLoading>,
    /// Mid-cruise mass of the design mission flown from the candidate's own
    /// takeoff loading, kg: the mass the optimizer's level-cruise thrust row
    /// is evaluated at (`SizedCandidate::mid_cruise_mass_kg`).
    pub mid_cruise_mass_kg: f64,
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
            design_landing_mass_kg: sized.design_mission_landing_mass_kg().unwrap_or(f64::NAN),
            takeoff_loading: sized.takeoff_loading,
            mid_cruise_mass_kg: sized.mid_cruise_mass_kg(),
        }
    }
}

/// The fuel artifacts of one report: carried from its sized candidate, or
/// built once on first use for a baseline report.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(into = "StoredReportFuel", from = "StoredReportFuel")]
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
            .get_or_init(|| baseline_artifacts(config, design))
            .clone()
    }

    /// The artifacts already carried or built, without building any.
    pub fn built_artifacts(&self) -> Option<Arc<CandidateFuelArtifacts>> {
        match &self.sized {
            Some(sized) => Some(sized.artifacts.clone()),
            None => self.artifacts.get().and_then(|built| built.clone().ok()),
        }
    }

    /// Store `artifacts`, built by [`baseline_artifacts`] for the same
    /// configuration and design the first [`Self::artifacts`] caller would
    /// pass, unless the report already carries or has built its own.
    fn prime(&self, artifacts: Result<Arc<CandidateFuelArtifacts>, String>) {
        if self.sized.is_none() {
            let _ = self.artifacts.set(artifacts);
        }
    }
}

/// What [`baseline_artifacts`] builds.
type BuiltArtifacts = Result<Arc<CandidateFuelArtifacts>, String>;

/// The artifacts of the baseline aircraft `design` under `config`: the sizing
/// closure of the declared aircraft over its route. While an analysis whose
/// artifacts are built beside it ([`with_baseline_artifacts`]) runs on the
/// same closure input, this is that build: a request from inside the analysis
/// waits for it (or builds it for the second thread) instead of sizing the
/// declared aircraft a second time.
fn baseline_artifacts(config: &AlasConfig, design: &DesignVector) -> BuiltArtifacts {
    match Beside::running(config, design) {
        Some(cell) => cell
            .get_or_init(|| build_baseline_artifacts(config, design))
            .clone(),
        None => build_baseline_artifacts(config, design),
    }
}

fn build_baseline_artifacts(config: &AlasConfig, design: &DesignVector) -> BuiltArtifacts {
    alas_opt::mdo::baseline_fuel_artifacts(config, design)
        .map(Arc::new)
        .map_err(|error| format!("fuel model artifacts unavailable: {error}"))
}

/// The complete input of [`alas_opt::mdo::baseline_fuel_artifacts`]: the
/// configuration its closure runs under (its `Debug` text, as the nominal
/// caches key a configuration) and the bits of the design vector it flies.
#[derive(PartialEq, Eq)]
struct ClosureInput {
    config: String,
    design: Vec<u64>,
}

impl ClosureInput {
    fn of(config: &AlasConfig, design: &DesignVector) -> Self {
        Self {
            config: format!("{:?}", alas_opt::mdo::baseline_closure_config(config)),
            design: design.to_array().into_iter().map(f64::to_bits).collect(),
        }
    }
}

type BesideCell = Arc<OnceLock<BuiltArtifacts>>;

/// The artifacts being built beside the analyses now running, one entry per
/// analysis, removed when it returns.
static BESIDE: Mutex<Vec<(ClosureInput, BesideCell)>> = Mutex::new(Vec::new());

fn beside_entries() -> MutexGuard<'static, Vec<(ClosureInput, BesideCell)>> {
    BESIDE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One running analysis's entry in [`BESIDE`].
struct Beside(BesideCell);

impl Beside {
    fn register(config: &AlasConfig, design: &DesignVector) -> Self {
        let cell = BesideCell::default();
        let input = ClosureInput::of(config, design);
        beside_entries().push((input, cell.clone()));
        Self(cell)
    }

    /// The cell of a running analysis on the closure input of `config` and
    /// `design`.
    fn running(config: &AlasConfig, design: &DesignVector) -> Option<BesideCell> {
        if beside_entries().is_empty() {
            return None;
        }
        let input = ClosureInput::of(config, design);
        beside_entries()
            .iter()
            .find(|(running, _)| *running == input)
            .map(|(_, cell)| cell.clone())
    }

    fn build(&self, config: &AlasConfig, design: &DesignVector) -> BuiltArtifacts {
        self.0
            .get_or_init(|| build_baseline_artifacts(config, design))
            .clone()
    }
}

impl Drop for Beside {
    fn drop(&mut self) {
        beside_entries().retain(|(_, cell)| !Arc::ptr_eq(cell, &self.0));
    }
}

/// `run`, an analysis of `design` under `full`, with the baseline fuel
/// artifacts built beside it.
///
/// They depend only on the configuration and the design, not on the finished
/// report, so they are built on a second thread while the analysis runs and
/// handed to the report when it is for exactly that design. The analysis
/// itself asks for them before it returns (the load plan judges the CG
/// envelope on the closure's CLmax), and that request takes this build
/// ([`baseline_artifacts`]). Either way the report holds what
/// [`ReportFuel::artifacts`] would have built from `full.config` and `design`.
/// The reported cruise point is then bound to the mission the analysed
/// loading flies on its own fuel down to reserves
/// ([`crate::full_analysis::FullAnalysis::bind_design_cruise_point`]).
fn with_baseline_artifacts(
    full: &crate::full_analysis::FullAnalysis,
    design: &DesignVector,
    run: impl FnOnce() -> Result<AnalysisReport, String>,
) -> Result<AnalysisReport, String> {
    let beside = Beside::register(&full.config, design);
    let (report, artifacts) = std::thread::scope(|scope| {
        let artifacts = scope.spawn(|| beside.build(&full.config, design));
        let report = run();
        let artifacts = artifacts
            .join()
            .unwrap_or_else(|error| std::panic::resume_unwind(error));
        (report, artifacts)
    });
    drop(beside);
    let mut report = report?;
    if report.design == *design {
        report.fuel.prime(artifacts);
    }
    full.bind_design_cruise_point(&mut report);
    Ok(report)
}

/// `full.run(design, true)` with the baseline fuel artifacts built beside it
/// ([`with_baseline_artifacts`]).
///
/// # Errors
///
/// The analysis failure, as [`crate::full_analysis::FullAnalysis::run`].
pub(crate) fn run_with_baseline_artifacts(
    full: &crate::full_analysis::FullAnalysis,
    design: &DesignVector,
) -> Result<AnalysisReport, String> {
    with_baseline_artifacts(full, design, || full.run(design, true))
}

/// [`crate::full_analysis::FullAnalysis::run_on_airplane`] with the baseline fuel artifacts built
/// beside it ([`with_baseline_artifacts`]): the aircraft is supplied, the artifacts (the sizing
/// closure of the declared aircraft, seconds of work) are built on a second thread while the
/// analysis runs, and the report holds what [`ReportFuel::artifacts`] would have built from the
/// same configuration and design.
///
/// # Errors
///
/// The analysis failure, as [`crate::full_analysis::FullAnalysis::run_on_airplane`].
pub(crate) fn run_on_airplane_with_baseline_artifacts(
    full: &crate::full_analysis::FullAnalysis,
    design: &DesignVector,
    airplane: alas_geom::aircraft::airplane::Airplane,
) -> Result<AnalysisReport, String> {
    with_baseline_artifacts(full, design, || full.run_on_airplane(design, airplane))
}

/// The model the *flown route* of `report`'s aircraft is priced with
/// ([`SegmentMissionModel::for_candidate_route`]): the route's own level and range,
/// whatever the MTOW mode's design mission.
///
/// # Errors
///
/// Why the artifacts or the model could not be built.
pub fn report_route_mission_model(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<SegmentMissionModel, String> {
    let artifacts = report.fuel.artifacts(config, &report.design)?;
    SegmentMissionModel::for_candidate_route(config, &artifacts)
        .map_err(|error| format!("segment fuel model is not usable: {error}"))
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
        // The report describes the aircraft the search scored, engine included.
        let mut assessed = config.clone();
        let assessed_design =
            crate::acceptance::apply_assessed_aircraft(&assessment.resolved, &mut assessed);
        let report = FullAnalysis::new(assessed.clone())
            .run_sized_candidate(&assessed_design, sized)
            .unwrap_or_else(|error| panic!("report: {error}"));
        let model = report_mission_model(&assessed, &report)
            .unwrap_or_else(|error| panic!("model: {error}"));
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

/// The stored form of a [`ReportFuel`] (`alas --only`'s report cache): the
/// carried record and the artifacts built so far. A build that failed is not
/// stored; the reader builds it again on first use.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredReportFuel {
    sized: Option<SizedFuel>,
    built: Option<Arc<CandidateFuelArtifacts>>,
}

impl From<ReportFuel> for StoredReportFuel {
    fn from(fuel: ReportFuel) -> Self {
        let built = match &fuel.sized {
            Some(_) => None,
            None => fuel.artifacts.get().and_then(|built| built.clone().ok()),
        };
        Self {
            sized: fuel.sized,
            built,
        }
    }
}

impl From<StoredReportFuel> for ReportFuel {
    fn from(stored: StoredReportFuel) -> Self {
        let artifacts = OnceLock::new();
        if let Some(built) = stored.built {
            let _ = artifacts.set(Ok(built));
        }
        Self {
            sized: stored.sized,
            artifacts,
        }
    }
}
