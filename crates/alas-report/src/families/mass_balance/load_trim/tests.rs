// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::sequences::potato_and_paths;
use super::*;
use crate::theme::{PALETTE_DARK, PALETTE_LIGHT};
use alas_payload::loading_sequence::{LoadingPoint, LoadingSequence};

const X_LEMAC: f64 = 15.26;
const MAC: f64 = 4.1935;

/// Ten 1 t items spread along an A320-sized cabin.
fn items() -> Vec<(f64, f64)> {
    (0..10)
        .map(|k| (1_000.0, 8.0 + 2.0 * f64::from(k)))
        .collect()
}

/// An A320-sized sheet: limits 17-40 %MAC, DOW 42 t at 26.5 %MAC.
fn sample() -> LoadTrimSheetData {
    let dow_x = X_LEMAC + 0.265 * MAC;
    let envelope = loading_envelope_polygon(42_000.0, dow_x, &items(), X_LEMAC, MAC);
    let zfw = envelope.iter().map(|p| p.0).fold(0.0, f64::max);
    let dow = LoadingPoint {
        mass_kg: 42_000.0,
        x_m: dow_x,
    };
    let path = |name: &str, order: Vec<(f64, f64)>| {
        let (mut mass, mut moment) = (dow.mass_kg, dow.mass_kg * dow.x_m);
        let mut points = vec![dow];
        for (m, x) in order {
            mass += m;
            moment += m * x;
            points.push(LoadingPoint {
                mass_kg: mass,
                x_m: moment / mass,
            });
        }
        LoadingSequence {
            name: name.to_owned(),
            points,
        }
    };
    let mut reversed = items();
    reversed.reverse();
    let composed = vec![path("fwd", items()), path("aft", reversed)];
    let (potato, sequences) = potato_and_paths(&composed, &[], |x| (x - X_LEMAC) / MAC * 100.0);
    let ground = vec![
        LimitVertex {
            mass_kg: 42_000.0,
            fwd_pct_mac: 5.0,
            aft_pct_mac: 45.0,
        },
        LimitVertex {
            mass_kg: 78_000.0,
            fwd_pct_mac: 5.0,
            aft_pct_mac: 45.0,
        },
    ];
    LoadTrimSheetData {
        title: "test".to_owned(),
        x_lemac_m: X_LEMAC,
        mac_m: MAC,
        index: BalanceIndex::for_aircraft(X_LEMAC, MAC, 78_000.0, 5.0, 45.0),
        ground_limits: ground,
        flight_limits: vec![
            LimitVertex {
                mass_kg: 52_000.0,
                fwd_pct_mac: 12.0,
                aft_pct_mac: 38.0,
            },
            LimitVertex {
                mass_kg: 78_000.0,
                fwd_pct_mac: 12.0,
                aft_pct_mac: 38.0,
            },
        ],
        landing_limits: vec![
            LimitVertex {
                mass_kg: 52_000.0,
                fwd_pct_mac: 14.0,
                aft_pct_mac: 45.0,
            },
            LimitVertex {
                mass_kg: 66_000.0,
                fwd_pct_mac: 14.0,
                aft_pct_mac: 45.0,
            },
        ],
        potato,
        sequences,
        takeoff_limits: vec![
            LimitVertex {
                mass_kg: 42_000.0,
                fwd_pct_mac: 17.0,
                aft_pct_mac: 40.0,
            },
            LimitVertex {
                mass_kg: 60_000.0,
                fwd_pct_mac: 18.0,
                aft_pct_mac: 40.0,
            },
            LimitVertex {
                mass_kg: 78_000.0,
                fwd_pct_mac: 22.0,
                aft_pct_mac: 37.0,
            },
        ],
        zfw_limits: vec![
            LimitVertex {
                mass_kg: 42_000.0,
                fwd_pct_mac: 19.0,
                aft_pct_mac: 35.0,
            },
            LimitVertex {
                mass_kg: 62_500.0,
                fwd_pct_mac: 19.0,
                aft_pct_mac: 35.0,
            },
        ],
        weight_lines: vec![
            WeightLine {
                role: MassRole::SizedTakeoff,
                mass_kg: 78_000.0,
            },
            WeightLine {
                role: MassRole::DesignLanding,
                mass_kg: 66_000.0,
            },
            WeightLine {
                role: MassRole::AnalyzedZeroFuel,
                mass_kg: 62_500.0,
            },
        ],
        fuel_curve: vec![(zfw, 27.0), (60_000.0, 29.0), (70_000.0, 28.0)],
        steps: vec![
            LoadStep {
                item: "Empty".to_owned(),
                state: "DOW".to_owned(),
                mass_kg: 42_000.0,
                pct_mac: 26.5,
                gate: None,
            },
            LoadStep {
                item: "Passengers".to_owned(),
                state: "ZFW".to_owned(),
                mass_kg: zfw,
                pct_mac: 27.0,
                gate: None,
            },
            LoadStep {
                item: "Fuel".to_owned(),
                state: "TOW".to_owned(),
                mass_kg: 70_000.0,
                pct_mac: 28.0,
                gate: None,
            },
        ],
    }
}

