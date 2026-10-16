// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission flight and score evaluation of one generated UAV for the
//! competition objective (split from `competition.rs` for the size budget).

use super::compat::{map_for, FULL_CELL_VOLTAGE_V};
use super::competition::{
    CompetitionEvaluation, CompetitionProblem, CompiledObjective, GATE_TOLERANCE,
};
use super::{
    GeneratedGeometry, OptimizationProblem, PropulsionMap, PropulsionOperatingPoint,
    SelectedComponents,
};
use crate::airframe::analysis::{AirframeAnalysis, STATIC_MARGIN_BAND};
use crate::perf::{
    fly_mission, score_inputs, Airframe, BatteryState, Environment, ParabolicPolar, PerfError,
    PowerPoint, Powertrain,
};
use crate::score::RoundInputs;
use crate::{UavDesign, UavReport};

/// Lift coefficient on the ground roll (ASSUMPTION: 2 degrees attitude with
/// a lift slope of 2 pi per radian).
pub const GROUND_LIFT_COEFFICIENT: f64 = 0.22;
/// Rolling friction coefficient (ASSUMPTION within the WUT-measured
/// 0.05-0.07 range quoted in `perf::Airframe`).
pub const ROLLING_FRICTION: f64 = 0.05;
/// Braking friction coefficient (ASSUMPTION).
pub const BRAKING_FRICTION: f64 = 0.4;

/// Propulsion operating points of one selected map, at full throttle, per
/// propulsor, with the throttle law documented on [`Powertrain::evaluate`].
///
/// ASSUMPTION: thrust is linear in throttle and the electrical power of the
/// propulsors scales as thrust to the 3/2 power at fixed airspeed (ideal
/// actuator-disc scaling), so battery current scales with throttle to the
/// 3/2 power. The map is full-throttle data only; a true throttle map needs
/// the power solver. Speeds outside the map's coverage are errors, never
/// extrapolated.
pub(super) struct MapPowertrain<'a> {
    pub(super) map: &'a PropulsionMap,
    pub(super) pack_voltage_v: f64,
}

impl Powertrain for MapPowertrain<'_> {
    fn evaluate(
        &self,
        throttle: f64,
        airspeed_m_s: f64,
        _air_density_kg_m3: f64,
        _battery: &BatteryState,
    ) -> Result<PowerPoint, PerfError> {
        if !(throttle.is_finite() && airspeed_m_s.is_finite()) {
            return Err(PerfError::Powertrain("non-finite operating point".into()));
        }
        let throttle = throttle.clamp(0.0, 1.0);
        let point: PropulsionOperatingPoint = self.map.at_speed(airspeed_m_s).ok_or_else(|| {
            PerfError::Powertrain(format!(
                "airspeed {airspeed_m_s:.2} m/s outside propulsion-map coverage"
            ))
        })?;
        let count = f64::from(self.map.motor_count);
        Ok(PowerPoint {
            thrust_n: count * throttle * point.thrust_n,
            battery_current_a: count * throttle.powf(1.5) * point.motor_current_a,
            battery_voltage_v: self.pack_voltage_v,
        })
    }
}

/// Hard airframe gates of the M4 analysis as normalised margins (>= 0 passes).
///
/// * `static_margin_band`: the M4 screening band [`STATIC_MARGIN_BAND`] at the
///   loaded CG, normalised by the band width (negative outside the band).
/// * `spar_bending_mos` and `spar_shear_mos`: spar margins of safety with the
///   safety factor already applied to the limit loads.
pub(super) fn airframe_gates(analysis: &AirframeAnalysis) -> Vec<(&'static str, f64)> {
    let (lo, hi) = STATIC_MARGIN_BAND;
    let sm = analysis.static_margin;
    let mut gates = vec![(
        "static_margin_band",
        ((sm - lo) / (hi - lo)).min((hi - sm) / (hi - lo)),
    )];
    if let Some(spar) = &analysis.spar {
        gates.push(("spar_bending_mos", spar.bending_margin_of_safety));
        gates.push(("spar_shear_mos", spar.shear_margin_of_safety));
    }
    gates
}

/// Why a candidate could not be scored, with its violation magnitude (>= 1).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Rejection {
    pub(super) violation: f64,
    pub(super) note: String,
}

impl Rejection {
    pub(super) fn plain(note: impl Into<String>) -> Self {
        Self {
            violation: 1.0,
            note: note.into(),
        }
    }
}

