// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `UavSpec` + powertrain -> models -> mission -> score.
//!
//! [`Vehicle`] gathers what the integrated chain needs beyond the spec: the
//! hull description, the electric chain, ground handling and aerodynamic
//! settings.  [`Vehicle::models`] builds the mission [`Airframe`] (with the
//! viscous-VLM polar) and the [`ElectricPowertrain`]; [`Vehicle::assess`] flies
//! the spec's mission segments and evaluates the preset's score for one round.
//!
//! Mass: the sum of the tree nodes' masses ([`tree_mass`]) - `Allowance` and
//! `Ballast` nodes carry a mass, `Payload` nodes and any other mass-bearing
//! node need `mass_override_kg`.  A mass-bearing node without a mass is an
//! error, never a default; the airframe lane will replace this by the
//! bottom-up estimate.

use std::collections::BTreeMap;

use crate::perf::{
    fly_mission, score_inputs, Airframe, CruiseControl, Environment, MissionConfig, MissionResult,
};
use crate::power;
use crate::score::{presets, RoundResult, ScoreEngine};
use crate::spec::score::Preset;
use crate::spec::skeleton::Part;
use crate::spec::UavSpec;

use super::geometry::build_wing;
use super::hull::{components, Hull};
use super::polar::{PolarInfo, PolarSettings, WingPolar};
use super::powertrain::{ElectricPowertrain, ElectricSettings};
use super::slipstream_drag::SlipstreamDrag;
use super::trim_drag::{wing_cm0, TrimDrag, TrimSettings};
use super::CoupledError;

/// Aerodynamic model settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AeroSettings {
    /// NeuralFoil critical amplification factor (calibration parameter;
    /// tunnel-clean 9-12, RC with propeller turbulence 5-7: plan 3.1, INFERENCE).
    pub n_crit: f64,
    /// Spanwise strips per half span.
    pub strips: usize,
    /// Trim drag of the tail (`None`: not modelled).  Needs a tail in the spec.
    pub trim: Option<TrimSettings>,
}

impl Default for AeroSettings {
    fn default() -> Self {
        Self {
            n_crit: 9.0,
            strips: 24,
            trim: Some(TrimSettings::default()),
        }
    }
}

/// Ground handling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundSetup {
    /// Rolling friction coefficient (WUT measured 0.05 - 0.07, competitions.md).
    pub mu_roll: f64,
    /// Braking friction coefficient.
    pub mu_brake: f64,
    /// Wing angle of attack on the ground roll (gear attitude), deg.
    pub ground_alpha_deg: f64,
    /// Wing height above the ground on the roll, m.
    pub wing_height_m: f64,
    /// Structural load-factor limit.
    pub n_max: f64,
}

/// Everything the chain needs beyond the spec.
pub struct Vehicle<'a> {
    /// The design.
    pub spec: &'a UavSpec,
    /// Fuselage envelope and itemised drag parts.
    pub hull: Hull,
    /// Motor - ESC - pack - propeller.
    pub chain: power::Powertrain,
    /// Electric adapter settings (current cap, temperatures, state of charge).
    pub electric: ElectricSettings,
    /// Ground handling.
    pub ground: GroundSetup,
    /// Aerodynamic settings.
    pub aero: AeroSettings,
    /// Extra score inputs (rule quantities the simulation cannot know).
    pub extra_inputs: BTreeMap<String, f64>,
}

/// The built models.
pub struct Models {
    /// Mission airframe (the polar is boxed inside).
    pub airframe: Airframe,
    /// Powertrain adapter.
    pub powertrain: ElectricPowertrain,
    /// Polar summary numbers.
    pub polar: PolarInfo,
    /// Modelling limits that apply to this spec.
    pub notes: Vec<String>,
}

/// A flown and scored round.
pub struct Assessment {
    /// The models used.
    pub models: Models,
    /// The flown mission.
    pub mission: MissionResult,
    /// Named score inputs.
    pub inputs: crate::score::RoundInputs,
    /// Evaluated round.
    pub round: RoundResult,
}

