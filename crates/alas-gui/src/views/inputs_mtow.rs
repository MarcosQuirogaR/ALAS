// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The "Maximum take-off mass" card of the Setup > Inputs page.
//!
//! The card owns `requirements.mtow_kg` and the four objective fields that
//! select and parameterize how the take-off mass is settled:
//! `optimizer.objective.{mtow_sizing, mtow_target_kg, mtow_band_fraction,
//! design_range_nmi}`. Masses are kilograms, ranges nautical miles (the
//! schema's display units); the band half-width is edited in percent and
//! stored as a fraction of the target.
//!
//! The mode choice only means something when the optimizer runs, so the
//! selector is enabled only then. With the optimizer off the declared MTOW is
//! the hard limit of a fixed-aircraft analysis whatever mode is stored.

use alas_config::{AlasConfig, DesignRange, Entry, Field, MtowSizing};
use alas_pipeline::quick_analysis::band::{
    design_mission_band_check, BandStatus, DesignMissionBand,
};
use egui::{ComboBox, DragValue, Id, RichText, Ui};
use serde_json::{json, Value};

use crate::state::AppState;
use crate::views::form::dynamic_form;
use crate::views::{tr, tr_fields};

const NMI_M: f64 = 1852.0;

/// Fields the Advanced optimizer page must not repeat, as dotted paths under
/// the optimizer group.
pub(crate) const RELOCATED_OBJECTIVE_PATHS: &[&str] = &[
    "objective.mtow_sizing",
    "objective.mtow_target_kg",
    "objective.mtow_band_fraction",
    "objective.design_range_nmi",
];

const HARD_HELP: &str =
    "The maximum take-off mass is a fixed limit and the mission must fit under it.";

/// The three modes a person can pick.
const CHOICES: [(MtowSizing, &str); 3] = [
    (MtowSizing::FixedRequirement, "Hard MTOW constraint"),
    (MtowSizing::MtowBand, "MTOW objective (band)"),
    (MtowSizing::PayloadAdjusted, "Payload-adjusted MTOW"),
];

/// Whether the optimizer will run, which is what makes the mode meaningful.
pub(crate) fn optimize_active(state: &AppState) -> bool {
    state.run_options.optimize && state.design_mode() != alas_config::DesignMode::BaselineSandbox
}

/// The stored mode; a document that omits it reads as the legacy default.
pub(crate) fn stored_mode(state: &AppState) -> MtowSizing {
    let name = state
        .config_values
        .pointer("/optimizer/objective/mtow_sizing")
        .and_then(Value::as_str);
    MtowSizing::ALL
        .into_iter()
        .find(|mode| Some(mode.as_str()) == name)
        .unwrap_or_default()
}

fn legacy_label(mode: MtowSizing, stored: bool) -> Option<&'static str> {
    match mode {
        MtowSizing::SizedByMission if stored => Some("Sized by mission (legacy)"),
        MtowSizing::SizedByMission => Some("Sized by mission (default)"),
        MtowSizing::Unconstrained => Some("Unconstrained (calibration)"),
        _ => None,
    }
}

/// Whether the document carries the mode key, as opposed to reading the
/// schema default.
fn mode_is_stored(state: &AppState) -> bool {
    state
        .config_values
        .pointer("/optimizer/objective/mtow_sizing")
        .is_some()
}

fn mode_label(mode: MtowSizing, stored: bool) -> &'static str {
    CHOICES
        .iter()
        .find(|(candidate, _)| *candidate == mode)
        .map(|(_, label)| *label)
        .or_else(|| legacy_label(mode, stored))
        .unwrap_or("Hard MTOW constraint")
}

fn objective_number(state: &AppState, key: &str) -> Option<f64> {
    state
        .config_values
        .pointer(&format!("/optimizer/objective/{key}"))
        .and_then(Value::as_f64)
}

