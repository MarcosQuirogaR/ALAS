// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The value editors of one stage and of its events. Each returns whether a
//! value changed this frame.

use alas_config::{
    CruiseExtent, EventAction, EventTime, PlanStage, StageEvent, StageKind, TakeoffField,
};
use egui::{DragValue, Ui};

use super::release;
use crate::views::tr_label;

fn value(ui: &mut Ui, label: &str, value: &mut f64, unit: &str, speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr_label(label));
        let mut drag = DragValue::new(value).speed(speed);
        if !unit.is_empty() {
            drag = drag.suffix(format!(" {unit}"));
        }
        changed = ui.add(drag).changed();
    });
    changed
}

/// A value that may be absent: a checkbox for "stated", the value when it is.
fn optional(
    ui: &mut Ui,
    label: &str,
    slot: &mut Option<f64>,
    default: f64,
    unit: &str,
    speed: f64,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut stated = slot.is_some();
        if crate::checkbox::checkbox(ui, &mut stated, tr_label(label)).changed() {
            *slot = stated.then_some(default);
            changed = true;
        }
        if let Some(current) = slot {
            let drag = DragValue::new(current)
                .speed(speed)
                .suffix(format!(" {unit}"));
            changed |= ui.add(drag).changed();
        }
    });
    changed
}

fn flag(ui: &mut Ui, label: &str, flag: &mut bool) -> bool {
    crate::checkbox::checkbox(ui, flag, tr_label(label)).changed()
}

fn altitude(ui: &mut Ui, label: &str, ft: &mut f64, agl: &mut bool) -> bool {
    let mut changed = value(ui, label, ft, "ft", 100.0);
    changed |= flag(ui, "Above Field", agl);
    changed
}

fn optional_altitude(ui: &mut Ui, ft: &mut Option<f64>, agl: &mut bool, default_ft: f64) -> bool {
    let mut changed = optional(ui, "Altitude", ft, default_ft, "ft", 100.0);
    if ft.is_some() {
        changed |= flag(ui, "Above Field", agl);
    }
    changed
}

fn takeoff_field(ui: &mut Ui, field: &mut Option<TakeoffField>) -> bool {
    let mut changed = false;
    let mut checked = field.is_some();
    if crate::checkbox::checkbox(ui, &mut checked, tr_label("Field Check")).changed() {
        *field = checked.then(|| TakeoffField {
            name: String::new(),
            elevation_ft: 0.0,
            toda_m: 2_500.0,
            isa_deviation_k: 0.0,
        });
        changed = true;
    }
    if let Some(field) = field {
        ui.horizontal(|ui| {
            ui.label(tr_label("Name"));
            changed |= ui.text_edit_singleline(&mut field.name).changed();
        });
        changed |= value(ui, "Elevation", &mut field.elevation_ft, "ft", 50.0);
        changed |= value(ui, "Distance Available", &mut field.toda_m, "m", 10.0);
        changed |= value(ui, "ISA Deviation", &mut field.isa_deviation_k, "K", 0.5);
    }
    changed
}

fn cruise_extent(ui: &mut Ui, extent: &mut CruiseExtent) -> bool {
    let mut changed = false;
    let names = ["Distance", "Time", "Route Remainder"];
    let current = match extent {
        CruiseExtent::DistanceKm(_) => 0,
        CruiseExtent::TimeMin(_) => 1,
        CruiseExtent::Remainder => 2,
    };
    ui.horizontal(|ui| {
        ui.label(tr_label("Extent"));
        for (index, name) in names.iter().enumerate() {
            if ui
                .selectable_label(current == index, tr_label(name))
                .clicked()
                && current != index
            {
                *extent = match index {
                    0 => CruiseExtent::DistanceKm(500.0),
                    1 => CruiseExtent::TimeMin(60.0),
                    _ => CruiseExtent::Remainder,
                };
                changed = true;
            }
        }
    });
    match extent {
        CruiseExtent::DistanceKm(km) => changed |= value(ui, "Distance", km, "km", 10.0),
        CruiseExtent::TimeMin(min) => changed |= value(ui, "Time", min, "min", 1.0),
        CruiseExtent::Remainder => {}
    }
    changed
}