/// `(take-off mass, payload mass)` of the spec tree, kg.
///
/// # Errors
///
/// [`crate::coupled::CoupledError::Spec`] when a mass-bearing node has no mass.
pub fn tree_mass(spec: &UavSpec) -> Result<(f64, f64), CoupledError> {
    let (mut total, mut payload) = (0.0, 0.0);
    for node in &spec.tree {
        let mass = match (&node.part, node.mass_override_kg) {
            (_, Some(m)) => m,
            (Part::Allowance { mass_kg } | Part::Ballast { mass_kg }, None) => *mass_kg,
            (Part::Assembly, None) => continue,
            (_, None) => {
                return Err(CoupledError::Spec(format!(
                    "node '{}' has no mass (set mass_override_kg)",
                    node.name
                )))
            }
        };
        if !(mass.is_finite() && mass >= 0.0) {
            return Err(CoupledError::Spec(format!(
                "node '{}' has a bad mass {mass}",
                node.name
            )));
        }
        total += mass;
        if matches!(node.part, Part::Payload { .. }) {
            payload += mass;
        }
    }
    if total <= 0.0 {
        return Err(CoupledError::Spec("the tree carries no mass".into()));
    }
    Ok((total, payload))
}

/// Mission settings read from the preset parameters: the ACC flight clock and
/// the flight window.  Cruise holds full throttle (the current limiter, when
/// configured, then sets the cruise condition).
pub fn mission_config_for(preset: &Preset) -> MissionConfig {
    let param = |name: &str| preset.parameters.iter().find(|p| p.name == name);
    MissionConfig {
        flight_clock_from_throttle_up: param("flight_clock_throttle_up")
            .is_some_and(|p| p.default > 0.0),
        window_s: param("flight_window_s").map(|p| p.default),
        cruise: CruiseControl::Throttle(1.0),
        ..MissionConfig::default()
    }
}

