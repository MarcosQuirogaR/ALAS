// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::airframe::analysis::{estimate_trim_setup, AirframeInputs};
use crate::airframe::geometry::{AirframeGeometry, Panel, Planform, TailRole, TailSurfaceGeom};
use crate::airframe::materials::SparMaterials;
use crate::airframe::spar::SparGeometry;
use crate::spec::topology::TailKind;

#[derive(Clone, Copy)]
pub(in super::super) struct CandidateSample {
    pub(in super::super) battery_index: usize,
    pub(in super::super) motor_index: usize,
    pub(in super::super) esc_index: usize,
    pub(in super::super) propeller_index: usize,
    pub(in super::super) servo_index: usize,
    pub(in super::super) material_index: usize,
    pub(in super::super) receiver_index: usize,
    pub(in super::super) electronics_index: usize,
    pub(in super::super) landing_gear_index: usize,
    pub(in super::super) wing_area_m2: f64,
    pub(in super::super) wing_aspect_ratio: f64,
    pub(in super::super) fuselage_length_m: f64,
    pub(in super::super) wing_leading_edge_fraction: f64,
}

pub(in super::super) struct Candidate {
    pub(in super::super) design: UavDesign,
    pub(in super::super) geometry: GeneratedGeometry,
    pub(in super::super) components: SelectedComponents,
    pub(in super::super) findings: Vec<Finding>,
}

