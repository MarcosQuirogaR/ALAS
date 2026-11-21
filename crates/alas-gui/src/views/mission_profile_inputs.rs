// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Inputs-page mission profile editor.
//!
//! The profile is edited from the same mission.profile JSON node used by the
//! pipeline. The card provides a route-linked phase overview; selecting a
//! phase opens its actual speed/rate/altitude fields in a popup. Its native
//! cruise-leg estimate fits step climbs to the route and makes manual edits
//! explicit.

use egui::{vec2, Context, RichText, Ui, ViewportBuilder, ViewportClass};
use serde_json::Value;

use alas_config::{airport_dataset, airports::Airport, AlasConfig, MissionProfileConfig};

use crate::native_viewport::show_compact_native_viewport;
use crate::state::AppState;
use crate::theme::card_frame;
use crate::views::{tr_fields, tr_label as tr};

const SHORT_ROUTE_M: f64 = 2_500_000.0;

/// State for the detached editor opened by clicking a phase in the live
/// mission figure. The phase remains selected while the native viewport is
/// resized or moved between monitors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MissionProfileWindow {
    /// Whether the native phase editor is open.
    pub open: bool,
    /// The solver phase currently shown in the editor.
    pub phase_id: Option<String>,
}

mod phases;
use phases::fit_to_content;
pub(crate) use phases::PHASES;
mod route;
pub(crate) use route::*;

/// What the Route row shows: the route distance and the cruise-leg count of
/// the standard mission, read before the row is drawn.
#[derive(Debug, Clone)]
pub(crate) struct RouteRow {
    distance_m: Option<f64>,
    cruise_count: usize,
    issue: Option<String>,
    leg_note: Option<String>,
    /// Total distance flown, m, with every loiter, diversion and reserve leg.
    flown_m: Option<f64>,
    /// Distance flown from take-off to landing, m, without the reserves.
    trip_m: Option<f64>,
    /// The route's own cruise altitude, m, when it is not the design one.
    route_cruise_m: Option<f64>,
}

/// Reconcile the profile with the route and read what the Route row shows.
pub(crate) fn begin_route_row(state: &mut AppState) -> RouteRow {
    reconcile_route_profile(state);
    let profile = state
        .typed_config_arc()
        .map(|config| config.mission.profile.clone())
        .unwrap_or_default();
    RouteRow {
        distance_m: route_distance_m(&state.config_values),
        cruise_count: active_cruise_count(&profile),
        issue: state.mission_profile_retained_validation.clone(),
        leg_note: None,
        flown_m: None,
        trip_m: None,
        route_cruise_m: route_cruise_override_m(state),
    }
}

impl RouteRow {
    /// This row with the total distance flown by the planned flight.
    pub(crate) fn with_flown_m(mut self, flown_m: Option<f64>) -> Self {
        self.flown_m = flown_m.filter(|m| m.is_finite() && *m > 0.0);
        self
    }

    /// This row with the distance flown from take-off to landing.
    pub(crate) fn with_trip_m(mut self, trip_m: Option<f64>) -> Self {
        self.trip_m = trip_m.filter(|m| m.is_finite() && *m > 0.0);
        self
    }

    /// This row with the note shown next to the cruise-leg radios.
    pub(crate) fn with_leg_note(mut self, note: Option<String>) -> Self {
        self.leg_note = note;
        self
    }
}

/// The cruise legs the profile sets (the radio value).
pub(crate) fn set_cruise_legs(config: &AlasConfig) -> usize {
    active_cruise_count(&config.mission.profile)
}

