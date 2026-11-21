// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A read-only view of the wing's vertical profile on the Geometry page: the
//! stations the builder lofts through (span fraction, ground height, flight
//! rise) and the aircraft from the front, rendered by the 3-D preview's engine.
//!
//! Frame: `y` metres outboard from the centreline, `z` metres up in the
//! geometry axes; the flight rise is positive up and is added to the ground
//! height to give the 1 g shape (`alas_config::WingShape`). Everything here is
//! read from the loaded configuration through `WingConfig::heights`, the call
//! the geometry builder makes, so the table and the markers cannot disagree with
//! the built wing.

use alas_config::{TransportPlanform, WingHeights, WingShape};
use egui::{RichText, Ui};

use crate::sandbox::scene;
use crate::state::AppState;
use crate::views::{tr, tr_label};

/// One row of the stations table.
#[derive(Debug, Clone, PartialEq)]
struct StationRow {
    /// `root`, `kink`, `tip` or empty for an interior station.
    name: &'static str,
    /// Span fraction, 0 at the centreline and 1 at the tip.
    eta: f64,
    /// Ground-shape leading-edge height, metres.
    ground_z_m: f64,
    /// Rise from the ground shape to the 1 g flight shape, metres.
    flight_rise_m: f64,
}

/// The stations of the wing, ascending in span fraction.
fn station_rows(
    ground: &WingHeights,
    flight: &WingHeights,
    planform: &TransportPlanform,
) -> Vec<StationRow> {
    let mut rows = vec![
        StationRow {
            name: "Root",
            eta: 0.0,
            ground_z_m: ground.root_z_m,
            flight_rise_m: flight.root_z_m - ground.root_z_m,
        },
        StationRow {
            name: "Kink",
            eta: planform.kink.span_fraction,
            ground_z_m: ground.break_z_m,
            flight_rise_m: flight.break_z_m - ground.break_z_m,
        },
        StationRow {
            name: "Tip",
            eta: 1.0,
            ground_z_m: ground.tip_z_m,
            flight_rise_m: flight.tip_z_m - ground.tip_z_m,
        },
    ];
    for (g, f) in ground.interior.iter().zip(&flight.interior) {
        rows.push(StationRow {
            name: "",
            eta: g.0,
            ground_z_m: g.1,
            flight_rise_m: f.1 - g.1,
        });
    }
    rows.sort_by(|a, b| a.eta.total_cmp(&b.eta));
    rows
}

/// Draw the card. Nothing is drawn while the configuration is unreadable or
/// its planform cannot be built.
pub(super) fn render(state: &AppState, ui: &mut Ui) {
    let Some(config) = state.typed_config_arc() else {
        return;
    };
    let wing = &config.geometry.wing;
    let design = state.current_design().unwrap_or_default();
    let Ok(planform) = wing.transport_planform(&design) else {
        return;
    };
    let (Ok(ground), Ok(flight)) = (
        wing.heights(WingShape::Ground, &planform),
        wing.heights(WingShape::Flight, &planform),
    ) else {
        return;
    };
    let rows = station_rows(&ground, &flight, &planform);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let title = crate::theme::card_title(ui, tr_label("Wing heights and dihedral"));
        let header = egui::CollapsingHeader::new(title)
            .id_salt("geometry::wing_dihedral")
            .default_open(true);
        crate::theme::show_header(ui, header, |ui| {
                ui.label(
                    RichText::new(tr(
                        "Read-only: the loaded aircraft's wing stations, as the geometry builder uses them.",
                    ))
                    .weak()
                    .small(),
                );
                ui.add_space(4.0);
                draw_table(ui, &rows);
                ui.add_space(6.0);
                draw_front_view(state, ui, &planform, &flight, &rows);
            });
    });
}

fn draw_table(ui: &mut Ui, rows: &[StationRow]) {
    use crate::views::form::{value_table, TableColumn, TableValue};
    // One precision per column: span fractions and heights in metres to
    // three decimals (a millimetre).
    let column = |header: &str, decimals: Option<usize>| TableColumn {
        header: tr_label(header),
        decimals,
    };
    let columns = [
        column("Station", None),
        column("Span fraction", Some(3)),
        column("Ground height [m]", Some(3)),
        column("Flight rise [m]", Some(3)),
    ];
    let rows: Vec<Vec<TableValue>> = rows
        .iter()
        .map(|row| {
            let name = if row.name.is_empty() {
                String::new()
            } else {
                tr_label(row.name)
            };
            vec![
                TableValue::Text(name),
                TableValue::Number(row.eta),
                TableValue::Number(row.ground_z_m),
                TableValue::Number(row.flight_rise_m),
            ]
        })
        .collect();
    value_table(ui, "wing_dihedral_table", &columns, &rows);
}

/// The front view's canvas in scene units for a card `available` scene units
/// wide (the card's settled width over the figure's scale): the card's width
/// and a height from the aircraft's own shape, so the view fills the card
/// with the aircraft at its true aspect.
fn front_canvas(available: f32) -> (f64, f64) {
    let width = f64::from(available).clamp(240.0, 2400.0).floor();
    (width, (width / 3.2).clamp(220.0, 440.0).round())
}

