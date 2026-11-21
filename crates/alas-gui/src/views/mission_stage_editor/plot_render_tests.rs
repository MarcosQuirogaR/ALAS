// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Renders the mission plot with the software rasteriser (no GPU) at the card widths of the
//! review's screenshots: the flight of the review (two and three cruise legs, the reserve
//! program, 15 to 17 phases) as the Mission page draws it, and checks that every phase boundary
//! is numbered, no text (the markers' numbers included) touches another, the distance axis is
//! distinct and the pictures are written for a look.

use egui::{pos2, Rect};

use super::list;
use super::model::profile;
use super::plot::{self, Inputs};
use super::plot_hover::facts_of;
use super::plot_markers::RADIUS;
use super::standard::{self, Leg, LegKind};
use crate::test_raster::{overlapping_texts, render_hovering, write, Shot};

fn leg(kind: LegKind, start_m: f64, end_m: f64, distance_km: f64, minutes: f64) -> Leg {
    Leg {
        kind,
        start_m,
        end_m,
        distance_m: distance_km * 1000.0,
        ground_m: distance_km * 1000.0,
        duration_s: minutes * 60.0,
        fuel_kg: None,
        reserve: matches!(
            kind,
            LegKind::MissedApproach
                | LegKind::DiversionClimb
                | LegKind::DiversionCruise
                | LegKind::DiversionDescent
                | LegKind::Hold
                | LegKind::DiversionLanding
        ),
        phase: None,
    }
}

/// The flight of the review: taxi, take-off, climb, `cruises` cruise legs with a step climb
/// between each pair, descent, landing, taxi-in, then the reserve program.
fn flight(cruises: usize) -> standard::Standard {
    let mut legs = vec![
        leg(LegKind::TaxiOut, 0.0, 0.0, 0.0, 9.0),
        leg(LegKind::Takeoff, 0.0, 10.0, 0.0, 1.0),
        leg(LegKind::Climb, 10.0, 10_668.0, 150.0, 18.0),
    ];
    for index in 0..cruises {
        if index > 0 {
            let top = 10_668.0 + 610.0 * index as f64;
            legs.push(leg(LegKind::StepClimb, top - 610.0, top, 40.0, 4.0));
        }
        let level = 10_668.0 + 610.0 * index as f64;
        legs.push(leg(
            LegKind::Cruise,
            level,
            level,
            3_400.0 / cruises as f64,
            60.0,
        ));
    }
    let top = 10_668.0 + 610.0 * (cruises - 1) as f64;
    legs.extend([
        leg(LegKind::Descent, top, 460.0, 130.0, 24.0),
        leg(LegKind::Landing, 460.0, 0.0, 0.0, 2.0),
        leg(LegKind::TaxiIn, 0.0, 0.0, 0.0, 7.0),
        leg(LegKind::MissedApproach, 0.0, 460.0, 0.0, 2.0),
        leg(LegKind::DiversionClimb, 460.0, 8_700.0, 0.0, 14.0),
        leg(LegKind::DiversionCruise, 8_700.0, 8_700.0, 200.0, 20.0),
        leg(LegKind::DiversionDescent, 8_700.0, 460.0, 0.0, 16.0),
        leg(LegKind::Hold, 460.0, 460.0, 0.0, 30.0),
        leg(LegKind::DiversionLanding, 460.0, 0.0, 0.0, 2.0),
    ]);
    standard::display(&legs)
}

/// One render of the plot of `flight` in a panel `width` pt wide with `room` pt for it, the
/// pointer resting at `pointer` when given.
fn shot_at(
    flight: &standard::Standard,
    width: f32,
    room: f32,
    estimate: bool,
    pointer: Option<egui::Pos2>,
) -> Shot {
    let segments = profile(&flight.plan, 0.0, 0.0, 3_000.0);
    let facts = facts_of(&flight.legs);
    let used = std::cell::Cell::new(0.0_f32);
    // A canvas taller than any plot; the picture is cut to the height the plot took.
    let mut picture = render_hovering(width, plot::max_total() + 140.0, 6, 0, pointer, |ui| {
        plot::show(
            ui,
            &Inputs {
                segments: &segments,
                rows: &flight.rows,
                facts: &facts,
                selected: Some(2),
                estimate,
                room,
            },
        );
        used.set(ui.cursor().top() - ui.spacing().item_spacing.y + 2.0);
    });
    if pointer.is_none() {
        picture.height = (used.get().ceil() as usize + 2).min(picture.height);
        picture.pixels.truncate(picture.height * picture.width);
    }
    picture
}