/// Write one objective field and refresh everything derived from the config.
pub(crate) fn set_objective(state: &mut AppState, key: &str, value: Value, label: &str) {
    let shown = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    // The objective group is omitted from the document while it is all
    // defaults, and its own serde defaults fill whatever an edit leaves out.
    let Some(objective) = state
        .group_mut("optimizer")
        .and_then(Value::as_object_mut)
        .map(|group| group.entry("objective").or_insert_with(|| json!({})))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    objective.insert(key.to_owned(), value);
    state.on_config_modified();
    state.note_parameter_modified(tr(label), shown);
}

fn requirements_field(state: &AppState, name: &str) -> Option<Field> {
    match &state.schema.field("requirements")?.entry {
        Entry::Node(node) => node.fields.iter().find(|field| field.name == name).cloned(),
        Entry::Leaf(_) => None,
    }
}

/// The declared MTOW field, rendered by the schema form so its unit and
/// bounds come from the schema; `help` states what the mode does with it.
fn show_declared_mtow(state: &mut AppState, ui: &mut Ui, help: &'static str) {
    let Some(mut field) = requirements_field(state, "mtow_kg") else {
        return;
    };
    field.help = help;
    let errors = state
        .validation_findings
        .iter()
        .filter(|finding| finding.field_path.ends_with("mtow_kg"))
        .map(|_| "mtow_kg".to_owned())
        .collect();
    let lang = Some(state.language.code());
    let fields = [field];
    let edits = state
        .group_mut("requirements")
        .map(|values| dynamic_form(ui, &fields, values, &errors, lang, false))
        .unwrap_or_default();
    if !edits.is_empty() {
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}

/// The mode selector. The three choices are always listed; a loaded legacy
/// mode is listed after them as a read-only entry.
fn show_mode_selector(state: &mut AppState, ui: &mut Ui) {
    let current = stored_mode(state);
    let stored = mode_is_stored(state);
    let mut chosen = None;
    ui.label(tr("MTOW mode"));
    ComboBox::from_id_salt("inputs_mtow_mode")
        .width(ui.available_width())
        .selected_text(tr(mode_label(current, stored)))
        .show_ui(ui, |ui| {
            for (mode, label) in CHOICES {
                if ui.selectable_label(current == mode, tr(label)).clicked() {
                    chosen = Some(mode);
                }
            }
            if let Some(label) = legacy_label(current, stored) {
                ui.separator();
                ui.add_enabled(false, egui::SelectableLabel::new(true, tr(label)));
            }
        });
    if let Some(mode) = chosen.filter(|mode| *mode != current) {
        set_objective(
            state,
            "mtow_sizing",
            json!(mode.as_str()),
            "Takeoff mass sizing",
        );
    }
}

/// A numeric objective field edited with a drag value.
fn drag_row(
    ui: &mut Ui,
    label: &str,
    hover: &str,
    value: &mut f64,
    speed: f64,
    unit: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(label)).on_hover_text(tr(hover));
        changed = ui
            .add(
                DragValue::new(value)
                    .speed(speed)
                    .suffix(format!(" {unit}")),
            )
            .changed();
    });
    changed && value.is_finite()
}

/// The design range field, with the value it resolves to when left at zero.
fn show_design_range(state: &mut AppState, ui: &mut Ui) {
    let mut range = objective_number(state, "design_range_nmi").unwrap_or(0.0);
    if drag_row(
        ui,
        "Design range",
        "Still-air distance of the design mission. Zero uses the declared design range of the aircraft, or the route distance when none is declared.",
        &mut range,
        10.0,
        "nmi",
    ) {
        set_objective(
            state,
            "design_range_nmi",
            json!(range.max(0.0)),
            "Design range",
        );
    }
    if range <= 0.0 {
        let text = match state
            .typed_config()
            .map(|config| config.design_mission().range)
        {
            Some(DesignRange::ChartedPoint(nmi)) => tr_fields(
                "Default: {range} nmi, the design point of the manufacturer's payload-range chart.",
                &[("range", format!("{nmi:.0}"))],
            ),
            Some(DesignRange::FlopsDesignRange(nmi)) => tr_fields(
                "Default: {range} nmi, the declared design range of the aircraft.",
                &[("range", format!("{nmi:.0}"))],
            ),
            _ => tr("Default: the route distance selected below."),
        };
        ui.label(RichText::new(text).weak().small());
    }
}

