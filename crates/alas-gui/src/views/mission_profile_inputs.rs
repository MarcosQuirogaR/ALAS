// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Inputs-page mission profile editor.
//!
//! The profile is edited from the same mission.profile JSON node used by the
//! pipeline. The card provides a route-linked phase overview; selecting a
//! phase opens its actual speed/rate/altitude fields in a popup. Its native
//! cruise-leg estimate fits step climbs to the route and makes manual edits
//! explicit.

use egui::{vec2, Context, Id, RichText, Ui, ViewportBuilder, ViewportClass, Window};
use serde_json::Value;

use alas_config::{airport_dataset, airports::Airport, AlasConfig, MissionProfileConfig};

use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::theme::card_frame;
use crate::views::{tr, tr_fields};

#[path = "mission_phase_launcher.rs"]
mod phase_launcher;
pub(crate) use phase_launcher::show_mission_profile_advanced;

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

#[derive(Clone, Copy)]
struct Phase {
    id: &'static str,
    title: &'static str,
    fields: &'static [&'static str],
}

const PHASES: &[Phase] = &[
    Phase {
        id: "takeoff",
        title: "Take-off",
        fields: &[
            "takeoff_altitude_gain_m",
            "takeoff_air_speed_m_s",
            "takeoff_climb_rate_m_s",
        ],
    },
    Phase {
        id: "initial_climb",
        title: "Initial climb",
        fields: &[
            "initial_climb_air_speed_m_s",
            "initial_climb_rate_m_s",
            "initial_climb_altitude_fraction",
        ],
    },
    Phase {
        id: "cruise_1",
        title: "Cruise leg 1",
        fields: &["cruise_1_air_speed_m_s", "cruise_1_distance_fraction"],
    },
    Phase {
        id: "step_climb_1",
        title: "Step climb 1",
        fields: &[
            "step_climb_1_air_speed_m_s",
            "step_climb_1_rate_m_s",
            "step_climb_1_altitude_fraction",
        ],
    },
    Phase {
        id: "cruise_2",
        title: "Cruise leg 2",
        fields: &["cruise_2_air_speed_m_s", "cruise_2_distance_fraction"],
    },
    Phase {
        id: "step_climb_2",
        title: "Step climb 2",
        fields: &["step_climb_2_air_speed_m_s", "step_climb_2_rate_m_s"],
    },
    Phase {
        id: "cruise_3",
        title: "Cruise leg 3",
        fields: &["cruise_3_air_speed_m_s", "cruise_3_distance_fraction"],
    },
    Phase {
        id: "descent_1",
        title: "Descent rung 1",
        fields: &[
            "descent_1_altitude_ft",
            "descent_1_air_speed_m_s",
            "descent_1_rate_m_s",
        ],
    },
    Phase {
        id: "descent_2",
        title: "Descent rung 2",
        fields: &[
            "descent_2_altitude_ft",
            "descent_2_air_speed_m_s",
            "descent_2_rate_m_s",
        ],
    },
    Phase {
        id: "descent_3",
        title: "Descent rung 3",
        fields: &[
            "descent_3_altitude_ft",
            "descent_3_air_speed_m_s",
            "descent_3_rate_m_s",
        ],
    },
    Phase {
        id: "descent_4",
        title: "Descent rung 4",
        fields: &[
            "descent_4_altitude_ft",
            "descent_4_air_speed_m_s",
            "descent_4_rate_m_s",
        ],
    },
    Phase {
        id: "final_approach",
        title: "Final approach",
        fields: &["landing_air_speed_m_s", "landing_descent_rate_m_s"],
    },
];

/// Render the route cards and interactive live profile on the Setup > Inputs
/// page. This is the only inline mission-profile view: clicking a phase opens
/// its detached editor, so the page stays a live overview rather than another
/// copy of the parameter form.
pub(crate) fn show_mission_profile_inputs(state: &mut AppState, ui: &mut Ui) {
    reconcile_route_profile(state);
    let route_distance = route_distance_m(&state.config_values);
    let profile = state
        .typed_config()
        .map(|config| config.mission.profile)
        .unwrap_or_default();
    let cruise_count = active_cruise_count(&profile);

    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Mission profile")).strong());
        ui.add_space(4.0);
        let card_width = ui.available_width();
        if card_width >= 560.0 {
            ui.columns(2, |columns| {
                show_route_distance_card(&mut columns[0], route_distance);
                show_cruise_legs_card(&mut columns[1], state, cruise_count);
            });
        } else {
            show_route_distance_card(ui, route_distance);
            ui.add_space(6.0);
            show_cruise_legs_card(ui, state, cruise_count);
        }
        ui.add_space(8.0);
        if let Some(config) = state.typed_config() {
            crate::views::mission_profile_preview::show_interactive_mission_profile_preview(
                state, ui, &config,
            );
        }
        if let Some(issue) = &state.mission_profile_retained_validation {
            ui.label(
                RichText::new(issue)
                    .color(ui.visuals().warn_fg_color)
                    .small(),
            );
        }
    });

    show_regeneration_prompt(state, ui);
}

fn show_route_distance_card(ui: &mut Ui, distance_m: Option<f64>) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Route distance")).strong());
        match distance_m {
            Some(distance_m) => ui.label(format!("{:.0} km", distance_m / 1000.0)),
            None => ui.label(
                RichText::new(tr("Unavailable until both airports resolve."))
                    .color(ui.visuals().warn_fg_color),
            ),
        };
    });
}

