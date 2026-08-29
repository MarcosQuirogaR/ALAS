// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed-component packaging, mass, and centre-of-gravity bookkeeping.

use crate::catalog::Dimensions;
use crate::feasibility::{failure, invalid, missing, positive};
use crate::model::{EquipmentBay, Finding, FindingKind, Placement, UavDesign};

#[derive(Debug, Clone, Copy)]
pub(crate) struct MassTerm {
    pub(crate) mass_kg: f64,
    pub(crate) x_m: f64,
}

pub(crate) fn check_packaging(design: &UavDesign, findings: &mut Vec<Finding>) {
    check_envelope(
        &design.battery.id,
        design.battery.spec.dimensions,
        design.battery.placement,
        design.airframe.equipment_bay,
        findings,
    );
    for (motor, esc, _) in design.propulsors() {
        check_envelope(
            &motor.id,
            motor.spec.dimensions,
            motor.placement,
            design.airframe.equipment_bay,
            findings,
        );
        check_envelope(
            &esc.id,
            esc.spec.dimensions,
            esc.placement,
            design.airframe.equipment_bay,
            findings,
        );
    }
    for servo in &design.servos {
        check_envelope(
            &servo.id,
            servo.spec.dimensions,
            servo.placement,
            design.airframe.equipment_bay,
            findings,
        );
    }
    for item in &design.other_items {
        check_envelope(
            &item.id,
            item.dimensions,
            item.placement,
            design.airframe.equipment_bay,
            findings,
        );
    }
}

fn check_envelope(
    id: &str,
    dimensions: Option<Dimensions>,
    placement: Placement,
    bay: EquipmentBay,
    findings: &mut Vec<Finding>,
) {
    if !placement.inside_equipment_bay {
        return;
    }
    if !placement.center_x_m.is_finite()
        || !placement.center_y_m.is_finite()
        || !placement.center_z_m.is_finite()
    {
        findings.push(invalid(id, placement.center_x_m));
        return;
    }
    let Some(dimensions) = dimensions else {
        missing(id, "dimensions required for packaging", findings);
        return;
    };
    if !positive(dimensions.length_m)
        || !positive(dimensions.width_m)
        || !positive(dimensions.height_m)
    {
        findings.push(invalid(&format!("{id} dimensions"), dimensions.length_m));
        return;
    }
    let half_x = dimensions.length_m / 2.0;
    let half_y = dimensions.width_m / 2.0;
    let half_z = dimensions.height_m / 2.0;
    if placement.center_x_m - half_x < bay.min_x_m
        || placement.center_x_m + half_x > bay.max_x_m
        || placement.center_y_m - half_y < bay.min_y_m
        || placement.center_y_m + half_y > bay.max_y_m
        || placement.center_z_m - half_z < bay.min_z_m
        || placement.center_z_m + half_z > bay.max_z_m
    {
        findings.push(failure(
            FindingKind::PackagingViolation,
            id,
            "axis-aligned component envelope crosses its equipment-bay boundary".to_owned(),
            None,
            None,
            None,
        ));
    }
}

pub(crate) fn collect_mass_terms(
    design: &UavDesign,
    findings: &mut Vec<Finding>,
) -> (Vec<MassTerm>, bool) {
    let mut terms = vec![MassTerm {
        mass_kg: design.airframe.fixed_mass_kg,
        x_m: design.airframe.fixed_cg_x_m,
    }];
    let mut all_known =
        positive(design.airframe.fixed_mass_kg) && design.airframe.fixed_cg_x_m.is_finite();
    all_known &= push_mass(
        &design.battery.id,
        design.battery.spec.mass_kg,
        design.battery.placement,
        &mut terms,
        findings,
    );
    for (motor, esc, propeller) in design.propulsors() {
        all_known &= push_mass(
            &motor.id,
            motor.spec.mass_kg,
            motor.placement,
            &mut terms,
            findings,
        );
        all_known &= push_mass(
            &esc.id,
            esc.spec.mass_kg,
            esc.placement,
            &mut terms,
            findings,
        );
        all_known &= push_mass(
            &propeller.id,
            propeller.spec.mass_kg,
            propeller.placement,
            &mut terms,
            findings,
        );
    }
    for servo in &design.servos {
        all_known &= push_mass(
            &servo.id,
            servo.spec.mass_kg,
            servo.placement,
            &mut terms,
            findings,
        );
    }
    for item in &design.other_items {
        all_known &= push_mass(&item.id, item.mass_kg, item.placement, &mut terms, findings);
    }
    (terms, all_known)
}

fn push_mass(
    id: &str,
    mass: Option<f64>,
    placement: Placement,
    terms: &mut Vec<MassTerm>,
    findings: &mut Vec<Finding>,
) -> bool {
    match mass {
        Some(mass) if positive(mass) && placement.center_x_m.is_finite() => {
            terms.push(MassTerm {
                mass_kg: mass,
                x_m: placement.center_x_m,
            });
            true
        }
        Some(mass) => {
            findings.push(invalid(&format!("{id} mass or placement"), mass));
            false
        }
        None => {
            missing(id, "mass required for takeoff mass and CG", findings);
            false
        }
    }
}

pub(crate) fn total_mass(terms: &[MassTerm]) -> Option<f64> {
    let mass: f64 = terms.iter().map(|term| term.mass_kg).sum();
    positive(mass).then_some(mass)
}
