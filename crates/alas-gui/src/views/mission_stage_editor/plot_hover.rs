// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the mission plot says about a phase when it is hovered: the facts of the
//! flown (or estimated) flight that are on hand, the tooltip built from them, and
//! the hover state the plot and the numbered list share.
//!
//! Nothing is computed here beyond sums of the phases' own figures: a value the
//! flight data does not give (the aircraft mass at the end of a phase, the fuel
//! of a planned flight) is left out of the tooltip.

use egui::{Context, Id, Ui};

use super::model::ProfileSegment;
use super::plot_axis::phase_name;
use super::standard::Leg;
use crate::views::{tr_fields, tr_label};

/// What the flight data gives about one phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Facts {
    /// Distance flown at the start and the end of the phase, km, for a phase of the trip.
    pub km: Option<(f64, f64)>,
    /// Ground distance of the phase, km.
    pub length_km: f64,
    /// Duration, s, when the phase takes time.
    pub duration_s: Option<f64>,
    /// Fuel burned in the phase, kg, when the flight was flown.
    pub fuel_kg: Option<f64>,
}

/// The facts of each leg; the trip's legs are placed along one distance axis from the departure,
/// the reserve legs (flown from the destination) only give their own length.
pub(super) fn facts_of(legs: &[Leg]) -> Vec<Facts> {
    let mut flown = 0.0_f64;
    legs.iter()
        .map(|leg| {
            let length_km = leg.ground_m.max(0.0) / 1_000.0;
            let km = (!leg.reserve).then(|| {
                let start = flown;
                flown += length_km;
                (start, flown)
            });
            Facts {
                km,
                length_km,
                duration_s: (leg.duration_s > 0.0).then_some(leg.duration_s),
                fuel_kg: leg.fuel_kg,
            }
        })
        .collect()
}

/// The distance flown at each of the `facts.len() + 1` phase boundaries, km, where the boundary
/// belongs to the trip; none when the trip has no distance at all.
pub(super) fn boundary_km(facts: &[Facts]) -> Option<Vec<Option<f64>>> {
    let total = facts
        .iter()
        .filter_map(|fact| fact.km)
        .map(|(_, end)| end)
        .fold(0.0_f64, f64::max);
    if total < 1.0 {
        return None;
    }
    Some(
        (0..=facts.len())
            .map(|index| {
                facts
                    .get(index)
                    .and_then(|fact| fact.km)
                    .map(|(start, _)| start)
                    .or_else(|| {
                        index
                            .checked_sub(1)
                            .and_then(|previous| facts[previous].km)
                            .map(|(_, end)| end)
                    })
            })
            .collect(),
    )
}

/// An altitude as the tooltip says it: the ground below 100 ft, else a flight level.
fn level(ft: f64) -> String {
    if ft < 100.0 {
        tr_label("Ground")
    } else {
        format!("FL{:.0}", ft / 100.0)
    }
}

/// A duration as "9 min", "1 h 05 min" or "45 s".
fn duration(seconds: f64) -> String {
    if seconds < 90.0 {
        return format!("{seconds:.0} s");
    }
    let minutes = (seconds / 60.0).round();
    if minutes < 60.0 {
        format!("{minutes:.0} min")
    } else {
        format!(
            "{:.0} h {:02.0} min",
            (minutes / 60.0).floor(),
            minutes % 60.0
        )
    }
}

/// The text of the tooltip of a phase.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tip {
    /// "k -> k+1 . name".
    pub head: String,
    /// The lines under it.
    pub lines: Vec<String>,
    /// Whether the figures are the estimate shown while the flown mission is computed.
    pub estimate: bool,
}

/// The tooltip of phase `index` of `segments`, from its stage-list `row` and `fact`.
pub(super) fn tip(
    index: usize,
    segment: &ProfileSegment,
    row: &str,
    fact: Option<&Facts>,
    estimate: bool,
) -> Tip {
    let head = format!(
        "{} \u{2192} {} \u{b7} {}",
        index + 1,
        index + 2,
        phase_name(row, index)
    );
    let mut lines = vec![tr_fields(
        "Altitude {from} \u{2192} {to}",
        &[
            ("from", level(segment.altitude_ft[0])),
            ("to", level(segment.altitude_ft[1])),
        ],
    )];
    if let Some(fact) = fact {
        if let Some((from, to)) = fact.km {
            lines.push(tr_fields(
                "Distance {from}\u{2013}{to} km",
                &[("from", format!("{from:.0}")), ("to", format!("{to:.0}"))],
            ));
        } else if fact.length_km >= 0.5 {
            lines.push(tr_fields(
                "Distance {length} km",
                &[("length", format!("{:.0}", fact.length_km))],
            ));
        }
        if let Some(seconds) = fact.duration_s {
            lines.push(tr_fields("Duration {time}", &[("time", duration(seconds))]));
        }
        if let Some(fuel) = fact.fuel_kg {
            lines.push(tr_fields(
                "Fuel burned {fuel} kg",
                &[("fuel", format!("{fuel:.0}"))],
            ));
        }
    }
    Tip {
        head,
        lines,
        estimate,
    }
}

