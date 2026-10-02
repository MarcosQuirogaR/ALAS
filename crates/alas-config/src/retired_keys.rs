// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optimizer keys an earlier build wrote that no setting reads any more.
//!
//! A saved configuration states the whole solver group and the whole weights
//! table, and an unrecognized key is an error when a document is overlaid, so
//! a removed setting would otherwise make every older file unloadable. The
//! loading boundary drops these keys instead.

use std::borrow::Cow;

/// `optimizer.solver` keys that nothing reads any more.
pub const RETIRED_SOLVER_KEYS: &[&str] = &[
    "strategy",
    "display_progress",
    "seed_near_initial_design",
    "seed_perturbation_fraction",
];

/// `optimizer.objective` keys that nothing reads any more: `max_span_m` gave
/// way to the aerodrome reference code, whose letter now sets the span limit.
pub const RETIRED_OBJECTIVE_KEYS: &[&str] = &["max_span_m"];

/// `drag_model` keys that nothing reads any more: the Korn technology factor
/// follows the wing's declared section class, `geometry.wing.airfoil_class`,
/// instead of being a free coefficient.
pub const RETIRED_DRAG_MODEL_KEYS: &[&str] = &["korn_technology_factor"];

/// `optimizer.weights` keys that only the retired weighted lift-to-drag
/// objective or the retired tail-volume window read.
pub const RETIRED_WEIGHT_KEYS: &[&str] = &[
    "ld_weight",
    "alpha_penalty_scale",
    "alpha_min_penalty_deg",
    "alpha_max_penalty_deg",
    "span_penalty_per_m",
    "cd0_penalty_scale",
    "area_penalty_scale",
    "wing_loading_penalty_scale",
    "cg_penalty_scale",
    "cg_envelope_penalty_scale",
    "cg_envelope_reward",
    "fuel_penalty_scale",
    "fuel_volume_penalty_scale",
    "static_margin_penalty_scale",
    "thickness_floor",
    "thickness_penalty_scale",
    "fuselage_floor_m",
    "fuselage_penalty_scale",
    "min_hstab_area_fraction",
    "min_vstab_area_fraction",
    "tail_area_penalty_scale",
    "tail_volume_penalty_scale",
    "min_hstab_volume_coef",
    "max_hstab_volume_coef",
    "min_vstab_volume_coef",
    "max_vstab_volume_coef",
    "min_wing_position_fraction",
    "wing_position_penalty_scale",
    "payload_shortfall_penalty_scale",
    "fineness_ratio_max",
    "fineness_ratio_penalty_scale",
    "instability_failure_cost",
];

/// `optimizer.solver` keys that stated the search budget in generations.
/// Unlike the keys above they are converted, not dropped: see
/// [`with_migrated_solver_budget`].
pub const RETIRED_BUDGET_KEYS: &[&str] = &["max_iterations", "population_size"];

/// The values those keys had when a file omitted one of them.
const LEGACY_DEFAULT_ITERATIONS: i64 = 15;
const LEGACY_DEFAULT_POPULATION: i64 = 6;

/// A retired `max_iterations` / `population_size` pair and the refinement
/// evaluation budget it was converted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacySolverBudget {
    /// Saved generation count.
    pub max_iterations: i64,
    /// Saved population multiplier on the design-variable count.
    pub population_size: i64,
    /// `(max_iterations + 1) x population_size x D`: the initial population
    /// plus one trial per member per generation, `D` the design-variable
    /// count.
    pub refinement_max_evaluations: i64,
}

/// `data` with the retired budget keys converted to
/// `optimizer.solver.refinement.max_evaluations` (unless the document already
/// states one) and removed; borrowed, with no budget, when it names neither.
pub(crate) fn with_migrated_solver_budget(
    data: &serde_json::Value,
) -> (Cow<'_, serde_json::Value>, Option<LegacySolverBudget>) {
    let Some(solver) = data.pointer("/optimizer/solver").filter(|solver| {
        RETIRED_BUDGET_KEYS
            .iter()
            .any(|key| solver.get(*key).is_some())
    }) else {
        return (Cow::Borrowed(data), None);
    };
    let read = |key: &str, default: i64| {
        solver
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(default)
            .max(0)
    };
    let max_iterations = read("max_iterations", LEGACY_DEFAULT_ITERATIONS);
    let population_size = read("population_size", LEGACY_DEFAULT_POPULATION).max(1);
    let variables = crate::DESIGN_VARIABLE_SPECS.len() as i64;
    let budget = LegacySolverBudget {
        max_iterations,
        population_size,
        refinement_max_evaluations: max_iterations
            .saturating_add(1)
            .saturating_mul(population_size)
            .saturating_mul(variables),
    };
    let mut migrated = data.clone();
    if let Some(map) = migrated
        .pointer_mut("/optimizer/solver")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in RETIRED_BUDGET_KEYS {
            map.remove(*key);
        }
        if let Some(refinement) = map
            .entry("refinement")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
        {
            refinement
                .entry("max_evaluations")
                .or_insert_with(|| serde_json::json!(budget.refinement_max_evaluations));
        }
    }
    (Cow::Owned(migrated), Some(budget))
}

