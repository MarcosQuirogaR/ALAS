// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's reduced, in-process Quick Analysis.
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
//! The drawn aircraft is never resized. The takeoff mass is the closure of
//! empty mass, carried payload and the mission fuel the declared route needs
//! (`assess_product_candidate` under the fixed-aircraft mass basis), so
//! "expected MTOW" means the mass this exact geometry must lift to fly the
//! brief, reported next to the declared MTOW requirement. Capacities are
//! reported separately from carried loads.
//!
//! * Lift-to-drag: the sized closure's cruise value (two vortex-lattice
//!   probes on the in-loop mesh plus the Raymer/Korn drag build-up).
//! * Range: Breguet cruise range at the requested cruise Mach and altitude
//!   with the closure lift-to-drag and the catalogue cruise TSFC, from the
//!   takeoff-mass estimate down to the mass with only reserve fuel left.
//!   Climb and descent are not credited separately.
//! * Fuel burn: the closure's block fuel for the declared route.
//! * Cruise speed and ceiling: thrust available from the catalogue deck at
//!   the maximum-climb rating against the parabolic polar fitted from the
//!   reduced full analysis, at the takeoff-mass estimate. The service
//!   ceiling is the altitude where the excess-power rate of climb falls to
//!   0.508 m/s (100 ft/min) at the best-climb Mach, bounded by the deck
//!   domain (13 716 m, Mach 0.9). The achievable Mach is the highest
//!   thrust-equals-drag Mach at the requested altitude, capped at 0.895.
//! * Payload-range: the same corner convention as the report figure (max
//!   payload with fuel to MTOW, max fuel with payload traded, ferry).
//! * Feasibility: the physical feasibility assessment of the reduced full
//!   analysis without a flown mission, plus the closure's dispatch flags.
//!
//! Every value is an initial estimate for design iteration, not a validated
//! performance figure.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::OEW_KEYS;
use alas_mass::dispatch::DispatchStatus;
use alas_opt::assess_product_candidate;

use crate::feasibility::{assess_physical_feasibility, FindingSeverity};
use crate::full_analysis::FullAnalysis;

mod breguet;
mod cruise;
mod types;

pub use breguet::{payload_range_corners, QuickPayloadRange};
pub use cruise::{CruiseSolve, SERVICE_CEILING_CLIMB_RATE_M_S};
pub use types::{
    QuickAnalysisRequest, QuickAnalysisSummary, QuickEvent, QuickFeasibility, QuickFlag,
    QuickMetric, QuickOutcome, QuickStage, QuickValue, QUICK_ANALYSIS_VERSION,
};