/// The compact Route row: the distance as text and the cruise legs as three
/// radios. Returns the leg count the user picked, if it changed.
pub(crate) fn route_row(ui: &mut Ui, row: &RouteRow) -> Option<usize> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Route")).strong());
        match row.distance_m {
            Some(distance_m) => ui
                .label(format!("{:.0} km", distance_m / 1000.0))
                .on_hover_text(tr("Airport to airport; it does not change with loiter or diversion phases.")),
            None => ui.label(
                RichText::new(tr("Unavailable until both airports resolve."))
                    .color(ui.visuals().warn_fg_color),
            ),
        };
        if let Some(trip_m) = row.trip_m.filter(|trip_m| {
            row.distance_m
                .is_some_and(|route_m| (trip_m - route_m).abs() > 0.02 * route_m)
        }) {
            ui.label(RichText::new(tr("Trip")).strong()).on_hover_text(tr(
                "Distance flown take-off to landing; the climbs and descents flown are longer than the planned ones.",
            ));
            ui.label(format!("{:.0} km", trip_m / 1000.0));
        }
        if let Some(flown_m) = row.flown_m {
            ui.label(RichText::new(tr("Flown")).strong()).on_hover_text(tr(
                "Total distance flown by the planned mission: trip, reserves, loiter and diversion.",
            ));
            ui.label(format!("{:.0} km", flown_m / 1000.0));
        }
        ui.separator();
        ui.label(tr("Cruise legs")).on_hover_text(tr(
            "Each additional cruise leg adds a step climb. The default is estimated from route cruise time and typical fuel-burn altitude drift; choose another count to edit the profile.",
        ));
        let mut selected = row.cruise_count.clamp(1, 3);
        for count in 1..=3 {
            ui.radio_value(&mut selected, count, count.to_string());
        }
        if selected != row.cruise_count {
            picked = Some(selected);
        }
        if let Some(cruise_m) = row.route_cruise_m {
            ui.separator();
            ui.label(RichText::new(tr_fields(
                "Cruise {level} (route default)",
                &[(
                    "level",
                    crate::views::mission_stage_editor::flight_level(cruise_m),
                )],
            )))
            .on_hover_text(tr(
                "The altitude this route flies by default; the design cruise altitude is set on the Aircraft page.",
            ));
        }
    });
    // One line is always reserved for the note, so the page below does not move when it comes and goes.
    ui.horizontal(|ui| {
        let text = row.leg_note.as_deref().or(row.issue.as_deref());
        let shown = text.unwrap_or(" ");
        let label = egui::Label::new(
            RichText::new(shown)
                .color(ui.visuals().warn_fg_color)
                .small(),
        )
        .wrap_mode(egui::TextWrapMode::Truncate);
        let response = ui.add(label);
        if let Some(text) = text {
            response.on_hover_text(text);
        }
    });
    picked
}

/// Apply the leg count picked in the Route row and show the regeneration
/// prompt when the route changed.
pub(crate) fn finish_route_row(state: &mut AppState, ui: &Ui, picked: Option<usize>) {
    if let Some(count) = picked {
        if apply_cruise_leg_count(state, count) {
            state.mission_profile_manual_edit = true;
            state.mission_profile_retained_validation = None;
            state.on_config_modified();
        }
    }
    show_regeneration_prompt(state, ui);
}

/// The Route row in a card of its own, for the legacy Inputs page.
pub(crate) fn show_mission_profile_inputs(state: &mut AppState, ui: &mut Ui) {
    let row = begin_route_row(state);
    let mut picked = None;
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        picked = route_row(ui, &row);
    });
    finish_route_row(state, ui, picked);
}

/// Open the native editor for a phase selected in the live preview.
pub(crate) fn open_phase_window(state: &mut AppState, phase_id: &str) {
    if PHASES.iter().any(|phase| phase.id == phase_id) {
        state.mission_profile_window.open = true;
        state.mission_profile_window.phase_id = Some(phase_id.to_owned());
    }
}

