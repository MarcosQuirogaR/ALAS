// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel-mass roles for product load cases and mission checks.
//!
//! The mass model closes to the configured MTOW by assigning its remaining
//! mass to fuel. That remainder is a mass-budget ceiling, not evidence that a
//! mission requires that much fuel. Keeping capacity, carried fuel, and
//! mission burn in separate fields prevents a tank-limited aircraft from being
//! declared unsafe merely because it takes off below its maximum weight.
//!
//! That remainder is also a *gross* fuel-system mass, not a usable-fuel mass:
//! part of it is permanently unusable fuel trapped in the tanks, which
//! [`super::mass_balance`]'s ledger charges as real, separate mass (the same
//! [`alas_mass::tanks::FuelTankLayout`] this module resolves below). Reserving
//! that mass out of the *usable* fuel budget here, once, keeps
//! `operating empty + payload + usable fuel + unusable fuel == MTOW` instead
//! of letting the analyzed takeoff mass silently exceed MTOW by the unusable
//! amount every time it is nonzero.

use alas_config::{presets, AlasConfig, DesignVector};
use alas_mass::tanks::FuelTankLayout;
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
    /// `MTOW - zero-fuel mass`, in kilograms: the *usable*-fuel budget, with
    /// the resolved tank inventory's unusable fuel already reserved out of
    /// the reported component's gross fuel-system mass (see
    /// [`resolved_unusable_fuel_kg`]).
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
    /// The tank inventory's unusable fuel already reserved out of
    /// `mtow_closure_fuel_kg`, in kilograms. `Some(0.0)` from
    /// [`plan_from_values`] means "nothing left to reserve, by construction"
    /// (a tank-agnostic caller). `None`, produced only by
    /// [`plan_fuel_loading`]'s real [`FuelTankLayout::resolve`] attempt
    /// failing, means the reservation could not be verified at all -- in
    /// that case [`findings`] raises [`FindingCode::FuelTankLayoutUnavailable`]
    /// rather than silently treating the unresolved mass as a verified zero.
    pub unusable_fuel_kg: Option<f64>,
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
            unusable_fuel_kg: None,
        }
    }
}