type FrontCache = std::cell::RefCell<Option<(u64, std::sync::Arc<alas_report::scene::Scene>)>>;

thread_local! {
    // One entry: the page shows one aircraft. Keyed by what the build reads.
    static FRONT_VIEW: FrontCache = const { std::cell::RefCell::new(None) };
}

/// The markers the front view carries: the named stations labelled, interior
/// breaks as bare dots, at the built (flight) leading-edge height.
fn front_markers(
    planform: &TransportPlanform,
    flight: &WingHeights,
    rows: &[StationRow],
) -> Vec<scene::WingMarker> {
    let semi_span = planform.tip.y_m;
    rows.iter()
        .map(|row| {
            let y_m = row.eta * semi_span;
            scene::WingMarker {
                y_m,
                z_m: flight.at(planform, y_m),
                label: if row.name.is_empty() {
                    String::new()
                } else {
                    tr_label(row.name)
                },
            }
        })
        .collect()
}

/// The cached scene for the current geometry. A rebuild happens only when the
/// inputs change, and not while a pointer button is held (a slider drag), so
/// editing stays as responsive as without the figure.
fn front_scene(
    state: &AppState,
    ui: &Ui,
    markers: &[scene::WingMarker],
    canvas: (f64, f64),
) -> Option<(std::sync::Arc<alas_report::scene::Scene>, u64)> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.config_fingerprint_now().hash(&mut hasher);
    crate::state::state_memo::design_values_fingerprint(&state.design_values).hash(&mut hasher);
    alas_geom::airfoil_io::generation().hash(&mut hasher);
    state.theme.figure_theme_name().hash(&mut hasher);
    alas_i18n::get_language().hash(&mut hasher);
    canvas.0.to_bits().hash(&mut hasher);
    canvas.1.to_bits().hash(&mut hasher);
    let key = hasher.finish();
    let cached = FRONT_VIEW.with(|slot| slot.borrow().clone());
    if let Some((cached_key, scene)) = &cached {
        if *cached_key == key {
            return Some((std::sync::Arc::clone(scene), key));
        }
        if ui.input(|input| input.pointer.any_down()) {
            ui.ctx().request_repaint();
            return Some((std::sync::Arc::clone(scene), *cached_key));
        }
    }
    if state.preview_jobs.borrow().enabled() {
        // Built on a worker (`crate::preview_jobs`): a first build held the
        // frame about 0.4 s (A320-200). Until it arrives the card keeps the
        // previous scene, or shows nothing on the very first visit.
        use crate::preview_jobs::PreviewLookup;
        const SLOT: &str = "geometry::wing_dihedral::front";
        let found = state.preview_jobs.borrow().peek(SLOT, key);
        let found = match found {
            Some(found) => found,
            None => {
                let config = state.typed_config()?;
                let design = state.current_design()?;
                let theme = state.theme.figure_theme_name().to_owned();
                let markers = markers.to_vec();
                state.preview_jobs.borrow_mut().lookup(SLOT, key, move || {
                    scene::build_front_view_from(&config, design, &theme, canvas, &markers)
                })
            }
        };
        return match found {
            PreviewLookup::Ready(scene) => {
                let scene = scene?;
                FRONT_VIEW.with(|slot| {
                    *slot.borrow_mut() = Some((key, std::sync::Arc::clone(&scene)));
                });
                Some((scene, key))
            }
            PreviewLookup::Pending(stale) => {
                stale.or_else(|| cached.map(|(old_key, scene)| (scene, old_key)))
            }
        };
    }
    let scene = std::sync::Arc::new(scene::build_front_view(state, canvas, markers)?);
    FRONT_VIEW.with(|slot| *slot.borrow_mut() = Some((key, std::sync::Arc::clone(&scene))));
    Some((scene, key))
}

