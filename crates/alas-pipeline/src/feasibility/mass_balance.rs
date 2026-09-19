// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item-level mass statement of a completed run: tanks resolved on the
//! built geometry, every component at its geometry-derived station, and the
//! mass, centre of gravity and inertia tensor of each named loading state.
//!
//! The lumped ten-group breakdown the analysis carries is the input; this
//! stage is where it becomes evidence a reviewer can check item by item.
//! The states are the operating empty aircraft, the zero-fuel aircraft, the
//! aircraft at the flown takeoff and landing masses, and the aircraft at the
//! largest fuel load its tanks and takeoff-mass limit admit. Fuel sits in
//! its tanks in the burn order the arrangement declares, so the centre of
//! gravity of each state is the tanks', not a single wing point.

use alas_config::{AlasConfig, DesignVector};
use alas_mass::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR,
    H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_mass::ledger::{MassItem, MassProperties};
use alas_mass::statement::{
    LedgerMethods, LoadState, MassStatement, MassStatementInputs, PayloadItemSummary,
};
use alas_mass::stations::{component_stations_with_gear, StationError};
use alas_mass::tanks::FuelTankLayout;

use crate::full_analysis::AnalysisReport;

mod summaries;
pub use summaries::{LedgerItemSummary, MassBalanceAssessment, MassStateSummary, TankSummary};

use super::{
    report_mass_basis_kg, FindingCode, FindingSeverity, FuelLoadingAssessment, PhysicalFinding,
};

/// Fraction of the takeoff fuel assumed to remain at landing when no flown
/// mission supplies a landing mass; the same operational-reserve convention
/// the model CG envelope uses for its reserve loading state.
const FALLBACK_LANDING_FUEL_FRACTION: f64 = 0.10;

/// Difference in percent MAC beyond which the ledger and the lumped model
/// are reported as disagreeing about the takeoff centre of gravity.
const CG_DISAGREEMENT_PCT_MAC: f64 = 5.0;

/// Fuel-load steps of the reported centre-of-gravity travel curve.
const FUEL_CG_CURVE_STEPS: usize = 24;