/// Build the load-case fuel contract before a mission is evaluated.
pub(crate) fn plan_fuel_loading(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> FuelLoadingAssessment {
    let analysis_mass_basis_kg = report_mass_basis_kg(config, report);
    let gross_mtow_closure_fuel_kg = report
        .component_masses
        .get("Fuel")
        .copied()
        .unwrap_or(f64::NAN);
    let usable_capacity = assess_fuel_capacity(config, design, report);
    let unusable_fuel_kg = resolved_unusable_fuel_kg(config, design, report);
    let usable_mtow_closure_fuel_kg = gross_mtow_closure_fuel_kg - unusable_fuel_kg.unwrap_or(0.0);
    FuelLoadingAssessment {
        unusable_fuel_kg,
        ..plan_from_values(
            analysis_mass_basis_kg,
            usable_mtow_closure_fuel_kg,
            usable_capacity,
        )
    }
}

/// Return the takeoff-mass basis used to build a report.
///
/// A mission-sized finalist carries its closed mass in the report provenance,
/// while `config.requirements.mtow_kg` remains the design or regulatory upper
/// limit. Downstream mission and feasibility code must use the former for
/// mass closure and retain the latter only as a limit; otherwise it silently
/// recreates fuel and zero-fuel mass at the heavier ceiling.
pub(crate) fn report_mass_basis_kg(config: &AlasConfig, report: &AnalysisReport) -> f64 {
    let sized = report
        .geometry_summary
        .get("analysis_mass_basis_kg")
        .copied();
    let is_sized = report
        .geometry_summary
        .get("analysis_mass_basis_is_sized")
        .is_some_and(|value| value.is_finite() && *value > 0.5);
    if is_sized {
        if let Some(value) = sized.filter(|value| value.is_finite() && *value > 0.0) {
            return value;
        }
    }
    config.requirements.mtow_kg
}

/// The tank-physical mass permanently unusable to the engines, from the same
/// [`FuelTankLayout`] inventory [`super::mass_balance::assess_mass_balance`]
/// resolves for the CG ledger (same geometry, same
/// [`super::mass_balance::tank_reference`] density/published-volume pair) --
/// not a separate wing-volume approximation. `None` when the layout cannot
/// be resolved; the caller must not treat that the same as a verified zero
/// (see [`findings`]'s [`FindingCode::FuelTankLayoutUnavailable`] check).
fn resolved_unusable_fuel_kg(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> Option<f64> {
    let (density_kg_m3, published_total_l) = super::mass_balance::tank_reference(config, design);
    let tanks = FuelTankLayout::resolve(
        &report.airplane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    )
    .ok()?;
    if config.mass_model.mass_architecture.is_pure_flops() {
        let total_kg = report
            .flops_mass_buildup
            .as_deref()?
            .systems_and_operating_items
            .operating_items
            .unusable_fuel_kg;
        tanks
            .with_unusable_fuel_total(total_kg)
            .ok()
            .map(|adjusted| adjusted.unusable_fuel_kg())
    } else {
        Some(tanks.unusable_fuel_kg())
    }
}

/// Build a load-case fuel contract from already-resolved values.
///
/// This is the tank-agnostic core: it does not itself attempt to resolve a
/// [`FuelTankLayout`], so it reports `unusable_fuel_kg: Some(0.0)` -- callers
/// that bypass tank resolution (direct unit tests, or any caller that has
/// already netted out unusable fuel from `mtow_closure_fuel_kg` itself) are
/// asserting there is nothing left to reserve, which is different from
/// [`plan_fuel_loading`]'s real resolution attempt failing. Only
/// [`plan_fuel_loading`] can produce `None`.
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
        unusable_fuel_kg: Some(0.0),
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
    if fuel_loading.unusable_fuel_kg.is_none() {
        // The tank inventory that would reserve unusable fuel out of the
        // MTOW closure budget could not be resolved. `mtow_closure_fuel_kg`
        // therefore carries the *gross* remainder unreserved, same as the
        // pre-fix behavior -- flagged here rather than silently treated as a
        // verified zero-unusable-fuel aircraft.
        findings.push(PhysicalFinding {
            code: FindingCode::FuelTankLayoutUnavailable,
            severity: FindingSeverity::Warning,
            message: "the fuel-tank arrangement could not be resolved, so unusable fuel was not \
                      reserved out of the MTOW closure budget"
                .to_owned(),
            actual: None,
            limit: None,
            unit: "kg",
        });
    }
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

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    fn atr_fuel_closure_reserves_unusable_fuel_out_of_the_usable_budget() {
        let preset = presets::get("ATR72-600").expect("registered ATR preset");
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
            .expect("ATR config");
        let mut config = config;
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        config.mass_model.apply_architecture();
        let report =
            crate::full_analysis::FullAnalysis::new_reference_compatibility(config.clone())
                .run(&preset.design_vector, true)
                .expect("ATR full analysis");

        let gross_fuel_kg = report
            .component_masses
            .get("Fuel")
            .copied()
            .expect("ATR analysis reports a Fuel component");
        let unusable_fuel_kg = resolved_unusable_fuel_kg(&config, &preset.design_vector, &report)
            .expect("ATR's tank layout resolves");
        assert!(
            unusable_fuel_kg > 0.0,
            "ATR's resolved tank inventory must carry a nonzero unusable fraction, got {unusable_fuel_kg} kg"
        );

        let fuel_loading = plan_fuel_loading(&config, &preset.design_vector, &report);

        // The resolution succeeded, so this is a verified reservation, not a
        // silent fallback -- no `FuelTankLayoutUnavailable` finding.
        assert_eq!(fuel_loading.unusable_fuel_kg, Some(unusable_fuel_kg));
        assert!(!findings(config.requirements.mtow_kg, &fuel_loading)
            .iter()
            .any(|finding| finding.code == FindingCode::FuelTankLayoutUnavailable));

        // The usable-fuel closure budget is the gross remainder minus the
        // unusable mass the tank layout separately charges in the CG ledger
        // -- reserved exactly once, not compensated with a coefficient.
        assert!(
            (fuel_loading.mtow_closure_fuel_kg - (gross_fuel_kg - unusable_fuel_kg)).abs() < 1.0e-6
        );

        // Same-state closure identity: operating empty + payload + usable
        // fuel + unusable fuel equals MTOW exactly when the tanks are not the
        // binding constraint (true for this fixture: usable capacity exceeds
        // the closure remainder, so the analyzed load is the MTOW remainder,
        // not a tank-capped load).
        assert_eq!(
            fuel_loading.carried_fuel_basis,
            CarriedFuelBasis::MtowMassClosure
        );
        assert!(
            (fuel_loading.analyzed_takeoff_mass_kg - config.requirements.mtow_kg).abs() < 1.0e-6,
            "operating empty + payload + usable fuel + unusable fuel should equal MTOW \
             ({} kg), got {} kg",
            config.requirements.mtow_kg,
            fuel_loading.analyzed_takeoff_mass_kg
        );
        // Before this fix, `zero_fuel_mass_kg` was built from the gross
        // remainder without reserving unusable fuel, so
        // `zero_fuel_mass_kg + gross_fuel_kg == mtow` looked closed on its
        // own -- but the CG ledger separately adds `unusable_fuel_kg` as
        // real extra mass on top, so the aircraft it actually described
        // weighed `mtow + unusable_fuel_kg`. The assertion above is that
        // exact overshoot's regression check.
    }

    #[test]
    fn an_unresolved_tank_layout_is_a_flagged_finding_not_a_silent_zero() {
        // `plan_from_values` never attempts tank resolution, so it always
        // reports `unusable_fuel_kg: Some(0.0)` -- a verified "nothing to
        // reserve", not an unresolved unknown. `findings` must therefore stay
        // silent on this axis for every existing `plan_from_values` caller.
        let loading = plan_from_values(10_000.0, 2_000.0, FuelCapacityAssessment::default());
        assert_eq!(loading.unusable_fuel_kg, Some(0.0));
        assert!(!findings(10_000.0, &loading)
            .iter()
            .any(|finding| finding.code == FindingCode::FuelTankLayoutUnavailable));

        // An explicitly unresolved reservation (what `plan_fuel_loading`
        // reports when `FuelTankLayout::resolve` fails) must be flagged, not
        // silently treated as zero.
        let unresolved = FuelLoadingAssessment {
            unusable_fuel_kg: None,
            ..loading
        };
        assert!(findings(10_000.0, &unresolved)
            .iter()
            .any(
                |finding| finding.code == FindingCode::FuelTankLayoutUnavailable
                    && finding.severity == FindingSeverity::Warning
            ));
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
