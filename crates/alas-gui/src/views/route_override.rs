// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Route card's derived value cells, and the take-off and landing distance
//! overrides edited in place at the Advanced level.
//!
//! The distances available are derived from the route's airports. A design field
//! length is stated on the case itself (`toda_override_m`, `lda_override_m`): it
//! is saved with the aircraft, leaves the airports' identity alone and can only
//! shorten the route's distance. A longer value is ignored, and the cell says so.

use alas_config::airport_dataset::{self, RunwayDataKind};
use egui::{RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr_label;

/// The stated distance of `key` on the case, if any.
pub(crate) fn stated(state: &AppState, key: &str) -> Option<f64> {
    state.config_values.get(key).and_then(Value::as_f64)
}

/// State (or clear, with `None`) the distance stored under `key`.
pub(crate) fn set_override(state: &mut AppState, key: &str, value_m: Option<f64>) {
    let Some(root) = state.config_values.as_object_mut() else {
        return;
    };
    match value_m {
        Some(value) => root.insert(key.to_owned(), Value::from(value)),
        None => root.remove(key),
    };
    state.on_config_modified();
    state.note_parameter_modified(
        tr_label("Design Field Length"),
        value_m.map_or_else(|| tr_label("Derived"), |value| format!("{value:.0} m")),
    );
}

/// The route's distance when the one stated under `key` is longer and so ignored.
fn ignored_route_m(state: &AppState, key: &str) -> Option<f64> {
    let config = state.typed_config_arc()?;
    let ignored = if key == "toda_override_m" {
        config.takeoff_override_ignored()
    } else {
        config.landing_override_ignored()
    };
    ignored.map(|(_, route_m)| route_m)
}

/// One value cell of the Route card: the label with its lock above a value box
/// that fills the column. A derived value is locked (it comes from the airport
/// database); at the Advanced level a take-off or landing distance is editable
/// in place, which states it on the case (`override_key`) and shows a reset
/// arrow until it is cleared again.
struct Cell<'a> {
    label: &'a str,
    value: Option<f64>,
    unit: &'a str,
    override_key: Option<&'a str>,
    physical_length: bool,
}

fn value_cell(state: &mut AppState, ui: &mut Ui, cell: Cell<'_>) {
    let stated_now = cell.override_key.and_then(|key| stated(state, key));
    let editable = cell.override_key.is_some() && state.shell.detail.is_advanced();
    let ignored = cell
        .override_key
        .and_then(|key| ignored_route_m(state, key));
    let mut cleared = false;
    let rect = ui
        .vertical(|ui| {
            // The caption of the cell, with the lock or the reset glyph over
            // the right end of the box that fills the cell.
            crate::views::form::label_row(ui, tr_label(cell.label), |ui| {
                if stated_now.is_some() {
                    if ui
                        .add(egui::Button::new("\u{21ba}").small())
                        .on_hover_text(tr_label("Back to the airport value"))
                        .clicked()
                    {
                        cleared = true;
                    }
                } else if !editable {
                    // A lock marks a value that cannot be edited here; an
                    // editable one looks editable and says where it comes
                    // from on hover.
                    crate::theme::lock_icon(ui, &tr_label("Derived from the route's airports."));
                }
                if cell.physical_length && stated_now.is_none() {
                    ui.label(RichText::new(tr_label("Physical length")).weak().small());
                }
            });
            let mut shown = cell.value.unwrap_or(0.0);
            let missing = cell.value.is_none();
            let mut drag = egui::DragValue::new(&mut shown)
                .speed(10.0)
                .max_decimals(0)
                .range(1.0..=30_000.0)
                .suffix(format!(" {}", cell.unit));
            if missing {
                drag = drag.custom_formatter(|_, _| "\u{2014}".to_owned());
            }
            let width = ui.available_width();
            let height = ui.spacing().interact_size.y;
            let changed = ui
                .add_enabled_ui(editable, |ui| {
                    let response = ui.add_sized([width, height], drag);
                    if stated_now.is_none() {
                        response.clone().on_hover_text(tr_label(
                            "From the airport database. Edit the value to override it.",
                        ));
                    }
                    response.changed()
                })
                .inner;
            if changed {
                if let Some(key) = cell.override_key {
                    set_override(state, key, Some(shown.round()));
                }
            }
            if let Some(route_m) = ignored {
                ui.label(
                    RichText::new(
                        tr_label("Ignored: Longer Than The Route's {route} m")
                            .replace("{route}", &format!("{route_m:.0}")),
                    )
                    .weak()
                    .small(),
                );
            }
        })
        .response
        .rect;
    if let Some(key) = cell.override_key {
        // The override is an Advanced field: at Basic the cell only shows the
        // derived value, so a search result for the override has nothing to land on.
        if editable {
            crate::search::focus_anchor(ui, key, rect);
        }
        if cleared {
            ui.data_mut(|data| {
                data.remove::<(u64, DerivedRoute)>(egui::Id::new("derived_route_cache"))
            });
            set_override(state, key, None);
        }
    }
}

