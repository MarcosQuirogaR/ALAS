// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawn cabin section of each registered body (measured section
//! per body): the passenger floor and the underfloor hold sit where the
//! manufacturer's section drawing puts them, the hold polygon lies inside the
//! lofted skin, and a body without a drawing takes the class section.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::{presets, AlasConfig, CabinSectionConfig, HoldSectionConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::geometry::CabinGeometry;

fn geometry_of(name: &str, section: Option<Option<CabinSectionConfig>>) -> CabinGeometry {
    let preset = presets::get(name).unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    if let Some(section) = section {
        config.geometry.fuselage.cabin_section = section;
    }
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    product_cabin_geometry(&plane, &config).unwrap()
}

fn x_ref(g: &CabinGeometry) -> f64 {
    0.5 * (g.cabin_start_x + g.cabin_end_x)
}

/// Depth of the main passenger floor below the outer crown at mid cabin, m.
fn floor_depth(g: &CabinGeometry) -> f64 {
    let x = x_ref(g);
    g.zc_at(x) + 0.5 * g.height_at(x) - g.floor_z(&g.passenger_decks[0], x)
}

#[test]
fn every_registered_body_builds_its_section_and_the_drawn_floor_depths_are_kept() {
    let drawn = [
        ("A320-200", 2.41),
        ("A220-300", 2.30),
        ("E195-E2", 2.115),
        ("ATR72-600", 2.114),
        ("A340-300", 2.90),
        ("B787-9", 3.34),
        ("DC-10", 3.40),
    ];
    for name in presets::available() {
        // A body whose section does not fit its skin fails to build.
        let g = geometry_of(name, None);
        if let Some((_, depth)) = drawn.iter().find(|(n, _)| *n == name) {
            assert!(
                (floor_depth(&g) - depth).abs() < 1.0e-9,
                "{name}: floor {:.4} m below the crown, drawn {depth} m",
                floor_depth(&g)
            );
        }
    }
}

#[test]
fn a_drawn_hold_keeps_its_polygon_and_the_skin_contains_it() {
    // Airbus AC A340 fig. 2-5-0-991-002: 1.71 m high, 3.18 m flat floor, 3.98 m
    // between the walls, 0.33 m chamfer, ceiling 3.27 m below the crown.
    let g = geometry_of("A340-300", None);
    let hold = g.hold.expect("the A340 hold is drawn");
    assert!((hold.height_m - 1.71).abs() < 1.0e-9);
    assert!((2.0 * hold.floor_half_m - 3.18).abs() < 1.0e-9);
    assert!((2.0 * hold.side_half_m - 3.98).abs() < 1.0e-9);
    assert!(hold.lining_m > 0.0, "the polygon stands inside the skin");
    let low = &g.lower_deck;
    let x = x_ref(&g);
    let ceiling = g.zc_at(x) + 0.5 * g.height_at(x) - 3.27;
    assert!((g.ceil_z(low, x) - ceiling).abs() < 1.0e-6);
    assert!((g.ceil_z(low, x) - g.floor_z(low, x) - 1.71).abs() < 1.0e-6);
    // The flat floor is as wide as drawn, the side walls as drawn higher up.
    assert!((g.usable_width(low, x) - 3.18).abs() < 1.0e-9);
    assert!((g.usable_width_at_z(x, g.floor_z(low, x) + 1.0) - 3.98).abs() < 1.0e-9);
    // A point outside the skin-less polygon is outside the hold.
    assert!(g.contains_point(x, 1.5, g.floor_z(low, x) + 0.1));
    assert!(!g.contains_point(x, 1.7, g.floor_z(low, x) + 0.01));
}

#[test]
fn the_a220_forward_hold_is_as_deep_as_its_drawing() {
    // Airbus A220 ACP issue 013 fig. 8 sheet 3, cargo compartment section
    // A-A: 1.05 m from floor to ceiling, 0.84 m flat floor. (Clearance
    // section D-D's 0.71 m runs from the ceiling to a side-wall point, not to
    // the floor.)
    let g = geometry_of("A220-300", None);
    let hold = g.hold.expect("the A220 hold is drawn");
    assert!((hold.height_m - 1.05).abs() < 1.0e-9);
    assert!((2.0 * hold.floor_half_m - 0.84).abs() < 1.0e-9);
}