/// The optimizer groups whose retired keys are dropped at load.
const GROUPS: [(&str, &[&str]); 3] = [
    ("solver", RETIRED_SOLVER_KEYS),
    ("weights", RETIRED_WEIGHT_KEYS),
    ("objective", RETIRED_OBJECTIVE_KEYS),
];

fn carries(data: &serde_json::Value, group: &str, keys: &[&str]) -> bool {
    data.get("optimizer")
        .and_then(|optimizer| optimizer.get(group))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|map| keys.iter().any(|key| map.contains_key(*key)))
}

fn carries_drag_model(data: &serde_json::Value) -> bool {
    data.get("drag_model")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|map| {
            RETIRED_DRAG_MODEL_KEYS
                .iter()
                .any(|key| map.contains_key(*key))
        })
}

/// Which groups of retired optimizer keys a loaded document carried, so the
/// load can say what it dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetiredKeysDropped {
    /// `optimizer.solver` options nothing reads.
    pub solver: bool,
    /// `optimizer.weights` of the retired weighted objective.
    pub weights: bool,
    /// `optimizer.objective.max_span_m`, replaced by the aerodrome code.
    pub span: bool,
    /// `drag_model.korn_technology_factor`, replaced by the declared airfoil
    /// class.
    pub korn_technology_factor: bool,
    /// A stage budget saved when pre-gate rejections counted against
    /// `max_evaluations`; the same number now counts analysed candidates.
    pub requested_count_budget: bool,
    /// A replay count recorded under that meaning, which cannot be
    /// converted and was dropped.
    pub requested_count_replay: bool,
}

impl RetiredKeysDropped {
    pub(crate) fn of(data: &serde_json::Value) -> Self {
        Self {
            solver: carries(data, "solver", RETIRED_SOLVER_KEYS),
            weights: carries(data, "weights", RETIRED_WEIGHT_KEYS),
            span: carries(data, "objective", RETIRED_OBJECTIVE_KEYS),
            korn_technology_factor: carries_drag_model(data),
            requested_count_budget: requested_count_stages(data).next().is_some(),
            requested_count_replay: requested_count_stages(data)
                .any(|stage| stage.contains_key(REPLAY_KEY)),
        }
    }

    /// One sentence per dropped group.
    pub fn messages(&self) -> Vec<&'static str> {
        [
            (self.solver, "The saved solver options 'strategy', 'display_progress', 'seed_near_initial_design' and 'seed_perturbation_fraction' were dropped; the search does not read them."),
            (self.weights, "The saved optimizer weights of the retired weighted lift-to-drag objective were dropped; the mission objective does not read them."),
            (self.span, "The saved span limit 'max_span_m' was dropped; the aerodrome reference code letter now sets the span limit."),
            (self.korn_technology_factor, "The saved Korn technology factor 'drag_model.korn_technology_factor' was dropped; it now follows the wing's declared airfoil class (0.87 conventional, 0.95 supercritical)."),
            (self.requested_count_budget, "The saved stage evaluation budgets counted design-vector pre-gate rejections; the same numbers now count analysed candidates only, and rejections have their own cap (20 times the budget unless set)."),
            (self.requested_count_replay, "The saved replay evaluation counts were recorded when pre-gate rejections counted as evaluations and were dropped; they would not reproduce that run."),
        ]
        .into_iter()
        .filter_map(|(dropped, message)| dropped.then_some(message))
        .collect()
    }
}

