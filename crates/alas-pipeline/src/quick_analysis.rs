// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's in-process Quick Analysis.
//!
//! Everything here runs in Rust on the calling thread and never launches a
//! process: the dependency set is the geometry builder, the mass and fuel
//! models, the native vortex lattice, the catalogue propulsion deck and the
//! closed-form performance relations. Results stream through a sink as they
//! become ready, tagged with the configuration revision that produced them so
//! a caller can reject late results for geometry that has since changed.
//!
//! # Model basis (fixed geometry)
//!
//! Two stages. The initial stage is the mission-sized closure of the fixed
//! aircraft; the extended stage is the full baseline analysis the sandbox's
//! Full Analysis runs (`FullAnalysis::run` under [`sandbox_config`]), so
//! every extended value is that analysis' own number.
//!
//! The drawn aircraft is never resized. The takeoff mass is the closure of
//! empty mass, carried payload and the mission fuel the declared route needs
//! (`assess_product_candidate` under the fixed-aircraft mass basis), so
//! "expected MTOW" means the mass this exact geometry must lift to fly the
//! brief, reported next to the declared MTOW requirement. Capacities are
//! reported separately from carried loads.
//!
//! * Lift-to-drag and static margin: the full baseline analysis' trimmed
//!   cruise L/D and fine-lattice static margin, the values its summary
//!   reports.
//! * Range: the largest still-air distance whose reserve-inclusive fuel
//!   plan fits the closure's carried fuel at the closure's takeoff mass,
//!   flown on the closure's own mission model (trimmed drag table, deck,
//!   climb, cruise levels and descent; `alas_mass::payload_range`).
//! * Fuel burn: the closure's block fuel for the design mission (the
//!   great circle), and the planned route's (airways where the navigation
//!   data is installed) flown off-design by the full analysis' mission
//!   stage on its report (`route`), the route fuel the full analysis reports.
//! * Cruise speed and ceiling: maximum-climb thrust of the full analysis'
//!   fuel-model deck against its trimmed drag table (`CD(CL, M, h)`, the
//!   drag the mission and the payload-range corners fly), at the
//!   takeoff-mass estimate. The service
//!   ceiling is the altitude where the excess-power rate of climb falls to
//!   0.508 m/s (100 ft/min) at the best-climb Mach, bounded by the deck
//!   domain (13 716 m, Mach 0.9). The achievable Mach is the highest
//!   thrust-equals-drag Mach at the requested altitude, capped at the
//!   smaller of 0.895 and the drag table's upper Mach node.
//! * Payload capacity: the estimated achievable payload of the fixed
//!   aircraft, the declared structural cap bounded by the preset MZFW-derived
//!   limit and by `MTOW - OEW`; the declared cap is the requested value.
//! * Fuel capacity: the usable capacity the full baseline analysis applies
//!   (`assess_fuel_capacity`): the published usable fuel of an unchanged
//!   preset, else the resolved tank layout. The closure's own dispatch is
//!   bounded by the same capacity (`alas_opt::mdo::usable_fuel_capacity`).
//! * Payload-range: the full baseline analysis' corners
//!   ([`payload_range_corners`] on its report; max payload with fuel to MTOW,
//!   max fuel with payload traded, ferry), with the maximum payload bounded
//!   as above so no corner exceeds MTOW; an empty mass at or above MTOW
//!   fails the diagram instead of drawing it.
//! * Feasibility: the physical feasibility assessment of the full baseline
//!   analysis without a flown mission, plus the closure's dispatch flags.
//!
//! [`QuickMetric::basis`] states, per metric, whether the value is the Full
//! Analysis' own or an estimate, and the measured bound of the closure
//! estimates against it. No value is a validated performance figure.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use alas_config::AlasConfig;
use alas_mass::breakdown::OEW_KEYS;
use alas_opt::assess_product_candidate;

use crate::feasibility::{
    assess_airplane_fuel_capacity, assess_physical_feasibility, FindingSeverity,
};
use crate::full_analysis::{effective_structural_payload_limit_kg, AnalysisReport, FullAnalysis};

pub mod band;
pub mod corners;
mod cruise;
mod dispatch_flags;
mod payload_range;
mod route;
mod types;

pub use cruise::{CruiseDrag, CruiseSolve, SERVICE_CEILING_CLIMB_RATE_M_S};
use dispatch_flags::{dispatch_flags_of, dispatch_status_text};
use payload_range::quick_range;
pub use payload_range::{
    fuel_capacity_basis, payload_capacity_estimate, payload_range_corners, PayloadCapacityEstimate,
    PayloadRangeUnavailable, QuickPayloadRange,
};
pub use types::{
    QuickAnalysisRequest, QuickAnalysisSummary, QuickBasis, QuickEvent, QuickFeasibility,
    QuickFlag, QuickMetric, QuickOutcome, QuickRouteFuel, QuickStage, QuickValue,
    QUICK_ANALYSIS_VERSION,
};