#[test]
fn a_body_without_a_drawing_takes_the_class_hold_and_a_turboprop_has_none() {
    // C919: no public section drawing; the single-aisle class hold is the
    // 46 in (1.168 m) of the LD-3-45W (Boeing TBC-LL-0035F).
    let g = geometry_of("C919", None);
    let hold = g.hold.expect("a single-aisle body has a class hold");
    assert!((hold.height_m - 1.168).abs() < 1.0e-9);
    // The ATR 72 is high-wing with the baggage on the main deck: the class
    // section leaves no room under its floor.
    assert!(geometry_of("ATR72-600", None).hold.is_none());
}

#[test]
fn the_class_section_is_what_a_body_without_the_data_gets() {
    let drawn = geometry_of("A340-300", None);
    let class = geometry_of("A340-300", Some(None));
    // Same body, drawing withheld: the survey ratio of 0.57 of the section
    // height and the class hold of 0.278 of it.
    assert!((floor_depth(&class) - 0.57 * 5.64).abs() < 1.0e-9);
    let hold = class.hold.expect("the class hold");
    assert!((hold.height_m - 0.285 * 5.64).abs() < 1.0e-9);
    assert!((floor_depth(&drawn) - floor_depth(&class)).abs() > 0.1);
}

#[test]
fn a_section_that_does_not_fit_the_skin_is_refused() {
    let wide = CabinSectionConfig {
        floor_below_crown_m: Some(2.9),
        cabin_widths: None,
        hold: Some(HoldSectionConfig {
            ceiling_below_crown_m: Some(3.27),
            height_m: 1.71,
            floor_width_m: 5.0,
            side_width_m: 5.4,
            chamfer_height_m: 0.33,
        }),
    };
    let preset = presets::get("A340-300").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A340-300" })).unwrap();
    config.geometry.fuselage.cabin_section = Some(wide);
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    assert!(product_cabin_geometry(&plane, &config).is_err());
}

#[test]
fn the_configuration_validation_names_a_section_that_is_not_physical() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A340-300" })).unwrap();
    assert!(alas_config::validate(&config)
        .iter()
        .all(|issue| issue.field_path != "geometry.fuselage.cabin_section"));
    config.geometry.fuselage.cabin_section = Some(CabinSectionConfig {
        floor_below_crown_m: Some(-1.0),
        cabin_widths: None,
        hold: None,
    });
    assert!(alas_config::validate(&config)
        .iter()
        .any(|issue| issue.field_path == "geometry.fuselage.cabin_section"));
}

#[test]
fn the_lofted_outer_width_is_the_printed_one_on_every_drawn_body() {
    // Outer widths the manufacturers print on their sections: A320 3.95 m
    // (AC 2-7-0 section A-A), A340 5.64 m (AC fig. 2-5-0-991-002), 787 5.77 m
    // (D6-58333 fig. 2.5.1), DC-10 237 in (ACAP fig. 2.5.3), E195-E2 3.01 m
    // (APM fig. 2.20).
    for (name, printed) in [
        ("A320-200", 3.95),
        ("A340-300", 5.64),
        ("B787-9", 5.77),
        ("DC-10", 6.02),
        ("E195-E2", 3.01),
    ] {
        let g = geometry_of(name, None);
        let width = g.width_at(x_ref(&g));
        assert!(
            (width - printed).abs() <= 0.02,
            "{name}: lofted {width:.3} m, printed {printed} m"
        );
    }
}

#[test]
fn the_lofted_a320_widest_line_sits_lower_than_the_drawn_one_by_the_recorded_amount() {
    // AC A320 fig. 2-5-0-991-005 (vector): crown 243.1 pt, widest line 397 pt on
    // a 3.95 m = 307.5 pt width, so the widest line is 154 / 77.85 = 1.978 m
    // below the crown; the loft's ellipse puts it at half the 4.14 m height,
    // 2.07 m. The 0.09 m disagreement is the loft section's; this
    // pins its size so that a loft that takes the drawn section tightens it.
    let g = geometry_of("A320-200", None);
    let x = x_ref(&g);
    let loft = 0.5 * g.height_at(x);
    let drawn = (397.0 - 243.1) / (307.5 / 3.95);
    assert!(
        (loft - drawn - 0.09).abs() < 0.01,
        "loft {loft:.3} m, drawn {drawn:.3} m"
    );
}

