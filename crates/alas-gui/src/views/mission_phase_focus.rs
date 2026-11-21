// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a search jump to a `mission.profile.*` field reaches the Mission page.
//!
//! These fields are edited in two places: a phase editor window (the standard
//! mission) and the stage editor (a mission plan with stages, where the profile
//! is flown from the stages). A jump therefore selects the stage of the
//! matching kind when the plan has stages, and opens the phase window of the
//! field otherwise; the control itself pulses through `search::focus_anchor`.

use alas_config::MissionPlan;

use crate::views::mission_profile_inputs::PHASES;

/// The solver phase whose editor window holds the profile field `name`.
pub(crate) fn phase_of_field(name: &str) -> Option<&'static str> {
    PHASES
        .iter()
        .find(|phase| phase.fields.contains(&name))
        .map(|phase| phase.id)
}

/// The stage kind (its `StageKind::label`) that flies a phase.
pub(crate) fn stage_label_of_phase(phase: &str) -> &'static str {
    match phase {
        "takeoff" => "Takeoff",
        "initial_climb" | "step_climb_1" | "step_climb_2" => "Climb",
        "final_approach" => "Landing",
        p if p.starts_with("cruise") => "Cruise",
        _ => "Descent",
    }
}

/// A profile field the search is highlighting right now.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhaseFocus {
    /// Dotted path, `mission.profile.landing_air_speed_m_s`.
    pub path: String,
    /// The phase editor that holds it.
    pub phase: &'static str,
}

/// The profile field being highlighted while the jump is fresh.
pub(crate) fn focused(ctx: &egui::Context) -> Option<PhaseFocus> {
    let focus = crate::search::published_focus(ctx).filter(|focus| focus.fresh)?;
    let name = focus.path.strip_prefix("mission.profile.")?;
    Some(PhaseFocus {
        phase: phase_of_field(name)?,
        path: focus.path,
    })
}

/// The id of the first stage of `plan` that flies `phase`.
pub(crate) fn stage_for_phase(plan: &MissionPlan, phase: &str) -> Option<String> {
    let label = stage_label_of_phase(phase);
    plan.stages
        .iter()
        .find(|stage| stage.kind.label() == label)
        .map(|stage| stage.id.clone())
}
