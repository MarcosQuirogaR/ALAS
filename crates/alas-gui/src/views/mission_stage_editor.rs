// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Mission page's stage editor, on the mission plan.
//!
//! An empty plan is the standard mission, edited by the profile card below
//! this editor. Starting from a template (the standard sequence or one of
//! Raymer's four Fig. 3.2 profiles) or by adding a stage turns the mission
//! into an ordered stage list under the mission's one profile plot; the plot
//! is always drawn from the stage list the solver flies (for the standard
//! mission, the plan it is equivalent to). Selecting a stage highlights its
//! segment. Stages move by drag
//! on their handle or by Up and Down on the focused handle; the order is
//! validated by [`MissionPlan::move_stage`], so a refused order is reported
//! and leaves the plan alone. Events (mass, store, refuel, fuel and trim-tank
//! changes) belong to the stage above them: they travel with it and have no
//! handle of their own.

mod fields;
mod flight_view;
mod list;
mod model;
mod planned_flight;
mod plot;
mod plot_axis;
mod plot_hover;
mod plot_markers;
mod plot_style;
mod release;
mod standard;
mod standard_note;
pub(crate) use standard::flight_level;

use alas_config::{MissionPlan, RaymerProfile, TemplateInputs};
use egui::{vec2, Id, RichText, Rounding, Stroke, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::{edit_draft, tr_label};
#[cfg(test)]
use list::drop_target;
use list::{stage_detail, stages_column};

const PILL_SECONDS: f64 = 6.0;
/// Height of the page above the plot and below the card (title, toolbar, route row, margins), pt.
const PLOT_CHROME: f32 = 250.0;
/// Card width from which the stage list sits beside the plot, pt. The plot
/// keeps at least 70 % of the card there; below it the list goes under the
/// plot and the plot takes the whole card width.
const SPLIT_WIDTH: f32 = 1500.0;

/// What a confirmation prompt will do when accepted.
#[derive(Debug, Clone, PartialEq)]
enum Confirm {
    Template(Template),
    Clear,
}

/// A starting plan.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Template {
    Standard,
    Raymer(RaymerProfile),
}

impl Template {
    fn title(self) -> &'static str {
        match self {
            Self::Standard => "Standard Sequence",
            Self::Raymer(profile) => profile.title(),
        }
    }
}

/// Editor state kept in the egui context: it is a view of the plan, not part
/// of the case.
#[derive(Debug, Clone, Default)]
struct EditorUi {
    selected: Option<String>,
    status: Option<(String, bool, f64)>,
    confirm: Option<Confirm>,
    dragging: Option<usize>,
    /// How far below the top of its row the pointer took the dragged card.
    drag_grab: f32,
}

fn ui_id() -> Id {
    Id::new("mission_stage_editor_state")
}

fn say(editor: &mut EditorUi, ui: &Ui, text: String, ok: bool) {
    editor.status = Some((text, ok, ui.input(|input| input.time)));
}

pub(crate) fn pill(ui: &mut Ui, text: &str, ok: bool) {
    let color = if ok {
        crate::theme::success_color(ui.visuals())
    } else {
        ui.visuals().warn_fg_color
    };
    egui::Frame::none()
        .stroke(Stroke::new(1.0_f32, color))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            // A long message is cut at the card's edge; the whole of it is the
            // hover text.
            ui.add(egui::Label::new(RichText::new(text).color(color).small()).truncate())
                .on_hover_text(text)
        });
}

/// The plan a template builds for the current case.
fn template_plan(state: &AppState, template: Template) -> MissionPlan {
    let cruise_ft = model::cruise_altitude_ft(state);
    let config = state.typed_config_arc();
    let policy = config
        .as_ref()
        .map(|config| config.fuel_policy.clone())
        .unwrap_or_default();
    match template {
        Template::Standard => {
            let profile = config
                .as_ref()
                .map(|config| config.mission.profile.clone())
                .unwrap_or_default();
            MissionPlan::from_profile(&profile, cruise_ft)
        }
        Template::Raymer(profile) => {
            let mut inputs = TemplateInputs::from_policy(&policy, cruise_ft);
            if let Some(config) = config.as_ref() {
                inputs = inputs.with_profile(&config.mission.profile);
            }
            let mut plan = MissionPlan::raymer(profile, &inputs);
            plan.name = profile.title().to_owned();
            plan
        }
    }
}