#[test]
fn the_787_hold_takes_the_ld3_it_carries_and_the_a320_forward_hold_is_the_drawn_one() {
    // D6-58333 2.6.2 shows two LD-3 across (1.626 m high); the hold is drawn at
    // that scale 1.695 m high and 3.17 m across the floor.
    let g = geometry_of("B787-9", None);
    let hold = g.hold.expect("the 787 hold is drawn");
    // The drawn polygon stood 0.07 to 0.15 m outside the lining at its lower
    // corners; bringing it inside raises the floor to the cabin floor's beam
    // depth, 1.6246 m of height, one drawing pixel (0.014 m) at most short of
    // the container, which a source that prints its rows allows.
    assert!(hold.height_m >= 1.626 - 0.014, "{}", hold.height_m);
    assert!(
        2.0 * hold.floor_half_m >= 2.0 * 1.562,
        "two LD-3 bases across"
    );
    let preset = presets::get("B787-9").unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "B787-9" })).unwrap();
    let manager = alas_payload::CargoLoadManager::new(
        &g,
        alas_config::CargoDeckConfig {
            use_main_deck: false,
            ..config.cabin.cargo.clone()
        },
    );
    assert_eq!(manager.lower_uld.code, "AKE", "{}", preset.name);
    // Printed (D6-58333 2.6.2): 20 forward + 16 aft LD-3, two across, so 10 and 8 rows. The declared holds
    // carry those row counts and the rows share each compartment at 15.72 m / 10 = 1.572 m. Before, the
    // loader's unsourced 0.08 m row clearance (pitch 1.64 m) fitted 9 forward rows (18 containers), and the
    // nose refit of the loft then took the first row too: the hold at its printed start (7.43 m) is
    // 1.627 m high against the 1.63 m container, 0.003 m short (1.641 m on the integration loft). A
    // source that prints its rows lets a hold stand 0.014 m (one drawing pixel) short of the container.
    let count = |prefix: &str| {
        manager
            .slots
            .iter()
            .filter(|slot| slot.sid.starts_with(prefix))
            .count()
    };
    assert_eq!((count("FWD-"), count("AFT-")), (20, 16));
    assert_eq!(
        manager.slots.len(),
        37,
        "20 + 16 containers and the bulk position"
    );

    // AC A320 fig. 2-6-0-991-003: forward compartment 4.95 m (FR24A-FR34),
    // three LD3-45 positions in it.
    let g = geometry_of("A320-200", None);
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    let manager = alas_payload::CargoLoadManager::new(
        &g,
        alas_config::CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "LD3-45".to_owned(),
            ..config.cabin.cargo.clone()
        },
    );
    let forward = manager
        .slots
        .iter()
        .filter(|slot| slot.sid.starts_with("FWD-"))
        .count();
    assert_eq!(forward, 3);
}

/// The widest row a manufacturer's own drawn cabin carries, against the width
/// the production seat rule (`row_chord_over`) gives the same section at mid
/// cabin: positive when the drawn row is wider than the rule allows, m.
fn drawn_row_excess(g: &CabinGeometry, seats_abreast: f64, seat_w: f64, aisle_w: f64) -> f64 {
    let x = x_ref(g);
    let deck = &g.passenger_decks[0];
    let allowed = g.row_chord_over(deck, x - 0.5, x + 0.5, seat_w);
    seats_abreast * seat_w + aisle_w - allowed
}

/// The A320 with its drawn shoulder-height lining width replaced.
fn a320_with_shoulder_width(shoulder_m: f64) -> CabinGeometry {
    let mut section = presets::get("A320-200")
        .unwrap()
        .geometry
        .fuselage
        .cabin_section
        .unwrap();
    section.cabin_widths.as_mut().unwrap().shoulder_m = shoulder_m;
    geometry_of("A320-200", Some(Some(section)))
}

