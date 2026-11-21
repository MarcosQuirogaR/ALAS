// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuselage-shape and cabin-layout plausibility rules.
//!
//! The cabin items are placed on a floor that follows the body centre line, so
//! a tail upsweep or a tapering section tilts the floor of the aft rows; these
//! rules judge that against the outer fuselage contour.

use super::{Findings, RuleInfo, Severity};

pub(super) const G_FLOOR_LEVEL: RuleInfo = RuleInfo {
    id: "CABIN-FLOOR-LEVEL",
    statement: "The main-deck floor under the seats and containers varies by at most 0.10 m over its length (HIGH above 0.25 m) and nowhere slopes more than 2 degrees",
    source: "Owner rule 2026-10-19 (sloping cabin floor in the tail); passenger floors are level in cruise attitude (Raymer Sec. 10.3)",
};
pub(super) const G_SEATS_IN_CONTOUR: RuleInfo = RuleInfo {
    id: "CABIN-SEATS-IN-CONTOUR",
    statement: "Every seat row and galley/lavatory block lies inside the outer fuselage contour at its station and height (HIGH if it protrudes; MEDIUM if within 0.08 m of the skin)",
    source: "Owner rule 2026-10-19 (seats overlapping the fuselage); outer skin is a hard limit, frames and lining take about 0.08-0.15 m",
};
pub(super) const G_AISLE_WIDTH: RuleInfo = RuleInfo {
    id: "CABIN-AISLE-WIDTH",
    statement: "Aisle width is at least 0.38 m (15 in) at floor level; at least 0.51 m (20 in) for cabins of 20 or more seats (MEDIUM below)",
    source: "14 CFR 25.815 / CS 25.815 (passenger aisle width)",
};
pub(super) const G_CARGO_IN_CONTOUR: RuleInfo = RuleInfo {
    id: "CARGO-IN-CONTOUR",
    statement: "Every container and bulk load lies inside the outer fuselage contour at its station and height (HIGH if it protrudes)",
    source: "Geometry; ULD contours must fit the hold cross-section (IATA ULD Regulations)",
};
pub(super) const G_NO_OVERLAP: RuleInfo = RuleInfo {
    id: "CABIN-NO-OVERLAP",
    statement: "No two seat rows, monuments or containers occupy the same volume (overlap above 0.01 m3; HIGH above 0.1 m3)",
    source: "Geometry; two solids cannot share volume",
};
pub(super) const G_TAILCONE_MONOTONIC: RuleInfo = RuleInfo {
    id: "FUSELAGE-TAILCONE",
    statement: "Aft of the maximum section the cross-section area never grows (1 % tolerance, HIGH above 5 %); width, height and centre line show no reversal",
    source: "Owner rule 2026-10-19 (tail-cone bumps); smooth closure of the aft body (Torenbeek Sec. 8.1, Raymer Sec. 7)",
};
pub(super) const G_SECTION_SMOOTH: RuleInfo = RuleInfo {
    id: "FUSELAGE-SMOOTH",
    statement: "In the nose and the tail cone width and height are individually monotonic and the area is continuous (no jump above 35 % of the maximum area between stations less than 1 m apart)",
    source: "Smooth lofted surface; sudden section changes are build artefacts",
};
pub(super) const G_NOSE_MONOTONIC: RuleInfo = RuleInfo {
    id: "FUSELAGE-NOSE",
    statement: "Forward of the maximum section the cross-section area never shrinks (1 % tolerance, HIGH above 5 %)",
    source: "Smooth closure of the forward body (Torenbeek Sec. 8.1)",
};

/// One fuselage cross-section, metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Longitudinal position.
    pub x: f64,
    /// Outer width.
    pub width: f64,
    /// Outer height.
    pub height: f64,
    /// Centre height.
    pub zc: f64,
    /// Superellipse exponent.
    pub shape: f64,
}

/// What a cabin item is, for the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemClass {
    /// A row of seats.
    Seat,
    /// A galley, lavatory, wheelchair bay or overhead-bin run.
    Monument,
    /// A container or bulk load.
    Cargo,
    /// A zero-mass exit marker.
    Exit,
}