fn shot(flight: &standard::Standard, width: f32, room: f32, estimate: bool) -> Shot {
    shot_at(flight, width, room, estimate, None)
}

/// Height the standard mission's numbered list takes under a plot `width` pt wide.
fn under(width: f32) -> f32 {
    list::rows_height(&alas_config::MissionPlan::default(), 15, width, 26.0)
}

/// Every panel width of the review's screenshots with the room the page gives the plot there:
/// A 1280x800 pt window (list under the plot), B 1920x1120 and C 2752x1120 (list beside it), D
/// 880x1120 (list under it).
fn panels() -> Vec<(&'static str, f32, f32)> {
    vec![
        ("A", 1_200.0, 800.0 - 250.0 - under(1_200.0)),
        ("B", 1_290.0, 1_120.0 - 250.0),
        ("C", 1_900.0, 1_120.0 - 250.0),
        ("D", 830.0, 1_120.0 - 250.0 - under(830.0)),
        ("E", 830.0, 608.0 - 250.0 - under(830.0)),
    ]
}

/// The numbered markers of a picture: `(number, centre)` of every text that is a bare number in
/// the plot area (the altitude labels are left of it, the distance ticks under it).
fn marker_labels(picture: &Shot) -> Vec<(usize, egui::Pos2)> {
    let bottom = picture.height as f32 - plot::BELOW;
    picture
        .texts
        .iter()
        .filter(|(text, rect)| {
            rect.center().x > 56.0
                && rect.center().y < bottom + 4.0
                && text.chars().all(|c| c.is_ascii_digit())
        })
        .filter_map(|(text, rect)| Some((text.parse().ok()?, rect.center())))
        .collect()
}

#[test]
fn the_plot_is_drawn_at_the_card_widths_of_the_review_screens() {
    for cruises in [2, 3] {
        let flight = flight(cruises);
        for (name, width, room) in panels() {
            let picture = shot(&flight, width, room, false);
            write(&format!("mission-{name}-{cruises}legs.png"), &picture);
        }
    }
    let estimate = shot(&flight(3), 1_200.0, 550.0, true);
    write("mission-A-3legs-estimate.png", &estimate);
}

#[test]
fn every_boundary_is_numbered_and_no_label_touches_another() {
    for cruises in [2, 3] {
        let flight = flight(cruises);
        for (name, width, room) in panels() {
            let picture = shot(&flight, width, room, false);
            let touching = overlapping_texts(&picture);
            assert!(touching.is_empty(), "{name} {cruises} legs: {touching:?}");
            // Boundaries 1 to N+1 are each numbered once, and their circles never overlap.
            let mut markers = marker_labels(&picture);
            markers.sort_by_key(|(number, _)| *number);
            let numbers: Vec<usize> = markers.iter().map(|(number, _)| *number).collect();
            let expected: Vec<usize> = (1..=flight.legs.len() + 1).collect();
            assert_eq!(numbers, expected, "{name} {cruises} legs");
            for (i, (a, at)) in markers.iter().enumerate() {
                for (b, bt) in &markers[i + 1..] {
                    assert!(
                        (*at - *bt).length() >= 2.0 * RADIUS + 6.0 - 0.5,
                        "{name} {cruises} legs: markers {a} and {b} are closer than 6 pt"
                    );
                }
            }
            // Every marker circle lies inside the plot area's width (first and last included).
            for (number, at) in &markers {
                assert!(
                    at.x - RADIUS >= plot::LEFT - 0.5
                        && at.x + RADIUS <= picture.width as f32 - plot::RIGHT + 0.5,
                    "{name} {cruises} legs: marker {number} at {at:?} leaves the plot"
                );
                assert!(at.y > 0.0 && at.y < picture.height as f32 - plot::BELOW + 10.0);
            }
            // No phase name is written on the plot: only the reserve band is labelled, once.
            for short in ["Taxi out", "Step climb", "Missed appr.", "Alt. landing"] {
                assert!(
                    picture.texts.iter().all(|(text, _)| text != short),
                    "{name}: {short:?} written under the plot"
                );
            }
            let bands = picture
                .texts
                .iter()
                .filter(|(text, _)| text == "Reserves (flown)")
                .count();
            assert_eq!(bands, 1, "{name} {cruises} legs");
            // The x axis is in km, at distinct ticks, and every text stays inside the picture.
            let ticks: Vec<&str> = picture
                .texts
                .iter()
                .filter(|(text, rect)| {
                    rect.center().y > picture.height as f32 - plot::BELOW
                        && rect.center().y < picture.height as f32 - 14.0
                        && is_tick_number(text)
                })
                .map(|(text, _)| text.as_str())
                .collect();
            assert!(ticks.len() >= 3, "{name}: {ticks:?}");
            let mut distinct = ticks.clone();
            distinct.sort_unstable();
            distinct.dedup();
            assert_eq!(ticks.len(), distinct.len(), "{name}: {ticks:?}");
            // The tick labels are at least 8 pt apart.
            let mut spans: Vec<(f32, f32)> = picture
                .texts
                .iter()
                .filter(|(text, rect)| {
                    rect.center().y > picture.height as f32 - plot::BELOW
                        && rect.center().y < picture.height as f32 - 14.0
                        && is_tick_number(text)
                })
                .map(|(_, rect)| (rect.left(), rect.right()))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            for pair in spans.windows(2) {
                assert!(
                    pair[1].0 - pair[0].1 >= 8.0,
                    "{name} {cruises} legs: tick labels {pair:?} are cramped"
                );
            }
            assert!(picture
                .texts
                .iter()
                .any(|(text, _)| text == "Distance [km]"));
            assert!(picture.texts.iter().all(|(text, _)| !text.ends_with(" %")));
            let frame = Rect::from_min_max(
                pos2(-1.0, -1.0),
                pos2(picture.width as f32 + 1.0, picture.height as f32 + 1.0),
            );
            for (text, rect) in &picture.texts {
                assert!(
                    frame.contains_rect(*rect),
                    "{name}: {text:?} {rect:?} leaves the picture"
                );
            }
        }
    }
}