fn show_band_controls(state: &mut AppState, ui: &mut Ui) {
    ui.label(tr(
        "The take-off mass may take any value in the band shown below, with no preference toward the target. MTOW is closed on the design mission (design range at full payload), and the route from the Inputs is then checked off-design.",
    ));
    let declared = state
        .typed_config()
        .map_or(0.0, |config| config.requirements.mtow_kg);
    let stored_target = objective_number(state, "mtow_target_kg").unwrap_or(0.0);
    let mut target = if stored_target > 0.0 {
        stored_target
    } else {
        declared
    };
    if drag_row(
        ui,
        "MTOW target",
        "Centre of the band. Defaults to the declared MTOW while it is zero.",
        &mut target,
        100.0,
        "kg",
    ) {
        set_objective(
            state,
            "mtow_target_kg",
            json!(target.max(0.0)),
            "MTOW target",
        );
    }
    let fraction = objective_number(state, "mtow_band_fraction").unwrap_or(0.05);
    let mut percent = fraction * 100.0;
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Allowed variation (+/-)"))
            .on_hover_text(tr("Half-width of the band as a percentage of the target."));
        changed = ui
            .add(
                DragValue::new(&mut percent)
                    .speed(0.1)
                    .range(0.1..=49.0)
                    .suffix(" %"),
            )
            .changed();
    });
    if changed && percent.is_finite() {
        set_objective(
            state,
            "mtow_band_fraction",
            json!(percent / 100.0),
            "MTOW band fraction",
        );
    }
    if let Some(plan) = state.typed_config().map(|config| config.mtow_plan()) {
        if let (Some(lo), Some(hi)) = (plan.lower_bound_kg, plan.upper_bound_kg) {
            ui.label(
                RichText::new(tr_fields(
                    "Band: {lower} to {upper} kg",
                    &[("lower", format!("{lo:.0}")), ("upper", format!("{hi:.0}"))],
                ))
                .weak(),
            );
        }
    }
    show_design_range(state, ui);
    show_band_check(state, ui);
}

/// The report the quick check runs against: the optimized design when a run
/// produced one, otherwise the baseline analysis.
fn latest_report(state: &AppState) -> Option<&alas_pipeline::full_analysis::AnalysisReport> {
    let result = state.pipeline_result.as_ref()?;
    result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
}

/// Quick reachability check of the design mission over the band, or `None`
/// when no analysis report is available.
fn band_check(state: &AppState) -> Option<DesignMissionBand> {
    let report = latest_report(state)?;
    let config: AlasConfig = state.typed_config()?;
    let plan = config.mtow_plan();
    let (lo_kg, hi_kg) = (plan.lower_bound_kg?, plan.upper_bound_kg?);
    let payload_kg = config.design_payload_kg().0;
    let range_m = match config.design_mission().range.declared_nmi() {
        Some(nmi) => nmi * NMI_M,
        None => state
            .pipeline_result
            .as_ref()?
            .route
            .as_ref()?
            .total_distance_m(),
    };
    Some(design_mission_band_check(
        &config, report, payload_kg, range_m, lo_kg, hi_kg,
    ))
}