/// Reach of an occupant's outboard shoulder beyond the production lining's
/// half-width at that height, at the A320's window seat (outboard seat centre
/// 1.565 m from the axis in the drawn 3.63 m row of 0.50 m units), m.
fn a320_reach_over_lining(g: &CabinGeometry, height: f64, half_bideltoid: f64) -> f64 {
    let x = x_ref(g);
    let floor = g.floor_z(&g.passenger_decks[0], x);
    0.5 * 3.63 - 0.25 + half_bideltoid
        - 0.5 * g.lining_chord_at_z(x, floor + height) * g.lining_scale[1]
}

#[test]
fn the_a320_shoulder_lining_is_the_figure_scaled_to_the_published_maximum() {
    // The A320 lining at shoulder height is not published. Airbus prints the 3.70 m
    // maximum cabin width; on AC A320 fig. 2-5-0-991-005 (`a320_lining_ratio.py`: 8x
    // render, lining band inner edge per row, floor top read from the band) the inner
    // width is flat (within 0.5 %) between 0.387 and 0.660 m above the floor and reads
    // 0.9996 of the widest at the 0.635 m armrest, 0.9541 at 1.034 m and 0.9382 at
    // 1.123 m. The figure's own scale is good to 0.8 % (its outer skin reads 3.98 m
    // against 3.95 m, its seat extent 3.64-3.65 m against 3.63 m), so the 0.09 m by
    // which its widest line (3.79 m) exceeds the published 3.70 m is a difference in
    // what the drawn lining surface is, not a scale error. Two reconciliations that do
    // not use the absolute scale agree: proportional, 3.70 x 0.9541 = 3.530 m, and a
    // constant offset, 3.615 - (3.790 - 3.70) = 3.526 m. The earlier pixel reading of
    // 3.615 m is withdrawn because it ignores the 3.70 m; keeping it would mean
    // disbelieving Airbus's published maximum (it needs about 3.586 m, at which the
    // median woman would clear by 0.003 m): that is the sensitivity of the shoulder rule.
    // Armrest: 3.698 m, so the shipped 3.70 m is a measurement. Uncertainty of the
    // 3.53 m (full width, linear sum): 0.0064 m from +-0.01 m of height times the 0.64
    // slope of width against height, 0.003 m of edge (2 px), 0.004 m between the two
    // reconciliations and 0.005 m from the 0.8 % scale of the heights (S = 614.9 px/m:
    // 0.008 m of height at 1.034 m): 0.018 m of width, 0.009 m of reach.
    // The shoulder rule is the highest ANSUR II
    // percentile the drawn A320 passes at the window seat: the 5th-percentile woman
    // (shoulders 0.515 m above the seat, half-bideltoid 0.203 m). The median woman
    // (0.564 m, 0.225 m) is over by 0.025 m and the 95th-percentile man (0.653 m,
    // 0.2835 m) by 0.120 m; both results are recorded here.
    let width_tolerance = 0.018;
    let g = a320_with_shoulder_width(3.53);
    let excess = drawn_row_excess(&g, 6.0, 0.50, 0.63);
    let man = a320_reach_over_lining(&g, 1.123, 0.2835);
    let median_woman = a320_reach_over_lining(&g, 1.034, 0.225);
    let rule_woman = a320_reach_over_lining(
        &g,
        alas_payload::geometry::SHOULDER_ABOVE_FLOOR_M,
        alas_payload::geometry::SHOULDER_HALF_BREADTH_M,
    );
    // The rule's shoulder height (0.985 m) is read through the lofted lining from the
    // 1.034 m the drawn width was measured at; the figure's own ratio at 0.985 m
    // (0.9618, 3.559 m) gives the reach it should reproduce: 1.565 + 0.203 - 1.7795.
    let measured_rule_woman = 0.5 * 3.63 - 0.25 + 0.203 - 0.5 * 3.559;
    assert!(
        (rule_woman - measured_rule_woman).abs() <= 0.5 * width_tolerance,
        "the production lining puts the 5th-percentile woman's reach at {rule_woman:.3} m, the figure's at {measured_rule_woman:.3} m"
    );
    println!(
        "A320 shoulder 3.53 m: drawn row {excess:+.3} m, 95th man {man:+.3} m, median woman {median_woman:+.3} m, 5th woman (the rule) {rule_woman:+.3} m"
    );
    // The 95th-percentile man is out by far more than the uncertainty.
    assert!(
        man > 0.5 * width_tolerance + 0.05,
        "95th-percentile man over by only {man:.3} m"
    );
    // The median woman is over by 0.025 m, to within half the width tolerance (reach
    // is a half-width): she is out, not within the measurement.
    assert!(
        (median_woman - 0.025).abs() <= 0.5 * width_tolerance,
        "median woman reach {median_woman:.3} m against 0.025 m"
    );
    // The rule's woman (5th percentile) clears the drawn window seat.
    assert!(
        rule_woman <= 0.0,
        "the 5th-percentile woman is over the A320 lining by {rule_woman:.3} m"
    );
    // The drawn row exceeds the rule by 0.057 m of width. The shoulder
    // leg is not what binds it (the rule's woman clears by 0.015 m): the remaining leg
    // is the seat back's, held to the lining at 1.10 m plus 2 x 0.048 m.
    assert!(
        (excess - 0.057).abs() <= width_tolerance,
        "the drawn row is {excess:.3} m wider than the rule"
    );
    // The A220 lining is a vector measurement (rounding of its printed row only,
    // 0.01 m of width): the drawn row clears the rule.
    let a220 = geometry_of("A220-300", None);
    let excess = drawn_row_excess(&a220, 5.0, 0.51, 0.56);
    println!("A220-300: the drawn row exceeds the rule by {excess:.3} m");
    assert!(
        excess <= 0.01,
        "A220-300: the drawn row is {excess:.3} m wider than the rule"
    );
}