/// The aircraft from the front, drawn by the 3-D preview's own renderer.
fn draw_front_view(
    state: &AppState,
    ui: &mut Ui,
    planform: &TransportPlanform,
    flight: &WingHeights,
    rows: &[StationRow],
) {
    use crate::theme::figure;
    const KEY: &str = "geometry::wing_dihedral::front";
    let markers = front_markers(planform, flight, rows);
    // The card figure system: built for the card width over the scale that
    // draws its smallest text at the figure-text token, drawn at that scale.
    let width = figure::settled_width(ui, KEY);
    let scale = figure::last_scale(ui, KEY, 1.0);
    let Some((mut scene, mut revision)) =
        front_scene(state, ui, &markers, front_canvas(width / scale))
    else {
        return;
    };
    let wanted = figure::text_scale(&scene);
    figure::remember_scale(ui, KEY, wanted);
    if figure::rescale_needed(scale, wanted) {
        if let Some(rebuilt) = front_scene(state, ui, &markers, front_canvas(width / wanted)) {
            (scene, revision) = rebuilt;
        }
    }
    let size = figure::figure_size(&scene, figure::text_scale(&scene), ui.available_width());
    let mut view_state = alas_viz::SceneViewState::default();
    // Rasterizing the shaded aircraft held the frame about 0.5 s (A320-200,
    // measured) after every geometry edit: with the preview workers on it is
    // drawn on the raster pool and the card keeps its previous image.
    let background = state
        .preview_jobs
        .borrow()
        .enabled()
        .then(|| tr("Building the preview\u{2026}"));
    let view = alas_viz::SceneView::new(&scene, &mut view_state)
        .static_view()
        .show_toolbar(false)
        .cache_key(KEY)
        .cache_revision(revision)
        .background_raster(background);
    figure::show(ui, size, view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{presets, DesignVector};

    /// The table rows come from the builder's own `heights`, so the root and
    /// tip rows are the configured values for a ground-only wing.
    #[test]
    fn rows_follow_the_configured_heights() {
        let preset = presets::get("A340-300").expect("A340-300 is registered");
        let wing = &preset.geometry.wing;
        let planform = wing
            .transport_planform(&DesignVector::default())
            .expect("planform");
        let ground = wing.heights(WingShape::Ground, &planform).expect("ground");
        let flight = wing.heights(WingShape::Flight, &planform).expect("flight");
        let rows = station_rows(&ground, &flight, &planform);
        assert_eq!(rows.first().map(|row| row.ground_z_m), Some(wing.root_z_m));
        assert_eq!(rows.last().map(|row| row.ground_z_m), Some(wing.tip_z_m));
        assert!(rows.windows(2).all(|pair| pair[0].eta <= pair[1].eta));
        assert!(rows.len() >= 3 + wing.dihedral_stations.len() - 1);
    }

    /// The front view is the Sandbox renderer's scene of the loaded aircraft:
    /// many shaded faces, plus one marker circle per station and a ground line.
    #[test]
    fn the_front_view_is_an_engine_render_with_station_markers() {
        use alas_report::scene::SceneElement;
        let mut state = AppState::default();
        state.load_preset("A340-300");
        let config = state.typed_config().expect("typed config");
        let design = state.current_design().unwrap_or_default();
        let wing = &config.geometry.wing;
        let planform = wing.transport_planform(&design).expect("planform");
        let ground = wing.heights(WingShape::Ground, &planform).expect("ground");
        let flight = wing.heights(WingShape::Flight, &planform).expect("flight");
        let rows = station_rows(&ground, &flight, &planform);
        let markers = front_markers(&planform, &flight, &rows);
        assert_eq!(markers.len(), rows.len());
        let scene = scene::build_front_view(&state, front_canvas(900.0), &markers).expect("builds");
        let circles = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Circle { .. }))
            .count();
        let polygons = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polygon { .. }))
            .count();
        assert_eq!(circles, markers.len());
        assert!(polygons > 100, "{polygons} faces");
        assert_labels_are_clear(&scene);
        if let Some(path) = std::env::var_os("ALAS_FRONT_VIEW_PNG") {
            let png = alas_viz::raster::render_scene_png(&scene).expect("raster");
            std::fs::write(path, png).expect("write png");
        }
    }

    /// Every station label sits in clear space: not on the ground line or its
    /// caption, not on the airframe's drawn faces, not on another label.
    fn assert_labels_are_clear(scene: &alas_report::scene::Scene) {
        use alas_report::scene::SceneElement;
        let mut labels = Vec::new();
        let mut ground_y = None;
        for element in &scene.elements {
            match element {
                SceneElement::Text {
                    text,
                    pos,
                    font_size,
                    bold: true,
                    ..
                } if *font_size == 12.0 => {
                    let half = text.chars().count() as f64 * 12.0 * 0.62 * 0.5;
                    labels.push((
                        text.clone(),
                        pos[0] - half,
                        pos[0] + half,
                        pos[1] - 16.0,
                        pos[1],
                    ));
                }
                SceneElement::Text { text, pos, .. } if text.starts_with("Lowest point") => {
                    ground_y = Some(pos[1] - 4.0);
                }
                _ => {}
            }
        }
        assert!(
            labels.len() >= 3,
            "root, kink and tip are labelled: {labels:?}"
        );
        let ground_y = ground_y.expect("the ground line is captioned");
        for (index, (name, left, right, top, bottom)) in labels.iter().enumerate() {
            assert!(*bottom < ground_y - 4.0, "{name} is above the ground line");
            for element in &scene.elements {
                if let SceneElement::Polygon { points, .. } = element {
                    for column in 0..=6 {
                        for row in 0..=2 {
                            let sample = [
                                left + (right - left) * f64::from(column) / 6.0,
                                top + (bottom - top) * f64::from(row) / 2.0,
                            ];
                            assert!(
                                !crate::sandbox::scene::point_in_polygon(sample, points),
                                "{name} lies over the airframe"
                            );
                        }
                    }
                }
            }
            for (other, l2, r2, t2, b2) in labels.iter().skip(index + 1) {
                assert!(
                    right <= l2 || left >= r2 || bottom <= t2 || top >= b2,
                    "{name} and {other} overlap"
                );
            }
        }
    }
}
