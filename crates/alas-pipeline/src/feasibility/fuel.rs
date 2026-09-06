// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel-mass roles for product load cases and mission checks.
//!
//! The mass model closes to the configured MTOW by assigning its remaining
//! mass to fuel. That remainder is a mass-budget ceiling, not evidence that a
//! mission requires that much fuel. Keeping capacity, carried fuel, and
//! mission burn in separate fields prevents a tank-limited aircraft from being
//! declared unsafe merely because it takes off below its maximum weight.

use alas_config::{presets, AlasConfig, DesignVector};
use alas_mission::MissionResult;
use alas_opt::wing_fuel_volume_m3;

use crate::full_analysis::AnalysisReport;

use super::{DispatchAssessment, FindingCode, FindingSeverity, PhysicalFinding};

/// Provenance of the usable-fuel capacity applied to one design result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuelCapacityEvidence {
    /// Revision-locked manufacturer data for an unchanged registered preset.
    PublishedPreset,
    /// Geometry-based estimate for a changed or notional design.
    GeometryEstimate,
    /// No usable capacity could be established.
    Unavailable,
}

/// Usable-fuel capacity and the evidence from which it was obtained.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelCapacityAssessment {
    /// Capacity at the source's declared density, in kilograms.
    pub capacity_kg: Option<f64>,
    /// Whether the value is published data or a geometry estimate.
    pub evidence: FuelCapacityEvidence,
}

impl Default for FuelCapacityAssessment {
    fn default() -> Self {
        Self {
            capacity_kg: None,
            evidence: FuelCapacityEvidence::Unavailable,
        }
    }
}

/// Evidence used to select the fuel actually carried by the analyzed load case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CarriedFuelBasis {
    /// The MTOW mass-closure remainder fits the established usable capacity.
    MtowMassClosure,
    /// Usable capacity limits the carried mass below the MTOW remainder.
    UsableFuelCapacity,
    /// Capacity is unknown, so the analysis assumes the MTOW remainder.
    #[default]
    CapacityUnverified,
    /// The fuel the policy requires for the flown route, bounded by the
    /// takeoff-mass limit and the tanks.
    ReservePolicyClosure,
}

/// Outcome of relating mission telemetry to the analyzed fuel load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissionFuelStatus {
    /// Mission analysis was disabled for this run.
    #[default]
    NotRequested,
    /// Mission analysis was enabled but produced no result.
    Unavailable,
    /// Telemetry exists, but one or more segment solves did not converge.
    NotConverged,
    /// Every modeled segment completed without crossing the dry-mass floor.
    Completed,
    /// The load case crossed its dry-mass floor before completing the mission.
    Exhausted,
}

/// Mission fuel burn and the trip-fuel requirement it establishes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MissionFuelAssessment {
    /// Typed completeness state for the mission fuel result.
    pub status: MissionFuelStatus,
    /// Fuel consumed by the available telemetry, in kilograms.
    pub burned_fuel_kg: Option<f64>,
    /// Fuel required for the modeled trip, excluding unmodeled reserves.
    ///
    /// This is available only after a complete, converged mission. An
    /// exhausted or non-converged trajectory establishes no total requirement.
    pub required_trip_fuel_kg: Option<f64>,
}

/// Distinct fuel masses governing one analyzed aircraft load case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelLoadingAssessment {
    /// `MTOW - zero-fuel mass`, in kilograms.
    pub mtow_closure_fuel_kg: f64,
    /// Established usable tank capacity, if available.
    pub usable_capacity: FuelCapacityAssessment,
    /// Fuel loaded into the analyzed aircraft, in kilograms.
    pub analyzed_carried_fuel_kg: f64,
    /// Evidence or assumption controlling the analyzed carried fuel.
    pub carried_fuel_basis: CarriedFuelBasis,
    /// Operating empty plus payload mass for this load case, in kilograms.
    pub zero_fuel_mass_kg: f64,
    /// Takeoff mass actually analyzed, in kilograms.
    pub analyzed_takeoff_mass_kg: f64,
    /// Landing mass reached by a complete mission, when available, in kilograms.
    ///
    /// This is an analyzed state, not the maximum-landing-mass limit or the
    /// fallback fraction used by preliminary field-performance screening.
    pub analyzed_landing_mass_kg: Option<f64>,
    /// Takeoff mass the tanks cannot supply below the configured MTOW, in
    /// kilograms: zero when a full load reaches MTOW. It describes the tank
    /// bound, not the flown load case, so a policy closure leaves it as is.
    pub mtow_shortfall_kg: f64,
    /// Mission burn/requirement result for the analyzed load case.
    pub mission: MissionFuelAssessment,
    /// The fuel policy as applied to the flown mission, when one was flown.
    pub dispatch: Option<DispatchAssessment>,
}

