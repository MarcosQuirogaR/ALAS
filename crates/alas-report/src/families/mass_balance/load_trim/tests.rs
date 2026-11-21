// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::sequences::potato_and_paths;
use super::*;
use crate::scene::{Point2D, SceneElement};
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
        en_route_above_mlw: None,
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
        lines: CgLines::default(),
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
        loading_curves: Vec::new(),
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

/// The sheet of a run whose flown route differs from the sizing loading:
/// the maximum-fuel takeoff is plotted and tabulated, the flown rows say so,
/// and the header line states both bases.
#[test]
fn a_sheet_with_the_maximum_fuel_state_names_its_two_mass_bases() {
    let mut data = sample();
    let gate = |mass: f64, phase: GatePhase| StepGate {
        state: alas_opt::ModelCgLoadingState::AnalyzedTakeoff,
        phase,
        cg_pct_mac: 25.0,
        fwd_pct_mac: 10.0,
        aft_pct_mac: 40.0,
        fwd_governance: alas_opt::envelope::ForwardLimitGovernance::MaxNoseLoadHandling,
        violated: mass < 0.0,
    };
    for step in &mut data.steps {
        if step.state != "DOW" {
            step.gate = Some(gate(1.0, GatePhase::Takeoff));
        }
    }
    data.steps.push(LoadStep {
        item: "Maximum fuel".to_owned(),
        state: "max-fuel TOW".to_owned(),
        mass_kg: 78_000.0,
        pct_mac: 24.5,
        gate: Some(gate(1.0, GatePhase::Takeoff)),
    });
    let line = panel::basis_line(&data);
    assert!(line.contains("Flown route: TOW 70.0 t"), "{line}");
    assert!(line.contains("MTOW sizing basis: 78.0 t"), "{line}");
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let texts: Vec<&str> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"TOW (flown route)"), "{texts:?}");
    assert!(texts.contains(&"max-fuel TOW"), "{texts:?}");
    // Without the maximum-fuel state the sheet keeps the plain state names.
    data.steps.pop();
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    assert!(!scene.elements.iter().any(|e| matches!(
        e, SceneElement::Text { text, .. } if text.contains("flown route")
    )));
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
        let (pct, mass) = frame.unmap(frame.map(s.pct_mac, s.mass_kg));
        assert!((mass - s.mass_kg).abs() < 1e-6);
        assert!((pct - s.pct_mac).abs() < 1e-9);
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
        assert!(svg.contains("Chart key") && svg.contains("Weight and balance"));
        assert!(svg.contains("MTOW 78 000 kg") && svg.contains("MLW 66 000 kg"));
        assert!(svg.contains("Fwd limit") && svg.contains("Aft limit") && svg.contains("%MAC"));
    }
}

/// The sample sheet with per-mechanism lines.
fn with_mechanisms() -> LoadTrimSheetData {
    let line = |mechanism, a: f64, b: f64| MechanismLine {
        mechanism,
        points: vec![(42_000.0, a), (78_000.0, b)],
        applies_kg: (f64::NEG_INFINITY, f64::INFINITY),
    };
    let mut data = sample();
    data.lines.mechanisms = vec![
        line(Mechanism::NoseGearLoad, 10.0, 10.0),
        line(Mechanism::Rotation, 15.0, 20.0),
        line(Mechanism::LandingTrim, 18.0, 18.0),
        line(Mechanism::StaticMargin, 40.0, 38.0),
        line(Mechanism::MinNoseLoad, 44.0, 44.0),
        line(Mechanism::TipBack, 50.0, 50.0),
    ];
    data
}