/// Replace the stages of `plan` with those of `new`. The settings the
/// Payload page and the fuel policy own (Lock Layout, the balance target, the
/// take-off fuel and the contingency) belong to the case, not to a template,
/// so they are kept.
fn replace_stages(plan: &mut MissionPlan, mut new: MissionPlan) {
    new.lock_layout = plan.lock_layout;
    new.balance_target_pct_mac = plan.balance_target_pct_mac;
    new.takeoff_fuel_kg = plan.takeoff_fuel_kg;
    if new.contingency_fraction == 0.0 {
        new.contingency_fraction = plan.contingency_fraction;
    }
    *plan = new;
}

/// The Mission page's stage editor.
pub(crate) fn show(state: &mut AppState, ui: &mut Ui) {
    let mut editor: EditorUi = ui
        .ctx()
        .data_mut(|data| data.get_temp(ui_id()))
        .unwrap_or_default();
    let committed = model::read_plan(state);
    let draft_id = Id::new("mission_stage_editor_draft");
    let mut plan = edit_draft::begin(ui.ctx(), draft_id, committed.clone());
    let items = release::payload_items(state);
    let cruise_ft = model::cruise_altitude_ft(state);
    let fields_ft = model::field_elevations_ft(state);
    let mut newly_releasable = Vec::new();
    let mut open_phase = None;
    // A search jump to a profile field selects the stage that flies it, or
    // opens that phase's editor when the plan has no such stage.
    if let Some(focus) = crate::views::mission_phase_focus::focused(ui.ctx()) {
        let stage = crate::views::mission_phase_focus::stage_for_phase(&plan, focus.phase);
        match stage.filter(|_| plan.has_stages()) {
            Some(id) => editor.selected = Some(id),
            None => open_phase = Some(focus.phase),
        }
    }

    let route = crate::views::mission_profile_inputs::begin_route_row(state);
    // The standard mission is drawn from the schedule the pipeline builds for
    // the route, so the plot and the list show the legs that will be flown.
    let planned = flight_view::standard_flight(state, ui.ctx(), &plan);
    let ground_m = |reserve: bool| {
        planned.0.as_ref().map(|flight| {
            flight
                .legs
                .iter()
                .filter(|leg| reserve || !leg.reserve)
                .map(|leg| leg.ground_m.max(0.0))
                .sum::<f64>()
        })
    };
    let route = route
        .with_leg_note(planned.1.clone())
        .with_flown_m(ground_m(true))
        .with_trip_m(ground_m(false));
    let mut route_choice = None;
    let shared: &AppState = state;
    let ctx = ui.ctx().clone();
    let tanks = || model::tank_ids(shared, &ctx);
    crate::views::focus_card::card_for(ui, "Mission Profile", &["mission.profile"], |ui| {
        toolbar(shared, ui, &mut plan, &mut editor, cruise_ft);
        ui.add_space(4.0);
        route_choice = crate::views::mission_profile_inputs::route_row(ui, &route);
        ui.add_space(6.0);
        // One plot, fed by the stage list the solver flies: the plan, or for
        // the standard mission (no stages) the plan it is equivalent to.
        let standard;
        let flown = if plan.has_stages() {
            &plan
        } else if let Some(flight) = planned.0.as_ref() {
            &flight.plan
        } else {
            standard = template_plan(shared, Template::Standard);
            &standard
        };
        let segments = model::profile(flown, fields_ft[0], fields_ft[1], fields_ft[2]);
        // The nominal schematic is dimmed while the flight of the case is being computed.
        let dim = planned.0.as_ref().is_some_and(|flight| flight.computing);
        let selected = editor.selected.as_deref().and_then(|id| flown.position(id));
        if plan.has_stages() && selected.is_none() {
            editor.selected = None;
        }
        let texts = flown_ids(&plan, flown, planned.0.as_ref());
        let tips: Vec<String> = planned
            .0
            .as_ref()
            .map(|flight| flight.tips.clone())
            .unwrap_or_default();
        let ids: Vec<String> = flown.stages.iter().map(|stage| stage.id.clone()).collect();
        let mut clicked = None;
        let mut row_clicked = None;
        let side_by_side = ui.available_width() >= SPLIT_WIDTH;
        // The plot takes the height the page has between the card header and the foot, less
        // the list when it sits under the plot.
        let page = ui.clip_rect().height().min(ui.ctx().screen_rect().height());
        let under = list::rows_height(
            &plan,
            flown.stages.len(),
            ui.available_width(),
            list::row_pitch(ui),
        );
        let under = if side_by_side { 0.0 } else { under };
        let room = (page - PLOT_CHROME - under).clamp(plot::min_total(), plot::max_total());
        let facts = planned
            .0
            .as_ref()
            .map(|flight| plot_hover::facts_of(&flight.legs))
            .unwrap_or_default();
        // The plot names a phase on hover with its stage-list row.
        let strip_rows: Vec<String> = if texts.is_empty() {
            (0..flown.stages.len())
                .map(|index| list::row_text(flown, index))
                .collect()
        } else {
            texts.clone()
        };
        if side_by_side {
            let total = ui.available_width();
            let right = (total * 0.3).round();
            let left = total - right - 14.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.allocate_ui_with_layout(
                    vec2(left, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(left);
                        clicked = plot::show(
                            ui,
                            &plot::Inputs {
                                segments: &segments,
                                rows: &strip_rows,
                                facts: &facts,
                                selected,
                                estimate: dim,
                                room,
                            },
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(right, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(right);
                        row_clicked = stages_column(ui, &mut plan, &texts, &tips, &mut editor);
                    },
                );
            });
        } else {
            clicked = plot::show(
                ui,
                &plot::Inputs {
                    segments: &segments,
                    rows: &strip_rows,
                    facts: &facts,
                    selected,
                    estimate: dim,
                    room,
                },
            );
            ui.add_space(6.0);
            row_clicked = stages_column(ui, &mut plan, &texts, &tips, &mut editor);
        }
        if plan.has_stages() {
            let reveal = clicked.is_some() || row_clicked.is_some();
            if let Some(clicked) = clicked {
                editor.selected = plan.stages.get(clicked).map(|stage| stage.id.clone());
            }
            if let Some(index) = editor.selected.as_deref().and_then(|id| plan.position(id)) {
                ui.add_space(6.0);
                stage_detail(
                    ui,
                    &mut plan,
                    index,
                    &items,
                    &mut newly_releasable,
                    &tanks,
                    reveal,
                );
            }
        } else {
            open_phase = open_phase.or_else(|| {
                clicked.or(row_clicked).and_then(|index| {
                    planned
                        .0
                        .as_ref()
                        .and_then(|flight| flight.legs.get(index))
                        .and_then(|leg| leg.phase)
                        .or_else(|| {
                            ids.get(index)
                                .and_then(|id| flight_view::phase_of_standard_stage(id))
                        })
                })
            });
        }
    });
    crate::views::mission_profile_inputs::finish_route_row(state, ui, route_choice);

    if let Some(plan) = edit_draft::finish(ui.ctx(), draft_id, &committed, plan) {
        model::write_plan(state, &plan);
    }
    if !newly_releasable.is_empty() {
        release::mark_releasable(state, &newly_releasable);
    }
    if let Some(phase) = open_phase {
        crate::views::mission_profile_inputs::open_phase_window(state, phase);
    }
    ui.ctx().data_mut(|data| data.insert_temp(ui_id(), editor));
}