#[test]
fn the_index_is_linear_in_mass_along_a_constant_mac_line_and_k_at_the_reference() {
    let data = sample();
    let p = 31.0;
    let (a, b, c) = (
        data.index_at(40_000.0, p),
        data.index_at(60_000.0, p),
        data.index_at(80_000.0, p),
    );
    assert!(
        ((b - a) - (c - b)).abs() < 1e-9,
        "equal mass steps give equal index steps"
    );
    assert!((data.index_at(55_000.0, 25.0) - data.index.k).abs() < 1e-9);
    assert!(
        (data.index_at(0.0, p) - data.index.k).abs() < 1e-12,
        "every %MAC line passes through (K, 0)"
    );
    assert!((data.pct_at(data.index_at(51_234.0, 33.3), 51_234.0) - 33.3).abs() < 1e-9);
}

#[test]
fn the_scale_constant_keeps_the_band_within_forty_units_of_k() {
    let data = sample();
    for pct in [17.0, 40.0] {
        assert!((data.index_at(78_000.0, pct) - data.index.k).abs() <= 40.0 + 1e-9);
    }
    assert!(BalanceIndex::NICE_C.contains(&data.index.c_kg_m));
}

#[test]
fn limits_interpolate_between_vertices_and_hold_beyond_them() {
    let data = sample();
    assert!((limit_at(&data.takeoff_limits, 51_000.0, true) - 17.5).abs() < 1e-12);
    assert!((limit_at(&data.takeoff_limits, 10_000.0, false) - 40.0).abs() < 1e-12);
    assert!((limit_at(&data.takeoff_limits, 90_000.0, true) - 22.0).abs() < 1e-12);
}

#[test]
fn the_loading_envelope_contains_every_loading_order_and_its_chains_are_convex_in_index_space() {
    let data = sample();
    let n = items().len();
    let polygon = loading_envelope_polygon(42_000.0, X_LEMAC + 0.265 * MAC, &items(), X_LEMAC, MAC);
    // Forward chain = first n+1 points; its index slopes per kg increase.
    let fwd: Vec<(f64, f64)> = polygon[..=n]
        .iter()
        .map(|&(m, p)| (m, data.index_at(m, p)))
        .collect();
    let slopes: Vec<f64> = fwd
        .windows(2)
        .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
        .collect();
    assert!(
        slopes.windows(2).all(|s| s[1] >= s[0] - 1e-12),
        "forward chain convex"
    );
    // A few arbitrary orders stay between the chains at every mass.
    let dow_x = X_LEMAC + 0.265 * MAC;
    for order in [
        [0, 9, 1, 8, 2, 7, 3, 6, 4, 5],
        [5, 4, 6, 3, 7, 2, 8, 1, 9, 0],
    ] {
        let (mut mass, mut moment) = (42_000.0, 42_000.0 * dow_x);
        for (k, &item) in order.iter().enumerate() {
            let (m, x) = items()[item];
            mass += m;
            moment += m * x;
            let pct = (moment / mass - X_LEMAC) / MAC * 100.0;
            let lo = polygon[k + 1].1;
            let hi = polygon[2 * n - k - 1].1;
            assert!(
                pct >= lo - 1e-9 && pct <= hi + 1e-9,
                "order point {k}: {pct} not in [{lo}, {hi}]"
            );
        }
    }
}