/// The editor of the selected stage's kind-specific values.
pub(super) fn stage(ui: &mut Ui, stage: &mut PlanStage) -> bool {
    let mut changed = false;
    match &mut stage.kind {
        StageKind::Taxi { time_min } => changed |= value(ui, "Time", time_min, "min", 0.5),
        StageKind::Takeoff { field } => changed |= takeoff_field(ui, field),
        StageKind::Climb {
            altitude_ft,
            agl,
            rate_m_s,
        } => {
            changed |= altitude(ui, "Altitude", altitude_ft, agl);
            changed |= optional(ui, "Climb Rate", rate_m_s, 10.0, "m/s", 0.5);
        }
        StageKind::Cruise {
            extent,
            altitude_ft,
            agl,
            speed_m_s,
        } => {
            changed |= cruise_extent(ui, extent);
            changed |= optional_altitude(ui, altitude_ft, agl, 35_000.0);
            changed |= optional(ui, "Speed", speed_m_s, 230.0, "m/s", 1.0);
        }
        StageKind::Loiter {
            time_min,
            altitude_ft,
            agl,
            speed_m_s,
        } => {
            changed |= value(ui, "Time", time_min, "min", 1.0);
            changed |= optional_altitude(ui, altitude_ft, agl, 1_500.0);
            changed |= optional(ui, "Speed", speed_m_s, 120.0, "m/s", 1.0);
        }
        StageKind::Combat {
            time_min,
            load_factor,
            altitude_ft,
            agl,
            speed_m_s,
        } => {
            changed |= value(ui, "Time", time_min, "min", 0.5);
            changed |= value(ui, "Load Factor", load_factor, "g", 0.1);
            changed |= optional_altitude(ui, altitude_ft, agl, 15_000.0);
            changed |= optional(ui, "Speed", speed_m_s, 200.0, "m/s", 1.0);
        }
        StageKind::Descent { altitude_ft, agl } | StageKind::GoAround { altitude_ft, agl } => {
            changed |= altitude(ui, "Altitude", altitude_ft, agl);
        }
        StageKind::Approach {
            decision_height_ft,
            field_elevation_ft,
        } => {
            changed |= value(ui, "Decision Height", decision_height_ft, "ft", 10.0);
            changed |= optional(ui, "Field Elevation", field_elevation_ft, 0.0, "ft", 50.0);
        }
        StageKind::Diversion {
            distance_km,
            cruise_altitude_ft,
            field_elevation_ft,
        } => {
            changed |= value(ui, "Distance", distance_km, "km", 10.0);
            changed |= value(ui, "Cruise Altitude", cruise_altitude_ft, "ft", 100.0);
            changed |= optional(ui, "Field Elevation", field_elevation_ft, 0.0, "ft", 50.0);
        }
        StageKind::Landing { field_elevation_ft } => {
            changed |= optional(ui, "Field Elevation", field_elevation_ft, 0.0, "ft", 50.0);
        }
    }
    changed |= flag(ui, "Reserve", &mut stage.reserve);
    changed |= value(
        ui,
        "Extra Drag Area",
        &mut stage.extra_drag_area_m2,
        "m\u{b2}",
        0.01,
    );
    if stage.extra_drag_area_m2 > 0.0 {
        ui.horizontal(|ui| {
            ui.label(tr_label("Drag Basis"));
            changed |= ui
                .text_edit_singleline(&mut stage.extra_drag_basis)
                .changed();
        });
    }
    changed
}

fn when(ui: &mut Ui, at: &mut EventTime) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr_label("At"));
        for (time, name) in [(EventTime::Start, "Start"), (EventTime::End, "End")] {
            if ui.selectable_label(*at == time, tr_label(name)).clicked() && *at != time {
                *at = time;
                changed = true;
            }
        }
    });
    changed
}

fn text(ui: &mut Ui, label: &str, text: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr_label(label));
        changed = ui.text_edit_singleline(text).changed();
    });
    changed
}

/// A choice among `options`; free text when there are none to offer.
fn choice(ui: &mut Ui, label: &str, current: &mut String, options: &[String]) -> bool {
    if options.is_empty() {
        return text(ui, label, current);
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr_label(label));
        egui::ComboBox::from_id_salt(("event_choice", label))
            .icon(crate::combo_icon::down_arrow)
            .selected_text(current.as_str())
            .show_ui(ui, |ui| {
                for option in options {
                    changed |= ui
                        .selectable_value(current, option.clone(), option)
                        .changed();
                }
            });
    });
    changed
}

/// What the editor of one event needs besides the event: the payload items a
/// release may name, the releases other stages already make, and where to
/// report point masses a ticked row made releasable.
pub(super) struct ReleaseContext<'a> {
    pub items: &'a [release::PayloadItem],
    pub elsewhere: &'a [(String, String)],
    pub newly_releasable: &'a mut Vec<String>,
}

/// The editor of one event.
pub(super) fn event(
    ui: &mut Ui,
    event: &mut StageEvent,
    payload: &mut ReleaseContext<'_>,
    tanks: &[String],
) -> bool {
    let mut changed = when(ui, &mut event.at);
    match &mut event.action {
        EventAction::PayloadRelease { items } => {
            let edit = release::table(ui, payload.items, items, payload.elsewhere);
            changed |= edit.changed;
            payload.newly_releasable.extend(edit.newly_releasable);
        }
        EventAction::StoreAdd {
            id,
            mass_kg,
            x_m,
            y_m,
            z_m,
            drag_area_m2,
        } => {
            changed |= text(ui, "Identifier", id);
            changed |= value(ui, "Item Mass", mass_kg, "kg", 10.0);
            changed |= value(ui, "Station", x_m, "m", 0.1);
            changed |= value(ui, "Lateral Position", y_m, "m", 0.1);
            changed |= value(ui, "Height", z_m, "m", 0.1);
            changed |= optional(ui, "Drag Area", drag_area_m2, 0.1, "m\u{b2}", 0.01);
        }
        EventAction::AerialRefuel { mass_kg } => {
            changed |= value(ui, "Fuel Received", mass_kg, "kg", 50.0);
        }
        EventAction::FuelTransfer { from, to, mass_kg } => {
            changed |= choice(ui, "From Tank", from, tanks);
            changed |= choice(ui, "To Tank", to, tanks);
            changed |= value(ui, "Mass Moved", mass_kg, "kg", 10.0);
        }
        EventAction::TrimTankControl {
            target_min_pct_mac,
            target_max_pct_mac,
        } => {
            changed |= value(ui, "Forward Limit", target_min_pct_mac, "% MAC", 0.1);
            changed |= value(ui, "Aft Limit", target_max_pct_mac, "% MAC", 0.1);
        }
        EventAction::TrimTankOff => {}
    }
    changed
}
