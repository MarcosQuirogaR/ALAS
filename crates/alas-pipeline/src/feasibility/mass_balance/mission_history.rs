// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mass, centre of gravity and tank state through a flown mission plan.
//!
//! The ledger of the run states the aircraft at the ramp, tank by tank. This
//! walks it through the flown legs of a [`PlanLoadCase`]: the fuel each leg
//! burned comes off the tanks in the burn order (the trim tank held while a
//! trim control is on), each event changes the aircraft where it happens (a
//! release takes the item's mass and moment out, a store puts them in, fuel is
//! received, offloaded or moved between tanks), and at every leg end and event
//! the centre of gravity is checked against the model envelope of that mass:
//! the landing-trim forward limit and the static-margin aft limit
//! (`alas-opt` envelope, the flight mechanisms).

use alas_mass::breakdown::MassBreakdown;
use alas_mass::ledger::{InertiaTensor, MassProperties, MassRole};
use alas_mass::statement::{LoadState, MassStatement};
use alas_mass::tanks::{FuelState, FuelTankLayout};
use alas_mission::segments::SegmentSpec;
use alas_mission::MissionResult;
use alas_opt::envelope::{
    assess_model_cg_envelope_with_ledger_and_landing, LedgerLandingState, LedgerLoadingBasis,
};

use crate::feasibility::FuelLoadingAssessment;
use crate::mission_stage::plan::{Applied, CarriedItem, PlanLoadCase, ResolvedEvent};

mod findings;
mod stages;
mod trim;
mod types;
pub(super) use findings::history_findings;
pub use types::{MissionCgPoint, MissionEventRecord, MissionHistory, MissionStageResult};

/// What the walk reads.
pub(super) struct HistoryInputs<'a> {
    pub config: &'a alas_config::AlasConfig,
    pub report: &'a crate::full_analysis::AnalysisReport,
    pub statement: &'a MassStatement,
    pub tanks: &'a FuelTankLayout,
    pub takeoff_state: &'a FuelState,
    pub basis: LedgerLoadingBasis,
    pub mac_le_x_m: f64,
    pub mac_m: f64,
    pub mission: &'a MissionResult,
    pub case: &'a PlanLoadCase,
}

struct Walker<'a> {
    inputs: &'a HistoryInputs<'a>,
    state: FuelState,
    released: Vec<CarriedItem>,
    added: Vec<CarriedItem>,
    trim_band: Option<[f64; 2]>,
    /// Trim pump rate, kg/s; `None` moves trim fuel at once.
    rate_kg_s: Option<f64>,
    /// A trim-fuel release is in progress (pump rate set): the fuel goes forward.
    releasing: bool,
    notes: Vec<String>,
}