#[test]
fn plotted_points_map_back_to_their_mass_and_cg() {
    let data = sample();
    let frame = frame_for(&data);
    for s in &data.steps {
        let (index, mass) = frame.unmap(frame.map(data.index_at(s.mass_kg, s.pct_mac), s.mass_kg));
        assert!((mass - s.mass_kg).abs() < 1e-6);
        assert!((data.pct_at(index, mass) - s.pct_mac).abs() < 1e-9);
    }
}

#[test]
fn the_sheet_renders_without_non_finite_coordinates_in_both_themes() {
    for pal in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let svg = crate::svg::render_svg(&figure_load_trim_sheet(&sample(), pal));
        assert!(
            !svg.contains("NaN") && !svg.contains("inf"),
            "{} theme",
            pal.name
        );
        assert!(svg.contains("LIMIT DEFINITIONS") && svg.contains("LOADING POINTS"));
        assert!(svg.contains("Sized TOW 78 000 kg") && svg.contains("%MAC"));
    }
}

fn polylines_with_dash(scene: &crate::scene::Scene, dash: Option<&[f64]>) -> usize {
    scene
        .elements
        .iter()
        .filter(|e| {
            matches!(e, crate::scene::SceneElement::Polyline { stroke, .. }
                if stroke.dash_array.as_deref() == dash && stroke.width > 1.5)
        })
        .count()
}

#[test]
fn every_limit_set_is_drawn_in_its_own_style() {
    let scene = figure_load_trim_sheet(&sample(), &PALETTE_LIGHT);
    // Two lines (forward, aft) per set present in the sample.
    assert_eq!(
        polylines_with_dash(&scene, Some(&[7.0, 4.0])),
        2,
        "ground dashed"
    );
    assert_eq!(
        polylines_with_dash(&scene, Some(&[1.5, 3.5])),
        2,
        "flight dotted"
    );
    assert_eq!(
        polylines_with_dash(&scene, Some(&[8.0, 3.0, 1.5, 3.0])),
        2,
        "landing dash-dot"
    );
    assert_eq!(polylines_with_dash(&scene, None), 2, "takeoff solid");
}

#[test]
fn the_potato_lies_inside_the_reorder_polygon_and_inside_the_ground_limits() {
    let data = sample();
    let polygon = loading_envelope_polygon(42_000.0, X_LEMAC + 0.265 * MAC, &items(), X_LEMAC, MAC);
    let n = items().len();
    let chain = |pts: &[(f64, f64)], m: f64| {
        let (a, b) = pts
            .windows(2)
            .find(|w| m >= w[0].0 - 1e-9 && m <= w[1].0 + 1e-9)
            .map(|w| (w[0], w[1]))
            .expect("mass inside chain");
        a.1 + (m - a.0) / (b.0 - a.0) * (b.1 - a.1)
    };
    let fwd = &polygon[..=n];
    let mut aft: Vec<(f64, f64)> = polygon[n..].to_vec();
    aft.reverse();
    assert!(!data.potato.is_empty());
    for level in &data.potato {
        let (lo, hi) = (chain(fwd, level.mass_kg), chain(&aft, level.mass_kg));
        assert!(
            level.fwd_pct_mac >= lo - 0.05 && level.aft_pct_mac <= hi + 0.05,
            "potato at {} kg outside the reorder polygon",
            level.mass_kg
        );
    }
    assert_eq!(data.potato_ground_exceedance_pct_mac(), 0.0);
    let mut tight = data.clone();
    tight.ground_limits[0].aft_pct_mac = 26.0;
    tight.ground_limits[1].aft_pct_mac = 26.0;
    assert!(tight.potato_ground_exceedance_pct_mac() > 0.0);
}

#[test]
fn the_sheet_omits_loading_envelope_and_prose_but_keeps_numbered_states() {
    let data = sample();
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let svg = crate::svg::render_svg(&scene);
    assert!(!svg.contains("Each item adds") && !svg.contains("Index I ="));
    assert!(!svg.contains("Boarding potato") && !svg.contains("One composed order"));
    for step in 1..=data.steps.len() {
        assert!(scene.elements.iter().any(|element| matches!(
            element, crate::scene::SceneElement::Text { text, .. }
                if text == &step.to_string()
        )));
    }
    let mut no_envelope = data.clone();
    no_envelope.potato.clear();
    no_envelope.sequences.clear();
    // Loading orders are computational data; none contribute rendered paths.
    let count_paths = |scene: &crate::scene::Scene| {
        scene
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    crate::scene::SceneElement::Polyline { .. }
                        | crate::scene::SceneElement::Polygon { .. }
                )
            })
            .count()
    };
    assert_eq!(
        count_paths(&scene),
        count_paths(&figure_load_trim_sheet(&no_envelope, &PALETTE_LIGHT))
    );
}