/// The configuration both stages analyse: the request's own, closed as the
/// baseline analysis closes it ([`alas_opt::mdo::baseline_closure_config`]:
/// the fixed-aircraft design mode the sandbox's Full Analysis runs with, the
/// route closed on its own takeoff mass under the declared MTOW). The
/// extended stage is that analysis, the closure never resizes the drawn
/// aircraft, and the closure the initial stage publishes is the one the
/// Full Analysis' fuel model carries. Geometry, masses, requirements and
/// resolutions are unchanged.
pub fn sandbox_config(config: &AlasConfig) -> AlasConfig {
    alas_opt::mdo::baseline_closure_config(config)
}

struct Publisher<'a> {
    revision: u64,
    started: Instant,
    sink: &'a mut dyn FnMut(QuickEvent),
    summary: QuickAnalysisSummary,
}

impl Publisher<'_> {
    fn publish(&mut self, metric: QuickMetric, outcome: QuickOutcome) {
        let elapsed_ms = self.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.summary.published += 1;
        if metric.stage() == QuickStage::Initial {
            self.summary.initial_stage_ms = elapsed_ms;
        }
        self.summary.final_ms = elapsed_ms;
        (self.sink)(QuickEvent {
            revision: self.revision,
            metric,
            outcome,
            elapsed_ms,
        });
    }

    fn value(&mut self, metric: QuickMetric, achieved: f64, requested: Option<f64>, note: &str) {
        self.publish(
            metric,
            QuickOutcome::Value(QuickValue {
                achieved,
                requested,
                unit: metric.unit().to_owned(),
                note: note.to_owned(),
            }),
        );
    }

    fn fail_all(&mut self, metrics: &[QuickMetric], message: &str) {
        for &metric in metrics {
            self.publish(metric, QuickOutcome::Failed(message.to_owned()));
        }
    }
}

const INITIAL_METRICS: [QuickMetric; 8] = [
    QuickMetric::TakeoffMass,
    QuickMetric::OperatingEmptyMass,
    QuickMetric::PayloadCapacity,
    QuickMetric::CarriedPayload,
    QuickMetric::FuelCapacity,
    QuickMetric::CarriedFuel,
    QuickMetric::Range,
    QuickMetric::FuelBurn,
];

const EXTENDED_METRICS: [QuickMetric; 8] = [
    QuickMetric::CruiseLiftToDrag,
    QuickMetric::StaticMargin,
    QuickMetric::CruiseSpeed,
    QuickMetric::CruiseAltitude,
    QuickMetric::ServiceCeiling,
    QuickMetric::PayloadRange,
    QuickMetric::RouteFuelBurn,
    QuickMetric::Feasibility,
];