fn toolbar(
    state: &AppState,
    ui: &mut Ui,
    plan: &mut MissionPlan,
    editor: &mut EditorUi,
    cruise_ft: f64,
) {
    ui.horizontal_wrapped(|ui| {
        ui.menu_button(tr_label("Templates"), |ui| {
            let templates = std::iter::once(Template::Standard)
                .chain(RaymerProfile::ALL.into_iter().map(Template::Raymer));
            for template in templates {
                if ui.button(tr_label(template.title())).clicked() {
                    if plan.has_stages() {
                        editor.confirm = Some(Confirm::Template(template));
                    } else {
                        replace_stages(plan, template_plan(state, template));
                        say(editor, ui, tr_label("Template Applied"), true);
                    }
                    ui.close_menu();
                }
            }
        });
        ui.menu_button(tr_label("Add Stage"), |ui| {
            for kind in model::STAGE_KINDS {
                if ui.button(tr_label(kind)).clicked() {
                    // Adding to the standard mission starts from its stages.
                    if !plan.has_stages() {
                        replace_stages(plan, template_plan(state, Template::Standard));
                    }
                    add_stage(plan, editor, kind, cruise_ft);
                    ui.close_menu();
                }
            }
        });
        if plan.has_stages() && ui.button(tr_label("Clear Stages")).clicked() {
            editor.confirm = Some(Confirm::Clear);
        }
        if let Some(confirm) = editor.confirm.clone() {
            ui.label(tr_label("Replace Stages?"));
            if ui.button(tr_label("Replace")).clicked() {
                let message = match confirm {
                    Confirm::Template(template) => {
                        replace_stages(plan, template_plan(state, template));
                        "Template Applied"
                    }
                    Confirm::Clear => {
                        replace_stages(plan, MissionPlan::default());
                        "Stages Cleared"
                    }
                };
                editor.selected = None;
                editor.confirm = None;
                say(editor, ui, tr_label(message), true);
            }
            if ui.button(tr_label("Cancel")).clicked() {
                editor.confirm = None;
            }
        }
        status(ui, plan, editor);
    });
}