/// Build the statement and append its findings.
///
/// Returns `None`, with a warning finding, when the geometry cannot place
/// its components or the tank arrangement cannot be resolved on it; the
/// lumped model then remains the only mass evidence, and the report says so.
///
/// The one station failure that is *not* a warning is
/// [`StationError::MainGearStationNotMeasured`]: the lumped model cannot
/// stand in for it, because the lumped gear point is the same refused
/// station. That case pushes an error finding, so the run is infeasible
/// rather than reported without a mass statement.
pub(super) fn assess_mass_balance(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    findings: &mut Vec<PhysicalFinding>,
) -> Option<MassBalanceAssessment> {
    let warn = |findings: &mut Vec<PhysicalFinding>, code, message: String| {
        findings.push(PhysicalFinding {
            code,
            severity: FindingSeverity::Warning,
            message,
            actual: None,
            limit: None,
            unit: "",
        });
    };
    let stations = match component_stations_with_gear(
        &report.airplane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    ) {
        Ok(stations) => stations,
        Err(
            error @ StationError::MainGearStationNotMeasured {
                wing_root_z_m,
                fuselage_crown_z_m,
            },
        ) => {
            // A missing main-gear datum is not a ledger that degraded to the
            // lumped model: it is a component this aircraft has no measured
            // station for. Reported as a warning the run stays feasible under
            // it, so the design would be published with its ground reactions
            // silently taken from a station the mass model refused. It is an
            // error finding, which `FeasibilityReport::is_feasible` fails on.
            //
            // The two heights are kept as the finding's own measured pair
            // (geometry frame, z up, m): the wing root leading edge, and the
            // fuselage crown at that station that the wing-mounted fallback
            // rule requires it to lie at or below.
            findings.push(PhysicalFinding {
                code: FindingCode::MassLedgerUnavailable,
                severity: FindingSeverity::Error,
                message: format!("component stations could not be placed: {error}"),
                actual: Some(wing_root_z_m),
                limit: Some(fuselage_crown_z_m),
                unit: "m",
            });
            return None;
        }
        Err(error) => {
            warn(
                findings,
                FindingCode::MassLedgerUnavailable,
                format!("component stations could not be placed: {error}"),
            );
            return None;
        }
    };
    let (density_kg_m3, published_total_l) = tank_reference(config, design);
    let resolved_tanks = match FuelTankLayout::resolve(
        &report.airplane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    ) {
        Ok(tanks) => tanks,
        Err(error) => {
            warn(
                findings,
                FindingCode::FuelTankLayoutUnavailable,
                format!("the fuel-tank arrangement could not be resolved: {error}"),
            );
            return None;
        }
    };
    // A pure FLOPS report carries the authoritative unusable-fuel total from
    // equation 121.  Keep the resolved tank positions and capacities, but
    // make their per-tank unusable rows sum to that same total so the ledger
    // cannot compare a policy fraction with the FLOPS operating-item slot.
    let flops_groups = report
        .flops_mass_buildup
        .as_deref()
        .map(|buildup| &buildup.systems_and_operating_items);
    if config.mass_model.mass_architecture.is_pure_flops() && flops_groups.is_none() {
        warn(
            findings,
            FindingCode::MassLedgerUnavailable,
            "the pure FLOPS mass report carries no grouped systems evaluation for the ledger"
                .to_owned(),
        );
        return None;
    }
    let tanks = if let Some(flops) = flops_groups {
        match resolved_tanks.with_unusable_fuel_total(flops.operating_items.unusable_fuel_kg) {
            Ok(tanks) => tanks,
            Err(error) => {
                warn(
                    findings,
                    FindingCode::FuelTankLayoutUnavailable,
                    format!(
                        "the FLOPS unusable-fuel total could not be placed in the tanks: {error}"
                    ),
                );
                return None;
            }
        }
    } else {
        resolved_tanks
    };
    let Some(masses) = lumped_masses(report) else {
        warn(
            findings,
            FindingCode::MassLedgerUnavailable,
            "the analysis report carries no complete component mass breakdown".to_owned(),
        );
        return None;
    };
    let payload_items = payload_items(report);
    let capacity_kg = tanks.usable_capacity_kg();
    let takeoff_fuel_kg = fuel_loading
        .analyzed_carried_fuel_kg
        .max(0.0)
        .min(capacity_kg);
    let landing_fuel_kg = fuel_loading
        .analyzed_landing_mass_kg
        .map(|mass| (mass - fuel_loading.zero_fuel_mass_kg).max(0.0))
        .unwrap_or(FALLBACK_LANDING_FUEL_FRACTION * takeoff_fuel_kg)
        .min(takeoff_fuel_kg);
    let fuel_items = |fuel_kg: f64| -> Result<Vec<MassItem>, String> {
        tanks
            .distribute(fuel_kg)
            .map(|state| state.mass_items(&tanks))
            .map_err(|error| error.to_string())
    };
    let (takeoff_fuel_items, landing_fuel_items) =
        match (fuel_items(takeoff_fuel_kg), fuel_items(landing_fuel_kg)) {
            (Ok(takeoff), Ok(landing)) => (takeoff, landing),
            (Err(error), _) | (_, Err(error)) => {
                warn(
                    findings,
                    FindingCode::FuelTankLayoutUnavailable,
                    format!("the analyzed fuel could not be placed in the tanks: {error}"),
                );
                return None;
            }
        };
    // The lumped breakdown this stage receives carries whichever methods the
    // configuration selected.  A pure FLOPS report also carries the grouped
    // evaluation that produced those slots; pass that same object through so
    // the ledger cannot relabel a lumped approximation as a FLOPS buildup.
    let statement = match MassStatement::build_with_methods(
        MassStatementInputs {
            masses: &masses,
            stations: &stations,
            payload_items: &payload_items,
            takeoff_fuel_items,
            landing_fuel_items,
            unusable_fuel_items: tanks.unusable_items(),
            flops: flops_groups,
        },
        LedgerMethods::from_mass_model(&config.mass_model),
    ) {
        Ok(statement) => statement,
        Err(error) => {
            warn(
                findings,
                FindingCode::MassLedgerUnavailable,
                format!("the mass ledger is not physical: {error}"),
            );
            return None;
        }
    };

    let (mac_le_x_m, mac_m) = mac_reference(report);
    let summarize = |label: &'static str, props: MassProperties| MassStateSummary {
        label,
        mass_kg: props.mass_kg,
        cg_m: props.cg_m,
        cg_pct_mac: statement.cg_pct_mac(&props, mac_le_x_m, mac_m),
        inertia_cg: props.inertia_cg,
    };
    let maximum_fuel_kg =
        capacity_kg.min((config.requirements.mtow_kg - fuel_loading.zero_fuel_mass_kg).max(0.0));
    let maximum_fuel_state = fuel_items(maximum_fuel_kg)
        .ok()
        .map(|items| statement.with_fuel_items(&items));
    let takeoff = statement.state(LoadState::Takeoff);
    let mut states = vec![
        summarize(
            "operating empty",
            statement.state(LoadState::OperatingEmpty),
        ),
        summarize("zero fuel", statement.state(LoadState::ZeroFuel)),
        summarize("flown takeoff", takeoff),
        summarize("flown landing", statement.state(LoadState::Landing)),
    ];
    if let Some(props) = maximum_fuel_state {
        states.push(summarize("maximum fuel takeoff", props));
    }

    // The report's own centre of gravity belongs to the takeoff-mass closure
    // load, which can exceed the tanks; the like-for-like comparison is the
    // lumped model re-evaluated at the fuel the ledger actually placed.
    let lumped_takeoff_cg_pct_mac = lumped_coordinates(report).map_or(f64::NAN, |coordinates| {
        let mut lumped = masses;
        lumped.fuel = takeoff_fuel_kg;
        let lumped_cg = calculate_physical_cg(&lumped, &coordinates);
        100.0 * (lumped_cg[0] - mac_le_x_m) / mac_m
    });
    let ledger_takeoff_cg_pct_mac = states[2].cg_pct_mac;
    if lumped_takeoff_cg_pct_mac.is_finite()
        && (ledger_takeoff_cg_pct_mac - lumped_takeoff_cg_pct_mac).abs() > CG_DISAGREEMENT_PCT_MAC
    {
        findings.push(PhysicalFinding {
            code: FindingCode::MassModelDisagreement,
            severity: FindingSeverity::Warning,
            message: format!(
                "at {takeoff_fuel_kg:.0} kg of fuel the item ledger places the takeoff centre of gravity at {ledger_takeoff_cg_pct_mac:.1} percent MAC and the lumped model at {lumped_takeoff_cg_pct_mac:.1}; the tank fill order and the detailed payload sit differently from the lumped fuel and payload points"
            ),
            actual: Some(ledger_takeoff_cg_pct_mac),
            limit: Some(lumped_takeoff_cg_pct_mac),
            unit: "% MAC",
        });
    }

    let span_m = report.airplane.b_ref;
    let fuselage_length_m = report
        .airplane
        .fuselages
        .first()
        .and_then(|fuselage| {
            Some(fuselage.xsecs.last()?.xyz_c[0] - fuselage.xsecs.first()?.xyz_c[0])
        })
        .unwrap_or(f64::NAN);
    Some(MassBalanceAssessment {
        states,
        tanks: tanks
            .tanks()
            .iter()
            .map(|tank| TankSummary {
                id: tank.id.clone(),
                kind: tank.kind.label(),
                usable_capacity_kg: tank.usable_capacity_kg,
                unusable_kg: tank.unusable_kg,
                centroid_m: tank.centroid_m,
                capacity_source: capacity_source_label(tank.capacity_source),
                burn_priority: tank.burn_priority,
            })
            .collect(),
        usable_capacity_kg: capacity_kg,
        unusable_fuel_kg: tanks.unusable_fuel_kg(),
        geometric_calibration_factor: tanks.geometric_calibration_factor,
        fuel_cg_curve: tanks.fuel_cg_curve(FUEL_CG_CURVE_STEPS),
        radii_check: statement.radii_of_gyration_check(
            LoadState::Takeoff,
            span_m,
            fuselage_length_m,
        ),
        ledger_items: statement
            .ledger()
            .items()
            .iter()
            .map(|item| LedgerItemSummary {
                id: item.id.clone(),
                group: item.group.label(),
                mass_kg: item.mass_kg,
                position_m: item.position_m,
            })
            .collect(),
        lumped_takeoff_cg_pct_mac,
    })
}

