// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the interface owes the user in words: why a configuration cannot be
//! run, and why a panel is not on screen.
//!
//! `alas_config::validate` already produces an exact sentence for every issue
//! it finds, naming both values and their units, and `AppState::blocked`
//! already gates the Run button on the error-severity ones. Until now nothing
//! rendered those sentences: an out-of-envelope cruise Mach number disabled
//! Run silently, and the only clue was an advanced field the page does not
//! show.
//!
//! This module is the one presentation boundary for those issues. It resolves
//! each `field_path` to its localized schema label, so the reader is told
//! which field to look at in the language the interface is in, and renders
//! the issue in the card that owns the group. Validation itself stays exactly
//! as strict as it was; nothing here relaxes, filters or reorders a finding.

use alas_config::{Entry, Node, Severity, ValidationIssue};
use egui::{Color32, RichText, Ui};

use crate::state::AppState;
use crate::views::{tr, tr_fields};

/// The localized label of the field a dotted `field_path` names, or the path
/// itself when the schema has no such field (an issue raised against a value
/// that is not a declared field, such as a whole-group rule).
pub fn localized_field_label(schema: &Node, field_path: &str, lang: Option<&str>) -> String {
    let mut node = schema;
    let mut label: Option<&'static str> = None;
    for segment in field_path.split('.') {
        let Some(field) = node.field(segment) else {
            return field_path.to_owned();
        };
        label = Some(field.label);
        match &field.entry {
            Entry::Node(child) => node = child,
            Entry::Leaf(_) => {}
        }
    }
    label
        .map(|label| alas_i18n::t(Some(label), lang).into_owned())
        .unwrap_or_else(|| field_path.to_owned())
}

/// One issue as a single readable line: which field, then what is wrong.
///
/// The framing is localized. The sentence `alas_config` produced is passed
/// through the catalog too, so a catalogued message is translated and an
/// uncatalogued one is shown verbatim rather than dropped.
pub fn issue_line(schema: &Node, issue: &ValidationIssue, lang: Option<&str>) -> String {
    tr_fields(
        "{field}: {message}",
        &[
            (
                "field",
                localized_field_label(schema, &issue.field_path, lang),
            ),
            ("message", tr(&issue.message)),
        ],
    )
}

/// The color an issue is rendered in.
fn severity_color(ui: &Ui, severity: Severity) -> Color32 {
    match severity {
        Severity::Error => ui.visuals().error_fg_color,
        Severity::Warning => ui.visuals().warn_fg_color,
    }
}

/// Every validation issue raised against `group`, most severe first.
fn group_issues<'a>(state: &'a AppState, group: &str) -> Vec<&'a ValidationIssue> {
    let prefix = format!("{group}.");
    let mut issues: Vec<&ValidationIssue> = state
        .validation_findings
        .iter()
        .filter(|issue| issue.field_path == group || issue.field_path.starts_with(&prefix))
        .collect();
    issues.sort_by_key(|issue| match issue.severity {
        Severity::Error => 0,
        Severity::Warning => 1,
    });
    issues
}

/// Render the validation issues of one configuration group inside the card
/// that edits it, above its fields.
///
/// An error-severity issue is the reason Run is disabled, so it is stated in
/// full: a user who typed an out-of-envelope Mach number reads why on the same
/// card, without hovering anything.
pub fn show_group_issues(state: &AppState, ui: &mut Ui, group: &str) {
    let issues = group_issues(state, group);
    if issues.is_empty() {
        return;
    }
    let lang = Some(state.language.code());
    let blocking = issues.iter().any(|issue| issue.severity == Severity::Error);
    if blocking {
        ui.label(
            RichText::new(tr("This design cannot be run until these are fixed:"))
                .strong()
                .color(ui.visuals().error_fg_color),
        );
    }
    for issue in issues {
        ui.label(
            RichText::new(issue_line(&state.schema, issue, lang))
                .color(severity_color(ui, issue.severity)),
        );
    }
    ui.add_space(6.0);
}

