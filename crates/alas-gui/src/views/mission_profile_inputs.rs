// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Inputs-page mission profile editor.
//!
//! The profile is edited from the same mission.profile JSON node used by the
//! pipeline. The card provides a route-linked phase overview; selecting a
//! phase opens its actual speed/rate/altitude fields in a popup. The route
//! policy keeps a long-haul step-climb schedule from being copied onto short
//! routes and makes manual edits explicit.

use egui::{vec2, Context, Id, RichText, Ui, ViewportBuilder, ViewportClass, Window};
use serde_json::Value;

use alas_config::{airport_dataset, MissionProfileConfig};

use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::theme::card_frame;
use crate::views::{tr, tr_fields};

const SHORT_ROUTE_M: f64 = 2_500_000.0;
const MEDIUM_ROUTE_M: f64 = 7_000_000.0;

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
                show_cruise_legs_card(&mut columns[1], cruise_count);
            });
        } else {
            show_route_distance_card(ui, route_distance);
            ui.add_space(6.0);
            show_cruise_legs_card(ui, cruise_count);
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

/// Render phase-launch buttons in the detached Mission Advanced Settings tab.
/// The advanced tab owns the phase navigation, while the live profile and its
/// route cards remain on Setup > Inputs.
pub(crate) fn show_mission_profile_advanced(state: &mut AppState, ui: &mut Ui) {
    reconcile_route_profile(state);
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Mission phases")).strong());
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for phase in PHASES {
                let selected = state.mission_profile_window.open
                    && state.mission_profile_window.phase_id.as_deref() == Some(phase.id);
                if ui
                    .add(crate::theme::selectable_button(tr(phase.title), selected))
                    .clicked()
                {
                    open_phase_window(state, phase.id);
                }
            }
        });
    });
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

fn show_cruise_legs_card(ui: &mut Ui, cruise_count: usize) {
    card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Suggested cruise legs")).strong());
        ui.label(cruise_count.to_string());
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
    ui.heading(tr(phase.title));
    if matches!(class, ViewportClass::Embedded) {
        ui.label(RichText::new(tr("Detached editor fallback")).weak().small());
    }
    ui.label(
        RichText::new(tr(
            "These values are used by the mission solver in SI units where shown.",
        ))
        .weak()
        .small(),
    );
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
            state.help_verbose,
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

fn reconcile_route_profile(state: &mut AppState) {
    let signature = route_signature(&state.config_values);
    if state.mission_profile_route_signature.is_empty() {
        state.mission_profile_route_signature = signature;
        if let Some(config) = state.typed_config() {
            if config.mission.profile == MissionProfileConfig::default() {
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
    let cruise_altitude_m = state
        .config_values
        .pointer("/requirements/cruise_altitude_m")
        .and_then(Value::as_f64)
        .unwrap_or(11_887.2);
    let cruise_mach = state
        .config_values
        .pointer("/requirements/cruise_mach")
        .and_then(Value::as_f64)
        .unwrap_or(0.82);
    let mut count = if distance_m < SHORT_ROUTE_M {
        1
    } else if distance_m < MEDIUM_ROUTE_M {
        2
    } else {
        3
    };
    // A low-altitude/low-speed requirement has little step-climb capability;
    // retain one cruise level even if the route itself is long.
    if cruise_altitude_m < 8_000.0 || cruise_mach < 0.65 {
        count = count.min(1);
    }
    let fractions = match count {
        1 => [1.0, 0.0, 0.0],
        2 => [0.45, 0.55, 0.0],
        _ => [0.28169, 0.33803, 0.38028],
    };
    let Some(profile) = state.config_values.pointer_mut("/mission/profile") else {
        return;
    };
    let Some(object) = profile.as_object_mut() else {
        return;
    };
    for (name, value) in [
        ("cruise_1_distance_fraction", fractions[0]),
        ("cruise_2_distance_fraction", fractions[1]),
        ("cruise_3_distance_fraction", fractions[2]),
    ] {
        object.insert(name.to_owned(), Value::from(value));
    }
    object.insert(
        "initial_climb_altitude_fraction".to_owned(),
        Value::from(if count == 1 { 0.82 } else { 0.795 }),
    );
    object.insert(
        "step_climb_1_altitude_fraction".to_owned(),
        Value::from(0.90),
    );
    state.on_config_modified();
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
        active_cruise_count, open_phase_window, reconcile_route_profile, route_signature, PHASES,
        SHORT_ROUTE_M,
    };
    use alas_config::MissionProfileConfig;
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
    fn short_route_profile_has_one_active_cruise_leg_by_policy() {
        let profile = MissionProfileConfig {
            cruise_2_distance_fraction: 0.0,
            cruise_3_distance_fraction: 0.0,
            ..Default::default()
        };
        assert_eq!(active_cruise_count(&profile), 1);
        // Compile-time sanity bound: the short-route cutoff must stay under
        // 3,000 km, or the single-cruise-leg policy this test exercises no
        // longer matches its name.
        const { assert!(SHORT_ROUTE_M < 3_000_000.0) };
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

    #[test]
    fn inputs_overview_and_advanced_phase_launchers_stay_on_separate_surfaces() {
        let inputs = include_str!("inputs_view.rs");
        let advanced = include_str!("form_page_parts/part_01.rs");

        assert!(inputs.contains("show_mission_profile_inputs(state, ui)"));
        assert!(!inputs.contains("show_mission_profile_advanced(state, ui)"));
        assert!(advanced.contains("page.surface == Surface::Advanced"));
        assert!(advanced.contains("show_mission_profile_advanced(state, ui)"));
        assert!(!advanced.contains("show_mission_profile_inputs(state, ui)"));
    }
}