impl Walker<'_> {
    fn properties(&self, state: &FuelState) -> MassProperties {
        let inputs = self.inputs;
        let base = inputs
            .statement
            .with_fuel_items(&state.mass_items(inputs.tanks));
        let removed = self
            .released
            .iter()
            .map(|item| MassProperties::point(-item.mass_kg, item.position_m));
        let added = self
            .added
            .iter()
            .map(|item| MassProperties::point(item.mass_kg, item.position_m));
        let parts: Vec<MassProperties> =
            std::iter::once(base).chain(removed).chain(added).collect();
        MassProperties::combine(parts.iter())
    }

    fn pct_mac(&self, props: &MassProperties) -> f64 {
        100.0 * (props.cg_m[0] - self.inputs.mac_le_x_m) / self.inputs.mac_m
    }

    fn apply(&mut self, applied: &Applied) -> String {
        let layout = self.inputs.tanks;
        match applied {
            Applied::Released(items) => {
                self.released.extend(items.iter().cloned());
                let names: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
                let mass: f64 = items.iter().map(|item| item.mass_kg).sum();
                format!(
                    "release {} item(s) ({mass:.0} kg): {}",
                    items.len(),
                    names.join(", ")
                )
            }
            Applied::Added(item) => {
                self.added.push(item.clone());
                format!("store {} ({:.0} kg) added", item.id, item.mass_kg)
            }
            Applied::Refuel { mass_kg } if *mass_kg >= 0.0 => {
                match self.state.received(layout, *mass_kg) {
                    Ok((state, taken)) => {
                        self.state = state;
                        if taken + 0.5 < *mass_kg {
                            self.notes.push(format!(
                                "air-to-air refuelling: only {taken:.0} of {mass_kg:.0} kg fit in the tanks"
                            ));
                        }
                        format!("refuel +{taken:.0} kg")
                    }
                    Err(error) => format!("refuel failed: {error}"),
                }
            }
            Applied::Refuel { mass_kg } => match self.state.burned(layout, -mass_kg) {
                Ok(state) => {
                    self.state = state;
                    format!("offload {:.0} kg", -mass_kg)
                }
                Err(error) => {
                    self.notes
                        .push(format!("offload of {:.0} kg: {error}", -mass_kg));
                    "offload failed".to_owned()
                }
            },
            Applied::Transfer { from, to, mass_kg } => {
                match self.state.transferred(layout, from, to, *mass_kg) {
                    Ok((state, moved)) => {
                        self.state = state;
                        if moved + 0.5 < *mass_kg {
                            self.notes.push(format!(
                                "transfer {from} -> {to}: {moved:.0} of {mass_kg:.0} kg could move"
                            ));
                        }
                        format!("transfer {moved:.0} kg {from} -> {to}")
                    }
                    Err(error) => {
                        self.notes.push(format!("transfer {from} -> {to}: {error}"));
                        "transfer failed".to_owned()
                    }
                }
            }
            Applied::TrimOn {
                min_pct_mac,
                max_pct_mac,
            } => {
                self.trim_on([*min_pct_mac, *max_pct_mac]);
                format!(
                    "trim control on, band {min_pct_mac:.1} to {max_pct_mac:.1} % MAC {}",
                    self.trim_pump_text()
                )
            }
            Applied::TrimOff => {
                self.trim_off();
                format!(
                    "trim control off, trim fuel moved forward {}",
                    self.trim_pump_text()
                )
            }
            Applied::Taxi { fuel_kg } => {
                match self.state.burned(layout, *fuel_kg) {
                    Ok(state) => self.state = state,
                    Err(error) => self.notes.push(format!("taxi fuel: {error}")),
                }
                format!("taxi, {fuel_kg:.0} kg of fuel")
            }
        }
    }

    fn burn(&mut self, burn_kg: f64, dt_s: f64) {
        let layout = self.inputs.tanks;
        let result = if self.trim_band.is_some() || self.releasing {
            self.state.burned_holding_trim(layout, burn_kg)
        } else {
            self.state.burned(layout, burn_kg)
        };
        match result {
            Ok(state) => self.state = state,
            Err(error) => {
                // The flown mass wins: burn what the tanks hold.
                self.notes.push(format!("burn of {burn_kg:.0} kg: {error}"));
                if let Ok(state) = self
                    .state
                    .burned(layout, self.state.total_kg().min(burn_kg))
                {
                    self.state = state;
                }
            }
        }
        self.hold_trim(dt_s);
    }

    fn point(
        &self,
        label: String,
        stage: Option<usize>,
        time_s: f64,
        range_m: f64,
    ) -> MissionCgPoint {
        let inputs = self.inputs;
        let props = self.properties(&self.state);
        let landing = LedgerLandingState {
            mass_kg: props.mass_kg,
            cg_x_m: props.cg_m[0],
            cg_z_m: props.cg_m[2],
        };
        let assessment = assess_model_cg_envelope_with_ledger_and_landing(
            &inputs.report.airplane,
            inputs.basis,
            Some(landing),
            inputs.report.x_neutral_point,
            inputs
                .report
                .neutral_point_conditions
                .as_ref()
                .map_or(inputs.report.x_neutral_point, |conditions| {
                    conditions.critical
                }),
            inputs.report.airplane.c_ref,
            inputs.config,
        )
        .ok();
        let envelope = assessment
            .as_ref()
            .and_then(|assessment| assessment.loading_states.last());
        let (pct, forward, aft) =
            envelope.map_or((self.pct_mac(&props), f64::NAN, f64::NAN), |state| {
                (
                    state.cg_pct_mac,
                    state.physical_limits.fwd_limit_pct_mac,
                    state.physical_limits.aft_limit_pct_mac,
                )
            });
        MissionCgPoint {
            label,
            stage,
            time_s,
            range_m,
            mass_kg: props.mass_kg,
            cg_m: props.cg_m,
            cg_pct_mac: pct,
            forward_limit_pct_mac: forward,
            aft_limit_pct_mac: aft,
            within_envelope: pct >= forward - 1.0e-9 && pct <= aft + 1.0e-9,
            fuel_kg: self.state.total_kg(),
            trim_fuel_kg: self.state.trim_fuel_kg(inputs.tanks),
            tank_fuel_kg: self.state.tank_fills(inputs.tanks),
        }
    }
}