impl Default for FuelLoadingAssessment {
    fn default() -> Self {
        Self {
            mtow_closure_fuel_kg: f64::NAN,
            usable_capacity: FuelCapacityAssessment::default(),
            analyzed_carried_fuel_kg: f64::NAN,
            carried_fuel_basis: CarriedFuelBasis::CapacityUnverified,
            zero_fuel_mass_kg: f64::NAN,
            analyzed_takeoff_mass_kg: f64::NAN,
            analyzed_landing_mass_kg: None,
            mtow_shortfall_kg: f64::NAN,
            mission: MissionFuelAssessment::default(),
            dispatch: None,
        }
    }
}

/// Build the load-case fuel contract before a mission is evaluated.
pub(crate) fn plan_fuel_loading(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> FuelLoadingAssessment {
    let mtow_closure_fuel_kg = report
        .component_masses
        .get("Fuel")
        .copied()
        .unwrap_or(f64::NAN);
    let usable_capacity = assess_fuel_capacity(config, design, report);
    plan_from_values(
        config.requirements.mtow_kg,
        mtow_closure_fuel_kg,
        usable_capacity,
    )
}

pub(super) fn plan_from_values(
    mtow_kg: f64,
    mtow_closure_fuel_kg: f64,
    usable_capacity: FuelCapacityAssessment,
) -> FuelLoadingAssessment {
    let nonnegative_closure_kg = mtow_closure_fuel_kg.max(0.0);
    let (analyzed_carried_fuel_kg, carried_fuel_basis) = match usable_capacity.capacity_kg {
        Some(capacity_kg) if capacity_kg.is_finite() => {
            let carried_kg = nonnegative_closure_kg.min(capacity_kg.max(0.0));
            let basis = if carried_kg < nonnegative_closure_kg {
                CarriedFuelBasis::UsableFuelCapacity
            } else {
                CarriedFuelBasis::MtowMassClosure
            };
            (carried_kg, basis)
        }
        _ => (nonnegative_closure_kg, CarriedFuelBasis::CapacityUnverified),
    };
    let zero_fuel_mass_kg = mtow_kg - mtow_closure_fuel_kg;
    let analyzed_takeoff_mass_kg = zero_fuel_mass_kg + analyzed_carried_fuel_kg;

    FuelLoadingAssessment {
        mtow_closure_fuel_kg,
        usable_capacity,
        analyzed_carried_fuel_kg,
        carried_fuel_basis,
        zero_fuel_mass_kg,
        analyzed_takeoff_mass_kg,
        analyzed_landing_mass_kg: None,
        mtow_shortfall_kg: (mtow_kg - analyzed_takeoff_mass_kg).max(0.0),
        mission: MissionFuelAssessment::default(),
        dispatch: None,
    }
}

/// Attach the mission burn and any defensible trip-fuel requirement.
pub(crate) fn assess_mission_fuel(
    mission_requested: bool,
    mission: Option<&MissionResult>,
) -> MissionFuelAssessment {
    if !mission_requested {
        return MissionFuelAssessment::default();
    }
    let Some(result) = mission else {
        return MissionFuelAssessment {
            status: MissionFuelStatus::Unavailable,
            ..MissionFuelAssessment::default()
        };
    };
    if let Some(exhaustion) = &result.fuel_exhaustion {
        return MissionFuelAssessment {
            status: MissionFuelStatus::Exhausted,
            burned_fuel_kg: Some(exhaustion.burned_fuel_kg),
            required_trip_fuel_kg: None,
        };
    }

    let burned_fuel_kg = result.fuel_burned_kg();
    let Some(summary) = result.completed_summary() else {
        return MissionFuelAssessment {
            status: MissionFuelStatus::NotConverged,
            burned_fuel_kg: burned_fuel_kg.is_finite().then_some(burned_fuel_kg),
            required_trip_fuel_kg: None,
        };
    };
    MissionFuelAssessment {
        status: MissionFuelStatus::Completed,
        burned_fuel_kg: Some(summary.trip_fuel_kg),
        required_trip_fuel_kg: Some(summary.trip_fuel_kg),
    }
}

/// Assess usable fuel capacity with explicit provenance for downstream
/// performance figures and feasibility consumers.
pub fn assess_fuel_capacity(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> FuelCapacityAssessment {
    if let Ok(preset) = presets::get(&config.preset) {
        if *design == preset.design_vector {
            if let Some(capacity_kg) = preset.reference.usable_fuel_mass_kg {
                return FuelCapacityAssessment {
                    capacity_kg: Some(capacity_kg),
                    evidence: FuelCapacityEvidence::PublishedPreset,
                };
            }
        }
    }

    let capacity_kg = report.airplane.wings.first().and_then(|wing| {
        valid_positive_capacity(
            wing_fuel_volume_m3(wing, config.mass_model.fuel_tank_usable_fraction)
                * config.mass_model.fuel_density_kg_m3,
        )
    });
    FuelCapacityAssessment {
        capacity_kg,
        evidence: if capacity_kg.is_some() {
            FuelCapacityEvidence::GeometryEstimate
        } else {
            FuelCapacityEvidence::Unavailable
        },
    }
}

/// Reject malformed derived evidence rather than allowing NaN, infinity, zero,
/// or negative tank masses to masquerade as a geometric capacity.
fn valid_positive_capacity(capacity_kg: f64) -> Option<f64> {
    (capacity_kg.is_finite() && capacity_kg > 0.0).then_some(capacity_kg)
}

pub(super) fn findings(mtow_kg: f64, fuel_loading: &FuelLoadingAssessment) -> Vec<PhysicalFinding> {
    let mut findings = Vec::new();
    let mtow_closure_fuel_kg = fuel_loading.mtow_closure_fuel_kg;
    if !mtow_closure_fuel_kg.is_finite() || mtow_closure_fuel_kg <= 0.0 {
        findings.push(PhysicalFinding {
            code: FindingCode::NonPositiveFuel,
            severity: FindingSeverity::Error,
            message: "MTOW mass closure leaves no positive finite fuel mass".to_owned(),
            actual: Some(mtow_closure_fuel_kg),
            limit: Some(0.0),
            unit: "kg",
        });
    }

    match fuel_loading.usable_capacity.capacity_kg {
        Some(capacity)
            if capacity.is_finite()
                && capacity > 0.0
                && mtow_closure_fuel_kg.is_finite()
                && mtow_closure_fuel_kg > capacity =>
        {
            let evidence = match fuel_loading.usable_capacity.evidence {
                FuelCapacityEvidence::PublishedPreset => "published usable fuel capacity",
                FuelCapacityEvidence::GeometryEstimate => "geometry-estimated usable tank capacity",
                FuelCapacityEvidence::Unavailable => "usable tank capacity",
            };
            // The tanks bound the takeoff mass at the zero-fuel mass plus
            // the capacity whatever load case is flown, so the finding quotes
            // that bound rather than the analyzed (possibly policy) mass.
            let tank_bound_takeoff_mass_kg = fuel_loading.zero_fuel_mass_kg + capacity;
            findings.push(PhysicalFinding {
                code: FindingCode::TankLimitedTakeoffMass,
                severity: FindingSeverity::Warning,
                message: format!(
                    "{evidence} limits the takeoff mass to {tank_bound_takeoff_mass_kg:.3} kg, below the {mtow_kg:.3} kg MTOW limit"
                ),
                actual: Some(mtow_closure_fuel_kg),
                limit: Some(capacity),
                unit: "kg",
            });
        }
        Some(capacity) if !capacity.is_finite() || capacity <= 0.0 => {
            findings.push(PhysicalFinding {
                code: FindingCode::FuelCapacityUnavailable,
                severity: FindingSeverity::Error,
                message: "usable-fuel capacity cannot be established for the analyzed aircraft"
                    .to_owned(),
                actual: None,
                limit: None,
                unit: "kg",
            })
        }
        None => findings.push(PhysicalFinding {
            code: FindingCode::FuelCapacityUnavailable,
            severity: FindingSeverity::Error,
            message: "usable-fuel capacity cannot be established for the analyzed aircraft"
                .to_owned(),
            actual: None,
            limit: None,
            unit: "kg",
        }),
        _ => {}
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tank_limit_reduces_takeoff_mass_without_relabeling_the_mtow_remainder() {
        let loading = plan_from_values(
            78_000.0,
            19_399.954_722_136_66,
            FuelCapacityAssessment {
                capacity_kg: Some(19_334.0),
                evidence: FuelCapacityEvidence::PublishedPreset,
            },
        );

        assert_eq!(loading.mtow_closure_fuel_kg, 19_399.954_722_136_66);
        assert_eq!(loading.analyzed_carried_fuel_kg, 19_334.0);
        assert_eq!(
            loading.carried_fuel_basis,
            CarriedFuelBasis::UsableFuelCapacity
        );
        assert!((loading.analyzed_takeoff_mass_kg - 77_934.045_277_863_34).abs() < 1.0e-9);
        assert!((loading.mtow_shortfall_kg - 65.954_722_136_66).abs() < 1.0e-9);
    }

    #[test]
    fn absent_capacity_is_explicit_even_when_the_analysis_uses_mass_closure_fuel() {
        let loading = plan_from_values(10_000.0, 2_000.0, FuelCapacityAssessment::default());

        assert_eq!(loading.analyzed_carried_fuel_kg, 2_000.0);
        assert_eq!(
            loading.carried_fuel_basis,
            CarriedFuelBasis::CapacityUnverified
        );
        assert_eq!(loading.analyzed_takeoff_mass_kg, 10_000.0);
        assert!(findings(10_000.0, &loading).iter().any(|finding| {
            finding.code == FindingCode::FuelCapacityUnavailable
                && finding.severity == FindingSeverity::Error
        }));
    }

    #[test]
    fn a_geometry_capacity_limits_a_notional_load_case_by_the_same_conservation_rule() {
        let loading = plan_from_values(
            1_000.0,
            300.0,
            FuelCapacityAssessment {
                capacity_kg: Some(240.0),
                evidence: FuelCapacityEvidence::GeometryEstimate,
            },
        );

        assert_eq!(loading.analyzed_carried_fuel_kg, 240.0);
        assert_eq!(loading.analyzed_takeoff_mass_kg, 940.0);
        assert_eq!(loading.mtow_shortfall_kg, 60.0);
    }

    #[test]
    fn malformed_derived_capacities_are_unavailable() {
        for capacity_kg in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(valid_positive_capacity(capacity_kg), None);
        }
        assert_eq!(valid_positive_capacity(1.0), Some(1.0));
    }

    #[test]
    fn malformed_capacity_evidence_produces_an_unavailable_finding() {
        for capacity_kg in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let loading = plan_from_values(
                1_000.0,
                300.0,
                FuelCapacityAssessment {
                    capacity_kg: Some(capacity_kg),
                    evidence: FuelCapacityEvidence::GeometryEstimate,
                },
            );
            assert!(findings(1_000.0, &loading).iter().any(|finding| {
                finding.code == FindingCode::FuelCapacityUnavailable
                    && finding.severity == FindingSeverity::Error
            }));
        }
    }

    #[test]
    fn partial_mission_telemetry_does_not_establish_required_trip_fuel() {
        let mission = MissionResult {
            segments: Vec::new(),
            solutions: Vec::new(),
            scheduled_segment_count: 1,
            fuel_exhaustion: None,
        };

        let assessment = assess_mission_fuel(true, Some(&mission));

        assert_eq!(assessment.status, MissionFuelStatus::NotConverged);
        assert_eq!(assessment.required_trip_fuel_kg, None);
    }
}