/// Render the detached native mission phase editor, if one is selected.
pub(crate) fn show_mission_profile_window(state: &mut AppState, ctx: &Context) {
    if !state.mission_profile_window.open {
        return;
    }
    let phase_id = state
        .mission_profile_window
        .phase_id
        .clone()
        .unwrap_or_else(|| "cruise_1".to_owned());
    let phase_title = PHASES
        .iter()
        .find(|phase| phase.id == phase_id)
        .map_or_else(|| tr("Mission profile"), |phase| tr(phase.title));
    let title = tr_fields("Mission profile: {phase}", &[("phase", phase_title)]);
    // The window is as large as its fields: one grid column per field (three at
    // most) wide, and the height of what is drawn, fitted after the first frame.
    let field_count = PHASES
        .iter()
        .find(|phase| phase.id == phase_id)
        .map_or(3, |phase| phase.fields.len());
    let columns = field_count.clamp(1, 3) as f32;
    let width = (columns * 300.0 + 60.0).max(420.0);
    let response = show_compact_native_viewport(
        ctx,
        "mission_profile_phase",
        title.clone(),
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(vec2(width, 200.0))
            .with_min_inner_size(vec2(420.0, 120.0))
            .with_resizable(true),
        |child_ctx, ui, class| {
            let frame = child_ctx.screen_rect().size() - ui.max_rect().size();
            show_phase_editor(state, ui, &phase_id, class);
            if matches!(class, ViewportClass::Immediate) {
                fit_to_content(child_ctx, &phase_id, frame + ui.min_rect().size());
            }
        },
    );
    if response.close_requested {
        state.mission_profile_window = MissionProfileWindow::default();
    }
}

