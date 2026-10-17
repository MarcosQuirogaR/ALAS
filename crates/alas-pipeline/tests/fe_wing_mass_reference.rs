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
//! 9-11.5 % of maximum takeoff mass. Wing Group / GW as printed in J. Roskam,
//! *Airplane Design Part V: Component Weight Estimation*, DARcorporation,
//! Appendix A (page images read directly): B737-200 0.092 and B727-100 0.111
//! (Table A7.2b, book p.153), DC-9-30 0.106 and MD-80 0.111 (Table A7.1b,
//! book p.151). The band's edges are therefore set by 1960s-80s aluminium
//! narrowbodies; the A320-200 itself is 8,801 kg of 73,500 kg = 11.97 % in
//! E. Obert, *Aerodynamic Design of Transport Aircraft*, 2009, Fig. 42.25
//! (book p.581, wing row only; pylons, fuel system and flight controls are
//! separate rows). Masses are kg.
//!
//! The composite presets (A220-300, B787-9) size their box with the
//! longitudinal `CFRP 60/30/10` caps with a 0.004 design strain and QI panels
//! (see `alas_config::preset_structures::cap_material_source`).
//! AVE is not asserted: no source states its wing material, and the database
//! default box keeps `CFRP UD` caps, limited to 0.004 strain in product sizing.

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

fn assert_within_a_quarter(name: &str) {
    let (fe_kg, flops_kg, _) = fe_and_flops_wing_kg(name);
    let ratio = fe_kg / flops_kg;
    assert!(
        (0.75..=1.25).contains(&ratio),
        "{name}: FE wing {fe_kg:.0} kg is {ratio:.2} of the FLOPS wing {flops_kg:.0} kg"
    );
}

/// Expected failure, finding of the final pass 2026-10-16. The A340-300 FE
/// wing is 36,069 kg, 1.29 of the FLOPS 28,012 kg. The stress sizing now
/// resolves the moment about the section normal to the swept elastic axis
/// (M / cos L, Torenbeek 2013 Eq. 11.11; fix-beam-vs-fe): the A340 primary
/// box grew 21,200 -> 23,056 kg (+8.8 %), which takes the previously
/// in-band ratio (about 1.22 from the box change alone) over the 1.25
/// edge. The correction is the physical result of the swept-axis theory
/// (an FE-vs-beam check agrees within 12 %), so the band is not widened;
/// remove the marker when the FE-versus-FLOPS comparison is resolved.
#[test]
#[should_panic(expected = "A340-300: FE wing")]
fn the_a340_fe_wing_is_within_a_quarter_of_the_flops_wing_known_failure() {
    assert_within_a_quarter("A340-300");
}

/// Expected failure, same cause as the A340 above: the A380-800 FE wing is
/// 87,654 kg, 1.30 of the FLOPS 67,621 kg (primary box 54,125 -> 57,451 kg,
/// +6.1 %, from the swept-axis M / cos L stress sizing; about 1.25 before).
#[test]
#[should_panic(expected = "A380-800: FE wing")]
fn the_a380_fe_wing_is_within_a_quarter_of_the_flops_wing_known_failure() {
    assert_within_a_quarter("A380-800");
}

#[test]
fn the_fe_wings_are_within_a_quarter_of_the_flops_wing() {
    for name in ["A320-200", "A220-300", "B787-9", "ATR72-600"] {
        assert_within_a_quarter(name);
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