/// Run the Quick Analysis, streaming every metric's terminal state.
///
/// Every metric in [`QuickMetric::ALL`] terminates exactly once unless the
/// run is cancelled, in which case the remaining metrics are left to the
/// caller to treat as abandoned for that revision.
pub fn run_quick_analysis(
    request: &QuickAnalysisRequest,
    sink: &mut dyn FnMut(QuickEvent),
    cancel: &AtomicBool,
) -> QuickAnalysisSummary {
    let mut publisher = Publisher {
        revision: request.revision,
        started: Instant::now(),
        sink,
        summary: QuickAnalysisSummary::default(),
    };
    let sandbox = sandbox_config(&request.config);
    let requirements = &sandbox.requirements;

    // Stage 1: the fixed-geometry mass and mission closure.
    let mut takeoff_mass_estimate_kg = requirements.mtow_kg;
    let mut dispatch_flags = Vec::new();
    match assess_product_candidate(&sandbox, &request.design) {
        Ok(assessment) => {
            let sized = &assessment.sized;
            takeoff_mass_estimate_kg = sized.takeoff_mass_kg;
            let closure_note = format!(
                "mission-sized closure on the fixed aircraft ({}); dispatch: {}",
                sized.sizing_basis,
                dispatch_status_text(&sized.dispatch.status)
            );
            publisher.value(
                QuickMetric::TakeoffMass,
                sized.takeoff_mass_kg,
                Some(requirements.mtow_kg),
                &closure_note,
            );
            publisher.value(
                QuickMetric::OperatingEmptyMass,
                sized.operating_empty_mass_kg,
                None,
                "FLOPS transport build-up at the fixed design weights",
            );
            let payload_capacity = payload_capacity_estimate(
                requirements.max_structural_payload_kg,
                effective_structural_payload_limit_kg(
                    &sandbox,
                    &request.design,
                    sized.operating_empty_mass_kg,
                ),
                sized.payload_kg,
                requirements.mtow_kg,
                sized.operating_empty_mass_kg,
            );
            publisher.value(
                QuickMetric::PayloadCapacity,
                payload_capacity.capacity_kg,
                Some(requirements.max_structural_payload_kg).filter(|cap| *cap > 0.0),
                &format!(
                    "estimated achievable payload of the fixed aircraft, bounded by the {}; not derived from the cabin volume",
                    payload_capacity.basis
                ),
            );
            publisher.value(
                QuickMetric::CarriedPayload,
                sized.payload_kg,
                Some(requirements.max_structural_payload_kg),
                &format!(
                    "declared load case: {} of {} seats, {:.0} kg cargo of {:.0} kg capacity",
                    sized.carried_passengers,
                    sized.passenger_capacity,
                    sized.carried_cargo_payload_kg,
                    sized.cargo_capacity_kg
                ),
            );
            // The capacity the full baseline analysis prices fuel against: the
            // published usable fuel of an unchanged preset, else the resolved
            // tank layout.
            let capacity = FullAnalysis::new(sandbox.clone())
                .build_airplane(&request.design, true)
                .ok()
                .map(|plane| assess_airplane_fuel_capacity(&sandbox, &request.design, &plane))
                .and_then(|capacity| capacity.capacity_kg.map(|kg| (kg, capacity.evidence)));
            match capacity {
                Some((capacity_kg, evidence)) => publisher.value(
                    QuickMetric::FuelCapacity,
                    capacity_kg,
                    None,
                    &format!(
                        "usable fuel capacity the full baseline analysis applies: {}",
                        fuel_capacity_basis(evidence)
                    ),
                ),
                None => publisher.publish(
                    QuickMetric::FuelCapacity,
                    QuickOutcome::Unsupported(
                        "usable fuel capacity is unavailable for this design".to_owned(),
                    ),
                ),
            }
            publisher.value(
                QuickMetric::CarriedFuel,
                sized.takeoff_fuel_kg,
                capacity.map(|(capacity_kg, _)| capacity_kg),
                "takeoff fuel for the declared route including reserves",
            );
            match quick_range(&sandbox, sized) {
                Ok((range_m, reserve_kg)) => publisher.value(
                    QuickMetric::Range,
                    range_m,
                    sized.mission_distance_known.then_some(sized.design_range_m),
                    &format!(
                        "still-air range of the carried fuel on the closure's mission model, {reserve_kg:.0} kg reserves held back under scheme {}",
                        sandbox.fuel_policy.scheme.as_str()
                    ),
                ),
                Err(reason) => {
                    publisher.publish(QuickMetric::Range, QuickOutcome::Unsupported(reason))
                }
            }
            publisher.value(
                QuickMetric::FuelBurn,
                sized.block_fuel_kg,
                None,
                &format!(
                    "block fuel of the segment mission model over {:.0} km",
                    sized.design_range_m / 1000.0
                ),
            );
            dispatch_flags = dispatch_flags_of(sized);
        }
        Err(error) => publisher.fail_all(&INITIAL_METRICS, &error),
    }
    if cancel.load(Ordering::Relaxed) {
        publisher.summary.cancelled = true;
        return publisher.summary;
    }

    // Stage 2: the full baseline analysis the sandbox's Full Analysis runs
    // (`FullAnalysis::run` on the drawn aircraft at the declared design
    // weights, its fuel priced on the baseline fuel model), then the
    // thrust-limited envelope.
    let design = request.design;
    let report = match FullAnalysis::new(sandbox.clone()).run(&design, true) {
        Ok(report) => report,
        Err(error) => {
            publisher.fail_all(
                &EXTENDED_METRICS,
                &format!("full baseline analysis failed: {error}"),
            );
            return publisher.summary;
        }
    };
    if cancel.load(Ordering::Relaxed) {
        publisher.summary.cancelled = true;
        return publisher.summary;
    }

    publish_cruise_point(&mut publisher, &sandbox, &report);

    match CruiseSolve::new(&sandbox, &report, takeoff_mass_estimate_kg) {
        Ok(solve) => {
            let requested_tas = solve.requested_tas_m_s();
            match solve.achievable_mach_at_requested_altitude() {
                Ok(mach) => {
                    let point = if mach >= solve.mach_cap() {
                        format!("Mach {mach:.3}, the drag table's upper Mach node, with thrust to spare")
                    } else {
                        format!("thrust-equals-drag Mach {mach:.3}")
                    };
                    publisher.value(
                        QuickMetric::CruiseSpeed,
                        solve.tas_at_requested_altitude(mach),
                        Some(requested_tas),
                        &format!(
                            "{point} at the requested altitude and {takeoff_mass_estimate_kg:.0} kg, maximum-climb rating; requested Mach {:.3}",
                            requirements.cruise_mach
                        ),
                    )
                }
                Err(message) => {
                    publisher.publish(QuickMetric::CruiseSpeed, QuickOutcome::Failed(message))
                }
            }
            match solve.service_ceiling_m() {
                Ok(ceiling_m) => {
                    publisher.value(
                        QuickMetric::ServiceCeiling,
                        ceiling_m,
                        None,
                        &format!(
                            "altitude where the best excess-power climb rate over the deck Mach domain falls to {SERVICE_CEILING_CLIMB_RATE_M_S} m/s at {takeoff_mass_estimate_kg:.0} kg, maximum-climb rating, bounded by the deck domain of 13716 m"
                        ),
                    );
                    publisher.value(
                        QuickMetric::CruiseAltitude,
                        requirements.cruise_altitude_m.min(ceiling_m),
                        Some(requirements.cruise_altitude_m),
                        "requested altitude capped by the service ceiling",
                    );
                }
                Err(message) => {
                    publisher.publish(
                        QuickMetric::ServiceCeiling,
                        QuickOutcome::Failed(message.clone()),
                    );
                    publisher.publish(QuickMetric::CruiseAltitude, QuickOutcome::Failed(message));
                }
            }
        }
        Err(message) => {
            for metric in [
                QuickMetric::CruiseSpeed,
                QuickMetric::ServiceCeiling,
                QuickMetric::CruiseAltitude,
            ] {
                publisher.publish(metric, QuickOutcome::Unsupported(message.clone()));
            }
        }
    }

    match payload_range_corners(&sandbox, &report) {
        Ok(corners) => publisher.publish(
            QuickMetric::PayloadRange,
            QuickOutcome::PayloadRange(corners),
        ),
        Err(PayloadRangeUnavailable::Unsupported(reason)) => {
            publisher.publish(QuickMetric::PayloadRange, QuickOutcome::Unsupported(reason))
        }
        Err(PayloadRangeUnavailable::Infeasible(reason)) => {
            publisher.publish(QuickMetric::PayloadRange, QuickOutcome::Failed(reason))
        }
    }

    // The planned route, flown off-design by the mission stage's own call.
    publisher.publish(
        QuickMetric::RouteFuelBurn,
        route::route_fuel(&sandbox, &report),
    );

    let feasibility = assess_physical_feasibility(&sandbox, &design, &report, None);
    let mut flags: Vec<QuickFlag> = feasibility
        .findings
        .iter()
        .map(|finding| QuickFlag {
            code: format!("{:?}", finding.code),
            blocking: finding.severity == FindingSeverity::Error,
            message: finding.message.clone(),
        })
        .collect();
    flags.append(&mut dispatch_flags);
    let feasible = flags.iter().all(|flag| !flag.blocking);
    publisher.publish(
        QuickMetric::Feasibility,
        QuickOutcome::Feasibility(QuickFeasibility { feasible, flags }),
    );
    publisher.summary
}