/// The takeoff mass properties of a report from its ledger, for consumers
/// such as the dynamic-mode figure that need an inertia tensor and no
/// findings. The fuel is the largest load the tanks and the takeoff-mass
/// limit admit.
pub fn takeoff_mass_properties(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Option<MassProperties> {
    let stations = component_stations_with_gear(
        &report.airplane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )
    .ok()?;
    let (density_kg_m3, published_total_l) = tank_reference(config, &report.design);
    let resolved_tanks = FuelTankLayout::resolve(
        &report.airplane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    )
    .ok()?;
    let flops_groups = report
        .flops_mass_buildup
        .as_deref()
        .map(|buildup| &buildup.systems_and_operating_items);
    if config.mass_model.mass_architecture.is_pure_flops() && flops_groups.is_none() {
        return None;
    }
    let tanks = if let Some(flops) = flops_groups {
        resolved_tanks
            .with_unusable_fuel_total(flops.operating_items.unusable_fuel_kg)
            .ok()?
    } else {
        resolved_tanks
    };
    let masses = lumped_masses(report)?;
    let mass_basis_kg = report_mass_basis_kg(config, report);
    let zero_fuel_mass_kg = mass_basis_kg - masses.fuel;
    let fuel_kg = tanks
        .usable_capacity_kg()
        .min((mass_basis_kg - zero_fuel_mass_kg).max(0.0));
    let fuel_items = tanks.distribute(fuel_kg).ok()?.mass_items(&tanks);
    let payload_items = payload_items(report);
    let statement = MassStatement::build_with_methods(
        MassStatementInputs {
            masses: &masses,
            stations: &stations,
            payload_items: &payload_items,
            takeoff_fuel_items: fuel_items,
            landing_fuel_items: Vec::new(),
            unusable_fuel_items: tanks.unusable_items(),
            flops: flops_groups,
        },
        LedgerMethods::from_mass_model(&config.mass_model),
    )
    .ok()?;
    Some(statement.state(LoadState::Takeoff))
}

/// Density and published total volume for the tank resolution: the
/// registered aircraft's own when the design is the unchanged preset, the
/// configured density and no published total otherwise.
///
/// The rule itself lives in [`alas_mass::product_stations`] so the
/// optimizer's search-time fuel placement resolves the same tanks this
/// report does.
pub(crate) use alas_mass::product_stations::tank_reference;

fn lumped_masses(report: &AnalysisReport) -> Option<MassBreakdown> {
    let mass = |name: &str| report.component_masses.get(name).copied();
    Some(MassBreakdown {
        wing: mass(WING)?,
        h_stab: mass(H_STAB)?,
        v_stab: mass(V_STAB)?,
        fuselage: mass(FUSELAGE)?,
        gear: mass(GEAR)?,
        propulsion: mass(PROPULSION)?,
        systems: mass(SYSTEMS)?,
        furnishings: mass(FURNISHINGS)?,
        payload: mass(PAYLOAD)?,
        fuel: mass(FUEL)?,
    })
}

fn lumped_coordinates(report: &AnalysisReport) -> Option<MassCoordinates> {
    let coordinate = |name: &str| report.mass_coordinates.get(name).copied();
    Some(MassCoordinates {
        wing: coordinate(WING)?,
        h_stab: coordinate(H_STAB)?,
        v_stab: coordinate(V_STAB)?,
        fuselage: coordinate(FUSELAGE)?,
        gear: coordinate(GEAR)?,
        propulsion: coordinate(PROPULSION)?,
        systems: coordinate(SYSTEMS)?,
        furnishings: coordinate(FURNISHINGS)?,
        payload: coordinate(PAYLOAD)?,
        fuel: coordinate(FUEL)?,
    })
}

fn payload_items(report: &AnalysisReport) -> Vec<PayloadItemSummary> {
    report
        .payload_layout
        .as_ref()
        .map(|layout| {
            layout
                .items
                .iter()
                .filter(|item| item.mass > 0.0)
                .enumerate()
                .map(|(index, item)| PayloadItemSummary {
                    label: format!("{}_{index}", item.kind.as_str()),
                    mass_kg: item.mass,
                    position_m: [item.x, item.y, item.z],
                    extent_m: [item.length, item.width, item.height],
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Leading edge of the model MAC and the MAC itself, the frame every
/// percent-MAC figure in the report uses.
fn mac_reference(report: &AnalysisReport) -> (f64, f64) {
    let mac_m = report.airplane.c_ref.max(1.0e-3);
    let mac_le_x_m = report
        .airplane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| report.airplane.wings.first())
        .map(|wing| wing.aerodynamic_center(0.25)[0] - 0.25 * mac_m)
        .unwrap_or(0.0);
    (mac_le_x_m, mac_m)
}

fn capacity_source_label(source: alas_mass::tanks::CapacitySource) -> &'static str {
    match source {
        alas_mass::tanks::CapacitySource::Published => "published",
        alas_mass::tanks::CapacitySource::GeometricCalibrated => "geometric, calibrated",
        alas_mass::tanks::CapacitySource::Geometric => "geometric",
        alas_mass::tanks::CapacitySource::Declared => "declared",
    }
}

// A test asserts on the default aircraft it built, so a failed expect there
// is the assertion failing, not a library panic.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::plan_fuel_loading;
    use crate::full_analysis::FullAnalysis;

    #[test]
    fn the_default_aircraft_yields_ordered_states_and_a_physical_tensor() {
        let config = AlasConfig::default();
        let design = DesignVector::default();
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("default analysis");
        let fuel_loading = plan_fuel_loading(&config, &design, &report);
        let mut findings = Vec::new();
        let assessment =
            assess_mass_balance(&config, &design, &report, &fuel_loading, &mut findings)
                .expect("the default aircraft has a mass statement");

        assert!(assessment.usable_capacity_kg > 0.0);
        assert!(assessment.tanks.len() >= 3, "{:?}", assessment.tanks);
        let mass = |label: &str| {
            assessment
                .states
                .iter()
                .find(|state| state.label == label)
                .map(|state| state.mass_kg)
                .expect(label)
        };
        assert!(mass("operating empty") < mass("zero fuel"));
        assert!(mass("zero fuel") < mass("flown takeoff"));
        assert!(mass("flown landing") <= mass("flown takeoff"));
        for state in &assessment.states {
            assert!(
                state.inertia_cg.is_physical(),
                "{}: {:?}",
                state.label,
                state.inertia_cg
            );
            assert!(state.cg_pct_mac.is_finite());
        }
        let takeoff = assessment
            .states
            .iter()
            .find(|state| state.label == "flown takeoff")
            .expect("takeoff state");
        // A transport's pitch and yaw radii of gyration lie within a factor
        // of two of Raymer's jet-transport fractions; roll depends on the
        // fuel distribution and is only required to be finite and positive.
        let ratio = assessment.radii_check.ratio;
        assert!(ratio[0] > 0.0, "roll ratio {ratio:?}");
        assert!((0.5..2.0).contains(&ratio[1]), "pitch ratio {ratio:?}");
        assert!((0.5..2.0).contains(&ratio[2]), "yaw ratio {ratio:?}");
        assert!(takeoff.inertia_cg.iyy > takeoff.inertia_cg.ixx);
        let last_curve_point = assessment.fuel_cg_curve.last().expect("curve");
        assert!((last_curve_point.fuel_kg - assessment.usable_capacity_kg).abs() < 1.0e-6);
        assert!(
            (takeoff.cg_pct_mac - assessment.lumped_takeoff_cg_pct_mac).abs() < 15.0,
            "ledger {} vs lumped {} percent MAC",
            takeoff.cg_pct_mac,
            assessment.lumped_takeoff_cg_pct_mac
        );
    }

    /// A refused main-gear station is an error finding, not a warning, and
    /// the low-wing fallback case beside it is untouched.
    ///
    /// Before this phase both halves returned `None` with a
    /// `FindingSeverity::Warning`, so an aircraft the mass model refused to
    /// place a main gear on was reported as a run that merely lacked an item
    /// ledger — and `FeasibilityReport::is_feasible`, which fails only on
    /// error findings, could still call it feasible. The lumped model is not
    /// a fallback here: its gear point is the same refused station.
    ///
    /// The two halves share one analysis run. The ATR half substitutes the
    /// registered ATR 72-600's configuration and built geometry, which is all
    /// `assess_mass_balance` reads before it resolves stations and returns.
    #[test]
    fn a_refused_main_gear_station_is_an_error_and_a_low_wing_fallback_is_not() {
        use alas_geom::builder::AircraftBuilder;

        let config = AlasConfig::default();
        let design = DesignVector::default();
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .expect("default analysis");
        let fuel_loading = plan_fuel_loading(&config, &design, &report);

        // The default aircraft is low-wing: its wing-mounted gear fallback
        // stands, the statement is built, and nothing is escalated.
        let mut findings = Vec::new();
        assert!(
            assess_mass_balance(&config, &design, &report, &fuel_loading, &mut findings).is_some(),
            "the low-wing fallback case must still produce a mass statement"
        );
        assert!(
            findings
                .iter()
                .all(|finding| finding.severity != FindingSeverity::Error),
            "{findings:?}"
        );

        // The ATR 72-600 registers no gear-station anchor and is high-wing,
        // so the station model refuses its main gear.
        let mut atr = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("{error}"));
        // Exercise the explicit refusal fixture; the registered ATR now has
        // source gear stations and is evaluated through the normal path.
        atr.landing_gear.reference_station_fuselage_length_m = None;
        atr.landing_gear.reference_nlg_x_fraction = None;
        atr.landing_gear.reference_mlg_x_fractions = None;
        let registered =
            alas_config::presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
        let mut atr_report = report;
        atr_report.airplane = AircraftBuilder::new(Some(atr.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .expect("the ATR builds");

        let mut findings = Vec::new();
        assert!(
            assess_mass_balance(
                &atr,
                &registered.design_vector,
                &atr_report,
                &fuel_loading,
                &mut findings
            )
            .is_none(),
            "no mass statement can be built without a main-gear station"
        );
        let finding = match findings.as_slice() {
            [finding] => finding,
            other => panic!("expected exactly one finding, got {other:?}"),
        };
        assert_eq!(finding.code, FindingCode::MassLedgerUnavailable);
        assert_eq!(
            finding.severity,
            FindingSeverity::Error,
            "a missing main-gear datum must not be downgradable to a warning"
        );
        assert!(
            finding.message.contains("main-gear"),
            "the finding must name the missing datum: {}",
            finding.message
        );
        // The evidence the station model measured is kept, in SI metres:
        // the wing root leading edge above the fuselage crown it would have
        // had to sit at or below for the wing-mounted rule to apply.
        assert_eq!(finding.unit, "m");
        let (root_z_m, crown_z_m) = (
            finding.actual.expect("wing root height"),
            finding.limit.expect("fuselage crown height"),
        );
        assert!(
            root_z_m > crown_z_m,
            "root {root_z_m} m, crown {crown_z_m} m"
        );
    }
}