/// Walk the flown `mission` of `case` through the ledger.
pub(super) fn walk(inputs: &HistoryInputs<'_>) -> MissionHistory {
    let specs: Vec<SegmentSpec> = inputs
        .mission
        .segments
        .iter()
        .map(|segment| segment.spec.clone())
        .collect();
    let triggers = crate::mission_stage::plan::events::trigger_legs(&specs, &inputs.case.events);
    let mut walker = Walker {
        inputs,
        state: inputs.takeoff_state.clone(),
        released: Vec::new(),
        added: Vec::new(),
        trim_band: None,
        rate_kg_s: inputs.config.fuel_tanks.trim.transfer_rate_kg_s,
        releasing: false,
        notes: Vec::new(),
    };
    let mut points = vec![walker.point("ramp".to_owned(), None, 0.0, 0.0)];
    let mut records: Vec<MissionEventRecord> = Vec::new();
    let mut max_mismatch: f64 = 0.0;
    let mut leg_cg_x_m: Vec<[f64; 2]> = Vec::with_capacity(inputs.mission.segments.len());
    let (mut clock_s, mut range_m) = (0.0, 0.0);
    let fire = |walker: &mut Walker<'_>,
                points: &mut Vec<MissionCgPoint>,
                records: &mut Vec<MissionEventRecord>,
                event: &ResolvedEvent,
                time_s: f64,
                range_m: f64| {
        let before = walker.point("before".to_owned(), Some(event.stage), time_s, range_m);
        let description = walker.apply(&event.applied);
        let label = format!(
            "{} ({description})",
            inputs.case.plan.stages[event.stage].id
        );
        let after = walker.point(label, Some(event.stage), time_s, range_m);
        records.push(MissionEventRecord {
            stage: event.stage,
            at: event.at,
            description,
            mass_before_kg: before.mass_kg,
            mass_after_kg: after.mass_kg,
            cg_before_pct_mac: before.cg_pct_mac,
            cg_after_pct_mac: after.cg_pct_mac,
            within_envelope: after.within_envelope,
        });
        points.push(after);
    };
    for (leg, segment) in inputs.mission.segments.iter().enumerate() {
        for (index, event) in inputs.case.events.iter().enumerate() {
            if triggers[index] == leg {
                fire(
                    &mut walker,
                    &mut points,
                    &mut records,
                    event,
                    clock_s,
                    range_m,
                );
            }
        }
        let conditions = &segment.conditions;
        let (Some(first), Some(last)) = (
            conditions.total_mass_kg.first(),
            conditions.total_mass_kg.last(),
        ) else {
            leg_cg_x_m.push([f64::NAN; 2]);
            continue;
        };
        let cg_start_m = walker.properties(&walker.state).cg_m[0];
        let leg_dt_s = conditions.time_s.last().copied().unwrap_or(0.0)
            - conditions.time_s.first().copied().unwrap_or(0.0);
        walker.burn((first - last).max(0.0), leg_dt_s);
        leg_cg_x_m.push([cg_start_m, walker.properties(&walker.state).cg_m[0]]);
        clock_s += conditions.time_s.last().copied().unwrap_or(0.0)
            - conditions.time_s.first().copied().unwrap_or(0.0);
        range_m = conditions
            .aircraft_range_m
            .last()
            .copied()
            .unwrap_or(range_m);
        let stage = inputs.case.stage_of_segment.get(leg).copied().flatten();
        let point = walker.point(
            segment.spec.tag.clone(),
            stage,
            conditions.time_s.last().copied().unwrap_or(clock_s),
            range_m,
        );
        max_mismatch = max_mismatch.max((point.mass_kg - last).abs());
        points.push(point);
    }
    for (index, event) in inputs.case.events.iter().enumerate() {
        if triggers[index] >= inputs.mission.segments.len() {
            fire(
                &mut walker,
                &mut points,
                &mut records,
                event,
                clock_s,
                range_m,
            );
        }
    }
    let stages = stages::stage_results(inputs, &points, &records);
    MissionHistory {
        points,
        events: records,
        stages,
        notes: walker.notes,
        leg_cg_x_m,
        max_mass_mismatch_kg: max_mismatch,
    }
}

/// The history of `case` flown as `mission`, on the ledger `(statement, tanks,
/// takeoff fuel state)` and its named `states`.
pub(super) fn for_flight(
    config: &alas_config::AlasConfig,
    report: &crate::full_analysis::AnalysisReport,
    mission: &MissionResult,
    case: &PlanLoadCase,
    (statement, tanks, takeoff_state): (&MassStatement, &FuelTankLayout, &FuelState),
    states: &[super::MassStateSummary],
    findings: &mut Vec<crate::feasibility::PhysicalFinding>,
) -> Option<MissionHistory> {
    let basis = history_basis(states)?;
    let (mac_le_x_m, mac_m) = super::mac_reference(report);
    let cg_config = crate::report_config::config_with_geometry_clmax(config, report);
    let mut history = walk(&HistoryInputs {
        config: &cg_config,
        report,
        statement,
        tanks,
        takeoff_state,
        basis,
        mac_le_x_m,
        mac_m,
        mission,
        case,
    });
    for (index, check) in crate::feasibility::plan_field::checks(&cg_config, report, mission, case)
    {
        if let (Some(stage), Ok(check)) = (history.stages.get_mut(index), check) {
            stage.field = Some(check);
        }
    }
    findings.extend(history_findings(
        &history,
        &config.mission.plan,
        config.requirements.mtow_kg,
    ));
    Some(history)
}