impl Vehicle<'_> {
    /// Build the airframe polar and the powertrain at `env`.
    ///
    /// # Errors
    ///
    /// [`CoupledError`] from any model.
    pub fn models(&self, env: &Environment) -> Result<Models, CoupledError> {
        let (mass, payload) = tree_mass(self.spec)?;
        let weight = mass * crate::perf::G0;
        let wing = build_wing(&self.spec.master, self.aero.n_crit, self.aero.strips)?;
        let comps = components(self.spec, &self.hull)?;
        let mut settings = PolarSettings::new(env.rho, weight);
        settings.t_k = self.electric.t_amb_k;
        settings.excrescence_fraction = self.hull.excrescence_fraction;
        let mut notes = wing.notes.clone();
        let trim = match &self.aero.trim {
            Some(ts) => {
                let xs = &wing.airplane.wings[0].xsecs;
                let y = &wing.stations_y_m;
                let w: Vec<f64> = (0..xs.len())
                    .map(|i| {
                        let (lo, hi) = (y[i.saturating_sub(1)], y[(i + 1).min(y.len() - 1)]);
                        xs[i].chord * xs[i].chord * 0.5 * (hi - lo)
                    })
                    .collect();
                let trim = TrimDrag::new(self.spec, ts, wing_cm0(&wing.sections, &w, 2.0e5)?)?;
                if trim.is_some() {
                    notes.push(format!(
                        "trim drag: static margin {:.2} MAC and an elevator-trimmed tail at every CL are ASSUMPTIONS (upper bound)",
                        ts.static_margin
                    ));
                }
                trim
            }
            None => None,
        };
        let polar = WingPolar::build(&wing, &comps, &settings)?.with_trim(trim);
        let info = polar.info.clone();
        let slipstream = if self.hull.motors.is_empty() {
            notes.push("no motor stations in the hull: slipstream drag not modelled".to_owned());
            None
        } else {
            let washed: Vec<_> = comps.iter().filter(|c| c.in_slipstream).cloned().collect();
            let other_cda_m2: f64 = if washed.is_empty() {
                0.0
            } else {
                let air = crate::aero::AirState::from_rho_t(15.0, env.rho, self.electric.t_amb_k)?;
                crate::aero::drag_lowre::build_up(&washed, &air, wing.s_ref_m2, 0.0)?
                    .lines
                    .iter()
                    .map(|l| l.cda_m2)
                    .sum()
            };
            Some(SlipstreamDrag {
                radius_m: 0.5 * self.chain.prop.diameter_m(),
                motors: self.hull.motors.clone(),
                wing_cd0: info.wing_cd0,
                chord_m: wing.s_ref_m2 / wing.span_m,
                other_cda_m2,
                t_k: self.electric.t_amb_k,
            })
        };
        let cl_ground = polar
            .cl_at_alpha(self.ground.ground_alpha_deg.to_radians())
            .max(0.0);
        let gear_area: f64 = if self.hull.ground_only.is_empty() {
            0.0
        } else {
            let air = crate::aero::AirState::from_rho_t(12.0, env.rho, self.electric.t_amb_k)?;
            crate::aero::drag_lowre::build_up(&self.hull.ground_only, &air, wing.s_ref_m2, 0.0)?
                .lines
                .iter()
                .map(|l| l.cda_m2)
                .sum()
        };
        let airframe = Airframe {
            mass_kg: mass,
            payload_kg: payload,
            wing_area_m2: wing.s_ref_m2,
            span_m: wing.span_m,
            polar: Box::new(polar),
            cl_ground,
            // Fixed gear sits in the polar already; a retractable gear's
            // extended drag (`Hull::ground_only`) acts on the ground only.
            gear_drag_area_m2: gear_area,
            wing_height_m: self.ground.wing_height_m,
            mu_roll: self.ground.mu_roll,
            mu_brake: self.ground.mu_brake,
            n_max: self.ground.n_max,
        };
        airframe.validate()?;
        let powertrain = ElectricPowertrain::new(self.chain.clone(), self.electric, env.rho)?
            .with_slipstream_drag(slipstream);
        Ok(Models {
            airframe,
            powertrain,
            polar: info,
            notes,
        })
    }

    /// Fly the spec's mission and evaluate one scored round.
    ///
    /// # Errors
    ///
    /// [`CoupledError`] from any model or from the score engine.
    pub fn assess(
        &self,
        env: Environment,
        cfg: &MissionConfig,
    ) -> Result<Assessment, CoupledError> {
        let models = self.models(&env)?;
        let mission = fly_mission(
            &models.airframe,
            &models.powertrain,
            env,
            &self.spec.mission.segments,
            cfg,
        )?;
        let mut inputs = score_inputs(&mission, &models.airframe);
        let d = &self.spec.mission.decisions;
        let mut put = |k: &str, v: f64| {
            inputs.values.insert(k.to_owned(), v);
        };
        if let Some(v) = d.announced_takeoff_m {
            put("announced_takeoff", v);
        }
        if let Some(v) = d.payload_count {
            put("payload_count", f64::from(v));
        }
        if let Some(v) = d.declared_payload_count {
            put("declared_payload_count", f64::from(v));
        }
        let chain = models.powertrain.chain();
        let cells = chain.battery.voc(1.0);
        put("motor_count", f64::from(chain.n_motors));
        put("pack_voltage_max", cells);
        put("battery_capacity", chain.battery.capacity_c());
        put("air_density", env.rho);
        put("prop_diameter", chain.prop.diameter_m());
        if let Some(i) = chain.battery.i_cont_a {
            put("battery_continuous_rating", i);
        }
        for (name, p) in &self.spec.mission.parameters {
            put(name, p.value);
        }
        for (k, v) in &self.extra_inputs {
            put(k, *v);
        }
        let preset = presets::load(&self.spec.mission.preset.id)?;
        let engine = ScoreEngine::new(&preset, self.spec.mission.preset.variant.as_deref())?;
        let round = engine.evaluate_round(&inputs)?;
        Ok(Assessment {
            models,
            mission,
            inputs,
            round,
        })
    }
}
