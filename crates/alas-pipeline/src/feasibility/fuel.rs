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
//!
//! The pure-FLOPS mass architecture already carries unusable fuel inside
//! operating empty mass (an operating item, FLOPS equation 141), so its
//! remainder is usable fuel and nothing is reserved a second time; only a
//! mass model that leaves unusable fuel out of OEW has it reserved here.

use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::tanks::resolve_product_layout;
use alas_opt::mdo::UsableCapacityBasis;

use crate::full_analysis::AnalysisReport;

use super::mission_fuel::MissionFuelAssessment;
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

/// Distinct fuel masses governing one analyzed aircraft load case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelLoadingAssessment {
    /// The maximum-fuel Hard-MTOW design load, retained independently of a
    /// flown route's dispatch. Route telemetry cannot redefine this envelope.
    pub design_takeoff_loading: Option<alas_mass::loading::MtowFuelLoading>,
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
    /// Usable fuel the dispatched route loads at brake release, in kilograms,
    /// when it differs in kind from the analyzed load: under Hard MTOW the
    /// analyzed load is the sized design loading
    /// ([`Self::design_takeoff_loading`]), and the route is flown at its own
    /// dispatch mass. `None` when the analyzed load is the flown one.
    pub flown_carried_fuel_kg: Option<f64>,
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
    /// [`plan_fuel_loading`]'s real [`alas_mass::tanks::FuelTankLayout::resolve`] attempt
    /// failing, means the reservation could not be verified at all, in
    /// that case [`findings`] raises [`FindingCode::FuelTankLayoutUnavailable`]
    /// rather than silently treating the unresolved mass as a verified zero.
    pub unusable_fuel_kg: Option<f64>,
}

