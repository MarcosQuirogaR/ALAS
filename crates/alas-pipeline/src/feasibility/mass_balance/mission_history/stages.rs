// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-stage summary of a walked mission.

use alas_config::StageKind;

use super::{HistoryInputs, MissionCgPoint, MissionEventRecord, MissionStageResult};
use crate::mission_stage::plan::Applied;

pub(super) fn stage_results(
    inputs: &HistoryInputs<'_>,
    points: &[MissionCgPoint],
    records: &[MissionEventRecord],
) -> Vec<MissionStageResult> {
    let mission = inputs.mission;
    let mut stages = Vec::new();
    for (index, stage) in inputs.case.plan.stages.iter().enumerate() {
        let legs: Vec<usize> = inputs
            .case
            .stage_of_segment
            .iter()
            .enumerate()
            .filter_map(|(leg, owner)| (*owner == Some(index)).then_some(leg))
            .collect();
        let mut result = MissionStageResult {
            id: stage.id.clone(),
            label: stage.kind.label(),
            fuel_kg: 0.0,
            time_s: 0.0,
            distance_m: 0.0,
            mass_start_kg: f64::NAN,
            mass_end_kg: f64::NAN,
            cg_end_pct_mac: f64::NAN,
            cg_range_pct_mac: [f64::INFINITY, f64::NEG_INFINITY],
            within_envelope: true,
            field: None,
        };
        if let StageKind::Taxi { .. } = stage.kind {
            result.fuel_kg = inputs
                .case
                .events
                .iter()
                .filter(|event| event.stage == index)
                .map(|event| match event.applied {
                    Applied::Taxi { fuel_kg } => fuel_kg,
                    _ => 0.0,
                })
                .sum();
        }
        for &leg in &legs {
            let c = &mission.segments[leg].conditions;
            let (Some(first), Some(last)) = (c.total_mass_kg.first(), c.total_mass_kg.last())
            else {
                continue;
            };
            if result.mass_start_kg.is_nan() {
                result.mass_start_kg = *first;
            }
            result.mass_end_kg = *last;
            result.fuel_kg += first - last;
            result.time_s +=
                c.time_s.last().copied().unwrap_or(0.0) - c.time_s.first().copied().unwrap_or(0.0);
            result.distance_m += c.aircraft_range_m.last().copied().unwrap_or(0.0)
                - c.aircraft_range_m.first().copied().unwrap_or(0.0);
        }
        // A stage without legs (taxi) is its events: the aircraft before and after.
        let own: Vec<&MissionEventRecord> = records
            .iter()
            .filter(|record| record.stage == index)
            .collect();
        if legs.is_empty() {
            if let (Some(first), Some(last)) = (own.first(), own.last()) {
                result.mass_start_kg = first.mass_before_kg;
                result.mass_end_kg = last.mass_after_kg;
            }
        }
        for point in points.iter().filter(|point| point.stage == Some(index)) {
            result.cg_range_pct_mac[0] = result.cg_range_pct_mac[0].min(point.cg_pct_mac);
            result.cg_range_pct_mac[1] = result.cg_range_pct_mac[1].max(point.cg_pct_mac);
            result.within_envelope &= point.within_envelope;
            result.cg_end_pct_mac = point.cg_pct_mac;
        }
        stages.push(result);
    }
    stages
}
