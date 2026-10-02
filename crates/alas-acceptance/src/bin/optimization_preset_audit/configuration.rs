// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a run's solver settings were arrived at: a registered preset or a
//! labelled search-effort experiment derived from one.

use std::io;

use alas_config::{solver_presets, SolverSettings};
use serde_json::{json, Value};

use super::Args;

/// How a run's solver settings were arrived at.
///
/// A registered preset and a hand-sized search effort are different claims
/// about a result, and a row that cannot tell them apart is how a reduced
/// budget gets read as `quick_draft`. Every artefact this harness writes
/// carries one of these two, and the experiment variant carries its label.
#[derive(Debug, Clone)]
pub(super) enum Configuration {
    /// Exactly the registered solver preset, unmodified.
    RegisteredPreset { name: String },
    /// The registered preset with its *numerical search effort* overridden
    /// under an explicit label.
    Experiment {
        label: String,
        derived_from: String,
        overrides: Vec<(&'static str, &'static str, Value, Value)>,
    },
}

impl Configuration {
    /// Apply the requested search-effort overrides to a registered preset's
    /// settings, rejecting an override that was not labelled.
    pub(super) fn resolve(args: &Args) -> io::Result<(Self, SolverSettings)> {
        let mut settings = resolve_settings(&args.solver_preset)?;
        let mut overrides = Vec::new();
        let count = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
        for (name, stage, evaluations, time_limit_s) in [
            (
                "refinement",
                &mut settings.refinement,
                args.max_evaluations,
                args.time_limit_s,
            ),
            (
                "screening",
                &mut settings.screening,
                args.screening_max_evaluations,
                args.screening_time_limit_s,
            ),
        ] {
            if let Some(value) = evaluations {
                let value = count(value);
                overrides.push((
                    name,
                    "max_evaluations",
                    json!(stage.max_evaluations),
                    json!(value),
                ));
                stage.max_evaluations = value;
            }
            if let Some(value) = time_limit_s {
                let value = value as f64;
                overrides.push((
                    name,
                    "time_limit_s",
                    json!(stage.time_limit_s),
                    json!(value),
                ));
                stage.time_limit_s = value;
            }
        }
        if let Some((screening, refinement, planned, restoration)) = args.replay_evaluations {
            for (name, stage, value) in [
                ("screening", &mut settings.screening, screening),
                ("refinement", &mut settings.refinement, refinement),
            ] {
                let value = Some(count(value));
                overrides.push((
                    name,
                    "replay_evaluations",
                    json!(stage.replay_evaluations),
                    json!(value),
                ));
                stage.replay_evaluations = value;
            }
            if let Some(planned) = planned.map(count) {
                let stage = &mut settings.refinement;
                overrides.push((
                    "refinement",
                    "replay_planned_evaluations",
                    json!(stage.replay_planned_evaluations),
                    json!(planned),
                ));
                stage.replay_planned_evaluations = Some(planned);
            }
            if let Some(restoration) = restoration.map(count) {
                let stage = &mut settings.refinement;
                overrides.push((
                    "refinement",
                    "replay_restoration_evaluations",
                    json!(stage.replay_restoration_evaluations),
                    json!(restoration),
                ));
                stage.replay_restoration_evaluations = Some(restoration);
            }
        }
        if args.stop_on_evaluations {
            overrides.push((
                "solver",
                "stop_on_evaluations_only",
                json!(false),
                json!(true),
            ));
            settings.stop_on_evaluations_only = true;
        }
        if let Some(workers) = args.workers {
            let workers = count(workers);
            overrides.push(("solver", "workers", json!(settings.workers), json!(workers)));
            settings.workers = workers;
        }
        settings
            .validate_budgets()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let Some(label) = args.experiment.as_ref() else {
            if overrides.is_empty() {
                return Ok((
                    Self::RegisteredPreset {
                        name: args.solver_preset.clone(),
                    },
                    settings,
                ));
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the stage budget flags change the registered preset's search effort and \
                 require an explicit --experiment <LABEL>, so the run cannot be read as the \
                 preset it was derived from",
            ));
        };
        // A label that collides with a registered preset name would reproduce
        // exactly the confusion the label exists to prevent.
        if solver_presets::get(label).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "--experiment label {label:?} is the name of a registered solver preset; \
                     choose a label that cannot be mistaken for one"
                ),
            ));
        }
        Ok((
            Self::Experiment {
                label: label.clone(),
                derived_from: args.solver_preset.clone(),
                overrides,
            },
            settings,
        ))
    }

    /// The token a row reports in `solver_preset`.
    ///
    /// An experiment reports its own label there, never the preset it was
    /// derived from: that field is what a reader compares runs by.
    pub(super) fn reported_name(&self) -> &str {
        match self {
            Self::RegisteredPreset { name } => name,
            Self::Experiment { label, .. } => label,
        }
    }

    /// The provenance block a row carries.
    pub(super) fn provenance(&self) -> Value {
        match self {
            Self::RegisteredPreset { name } => json!({
                "kind": "registered_solver_preset",
                "solver_preset": name,
                "search_effort_overrides": [],
            }),
            Self::Experiment {
                label,
                derived_from,
                overrides,
            } => json!({
                "kind": "experiment",
                "experiment_label": label,
                "derived_from_solver_preset": derived_from,
                "search_effort_overrides": overrides
                    .iter()
                    .map(|(stage, field, before, after)| json!({
                        "setting": format!("{stage}.{field}"),
                        "registered_value": before,
                        "experiment_value": after,
                    }))
                    .collect::<Vec<_>>(),
                "what_this_changes": "numerical search effort only: how many candidates each \
                                      search stage may evaluate and for how long",
                "what_this_does_not_change": "the registered aircraft preset, its requirements, \
                                              design mission, design space and bounds, every \
                                              physical constraint and limit, the reporting-fidelity \
                                              re-evaluation, and every feasibility clause in this \
                                              row. None of them is reachable from these flags.",
                "not_a_substitute_for": "the registered quick_draft/balanced/thorough presets, a \
                                         clean-sheet search, the reference baseline, or the \
                                         all-preset acceptance matrix",
            }),
        }
    }
}
pub(super) fn resolve_settings(name: &str) -> io::Result<SolverSettings> {
    solver_presets::get(name)
        .map(|preset| preset.settings.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
}