impl Default for FuelLoadingAssessment {
    fn default() -> Self {
        Self {
            design_takeoff_loading: None,
            mtow_closure_fuel_kg: f64::NAN,
            usable_capacity: FuelCapacityAssessment::default(),
            analyzed_carried_fuel_kg: f64::NAN,
            carried_fuel_basis: CarriedFuelBasis::CapacityUnverified,
            zero_fuel_mass_kg: f64::NAN,
            analyzed_takeoff_mass_kg: f64::NAN,
            flown_carried_fuel_kg: None,
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
    let analysis_mass_basis_kg = report.loaded_takeoff_mass_kg(config.requirements.mtow_kg);
    let gross_mtow_closure_fuel_kg = report
        .component_masses
        .get("Fuel")
        .copied()
        .unwrap_or(f64::NAN);
    let usable_capacity = assess_fuel_capacity(config, design, report);
    let unusable_fuel_kg = resolved_unusable_fuel_kg(config, design, report);
    let usable_mtow_closure_fuel_kg = usable_closure_fuel_kg(
        gross_mtow_closure_fuel_kg,
        unusable_fuel_kg,
        config.mass_model.mass_architecture.is_pure_flops(),
    );
    let hard_mtow = config.optimizer.objective.mtow_sizing
        == alas_config::MtowSizing::FixedRequirement
        && config.mass_model.mass_architecture.is_pure_flops();
    let zero_fuel_mass_kg = analysis_mass_basis_kg - usable_mtow_closure_fuel_kg;
    let (mass_limit_kg, usable_budget_kg) = if hard_mtow {
        (
            config.requirements.mtow_kg,
            config.requirements.mtow_kg - zero_fuel_mass_kg,
        )
    } else {
        (analysis_mass_basis_kg, usable_mtow_closure_fuel_kg)
    };
    let mut loading = FuelLoadingAssessment {
        unusable_fuel_kg,
        ..plan_from_values(mass_limit_kg, usable_budget_kg, usable_capacity)
    };
    if !hard_mtow {
        loading.design_takeoff_loading = None;
    }
    // A report bound to a sized candidate carries the fuel its dispatch
    // plan loads at brake release; that, not the mass-budget remainder, is
    // the fuel the aircraft takes off with, at the report's own zero-fuel
    // mass.
    if let Some(sized) = report.fuel.sized_fuel() {
        if let Some(design_loading) = sized.takeoff_loading {
            loading.design_takeoff_loading = Some(design_loading);
        }
        let carried_kg = sized.takeoff_fuel_kg;
        if sized.takeoff_loading.is_none() && carried_kg.is_finite() && carried_kg > 0.0 {
            loading.analyzed_carried_fuel_kg = carried_kg;
            loading.analyzed_takeoff_mass_kg = loading.zero_fuel_mass_kg + carried_kg;
            loading.carried_fuel_basis = CarriedFuelBasis::ReservePolicyClosure;
        }
    }
    loading
}

/// The tank-physical mass permanently unusable to the engines, from the same
/// [`alas_mass::tanks::FuelTankLayout`] inventory [`super::mass_balance::assess_mass_balance`]
/// resolves for the CG ledger (same geometry, same
/// shared product tank resolver),
/// not a separate wing-volume approximation. `None` when the layout cannot
/// be resolved; the caller must not treat that the same as a verified zero
/// (see [`findings`]'s [`FindingCode::FuelTankLayoutUnavailable`] check).
/// The usable part of the MTOW fuel closure, kg.
///
/// `unusable_inside_oew` is true when the mass model already counts unusable
/// fuel inside operating empty mass; the closure is then usable fuel as it
/// stands, and subtracting the unusable mass again would count it twice.
fn usable_closure_fuel_kg(
    gross_closure_kg: f64,
    unusable_fuel_kg: Option<f64>,
    unusable_inside_oew: bool,
) -> f64 {
    if unusable_inside_oew {
        gross_closure_kg
    } else {
        gross_closure_kg - unusable_fuel_kg.unwrap_or(0.0)
    }
}

fn resolved_unusable_fuel_kg(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> Option<f64> {
    let tanks = resolve_product_layout(config, design, &report.airplane).ok()?;
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
/// [`alas_mass::tanks::FuelTankLayout`], so it reports `unusable_fuel_kg: Some(0.0)`: callers
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
    let zero_fuel_mass_kg = mtow_kg - mtow_closure_fuel_kg;
    let design_takeoff_loading = alas_mass::loading::MtowFuelLoading::resolve(
        mtow_kg,
        zero_fuel_mass_kg,
        usable_capacity.capacity_kg,
    )
    .ok();
    // The same `min(MTOW - ZFW, capacity)` as the loading above, taken on
    // the closure itself so the carried fuel is exactly the closure or the
    // capacity rather than a re-differenced remainder.
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
    let analyzed_takeoff_mass_kg = zero_fuel_mass_kg + analyzed_carried_fuel_kg;

    FuelLoadingAssessment {
        design_takeoff_loading,
        mtow_closure_fuel_kg,
        usable_capacity,
        analyzed_carried_fuel_kg,
        carried_fuel_basis,
        zero_fuel_mass_kg,
        analyzed_takeoff_mass_kg,
        flown_carried_fuel_kg: None,
        analyzed_landing_mass_kg: None,
        mtow_shortfall_kg: (mtow_kg - analyzed_takeoff_mass_kg).max(0.0),
        mission: MissionFuelAssessment::default(),
        dispatch: None,
        unusable_fuel_kg: Some(0.0),
    }
}

/// Assess usable fuel capacity with explicit provenance for downstream
/// performance figures and feasibility consumers.
pub fn assess_fuel_capacity(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
) -> FuelCapacityAssessment {
    assess_airplane_fuel_capacity(config, design, &report.airplane)
}

/// [`assess_fuel_capacity`] for an aircraft that has been built but not yet
/// analysed: the capacity depends only on the configuration, the design
/// vector and the built geometry.
pub fn assess_airplane_fuel_capacity(
    config: &AlasConfig,
    design: &DesignVector,
    airplane: &Airplane,
) -> FuelCapacityAssessment {
    // The one capacity rule the sizing closure's dispatch applies too.
    match alas_opt::mdo::usable_fuel_capacity(config, design, airplane) {
        Some(capacity) => FuelCapacityAssessment {
            capacity_kg: Some(capacity.kg),
            evidence: match capacity.basis {
                UsableCapacityBasis::PublishedPreset => FuelCapacityEvidence::PublishedPreset,
                UsableCapacityBasis::ResolvedLayout => FuelCapacityEvidence::GeometryEstimate,
            },
        },
        // Resolution failure is missing evidence, not permission to invent
        // fuel.
        None => FuelCapacityAssessment {
            capacity_kg: None,
            evidence: FuelCapacityEvidence::Unavailable,
        },
    }
}

pub(super) fn findings(mtow_kg: f64, fuel_loading: &FuelLoadingAssessment) -> Vec<PhysicalFinding> {
    let mut findings = Vec::new();
    if fuel_loading.unusable_fuel_kg.is_none() {
        // The tank inventory that would reserve unusable fuel out of the
        // MTOW closure budget could not be resolved. `mtow_closure_fuel_kg`
        // therefore carries the *gross* remainder unreserved: flagged here
        // rather than silently treated as a verified zero-unusable-fuel
        // aircraft.
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
    use alas_config::presets;

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
        // silent fallback, no `FuelTankLayoutUnavailable` finding.
        assert_eq!(fuel_loading.unusable_fuel_kg, Some(unusable_fuel_kg));
        assert!(!findings(config.requirements.mtow_kg, &fuel_loading)
            .iter()
            .any(|finding| finding.code == FindingCode::FuelTankLayoutUnavailable));

        // The usable-fuel closure budget is the gross remainder minus the
        // unusable mass the tank layout separately charges in the CG ledger:
        // reserved exactly once, not compensated with a coefficient.
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
        // `zero_fuel_mass_kg` must reserve unusable fuel: the CG ledger adds
        // `unusable_fuel_kg` as real mass on top, so building it from the
        // gross remainder would describe an aircraft weighing
        // `mtow + unusable_fuel_kg`. The assertion above checks that overshoot.
    }

    #[test]
    fn unusable_fuel_is_reserved_only_when_oew_does_not_already_carry_it() {
        assert_eq!(
            usable_closure_fuel_kg(10_000.0, Some(150.0), true),
            10_000.0
        );
        assert_eq!(
            usable_closure_fuel_kg(10_000.0, Some(150.0), false),
            9_850.0
        );
        assert_eq!(usable_closure_fuel_kg(10_000.0, None, false), 10_000.0);
    }

    #[test]
    fn pure_flops_closure_counts_unusable_fuel_once() {
        // The default product architecture is pure FLOPS, whose OEW already
        // holds unusable fuel as an operating item (equation 141).
        let preset = presets::get("A320-200").expect("registered A320 preset");
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
            .expect("A320 config");
        assert!(config.mass_model.mass_architecture.is_pure_flops());
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("A320 full analysis");
        let gross_fuel_kg = report.component_masses["Fuel"];
        let fuel_loading = plan_fuel_loading(&config, &preset.design_vector, &report);
        assert!(fuel_loading.unusable_fuel_kg.is_some_and(|kg| kg > 0.0));
        let design = fuel_loading
            .design_takeoff_loading
            .expect("Hard-MTOW design loading");
        assert_eq!(gross_fuel_kg, design.carried_usable_fuel_kg);
        assert_eq!(
            fuel_loading.mtow_closure_fuel_kg,
            config.requirements.mtow_kg - design.zero_fuel_mass_kg
        );
        assert_eq!(
            fuel_loading.mtow_closure_fuel_kg - gross_fuel_kg,
            design.mtow_margin_kg
        );
        assert_eq!(
            fuel_loading.analyzed_takeoff_mass_kg,
            design.zero_fuel_mass_kg + gross_fuel_kg
        );
    }

    #[test]
    fn an_unresolved_tank_layout_is_a_flagged_finding_not_a_silent_zero() {
        // `plan_from_values` never attempts tank resolution, so it always
        // reports `unusable_fuel_kg: Some(0.0)`: a verified "nothing to
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
}