pub(in super::super) fn generate(
    problem: &OptimizationProblem<'_>,
    choices: &Choices<'_>,
    sample: CandidateSample,
) -> Result<Candidate, Vec<Finding>> {
    let (selection, evidence) = select_and_require(problem, choices, &sample)?;
    let model = problem.model;
    let wing = wing_geometry(sample);
    let tail_x_m = 0.9 * sample.fuselage_length_m;
    let tail_arm_m = tail_x_m - (wing.leading_edge_x_m + 0.25 * wing.mean_chord_m);
    if tail_arm_m <= 0.25 * wing.mean_chord_m {
        return Err(vec![invalid(
            "tail arm",
            tail_arm_m,
            "tail quarter chord must remain aft of the wing",
        )]);
    }
    let empennage = empennage_geometry(wing, tail_arm_m, model);
    let equipment = [
        evidence.battery_dimensions,
        evidence.esc_dimensions,
        evidence.receiver_dimensions,
        evidence.electronics_dimensions,
        problem.systems.avionics_dimensions,
        problem.objectives.payload_dimensions,
    ];
    let fuselage = fuselage_geometry(
        sample.fuselage_length_m,
        equipment,
        evidence.material_dimensions.height_m,
        model,
    )?;
    let landing_gear = LandingGearGeometry {
        track_m: model.landing_gear_track_fraction * wing.span_m,
        wheelbase_m: model.landing_gear_wheelbase_fraction * fuselage.length_m,
        minimum_leg_length_m: evidence.propeller_diameter_m / 2.0
            + model.propeller_ground_clearance_m,
        design_load_factor: model.limit_load_factor,
    };
    let geometry = GeneratedGeometry {
        wing,
        fuselage,
        empennage,
        landing_gear,
    };
    let airframe_inputs = airframe_inputs(problem, &geometry);
    let density = evidence.material_mass_kg
        / (evidence.material_dimensions.length_m
            * evidence.material_dimensions.width_m
            * evidence.material_dimensions.height_m);
    let thickness_m = evidence.material_dimensions.height_m;
    let masses = airframe_masses(
        geometry,
        density,
        thickness_m,
        evidence.landing_gear_mass_kg,
        model,
    );
    let fixed_mass_kg = masses.iter().map(|mass| mass.mass_kg).sum::<f64>();
    let fixed_cg_x_m = masses
        .iter()
        .map(|mass| mass.mass_kg * mass.x_m)
        .sum::<f64>()
        / fixed_mass_kg;
    let placements = place_equipment(fuselage.equipment_bay, equipment, model.equipment_gap_m);
    let cruise = operating_point(
        selection.propulsion_map,
        problem.objectives.cruise_speed_m_s,
    );
    let stall = operating_point(
        selection.propulsion_map,
        problem.objectives.maximum_stall_speed_m_s,
    );
    let propulsor_count = f64::from(selection.propulsion_map.motor_count);
    let servo_current_a = selection
        .servo
        .stall_current_at_voltage(problem.systems.control_bus_voltage_v)
        .map(|current| current * problem.systems.servo_continuous_current_fraction);
    let mission = mission_summary(
        problem,
        stall,
        cruise,
        propulsor_count,
        evidence.battery_voltage_v,
    );
    let total_mass_kg = fixed_mass_kg
        + evidence.battery_mass_kg
        + propulsor_count * evidence.motor_mass_kg
        + propulsor_count * evidence.esc_mass_kg
        + propulsor_count * evidence.propeller_mass_kg
        + 3.0 * evidence.servo_mass_kg
        + evidence.receiver_mass_kg
        + evidence.electronics_mass_kg
        + problem.systems.avionics_mass_kg
        + problem.objectives.payload_mass_kg;

    // Integrating an elliptical semispan gives M_root = n W b / (3 pi).
    // The cap allowable is the elementary axial-force couple divided by the
    // explicit structural safety factor.
    let root_moment_nm =
        model.limit_load_factor * total_mass_kg * STANDARD_GRAVITY_M_S2 * wing.span_m / (3.0 * PI);
    let cap_area_m2 = model.spar_cap_width_fraction * wing.mean_chord_m * thickness_m;
    let cap_separation_m = model.spar_cap_separation_fraction * wing.mean_chord_m;
    let root_capacity_nm = evidence.material_allowable_stress_pa * cap_area_m2 * cap_separation_m
        / model.structural_safety_factor;
    let allowable_load_factor = root_capacity_nm / (root_moment_nm / model.limit_load_factor);
    let (aileron_torque_nm, elevator_torque_nm) = control_torques(problem, geometry);
    let external = |x, y| Placement {
        center_x_m: x,
        center_y_m: y,
        center_z_m: 0.0,
        inside_equipment_bay: false,
    };
    let (motor, esc, propeller, additional_propulsors) = propulsor_installations(
        &selection,
        selection.propulsion_map.motor_count,
        fuselage.length_m,
        wing,
        placements[1],
    );
    let design = UavDesign {
        airframe: Airframe {
            fixed_mass_kg,
            fixed_cg_x_m,
            wing_area_m2: wing.area_m2,
            maximum_lift_coefficient: model.maximum_lift_coefficient,
            zero_lift_drag_coefficient: model.zero_lift_drag_coefficient,
            induced_drag_factor: 1.0 / (PI * model.oswald_efficiency * wing.aspect_ratio),
            forward_cg_limit_x_m: wing.leading_edge_x_m
                + model.forward_cg_chord_fraction * wing.mean_chord_m,
            aft_cg_limit_x_m: wing.leading_edge_x_m
                + model.aft_cg_chord_fraction * wing.mean_chord_m,
            equipment_bay: fuselage.equipment_bay,
        },
        battery: InstalledBattery {
            id: selection.battery_record.id.clone(),
            spec: selection.battery.clone(),
            placement: placements[0],
        },
        motor,
        esc,
        propeller,
        additional_propulsors,
        servos: vec![
            installed_servo(
                "left aileron",
                selection.servo,
                external(
                    wing.leading_edge_x_m + 0.7 * wing.mean_chord_m,
                    -0.3 * wing.span_m,
                ),
                aileron_torque_nm,
                servo_current_a,
            ),
            installed_servo(
                "right aileron",
                selection.servo,
                external(
                    wing.leading_edge_x_m + 0.7 * wing.mean_chord_m,
                    0.3 * wing.span_m,
                ),
                aileron_torque_nm,
                servo_current_a,
            ),
            installed_servo(
                "elevator",
                selection.servo,
                external(tail_x_m, 0.0),
                elevator_torque_nm,
                servo_current_a,
            ),
        ],
        other_items: installed_items(problem, &selection, &evidence, placements),
        propulsion_demand: PropulsionElectricalDemand {
            battery_current_a: mission.maximum_battery_current_a,
            motor_current_a: mission.maximum_motor_current_a,
            motor_power_w: mission.maximum_motor_power_w,
        },
        control_bus: ControlBusDemand {
            voltage_v: problem.systems.control_bus_voltage_v,
            other_continuous_current_a: problem.systems.control_bus_current_a,
        },
        mission_energy: MissionEnergyDemand {
            mission_energy_wh: mission.energy_wh,
            maximum_depth_of_discharge: problem.systems.maximum_depth_of_discharge,
            reserve_fraction: problem.systems.reserve_fraction,
        },
        flight_conditions: flight_conditions(problem, stall, cruise, propulsor_count),
        structural_cases: vec![StructuralCase {
            name: "positive maneuver".to_owned(),
            load_factor: model.limit_load_factor,
            allowable_load_factor: Some(allowable_load_factor),
            wing_root_bending_moment_nm: Some(root_moment_nm),
            allowable_wing_root_bending_moment_nm: Some(root_capacity_nm),
        }],
        airframe_inputs,
    };
    let mut findings = Vec::new();
    check_control_equipment(problem, &selection, &mut findings);
    if total_mass_kg > evidence.landing_gear_max_aircraft_mass_kg {
        findings.push(failure(
            FindingKind::LandingGearOverload,
            &selection.landing_gear_record.id,
            "takeoff mass exceeds the published landing-gear rating",
            Some(total_mass_kg),
            Some(evidence.landing_gear_max_aircraft_mass_kg),
            Some("kg"),
        ));
    }
    Ok(Candidate {
        design,
        geometry,
        components: SelectedComponents {
            battery_id: selection.battery_record.id.clone(),
            motor_id: selection.motor_record.id.clone(),
            motor_count: selection.propulsion_map.motor_count,
            esc_id: selection.esc_record.id.clone(),
            propeller_id: selection.propeller_record.id.clone(),
            servo_id: selection.servo_record.id.clone(),
            servo_count: 3,
            material_id: selection.material_record.id.clone(),
            receiver_id: selection.receiver_record.id.clone(),
            electronics_id: selection.electronics_record.id.clone(),
            landing_gear_id: selection.landing_gear_record.id.clone(),
            propulsion_evidence: selection.propulsion_map.evidence.clone(),
        },
        findings,
    })
}