/// A placed cabin item, metres and kilograms.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFacts {
    /// Class for rule selection.
    pub class: ItemClass,
    /// Deck name.
    pub deck: String,
    /// Human label.
    pub label: String,
    /// Centre x.
    pub x: f64,
    /// Centre y.
    pub y: f64,
    /// Centre z.
    pub z: f64,
    /// Extent along x.
    pub length: f64,
    /// Extent along y.
    pub width: f64,
    /// Extent along z.
    pub height: f64,
    /// Aisle width the row was laid out against (seat rows).
    pub aisle_w: Option<f64>,
    /// Seats in the row (seat rows).
    pub seats: i64,
    /// How far the item's box starts above the floor it stands on, m: a seat
    /// row's envelope starts at the seat pan, the base below it standing on
    /// the floor tracks.
    pub base_above_floor: f64,
}

fn lerp_station(stations: &[Station], x: f64) -> Option<Station> {
    let first = stations.first()?;
    let last = stations.last()?;
    if x < first.x - 1e-9 || x > last.x + 1e-9 {
        return None;
    }
    let i = stations
        .partition_point(|s| s.x <= x)
        .clamp(1, stations.len() - 1);
    let (a, b) = (stations[i - 1], stations[i]);
    let t = if (b.x - a.x).abs() < 1e-12 {
        0.0
    } else {
        ((x - a.x) / (b.x - a.x)).clamp(0.0, 1.0)
    };
    let mix = |p: f64, q: f64| p + t * (q - p);
    Some(Station {
        x,
        width: mix(a.width, b.width),
        height: mix(a.height, b.height),
        zc: mix(a.zc, b.zc),
        shape: mix(a.shape, b.shape),
    })
}

/// Outer half-width of the contour at station `x` and height `z`, or `None`
/// when `x` is outside the body. Zero when `z` is outside the section.
pub fn half_width_at(stations: &[Station], x: f64, z: f64) -> Option<f64> {
    let s = lerp_station(stations, x)?;
    let (a, b) = (0.5 * s.width, 0.5 * s.height);
    if b <= 0.0 {
        return Some(0.0);
    }
    let dz = (z - s.zc).abs() / b;
    if dz >= 1.0 {
        return Some(0.0);
    }
    let shape = s.shape.max(1.0);
    Some(a * (1.0 - dz.powf(shape)).powf(1.0 / shape))
}

/// Margin (m) between the item and the outer contour: negative when it protrudes.
fn contour_margin(stations: &[Station], item: &ItemFacts) -> f64 {
    let mut margin = f64::INFINITY;
    for dx in [-0.5, 0.0, 0.5] {
        for dz in [-0.5, 0.5] {
            let x = item.x + dx * item.length;
            let z = item.z + dz * item.height;
            match half_width_at(stations, x, z) {
                Some(hw) => margin = margin.min(hw - (item.y.abs() + 0.5 * item.width)),
                None => return f64::NEG_INFINITY,
            }
        }
    }
    margin
}