/// The values of the derived route rows, resolved from the route's airports.
#[derive(Clone, Default)]
struct DerivedRoute {
    toda_m: Option<f64>,
    lda_m: Option<f64>,
    elevation_m: Option<f64>,
    isa_deviation_c: Option<f64>,
    takeoff_physical: bool,
    landing_physical: bool,
}

impl DerivedRoute {
    /// Resolve the rows from the airport dataset. Not free: it is cached by
    /// [`derived_route`] against the inputs it reads.
    fn resolve(state: &AppState) -> Self {
        let text = |key: &str| {
            state
                .config_values
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let departure = airport_dataset::resolve(&text("departure_airport")).ok();
        let arrival = airport_dataset::resolve(&text("arrival_airport")).ok();
        let config = state.typed_config_arc();
        let physical = |record: &Option<airport_dataset::ProvenancedAirport>, overridden: bool| {
            !overridden
                && record
                    .as_ref()
                    .is_some_and(|a| a.runway_data_kind == RunwayDataKind::PhysicalRunwayLength)
        };
        let takeoff_stated = config.as_ref().is_some_and(|c| c.toda_override_m.is_some());
        let landing_stated = config.as_ref().is_some_and(|c| c.lda_override_m.is_some());
        Self {
            toda_m: config
                .as_ref()
                .and_then(|c| c.departure_airport_record().ok())
                .map(|airport| airport.toda_m),
            lda_m: config
                .as_ref()
                .and_then(|c| c.arrival_airport_record().ok())
                .map(|airport| airport.lda_m),
            elevation_m: departure.as_ref().and_then(|a| a.elevation_m.value),
            isa_deviation_c: departure.as_ref().and_then(|a| a.isa_deviation_c.value),
            takeoff_physical: physical(&departure, takeoff_stated),
            landing_physical: physical(&arrival, landing_stated),
        }
    }
}

/// The derived route rows, resolved again only when the airports, the stated
/// distances or the airport registry change (airport lookups used to run on
/// every frame).
fn derived_route(state: &AppState, ui: &Ui) -> DerivedRoute {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for key in [
        "departure_airport",
        "arrival_airport",
        "toda_override_m",
        "lda_override_m",
    ] {
        state
            .config_values
            .get(key)
            .map(Value::to_string)
            .hash(&mut hasher);
    }
    // A custom airport registers without touching these keys.
    alas_config::airport_io::registered_custom_airports()
        .len()
        .hash(&mut hasher);
    let key = hasher.finish();
    let id = egui::Id::new("derived_route_cache");
    if let Some((cached_key, cached)) = ui.data(|data| data.get_temp::<(u64, DerivedRoute)>(id)) {
        if cached_key == key {
            return cached;
        }
    }
    let resolved = DerivedRoute::resolve(state);
    ui.data_mut(|data| data.insert_temp(id, (key, resolved.clone())));
    resolved
}

/// Take-off and landing distances available, the departure elevation and the
/// design temperature, read from the route's airports, as cells of the Route
/// card's `grid`. A stated override shows in place of the derived distance; a
/// field with physical runway data only is flagged, its value being the
/// longest runway used conservatively.
pub(crate) fn derived_route_cells(
    state: &mut AppState,
    ui: &mut Ui,
    grid: &mut crate::views::form::CellGrid,
) {
    let route = derived_route(state, ui);
    let cells = [
        Cell {
            label: "Take-off Distance Available",
            value: route.toda_m,
            unit: "m",
            override_key: Some("toda_override_m"),
            physical_length: route.takeoff_physical,
        },
        Cell {
            label: "Landing Distance Available",
            value: route.lda_m,
            unit: "m",
            override_key: Some("lda_override_m"),
            physical_length: route.landing_physical,
        },
        Cell {
            label: "Departure Elevation",
            value: route.elevation_m,
            unit: "m",
            override_key: None,
            physical_length: false,
        },
        Cell {
            label: "Design Temperature Deviation",
            value: route.isa_deviation_c,
            unit: "K",
            override_key: None,
            physical_length: false,
        },
    ];
    for cell in cells {
        grid.cell(ui, |ui, _| value_cell(state, ui, cell));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_library::ObjectKind;

    #[test]
    fn a_stated_distance_reaches_the_airport_record_and_survives_save_and_reopen() {
        let root = std::env::temp_dir().join(format!("alas-override-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        state.load_preset("A320-200");
        let config = state.typed_config().expect("typed");
        let derived = config.departure_airport_record().expect("airport").toda_m;
        let landing = config.arrival_airport_record().expect("airport").lda_m;

        set_override(&mut state, "toda_override_m", Some(2_345.0));
        let config = state.typed_config().expect("typed");
        assert_eq!(config.toda_override_m, Some(2_345.0));
        let departure = config.departure_airport_record().expect("airport");
        assert!((departure.toda_m - 2_345.0).abs() < 1e-9);
        assert_eq!(departure.name, config.departure_airport);
        // The other end is untouched, and a round trip does not share state.
        assert!((config.arrival_airport_record().expect("airport").lda_m - landing).abs() < 1e-9);

        assert!(state.library_save(ObjectKind::Aircraft, "Override", ""));
        let mut fresh = AppState {
            tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
            ..AppState::default()
        };
        fresh.load_preset("A220-300");
        let entry = crate::file_library::list(&fresh.data_dir(), ObjectKind::Aircraft)
            .into_iter()
            .next()
            .expect("saved aircraft");
        assert!(fresh.library_open(&entry));
        assert_eq!(stated(&fresh, "toda_override_m"), Some(2_345.0));

        set_override(&mut fresh, "toda_override_m", None);
        let config = fresh.typed_config().expect("typed");
        assert_eq!(config.toda_override_m, None);
        assert!(
            (config.departure_airport_record().expect("airport").toda_m - derived).abs() < 1e-9
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_longer_value_than_the_route_is_reported_as_ignored_and_a_shorter_one_is_not() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let route = state
            .typed_config()
            .expect("typed")
            .departure_airport_record()
            .expect("airport")
            .toda_m;
        set_override(&mut state, "toda_override_m", Some(route + 500.0));
        assert_eq!(ignored_route_m(&state, "toda_override_m"), Some(route));
        set_override(&mut state, "toda_override_m", Some(route - 500.0));
        assert_eq!(ignored_route_m(&state, "toda_override_m"), None);
        assert_eq!(ignored_route_m(&state, "lda_override_m"), None);
        // The landing end follows the same rule.
        let landing = state
            .typed_config()
            .expect("typed")
            .arrival_airport_record()
            .expect("airport")
            .lda_m;
        set_override(&mut state, "lda_override_m", Some(landing + 700.0));
        assert_eq!(ignored_route_m(&state, "lda_override_m"), Some(landing));
        assert_eq!(ignored_route_m(&state, "toda_override_m"), None);
        set_override(&mut state, "lda_override_m", Some(landing - 400.0));
        assert_eq!(ignored_route_m(&state, "lda_override_m"), None);
    }
}