fn show_phase_editor(state: &mut AppState, ui: &mut Ui, phase_id: &str, class: ViewportClass) {
    let Some(phase) = PHASES.iter().find(|phase| phase.id == phase_id).copied() else {
        return;
    };
    ui.heading(tr(phase.title)).on_hover_text(tr(
        "These values are used by the mission solver in SI units where shown.",
    ));
    if matches!(class, ViewportClass::Embedded) {
        ui.label(RichText::new(tr("Detached editor fallback")).weak().small());
    }
    ui.add_space(6.0);
    let fields = profile_fields(&state.schema)
        .into_iter()
        .filter(|field| phase.fields.contains(&field.name))
        .collect::<Vec<_>>();
    let mut edits = Vec::new();
    if let Some(profile) = state.config_values.pointer_mut("/mission/profile") {
        edits = crate::views::form::with_baseline_group("mission", || {
            crate::views::form::dynamic_form_at(
                ui,
                &fields,
                profile,
                &Default::default(),
                Some(state.language.code()),
                "::profile",
            )
        });
    }
    if !edits.is_empty() {
        state.mission_profile_manual_edit = true;
        state.mission_profile_retained_validation = None;
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}

fn show_regeneration_prompt(state: &mut AppState, ui: &Ui) {
    if !state.mission_profile_regeneration_prompt {
        return;
    }
    let distance = route_distance_m(&state.config_values);
    let issues = profile_issues(state, distance);
    use crate::shell_confirm::{confirm_dialog, ConfirmAction, ConfirmAnswer};
    let mut body = tr(
        "The route changed after a manual profile edit. Regenerate the route-linked suggestion or retain the edited phases and validate them below.",
    );
    if issues.is_empty() {
        body.push_str(
            "

",
        );
        body.push_str(&tr("The retained profile is valid for the new route."));
    } else {
        for issue in &issues {
            body.push_str(
                "

",
            );
            body.push_str(issue);
        }
    }
    // Some(Some(true)): regenerate; Some(Some(false)): keep the edited
    // phases; Some(None): cancel (also Escape).
    let decision = match confirm_dialog(
        ui.ctx(),
        "mission_profile_regeneration_block",
        &tr("Mission profile changed with route"),
        Some(&body),
        &[
            ConfirmAction::primary(tr("Regenerate profile")),
            ConfirmAction::plain(tr("Keep edited profile")),
        ],
    ) {
        Some(ConfirmAnswer::Action(0)) => Some(Some(true)),
        Some(ConfirmAnswer::Action(_)) => Some(Some(false)),
        Some(ConfirmAnswer::Cancel) => Some(None),
        None => None,
    };
    if let Some(choice) = decision {
        state.mission_profile_regeneration_prompt = false;
        // Cancel dismisses the prompt and leaves the edited profile as it is.
        let Some(regenerate) = choice else {
            return;
        };
        if regenerate {
            state.mission_profile_manual_edit = false;
            state.mission_profile_retained_validation = None;
            if let Some(distance_m) = distance {
                regenerate_profile(state, distance_m);
            }
        } else {
            state.mission_profile_retained_validation = if issues.is_empty() {
                Some(tr("Retained profile validated for the new route."))
            } else {
                Some(issues.join(" "))
            };
        }
    }
}

fn profile_fields(schema: &alas_config::Node) -> Vec<alas_config::Field> {
    let Some(mission) = schema.field("mission") else {
        return Vec::new();
    };
    let alas_config::Entry::Node(mission) = &mission.entry else {
        return Vec::new();
    };
    let Some(profile) = mission.field("profile") else {
        return Vec::new();
    };
    let alas_config::Entry::Node(profile) = &profile.entry else {
        return Vec::new();
    };
    profile.fields.clone()
}

fn route_suggested_cruise_count(state: &AppState, route_distance_m: f64) -> Option<usize> {
    let config = state.typed_config()?;
    let (origin, destination) = resolved_route_airports(&config)?;
    let proposal =
        alas_mission::propose_profile_for_route(&config, &origin, &destination, route_distance_m)
            .ok()?;
    Some(proposal.active_cruise_legs)
}

fn apply_cruise_leg_count(state: &mut AppState, count: usize) -> bool {
    let config = state.typed_config().unwrap_or_default();
    let (cruise_altitude_m, departure_elevation_m) = resolved_route_airports(&config)
        .map(|(origin, destination)| {
            (
                alas_mission::route_cruise_altitude_m(&config, &origin, &destination),
                origin.elevation_m,
            )
        })
        .unwrap_or((config.requirements.cruise_altitude_m, 0.0));
    let mut updated_profile = config.mission.profile;
    alas_mission::configure_cruise_legs(
        &mut updated_profile,
        count,
        cruise_altitude_m,
        departure_elevation_m,
    );
    let Some(profile) = state
        .config_values
        .pointer_mut("/mission/profile")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };

    let fields = [
        (
            "cruise_1_distance_fraction",
            updated_profile.cruise_1_distance_fraction,
        ),
        (
            "cruise_2_distance_fraction",
            updated_profile.cruise_2_distance_fraction,
        ),
        (
            "cruise_3_distance_fraction",
            updated_profile.cruise_3_distance_fraction,
        ),
        (
            "initial_climb_altitude_fraction",
            updated_profile.initial_climb_altitude_fraction,
        ),
        (
            "step_climb_1_altitude_fraction",
            updated_profile.step_climb_1_altitude_fraction,
        ),
    ];
    let changed = fields
        .iter()
        .any(|(name, value)| profile.get(*name).and_then(Value::as_f64) != Some(*value));
    if changed {
        for (name, value) in fields {
            profile.insert(name.to_owned(), Value::from(value));
        }
    }
    changed
}

/// Apply the route-aware count when a preset or untouched route profile is
/// loaded. The cruise altitude ladder is brought into line with that count;
/// aircraft-specific phase speeds and rates remain the preset's own inputs.
pub(crate) fn initialize_route_profile(state: &mut AppState) {
    state.mission_profile_manual_edit = false;
    state.mission_profile_regeneration_prompt = false;
    state.mission_profile_retained_validation = None;
    if let Some(distance_m) = route_distance_m(&state.config_values) {
        if let Some(count) = route_suggested_cruise_count(state, distance_m) {
            apply_cruise_leg_count(state, count);
        }
    }
    state.mission_profile_route_signature = route_signature(&state.config_values);
}

