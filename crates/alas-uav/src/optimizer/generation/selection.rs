// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Catalogue partitioning and the optimizer's no-inference evidence boundary.

use crate::catalog::{
    BatterySpec, ComponentKind, ComponentRecord, Dimensions, ElectronicsSpec, EscSpec,
    LandingGearSpec, MaterialStockSpec, MotorSpec, PropellerSpec, ReceiverSpec, ServoSpec,
};
use crate::{Catalog, Finding};

use super::{invalid, missing, CandidateSample};
use crate::optimizer::{OptimizationProblem, PropulsionMap};

pub(in crate::optimizer) struct Choices<'a> {
    pub(in crate::optimizer) batteries: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) motors: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) escs: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) propellers: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) servos: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) materials: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) receivers: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) electronics: Vec<&'a ComponentRecord>,
    pub(in crate::optimizer) landing_gear: Vec<&'a ComponentRecord>,
}

impl<'a> Choices<'a> {
    pub(in crate::optimizer) fn from_catalog(catalog: &'a Catalog) -> Self {
        let mut choices = Self {
            batteries: Vec::new(),
            motors: Vec::new(),
            escs: Vec::new(),
            propellers: Vec::new(),
            servos: Vec::new(),
            materials: Vec::new(),
            receivers: Vec::new(),
            electronics: Vec::new(),
            landing_gear: Vec::new(),
        };
        for record in &catalog.records {
            match record.kind {
                ComponentKind::Battery(_) => choices.batteries.push(record),
                ComponentKind::Motor(_) => choices.motors.push(record),
                ComponentKind::Esc(_) => choices.escs.push(record),
                ComponentKind::Propeller(_) => choices.propellers.push(record),
                ComponentKind::Servo(_) => choices.servos.push(record),
                ComponentKind::MaterialStock(_) => choices.materials.push(record),
                ComponentKind::Receiver(_) => choices.receivers.push(record),
                ComponentKind::Electronics(_) => choices.electronics.push(record),
                ComponentKind::LandingGear(_) => choices.landing_gear.push(record),
            }
        }
        choices
    }
}

pub(super) struct Selection<'a> {
    pub(super) battery_record: &'a ComponentRecord,
    pub(super) battery: &'a BatterySpec,
    pub(super) motor_record: &'a ComponentRecord,
    pub(super) motor: &'a MotorSpec,
    pub(super) esc_record: &'a ComponentRecord,
    pub(super) esc: &'a EscSpec,
    pub(super) propeller_record: &'a ComponentRecord,
    pub(super) propeller: &'a PropellerSpec,
    pub(super) servo_record: &'a ComponentRecord,
    pub(super) servo: &'a ServoSpec,
    pub(super) material_record: &'a ComponentRecord,
    pub(super) material: &'a MaterialStockSpec,
    pub(super) receiver_record: &'a ComponentRecord,
    pub(super) receiver: &'a ReceiverSpec,
    pub(super) electronics_record: &'a ComponentRecord,
    pub(super) electronics: &'a ElectronicsSpec,
    pub(super) landing_gear_record: &'a ComponentRecord,
    pub(super) landing_gear: &'a LandingGearSpec,
    pub(super) propulsion_map: &'a PropulsionMap,
}

pub(super) struct RequiredEvidence {
    pub(super) battery_dimensions: Dimensions,
    pub(super) battery_mass_kg: f64,
    pub(super) battery_voltage_v: f64,
    pub(super) motor_mass_kg: f64,
    pub(super) esc_dimensions: Dimensions,
    pub(super) esc_mass_kg: f64,
    pub(super) propeller_mass_kg: f64,
    pub(super) propeller_diameter_m: f64,
    pub(super) servo_mass_kg: f64,
    pub(super) material_dimensions: Dimensions,
    pub(super) material_mass_kg: f64,
    pub(super) material_allowable_stress_pa: f64,
    pub(super) receiver_dimensions: Dimensions,
    pub(super) receiver_mass_kg: f64,
    pub(super) electronics_dimensions: Dimensions,
    pub(super) electronics_mass_kg: f64,
    pub(super) landing_gear_mass_kg: f64,
    pub(super) landing_gear_max_aircraft_mass_kg: f64,
}

pub(super) fn select_and_require<'a>(
    problem: &'a OptimizationProblem<'a>,
    choices: &'a Choices<'a>,
    sample: &CandidateSample,
) -> Result<(Selection<'a>, RequiredEvidence), Vec<Finding>> {
    let selection = select(problem, choices, sample)?;
    let evidence = require_evidence(&selection, problem)?;
    Ok((selection, evidence))
}