#[test]
fn each_envelope_takes_the_mechanisms_the_gate_scopes_to_its_phase() {
    let data = with_mechanisms();
    let rotation_at_60t = 15.0 + 5.0 * (60_000.0 - 42_000.0) / 36_000.0;
    // Takeoff: rotation and the ground and static-margin mechanisms.
    assert!(
        (data.envelope_limit_at(EnvelopeKind::Takeoff, 60_000.0, true) - rotation_at_60t).abs()
            < 1e-9
    );
    // Landing and zero fuel: landing trim in place of rotation.
    for kind in [EnvelopeKind::Landing, EnvelopeKind::ZeroFuel] {
        assert!(
            (data.envelope_limit_at(kind, 60_000.0, true) - 18.0).abs() < 1e-9,
            "{kind:?}"
        );
    }
    // Aft: the most forward of static margin (39 at 60 t), minimum nose load
    // and tip-back.
    let aft = 40.0 - 2.0 * (60_000.0 - 42_000.0) / 36_000.0;
    for kind in EnvelopeKind::ALL {
        assert!(
            (data.envelope_limit_at(kind, 60_000.0, false) - aft).abs() < 1e-9,
            "{kind:?}"
        );
    }
    assert_eq!(
        data.governing_mechanism(EnvelopeKind::Takeoff, 60_000.0, true),
        Some(Mechanism::Rotation)
    );
    assert_eq!(
        data.governing_mechanism(EnvelopeKind::Landing, 60_000.0, true),
        Some(Mechanism::LandingTrim)
    );
    assert_eq!(
        data.governing_mechanism(EnvelopeKind::Takeoff, 60_000.0, false),
        Some(Mechanism::StaticMargin)
    );
    // The gate's ground mechanisms bound the empty weight alone.
    assert_eq!(
        data.governing_mechanism(EnvelopeKind::ZeroFuel, 42_000.0, true),
        Some(Mechanism::LandingTrim)
    );
}