fn show_band_check(state: &AppState, ui: &mut Ui) {
    let Some(config) = state.typed_config() else {
        return;
    };
    let plan = config.mtow_plan();
    let key = format!(
        "{:?}|{:?}|{}",
        plan,
        config.fuel_policy,
        latest_report(state).map_or(0, |report| std::ptr::from_ref(report) as usize),
    );
    let id = Id::new("inputs_mtow_band_check");
    let cached = ui
        .ctx()
        .data(|data| data.get_temp::<(String, Option<DesignMissionBand>)>(id));
    // While a pointer button is held (a DragValue is being dragged) the two
    // range searches are not rerun; the last result stays until release.
    let pointer_held = ui.input(|input| input.pointer.any_down());
    let check = match cached {
        Some((cached_key, check)) if cached_key == key || pointer_held => check,
        _ => {
            let check = band_check(state);
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, (key, check.clone())));
            check
        }
    };
    let Some(check) = check else {
        ui.label(RichText::new(tr("Run a baseline analysis to check the band.")).weak());
        return;
    };
    let fields = |check: &DesignMissionBand| {
        [
            ("lo", format!("{:.0}", check.range_at_lo_m / NMI_M)),
            ("hi", format!("{:.0}", check.range_at_hi_m / NMI_M)),
        ]
    };
    let warn = ui.visuals().warn_fg_color;
    match check.status {
        BandStatus::BandTooHeavy => {
            ui.label(RichText::new(tr_fields(
                "Warning: the design mission is already reachable at the lower band edge ({lo} nmi), so every mass in the band is heavier than the mission needs. Lower the target or lengthen the design range.",
                &fields(&check),
            )).color(warn));
        }
        BandStatus::BandTooLight => {
            ui.label(RichText::new(tr_fields(
                "Warning: the design mission is out of reach even at the upper band edge ({hi} nmi). Raise the target or the allowed variation, or shorten the design range.",
                &fields(&check),
            )).color(warn));
        }
        BandStatus::Inside => {
            ui.label(
                RichText::new(tr(
                    "The design mission fits inside the band (quick estimate).",
                ))
                .weak(),
            );
        }
        BandStatus::Unavailable => {
            ui.label(RichText::new(tr("The band check is unavailable for this report.")).weak());
        }
    }
}

fn show_adjusted_controls(state: &mut AppState, ui: &mut Ui) {
    show_design_range(state, ui);
    ui.label(tr(
        "Take-off mass is closed by the mission with the selected passengers or cargo and the fuel reserves of the fuel policy.",
    ));
    ui.label(
        RichText::new(tr(
            "Seeded from the declared MTOW; not a limit in this mode.",
        ))
        .weak(),
    );
}

/// Render the card body.
fn show_mtow_body(state: &mut AppState, ui: &mut Ui) {
    if !optimize_active(state) {
        ui.label(RichText::new(tr("Available when Optimize design space is on.")).weak());
        show_declared_mtow(state, ui, HARD_HELP);
        return;
    }
    show_mode_selector(state, ui);
    match stored_mode(state) {
        MtowSizing::FixedRequirement => show_declared_mtow(state, ui, HARD_HELP),
        MtowSizing::MtowBand => {
            show_declared_mtow(
                state,
                ui,
                "Declared MTOW. The band target defaults to it while the target is zero.",
            );
            show_band_controls(state, ui);
        }
        MtowSizing::PayloadAdjusted => {
            show_declared_mtow(
                state,
                ui,
                "Declared MTOW. Only the first estimate of the closure starts from it.",
            );
            show_adjusted_controls(state, ui);
        }
        MtowSizing::SizedByMission | MtowSizing::Unconstrained => {
            show_declared_mtow(
                state,
                ui,
                "Declared MTOW. In the loaded mode it seeds the closure, and it also caps it unless the mode is unconstrained.",
            );
            let note = if mode_is_stored(state) {
                "This mode came from a loaded configuration. Choose one of the listed modes to replace it."
            } else {
                "Default mode: take-off mass closed by the mission up to the declared MTOW."
            };
            ui.label(RichText::new(tr(note)).weak());
        }
    }
}

/// The Maximum take-off mass card.
pub(crate) fn show_mtow_card(state: &mut AppState, ui: &mut Ui) {
    let _ = super::inputs_view::card(ui, "Maximum take-off mass", |ui| {
        show_mtow_body(state, ui);
    });
    ui.add_space(8.0);
}