/// Narrowest the mission-profile tooltip gets, in points.
const TIP_MIN_WIDTH: f32 = 220.0;

/// Show `tip` next to the pointer.
pub(super) fn show_tip(ui: &Ui, tip: &Tip) {
    egui::show_tooltip_at_pointer(
        ui.ctx(),
        ui.layer_id(),
        Id::new("mission_profile_tip"),
        |ui| {
            // A tooltip placed at the pointer takes the room right of it,
            // which near the window edge is a word wide: keep a readable
            // width so no label wraps mid-phrase.
            ui.set_min_width(TIP_MIN_WIDTH);
            ui.label(egui::RichText::new(&tip.head).strong());
            if tip.estimate {
                ui.colored_label(ui.visuals().warn_fg_color, tr_label("Estimate"));
            }
            for line in &tip.lines {
                ui.label(line);
            }
        },
    );
}

/// The segment each side of the plot and list pair is hovering (an index into the drawn
/// segments), kept in the egui memory so one highlights the other.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Shared {
    plot: Option<usize>,
    list: Option<usize>,
}

fn key() -> Id {
    Id::new("mission_profile_hover")
}

fn read(ctx: &Context) -> Shared {
    ctx.data(|data| data.get_temp::<Shared>(key()))
        .unwrap_or_default()
}

fn write(ctx: &Context, update: impl FnOnce(&mut Shared)) {
    let mut shared = read(ctx);
    let before = shared;
    update(&mut shared);
    if shared != before {
        ctx.data_mut(|data| data.insert_temp(key(), shared));
        ctx.request_repaint();
    }
}

/// The segment hovered on the plot or on the list, if any.
pub(super) fn hovered(ctx: &Context) -> Option<usize> {
    let shared = read(ctx);
    shared.plot.or(shared.list)
}

/// Record the segment the plot is hovering this frame.
pub(super) fn set_plot(ctx: &Context, index: Option<usize>) {
    write(ctx, |shared| shared.plot = index);
}

/// Record the segment the list is hovering this frame (`index` is the hovered entry; a list
/// entry that is not hovered leaves the record alone unless it was the one recorded).
pub(super) fn set_list(ctx: &Context, index: usize, hovered: bool) {
    write(ctx, |shared| {
        if hovered {
            shared.list = Some(index);
        } else if shared.list == Some(index) {
            shared.list = None;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::mission_stage_editor::standard::LegKind;

    fn leg(reserve: bool, ground_km: f64, fuel: Option<f64>) -> Leg {
        Leg {
            kind: LegKind::Cruise,
            start_m: 0.0,
            end_m: 0.0,
            distance_m: 0.0,
            ground_m: ground_km * 1_000.0,
            duration_s: 600.0,
            fuel_kg: fuel,
            reserve,
            phase: None,
        }
    }

    #[test]
    fn the_trip_is_placed_along_one_distance_axis_and_the_reserves_only_have_a_length() {
        let facts = facts_of(&[
            leg(false, 100.0, Some(500.0)),
            leg(false, 300.0, None),
            leg(true, 200.0, None),
        ]);
        assert_eq!(facts[0].km, Some((0.0, 100.0)));
        assert_eq!(facts[1].km, Some((100.0, 400.0)));
        assert_eq!(facts[2].km, None);
        assert_eq!(facts[2].length_km, 200.0);
        let km = boundary_km(&facts).expect("distance");
        assert_eq!(km, [Some(0.0), Some(100.0), Some(400.0), None]);
        assert_eq!(boundary_km(&facts_of(&[leg(false, 0.0, None)])), None);
    }

    #[test]
    fn the_tip_leaves_out_what_the_data_does_not_give() {
        let segment = ProfileSegment {
            stage: 0,
            progress: [0.0, 0.1],
            altitude_ft: [0.0, 35_000.0],
            via_ft: None,
            trend: crate::views::mission_stage_editor::model::Trend::Climb,
            reserve: false,
            has_events: false,
            release: None,
        };
        let facts = facts_of(&[leg(false, 120.0, None)]);
        let tip = tip(2, &segment, "3. Climb - FL350", facts.first(), true);
        assert_eq!(tip.head, "3 \u{2192} 4 \u{b7} Climb \u{2013} FL350");
        assert!(tip.estimate);
        assert_eq!(tip.lines.len(), 3, "{:?}", tip.lines);
        assert!(tip.lines[0].contains("Ground") && tip.lines[0].contains("FL350"));
        assert!(tip.lines[1].contains("0\u{2013}120 km"));
        assert!(tip.lines[2].contains("10 min"));
        let none = super::tip(0, &segment, "1. Taxi", None, false);
        assert_eq!(none.lines.len(), 1);
    }

    #[test]
    fn durations_read_in_the_unit_that_fits() {
        assert_eq!(duration(45.0), "45 s");
        assert_eq!(duration(540.0), "9 min");
        assert_eq!(duration(3_900.0), "1 h 05 min");
    }
}
