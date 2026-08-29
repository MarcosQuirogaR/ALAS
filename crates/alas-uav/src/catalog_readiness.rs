// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence gates for components that may enter an automatic UAV analysis.
//!
//! The source archive deliberately preserves partial records: a manufacturer
//! page that omits a mass or a retailer price is still useful provenance.  It
//! is not, however, selectable hardware for an automatic design calculation.
//! This module keeps that distinction executable so a GUI cannot present a
//! partial record as a design-ready choice.

use crate::catalog::{Catalog, ComponentKind, ComponentRecord};
use crate::procurement::has_reviewed_quote;
use crate::propulsion_electric::apc_performance_map;

/// Names of source-backed fields absent from one analysis record.
///
/// These are stable engineering field names, not translated UI prose.  A
/// caller can decide whether to display them directly or localize them.
pub fn analysis_evidence_gaps(
    record: &ComponentRecord,
    control_bus_voltage_v: f64,
) -> Vec<&'static str> {
    match &record.kind {
        ComponentKind::Battery(spec) => gaps([
            (spec.series_cells.is_some(), "series-cell count"),
            (spec.nominal_voltage_v.is_some(), "nominal voltage"),
            (spec.capacity_ah.is_some(), "capacity"),
            (spec.discharge_rating_c.is_some(), "discharge rating"),
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
            (spec.connector.is_some(), "connector"),
        ]),
        ComponentKind::Motor(spec) => gaps([
            (spec.kv_rpm_per_v.is_some(), "Kv"),
            (spec.winding_resistance_ohm.is_some(), "winding resistance"),
            (spec.no_load_current_a.is_some(), "no-load current"),
            (
                spec.no_load_test_voltage_v.is_some(),
                "no-load test voltage",
            ),
            (spec.min_series_cells.is_some(), "minimum cell count"),
            (spec.max_series_cells.is_some(), "maximum cell count"),
            (spec.max_current_a.is_some(), "maximum current"),
            (spec.max_power_w.is_some(), "maximum power"),
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
        ]),
        ComponentKind::Esc(spec) => gaps([
            (spec.min_series_cells.is_some(), "minimum cell count"),
            (spec.max_series_cells.is_some(), "maximum cell count"),
            (spec.continuous_current_a.is_some(), "continuous current"),
            (spec.bec_min_voltage_v.is_some(), "BEC minimum voltage"),
            (spec.bec_max_voltage_v.is_some(), "BEC maximum voltage"),
            (
                spec.bec_continuous_current_a.is_some(),
                "BEC continuous current",
            ),
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
        ]),
        ComponentKind::Servo(spec) => gaps([
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
            (
                spec.stall_torque_at_voltage(control_bus_voltage_v)
                    .is_some(),
                "stall torque at control-bus voltage",
            ),
            (
                spec.stall_current_at_voltage(control_bus_voltage_v)
                    .is_some(),
                "stall current at control-bus voltage",
            ),
        ]),
        ComponentKind::Propeller(spec) => gaps([
            (spec.diameter_m.is_some(), "diameter"),
            (spec.mass_kg.is_some(), "mass"),
            (
                apc_performance_map(&record.id).is_ok(),
                "bounded performance table",
            ),
        ]),
        ComponentKind::MaterialStock(spec) => gaps([
            (spec.dimensions.is_some(), "stock dimensions"),
            (spec.mass_kg.is_some(), "stock mass"),
            (spec.youngs_modulus_pa.is_some(), "Young's modulus"),
            (spec.allowable_stress_pa.is_some(), "allowable stress"),
        ]),
        ComponentKind::Receiver(spec) => gaps([
            (spec.channel_count.is_some(), "channel count"),
            (spec.min_voltage_v.is_some(), "minimum voltage"),
            (spec.max_voltage_v.is_some(), "maximum voltage"),
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
        ]),
        ComponentKind::Electronics(spec) => gaps([
            (!spec.role.trim().is_empty(), "role"),
            (spec.min_voltage_v.is_some(), "minimum voltage"),
            (spec.max_voltage_v.is_some(), "maximum voltage"),
            (spec.max_current_a.is_some(), "maximum current"),
            (spec.mass_kg.is_some(), "mass"),
            (spec.dimensions.is_some(), "dimensions"),
        ]),
        ComponentKind::LandingGear(spec) => gaps([
            (!spec.form.trim().is_empty(), "form"),
            (spec.mass_kg.is_some(), "mass"),
            (spec.max_aircraft_mass_kg.is_some(), "maximum aircraft mass"),
            (spec.dimensions.is_some(), "dimensions"),
        ]),
    }
}

/// Whether a component independently carries every field its current analysis
/// family needs.  Compatible multi-component combinations remain checked by
/// the solver and feasibility report.
pub fn is_analysis_ready(record: &ComponentRecord, control_bus_voltage_v: f64) -> bool {
    analysis_evidence_gaps(record, control_bus_voltage_v).is_empty()
}

/// Copy only records that may enter an automatic design run.
///
/// The source archive may retain incomplete historic data, but the application
/// catalogue is stricter: every engineering input must be published and every
/// component must have a dated source price.
pub fn selectable_catalog(source: &Catalog, control_bus_voltage_v: f64) -> Catalog {
    Catalog {
        schema_version: source.schema_version,
        records: source
            .records
            .iter()
            .filter(|record| {
                is_analysis_ready(record, control_bus_voltage_v) && has_reviewed_quote(&record.id)
            })
            .cloned()
            .collect(),
    }
}

fn gaps<const N: usize>(fields: [(bool, &'static str); N]) -> Vec<&'static str> {
    fields
        .into_iter()
        .filter_map(|(present, field)| (!present).then_some(field))
        .collect()
}