#[test]
fn a_wide_seat_keeps_its_back_inside_the_lining() {
    // A seat back is the unit width less the armrest housing on each side
    // (0.048 m, AC A320 fig. 2-5-0-991-005: the outer backs span 3.534 m in the
    // 3.63 m row), so a 0.65 m business seat's back is 0.554 m wide, wider than
    // the occupant's shoulders: its overhang is -0.048 m, not -0.10 m.
    use alas_payload::geometry::{shoulder_overhang_m, SEAT_BACK_INSET_M};
    assert!((shoulder_overhang_m(0.65) + SEAT_BACK_INSET_M).abs() < 1.0e-12);
    assert!((shoulder_overhang_m(0.46) + 0.027).abs() < 1.0e-12);
    let g = geometry_of("B787-9", None);
    let x = x_ref(&g);
    let deck = &g.passenger_decks[0];
    let chord = g.row_chord_over(deck, x - 0.5, x + 0.5, 0.65);
    let at_shoulders = g.lining_chord_at_z(x, g.floor_z(deck, x) + 1.034) * g.lining_scale[1];
    assert!(
        chord <= at_shoulders + 2.0 * SEAT_BACK_INSET_M + 1.0e-9,
        "a 0.65 m row of {chord:.3} m against a shoulder-height lining of {at_shoulders:.3} m"
    );
}

/// The registered bodies whose cabins are held to the physical envelope.
const STRICT_BODIES: usize = 12;