/// The reduced-resolution analysis settings the extended stage runs with.
///
/// The fine sweep collapses to the in-loop lattice with at most nine sweep
/// points; the geometry, masses and requirements are unchanged and the mass
/// basis is the fixed aircraft.
pub fn reduced_config(config: &AlasConfig) -> AlasConfig {
    let mut reduced = config.clone();
    reduced.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let analysis = &mut reduced.analysis;
    analysis.fine_spanwise_resolution = analysis
        .fine_spanwise_resolution
        .min(analysis.spanwise_resolution.max(1));
    analysis.fine_chordwise_resolution = analysis
        .fine_chordwise_resolution
        .min(analysis.chordwise_resolution.max(1));
    analysis.sweep_n_points = analysis.sweep_n_points.clamp(3, 9);
    reduced
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

const INITIAL_METRICS: [QuickMetric; 10] = [
    QuickMetric::TakeoffMass,
    QuickMetric::OperatingEmptyMass,
    QuickMetric::PayloadCapacity,
    QuickMetric::CarriedPayload,
    QuickMetric::FuelCapacity,
    QuickMetric::CarriedFuel,
    QuickMetric::CruiseLiftToDrag,
    QuickMetric::Range,
    QuickMetric::FuelBurn,
    QuickMetric::StaticMargin,
];

const EXTENDED_METRICS: [QuickMetric; 5] = [
    QuickMetric::CruiseSpeed,
    QuickMetric::CruiseAltitude,
    QuickMetric::ServiceCeiling,
    QuickMetric::PayloadRange,
    QuickMetric::Feasibility,
];

/// Run the reduced analysis, streaming every metric's terminal state.
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
    let reduced = reduced_config(&request.config);
    let requirements = &reduced.requirements;

    let plane = match AircraftBuilder::new(Some(reduced.geometry.clone()))
        .build(Some(&request.design), true)
    {
        Ok(plane) => plane,
        Err(error) => {
            let message = format!("geometry does not build: {error:?}");
            publisher.fail_all(&INITIAL_METRICS, &message);
            publisher.fail_all(&EXTENDED_METRICS, &message);
            return publisher.summary;
        }
    };
    if cancel.load(Ordering::Relaxed) {
        publisher.summary.cancelled = true;
        return publisher.summary;
    }

    // Stage 1: the fixed-geometry mass and mission closure.
    let mut takeoff_mass_estimate_kg = requirements.mtow_kg;
    let mut dispatch_flags = Vec::new();
    match assess_product_candidate(&reduced, &request.design) {
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
            publisher.value(
                QuickMetric::PayloadCapacity,
                requirements.max_structural_payload_kg,
                None,
                "declared structural payload cap; not derived from the cabin",
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
            publisher.value(
                QuickMetric::FuelCapacity,
                sized.usable_capacity_kg,
                None,
                "usable tank capacity of the resolved tank layout",
            );
            publisher.value(
                QuickMetric::CarriedFuel,
                sized.takeoff_fuel_kg,
                Some(sized.usable_capacity_kg),
                "takeoff fuel for the declared route including reserves",
            );
            publisher.value(
                QuickMetric::CruiseLiftToDrag,
                sized.lift_to_drag,
                None,
                "in-loop vortex lattice with Raymer/Korn drag build-up at the closure cruise CL",
            );
            let reserve_kg = sized.dispatch.plan.reserve_fuel_kg();
            let usable_for_range_kg = (sized.takeoff_mass_kg - sized.zero_fuel_mass_kg)
                .min(sized.usable_capacity_kg)
                .max(0.0);
            match breguet::range_model(&reduced, sized.lift_to_drag) {
                Some(model) => {
                    let end_mass_kg =
                        sized.takeoff_mass_kg - (usable_for_range_kg - reserve_kg).max(0.0);
                    let range_m = model.range_m(sized.takeoff_mass_kg, end_mass_kg);
                    publisher.value(
                        QuickMetric::Range,
                        range_m,
                        sized.mission_distance_known.then_some(sized.design_range_m),
                        &format!(
                            "Breguet cruise at the requested Mach/altitude with the carried fuel less {reserve_kg:.0} kg reserves; {}",
                            model.note
                        ),
                    );
                }
                None => publisher.publish(
                    QuickMetric::Range,
                    QuickOutcome::Unsupported(
                        "no cruise fuel-flow anchor for the selected propulsion model".to_owned(),
                    ),
                ),
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
            let resolved = &assessment.resolved;
            if resolved.mac_m.is_finite() && resolved.mac_m > 0.0 {
                let margin_pct =
                    (resolved.x_neutral_point_m - resolved.cg_x_m) / resolved.mac_m * 100.0;
                publisher.value(
                    QuickMetric::StaticMargin,
                    margin_pct,
                    Some(requirements.min_physical_static_margin * 100.0),
                    "neutral point from the in-loop lattice at the closure centre of gravity",
                );
            } else {
                publisher.publish(
                    QuickMetric::StaticMargin,
                    QuickOutcome::Failed("mean aerodynamic chord is not finite".to_owned()),
                );
            }
            dispatch_flags = dispatch_flags_of(sized);
        }
        Err(error) => publisher.fail_all(&INITIAL_METRICS, &error),
    }
    if cancel.load(Ordering::Relaxed) {
        publisher.summary.cancelled = true;
        return publisher.summary;
    }

    // Stage 2: the reduced full analysis and the thrust-limited envelope.
    let report = match FullAnalysis::new(reduced.clone()).run(&request.design, true) {
        Ok(report) => report,
        Err(error) => {
            publisher.fail_all(
                &EXTENDED_METRICS,
                &format!("reduced full analysis failed: {error}"),
            );
            return publisher.summary;
        }
    };
    if cancel.load(Ordering::Relaxed) {
        publisher.summary.cancelled = true;
        return publisher.summary;
    }

    match CruiseSolve::new(
        &reduced,
        &plane,
        &request.design,
        &report,
        takeoff_mass_estimate_kg,
    ) {
        Ok(solve) => {
            let requested_tas = solve.requested_tas_m_s();
            match solve.achievable_mach_at_requested_altitude() {
                Ok(mach) => publisher.value(
                    QuickMetric::CruiseSpeed,
                    solve.tas_at_requested_altitude(mach),
                    Some(requested_tas),
                    &format!(
                        "thrust-equals-drag Mach {mach:.3} at the requested altitude and {takeoff_mass_estimate_kg:.0} kg, maximum-climb rating; requested Mach {:.3}",
                        requirements.cruise_mach
                    ),
                ),
                Err(message) => publisher.publish(QuickMetric::CruiseSpeed, QuickOutcome::Failed(message)),
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

    match payload_range_corners(&reduced, &report) {
        Some(corners) => publisher.publish(
            QuickMetric::PayloadRange,
            QuickOutcome::PayloadRange(corners),
        ),
        None => publisher.publish(
            QuickMetric::PayloadRange,
            QuickOutcome::Unsupported("fuel capacity or propulsion anchor unavailable".to_owned()),
        ),
    }

    let feasibility = assess_physical_feasibility(&reduced, &request.design, &report, None);
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
    let _ = OEW_KEYS;
    publisher.summary
}

fn dispatch_status_text(status: &DispatchStatus) -> String {
    match status {
        DispatchStatus::Converged => "converged".to_owned(),
        DispatchStatus::MtowLimited { shortfall_kg } => {
            format!("MTOW-limited by {shortfall_kg:.0} kg")
        }
        DispatchStatus::TankLimited { shortfall_kg } => {
            format!("tank-limited by {shortfall_kg:.0} kg")
        }
        DispatchStatus::NotConverged { last_change_kg } => {
            format!("not converged (last change {last_change_kg:.0} kg)")
        }
        DispatchStatus::ModelFailed(message) => format!("model failed: {message}"),
    }
}

fn dispatch_flags_of(sized: &alas_opt::SizedCandidate) -> Vec<QuickFlag> {
    let mut flags = Vec::new();
    match &sized.dispatch.status {
        DispatchStatus::Converged => {}
        other => flags.push(QuickFlag {
            code: "Dispatch".to_owned(),
            blocking: true,
            message: dispatch_status_text(other),
        }),
    }
    if sized.dispatch.landing_mass_exceeds_mlw {
        flags.push(QuickFlag {
            code: "LandingMassLimit".to_owned(),
            blocking: true,
            message: "destination landing mass exceeds the landing mass limit".to_owned(),
        });
    }
    if sized.dispatch.zero_fuel_mass_exceeds_mzfw {
        flags.push(QuickFlag {
            code: "ZeroFuelMassLimit".to_owned(),
            blocking: true,
            message: "zero-fuel mass exceeds the maximum zero-fuel mass".to_owned(),
        });
    }
    if !sized.sizing_closed {
        flags.push(QuickFlag {
            code: "SizingNotClosed".to_owned(),
            blocking: false,
            message: format!(
                "takeoff-mass closure stopped after {} iterations",
                sized.sizing_iterations
            ),
        });
    }
    flags
}

/// Operating empty mass summed from a report's component masses.
pub fn report_oew_kg(report: &crate::full_analysis::AnalysisReport) -> f64 {
    OEW_KEYS
        .iter()
        .map(|key| report.component_masses.get(*key).copied().unwrap_or(0.0))
        .sum()
}