#[test]
fn coincident_model_masses_preserve_provenance_in_one_label_below_the_cap() {
    for takeoff_role in [MassRole::SizedTakeoff, MassRole::AnalyzedTakeoff] {
        let takeoff_label = takeoff_role.label();
        let mut data = sample();
        let takeoff_mass = data.weight(MassRole::SizedTakeoff).unwrap();
        data.weight_lines[0].role = takeoff_role;
        data.weight_lines
            .iter_mut()
            .find(|line| line.role == MassRole::DesignLanding)
            .unwrap()
            .mass_kg = takeoff_mass;
        let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
        let frame = frame_for(&data);
        let cap_y = frame.map(frame.i_range.0, takeoff_mass)[1];
        let labels: Vec<_> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                crate::scene::SceneElement::Text {
                    text,
                    pos,
                    baseline,
                    ..
                } if text.contains(takeoff_label) || text.contains("Design LW") => {
                    Some((text, pos, baseline))
                }
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 1);
        assert!(labels[0]
            .0
            .contains(&format!("{takeoff_label} / Design LW")));
        assert!(labels[0].1[1] > cap_y);
        assert_eq!(*labels[0].2, crate::scene::TextBaseline::Top);
    }
}
#[test]
fn hidden_loading_envelopes_do_not_change_the_frame_or_rendered_sheet() {
    let data = sample();
    let expected = crate::svg::render_svg(&figure_load_trim_sheet(&data, &PALETTE_LIGHT));
    let mut changed = data.clone();
    changed.potato = vec![PotatoLevel {
        mass_kg: 1.0e9,
        fwd_pct_mac: -1000.0,
        aft_pct_mac: 1000.0,
    }];
    changed.sequences = vec![NamedPath {
        name: "Extreme hidden order".to_owned(),
        points: vec![(1.0e9, -1000.0), (2.0e9, 1000.0)],
    }];
    assert_eq!(frame_for(&data).i_range, frame_for(&changed).i_range);
    assert_eq!(frame_for(&data).w_range_kg, frame_for(&changed).w_range_kg);
    assert_eq!(
        expected,
        crate::svg::render_svg(&figure_load_trim_sheet(&changed, &PALETTE_LIGHT))
    );
    changed.potato.clear();
    changed.sequences.clear();
    assert_eq!(
        expected,
        crate::svg::render_svg(&figure_load_trim_sheet(&changed, &PALETTE_LIGHT))
    );
}

#[test]
fn visible_fuel_curve_and_phase_limits_fit_the_frame() {
    let mut data = sample();
    data.fuel_curve.push((90_000.0, 70.0));
    data.flight_limits.push(LimitVertex {
        mass_kg: 85_000.0,
        fwd_pct_mac: -30.0,
        aft_pct_mac: 75.0,
    });
    let frame = frame_for(&data);
    for (mass, pct) in [(90_000.0, 70.0), (85_000.0, -30.0), (85_000.0, 75.0)] {
        let index = data.index_at(mass, pct);
        assert!(mass >= frame.w_range_kg.0 && mass <= frame.w_range_kg.1);
        assert!(index >= frame.i_range.0 && index <= frame.i_range.1);
    }
}