fn select<'a>(
    problem: &'a OptimizationProblem<'a>,
    choices: &'a Choices<'a>,
    sample: &CandidateSample,
) -> Result<Selection<'a>, Vec<Finding>> {
    let battery_record = choice(&choices.batteries, sample.battery_index, "battery")?;
    let motor_record = choice(&choices.motors, sample.motor_index, "motor")?;
    let esc_record = choice(&choices.escs, sample.esc_index, "ESC")?;
    let propeller_record = choice(&choices.propellers, sample.propeller_index, "propeller")?;
    let servo_record = choice(&choices.servos, sample.servo_index, "servo")?;
    let material_record = choice(&choices.materials, sample.material_index, "material")?;
    let receiver_record = choice(&choices.receivers, sample.receiver_index, "receiver")?;
    let electronics_record = choice(
        &choices.electronics,
        sample.electronics_index,
        "electronics",
    )?;
    let landing_gear_record = choice(
        &choices.landing_gear,
        sample.landing_gear_index,
        "landing-gear",
    )?;
    let ComponentKind::Battery(battery) = &battery_record.kind else {
        return Err(vec![missing("battery", "typed battery specification")]);
    };
    let ComponentKind::Motor(motor) = &motor_record.kind else {
        return Err(vec![missing("motor", "typed motor specification")]);
    };
    let ComponentKind::Esc(esc) = &esc_record.kind else {
        return Err(vec![missing("ESC", "typed ESC specification")]);
    };
    let ComponentKind::Propeller(propeller) = &propeller_record.kind else {
        return Err(vec![missing("propeller", "typed propeller specification")]);
    };
    let ComponentKind::Servo(servo) = &servo_record.kind else {
        return Err(vec![missing("servo", "typed servo specification")]);
    };
    let ComponentKind::MaterialStock(material) = &material_record.kind else {
        return Err(vec![missing("material", "typed material specification")]);
    };
    let ComponentKind::Receiver(receiver) = &receiver_record.kind else {
        return Err(vec![missing("receiver", "typed receiver specification")]);
    };
    let ComponentKind::Electronics(electronics) = &electronics_record.kind else {
        return Err(vec![missing(
            "electronics",
            "typed electronics specification",
        )]);
    };
    let ComponentKind::LandingGear(landing_gear) = &landing_gear_record.kind else {
        return Err(vec![missing(
            "landing gear",
            "typed landing-gear specification",
        )]);
    };
    if electronics.role != problem.required_electronics_role {
        return Err(vec![invalid(
            &electronics_record.id,
            0.0,
            &format!(
                "electronics role '{}' does not satisfy required role '{}'",
                electronics.role, problem.required_electronics_role
            ),
        )]);
    }
    let Some(series_cells) = battery.series_cells else {
        return Err(vec![missing(
            &battery_record.id,
            "battery series-cell count",
        )]);
    };
    let Some(propulsion_map) = problem.propulsion_maps.iter().find(|map| {
        map.motor_id == motor_record.id
            && map.propeller_id == propeller_record.id
            && map.series_cells == series_cells
    }) else {
        return Err(vec![missing(
            "propulsion map",
            "thrust-at-speed data for the selected motor, propeller, and battery cell count",
        )]);
    };
    Ok(Selection {
        battery_record,
        battery,
        motor_record,
        motor,
        esc_record,
        esc,
        propeller_record,
        propeller,
        servo_record,
        servo,
        material_record,
        material,
        receiver_record,
        receiver,
        electronics_record,
        electronics,
        landing_gear_record,
        landing_gear,
        propulsion_map,
    })
}

fn choice<'a>(
    choices: &[&'a ComponentRecord],
    index: usize,
    family: &str,
) -> Result<&'a ComponentRecord, Vec<Finding>> {
    choices.get(index).copied().ok_or_else(|| {
        vec![missing(
            "catalogue",
            &format!("at least one {family} record"),
        )]
    })
}