fn installed_items(
    problem: &OptimizationProblem<'_>,
    selection: &Selection<'_>,
    evidence: &selection::RequiredEvidence,
    placements: [Placement; 6],
) -> Vec<InstalledMass> {
    vec![
        InstalledMass {
            id: selection.receiver_record.id.clone(),
            mass_kg: Some(evidence.receiver_mass_kg),
            dimensions: Some(evidence.receiver_dimensions),
            placement: placements[2],
        },
        InstalledMass {
            id: selection.electronics_record.id.clone(),
            mass_kg: Some(evidence.electronics_mass_kg),
            dimensions: Some(evidence.electronics_dimensions),
            placement: placements[3],
        },
        InstalledMass {
            id: "flight computer".to_owned(),
            mass_kg: Some(problem.systems.avionics_mass_kg),
            dimensions: Some(problem.systems.avionics_dimensions),
            placement: placements[4],
        },
        InstalledMass {
            id: "payload".to_owned(),
            mass_kg: Some(problem.objectives.payload_mass_kg),
            dimensions: Some(problem.objectives.payload_dimensions),
            placement: placements[5],
        },
    ]
}

fn propulsor_installations(
    selection: &Selection<'_>,
    requested_count: u16,
    fuselage_length_m: f64,
    wing: WingGeometry,
    single_esc_placement: Placement,
) -> (
    InstalledMotor,
    InstalledEsc,
    InstalledPropeller,
    Vec<InstalledPropulsor>,
) {
    let count = requested_count.max(1);
    let installation = |index: u16| {
        let y_m = propulsor_lateral_position(index, count, wing.span_m);
        let x_m = if count == 1 {
            0.02 * fuselage_length_m
        } else {
            wing.leading_edge_x_m + 0.25 * wing.mean_chord_m
        };
        let external = |x_m, y_m| Placement {
            center_x_m: x_m,
            center_y_m: y_m,
            center_z_m: 0.0,
            inside_equipment_bay: false,
        };
        let esc_placement = if count == 1 {
            single_esc_placement
        } else {
            external(x_m + 0.04 * wing.mean_chord_m, y_m)
        };
        (
            InstalledMotor {
                id: selection.motor_record.id.clone(),
                spec: selection.motor.clone(),
                placement: external(x_m, y_m),
            },
            InstalledEsc {
                id: selection.esc_record.id.clone(),
                spec: selection.esc.clone(),
                placement: esc_placement,
            },
            InstalledPropeller {
                id: selection.propeller_record.id.clone(),
                spec: selection.propeller.clone(),
                placement: external(x_m - 0.03 * wing.mean_chord_m, y_m),
            },
        )
    };
    let (motor, esc, propeller) = installation(0);
    let additional_propulsors = (1..count)
        .map(|index| {
            let (motor, esc, propeller) = installation(index);
            InstalledPropulsor {
                motor,
                esc,
                propeller,
            }
        })
        .collect();
    (motor, esc, propeller, additional_propulsors)
}