/// The cruise lift-to-drag ratio and static margin the Full Analysis summary
/// reports for `report`: the trimmed cruise point's L/D (the untrimmed
/// design point when trim did not converge) and `report.static_margin`, the
/// fine-lattice neutral point at the report's centre of gravity.
fn publish_cruise_point(
    publisher: &mut Publisher<'_>,
    config: &AlasConfig,
    report: &AnalysisReport,
) {
    let (l_over_d, basis) = report
        .trimmed_design_point
        .as_ref()
        .map(|point| (point.l_over_d, "trimmed"))
        .unwrap_or((report.design_point.l_over_d, "untrimmed"));
    if l_over_d.is_finite() && l_over_d > 0.0 {
        publisher.value(
            QuickMetric::CruiseLiftToDrag,
            l_over_d,
            None,
            &format!(
                "{basis} cruise point of the full baseline analysis: fine vortex lattice with the Raymer/Korn drag build-up"
            ),
        );
    } else {
        publisher.publish(
            QuickMetric::CruiseLiftToDrag,
            QuickOutcome::Failed(format!("cruise L/D {l_over_d} is not usable")),
        );
    }
    if report.static_margin.is_finite() {
        publisher.value(
            QuickMetric::StaticMargin,
            report.static_margin * 100.0,
            Some(config.requirements.min_physical_static_margin * 100.0),
            "fine-lattice neutral point of the full baseline analysis at its centre of gravity",
        );
    } else {
        publisher.publish(
            QuickMetric::StaticMargin,
            QuickOutcome::Failed("static margin is not finite".to_owned()),
        );
    }
}

/// Operating empty mass summed from a report's component masses.
pub fn report_oew_kg(report: &crate::full_analysis::AnalysisReport) -> f64 {
    OEW_KEYS
        .iter()
        .map(|key| report.component_masses.get(*key).copied().unwrap_or(0.0))
        .sum()
}
