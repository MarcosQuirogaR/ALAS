// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The finite-element wing and the FLOPS wing describe one wing group.
//!
//! The FE wing is the strength- and flight-shape-accepted primary box the
//! structural stage meshes (both semi-wings), plus the enumerated non-box
//! inventory (leading and trailing edge, movables, fittings). It is compared
//! with the FLOPS transport wing (NASA/TM-2017-219627 Vol. I, eqs. 33-45) on
//! the same planform and design gross mass, and on the A320-200 with the
//! published class statement that a narrowbody transport wing group is
//! 9-11 % of maximum takeoff mass (B737-200 9.2 %, B727-200 10.4 %,
//! DC-9-30 10.5 %; J. Roskam, *Airplane Design Part V: Component Weight
//! Estimation*, DARcorporation). Masses are kg.
//!
//! The composite presets (A220-300, B787-9) size their box with the
//! damage-tolerant `CFRP QI` design allowable (see `alas_config::materials`).
//! AVE is not asserted: no source states its wing material, and the database
//! default box it keeps carries `CFRP UD` caps at a pristine tension value.

// A test asserts on values it constructed, so a failed unwrap is the assertion
// failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;

/// `(FE complete wing, FLOPS wing, MTOW)` of a registered aircraft, kg.
fn fe_and_flops_wing_kg(name: &str) -> (f64, f64, f64) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    let design = presets::get(name).unwrap().design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let assessment =
        alas_opt::mdo::structural_feasibility::assess_candidate(&config, &design, &plane).unwrap();
    assert!(assessment.passes(), "{name}: {assessment:#?}");
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg =
        alas_opt::mdo::structural_feasibility::structural_design_mass_kg(&config);
    let (primary, _) =
        alas_mass::wing_reconciliation::sized_primary_wing(&config, &design, &plane, &requirements)
            .unwrap();
    let wing = alas_mass::wing_reconciliation::main_wing(&plane).unwrap();
    let inventory =
        alas_mass::wing_reconciliation::clean_sheet_secondary(&config, wing, primary).unwrap();
    (
        assessment.primary_mass_kg + inventory.total_kg(),
        inventory.diagnostics.flops_group_total_kg,
        config.requirements.mtow_kg,
    )
}

#[test]
fn the_fe_wings_are_within_a_quarter_of_the_flops_wing() {
    for name in [
        "A320-200",
        "A220-300",
        "A340-300",
        "A380-800",
        "B787-9",
        "ATR72-600",
    ] {
        let (fe_kg, flops_kg, _) = fe_and_flops_wing_kg(name);
        let ratio = fe_kg / flops_kg;
        assert!(
            (0.75..=1.25).contains(&ratio),
            "{name}: FE wing {fe_kg:.0} kg is {ratio:.2} of the FLOPS wing {flops_kg:.0} kg"
        );
    }
}

#[test]
fn the_a320_fe_wing_group_is_inside_the_published_narrowbody_band() {
    let (fe_kg, _, mtow_kg) = fe_and_flops_wing_kg("A320-200");
    let fraction = fe_kg / mtow_kg;
    assert!(
        (0.09..=0.115).contains(&fraction),
        "A320 FE wing group {fe_kg:.0} kg is {:.2} % of MTOW",
        100.0 * fraction
    );
}
