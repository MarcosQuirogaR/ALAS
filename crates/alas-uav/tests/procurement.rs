// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Procurement estimates remain separate from component physics evidence.

use alas_uav::propulsion_electric::CataloguePowertrainSelection;
use alas_uav::{
    estimate_optimized_aircraft_cost, estimate_powertrain_procurement_cost,
    estimate_procurement_cost, optimized_aircraft_bom, BomCategory, Currency, ProcurementSelection,
};

#[test]
fn dated_quotes_are_summed_per_currency_and_never_hide_missing_components() {
    let estimate = estimate_procurement_cost(&[
        ProcurementSelection {
            component_id: "tmotor-at2814-900kv".to_owned(),
            quantity: 2,
        },
        ProcurementSelection {
            component_id: "unmannedtech-gensace-gtech-2200-3s-45c-xt60".to_owned(),
            quantity: 1,
        },
        ProcurementSelection {
            component_id: "apc-12x6e".to_owned(),
            quantity: 2,
        },
    ]);

    assert!(estimate.complete);
    assert_eq!(estimate.subtotals.len(), 2);
    assert_eq!(estimate.subtotals[0].currency, Currency::Gbp);
    assert_eq!(estimate.subtotals[0].amount_minor, 2_474);
    assert_eq!(estimate.subtotals[1].currency, Currency::Usd);
    assert_eq!(estimate.subtotals[1].amount_minor, 10_972);
    assert!(estimate.lines.iter().any(|line| {
        line.component_id == "apc-12x6e" && line.quote.is_some() && line.quantity == 2
    }));
}

#[test]
fn a_zero_quantity_is_not_misrepresented_as_a_complete_estimate() {
    let estimate = estimate_procurement_cost(&[ProcurementSelection {
        component_id: "tmotor-at2814-900kv".to_owned(),
        quantity: 0,
    }]);

    assert!(!estimate.complete);
    assert!(estimate.subtotals.is_empty());
}

#[test]
fn a_powertrain_selection_prices_each_identical_propulsor_without_reentry() {
    let mut selection = CataloguePowertrainSelection::single_motor(
        "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
        "tmotor-at2814-900kv",
        "hobbywing-skywalker-30a-v2-mini",
        "apc-12x6e",
    );
    selection.motor_count = 2;
    let estimate = estimate_powertrain_procurement_cost(&selection);

    assert!(estimate.complete);
    assert_eq!(estimate.subtotals[0].currency, Currency::Gbp);
    assert_eq!(estimate.subtotals[0].amount_minor, 2_474);
    assert_eq!(estimate.subtotals[1].currency, Currency::Usd);
    assert_eq!(estimate.subtotals[1].amount_minor, 14_970);
    assert!(estimate.lines.iter().all(|line| {
        (line.component_id == "unmannedtech-gensace-gtech-2200-3s-45c-xt60" && line.quantity == 1)
            || (line.component_id != "unmannedtech-gensace-gtech-2200-3s-45c-xt60"
                && line.quantity == 2)
    }));
}

#[test]
fn an_optimizer_selection_becomes_a_complete_visible_bill_of_materials() {
    let components = alas_uav::optimizer::SelectedComponents {
        battery_id: "unmannedtech-gensace-gtech-2200-3s-45c-xt60".to_owned(),
        motor_id: "tmotor-at2814-900kv".to_owned(),
        motor_count: 2,
        esc_id: "hobbywing-skywalker-30a-v2-mini".to_owned(),
        propeller_id: "apc-12x6e".to_owned(),
        servo_id: "servo".to_owned(),
        servo_count: 3,
        material_id: "material".to_owned(),
        receiver_id: "receiver".to_owned(),
        electronics_id: "electronics".to_owned(),
        landing_gear_id: "landing-gear".to_owned(),
        propulsion_evidence: "native solver".to_owned(),
    };
    let bom = optimized_aircraft_bom(&components);
    let estimate = estimate_optimized_aircraft_cost(&components);

    assert_eq!(estimate.lines.len(), 9);
    assert!(!estimate.complete);
    assert!(bom.lines.iter().any(|line| {
        line.category == BomCategory::Motor
            && line.selection.component_id == "tmotor-at2814-900kv"
            && line.selection.quantity == 2
    }));
    assert!(bom
        .lines
        .iter()
        .any(|line| { line.category == BomCategory::Servo && line.selection.quantity == 3 }));
    assert!(estimate.lines.iter().any(|line| {
        line.component_id == "hobbywing-skywalker-30a-v2-mini"
            && line.quantity == 2
            && line.quote.is_some()
    }));
}