#[test]
fn every_emitted_row_and_monument_clears_the_lining_at_its_own_heights() {
    use alas_payload::geometry::{
        ARMREST_ABOVE_FLOOR_M, SEAT_BACK_FULL_WIDTH_TOP_M, SEAT_BACK_INSET_M,
        SEAT_PAN_ABOVE_FLOOR_M, SHOULDER_ABOVE_FLOOR_M,
    };
    use alas_payload::{build_payload_layout, ItemKind, ItemMeta};
    let (mut bodies, mut rows, mut monuments, mut widest_seat) = (0, 0, 0, 0.0_f64);
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        let g = product_cabin_geometry(&plane, &config).unwrap();
        if !g.enforces_physical_envelope() {
            continue;
        }
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .unwrap_or_else(|e| panic!("{name}: the payload layout failed: {e:?}"));
        bodies += 1;
        let rows_before = rows;
        for item in &layout.items {
            let Some(deck) = g.passenger_decks.iter().find(|d| d.name == item.deck) else {
                continue;
            };
            let (x0, x1) = (item.x - 0.5 * item.length, item.x + 0.5 * item.length);
            let chord_at = |x: f64, height: f64, scale: f64| {
                g.lining_chord_at_z(x, g.floor_z(deck, x) + height) * scale
            };
            match (&item.kind, &item.meta) {
                (ItemKind::SeatRow, ItemMeta::Seat(meta)) => {
                    rows += 1;
                    widest_seat = widest_seat.max(meta.seat_w);
                    // The seated occupant's CG is not clamped by headroom (it was
                    // by `clamp_height`): it stays inside the row's occupied envelope.
                    let cg_above_floor = meta.cg_z - meta.floor_z;
                    assert!(
                        cg_above_floor <= SEAT_PAN_ABOVE_FLOOR_M + item.height + 1.0e-9,
                        "{name}: a row at x = {:.2} m has its occupant CG {cg_above_floor:.3} m above the floor, above its envelope",
                        item.x
                    );
                    let overhang = alas_payload::geometry::shoulder_overhang_m(meta.seat_w);
                    let [s0, s1] = g.lining_scale;
                    for x in [x0, x1] {
                        let armrest = chord_at(x, ARMREST_ABOVE_FLOOR_M, s0) - item.width;
                        let shoulder =
                            chord_at(x, SHOULDER_ABOVE_FLOOR_M, s1) - 2.0 * overhang - item.width;
                        let back = chord_at(x, SEAT_BACK_FULL_WIDTH_TOP_M, s1)
                            + 2.0 * SEAT_BACK_INSET_M
                            - item.width;
                        for (what, clearance) in
                            [("armrest", armrest), ("shoulder", shoulder), ("back", back)]
                        {
                            assert!(
                                clearance >= -1.0e-6,
                                "{name}: a row at x = {x:.2} m is {:.3} m wider than the lining at {what} height",
                                -clearance
                            );
                        }
                    }
                }
                (
                    ItemKind::Galley
                    | ItemKind::Lav
                    | ItemKind::AccessibleLav
                    | ItemKind::WheelchairStowage,
                    _,
                ) => {
                    // A monument stands against the lining: its outboard edge is
                    // inside the lining at the floor and at its own top (the production
                    // `contained_width_to` rule restated on the unscaled lofted lining,
                    // so this guards regressions, not the rule itself).
                    monuments += 1;
                    let half = item.y.abs() + 0.5 * item.width;
                    let top = item.height;
                    for x in [x0, item.x, x1] {
                        for height in [0.0, top] {
                            let chord = g.lining_chord_at_z(x, g.floor_z(deck, x) + height);
                            assert!(
                                2.0 * half <= chord + 1.0e-6,
                                "{name}: {:?} at x = {x:.2} m reaches {half:.3} m, the lining at {height:.2} m is {:.3} m wide",
                                item.kind,
                                chord
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        println!("{name}: {} seat rows", rows - rows_before);
    }
    println!("checked {bodies} bodies, {rows} seat rows, {monuments} monuments, widest seat {widest_seat:.3} m");
    assert_eq!(bodies, STRICT_BODIES, "the set of strict bodies changed");
    assert_eq!(
        (rows, monuments),
        // The A320-200 aft-body loft gives 391 seat rows (the shoulder offset k is not the cause). The loft keeps the fuselage wider through the aft taper (outer width
        // 3.726 m against 3.588 m at x = 29.5 m, armrest-height lining 3.424 against 3.283 m), so the last two
        // rows of a narrower cabin (2.81 m width at x = 29.6 and 30.5 m) are replaced by one full-width row
        // at x = 29.8 m, 25 rows against 26 (`hold_probe` prints the lining table).
        (391, 120),
        "the checked item counts changed"
    );
    assert!(
        widest_seat >= 0.55,
        "no emitted row of a 0.55 m class or wider was checked"
    );
}