/// The ledger's operating-empty, zero-fuel and flown-takeoff states as the
/// basis the envelope assessment anchors its other states to.
fn history_basis(
    states: &[super::MassStateSummary],
) -> Option<alas_opt::envelope::LedgerLoadingBasis> {
    let state = |label: &str| states.iter().find(|state| state.label == label);
    let (oew, zero_fuel, takeoff) = (
        state("operating empty")?,
        state("zero fuel")?,
        state("flown takeoff")?,
    );
    Some(alas_opt::envelope::LedgerLoadingBasis {
        oew_mass_kg: oew.mass_kg,
        oew_cg_x_m: oew.cg_m[0],
        oew_cg_z_m: oew.cg_m[2],
        zero_fuel_mass_kg: zero_fuel.mass_kg,
        zero_fuel_cg_x_m: zero_fuel.cg_m[0],
        zero_fuel_cg_z_m: zero_fuel.cg_m[2],
        takeoff_mass_kg: takeoff.mass_kg,
        takeoff_cg_x_m: takeoff.cg_m[0],
        takeoff_cg_z_m: takeoff.cg_m[2],
        takeoff_pitch_inertia_kg_m2: takeoff.inertia_cg.iyy,
        oew_pitch_inertia_kg_m2: oew.inertia_cg.iyy,
        zero_fuel_pitch_inertia_kg_m2: zero_fuel.inertia_cg.iyy,
    })
}

/// The fuel the ledger loads, kg: the ramp fuel of a flown plan, which starts
/// its history at the ramp (the taxi burn is its first event), else the fuel of
/// the dispatched route, else the analyzed load.
pub(super) fn loaded_fuel_kg(
    fuel_loading: &crate::feasibility::FuelLoadingAssessment,
    flight: Option<(&MissionResult, &PlanLoadCase)>,
) -> f64 {
    flight.map_or_else(
        || {
            fuel_loading
                .flown_carried_fuel_kg
                .unwrap_or(fuel_loading.analyzed_carried_fuel_kg)
        },
        |(_, case)| case.ramp_fuel_kg,
    )
}

/// The flown takeoff and landing states of `statement`.
///
/// A landing-weight-limited route leaves
/// [`FuelLoadingAssessment::offloaded_payload_kg`] of the analysed payload
/// behind, taken from every payload item in proportion to its mass at its own
/// station: the states lose that fraction of the payload body, its mass, its
/// moment and its centroidal inertia (each item's prism inertia scales with
/// its mass). The ledger rows, the zero-fuel state and the maximum-fuel state
/// stay the analysed load, which the MZFW check and the design-loading centre
/// of gravity read. A flown mission plan never offloads.
pub(super) fn flown_states(
    statement: &MassStatement,
    fuel_loading: &FuelLoadingAssessment,
) -> [MassProperties; 2] {
    let states = [LoadState::Takeoff, LoadState::Landing].map(|state| statement.state(state));
    let payload = statement
        .ledger()
        .properties_where(|item| item.role == MassRole::Payload);
    let offloaded_kg = fuel_loading.offloaded_payload_kg();
    if !(offloaded_kg > 0.0 && payload.mass_kg > 0.0) {
        return states;
    }
    // Removing a body is adding it with negative mass and inertia.
    let removed = -(offloaded_kg / payload.mass_kg).min(1.0);
    let inertia = payload.inertia_cg;
    let offload = MassProperties {
        mass_kg: removed * payload.mass_kg,
        cg_m: payload.cg_m,
        inertia_cg: InertiaTensor {
            ixx: removed * inertia.ixx,
            iyy: removed * inertia.iyy,
            izz: removed * inertia.izz,
            pxy: removed * inertia.pxy,
            pxz: removed * inertia.pxz,
            pyz: removed * inertia.pyz,
        },
    };
    states.map(|state| MassProperties::combine([state, offload].iter()))
}

/// The lumped breakdown `masses` at the payload the route flies
/// ([`flown_states`]).
pub(super) fn flown_lumped(
    mut masses: MassBreakdown,
    fuel_loading: &FuelLoadingAssessment,
) -> MassBreakdown {
    masses.payload -= fuel_loading.offloaded_payload_kg().min(masses.payload);
    masses
}