#[test]
fn missing_payload_layout_preserves_assessed_zero_fuel_mass_and_cg() {
    use alas_config::{design_variables::DesignVector, AlasConfig};
    use alas_mass::breakdown::{FUEL, PAYLOAD};

    let mut config = AlasConfig::default();
    let design = DesignVector::default();
    alas_payload::apply_cabin_preset(&mut config, Some(&design)).unwrap();
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let mut report = super::super::quick_preview_report(airplane, &config, design).unwrap();
    // A finite synthetic neutral point enables this mass-accounting fixture;
    // the test does not validate aerodynamic limits.
    report.x_neutral_point = report.physical_cg[0] + report.airplane.c_ref;
    report.payload_layout = None;
    assert!(report.component_masses[PAYLOAD] > 0.0);
    let data = super::data::load_trim_data_from_report(&report, &config).unwrap();
    let step = |name: &str| data.steps.iter().find(|s| s.state == name).unwrap();
    let expected_zfw: f64 = report
        .component_masses
        .iter()
        .filter(|(name, _)| name.as_str() != FUEL)
        .map(|(_, mass)| mass)
        .sum();
    let expected_moment: f64 = report
        .component_masses
        .iter()
        .filter(|(name, _)| name.as_str() != FUEL)
        .map(|(name, mass)| mass * report.mass_coordinates[name][0])
        .sum();
    let frame = report.airplane.mac_frame().unwrap();
    assert!((step("ZFW").mass_kg - expected_zfw).abs() < 1e-9);
    assert!((step("ZFW").pct_mac - frame.pct_mac(expected_moment / expected_zfw)).abs() < 1e-9);
    assert!(
        (step("ZFW").mass_kg - step("DOW").mass_kg - report.component_masses[PAYLOAD]).abs() < 1e-9
    );
    assert!(
        (step("TOW").mass_kg - step("ZFW").mass_kg - report.component_masses[FUEL]).abs() < 1e-9
    );
    assert_eq!(
        data.weight(MassRole::AnalyzedZeroFuel),
        Some(step("ZFW").mass_kg)
    );
    assert!(data.fuel_curve.is_empty() && data.sequences.is_empty());
    assert_eq!(
        data.weight(MassRole::AnalyzedTakeoff),
        Some(step("TOW").mass_kg)
    );
    assert!(data.weight(MassRole::SizedTakeoff).is_none());
    assert_eq!(
        data.steps
            .iter()
            .map(|step| step.state.as_str())
            .collect::<Vec<_>>(),
        ["DOW", "ZFW", "TOW", "LW"]
    );
}

#[test]
fn phase_limit_index_extrema_between_vertices_fit_the_frame() {
    let mut data = sample();
    data.flight_limits = vec![
        LimitVertex {
            mass_kg: 10_000.0,
            fwd_pct_mac: 325.0,
            aft_pct_mac: 326.0,
        },
        LimitVertex {
            mass_kg: 90_000.0,
            fwd_pct_mac: 25.0,
            aft_pct_mac: 26.0,
        },
    ];
    let frame = frame_for(&data);
    for mass in (10_000..=90_000).step_by(1000) {
        for forward in [true, false] {
            let index = data.index_at(
                f64::from(mass),
                limit_at(&data.flight_limits, f64::from(mass), forward),
            );
            assert!(index >= frame.i_range.0 && index <= frame.i_range.1);
        }
    }
}

#[test]
fn model_mass_annotations_do_not_claim_structural_maxima() {
    let data = sample();
    for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let svg = crate::svg::render_svg(&figure_load_trim_sheet(&data, palette));
        for label in ["Sized TOW", "Design LW", "Analyzed ZFW"] {
            assert!(svg.contains(label), "missing semantic mass label: {label}");
        }
        for label in ["MTOW", "MLW", "MZFW"] {
            assert!(
                !svg.contains(label),
                "model mass is not a structural maximum: {label}"
            );
        }
        assert!(!svg.contains("Published reference") && !svg.contains("Limits are"));
    }
}

#[test]
fn mass_annotation_backplates_fit_localized_labels_and_leave_mass_lines_visible() {
    use crate::scene::{
        Color, SceneElement, CONSERVATIVE_ADVANCE_EM, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
    };

    let mut data = sample();
    let takeoff_mass = data.weight(MassRole::SizedTakeoff).unwrap();
    for line in &mut data.weight_lines {
        line.mass_kg = takeoff_mass;
    }
    let translated = format!(
        "TOW dimensionada / LW de dise\u{f1}o / ZFW analizada {} kg",
        super::panel::kg(takeoff_mass)
    );
    for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let scene = figure_load_trim_sheet(&data, palette);
        let frame = frame_for(&data);
        let line_y = frame.map(frame.i_range.0, takeoff_mass)[1];
        let (index, font_size, position) = scene
            .elements
            .iter()
            .enumerate()
            .find_map(|(index, element)| match element {
                SceneElement::Text {
                    text,
                    font_size,
                    pos,
                    ..
                } if text.contains("Sized TOW /") => Some((index, *font_size, *pos)),
                _ => None,
            })
            .unwrap();
        let SceneElement::Rect {
            x,
            y,
            width,
            height,
            fill: Some(fill),
            ..
        } = &scene.elements[index - 1]
        else {
            panic!("mass annotation requires a backplate");
        };
        assert_eq!(fill.color, Color::from_hex(palette.bg));
        let translated_width = translated.chars().count() as f64
            * font_size
            * CSS_PIXELS_PER_POINT
            * CONSERVATIVE_ADVANCE_EM;
        assert!(position[0] >= *x && position[0] + translated_width <= x + width);
        assert!(
            position[1] >= *y
                && position[1] + font_size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM
                    <= y + height
        );
        assert!(*x >= frame.left && x + width <= frame.right());
        assert!(*y > line_y && y + height <= frame.bottom());
    }
}

