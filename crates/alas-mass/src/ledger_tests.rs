// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

fn item(id: &str, role: MassRole, mass_kg: f64, position_m: [f64; 3]) -> MassItem {
    MassItem {
        id: id.to_owned(),
        group: MassGroup::Systems,
        role,
        mass_kg,
        position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::Declared,
    }
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
}

#[test]
fn two_point_masses_balance_at_their_mass_weighted_mean_in_every_axis() {
    let a = MassProperties::point(2.0, [1.0, 2.0, 3.0]);
    let b = MassProperties::point(6.0, [5.0, -2.0, 7.0]);
    let combined = MassProperties::combine([&a, &b]);
    assert_eq!(combined.mass_kg, 8.0);
    assert!(close(combined.cg_m[0], 4.0));
    assert!(close(combined.cg_m[1], -1.0));
    assert!(close(combined.cg_m[2], 6.0));
}

#[test]
fn a_point_mass_moved_off_its_centroid_gains_the_parallel_axis_terms() {
    let translated = InertiaTensor::ZERO.translated(10.0, [1.0, 2.0, 3.0]);
    assert!(close(translated.ixx, 10.0 * (4.0 + 9.0)));
    assert!(close(translated.iyy, 10.0 * (1.0 + 9.0)));
    assert!(close(translated.izz, 10.0 * (1.0 + 4.0)));
    assert!(close(translated.pxy, 20.0));
    assert!(close(translated.pxz, 30.0));
    assert!(close(translated.pyz, 60.0));
    let matrix = translated.matrix();
    assert_eq!(matrix[0][1], matrix[1][0]);
    assert_eq!(matrix[0][1], -20.0);
}

#[test]
fn two_equal_masses_on_the_x_axis_have_the_dumbbell_inertia_about_their_centre() {
    let a = MassProperties::point(5.0, [-2.0, 0.0, 0.0]);
    let b = MassProperties::point(5.0, [2.0, 0.0, 0.0]);
    let combined = MassProperties::combine([&a, &b]);
    assert!(close(combined.inertia_cg.ixx, 0.0));
    assert!(close(combined.inertia_cg.iyy, 40.0));
    assert!(close(combined.inertia_cg.izz, 40.0));
    assert!(combined.inertia_cg.is_physical());
    assert!(close(combined.radii_of_gyration()[1], 2.0));
}

#[test]
fn a_tensor_about_the_centre_translates_back_consistently_about_any_reference() {
    let a = MassProperties::point(3.0, [1.0, 0.0, 0.0]);
    let b = MassProperties::point(1.0, [-3.0, 0.0, 0.0]);
    let combined = MassProperties::combine([&a, &b]);
    let about_origin = combined.inertia_about([0.0, 0.0, 0.0]);
    let direct = 3.0 * 1.0 + 1.0 * 9.0;
    assert!(close(about_origin.iyy, direct));
}

#[test]
fn moving_one_item_changes_only_its_own_first_moment() {
    let mut ledger = MassLedger::new();
    ledger.push(item("a", MassRole::Fixed, 10.0, [1.0, 0.0, 0.0]));
    ledger.push(item("b", MassRole::Fixed, 10.0, [3.0, 0.0, 0.0]));
    let before = ledger.operating_empty();
    ledger.items_mut()[1].position_m = [5.0, 0.0, 0.0];
    let after = ledger.operating_empty();
    assert!(close(before.cg_m[0], 2.0));
    assert!(close(after.cg_m[0], 3.0));
    assert_eq!(before.mass_kg, after.mass_kg);
}

#[test]
fn roles_decide_which_items_the_operating_empty_state_counts() {
    let mut ledger = MassLedger::new();
    ledger.push(item("wing", MassRole::Fixed, 100.0, [10.0, 0.0, 0.0]));
    ledger.push(item("crew", MassRole::OperatingItem, 5.0, [2.0, 0.0, 0.0]));
    ledger.push(item(
        "trapped",
        MassRole::UnusableFuel,
        1.0,
        [10.0, 0.0, 0.0],
    ));
    ledger.push(item("pax", MassRole::Payload, 50.0, [12.0, 0.0, 0.0]));
    ledger.push(item("tank", MassRole::UsableFuel, 40.0, [11.0, 0.0, 0.0]));
    assert_eq!(ledger.operating_empty().mass_kg, 106.0);
    assert_eq!(
        ledger
            .properties_of(&[MassRole::Payload, MassRole::UsableFuel])
            .mass_kg,
        90.0
    );
    assert_eq!(
        ledger.mass_where(|item| item.role == MassRole::UsableFuel),
        40.0
    );
    assert_eq!(ledger.group_totals(), vec![(MassGroup::Systems, 196.0)]);
}

#[test]
fn a_negative_mass_is_refused_rather_than_clamped() {
    let mut ledger = MassLedger::new();
    ledger.push(item("bad", MassRole::Fixed, -1.0, [0.0; 3]));
    assert!(matches!(
        ledger.validate(),
        Err(LedgerError::InvalidMass { .. })
    ));
}

#[test]
fn a_non_finite_position_and_a_duplicate_id_are_refused() {
    let mut ledger = MassLedger::new();
    ledger.push(item("a", MassRole::Fixed, 1.0, [f64::NAN, 0.0, 0.0]));
    assert!(matches!(
        ledger.validate(),
        Err(LedgerError::InvalidPosition { .. })
    ));
    let mut duplicated = MassLedger::new();
    duplicated.push(item("a", MassRole::Fixed, 1.0, [0.0; 3]));
    duplicated.push(item("a", MassRole::Fixed, 1.0, [0.0; 3]));
    assert!(matches!(
        duplicated.validate(),
        Err(LedgerError::DuplicateId { .. })
    ));
}

#[test]
fn a_tensor_violating_the_triangle_inequality_is_not_physical() {
    let impossible = InertiaTensor::diagonal(1.0, 1.0, 5.0);
    assert!(!impossible.is_physical());
    let mut ledger = MassLedger::new();
    let mut bad = item("a", MassRole::Fixed, 1.0, [0.0; 3]);
    bad.local_inertia = impossible;
    ledger.push(bad);
    assert!(matches!(
        ledger.validate(),
        Err(LedgerError::InvalidInertia { .. })
    ));
    assert!(InertiaTensor::diagonal(2.0, 3.0, 4.0).is_physical());
}

#[test]
fn a_massless_set_has_empty_properties_rather_than_a_nan_centre() {
    let combined = MassProperties::combine(std::iter::empty());
    assert_eq!(combined, MassProperties::EMPTY);
    assert_eq!(MassProperties::EMPTY.radii_of_gyration(), [0.0; 3]);
}