#[test]
fn the_plot_follows_the_room_the_page_gives_it_between_a_least_and_a_most() {
    let flight = flight(2);
    let tall = shot(&flight, 1_290.0, 5_000.0, false);
    let short = shot(&flight, 1_290.0, 10.0, false);
    assert!(
        tall.height as f32 <= plot::max_total() + 5.0,
        "{}",
        tall.height
    );
    assert!(short.height as f32 >= plot::min_total(), "{}", short.height);
    let mid = shot(&flight, 1_290.0, 500.0, false);
    assert!((mid.height as f32 - 500.0).abs() < 14.0, "{}", mid.height);
}

#[test]
fn hovering_a_step_climb_shows_its_tooltip_and_writes_the_picture() {
    let flight = flight(3);
    let segments = profile(&flight.plan, 0.0, 0.0, 3_000.0);
    let step = flight
        .legs
        .iter()
        .position(|leg| leg.kind == LegKind::StepClimb)
        .expect("a step climb");
    let (_, width, room) = panels()[0];
    let pointer = plot::anchor(width, room, &segments, step);
    let picture = shot_at(&flight, width, room, false, Some(pointer));
    let texts: Vec<&str> = picture
        .texts
        .iter()
        .map(|(text, _)| text.as_str())
        .collect();
    let head = format!("{} \u{2192} {}", step + 1, step + 2);
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with(&head) && text.contains("Step climb")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.starts_with("Duration")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.starts_with("Distance")),
        "{texts:?}"
    );
    // The flight is not flown here: no fuel figure is made up.
    assert!(texts.iter().all(|text| !text.starts_with("Fuel burned")));
    write("mission-A-hover.png", &picture);
    // The estimate says so in the tooltip too.
    let estimate = shot_at(&flight, width, room, true, Some(pointer));
    assert!(
        estimate
            .texts
            .iter()
            .filter(|(t, _)| t == "Estimate")
            .count()
            >= 2
    );
}

#[test]
fn the_tooltip_of_the_last_phase_keeps_its_head_on_one_line_at_the_right_edge() {
    let flight = flight(3);
    let segments = profile(&flight.plan, 0.0, 0.0, 3_000.0);
    let last = segments.len() - 1;
    let (_, width, room) = panels()[0];
    let pointer = plot::anchor(width, room, &segments, last);
    let picture = shot_at(&flight, width, room, false, Some(pointer));
    let head = picture
        .texts
        .iter()
        .find(|(text, _)| text.contains(" \u{2192} "))
        .unwrap_or_else(|| panic!("no tooltip head in {:?}", picture.texts));
    // One line: the whole head in one text, wider than a word and as tall as one row.
    assert!(head.1.width() > 100.0, "{head:?}");
    assert!(head.1.height() < 24.0, "{head:?}");
    crate::test_raster::write_in(
        &crate::test_raster::render_dir(),
        "c2-mission-tooltip-right-edge.png",
        &picture,
    );
}

/// An x tick label: a distance in km, its thousands grouped by spaces
/// (`alas_units::notation::group_thousands`).
fn is_tick_number(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_digit() || c == ' ')
}