#[test]
fn zero_fuel_backplate_stays_above_its_mass_line_in_both_themes() {
    use crate::scene::{
        SceneElement, CONSERVATIVE_ADVANCE_EM, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
    };
    let data = sample();
    let mass = data.weight(MassRole::AnalyzedZeroFuel).unwrap();
    let translated = format!("ZFW analizada {} kg", super::panel::kg(mass));
    for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let scene = figure_load_trim_sheet(&data, palette);
        let frame = frame_for(&data);
        let line_y = frame.map(frame.i_range.0, mass)[1];
        let (index, size, position) = scene
            .elements
            .iter()
            .enumerate()
            .find_map(|(index, element)| match element {
                SceneElement::Text {
                    text,
                    font_size,
                    pos,
                    ..
                } if text.starts_with("Analyzed ZFW ") => Some((index, *font_size, *pos)),
                _ => None,
            })
            .unwrap();
        let SceneElement::Rect {
            x,
            y,
            width,
            height,
            ..
        } = &scene.elements[index - 1]
        else {
            panic!("zero-fuel annotation requires a backplate");
        };
        assert!(
            position[0]
                + translated.chars().count() as f64
                    * size
                    * CSS_PIXELS_PER_POINT
                    * CONSERVATIVE_ADVANCE_EM
                <= x + width
        );
        assert!(position[1] - size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM >= *y);
        assert!(position[1] <= y + height && y + height < line_y);
        assert!(*x >= frame.left && x + width <= frame.right() && *y >= frame.top);
    }
}

/// Rectangles of the panel boxes: paper-filled, stroked, right of the chart
/// and wider than the key swatch.
fn panel_boxes(scene: &crate::scene::Scene, frame: &Frame) -> Vec<[f64; 4]> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            crate::scene::SceneElement::Rect {
                x,
                y,
                width,
                height,
                stroke: Some(_),
                fill: Some(_),
                ..
            } if *x > frame.right() && *width > 100.0 => Some([*x, *y, x + width, y + height]),
            _ => None,
        })
        .collect()
}

#[test]
fn the_side_panel_spans_the_chart_height_for_any_number_of_states() {
    for states in 1..=9 {
        let mut data = sample();
        let template = data.steps[1].clone();
        data.steps.truncate(1);
        for k in 1..states {
            let mut step = template.clone();
            step.state = format!("S{k}");
            step.mass_kg = 42_000.0 + 3_000.0 * k as f64;
            data.steps.push(step);
        }
        for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
            let scene = figure_load_trim_sheet(&data, palette);
            let frame = frame_for(&data);
            let boxes = panel_boxes(&scene, &frame);
            assert_eq!(boxes.len(), 3, "{states} states");
            assert!((boxes[0][1] - frame.top).abs() < 0.5);
            assert!(
                (boxes[2][3] - frame.bottom()).abs() < 0.5,
                "{states} states: panel ends at {} for a chart bottom at {}",
                boxes[2][3],
                frame.bottom()
            );
            for pair in boxes.windows(2) {
                assert!(pair[0][3] < pair[1][1], "boxes overlap: {pair:?}");
            }
            for rect in &boxes {
                assert!(rect[2] <= scene.width && rect[3] <= scene.height);
            }
            // No panel text leaves its box.
            for element in &scene.elements {
                if let crate::scene::SceneElement::Text { pos, .. } = element {
                    if pos[0] > frame.right() + 20.0 {
                        assert!(
                            boxes.iter().any(|b| pos[0] >= b[0]
                                && pos[0] <= b[2]
                                && pos[1] >= b[1]
                                && pos[1] <= b[3]),
                            "panel text at {pos:?} outside the boxes"
                        );
                    }
                }
            }
        }
    }
}