/// Airframe-analysis inputs of a generated design: the wing as one rectangular
/// panel per half (the generator has no taper), the tails from the empennage
/// areas and spans with their quarter-chord at `tail_arm_m` aft of the wing,
/// and the spar from the rule-of-thumb cap fractions.  Spar laminates are the
/// literature default until `alas-uav-db` carries ply data (see
/// `airframe::materials`).
fn airframe_inputs(
    problem: &OptimizationProblem<'_>,
    geometry: &GeneratedGeometry,
) -> AirframeInputs {
    let model = problem.model;
    let wing = geometry.wing;
    let tail = geometry.empennage;
    let wing_ac_x = wing.leading_edge_x_m + 0.25 * wing.mean_chord_m;
    let tail_chord = tail.horizontal_area_m2 / tail.horizontal_span_m;
    let tail_root_x = wing_ac_x + tail.tail_arm_m - 0.25 * tail_chord;
    let fin_chord = tail.vertical_area_m2 / tail.vertical_span_m;
    let fin_root_x = wing_ac_x + tail.tail_arm_m - 0.25 * fin_chord;
    let planform = |panel_span_m: f64, chord_m: f64, x_m: f64, vertical: bool| Planform {
        panels: vec![Panel {
            span_m: panel_span_m,
            chord_root_m: chord_m,
            chord_tip_m: chord_m,
            root_le_m: [x_m, 0.0, 0.0],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: !vertical,
        vertical,
    };
    let depth_m = model.spar_cap_separation_fraction * wing.mean_chord_m;
    let cap_width_m = model.spar_cap_width_fraction * wing.mean_chord_m;
    AirframeInputs {
        geometry: AirframeGeometry {
            wing: planform(
                0.5 * wing.span_m,
                wing.mean_chord_m,
                wing.leading_edge_x_m,
                false,
            ),
            horizontal: Some(TailSurfaceGeom {
                role: TailRole::Horizontal,
                planform: planform(0.5 * tail.horizontal_span_m, tail_chord, tail_root_x, false),
                v_dihedral_rad: 0.0,
                incidence_rad: 0.0,
                count: 1,
            }),
            vertical: Some(TailSurfaceGeom {
                role: TailRole::Vertical,
                planform: planform(tail.vertical_span_m, fin_chord, fin_root_x, true),
                v_dihedral_rad: 0.0,
                incidence_rad: 0.0,
                count: 1,
            }),
            tail_kind: TailKind::Conventional,
        },
        cruise_speed_m_s: problem.objectives.cruise_speed_m_s,
        air_density_kg_m3: model.air_density_kg_m3,
        limit_load_factor: model.limit_load_factor,
        safety_factor: model.structural_safety_factor,
        trim_setup: estimate_trim_setup(),
        spar_geometry: SparGeometry {
            semispan_m: 0.5 * wing.span_m,
            root_depth_m: depth_m,
            tip_depth_m: depth_m,
            root_cap_width_m: cap_width_m,
            tip_cap_width_m: cap_width_m,
        },
        spar_materials: SparMaterials::LiteratureDefault,
        spar_stations: 200,
    }
}

fn propulsor_lateral_position(index: u16, count: u16, wing_span_m: f64) -> f64 {
    if count == 1 {
        0.0
    } else {
        let fraction = f64::from(index) / f64::from(count - 1) - 0.5;
        0.7 * wing_span_m * fraction
    }
}

fn control_torques(problem: &OptimizationProblem<'_>, geometry: GeneratedGeometry) -> (f64, f64) {
    let model = problem.model;
    let q = 0.5 * model.air_density_kg_m3 * problem.objectives.cruise_speed_m_s.powi(2);
    let wing = geometry.wing;
    let aileron = model.hinge_moment_coefficient
        * q
        * model.aileron_area_fraction
        * wing.area_m2
        * model.aileron_chord_fraction
        * wing.mean_chord_m;
    let horizontal_chord =
        geometry.empennage.horizontal_area_m2 / geometry.empennage.horizontal_span_m;
    let elevator = model.hinge_moment_coefficient
        * q
        * model.elevator_area_fraction
        * geometry.empennage.horizontal_area_m2
        * model.elevator_chord_fraction
        * horizontal_chord;
    (aileron, elevator)
}