fn show_cruise_legs_card(ui: &mut Ui, state: &mut AppState, cruise_count: usize) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Cruise legs")).strong())
            .on_hover_text(tr(
                "Each additional cruise leg adds a step climb. The default is estimated from route cruise time and typical fuel-burn altitude drift; choose another count to edit the profile.",
            ));
        let mut selected = cruise_count.clamp(1, 3);
        ui.horizontal(|ui| {
            for count in 1..=3 {
                ui.radio_value(&mut selected, count, count.to_string());
            }
        });
        if selected != cruise_count && apply_cruise_leg_count(state, selected) {
            state.mission_profile_manual_edit = true;
            state.mission_profile_retained_validation = None;
            state.on_config_modified();
        }
    });
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
        .map(|phase| tr(phase.title))
        .unwrap_or_else(|| tr("Mission profile"));
    let response = show_native_viewport(
        ctx,
        "mission_profile_phase",
        format!("Mission profile: {phase_title}"),
        ViewportBuilder::default()
            .with_title(format!("Mission profile: {phase_title}"))
            .with_inner_size(vec2(560.0, 430.0))
            .with_min_inner_size(vec2(420.0, 300.0))
            .with_resizable(true),
        |_child_ctx, ui, class| {
            show_phase_editor(state, ui, &phase_id, class);
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
        edits = crate::views::form::dynamic_form(
            ui,
            &fields,
            profile,
            &std::collections::HashSet::new(),
            Some(state.language.code()),
            false,
        );
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
    let mut open = true;
    let mut decision = None;
    Window::new(tr("Mission profile changed with route"))
        .id(Id::new("mission_profile_regeneration_prompt"))
        .open(&mut open)
        .collapsible(false)
        .show(ui.ctx(), |ui| {
            ui.label(tr(
                "The route changed after a manual profile edit. Regenerate the route-linked suggestion or retain the edited phases and validate them below.",
            ));
            if issues.is_empty() {
                ui.label(
                    RichText::new(tr("The retained profile is valid for the new route.")).weak(),
                );
            } else {
                for issue in &issues {
                    ui.label(RichText::new(issue).color(ui.visuals().warn_fg_color));
                }
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Regenerate profile")).clicked() {
                    decision = Some(true);
                }
                if ui.button(tr("Retain edited profile")).clicked() {
                    decision = Some(false);
                }
            });
        });
    if let Some(regenerate) = decision {
        state.mission_profile_regeneration_prompt = false;
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
    } else if !open {
        state.mission_profile_regeneration_prompt = false;
        state.mission_profile_retained_validation = if issues.is_empty() {
            Some(tr("Retained profile validated for the new route."))
        } else {
            Some(issues.join(" "))
        };
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

/// Stable route identity used by the explicit regeneration policy.
pub(crate) fn route_signature(values: &Value) -> String {
    format!(
        "{}|{}",
        airport_signature(
            values
                .get("departure_airport")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        airport_signature(
            values
                .get("arrival_airport")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    )
}

/// Include the resolved physical endpoint in the route identity. A custom
/// airport can retain its ICAO/name while its coordinates or field climate
/// are edited; the profile policy must see that as a route change rather than
/// silently retaining a schedule derived from the old endpoint.
fn airport_signature(name: &str) -> String {
    let resolved = airport_dataset::resolve(name).ok().map(|airport| {
        (
            airport.icao.value,
            airport.latitude_deg.value,
            airport.longitude_deg.value,
            airport.elevation_m.value,
            airport.isa_deviation_c.value,
        )
    });
    format!("{name}|{resolved:?}")
}

fn route_distance_m(values: &Value) -> Option<f64> {
    let departure = values.get("departure_airport").and_then(Value::as_str)?;
    let arrival = values.get("arrival_airport").and_then(Value::as_str)?;
    let departure = airport_dataset::resolve(departure).ok()?;
    let arrival = airport_dataset::resolve(arrival).ok()?;
    let lat1 = departure.latitude_deg.value?.to_radians();
    let lat2 = arrival.latitude_deg.value?.to_radians();
    let dlat = lat2 - lat1;
    let lon1 = departure.longitude_deg.value?.to_radians();
    let lon2 = arrival.longitude_deg.value?.to_radians();
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    Some(2.0 * 6_371_000.0 * a.sqrt().atan2((1.0 - a).sqrt()))
}

/// Resolve route endpoints into the minimal airport values needed by
/// the shared route-aware mission proposal. Route distance and cruise-altitude
/// selection must use the same endpoint records as the Inputs card.
pub(crate) fn resolved_route_airports(config: &AlasConfig) -> Option<(Airport, Airport)> {
    fn resolve(name: &str) -> Option<Airport> {
        let airport = airport_dataset::resolve(name).ok()?;
        Some(Airport {
            name: airport.name.value.unwrap_or_else(|| name.to_owned()),
            icao: airport.icao.value.unwrap_or_default(),
            elevation_m: airport.elevation_m.value?,
            toda_m: airport.toda_m.value.unwrap_or_default(),
            lda_m: airport.lda_m.value.unwrap_or_default(),
            isa_deviation_c: airport.isa_deviation_c.value.unwrap_or_default(),
            notes: String::new(),
            latitude_deg: airport.latitude_deg.value?,
            longitude_deg: airport.longitude_deg.value?,
        })
    }

    Some((
        resolve(&config.departure_airport)?,
        resolve(&config.arrival_airport)?,
    ))
}

/// Whether the profile still matches a registered preset's unedited
/// operational schedule (or the generic default), so its route-based cruise
/// count may be initialized automatically.
pub(crate) fn is_automatic_profile(config: &AlasConfig) -> bool {
    config.mission.profile == MissionProfileConfig::default()
        || alas_config::presets::get(&config.preset)
            .ok()
            .is_some_and(|preset| {
                config.mission.profile == preset.operational_mission_defaults().profile
            })
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
    let Some(config) = state.typed_config() else {
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