fn alert_rings(scene: &crate::scene::Scene, palette: &crate::theme::Palette) -> Vec<Point2D> {
    let alert = super::render::Ink::for_palette(palette).alert;
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            crate::scene::SceneElement::Circle {
                center,
                stroke: Some(stroke),
                fill: None,
                ..
            } if stroke.color == alert => Some(*center),
            _ => None,
        })
        .collect()
}

#[test]
fn every_state_the_gate_failed_and_only_those_is_ringed_on_the_chart() {
    use alas_opt::ModelCgLoadingState as State;
    let gate = |state, violated| {
        Some(StepGate {
            state,
            phase: GatePhase::of(state),
            cg_pct_mac: 27.0,
            fwd_pct_mac: 15.0,
            aft_pct_mac: 40.0,
            violated,
        })
    };
    let mut inside = sample();
    inside.steps[0].gate = gate(State::OperatingEmpty, false);
    inside.steps[2].gate = gate(State::AnalyzedTakeoff, false);
    for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
        assert!(alert_rings(&figure_load_trim_sheet(&inside, palette), palette).is_empty());
    }
    // The ring follows the gate's verdict, not the plotted point: a CG drawn
    // well outside the limits stays unringed while the gate passes it, and
    // a state with no verdict (step 2) is never ringed.
    let mut outside = inside.clone();
    outside.steps[1].pct_mac = 60.0;
    outside.steps[0].gate = gate(State::OperatingEmpty, true);
    outside.steps[2].gate = gate(State::AnalyzedTakeoff, true);
    for palette in [&PALETTE_LIGHT, &PALETTE_DARK] {
        let scene = figure_load_trim_sheet(&outside, palette);
        let frame = frame_for(&outside);
        let rings = alert_rings(&scene, palette);
        for (k, step) in outside.steps.iter().enumerate() {
            let at = frame.map(outside.index_at(step.mass_kg, step.pct_mac), step.mass_kg);
            let ringed = rings
                .iter()
                .any(|c| (c[0] - at[0]).hypot(c[1] - at[1]) < 1e-6);
            assert_eq!(ringed, k != 1, "state {} ({})", k + 1, step.state);
        }
        // The key explains the ring only when one is drawn.
        let svg = crate::svg::render_svg(&scene);
        assert!(svg.contains(super::panel::KEY_OUTSIDE));
    }
    let svg = crate::svg::render_svg(&figure_load_trim_sheet(&inside, &PALETTE_LIGHT));
    assert!(!svg.contains(super::panel::KEY_OUTSIDE));
}

#[test]
fn mass_label_reservations_cover_both_shipped_languages() {
    for (role, spanish) in [
        (MassRole::SizedTakeoff, "TOW dimensionada"),
        (MassRole::AnalyzedTakeoff, "TOW analizada"),
        (MassRole::DesignLanding, "LW de dise\u{f1}o"),
        (MassRole::AnalyzedZeroFuel, "ZFW analizada"),
    ] {
        let longest = role.label().chars().count().max(spanish.chars().count());
        assert!(role.reserved_chars() >= longest, "{role:?}");
    }
}

#[test]
fn a_state_on_its_limit_reads_a_zero_margin_without_a_sign() {
    assert_eq!(super::panel::signed_tenths(-0.04), "0.0");
    assert_eq!(super::panel::signed_tenths(0.04), "0.0");
    assert_eq!(super::panel::signed_tenths(-0.06), "-0.1");
    assert_eq!(super::panel::signed_tenths(2.26), "+2.3");
}

/// Mass-label backplates of `scene`, `[x0, y0, x1, y1]`, in drawing order.
fn label_plates(scene: &crate::scene::Scene) -> Vec<[f64; 4]> {
    scene
        .elements
        .windows(2)
        .filter_map(|pair| match pair {
            [crate::scene::SceneElement::Rect {
                x,
                y,
                width,
                height,
                stroke: None,
                ..
            }, crate::scene::SceneElement::Text { text, .. }]
                if text.ends_with(" kg") =>
            {
                Some([*x, *y, x + width, y + height])
            }
            _ => None,
        })
        .collect()
}