#[test]
fn an_envelope_runs_from_the_minimum_weight_to_its_own_top_weight() {
    let data = sample();
    let envelopes = data.envelopes();
    assert_eq!(
        envelopes.iter().map(|e| e.kind).collect::<Vec<_>>(),
        EnvelopeKind::ALL
    );
    for envelope in &envelopes {
        assert_eq!(envelope.bottom_kg, 42_000.0, "{:?}", envelope.kind);
        assert_eq!(envelope.vertices.first().map(|v| v.mass_kg), Some(42_000.0));
        assert_eq!(
            envelope.vertices.last().map(|v| v.mass_kg),
            Some(envelope.top_kg)
        );
        let outline = envelope.outline();
        assert_eq!(outline.len(), 2 * envelope.vertices.len());
        assert!(outline.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    }
    let top = |kind| envelopes.iter().find(|e| e.kind == kind).map(|e| e.top_kg);
    assert_eq!(top(EnvelopeKind::Takeoff), Some(78_000.0));
    assert_eq!(top(EnvelopeKind::Landing), Some(66_000.0));
    assert_eq!(top(EnvelopeKind::ZeroFuel), Some(62_500.0));
}

#[test]
fn collinear_vertices_are_dropped_and_corners_kept() {
    let v = |m: f64, f: f64, a: f64| LimitVertex {
        mass_kg: m,
        fwd_pct_mac: f,
        aft_pct_mac: a,
    };
    let kept = simplified(vec![
        v(0.0, 10.0, 40.0),
        v(1.0, 11.0, 40.0),
        v(2.0, 12.0, 40.0),
        v(3.0, 12.0, 38.0),
        v(4.0, 12.0, 36.0),
    ]);
    assert_eq!(
        kept.iter().map(|k| k.mass_kg).collect::<Vec<_>>(),
        [0.0, 2.0, 4.0]
    );
}

#[test]
fn a_band_whose_forward_limit_lies_aft_of_its_aft_limit_is_found_and_shaded() {
    let mut data = sample();
    data.takeoff_limits = vec![
        LimitVertex {
            mass_kg: 42_000.0,
            fwd_pct_mac: 17.0,
            aft_pct_mac: 40.0,
        },
        LimitVertex {
            mass_kg: 78_000.0,
            fwd_pct_mac: 45.0,
            aft_pct_mac: 37.0,
        },
    ];
    let takeoff = data
        .envelopes()
        .into_iter()
        .find(|e| e.kind == EnvelopeKind::Takeoff)
        .expect("takeoff envelope");
    let bands = takeoff.crossed_bands();
    assert_eq!(bands.len(), 1);
    assert!(bands[0].0 > 42_000.0 && (bands[0].1 - 78_000.0).abs() < 1e-6);
    let ink = super::render::Ink::for_palette(&PALETTE_LIGHT);
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let red = scene.elements.iter().filter(
        |e| matches!(e, SceneElement::Polygon { fill: Some(f), .. } if f.color == ink.crossed()),
    );
    assert_eq!(red.count(), 1);
    assert!(sample()
        .envelopes()
        .iter()
        .all(|e| e.crossed_bands().is_empty()));
}

#[test]
fn only_the_takeoff_envelope_is_filled_and_the_others_are_outlines() {
    let data = sample();
    let ink = super::render::Ink::for_palette(&PALETTE_LIGHT);
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    for kind in EnvelopeKind::ALL {
        let (fill, edge) = ink.envelope(kind);
        let drawn = scene
            .elements
            .iter()
            .filter(|e| match e {
                SceneElement::Polygon {
                    fill: f,
                    stroke: Some(s),
                    points,
                } => {
                    s.color == edge
                        && s.width <= 1.5
                        && points.len() >= 4
                        && f.is_some_and(|f| f.color == fill) == (kind == EnvelopeKind::Takeoff)
                }
                _ => false,
            })
            .count();
        assert_eq!(drawn, 1, "{kind:?}");
    }
}

#[test]
fn the_zero_fuel_top_is_labelled_analysed_when_no_mzfw_is_registered() {
    let mut data = sample();
    let label = |d: &LoadTrimSheetData| {
        d.envelopes()
            .iter()
            .find(|e| e.kind == EnvelopeKind::ZeroFuel)
            .unwrap()
            .label
    };
    assert_eq!(label(&data), "ZFW (analysed)");
    data.weight_lines.push(WeightLine {
        role: MassRole::MaximumZeroFuel,
        mass_kg: 62_500.0,
    });
    assert_eq!(label(&data), "MZFW");
}

/// The estimated box of a horizontal text.
fn text_box(element: &SceneElement) -> Option<(String, [f64; 4])> {
    let SceneElement::Text {
        text,
        pos,
        font_size,
        align,
        angle_deg,
        ..
    } = element
    else {
        return None;
    };
    let (w, h) = (
        super::geom::text_width(text, *font_size),
        super::geom::text_height(*font_size),
    );
    let (w, h) = if angle_deg.abs() > 45.0 {
        (h, w)
    } else {
        (w, h)
    };
    let left = match align {
        crate::scene::TextAlign::Left => pos[0],
        crate::scene::TextAlign::Right => pos[0] - w,
        _ => pos[0] - w / 2.0,
    };
    Some((
        text.clone(),
        [left, pos[1] - h / 2.0, left + w, pos[1] + h / 2.0],
    ))
}

#[test]
fn chart_labels_never_overlap_each_other() {
    for data in [sample(), with_mechanisms()] {
        let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
        let frame = frame_in(&data, super::geom::Geom::standard().chart);
        let inside = |b: &[f64; 4]| {
            b[0] > frame.left && b[2] < frame.right() && b[1] > frame.top && b[3] < frame.bottom()
        };
        let labels: Vec<_> = scene
            .elements
            .iter()
            .filter_map(text_box)
            .filter(|(_, b)| inside(b))
            .collect();
        assert!(labels.len() >= 5, "{}", labels.len());
        for (i, (a, ab)) in labels.iter().enumerate() {
            for (b, bb) in &labels[i + 1..] {
                let apart = ab[2] <= bb[0] || bb[2] <= ab[0] || ab[3] <= bb[1] || bb[3] <= ab[1];
                assert!(apart, "labels '{a}' and '{b}' overlap");
            }
        }
    }
}

#[test]
fn every_state_is_labelled_beside_its_point() {
    let data = sample();
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let frame = frame_in(&data, super::geom::Geom::standard().chart);
    let texts: Vec<_> = scene.elements.iter().filter_map(text_box).collect();
    for step in &data.steps {
        let at = frame.map(step.pct_mac, step.mass_kg);
        let near = texts.iter().any(|(t, b)| {
            t.starts_with(&step.state)
                && (b[0] - 40.0..=b[2] + 40.0).contains(&at[0])
                && (b[1] - 40.0..=b[3] + 40.0).contains(&at[1])
        });
        assert!(near, "state {}", step.state);
    }
}

#[test]
fn every_fixed_text_the_sheet_draws_is_in_the_catalog_list() {
    let data = with_mechanisms();
    for scene in [
        figure_load_trim_sheet(&data, &PALETTE_LIGHT),
        figure_load_trim_sheet_sized(&data, &PALETTE_LIGHT, 700.0),
    ] {
        for element in &scene.elements {
            let SceneElement::Text { text, .. } = element else {
                continue;
            };
            let numeric = text.chars().any(|c| c.is_ascii_digit()) || *text == data.title;
            let state = matches!(text.as_str(), "DOW" | "ZFW" | "TOW" | "LW" | "max-fuel TOW");
            assert!(
                numeric || state || SHEET_TEXT.contains(&text.as_str()),
                "'{text}' is neither a number nor in SHEET_TEXT"
            );
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
fn a_state_the_gate_failed_is_ringed_on_the_chart_and_in_the_table_and_no_other() {
    use alas_opt::ModelCgLoadingState as State;
    let gate = |state, violated| {
        Some(StepGate {
            state,
            phase: GatePhase::of(state),
            cg_pct_mac: 30.0,
            fwd_pct_mac: 15.0,
            aft_pct_mac: 40.0,
            fwd_governance: alas_opt::envelope::ForwardLimitGovernance::MaxNoseLoadHandling,
            violated,
        })
    };
    let mut data = sample();
    data.steps[0].gate = gate(State::OperatingEmpty, false);
    data.steps[1].gate = gate(State::AnalyzedZeroFuel, true);
    data.steps[2].gate = gate(State::AnalyzedTakeoff, false);
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let frame = frame_in(&data, super::geom::Geom::standard().chart);
    let rings = alert_rings(&scene, &PALETTE_LIGHT);
    for (k, step) in data.steps.iter().enumerate() {
        let at = frame.map(step.pct_mac, step.mass_kg);
        let ringed = rings
            .iter()
            .any(|c| (c[0] - at[0]).hypot(c[1] - at[1]) < 1e-6);
        assert_eq!(ringed, k == 1, "{}", step.state);
    }
    // Chart ring, table ring and key ring.
    assert_eq!(rings.len(), 3);
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
    // Above the MLW an en-route state has its own phase: no landing trim and
    // no rotation, the static-margin floor aft.
    assert_eq!(
        GatePhase::EnRouteAboveMlw.limits(),
        PhaseLimits::EN_ROUTE_ABOVE_MLW
    );
}

/// A state the gate scoped above the maximum landing mass is its own phase:
/// its row keeps the gate's own limits, and the table names the forward
/// limit the gate reported for it (not the landing trim of the flight
/// phase), as the mechanism its governance maps to.
#[test]
fn a_state_gated_above_the_mlw_is_its_own_phase_and_names_its_forward_limit() {
    use alas_opt::envelope::ForwardLimitGovernance as Fwd;
    use alas_opt::ModelCgLoadingState as State;
    for (governance, mechanism) in [
        (Fwd::MaxNoseLoadHandling, Mechanism::NoseGearLoad),
        (Fwd::RotationNoseWheelLiftoff, Mechanism::Rotation),
        (Fwd::TakeoffTrimStabiliserStop, Mechanism::Rotation),
        (Fwd::ScissorPlotEstimate, Mechanism::LandingTrim),
        (Fwd::LandingTrimDesignRule, Mechanism::LandingTrim),
        (Fwd::LandingTrimSourcedTravel, Mechanism::LandingTrim),
    ] {
        assert_eq!(Mechanism::of_forward_governance(governance), mechanism);
        assert!(mechanism.is_forward());
    }
    let mut data = with_mechanisms();
    assert!(super::gate::table_notes(&data.steps)
        .iter()
        .all(|(lead, _)| *lead != super::gate::NOTE_ABOVE_MLW));
    let reserve = LoadStep {
        item: "Trip fuel burn".to_owned(),
        state: "LW".to_owned(),
        mass_kg: 68_000.0,
        pct_mac: 28.0,
        gate: Some(StepGate {
            state: State::OperationalReserve,
            phase: GatePhase::EnRouteAboveMlw,
            cg_pct_mac: 28.0,
            fwd_pct_mac: 12.3,
            aft_pct_mac: 38.0,
            fwd_governance: Fwd::MaxNoseLoadHandling,
            violated: false,
        }),
    };
    data.steps.push(reserve);
    let notes = super::gate::table_notes(&data.steps);
    assert_eq!(
        notes.last(),
        Some(&(super::gate::NOTE_ABOVE_MLW, Some("nose-gear load")))
    );
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let drawn = |wanted: &str| {
        scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text == wanted))
    };
    assert!(drawn(super::gate::NOTE_ABOVE_MLW) && drawn("nose-gear load"));
    // The table states the gate's own forward limit for the row.
    assert!(drawn("12.3"));
}

#[test]
fn the_envelope_scopes_agree_with_the_gate_scopes() {
    use alas_opt::envelope::PhaseLimits;
    for (kind, scope) in [
        (EnvelopeKind::Takeoff, PhaseLimits::TAKEOFF),
        (EnvelopeKind::Landing, PhaseLimits::LANDING),
        (EnvelopeKind::ZeroFuel, PhaseLimits::FLIGHT),
    ] {
        assert_eq!(kind.admits(Mechanism::Rotation), scope.rotation, "{kind:?}");
        assert_eq!(
            kind.admits(Mechanism::LandingTrim),
            scope.landing_trim,
            "{kind:?}"
        );
        assert_eq!(
            kind.admits(Mechanism::StaticMargin),
            scope.static_margin,
            "{kind:?}"
        );
        for ground in [
            Mechanism::NoseGearLoad,
            Mechanism::MinNoseLoad,
            Mechanism::TipBack,
        ] {
            assert!(kind.admits(ground));
        }
    }
}

#[test]
fn the_panel_stands_beside_the_chart_on_a_wide_card_and_stacks_below_on_a_narrow_one() {
    let data = with_mechanisms();
    let frame_of = |width: f64| {
        let scene = figure_load_trim_sheet_sized(&data, &PALETTE_LIGHT, width);
        let panel_title = scene
            .elements
            .iter()
            .find_map(|e| match e {
                SceneElement::Text { text, pos, .. } if text == "Chart key" => Some(*pos),
                _ => None,
            })
            .expect("key title");
        (scene.width, scene.height, panel_title)
    };
    let (wide_w, wide_h, wide_title) = frame_of(1400.0);
    let (narrow_w, narrow_h, narrow_title) = frame_of(640.0);
    // Beside: the panel starts right of the chart and near the top.
    assert!(wide_title[0] > 0.6 * wide_w && wide_title[1] < 0.3 * wide_h);
    // Stacked: the panel starts at the left, below the chart.
    assert!(narrow_title[0] < 0.2 * narrow_w && narrow_title[1] > 0.5 * narrow_h);
    for (w, h) in [(wide_w, wide_h), (narrow_w, narrow_h)] {
        assert!(w.is_finite() && h.is_finite() && h > 300.0);
    }
    // Nothing the panel draws leaves the canvas.
    for width in [640.0, 900.0, 1100.0, 1400.0, 1900.0] {
        let scene = figure_load_trim_sheet_sized(&data, &PALETTE_LIGHT, width);
        for element in &scene.elements {
            if let Some((t, b)) = text_box(element) {
                assert!(
                    b[0] >= -1.0
                        && b[2] <= scene.width + 1.0
                        && b[1] >= -1.0
                        && b[3] <= scene.height + 1.0,
                    "'{t}' leaves the {width} px canvas: {b:?} of {} x {}",
                    scene.width,
                    scene.height
                );
            }
        }
    }
}

#[test]
fn a_certified_line_is_drawn_dashed_in_its_own_colour_and_named_in_the_key() {
    let mut data = with_mechanisms();
    data.lines.published = vec![PublishedLine {
        source: "Test Manual, Table 1".to_owned(),
        reference: "Test Manual, Table 1, p1".to_owned(),
        forward: true,
        points: vec![(45_000.0, 14.0), (70_000.0, 16.0)],
    }];
    let ink = super::render::Ink::for_palette(&PALETTE_LIGHT);
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let dashed = scene.elements.iter().filter(|e| {
        matches!(e, SceneElement::Polyline { stroke, .. }
            if stroke.color == ink.certified && stroke.dash_array.is_some())
    });
    assert_eq!(dashed.count(), 1);
    let svg = crate::svg::render_svg(&scene);
    assert!(svg.contains("Test Manual, Table 1"));
}

#[test]
fn published_envelopes_exist_only_for_the_sourced_presets() {
    use super::published::published_for;
    assert!(published_for("A320-200", 11.0, 4.2).is_empty());
    assert!(!published_for("A220-300", 11.0, 4.2).is_empty());
    assert!(!published_for("B747-400", 11.0, 4.2).is_empty());
    let atr = published_for("ATR72-600", 13.604, 2.303);
    assert_eq!(atr.iter().filter(|l| l.forward).count(), 2);
    assert_eq!(atr[0].points.last(), Some(&(22_970.0, 20.8)));
    assert!(atr.iter().all(|l| l.reference.contains("EASA.A.084")));
    // The C-295 TCDS (EASA.A.186 Issue 08, Section 5 item 14) refers the CG
    // range to the AFM: no published envelope.
    assert!(published_for("C295", 11.0, 4.2).is_empty());
}

/// A gated state at `mass_kg` scoped to `scope`, with the mechanisms of the
/// sample sheet: nose-gear load 10, landing trim 18, rotation 20 forward;
/// static margin 38, minimum nose load 44, tip-back 50 aft (%MAC).
fn gated_state(
    state: alas_opt::ModelCgLoadingState,
    scope: alas_opt::envelope::PhaseLimits,
    mass_kg: f64,
) -> alas_opt::ModelCgLoadingAssessment {
    use alas_opt::envelope::{AftLimitGovernance, ForwardLimitGovernance, PhysicalCgLimits};
    let all = PhysicalCgLimits {
        aft_limit_pct_mac: 38.0,
        aft_limit_governance: AftLimitGovernance::Aerodynamic,
        clean_np_pct_mac: 45.0,
        aerodynamic_aft_pct_mac: 38.0,
        ground_aft_pct_mac: 44.0,
        tip_back_aft_pct_mac: 50.0,
        fwd_limit_pct_mac: 20.0,
        fwd_limit_governance: ForwardLimitGovernance::RotationNoseWheelLiftoff,
        max_nose_load_fwd_pct_mac: 10.0,
        scissor_plot_fwd_pct_mac: 8.0,
        rotation_fwd_pct_mac: 20.0,
        takeoff_trim_fwd_pct_mac: f64::NEG_INFINITY,
        clean_trim_fwd_pct_mac: f64::NEG_INFINITY,
        landing_approach_fwd_pct_mac: f64::NEG_INFINITY,
        landing_flare_fwd_pct_mac: f64::NEG_INFINITY,
        rotation_longitudinal_force_shift_pct_mac: 0.0,
        rotation_tail_lift_coefficient: -1.3,
        rotation_stabilizer_nose_up_deg: None,
        landing_trim_fwd_pct_mac: 18.0,
        iced_tail_fwd_pct_mac: 9.0,
        landing_tail_lift_coefficient: -0.8,
        landing_tail_trim_limited: false,
        landing_tail_sourced: false,
        landing_trim_condition: "design rule",
        usable_range_pct_mac: 18.0,
    };
    alas_opt::ModelCgLoadingAssessment {
        state,
        scope,
        mass_kg,
        cg_x_m: X_LEMAC + 0.3 * MAC,
        h_cg_m: 3.0,
        cg_pct_mac: 30.0,
        static_margin: 0.1,
        critical_static_margin: 0.1,
        nose_gear_load_fraction: 0.08,
        ground_reactions_admissible: true,
        physical_limits: all.scoped(scope),
        constraints: Vec::new(),
    }
}

/// The en-route states the gate scoped above the MLW are a limit set of
/// their own, from the MLW to the heaviest of them, at their own scope (no
/// landing trim, no rotation): drawn as an outline in its own style and keyed
/// by the mechanism the gate reported bounding them forward, read from the
/// heaviest such state's verdict.
#[test]
fn the_en_route_states_above_the_mlw_are_their_own_set_drawn_and_keyed_apart() {
    use super::limits::{limit_sets, Bands};
    use alas_opt::envelope::{ForwardLimitGovernance, PhaseLimits};
    use alas_opt::ModelCgLoadingState as State;
    let mut states = vec![
        gated_state(State::OperatingEmpty, PhaseLimits::GROUND, 42_000.0),
        gated_state(State::AnalyzedZeroFuel, PhaseLimits::FLIGHT, 60_000.0),
        gated_state(State::OperationalReserve, PhaseLimits::FLIGHT, 64_000.0),
        gated_state(
            State::OperationalMidMission,
            PhaseLimits::EN_ROUTE_ABOVE_MLW,
            71_000.0,
        ),
        gated_state(State::AnalyzedTakeoff, PhaseLimits::TAKEOFF, 78_000.0),
    ];
    let bands = Bands {
        zfw_kg: 60_000.0,
        takeoff_kg: 78_000.0,
        landing_kg: 66_000.0,
    };
    let sets = limit_sets(&states.iter().collect::<Vec<_>>(), bands);
    // Five sets; the flight (landing-trim) set ends at the MLW.
    for (name, set) in [
        ("ground", &sets.ground),
        ("takeoff", &sets.takeoff),
        ("flight", &sets.flight),
        ("landing", &sets.landing),
    ] {
        assert!(!set.is_empty(), "{name}");
    }
    assert!(sets.flight.iter().all(|v| v.mass_kg <= 66_000.0 + 1.0));
    let above = sets.en_route_above_mlw.expect("an en-route-above-MLW set");
    let masses: Vec<f64> = above.vertices.iter().map(|v| v.mass_kg).collect();
    assert_eq!(masses, vec![66_000.0, 71_000.0]);
    for v in &above.vertices {
        // Nose-gear load forward (not landing trim 18, not rotation 20) and
        // the static-margin floor aft.
        assert_eq!((v.fwd_pct_mac, v.aft_pct_mac), (10.0, 38.0));
    }
    assert_eq!(above.forward_bound, Mechanism::NoseGearLoad);

    // The key names what the verdict says, not a fixed mechanism.
    states[3].physical_limits.fwd_limit_governance = ForwardLimitGovernance::LandingTrimDesignRule;
    let relabelled = limit_sets(&states.iter().collect::<Vec<_>>(), bands);
    assert_eq!(
        relabelled.en_route_above_mlw.map(|s| s.forward_bound),
        Some(Mechanism::LandingTrim)
    );
    // No state above the MLW: no set.
    states.remove(3);
    assert!(limit_sets(&states.iter().collect::<Vec<_>>(), bands)
        .en_route_above_mlw
        .is_none());

    // Drawn as its own outline and keyed with its forward bound.
    let mut data = with_mechanisms();
    let style = super::render::Ink::for_palette(&PALETTE_LIGHT).above_mlw_stroke();
    let unstyled = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    data.en_route_above_mlw = Some(above);
    let scene = figure_load_trim_sheet(&data, &PALETTE_LIGHT);
    let styled = |scene: &crate::scene::Scene| {
        scene
            .elements
            .iter()
            .filter(|e| match e {
                SceneElement::Polygon {
                    stroke: Some(s), ..
                }
                | SceneElement::Line { stroke: s, .. } => *s == style,
                _ => false,
            })
            .count()
    };
    assert_eq!(styled(&unstyled), 0);
    // The outline on the chart and the sample line in the key.
    assert_eq!(styled(&scene), 2);
    let drawn = |wanted: &str| {
        scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text == wanted))
    };
    assert!(drawn(super::panel::KEY_ABOVE_MLW) && drawn("nose-gear load"));
    assert!(super::panel::SHEET_TEXT.contains(&super::panel::KEY_ABOVE_MLW));
}