fn reconcile_route_profile(state: &mut AppState) {
    let signature = route_signature(&state.config_values);
    if state.mission_profile_route_signature.is_empty() {
        state.mission_profile_route_signature = signature;
        if let Some(config) = state.typed_config() {
            if is_automatic_profile(&config) {
                state.mission_profile_manual_edit = false;
                if let Some(distance_m) = route_distance_m(&state.config_values) {
                    regenerate_profile(state, distance_m);
                }
            } else {
                // A non-default profile may have come from a loaded case or
                // an Advanced Settings edit before this card was first
                // rendered. Preserve it and require an explicit decision on
                // the next route change instead of classifying it as an
                // automatic suggestion after the fact.
                state.mission_profile_manual_edit = true;
            }
        }
        return;
    }
    if signature == state.mission_profile_route_signature {
        return;
    }
    state.mission_profile_route_signature = signature;
    let Some(distance_m) = route_distance_m(&state.config_values) else {
        return;
    };
    if state.mission_profile_manual_edit {
        state.mission_profile_regeneration_prompt = true;
        return;
    }
    regenerate_profile(state, distance_m);
}

fn regenerate_profile(state: &mut AppState, distance_m: f64) {
    let Some(count) = route_suggested_cruise_count(state, distance_m) else {
        return;
    };
    if apply_cruise_leg_count(state, count) {
        state.mission_profile_manual_edit = false;
        state.mission_profile_retained_validation = None;
        state.on_config_modified();
    }
}

fn active_cruise_count(profile: &MissionProfileConfig) -> usize {
    [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ]
    .iter()
    .rposition(|fraction| *fraction > 1.0e-6)
    .map_or(1, |index| index + 1)
}

fn profile_issues(state: &AppState, distance_m: Option<f64>) -> Vec<String> {
    let Some(config) = state.typed_config_arc() else {
        return vec![tr("Mission configuration is invalid.")];
    };
    let profile = &config.mission.profile;
    let mut issues = Vec::new();
    let fractions = [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ];
    let active = active_cruise_count(profile);
    let sum: f64 = fractions[..active].iter().sum();
    if !sum.is_finite() || sum <= 0.0 {
        issues.push(tr(
            "Cruise-leg fractions must contain a positive finite share.",
        ));
    } else if (sum - 1.0).abs() > 0.05 {
        issues.push(tr_fields(
            "Active cruise-leg fractions sum to {sum}; the solver will normalize them.",
            &[("sum", format!("{sum:.3}"))],
        ));
    }
    if distance_m.is_some_and(|distance| distance < SHORT_ROUTE_M && active > 1) {
        issues.push(tr(
            "This short route retains multiple cruise legs/step climbs; verify that the schedule is intentional.",
        ));
    }
    if profile.step_climb_1_altitude_fraction <= profile.initial_climb_altitude_fraction
        && active > 1
    {
        issues.push(tr("Step-climb 1 must be above the initial-climb level."));
    }
    issues
}

/// The route's own cruise altitude, m, when it is not the design one the Aircraft page sets.
fn route_cruise_override_m(state: &AppState) -> Option<f64> {
    let config = state.typed_config_arc()?;
    let (origin, destination) = resolved_route_airports(&config)?;
    let route_m = alas_mission::route_cruise_altitude_m(&config, &origin, &destination);
    ((route_m - config.requirements.cruise_altitude_m).abs() > 1.0).then_some(route_m)
}

#[cfg(test)]
mod tests {
    use super::{
        active_cruise_count, apply_cruise_leg_count, open_phase_window, reconcile_route_profile,
        route_signature, PHASES,
    };
    use alas_config::{AlasConfig, MissionProfileConfig};
    use serde_json::json;

    #[test]
    fn route_signature_tracks_only_the_selected_endpoints() {
        assert_eq!(
            route_signature(&json!({
                "departure_airport": "A",
                "arrival_airport": "B"
            })),
            "A|None|B|None"
        );
    }

    #[test]
    fn route_signature_contains_resolved_endpoint_facts() {
        let signature = route_signature(&json!({
            "departure_airport": "EGLL",
            "arrival_airport": "LEMD"
        }));
        assert!(signature.contains("51.47"));
        assert!(signature.contains("40.4719"));
    }