/// Say, in one line above the page, why the live preview is not on screen.
///
/// Without this the dock simply vanishes on a narrow window and the reader
/// has no way to tell a responsive layout decision from a lost panel.
pub fn show_preview_suppressed(ui: &mut Ui, suppressed: bool) {
    if !suppressed {
        return;
    }
    ui.label(
        RichText::new(tr(
            "The 3D live preview is hidden: this window is too narrow to show it beside a readable form. Widen the window to bring it back.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(6.0);
}

/// Every error-severity reason the Run button is disabled, one line each.
pub fn blocking_reasons(state: &AppState) -> Vec<String> {
    let lang = Some(state.language.code());
    state
        .validation_findings
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| issue_line(&state.schema, issue, lang))
        .collect()
}

/// The disabled Run button's explanation: the blocking reasons themselves,
/// not a generic instruction to go and find them.
pub fn run_blocked_hover_text(state: &AppState) -> String {
    let reasons = blocking_reasons(state);
    if reasons.is_empty() {
        return tr("Fix error-severity validation issues first");
    }
    format!(
        "{}\n{}",
        tr("Fix error-severity validation issues first"),
        reasons.join("\n")
    )
}

/// The compact, always-visible marker beside a disabled Run button.
pub fn show_run_blocked_marker(state: &AppState, ui: &mut Ui) {
    let reasons = blocking_reasons(state);
    if reasons.is_empty() {
        return;
    }
    let text = if reasons.len() == 1 {
        tr("1 blocking issue")
    } else {
        tr_fields(
            "{count} blocking issues",
            &[("count", reasons.len().to_string())],
        )
    };
    ui.label(RichText::new(text).color(ui.visuals().error_fg_color))
        .on_hover_text(reasons.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{AlasConfig, ConfigNode};

    #[test]
    fn a_dotted_path_resolves_to_the_localized_leaf_label() {
        let schema = AlasConfig::default().schema();
        assert_eq!(
            localized_field_label(&schema, "requirements.dive_speed_m_s", Some("en")),
            "Design dive speed (V_dive)"
        );
        alas_i18n::es::install();
        let spanish = localized_field_label(&schema, "requirements.dive_speed_m_s", Some("es"));
        assert!(
            spanish.starts_with("Velocidad de picado"),
            "the Spanish label must come from the catalog: {spanish}"
        );
    }

    #[test]
    fn an_unknown_path_falls_back_to_the_path_instead_of_vanishing() {
        let schema = AlasConfig::default().schema();
        assert_eq!(
            localized_field_label(&schema, "requirements.not_a_field", Some("en")),
            "requirements.not_a_field"
        );
    }

    #[test]
    fn an_out_of_envelope_cruise_mach_number_produces_a_readable_blocking_reason() {
        let mut state = AppState::default();
        state.config_values["requirements"]["cruise_mach"] = serde_json::Value::from(3.5);
        state.on_config_modified();
        assert!(state.blocked(), "Mach 3.5 must still block the run");

        let reasons = blocking_reasons(&state);
        assert!(!reasons.is_empty(), "a blocked run must state a reason");
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("dive speed") && reason.contains("3.50")),
            "the reason must name the field and the offending value: {reasons:?}"
        );
        assert!(
            run_blocked_hover_text(&state).contains("dive speed"),
            "the Run hover text must carry the reason"
        );
    }

    #[test]
    fn a_valid_configuration_reports_no_blocking_reason() {
        let state = AppState::default();
        assert!(!state.blocked());
        assert!(blocking_reasons(&state).is_empty());
    }

    #[test]
    fn group_issues_are_scoped_to_their_own_group_and_ordered_by_severity() {
        let mut state = AppState::default();
        state.config_values["requirements"]["cruise_mach"] = serde_json::Value::from(3.5);
        state.on_config_modified();
        let issues = group_issues(&state, "requirements");
        assert!(!issues.is_empty());
        assert!(issues
            .iter()
            .all(|issue| issue.field_path.starts_with("requirements.")));
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(group_issues(&state, "geometry").is_empty());
    }
}