/// Fly the mission and score the round. `Err` is a reason string for the
/// candidate, not a search failure.
pub(super) fn fly_and_score(
    base: &OptimizationProblem<'_>,
    objective: &CompiledObjective,
    problem: &CompetitionProblem<'_>,
    design: &UavDesign,
    geometry: &GeneratedGeometry,
    components: &SelectedComponents,
    report: &UavReport,
) -> Result<CompetitionEvaluation, Rejection> {
    let mass_kg = report
        .takeoff_mass_kg
        .ok_or_else(|| Rejection::plain("takeoff mass unknown"))?;
    let battery = &design.battery.spec;
    let cells = battery
        .series_cells
        .ok_or_else(|| Rejection::plain("battery series-cell count unknown"))?;
    let map = map_for(
        base.propulsion_maps,
        &components.motor_id,
        &components.propeller_id,
        cells,
    )
    .ok_or_else(|| Rejection::plain("no propulsion map for the selected bundle"))?;
    let nominal_voltage_v = battery
        .nominal_voltage_v
        .ok_or_else(|| Rejection::plain("battery nominal voltage unknown"))?;
    let rated_current_a = battery
        .rated_discharge_current_a()
        .ok_or_else(|| Rejection::plain("battery rating unknown"))?;
    let capacity_ah = battery
        .capacity_ah
        .ok_or_else(|| Rejection::plain("battery capacity unknown"))?;
    let wing = geometry.wing;
    let airframe = Airframe {
        mass_kg,
        payload_kg: base.objectives.payload_mass_kg,
        wing_area_m2: wing.area_m2,
        span_m: wing.span_m,
        polar: Box::new(ParabolicPolar::from_aspect_ratio(
            base.model.zero_lift_drag_coefficient,
            wing.aspect_ratio,
            base.model.oswald_efficiency,
            base.model.maximum_lift_coefficient,
        )),
        cl_ground: GROUND_LIFT_COEFFICIENT,
        // ASSUMPTION: landing-gear drag is not in the preliminary model, so it
        // is zero here and take-off distance is optimistic by that amount.
        gear_drag_area_m2: 0.0,
        wing_height_m: geometry.landing_gear.minimum_leg_length_m,
        mu_roll: ROLLING_FRICTION,
        mu_brake: BRAKING_FRICTION,
        n_max: base.model.limit_load_factor,
    };
    airframe
        .validate()
        .map_err(|error| Rejection::plain(format!("airframe: {error}")))?;
    let powertrain = MapPowertrain {
        map,
        pack_voltage_v: nominal_voltage_v,
    };
    let environment = Environment::new(base.model.air_density_kg_m3, 0.0)
        .map_err(|error| Rejection::plain(format!("environment: {error}")))?;
    let mission = fly_mission(
        &airframe,
        &powertrain,
        environment,
        &objective.segments,
        &objective.config,
    )
    .map_err(|error| match error {
        // A landing or turn that needs more lift than CLmax is graded by the
        // overshoot, so the search can move towards feasibility.
        PerfError::Stall {
            cl,
            cl_max,
            context,
        } => Rejection {
            violation: 1.0 + (cl / cl_max - 1.0).max(0.0),
            note: format!("stall: CL {cl:.3} > CLmax {cl_max:.3} ({context})"),
        },
        other => Rejection::plain(format!("mission: {other}")),
    })?;
    let mut inputs: RoundInputs = score_inputs(&mission, &airframe);
    let derived: [(&str, f64); 6] = [
        ("motor_count", f64::from(map.motor_count)),
        ("pack_voltage_max", f64::from(cells) * FULL_CELL_VOLTAGE_V),
        ("battery_capacity", capacity_ah * 3600.0),
        ("air_density", base.model.air_density_kg_m3),
        (
            "prop_diameter",
            design.propeller.spec.diameter_m.unwrap_or(f64::NAN),
        ),
        ("battery_continuous_rating", rated_current_a),
    ];
    for (name, value) in derived {
        inputs.values.insert(name.to_owned(), value);
    }
    // Gate inputs derived from the selected pack: series cells and nominal energy.
    inputs
        .values
        .insert("cell_count".to_owned(), f64::from(cells));
    if let Some(energy_wh) = battery.nominal_energy_wh() {
        inputs
            .values
            .insert("pack_energy_max".to_owned(), energy_wh);
    }
    inputs
        .values
        .insert("announced_takeoff".to_owned(), problem.announced_takeoff_m);
    for (name, value) in &problem.extra_inputs {
        inputs.values.insert(name.clone(), *value);
    }
    // M4 static margin at the loaded CG (fraction of MAC). The preset gate reads
    // the envelope over payload states; M4 gives one loaded-CG value, so the
    // minimum and the maximum are both that value (a stated simplification).
    let mut airframe_violation = 0.0;
    let mut airframe_failed = Vec::new();
    match report.airframe.as_ref() {
        Some(analysis) => {
            if analysis.static_margin.is_finite() {
                inputs
                    .values
                    .insert("static_margin_min".to_owned(), analysis.static_margin);
                inputs
                    .values
                    .insert("static_margin_max".to_owned(), analysis.static_margin);
            }
            for (gate, margin) in airframe_gates(analysis) {
                if margin.is_nan() || margin < -GATE_TOLERANCE {
                    airframe_failed.push(gate.to_owned());
                    airframe_violation += if margin.is_finite() { -margin } else { 1.0 };
                }
            }
        }
        None => {
            airframe_violation += 1.0;
            airframe_failed.push("airframe_analysis".to_owned());
        }
    }
    let round = objective
        .engine
        .evaluate_round(&inputs)
        .map_err(|error| Rejection::plain(format!("round: {error}")))?;
    let takeoff_distance_m = inputs.values.get("takeoff_distance").copied();
    let mut violation = airframe_violation;
    let mut failed_gates = airframe_failed;
    for gate in round
        .gates
        .iter()
        .filter(|gate| gate.hard && gate.margin < -GATE_TOLERANCE)
    {
        failed_gates.push(gate.id.clone());
        violation += if gate.margin.is_finite() {
            -gate.margin
        } else {
            1.0
        };
    }
    let mut note = None;
    match takeoff_distance_m {
        Some(takeoff) if takeoff.is_finite() => {
            let excess = (takeoff - problem.announced_takeoff_m) / problem.announced_takeoff_m;
            if excess > 0.0 {
                violation += excess;
                failed_gates.push("takeoff_distance".to_owned());
            }
        }
        _ => {
            violation += 1.0;
            note = Some("take-off distance not produced by the mission".to_owned());
        }
    }
    let score = round.score;
    if !score.is_finite() {
        return Err(Rejection::plain("competition score is not finite"));
    }
    let feasible = violation == 0.0;
    Ok(CompetitionEvaluation {
        feasible,
        violation,
        score: Some(score),
        takeoff_distance_m,
        takeoff_mass_kg: Some(mass_kg),
        unevaluated_gates: round.unevaluated_gates.clone(),
        failed_gates,
        note,
    })
}