fn require_evidence(
    selection: &Selection<'_>,
    problem: &OptimizationProblem<'_>,
) -> Result<RequiredEvidence, Vec<Finding>> {
    let mut findings = Vec::new();
    let battery_dimensions = required(
        selection.battery.dimensions,
        selection.battery_record,
        "dimensions",
        &mut findings,
    );
    let battery_mass_kg = required(
        selection.battery.mass_kg,
        selection.battery_record,
        "mass",
        &mut findings,
    );
    let battery_voltage_v = required(
        selection.battery.nominal_voltage_v,
        selection.battery_record,
        "nominal voltage",
        &mut findings,
    );
    let motor_mass_kg = required(
        selection.motor.mass_kg,
        selection.motor_record,
        "mass",
        &mut findings,
    );
    let esc_dimensions = required(
        selection.esc.dimensions,
        selection.esc_record,
        "dimensions",
        &mut findings,
    );
    let esc_mass_kg = required(
        selection.esc.mass_kg,
        selection.esc_record,
        "mass",
        &mut findings,
    );
    let propeller_mass_kg = required(
        selection.propeller.mass_kg,
        selection.propeller_record,
        "mass",
        &mut findings,
    );
    let propeller_diameter_m = required(
        selection.propeller.diameter_m,
        selection.propeller_record,
        "diameter",
        &mut findings,
    );
    let servo_dimensions = required(
        selection.servo.dimensions,
        selection.servo_record,
        "dimensions",
        &mut findings,
    );
    let servo_mass_kg = required(
        selection.servo.mass_kg,
        selection.servo_record,
        "mass",
        &mut findings,
    );
    let material_dimensions = required(
        selection.material.dimensions,
        selection.material_record,
        "stock dimensions",
        &mut findings,
    );
    let material_mass_kg = required(
        selection.material.mass_kg,
        selection.material_record,
        "stock mass",
        &mut findings,
    );
    let youngs_modulus_pa = required(
        selection.material.youngs_modulus_pa,
        selection.material_record,
        "Young's modulus",
        &mut findings,
    );
    let material_allowable_stress_pa = required(
        selection.material.allowable_stress_pa,
        selection.material_record,
        "allowable stress",
        &mut findings,
    );
    let receiver_dimensions = required(
        selection.receiver.dimensions,
        selection.receiver_record,
        "dimensions",
        &mut findings,
    );
    let receiver_mass_kg = required(
        selection.receiver.mass_kg,
        selection.receiver_record,
        "mass",
        &mut findings,
    );
    let electronics_dimensions = required(
        selection.electronics.dimensions,
        selection.electronics_record,
        "dimensions",
        &mut findings,
    );
    let electronics_mass_kg = required(
        selection.electronics.mass_kg,
        selection.electronics_record,
        "mass",
        &mut findings,
    );
    let landing_gear_mass_kg = required(
        selection.landing_gear.mass_kg,
        selection.landing_gear_record,
        "mass",
        &mut findings,
    );
    let landing_gear_dimensions = required(
        selection.landing_gear.dimensions,
        selection.landing_gear_record,
        "dimensions",
        &mut findings,
    );
    let landing_gear_max_aircraft_mass_kg = required(
        selection.landing_gear.max_aircraft_mass_kg,
        selection.landing_gear_record,
        "maximum supported aircraft mass",
        &mut findings,
    );
    for speed in [
        problem.objectives.maximum_stall_speed_m_s,
        problem.objectives.cruise_speed_m_s,
    ] {
        if selection.propulsion_map.at_speed(speed).is_none() {
            findings.push(missing(
                &selection.propulsion_map.evidence,
                &format!("propulsion-map coverage at {speed:.3} m/s"),
            ));
        }
    }
    if !findings.is_empty() {
        return Err(findings);
    }
    let (
        Some(battery_dimensions),
        Some(battery_mass_kg),
        Some(battery_voltage_v),
        Some(motor_mass_kg),
        Some(esc_dimensions),
        Some(esc_mass_kg),
        Some(propeller_mass_kg),
        Some(propeller_diameter_m),
        Some(_servo_dimensions),
        Some(servo_mass_kg),
        Some(material_dimensions),
        Some(material_mass_kg),
        Some(_youngs_modulus_pa),
        Some(material_allowable_stress_pa),
        Some(receiver_dimensions),
        Some(receiver_mass_kg),
        Some(electronics_dimensions),
        Some(electronics_mass_kg),
        Some(landing_gear_mass_kg),
        Some(_landing_gear_dimensions),
        Some(landing_gear_max_aircraft_mass_kg),
    ) = (
        battery_dimensions,
        battery_mass_kg,
        battery_voltage_v,
        motor_mass_kg,
        esc_dimensions,
        esc_mass_kg,
        propeller_mass_kg,
        propeller_diameter_m,
        servo_dimensions,
        servo_mass_kg,
        material_dimensions,
        material_mass_kg,
        youngs_modulus_pa,
        material_allowable_stress_pa,
        receiver_dimensions,
        receiver_mass_kg,
        electronics_dimensions,
        electronics_mass_kg,
        landing_gear_mass_kg,
        landing_gear_dimensions,
        landing_gear_max_aircraft_mass_kg,
    )
    else {
        return Err(vec![missing(
            "optimizer evidence",
            "internally consistent required values",
        )]);
    };
    Ok(RequiredEvidence {
        battery_dimensions,
        battery_mass_kg,
        battery_voltage_v,
        motor_mass_kg,
        esc_dimensions,
        esc_mass_kg,
        propeller_mass_kg,
        propeller_diameter_m,
        servo_mass_kg,
        material_dimensions,
        material_mass_kg,
        material_allowable_stress_pa,
        receiver_dimensions,
        receiver_mass_kg,
        electronics_dimensions,
        electronics_mass_kg,
        landing_gear_mass_kg,
        landing_gear_max_aircraft_mass_kg,
    })
}

fn required<T: Copy>(
    value: Option<T>,
    record: &ComponentRecord,
    field: &str,
    findings: &mut Vec<Finding>,
) -> Option<T> {
    if value.is_none() {
        findings.push(missing(&record.id, field));
    }
    value
}