#[test]
fn mass_labels_keep_clear_of_every_mass_line_and_of_each_other() {
    // Spread lines, then a landing mass 1 % under takeoff, then all three
    // within a few hundred kilograms: the labels must move, not overlap.
    for (landing, zero_fuel) in [
        (66_000.0, 62_500.0),
        (77_200.0, 62_500.0),
        (77_600.0, 77_200.0),
    ] {
        let mut data = sample();
        for line in &mut data.weight_lines {
            match line.role {
                MassRole::DesignLanding => line.mass_kg = landing,
                MassRole::AnalyzedZeroFuel => line.mass_kg = zero_fuel,
                _ => {}
            }
        }
        let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
        let frame = frame_for(&data);
        let plates = label_plates(&scene);
        assert_eq!(plates.len(), 3);
        for (k, plate) in plates.iter().enumerate() {
            assert!(plate[0] >= frame.left && plate[2] <= frame.right());
            assert!(plate[1] >= frame.top && plate[3] <= frame.bottom());
            for line in &data.weight_lines {
                let y = frame.map(frame.i_range.0, line.mass_kg)[1];
                assert!(
                    plate[3] <= y - 5.0 || plate[1] >= y + 5.0,
                    "label {k} {plate:?} crowds the line at y = {y} ({landing}, {zero_fuel})"
                );
            }
            for other in &plates[..k] {
                assert!(
                    plate[2] <= other[0]
                        || other[2] <= plate[0]
                        || plate[3] <= other[1]
                        || other[3] <= plate[1],
                    "labels overlap: {plate:?} {other:?}"
                );
            }
        }
    }
}

#[test]
fn a_gate_phase_has_the_mechanisms_the_gate_scopes_to_its_state() {
    use alas_opt::envelope::PhaseLimits;
    use alas_opt::ModelCgLoadingState as State;
    for state in [
        State::OperatingEmpty,
        State::AnalyzedZeroFuel,
        State::OperationalMidMission,
        State::OperationalReserve,
        State::AnalyzedTakeoff,
        State::AnalyzedLanding,
    ] {
        assert_eq!(
            GatePhase::of(state).limits(),
            PhaseLimits::for_state(state),
            "{state:?}"
        );
    }
}

/// The ringed states of a run's sheet are exactly the gated states the
/// run's own CG gate failed, on a passing and a failing preset.
#[test]
fn the_ringed_states_are_the_states_the_run_gate_failed() {
    use alas_opt::ModelCgLoadingState as State;
    let mut outcomes = Vec::new();
    for preset in ["A320-200", "ATR72-600"] {
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": preset}))
            .expect("registered preset");
        let options = alas_pipeline::PipelineOptions {
            optimize: false,
            compare_baseline: false,
            quiet: true,
            seed: Some(42),
            ..Default::default()
        };
        let result = alas_pipeline::DesignPipeline::new(config)
            .run(&options, &alas_pipeline::RunEnvironment::default())
            .expect("analysis run");
        let gate = result.feasibility.model_cg.as_ref().expect("CG gate");
        let failed = |state: State| {
            gate.loading_states
                .iter()
                .find(|s| s.state == state)
                .is_some_and(|s| {
                    s.constraints
                        .iter()
                        .any(|c| c.violated && !c.constraint.is_diagnostic())
                })
        };
        let data = super::data::load_trim_data_from_pipeline(&result).expect("sheet data");
        let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
        let frame = frame_for(&data);
        let rings = alert_rings(&scene, &PALETTE_LIGHT);
        let mut any_failed = false;
        for step in &data.steps {
            let at = frame.map(data.index_at(step.mass_kg, step.pct_mac), step.mass_kg);
            let ringed = rings
                .iter()
                .any(|c| (c[0] - at[0]).hypot(c[1] - at[1]) < 1e-6);
            let expected = match step.state.as_str() {
                "DOW" => failed(State::OperatingEmpty),
                "ZFW" => failed(State::AnalyzedZeroFuel),
                "TOW" => failed(State::AnalyzedTakeoff),
                "LW" => failed(State::OperationalReserve),
                _ => false,
            };
            any_failed |= expected;
            assert_eq!(ringed, expected, "{preset} {}", step.state);
        }
        outcomes.push((preset, any_failed));
    }
    assert!(
        outcomes.iter().any(|o| o.1) && outcomes.iter().any(|o| !o.1),
        "one passing and one failing preset expected: {outcomes:?}"
    );
}