/// Items against the contour, aisles, floor and each other.
pub fn check_cabin(out: &mut Findings, stations: &[Station], items: &[ItemFacts]) {
    let mut worst_seat = f64::INFINITY;
    let mut worst_cargo = f64::INFINITY;
    let (mut seat_label, mut cargo_label) = (String::new(), String::new());
    for item in items {
        match item.class {
            ItemClass::Seat | ItemClass::Monument => {
                let m = contour_margin(stations, item);
                if m < worst_seat {
                    worst_seat = m;
                    seat_label = format!("{} deck {} at x={:.1} m", item.label, item.deck, item.x);
                }
            }
            ItemClass::Cargo => {
                let m = contour_margin(stations, item);
                if m < worst_cargo {
                    worst_cargo = m;
                    cargo_label = format!("{} deck {} at x={:.1} m", item.label, item.deck, item.x);
                }
            }
            ItemClass::Exit => {}
        }
    }
    if worst_seat < 0.0 {
        out.add(
            G_SEATS_IN_CONTOUR.id,
            Severity::High,
            format!(
                "{seat_label} protrudes {:.2} m beyond the skin",
                -worst_seat
            ),
        );
    } else if worst_seat < 0.08 {
        out.add(
            G_SEATS_IN_CONTOUR.id,
            Severity::Medium,
            format!("{seat_label} is {:.3} m from the skin", worst_seat),
        );
    }
    if worst_cargo < 0.0 {
        out.add(
            G_CARGO_IN_CONTOUR.id,
            Severity::High,
            format!(
                "{cargo_label} protrudes {:.2} m beyond the skin",
                -worst_cargo
            ),
        );
    }

    let seats_total: i64 = items
        .iter()
        .filter(|i| i.class == ItemClass::Seat)
        .map(|i| i.seats)
        .sum();
    let narrowest = items
        .iter()
        .filter(|i| i.class == ItemClass::Seat)
        .filter_map(|i| i.aisle_w.map(|w| (w, i)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((w, item)) = narrowest {
        if w < 0.38 {
            out.add(
                G_AISLE_WIDTH.id,
                Severity::High,
                format!("aisle {w:.3} m at x={:.1} m", item.x),
            );
        } else if w < 0.51 && seats_total >= 20 {
            out.add(
                G_AISLE_WIDTH.id,
                Severity::Medium,
                format!("aisle {w:.3} m with {seats_total} seats"),
            );
        }
    }

    check_floor(out, items);
    check_overlap(out, items);
}

fn check_floor(out: &mut Findings, items: &[ItemFacts]) {
    let mut floors: Vec<(f64, f64)> = items
        .iter()
        .filter(|i| i.deck == "main" && matches!(i.class, ItemClass::Seat | ItemClass::Cargo))
        .map(|i| (i.x, i.z - 0.5 * i.height - i.base_above_floor))
        .collect();
    floors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (Some(lo), Some(hi)) = (
        floors.iter().map(|p| p.1).reduce(f64::min),
        floors.iter().map(|p| p.1).reduce(f64::max),
    ) else {
        return;
    };
    let spread = hi - lo;
    if spread > 0.25 {
        out.add(
            G_FLOOR_LEVEL.id,
            Severity::High,
            format!("floor height varies {spread:.2} m over the cabin"),
        );
    } else if spread > 0.10 {
        out.add(
            G_FLOOR_LEVEL.id,
            Severity::Medium,
            format!("floor height varies {spread:.2} m over the cabin"),
        );
    }
    let max_slope = floors
        .windows(2)
        .filter(|w| w[1].0 - w[0].0 > 0.2)
        .map(|w| {
            ((w[1].1 - w[0].1) / (w[1].0 - w[0].0))
                .atan()
                .to_degrees()
                .abs()
        })
        .fold(0.0_f64, f64::max);
    if max_slope > 2.0 {
        out.add(
            G_FLOOR_LEVEL.id,
            Severity::Medium,
            format!("floor slopes {max_slope:.1} degrees between rows"),
        );
    }
}

fn overlap_volume(a: &ItemFacts, b: &ItemFacts) -> f64 {
    let ov = |ca: f64, la: f64, cb: f64, lb: f64| {
        ((ca + 0.5 * la).min(cb + 0.5 * lb) - (ca - 0.5 * la).max(cb - 0.5 * lb)).max(0.0)
    };
    ov(a.x, a.length, b.x, b.length)
        * ov(a.y, a.width, b.y, b.width)
        * ov(a.z, a.height, b.z, b.height)
}

fn check_overlap(out: &mut Findings, items: &[ItemFacts]) {
    let mut worst = (0.0_f64, String::new());
    let solid = |i: &ItemFacts| i.class != ItemClass::Exit && i.deck != "none";
    for (index, a) in items.iter().enumerate() {
        if !solid(a) {
            continue;
        }
        for b in &items[index + 1..] {
            if !solid(b) || a.deck != b.deck {
                continue;
            }
            let v = overlap_volume(a, b);
            if v > worst.0 {
                worst = (
                    v,
                    format!(
                        "{} and {} on the {} deck near x={:.1} m",
                        a.label, b.label, a.deck, a.x
                    ),
                );
            }
        }
    }
    if worst.0 > 0.1 {
        out.add(
            G_NO_OVERLAP.id,
            Severity::High,
            format!("{} overlap {:.2} m3", worst.1, worst.0),
        );
    } else if worst.0 > 0.01 {
        out.add(
            G_NO_OVERLAP.id,
            Severity::Medium,
            format!("{} overlap {:.3} m3", worst.1, worst.0),
        );
    }
}

/// Fuselage shape: nose and tail-cone monotonicity and smoothness.
pub fn check_fuselage(out: &mut Findings, stations: &[Station]) {
    if stations.len() < 4 {
        return;
    }
    let area: Vec<f64> = stations.iter().map(|s| s.width * s.height).collect();
    let (imax, amax) =
        area.iter().enumerate().fold(
            (0, 0.0_f64),
            |acc, (i, &a)| if a > acc.1 + 1e-12 { (i, a) } else { acc },
        );
    if amax <= 0.0 {
        return;
    }
    // Tail: a station with growth over its predecessor, relative to its predecessor.
    let growth = |range: std::ops::Range<usize>, sign: f64| {
        range
            .filter(|&i| i >= 1 && i < stations.len())
            .map(|i| (i, sign * (area[i] - area[i - 1]) / area[i - 1].max(1e-9)))
            .fold(
                (0, 0.0_f64),
                |acc, item| if item.1 > acc.1 { item } else { acc },
            )
    };
    // The maximum section is the last station of the constant-section run.
    let last_max = area
        .iter()
        .rposition(|&a| a >= amax * (1.0 - 1e-9))
        .unwrap_or(imax);
    let first_max = area
        .iter()
        .position(|&a| a >= amax * (1.0 - 1e-9))
        .unwrap_or(imax);
    let (ti, tg) = growth(last_max + 1..stations.len(), 1.0);
    if tg > 0.05 {
        out.add(
            G_TAILCONE_MONOTONIC.id,
            Severity::High,
            format!(
                "tail-cone area grows {:.0} % at x={:.1} m",
                100.0 * tg,
                stations[ti].x
            ),
        );
    } else if tg > 0.01 {
        out.add(
            G_TAILCONE_MONOTONIC.id,
            Severity::Medium,
            format!(
                "tail-cone area grows {:.1} % at x={:.1} m",
                100.0 * tg,
                stations[ti].x
            ),
        );
    }
    let (ni, ng) = growth(1..first_max + 1, -1.0);
    if ng > 0.05 {
        out.add(
            G_NOSE_MONOTONIC.id,
            Severity::High,
            format!(
                "nose area shrinks {:.0} % at x={:.1} m",
                100.0 * ng,
                stations[ni].x
            ),
        );
    } else if ng > 0.01 {
        out.add(
            G_NOSE_MONOTONIC.id,
            Severity::Medium,
            format!(
                "nose area shrinks {:.1} % at x={:.1} m",
                100.0 * ng,
                stations[ni].x
            ),
        );
    }

    let tail = &stations[last_max..];
    let mut reversal = |name: &str,
                        f: &dyn Fn(&Station) -> f64,
                        falling: bool,
                        tol: f64,
                        region: &[Station],
                        rule: &'static str| {
        for w in region.windows(2) {
            let d = f(&w[1]) - f(&w[0]);
            if (falling && d > tol) || (!falling && d < -tol) {
                out.add(
                    rule,
                    Severity::Medium,
                    format!("{name} reverses by {:.3} m at x={:.1} m", d.abs(), w[1].x),
                );
                return;
            }
        }
    };
    reversal(
        "tail width",
        &|s| s.width,
        true,
        0.01 * amax.sqrt(),
        tail,
        G_SECTION_SMOOTH.id,
    );
    reversal(
        "tail height",
        &|s| s.height,
        true,
        0.01 * amax.sqrt(),
        tail,
        G_SECTION_SMOOTH.id,
    );
    reversal(
        "tail centre line",
        &|s| s.zc,
        false,
        0.01 * amax.sqrt(),
        tail,
        G_TAILCONE_MONOTONIC.id,
    );
    let nose = &stations[..=first_max];
    reversal(
        "nose width",
        &|s| s.width,
        false,
        0.01 * amax.sqrt(),
        nose,
        G_SECTION_SMOOTH.id,
    );
    reversal(
        "nose height",
        &|s| s.height,
        false,
        0.01 * amax.sqrt(),
        nose,
        G_SECTION_SMOOTH.id,
    );
    let jump = stations
        .windows(2)
        .filter(|w| w[1].x - w[0].x < 1.0)
        .map(|w| ((w[1].width * w[1].height) - (w[0].width * w[0].height)).abs() / amax)
        .fold(0.0_f64, f64::max);
    if jump > 0.35 {
        out.add(
            G_SECTION_SMOOTH.id,
            Severity::Medium,
            format!(
                "area jumps {:.0} % of the maximum between adjacent stations",
                100.0 * jump
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(x: f64, d: f64) -> Station {
        Station {
            x,
            width: d,
            height: d,
            zc: 0.0,
            shape: 2.0,
        }
    }

    fn body() -> Vec<Station> {
        vec![
            st(0.0, 0.5),
            st(1.5, 2.5),
            st(3.0, 3.6),
            st(4.0, 3.9),
            st(5.0, 3.95),
            st(30.0, 3.95),
            st(38.0, 2.5),
            st(42.0, 0.8),
        ]
    }

    fn run(f: impl FnOnce(&mut Findings)) -> Findings {
        let mut out = Findings::new("synthetic");
        f(&mut out);
        out
    }

    fn seat(x: f64, y: f64, w: f64, z: f64) -> ItemFacts {
        ItemFacts {
            class: ItemClass::Seat,
            deck: "main".into(),
            label: "row".into(),
            x,
            y,
            z,
            length: 0.8,
            width: w,
            height: 1.0,
            aisle_w: Some(0.55),
            seats: 6,
            base_above_floor: 0.0,
        }
    }

    #[test]
    fn smooth_body_passes_and_a_bump_in_the_tail_cone_is_high() {
        assert!(run(|o| check_fuselage(o, &body())).items().is_empty());
        let mut bumpy = body();
        bumpy.insert(7, st(39.0, 3.2));
        let out = run(|o| check_fuselage(o, &bumpy));
        assert_eq!(out.severity_of("FUSELAGE-TAILCONE"), Some(Severity::High));
    }

    #[test]
    fn seats_inside_pass_and_outside_are_high() {
        let ok = vec![seat(15.0, 0.0, 3.2, 0.0)];
        assert!(run(|o| check_cabin(o, &body(), &ok)).items().is_empty());
        let out_row = vec![seat(15.0, 0.0, 4.2, 0.0)];
        assert_eq!(
            run(|o| check_cabin(o, &body(), &out_row)).severity_of("CABIN-SEATS-IN-CONTOUR"),
            Some(Severity::High)
        );
        let tail_row = vec![seat(41.0, 0.0, 1.5, 0.0)];
        assert_eq!(
            run(|o| check_cabin(o, &body(), &tail_row)).severity_of("CABIN-SEATS-IN-CONTOUR"),
            Some(Severity::High)
        );
    }

    #[test]
    fn sloping_floor_overlap_and_narrow_aisle_are_found() {
        let rows: Vec<ItemFacts> = (0..12)
            .map(|i| {
                let x = 10.0 + 0.8 * i as f64;
                let mut r = seat(x, 0.0, 3.0, -0.4 + 0.03 * (i as f64).powi(2));
                r.aisle_w = Some(0.30);
                r
            })
            .collect();
        let out = run(|o| check_cabin(o, &body(), &rows));
        assert!(out.has("CABIN-FLOOR-LEVEL"));
        assert_eq!(out.severity_of("CABIN-AISLE-WIDTH"), Some(Severity::High));
        let dup = vec![seat(15.0, 0.0, 3.0, 0.0), seat(15.1, 0.0, 3.0, 0.0)];
        assert_eq!(
            run(|o| check_cabin(o, &body(), &dup)).severity_of("CABIN-NO-OVERLAP"),
            Some(Severity::High)
        );
    }
}