    #[test]
    fn loaded_non_default_profile_is_preserved_until_route_decision() {
        let mut state = crate::state::AppState::default();
        state.config_values["departure_airport"] = json!("EGLL");
        state.config_values["arrival_airport"] = json!("LEMD");
        let profile = MissionProfileConfig {
            cruise_1_distance_fraction: 0.8,
            cruise_2_distance_fraction: 0.2,
            ..Default::default()
        };
        state.config_values["mission"]["profile"] =
            serde_json::to_value(profile).expect("profile serializes");
        state.mission_profile_manual_edit = false;
        state.mission_profile_route_signature.clear();

        reconcile_route_profile(&mut state);
        assert!(state.mission_profile_manual_edit);

        state.config_values["arrival_airport"] = json!("KDEN");
        reconcile_route_profile(&mut state);
        assert!(state.mission_profile_regeneration_prompt);
    }

    #[test]
    fn one_leg_profile_has_one_active_cruise_leg() {
        let profile = MissionProfileConfig {
            cruise_2_distance_fraction: 0.0,
            cruise_3_distance_fraction: 0.0,
            ..Default::default()
        };
        assert_eq!(active_cruise_count(&profile), 1);
    }

    #[test]
    fn selecting_cruise_legs_updates_the_distance_split_and_altitude_ladder() {
        let config = AlasConfig::from_value(&json!({"preset": "A320-200"}))
            .expect("the registered A320-200 loads");
        let mut state = crate::state::AppState {
            config_values: serde_json::to_value(config).expect("configuration serializes"),
            ..Default::default()
        };

        assert!(apply_cruise_leg_count(&mut state, 2));
        let profile: MissionProfileConfig = serde_json::from_value(
            state
                .config_values
                .pointer("/mission/profile")
                .cloned()
                .expect("mission profile exists"),
        )
        .expect("updated mission profile decodes");
        assert_eq!(active_cruise_count(&profile), 2);
        assert_eq!(profile.cruise_3_distance_fraction, 0.0);
        let declared_altitude_m = 28_000.0 * 0.3048;
        let initial_level_m = declared_altitude_m * profile.initial_climb_altitude_fraction;
        let step_level_m = declared_altitude_m * profile.step_climb_1_altitude_fraction;
        assert!((initial_level_m - (declared_altitude_m - 2000.0 * 0.3048)).abs() < 2.0);
        assert!((step_level_m - declared_altitude_m).abs() < 2.0);
    }

    #[test]
    fn advanced_phase_launcher_maps_each_declared_phase_to_the_detached_editor() {
        assert_eq!(PHASES.len(), 12);
        assert_eq!(PHASES.first().map(|phase| phase.id), Some("takeoff"));
        assert_eq!(PHASES.last().map(|phase| phase.id), Some("final_approach"));
        assert!(PHASES.iter().all(|phase| !phase.fields.is_empty()));

        let mut state = crate::state::AppState::default();
        open_phase_window(&mut state, "takeoff");
        assert!(state.mission_profile_window.open);
        assert_eq!(
            state.mission_profile_window.phase_id.as_deref(),
            Some("takeoff")
        );

        open_phase_window(&mut state, "not-a-mission-phase");
        assert_eq!(
            state.mission_profile_window.phase_id.as_deref(),
            Some("takeoff")
        );
    }
}

#[cfg(test)]
mod phase_window_tests {
    use super::*;
    use crate::test_raster::{render, write_in};

    /// The window of a phase at the size it asks for holds its fields on one
    /// row with no empty body below.
    #[test]
    fn the_phase_editor_fills_a_window_sized_to_it() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        for id in ["descent_1", "cruise_1"] {
            let Some(phase) = PHASES.iter().find(|phase| phase.id == id) else {
                continue;
            };
            let columns = phase.fields.len().clamp(1, 3) as f32;
            let width = (columns * 300.0 + 60.0).max(420.0);
            let shot = render(width, 260.0, 4, 0, |ui| {
                ui.set_width(width - 16.0);
                show_phase_editor(&mut state, ui, id, ViewportClass::Immediate);
            });
            write_in(
                &crate::test_raster::render_dir(),
                &format!("phase-window-{id}.png"),
                &shot,
            );
        }
    }
}