/// `data` without the retired optimizer and drag-model keys; borrowed when it
/// carries none.
pub(crate) fn without_retired_optimizer_keys(
    data: &serde_json::Value,
) -> Cow<'_, serde_json::Value> {
    let stale_replay = requested_count_stages(data).any(|stage| stage.contains_key(REPLAY_KEY));
    if !stale_replay
        && !carries_drag_model(data)
        && !GROUPS
            .iter()
            .any(|(group, keys)| carries(data, group, keys))
    {
        return Cow::Borrowed(data);
    }
    let mut cleaned = data.clone();
    for (group, keys) in GROUPS {
        if let Some(map) = cleaned
            .pointer_mut(&format!("/optimizer/{group}"))
            .and_then(serde_json::Value::as_object_mut)
        {
            for key in keys {
                map.remove(*key);
            }
        }
    }
    if let Some(map) = cleaned
        .get_mut("drag_model")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in RETIRED_DRAG_MODEL_KEYS {
            map.remove(*key);
        }
    }
    for stage in SEARCH_STAGES {
        if let Some(map) = cleaned
            .pointer_mut(&format!("/optimizer/solver/{stage}"))
            .and_then(serde_json::Value::as_object_mut)
            .filter(|map| !map.contains_key(REJECT_CAP_KEY))
        {
            map.remove(REPLAY_KEY);
        }
    }
    Cow::Owned(cleaned)
}

/// The `optimizer.solver` stage blocks.
const SEARCH_STAGES: [&str; 2] = ["screening", "refinement"];
/// The stage key every file carries since budgets count analysed candidates.
const REJECT_CAP_KEY: &str = "max_pregate_rejects";
/// The stage replay count.
const REPLAY_KEY: &str = "replay_evaluations";

/// Stage blocks saved when pre-gate rejections counted against
/// `max_evaluations`: they state that budget or a replay count without
/// [`REJECT_CAP_KEY`], which a file saved since always carries.
fn requested_count_stages(
    data: &serde_json::Value,
) -> impl Iterator<Item = &serde_json::Map<String, serde_json::Value>> {
    SEARCH_STAGES.into_iter().filter_map(move |stage| {
        data.pointer(&format!("/optimizer/solver/{stage}"))
            .and_then(serde_json::Value::as_object)
            .filter(|map| {
                !map.contains_key(REJECT_CAP_KEY)
                    && (map.contains_key("max_evaluations") || map.contains_key(REPLAY_KEY))
            })
    })
}

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use crate::AlasConfig;

    #[test]
    fn a_saved_file_with_the_removed_span_and_tail_volume_keys_still_loads() {
        let (config, notes) = AlasConfig::from_value_with_notes(&serde_json::json!({
            "optimizer": {
                "objective": {"max_span_m": 65.0},
                "weights": {"min_hstab_volume_coef": 0.75, "max_vstab_volume_coef": 0.13},
                "solver": {"display_progress": false, "seed_perturbation_fraction": 0.1}
            }
        }))
        .unwrap();
        // Every dropped group is said, not silently lost.
        assert_eq!(
            notes.retired_keys,
            super::RetiredKeysDropped {
                solver: true,
                weights: true,
                span: true,
                ..Default::default()
            }
        );
        assert_eq!(notes.messages().len(), 3);
        // The removed span key is dropped, not honoured: the letter decides.
        assert_eq!(
            config.optimizer.objective,
            crate::ObjectiveConfig::default()
        );
    }

    #[test]
    fn a_budget_saved_before_rejections_had_their_own_cap_keeps_its_number_and_loses_its_replay() {
        let saved = serde_json::json!({
            "optimizer": {"solver": {
                "screening": {"max_evaluations": 900, "time_limit_s": 20.0, "replay_evaluations": 640},
                "refinement": {"max_evaluations": 300, "time_limit_s": 60.0}
            }}
        });
        let (config, notes) = AlasConfig::from_value_with_notes(&saved).unwrap();
        assert!(notes.retired_keys.requested_count_budget);
        assert!(notes.retired_keys.requested_count_replay);
        assert_eq!(notes.messages().len(), 2);
        let solver = &config.optimizer.solver;
        assert_eq!(solver.screening.max_evaluations, 900);
        assert_eq!(solver.screening.replay_evaluations, None);
        assert_eq!(solver.screening.resolved_max_pregate_rejects(), 18_000);
        // A file saved by this build carries the cap key and loads silently,
        // its replay count intact.
        let resaved = serde_json::to_value(&config).unwrap();
        let mut replayed = resaved.clone();
        replayed["optimizer"]["solver"]["screening"]["replay_evaluations"] = 640.into();
        let (reloaded, notes) = AlasConfig::from_value_with_notes(&replayed).unwrap();
        assert_eq!(notes.retired_keys, super::RetiredKeysDropped::default());
        assert_eq!(
            reloaded.optimizer.solver.screening.replay_evaluations,
            Some(640)
        );
        assert!(resaved["optimizer"]["solver"]["screening"]
            .get("max_pregate_rejects")
            .is_some());
    }
}