fn add_stage(plan: &mut MissionPlan, editor: &mut EditorUi, kind: &str, cruise_ft: f64) {
    let Some(mut stage) = model::new_stage(plan, kind, cruise_ft) else {
        return;
    };
    let at = model::insert_position(plan, &stage);
    // A stage added after a reserve is part of the reserve: the fuel it burns
    // is carried, not trip fuel.
    stage.reserve = at > 0 && plan.stages[at - 1].reserve;
    editor.selected = Some(stage.id.clone());
    plan.stages.insert(at, stage);
}

/// The status pill: a refused move, then the plan's validation, then the
/// last confirmation.
fn status(ui: &mut Ui, plan: &MissionPlan, editor: &EditorUi) {
    let now = ui.input(|input| input.time);
    if let Some((text, ok, since)) = &editor.status {
        if now - since < PILL_SECONDS {
            pill(ui, text, *ok);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }
    }
    if plan.has_stages() {
        match plan.validate() {
            Ok(()) => {}
            Err(error) => pill(ui, &error.to_string(), false),
        }
    }
}

/// The row texts of the stages the solver flies.
fn flown_ids(
    plan: &MissionPlan,
    flown: &MissionPlan,
    standard: Option<&standard::Standard>,
) -> Vec<String> {
    if plan.has_stages() {
        return Vec::new();
    }
    if let Some(standard) = standard {
        return standard.rows.clone();
    }
    (0..flown.stages.len())
        .map(|index| list::row_text(flown, index))
        .collect()
}

/// The 1-based stage that drops each declared point mass, by item id, for the
/// "released at stage N" chip of the Payload page.
pub(crate) fn release_stages(state: &AppState) -> std::collections::BTreeMap<String, usize> {
    let plan = model::read_plan(state);
    if !plan.has_stages() {
        return std::collections::BTreeMap::new();
    }
    release::release_stages(&plan, &release::payload_items(state))
}

/// Lock Layout and the balance target of the plan, which the payload editor
/// edits: the auto-balance never moves an item while the layout is locked.
pub(crate) fn layout_settings(state: &AppState) -> (bool, Option<f64>) {
    let plan = model::read_plan(state);
    (plan.lock_layout, plan.balance_target_pct_mac)
}

/// The plan settings a Payload Layout file carries.
pub(crate) fn capture_layout_settings(state: &AppState) -> Value {
    let (lock_layout, target) = layout_settings(state);
    serde_json::json!({"lock_layout": lock_layout, "balance_target_pct_mac": target})
}

/// Apply the settings of a Payload Layout file to a configuration value,
/// without the change handling (the caller's own follows).
pub(crate) fn store_layout_settings(config_values: &mut Value, settings: &Value) {
    let Some(plan) = config_values
        .pointer("/mission/plan")
        .and_then(|node| serde_json::from_value::<MissionPlan>(node.clone()).ok())
        .or_else(|| Some(MissionPlan::default()))
    else {
        return;
    };
    let mut plan = plan;
    plan.lock_layout = settings
        .get("lock_layout")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    plan.balance_target_pct_mac = settings
        .get("balance_target_pct_mac")
        .and_then(Value::as_f64);
    model::store_plan(config_values, &plan);
}

/// Store the payload editor's plan settings. Clearing both on a plan with no
/// stages removes the plan node, which is the standard mission.
pub(crate) fn set_layout_settings(state: &mut AppState, lock_layout: bool, target: Option<f64>) {
    let mut plan = model::read_plan(state);
    plan.lock_layout = lock_layout;
    plan.balance_target_pct_mac = target;
    model::write_plan(state, &plan);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_more;

#[cfg(test)]
mod flight_tests;

#[cfg(test)]
mod standard_tests;

#[cfg(test)]
mod plot_render_tests;
