// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Inputs page's constraint policy (clarified ledger D01-D03).
//!
//! The ledger asks for "a simple Inputs policy for explicitly allowing
//! limited violations when a user has overconstrained the problem", with the
//! per-limit detail in Advanced Settings. This is that control, and the one
//! thing it must not do is look like a switch that changes a run when it
//! does not.
//!
//! D02's engineering review, recorded in
//! `alas_config::optimizer::policy_review`, currently admits no limit at
//! all, so a policy switched on here still relaxes nothing. That is stated
//! in the card rather than discovered afterwards: the switch and the group
//! count are written into the configuration exactly as a user sets them, the
//! reviewed count is shown beside them, and the eligibility list itself
//! stays in Advanced Settings and in the saved document where a reviewed
//! entry would be added.

use alas_config::optimizer::policy_review::{eligible_count, reviewed_count};
use egui::{DragValue, RichText, Ui};
use serde_json::{json, Value};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

/// Discipline families a candidate can miss: Mass, Balance, Performance and
/// Geometry. The allowance cannot exceed them, because missing all four is
/// the same as having no requirements.
const DISCIPLINE_GROUP_COUNT: i64 = 4;

/// Render the constraint-policy controls inside the Run options card.
pub(crate) fn show_constraint_policy(state: &mut AppState, ui: &mut Ui) {
    ui.add_space(6.0);
    ui.label(RichText::new(tr("Constraint policy")).strong());

    let mut enabled = policy_enabled(state);
    if ui
        .checkbox(&mut enabled, tr("Allow limited constraint violations"))
        .on_hover_text(tr(
            "Off: every Mass, Balance, Performance and Geometry limit is hard, which is how every measurement in this product was taken. On: a bounded number of discipline groups may be missed, but only for limits an engineering review has declared eligible and only inside each limit's own tolerance.",
        ))
        .changed()
    {
        set_policy(state, "enabled", Value::Bool(enabled));
    }

    ui.add_enabled_ui(enabled, |ui| {
        let mut groups = allowed_groups(state);
        ui.horizontal(|ui| {
            ui.label(tr("Discipline groups that may be missed"));
            if ui
                .add(DragValue::new(&mut groups).range(0..=DISCIPLINE_GROUP_COUNT))
                .on_hover_text(tr(
                    "Counts groups, not limits: several eligible exceeded limits inside Mass count as one violated group. A candidate that exceeds this count is rejected exactly as it would be with the policy off.",
                ))
                .changed()
            {
                set_policy(state, "allowed_violated_groups", json!(groups));
            }
        });
    });

    ui.label(
        RichText::new(tr_fields(
            "{eligible} of {reviewed} limits are currently eligible for relaxation.",
            &[
                ("eligible", eligible_count().to_string()),
                ("reviewed", reviewed_count().to_string()),
            ],
        ))
        .weak()
        .small(),
    );
    if eligible_count() == 0 {
        // The honest consequence, stated where the switch is rather than
        // left for the user to infer from an unchanged result.
        ui.label(
            RichText::new(tr(
                "No limit has a tolerance with a traceable primary source yet, so this run stays strict whatever this switch is set to. A relaxed design would always be reported as relaxed and always rank behind every fully feasible one.",
            ))
            .weak()
            .small(),
        );
    }
}

/// Whether the edited configuration switches the policy on.
///
/// The group is omitted from a saved document while it holds the shipped
/// defaults, so an absent key is the strict policy rather than a missing
/// value to complain about.
fn policy_enabled(state: &AppState) -> bool {
    state
        .config_values
        .pointer("/optimizer/relaxation/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The configured allowance, clamped to the families that exist.
fn allowed_groups(state: &AppState) -> i64 {
    state
        .config_values
        .pointer("/optimizer/relaxation/allowed_violated_groups")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, DISCIPLINE_GROUP_COUNT)
}

/// Write one policy field, creating the group when the document omits it.
fn set_policy(state: &mut AppState, field: &str, value: Value) {
    let Some(optimizer) = state.group_mut("optimizer").and_then(Value::as_object_mut) else {
        return;
    };
    let relaxation = optimizer
        .entry("relaxation")
        .or_insert_with(|| json!({"enabled": false, "allowed_violated_groups": 0, "eligible": []}));
    if let Some(object) = relaxation.as_object_mut() {
        object.insert(field.to_owned(), value);
        // `deny_unknown_fields` means the group has to be complete once it
        // exists, and an older buffer may predate one of its keys.
        object.entry("enabled").or_insert(Value::Bool(false));
        object.entry("allowed_violated_groups").or_insert(json!(0));
        object.entry("eligible").or_insert(json!([]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_buffer_reads_as_the_strict_policy() {
        let state = AppState::default();
        assert!(!policy_enabled(&state));
        assert_eq!(allowed_groups(&state), 0);
    }

    #[test]
    fn switching_the_policy_on_writes_a_complete_group_the_config_can_parse() {
        let mut state = AppState::default();
        set_policy(&mut state, "enabled", Value::Bool(true));
        set_policy(&mut state, "allowed_violated_groups", json!(2));
        assert!(policy_enabled(&state));
        assert_eq!(allowed_groups(&state), 2);
        let config = state
            .typed_config()
            .expect("the edited buffer stays a readable configuration");
        assert!(config.optimizer.relaxation.enabled);
        assert_eq!(config.optimizer.relaxation.allowed_violated_groups, 2);
        // D02: switching it on still admits nothing, because the review
        // lists nothing. This is the property the card states in words.
        assert!(!config.optimizer.relaxation.is_active());
        assert_eq!(eligible_count(), 0);
    }

    #[test]
    fn the_control_cannot_ask_for_more_groups_than_there_are_families() {
        let mut state = AppState::default();
        set_policy(&mut state, "allowed_violated_groups", json!(97));
        assert_eq!(allowed_groups(&state), DISCIPLINE_GROUP_COUNT);
    }
}
