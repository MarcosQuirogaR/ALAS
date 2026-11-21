// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the aircraft preset registry against the one the reference
//! implementation registers.
//!
//! Every preset field and registration order is checked at `exact`. An
//! explicit two-sided ledger pins independently audited corrections while all
//! other translated fields compare directly with Python. Numbers compare by
//! value so an upstream integer and an equivalent Rust float agree.
//! Published landing inputs are compared to their source-derived values;
//! their frozen legacy values remain checked separately from the source ledger.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{
    presets, AircraftPreset, DesignMissionEvidence, MissingDesignMissionDatum,
    PartialMissionEvidenceKind, PublishedRange,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[path = "support/product_corrections.rs"]
mod product_corrections;

/// How many source corrections the ledger is allowed to hold.
///
/// Pinned so a correction cannot be added without someone noticing. The count
/// includes the absolute panel counts (see [`add_spanwise_panel_corrections`]),
/// the four registered planning-cabin seat counts, the two AVE inboard chords
/// that keep its root-to-kink trailing edge running aft, the AVE cabin
/// preset that seats its declared 777-9 cabin, and the default
/// landing ratio the A220 calibration inherits. The wing and tail panel counts
/// are seven entries each.
const SOURCE_CORRECTION_COUNT: usize = 88;
const DC_10_UPSTREAM_DISPLAY_NAME: &str = "McDonnell Douglas DC-10";
const DC_10_CORRECTED_DISPLAY_NAME: &str = "McDonnell Douglas DC-10-30 (572k option)";

struct SourceCorrection {
    upstream: Value,
    corrected: Value,
}

#[derive(Deserialize)]
struct Fixture {
    presets: Vec<Value>,
    available: Vec<String>,
    display_names: std::collections::BTreeMap<String, String>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "aircraft_presets")
}

/// The empty-mass figure a preset's declared structural payload was derived
/// from, as the OEW reference registry documents it.
fn structural_payload_basis_oew_kg(name: &str) -> f64 {
    alas_config::oew_reference::get(name)
        .and_then(|record| record.structural_payload_basis_oew_kg)
        .unwrap_or_else(|| panic!("{name} has a documented structural payload basis"))
}

#[test]
fn every_aircraft_preset_matches_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets", Tier::Exact);
    let mut source_corrections = source_corrections();
    comparison.exact(
        "source-correction ledger size",
        &source_corrections.len(),
        &SOURCE_CORRECTION_COUNT,
    );

    let names: Vec<&str> = presets::available()
        .into_iter()
        .filter(|name| fixture.available.iter().any(|reference| reference == name))
        .collect();
    let expected_names: Vec<&str> = fixture.available.iter().map(String::as_str).collect();
    comparison.exact("registration order", &names, &expected_names);
    for expected in &fixture.presets {
        let preset = presets::get(expected["name"].as_str().unwrap())
            .expect("reference preset remains registered");
        compare_values(
            &mut comparison,
            &mut source_corrections,
            preset.name,
            &as_value(preset),
            expected,
        );
    }
    let unvisited_corrections: Vec<String> = source_corrections.keys().cloned().collect();
    comparison.exact(
        "source-correction ledger paths not present in the fixture",
        &unvisited_corrections,
        &Vec::<String>::new(),
    );
    comparison.finish();
}

/// How far, metres, a leading-edge vertex may lie off the straight line through its
/// neighbouring declared breaks and still be read as on that line: half the drawn line
/// weight at the drawing scale, measured on the vector drawings of the two presets with
/// a declared leading-edge crank. The stricter value is kept, unrounded:
///
/// * Boeing 747-400 ACAP, D6-58326-1 Rev F, section 2.2.1, p. 2-14 (kept): the plan-view
///   leading edge is a filled vector outline 0.2836 pt wide, and the printed 70.67 m
///   spans 272.03 pt between its arrow tips (about 1:736), so half the line is 0.0368 m.
/// * Airbus A350 Aircraft Characteristics Rev 16 (Feb 01/26), fig 2-2-0-991-001-A01 sheet 2
///   (printed values unchanged in Rev 17, Jul 01/26): the outline is
///   a raster image; the vector extension lines that locate the printed points are
///   stroked 0.4548 pt, and the printed 66.80 m spans 367.04 pt (about 1:516), so half
///   a line is 0.0414 m.
const DRAWING_READING_TOLERANCE_M: f64 = 0.5 * 0.2836 * 70.67 / 272.03;

/// Perpendicular distance, metres, of `(y, x)` vertex `v` from the line through `a` and `b`.
fn off_line_m(a: (f64, f64), b: (f64, f64), v: (f64, f64)) -> f64 {
    let (dy, dx) = (b.0 - a.0, b.1 - a.1);
    ((v.0 - a.0) * dx - (v.1 - a.1) * dy).abs() / dy.hypot(dx)
}

/// Leading-edge sweep, degrees positive aft, of the straight line from `a` to `b`.
fn line_sweep_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.1 - a.1).atan2(b.0 - a.0).to_degrees()
}

/// The leading edge of every product wing, blends removed, is straight between
/// declared breaks and carries the declared sweeps:
///
/// * A region (centreline to kink; kink to the rake start or the tip) with no
///   declared leading-edge crank runs at one sweep, exactly: `inboard_le_sweep_deg`
///   inboard of the kink, the design sweep outboard.
/// * A region with a drawn crank declared in `presets::planform_breaks`
///   (sourced, point break of kind `Crank` on the leading edge) is straight, within
///   [`DRAWING_READING_TOLERANCE_M`], from the side of body to the rake start or tip
///   between the declared breaks; each declared crank is a corner larger than that
///   tolerance; and the region's declared sweep lies between the sweeps of the
///   straight runs that cross it (the declared angle is the mean of the drawn edge,
///   not one of its runs). The datum panel inboard of the side of body (the
///   carry-through on its edge lines) keeps the exact `inboard_le_sweep_deg` check
///   in a cranked inboard region too.
/// * On a wing with a declared crank, a vertex that is not a declared leading-edge
///   break (the kink included) is no corner of the leading edge (the same
///   tolerance), so a crank can only sit where the drawing puts it.
/// * The swept-back tip rake keeps its own checks below.
#[test]
fn every_product_preset_leading_edge_breaks_only_at_declared_stations() {
    use presets::planform_breaks::{
        PlanformBreakKind, PlanformEdge, PlanformSurface, DECLARED_BREAKS,
    };
    for preset in presets::registry() {
        // The wing, not its blends: a fillet's edge stations inboard of the
        // straight edge runs are data for the drawn blend body, and the wing
        // stays on its edge lines to the centreline.
        let planform = preset
            .geometry
            .wing
            .transport_planform(&preset.design_vector)
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name))
            .without_blends();
        let tolerance_m = 1.0e-9 * planform.tip.y_m;
        let design_sweep_deg = preset.design_vector.sweep_deg;
        // Declared point breaks of the leading edge, as spanwise stations, metres.
        let declared_le_breaks: Vec<(PlanformBreakKind, f64)> = DECLARED_BREAKS
            .iter()
            .filter(|b| {
                b.preset == preset.name
                    && b.surface == PlanformSurface::MainWing
                    && matches!(b.edge, PlanformEdge::Leading | PlanformEdge::Both)
                    && (b.to_eta - b.from_eta).abs() < 1.0e-12
                    && matches!(b.kind, PlanformBreakKind::Crank | PlanformBreakKind::Kink)
            })
            .map(|b| (b.kind, b.from_eta * planform.tip.y_m))
            .collect();
        // Matches a declared span fraction (planform_breaks matches to 1e-3 of the semispan).
        let station_tolerance_m = 1.0e-3 * planform.tip.y_m;
        let kink_y_m = planform.kink.y_m;
        let start_y_m = planform.side_of_body.map_or(0.0, |sob| sob.y_m);
        let end_y_m = planform.tip_rake.map_or(planform.tip.y_m, |rake| rake.y_m);
        let crank_in = |from_m: f64, to_m: f64| {
            declared_le_breaks.iter().any(|&(kind, y_m)| {
                kind == PlanformBreakKind::Crank
                    && y_m > from_m + station_tolerance_m
                    && y_m < to_m - station_tolerance_m
            })
        };
        let inboard_cranked = crank_in(start_y_m, kink_y_m);
        let outboard_cranked = crank_in(kink_y_m, end_y_m);
        // A declared swept-back tip is drawn outboard of
        // its start, edge stations included (raked tips): as a
        // whole it sweeps further aft than the straight leading edge, and
        // panel by panel its leading edge never sweeps less than the design
        // sweep nor less than the panel inboard of it.
        let rake_start_m = planform.tip_rake.map(|rake| rake.y_m - tolerance_m);
        let mut inboard_rake_sweep_deg: Option<f64> = None;
        if let Some(rake) = planform.tip_rake {
            let tip = *planform
                .stations_with_rake()
                .last()
                .expect("a planform has a tip");
            let straight_x_m =
                rake.leading_edge_x_m + (tip.y_m - rake.y_m) * design_sweep_deg.to_radians().tan();
            assert!(
                tip.leading_edge_x_m > straight_x_m,
                "{}: a tip rake must sweep aft",
                preset.name
            );
        }
        for panel in planform.panels() {
            if rake_start_m.is_some_and(|start_m| panel.inboard.y_m >= start_m) {
                let sweep_deg = panel.leading_edge_sweep_deg;
                assert!(
                    sweep_deg >= design_sweep_deg - 1.0e-10,
                    "{} {:?}->{:?}: rake panel leading edge {sweep_deg} deg sweeps less than the design {design_sweep_deg} deg",
                    preset.name,
                    panel.inboard.kind,
                    panel.outboard.kind,
                );
                if let Some(inboard_deg) = inboard_rake_sweep_deg {
                    assert!(
                        sweep_deg >= inboard_deg - 1.0e-10,
                        "{} {:?}->{:?}: rake panel leading edge {sweep_deg} deg sweeps less than the {inboard_deg} deg inboard of it",
                        preset.name,
                        panel.inboard.kind,
                        panel.outboard.kind,
                    );
                }
                inboard_rake_sweep_deg = Some(sweep_deg);
                continue;
            }
            let inboard_region = panel.outboard.y_m <= kink_y_m + tolerance_m;
            // The root-to-side-of-body datum panel is not drawn and stays exact.
            let drawn = panel.inboard.y_m >= start_y_m - tolerance_m;
            if drawn
                && ((inboard_region && inboard_cranked) || (!inboard_region && outboard_cranked))
            {
                continue; // a drawn crank: checked on the polyline below
            }
            // a declared inboard crank (A380-800) applies up to the kink
            let expected_deg = if inboard_region {
                planform.inboard_le_sweep_deg
            } else {
                design_sweep_deg
            };
            assert!(
                (panel.leading_edge_sweep_deg - expected_deg).abs() < 1.0e-10,
                "{} {:?}->{:?}: leading edge {} deg instead of the declared {} deg, with no leading-edge crank declared in this region",
                preset.name,
                panel.inboard.kind,
                panel.outboard.kind,
                panel.leading_edge_sweep_deg,
                expected_deg
            );
        }
        if !(inboard_cranked || outboard_cranked) {
            continue;
        }
        // The drawn leading edge, side of body to rake start (or tip), as (y, x) metres.
        let vertices: Vec<(f64, f64)> = planform
            .stations_with_rake()
            .iter()
            .filter(|s| s.y_m >= start_y_m - tolerance_m && s.y_m <= end_y_m + tolerance_m)
            .map(|s| (s.y_m, s.leading_edge_x_m))
            .collect();
        let last = vertices.len() - 1;
        let declared_at = |y_m: f64| {
            declared_le_breaks
                .iter()
                .any(|&(_, break_y_m)| (break_y_m - y_m).abs() <= station_tolerance_m)
        };
        for &(kind, y_m) in &declared_le_breaks {
            assert!(
                y_m > end_y_m + station_tolerance_m
                    || vertices
                        .iter()
                        .any(|v| (v.0 - y_m).abs() <= station_tolerance_m),
                "{}: declared leading-edge {kind:?} at y {y_m:.3} m has no planform station",
                preset.name
            );
        }
        let pinned: Vec<usize> = (0..=last)
            .filter(|&i| i == 0 || i == last || declared_at(vertices[i].0))
            .collect();
        for i in 1..last {
            let left = *pinned
                .iter()
                .rev()
                .find(|&&j| j < i)
                .expect("the start is pinned");
            let right = *pinned.iter().find(|&&j| j > i).expect("the end is pinned");
            // Off the straight line through the nearest pinned vertices either side.
            let (a, b) = (vertices[left], vertices[right]);
            let off_m = off_line_m(a, b, vertices[i]);
            if pinned.contains(&i) {
                assert!(
                    off_m > DRAWING_READING_TOLERANCE_M,
                    "{}: declared leading-edge break at y {:.3} m is {:.1} mm off the straight line of its neighbours, within the drawing reading tolerance: remove the declaration or the station",
                    preset.name,
                    vertices[i].0,
                    off_m * 1000.0
                );
            } else {
                assert!(
                    off_m <= DRAWING_READING_TOLERANCE_M,
                    "{}: undeclared leading-edge corner at y {:.3} m, {:.1} mm off the straight line from y {:.3} to {:.3} m",
                    preset.name,
                    vertices[i].0,
                    off_m * 1000.0,
                    a.0,
                    b.0
                );
            }
        }
        // The declared sweep of a cranked region lies between its drawn runs.
        let runs: Vec<((f64, f64), f64)> = pinned
            .windows(2)
            .map(|pair| {
                let (a, b) = (vertices[pair[0]], vertices[pair[1]]);
                ((a.0, b.0), line_sweep_deg(a, b))
            })
            .collect();
        for (cranked, from_m, to_m, declared_deg, field) in [
            (
                inboard_cranked,
                start_y_m,
                kink_y_m,
                planform.inboard_le_sweep_deg,
                "inboard_le_sweep_deg",
            ),
            (
                outboard_cranked,
                kink_y_m,
                end_y_m,
                design_sweep_deg,
                "design sweep",
            ),
        ] {
            if !cranked {
                continue;
            }
            let crossing: Vec<f64> = runs
                .iter()
                .filter(|((run_from_m, run_to_m), _)| {
                    *run_to_m > from_m + tolerance_m && *run_from_m < to_m - tolerance_m
                })
                .map(|(_, sweep_deg)| *sweep_deg)
                .collect();
            let lowest = crossing.iter().copied().fold(f64::INFINITY, f64::min);
            let highest = crossing.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!(
                declared_deg >= lowest - 1.0e-10 && declared_deg <= highest + 1.0e-10,
                "{}: {field} {declared_deg} deg lies outside the drawn leading-edge runs {crossing:?} deg between y {from_m:.3} and {to_m:.3} m",
                preset.name
            );
        }
    }
}

#[test]
fn the_dropdown_labels_match_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets display names", Tier::Exact);
    for (name, display_name) in presets::display_names() {
        if name == "ATR72-600" {
            assert_eq!(display_name, "ATR 72-600");
            continue;
        }
        if name == "C919" {
            // Not in the upstream reference: a COMAC preset added in Rust.
            assert_eq!(display_name, "COMAC C919");
            continue;
        }
        if name == "E195-E2" {
            // Not in the upstream reference: an Embraer preset added in Rust.
            assert_eq!(display_name, "Embraer E195-E2");
            continue;
        }
        // Registered after the reference implementation: no upstream label.
        if name == "B747-400" {
            assert_eq!(display_name, "Boeing 747-400 (CF6-80C2B1F)");
            continue;
        }
        // The A400M post-dates the frozen reference: not in its label table.
        if name == "A400M" {
            assert_eq!(display_name, "Airbus A400M Atlas");
            continue;
        }
        // Registered in Rust after the frozen reference: the
        // A350-900, the Learjet 60 and the C295
        // have no upstream label.
        if name == "A350-900" {
            assert_eq!(display_name, "Airbus A350-900");
            continue;
        }
        if name == "LJ60" {
            assert_eq!(display_name, "Learjet 60");
            continue;
        }
        if name == "C295" {
            assert_eq!(display_name, "Airbus C295");
            continue;
        }
        let upstream = fixture.display_names.get(name).map(String::as_str);
        if name == "DC-10" {
            comparison.exact(
                "DC-10 frozen dropdown label",
                &upstream,
                &Some(DC_10_UPSTREAM_DISPLAY_NAME),
            );
            comparison.exact(
                "DC-10 corrected dropdown label",
                &display_name,
                &DC_10_CORRECTED_DISPLAY_NAME,
            );
        } else {
            comparison.exact(name, &Some(display_name), &upstream);
        }
    }
    comparison.exact(
        "count",
        // old len - 2 -> new len - 8: the ATR 72-600, E195-E2, C919, B747-400,
        // A400M, A350-900, LJ60 and C295 have no upstream label.
        &(presets::display_names().len() - 8),
        &fixture.display_names.len(),
    );
    comparison.finish();
}

#[test]
fn source_corrected_presets_name_the_revision_locked_evidence() {
    let a340 = assert_source_record(
        "A340-300",
        [
            "A340-312",
            "WV029",
            "CFM56-5C3/F",
            "public WV029 planning baseline",
            "three tanks",
        ],
        &[
            "EASA.A.015 Issue 28, 2026-01-15, pp.32-36",
            "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 2-1-1",
        ],
    );
    assert_eq!(a340.reference.mtow_kg, Some(260_000.0));
    assert_eq!(a340.reference.mtow_kg, Some(a340.requirements.mtow_kg));

    let a380 = assert_source_record(
        "A380-800",
        [
            "A380-841",
            "WV000",
            "Trent 970-84",
            "WV000 public planning baseline",
            "323,546 L tanks + 793 L usable system inventory",
        ],
        &[
            "EASA.A.110 Issue 17, 2026-08-05, pp.10-15",
            "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 2-1-1",
            "Airbus A380 Facts and Figures, February 2022, p.3",
        ],
    );
    assert_eq!(a380.reference.reference_wing_area_m2, Some(845.0));
    assert_eq!(
        a380.reference.reference_wing_area_m2,
        Some(a380.requirements.max_wing_area_m2)
    );
    // The A380 reference wing is the Published one (845 m2). The drawn planform of the
    // vector-outline fit (kink chord 12.05 m, LE 28.18 m / TE 40.23 m at the kink,
    // inboard LE crank 39.55 deg, drawn TE breaks and fillet stations, AC 2-2-0 sheet 2) is 856.629879362 m2
    // (+1.38 %); it is 847.295188433 m2 on an 11.62 m kink chord.
    let projected = preset_projected_area(a380);
    assert!(
        (projected - 856.629_879_361_720_8).abs() < 1.0e-6,
        "A380 projected area {projected} m2",
    );

    let b787 = assert_source_record(
        "B787-9",
        [
            "787-9",
            "legacy 561,500 lb MTOW",
            "GEnx-1B74/75 P2 family",
            "Boeing Rev Q legacy-weight planning baseline",
            "standard 33,399 US gal usable system",
        ],
        &[
            "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2",
            "NASA/TP-20210023843, December 2022, Table I",
            "Boeing 787 ACAP D6-58333 Rev L, December 2015, p.2-3 (typical OEW only)",
        ],
    );
    assert_eq!(b787.reference.mtow_kg, Some(254_692.0));
    assert_eq!(b787.reference.mtow_kg, Some(b787.requirements.mtow_kg));
    // No configuration-matched OEW is published for the legacy variant; the
    // declared structural payload keeps its documented derivation basis.
    assert_eq!(b787.reference.oew_kg, None);
    assert_eq!(
        b787.requirements.max_structural_payload_kg,
        b787.reference.mzfw_kg.unwrap() - structural_payload_basis_oew_kg("B787-9")
    );

    let a320 = assert_source_record(
        "A320-200",
        [
            "A320-214",
            "WV017",
            "CFM56-5B4/3",
            "MOD160500 sharklets; MOD37147 Tech Insertion",
            "three tanks; MOD37331 + MOD160001",
        ],
        &[
            "Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 2-1-1 p.2",
            "EASA.A.064 Issue 62, pp.37-48",
            "EASA.E.003 Issue 06, pp.10-11",
        ],
    );
    assert_eq!(a320.engine_name, a320.identity.engine_model);
    assert_eq!(a320.geometry.engine.engine_name, a320.engine_name);

    let a220 = assert_source_record(
        "A220-300",
        [
            "BD-500-1A11",
            "legacy 149,000 lb MTOW",
            "PW1521G-3",
            "S/N 55001-59999 planning configuration",
            "standard integral tanks",
        ],
        &[
            "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00, May 2026, J06-20-01 p.14 and J08-41-03-01 p.2",
            "Airbus A220 ARP J07-40-00-06AAA-030A-A, 2019-10-22 p.2",
        ],
    );
    assert_eq!(a220.reference.mtow_kg, Some(67_585.0));
    assert_eq!(a220.reference.mtow_kg, Some(a220.requirements.mtow_kg));
    assert_eq!(a220.reference.oew_kg, Some(37_149.0));
    assert_eq!(
        a220.requirements.max_structural_payload_kg,
        a220.reference.mzfw_kg.unwrap() - a220.reference.oew_kg.unwrap()
    );

    let dc_10 = assert_source_record(
        "DC-10",
        [
            "DC-10-30 passenger",
            "ACAP 572,000 lb option",
            "CF6-50C1",
            "DAC-67803A Rev A footnoted 572k planning option",
            "36,522 US gal: wing tanks 1 and 3, tank 2, center-wing tank, under-wing auxiliary tank",
        ],
        &[
            "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.1",
            "FAA TCDS A22WE Rev 13, 2018-04-30",
            "NASA CR-3119, April 1979",
        ],
    );
    assert_eq!(dc_10.reference.mtow_kg, Some(259_454.0));
    assert_eq!(dc_10.reference.mtow_kg, Some(dc_10.requirements.mtow_kg));
    // The ACAP Series 30 passenger column with its 572,000 lb footnote
    // applied (266,191 + 379 lb) is the registry's comparable value, and the
    // structural payload is the same column's 101,809 - 379 lb.
    assert_eq!(dc_10.reference.oew_kg, Some(120_914.0));
    assert_eq!(
        dc_10.requirements.max_structural_payload_kg,
        dc_10.reference.mzfw_kg.unwrap() - structural_payload_basis_oew_kg("DC-10")
    );
    assert_eq!(
        dc_10.reference.reference_wing_area_m2,
        Some(dc_10.requirements.max_wing_area_m2)
    );
    // The DC-10 reference wing is the Published one (338.84 m2, asserted
    // above and in `requirements`); the drawn planform with its inboard trailing-edge extension is
    // 360.652794505 m2 (+6.4 %), no longer the area the analysis uses as S_ref. Figure 2.2 chords
    // give about 368.9 m2; that value is settled together with the DC-10 chord and datum
    // entries of `source_corrections` through a built-overlay review.
    let projected = preset_projected_area(dc_10);
    assert!(
        (projected - 360.652_794_505).abs() < 1.0e-6,
        "DC-10 projected area {projected} m2",
    );
}

#[test]
fn partial_mission_evidence_never_promotes_a_capability_claim_to_a_design_mission() {
    let expected_sources: [(&str, &[&str]); 7] = [
        ("AVE", &[]),
        (
            "A340-300",
            &["Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, section 3-2-1 p.4, Figure 3-2-1-991-013-A01"],
        ),
        (
            "A380-800",
            &["Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, section 3-2-1 p.2, Figure 3-2-1-991-001-A01"],
        ),
        (
            "B787-9",
            &["Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 3.2.2 p.3-3"],
        ),
        (
            "A320-200",
            &["Airbus A320 Aircraft Characteristics Rev 46, 2026-07-01, section 3-2-1 p.3, Figure 3-2-1-991-017-A01"],
        ),
        (
            "A220-300",
            &[
                "Airbus A220 Digital Pamphlet FAI V5.2, July 2022, p.1",
                "Airbus A220-300 APP Issue 031, 2023-10-19, data module BD500-A-J00-00-00-13AAB-030A-A pp.2-3, Figure 1",
            ],
        ),
        ("DC-10", &[]),
    ];

    for (name, expected) in expected_sources {
        let preset = presets::get(name).unwrap();
        assert_eq!(
            preset.reference.design_mission_evidence,
            DesignMissionEvidence::Unverified,
            "{name} must remain unverified"
        );
        assert_eq!(
            preset
                .reference
                .partial_design_mission_evidence
                .iter()
                .map(|record| record.source)
                .collect::<Vec<_>>(),
            expected,
            "{name} partial evidence sources"
        );
        for record in &preset.reference.partial_design_mission_evidence {
            assert_ne!(
                record.kind,
                PartialMissionEvidenceKind::ActualDesignMission,
                "{name} has no public actual-design-mission record"
            );
            assert_ne!(
                record.kind,
                PartialMissionEvidenceKind::CertificationDemonstration,
                "{name} has no public certification-demonstration mission"
            );
            assert!(
                record.payload_kg.is_none(),
                "{name} has no published mission payload"
            );
            assert!(
                record.missing.contains(&MissingDesignMissionDatum::Payload),
                "{name} must name its missing payload"
            );
            assert!(
                record.missing.contains(&MissingDesignMissionDatum::Profile),
                "{name} must name its incomplete profile"
            );
            assert!(
                record
                    .missing
                    .contains(&MissingDesignMissionDatum::ReserveFuel),
                "{name} must name its missing reserve mass"
            );
        }
    }

    let a220 = presets::get("A220-300").unwrap();
    assert_eq!(
        a220.reference.partial_design_mission_evidence[0].range,
        Some(PublishedRange::NauticalMiles(3_400.0))
    );
    assert_eq!(
        a220.reference.partial_design_mission_evidence[0].kind,
        PartialMissionEvidenceKind::AdvertisedRange
    );
    assert_eq!(
        a220.reference.partial_design_mission_evidence[1].kind,
        PartialMissionEvidenceKind::PayloadRangeChart
    );

    let a380 = presets::get("A380-800").unwrap();
    let chart = &a380.reference.partial_design_mission_evidence[0];
    assert_eq!(chart.kind, PartialMissionEvidenceKind::PayloadRangeChart);
    assert_eq!(
        chart.reserve_assumptions,
        Some("200 nm diversion; 5% trip fuel allowance; 30 min holding")
    );
    assert!(chart.missing.contains(&MissingDesignMissionDatum::Range));
    assert!(chart
        .missing
        .contains(&MissingDesignMissionDatum::ReserveFuel));
}

#[test]
fn a220_source_gear_stations_are_normalized_and_topology_is_explicit() {
    let preset = presets::get("A220-300").expect("registered A220-300 preset");
    let gear = &preset.landing_gear;

    assert_eq!(gear.n_nlg_wheels, 2);
    assert_eq!(gear.n_mlg_struts, 2);
    assert_eq!(gear.wheels_per_mlg_strut, 2);
    assert_eq!(gear.mlg_strut_bogie_wheels, Some(vec![2, 2]));
    assert_eq!(
        gear.reference_station_frame.as_deref(),
        Some("nose_tip_drawing_reference")
    );
    assert_eq!(gear.reference_wheelbase_m, Some(15.23238));
    assert_eq!(gear.reference_track_m, Some(6.731));

    let source_length_m = 38.68928;
    let resolved = gear.resolved_station_positions(1.0, 2.0, 0.0, source_length_m);
    assert!(resolved.source_scaled);
    assert_eq!(resolved.main_gear_x_m.len(), 2);
    assert!((resolved.x_nlg_m - 3.401568).abs() < 1.0e-12);
    assert!((resolved.x_mlg_m - 18.633948).abs() < 1.0e-12);
    assert!((resolved.x_mlg_m - resolved.x_nlg_m - 15.23238).abs() < 1.0e-12);
    assert!(
        (resolved.main_gear_x_m[0] - resolved.main_gear_x_m[1]).abs() < 1.0e-12,
        "left and right A220 main axles share the source longitudinal station"
    );

    // The source dimensions are not frozen absolute stations. A shrink run
    // re-applies the normalized fractions to the active fuselage length.
    let shrunk_length_m = 30.0;
    let shrunk = gear.resolved_station_positions(1.0, 2.0, 0.0, shrunk_length_m);
    assert!(shrunk.source_scaled);
    assert!((shrunk.x_nlg_m - 3.401568 * shrunk_length_m / source_length_m).abs() < 1.0e-12);
    assert!((shrunk.x_mlg_m - 18.633948 * shrunk_length_m / source_length_m).abs() < 1.0e-12);
    assert!(shrunk.x_mlg_m < resolved.x_mlg_m);
}

/// Gross projected area of the built planform, through every station
/// (centreline root, side of body, kink and tip).
fn preset_projected_area(preset: &AircraftPreset) -> f64 {
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
    planform
        .stations_with_rake()
        .windows(2)
        .map(|pair| (pair[1].y_m - pair[0].y_m) * (pair[0].chord_m + pair[1].chord_m))
        .sum()
}

/// One preset in the shape the fixture records it.
///
/// The two calibrations are `null` upstream when the preset does not carry
/// one, and `None` here; both serialize to the same absence, which is what
/// makes "this type uses the global default" comparable at all.
fn as_value(preset: &AircraftPreset) -> Value {
    serde_json::json!({
        "name": preset.name,
        "display_name": preset.display_name,
        "description": preset.description,
        "engine_name": preset.engine_name,
        "n_engines": preset.n_engines,
        "design_vector": preset.design_vector,
        "geometry": preset.geometry,
        "requirements": preset.requirements,
        "mass_model": preset.mass_model,
        "performance": preset.performance,
        "engine_spanwise_positions": preset.engine_spanwise_positions(),
    })
}

/// The fields for which the source audit supersedes the frozen Python table.
///
/// Pinning both sides prevents this exception from accepting a different Rust
/// value or hiding a later, unrelated fixture change at the same path.
fn source_corrections() -> BTreeMap<String, SourceCorrection> {
    let mut corrections = [
        source_correction("A340-300.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A380-800.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("B787-9.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A320-200.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A220-300.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("DC-10.requirements.cabin_preset", "Ryanair", "Custom"),
        // AVE flies the 777-9 standard two-class cabin its reference data
        // declares (Boeing D6-86073 Rev G Table 2-1), not an airline profile.
        source_correction("AVE.requirements.cabin_preset", "Ryanair", "Custom"),
        source_correction("A340-300.requirements.num_passengers", 290.0, 335.0),
        source_correction("A380-800.requirements.num_passengers", 525.0, 555.0),
        source_correction("A220-300.requirements.num_passengers", 130.0, 140.0),
        source_correction("DC-10.requirements.num_passengers", 250.0, 255.0),
        source_correction("AVE.design_vector.root_chord_m", 16.5, 16.0),
        source_correction("AVE.design_vector.break_chord_m", 7.8, 8.0),
        source_correction(
            "A340-300.description",
            "Long-range quad-engine widebody with CFM56-5C engines.",
            "Airbus A340-312 WV029 with CFM56-5C3-family engines.",
        ),
        // Airbus A340-200/-300 AC 2-2-0: 62.84 m is the printed body length to
        // the tail-cone end; the published 63.66 m runs to the fin and tailplane
        // tip trailing points.
        source_correction("A340-300.design_vector.fuselage_length_m", 63.69, 62.84),
        source_correction("A340-300.engine_name", "CFM56-5C", "CFM56-5C3/F"),
        source_correction(
            "A340-300.geometry.engine.engine_name",
            "CFM56-5C",
            "CFM56-5C3/F",
        ),
        source_correction("A340-300.requirements.mtow_kg", 275_000.0, 260_000.0),
        source_correction(
            "A380-800.description",
            "Double-deck super-jumbo with Trent 900 engines.",
            "Airbus A380-841 WV000 with Trent 970-84 engines.",
        ),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01: the side-view vector
        // strokes close the body at the printed 70.40 m tail cut; 72.72 m runs
        // to the fin tip trailing point.
        source_correction("A380-800.design_vector.fuselage_length_m", 72.72, 70.4),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 2: 3.98 m tip
        // and 17.67 m side-of-body chords. Geometry round 2: the drawn trailing
        // edge runs straight through the body to the centreline (root 19.678 m,
        // was 17.935 m with a trailing-edge crank at the side of body). The
        // kink chord is the sheet 2 vector outline, LE 28.18 m and TE 40.23 m
        // at the kink (not 11.620 m, whose short kink
        // forced a trailing-edge bump); the published reference wing keeps
        // 845 m^2.
        source_correction(
            "A380-800.design_vector.root_chord_m",
            23.0,
            19.678_256_676_269_644,
        ),
        source_correction("A380-800.design_vector.break_chord_m", 11.3, 12.05),
        source_correction("A380-800.design_vector.tip_chord_m", 3.5, 3.98),
        source_correction("A380-800.engine_name", "Trent 900", "Trent 970-84"),
        source_correction(
            "A380-800.geometry.engine.engine_name",
            "Trent 900",
            "Trent 970-84",
        ),
        source_correction("A380-800.requirements.max_wing_area_m2", 855.0, 845.0),
        // NASA Common Research Model camber distribution (AIAA 2008-6919;
        // NASA CRM geometry page, max-camber figure; eta 0.65 section
        // crm.eta65.unswept31.5deg): camber rises outboard, so the SC(2)
        // design lift is exchanged between root and tip at fixed thickness.
        source_correction("A380-800.geometry.wing.root_airfoil", "SC2-0714", "sc20414"),
        source_correction("A380-800.geometry.wing.tip_airfoil", "sc20410", "sc20610"),
        source_correction(
            "B787-9.requirements.max_structural_payload_kg",
            52_600.0,
            52_586.0,
        ),
        source_correction("B787-9.requirements.mtow_kg", 254_000.0, 254_692.0),
        source_correction(
            "A320-200.description",
            "Short/medium-range narrow-body twin with LEAP-1A engines.",
            "Airbus A320-214 WV017 with CFM56-5B4/3 engines and sharklets.",
        ),
        source_correction("A320-200.engine_name", "LEAP-1A", "CFM56-5B4/3"),
        source_correction(
            "A320-200.geometry.engine.engine_name",
            "LEAP-1A",
            "CFM56-5B4/3",
        ),
        source_correction(
            "A220-300.description",
            "Short/medium-range narrow-body twin with PW1500G geared turbofans.",
            "BD-500-1A11 legacy-weight A220-300 with PW1521G-3 engines.",
        ),
        source_correction(
            "A220-300.requirements.max_structural_payload_kg",
            18_700.0,
            18_643.0,
        ),
        source_correction("A220-300.requirements.mtow_kg", 70_900.0, 67_585.0),
        // A220-300.requirements.dive_speed_m_s is deliberately not pinned: the
        // preset's 212.208 m/s is VD = 1.25 x VMO 330 KIAS (EASA TCDS
        // IM.A.570 Issue 25, Section 2 item 10), a model convention, since VD is
        // not published. The check stays failing.
        // The A220 calibration spreads `MassModelConfig::default()`, so it
        // carries the default landing ratio (the 777-9 benchmark, see
        // `LONG_HAUL_MLW_FRACTION_MTOW`). Loading the preset replaces it with
        // the A220's own certified 58,740 / 67,585 either way.
        source_correction(
            "A220-300.mass_model.mlw_fraction_mtow",
            0.92,
            alas_config::landing_mass_ratio::LONG_HAUL_MLW_FRACTION_MTOW,
        ),
        source_correction(
            "DC-10.description",
            "Classic long-range trijet widebody with underwing and tail-mounted CF6-50 engines.",
            "DC-10-30 ACAP 572,000 lb option with three CF6-50C1 engines.",
        ),
        source_correction("DC-10.design_vector.span_m", 50.41, 50.39),
        // Held at their earlier pins pending a built-overlay review: the
        // Figure 2.2 chords (13.974 / 7.611 / 2.80 m), the wing
        // datum (22.594 m) and the drawn area they give (about
        // 368.9 m^2 against the 360.65 m^2 pinned below) are settled together.
        source_correction("DC-10.design_vector.root_chord_m", 12.8, 13.61),
        source_correction("DC-10.design_vector.break_chord_m", 7.8, 7.564),
        source_correction("DC-10.design_vector.tip_chord_m", 1.8, 2.64),
        source_correction(
            "DC-10.display_name",
            DC_10_UPSTREAM_DISPLAY_NAME,
            DC_10_CORRECTED_DISPLAY_NAME,
        ),
        source_correction(
            "DC-10.requirements.max_structural_payload_kg",
            48_000.0,
            46_008.0,
        ),
        source_correction("DC-10.requirements.max_wing_area_m2", 338.8, 338.84),
        source_correction("DC-10.requirements.mtow_kg", 259_450.0, 259_454.0),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    add_transport_planform_corrections(&mut corrections);
    add_spanwise_panel_corrections(&mut corrections);
    add_part_power_schedule_corrections(&mut corrections);
    corrections
}

fn add_part_power_schedule_corrections(corrections: &mut BTreeMap<String, SourceCorrection>) {
    for preset in [
        "AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
    ] {
        corrections.insert(
            format!("{preset}.geometry.engine.part_power_fuel_flow_ratios"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: serde_json::json!([0.08257870, 0.25328704, 0.815, 1.0]),
            },
        );
        corrections.insert(
            format!("{preset}.geometry.engine.part_power_source"),
            SourceCorrection {
                upstream: Value::String("absent upstream".to_owned()),
                corrected: Value::String(
                    "ICAO EEDB 03/2026 family proxy: 07P27GE235 GEnx-1B74/75/P2".to_owned(),
                ),
            },
        );
    }
}

fn source_correction(
    path: impl Into<String>,
    upstream: impl Into<Value>,
    corrected: impl Into<Value>,
) -> (String, SourceCorrection) {
    (
        path.into(),
        SourceCorrection {
            upstream: upstream.into(),
            corrected: corrected.into(),
        },
    )
}

/// The wing spanwise panel count, which every registered aircraft inherits.
///
/// The frozen value is a per-section multiplier: eight panels on each lofted
/// section, so a planform with a side-of-body station meshed to 24 panels per
/// semispan and one without to 16. The product states the count absolutely
/// instead, which makes the mesh the same density on every aircraft and stops
/// a design vector that adds a station from changing the panel count
/// underneath a search (`alas_geom::aircraft::spanwise`). 24 is the count the
/// three-section presets already had, so their aerodynamics are unchanged;
/// the two-section ones refine from 16 to 24.
///
/// Each tail surface likewise carries 24 spanwise panels, a mesh-convergence change: from 6 to 48 panels Cn_beta falls
/// 8-9 % and Cy_beta 5-6 %, halving the step at each doubling, and 24 panels
/// is within 3 % of 96 (A320 Cn_beta 0.1976 / 0.1830 / 0.1789 per rad at 6 /
/// 24 / 96). The reference-compatibility mesh keeps 6.
fn add_spanwise_panel_corrections(corrections: &mut BTreeMap<String, SourceCorrection>) {
    for preset in [
        "AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
    ] {
        let (path, correction) =
            source_correction(format!("{preset}.geometry.wing.n_subdivisions"), 8, 24);
        corrections.insert(path, correction);
        let (path, correction) =
            source_correction(format!("{preset}.geometry.empennage.n_subdivisions"), 6, 24);
        corrections.insert(path, correction);
    }
}

/// Kink stations from the drawn trailing-edge breaks: A340 AC Rev 33
/// FIGURE-2-2-0-991-007-A01 sheet 2, TE break y 9.25 m of the 28.95 m semispan to
/// the winglet root; A220 ACP Fig 1 plan view, TE corner y 6.32 m;
/// DC-10 ACAP DAC-67803A Rev A Figure 2.2, Yehudi kink y 9.55 m and the
/// 3.01 m fuselage half width over the 25.195 m semispan.
fn add_transport_planform_corrections(corrections: &mut BTreeMap<String, SourceCorrection>) {
    for (preset, kink_fraction) in [
        ("AVE", 0.35),
        ("A340-300", 9.25 / 28.95),
        ("A380-800", 0.359_236_516_064_625_5),
        ("B787-9", 0.3696),
        ("A320-200", 0.379_3),
        ("A220-300", 0.36),
        ("DC-10", 0.379),
    ] {
        for (field, value) in [
            (
                "side_of_body_span_fraction",
                match preset {
                    "A320-200" => 0.115_84,
                    "A380-800" => 3.57 / 39.875,
                    "B787-9" => 2.885 / 30.06,
                    "DC-10" => 0.1195,
                    _ => 0.10,
                },
            ),
            ("kink_span_fraction", kink_fraction),
        ] {
            corrections.insert(
                format!("{preset}.geometry.wing.{field}"),
                SourceCorrection {
                    upstream: Value::String("absent upstream".to_owned()),
                    corrected: Value::from(value),
                },
            );
        }
    }
}

fn assert_source_record(
    name: &str,
    identity: [&str; 5],
    sources: &[&str],
) -> &'static AircraftPreset {
    let preset = presets::get(name).unwrap();
    assert_eq!(
        [
            preset.identity.model,
            preset.identity.weight_variant,
            preset.identity.engine_model,
            preset.identity.modification_state,
            preset.identity.tank_configuration,
        ],
        identity,
        "{name} variant identity"
    );
    for source in sources {
        assert!(
            preset.reference.sources.contains(source),
            "{name}: missing source {source}"
        );
    }
    preset
}

/// Compare two trees, reporting each disagreeing key by its path rather than
/// dumping both.
///
/// Two numbers are compared by value, so an integer upstream and a float here
/// agree when they denote the same quantity.
fn compare_values(
    comparison: &mut Comparison,
    source_corrections: &mut BTreeMap<String, SourceCorrection>,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    if let Some((frozen, sourced)) = product_corrections::published_landing_change(path) {
        compare_recorded_value(
            comparison,
            &format!("{path}: frozen landing input"),
            expected,
            &frozen,
        );
        compare_recorded_value(
            comparison,
            &format!("{path}: published landing input"),
            actual,
            &sourced,
        );
        return;
    }
    if let Some((old, new)) = product_corrections::dimensions(path) {
        compare_recorded_value(
            comparison,
            &format!("{path}: frozen dimension"),
            expected,
            &old,
        );
        compare_recorded_value(
            comparison,
            &format!("{path}: published dimension"),
            actual,
            &new,
        );
        return;
    }
    if let Some(new) = product_corrections::engine_copy(path) {
        source_corrections.remove(path);
        compare_recorded_value(
            comparison,
            &format!("{path}: selected catalogue engine"),
            actual,
            &new,
        );
        return;
    }
    if let Some(new) = product_corrections::design_dive_speed(path) {
        source_corrections.remove(path);
        compare_recorded_value(
            comparison,
            &format!("{path}: CS 25.335(b) design dive speed"),
            actual,
            &new,
        );
        return;
    }
    if let Some(correction) = source_corrections.remove(path) {
        compare_recorded_value(
            comparison,
            &format!("{path} frozen Python value"),
            expected,
            &correction.upstream,
        );
        compare_recorded_value(
            comparison,
            &format!("{path} source-backed Rust value"),
            actual,
            &correction.corrected,
        );
        return;
    }

    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(actual_value) => {
                        compare_values(
                            comparison,
                            source_corrections,
                            &child,
                            actual_value,
                            expected_value,
                        );
                    }
                    None => {
                        comparison.exact(&child, &Value::Null, expected_value);
                    }
                }
            }
            for key in actual.keys() {
                if product_corrections::native_field(path, key) {
                    continue;
                }
                if key == "optimize_passenger_capacity" && path.ends_with(".requirements") {
                    continue;
                }
                if !expected.contains_key(key) {
                    let child = format!("{path}.{key}");
                    if let Some(new) = product_corrections::engine_copy(&child)
                        .or_else(|| product_corrections::added_planform(&child))
                        .or_else(|| product_corrections::declared_airfoil_class(&child))
                        .or_else(|| product_corrections::declared_lower_holds(&child))
                        .or_else(|| declared_flap_deflection(&child))
                    {
                        source_corrections.remove(&child);
                        compare_recorded_value(comparison, &child, &actual[key], &new);
                        continue;
                    }
                    if let Some(correction) = source_corrections.remove(child.as_str()) {
                        compare_recorded_value(
                            comparison,
                            &format!("{child} frozen Python value"),
                            &Value::String("absent upstream".to_owned()),
                            &correction.upstream,
                        );
                        compare_recorded_value(
                            comparison,
                            &format!("{child} source-corrected Rust value"),
                            actual.get(key).unwrap_or(&Value::Null),
                            &correction.corrected,
                        );
                    } else {
                        comparison.exact(
                            &child,
                            &"present".to_owned(),
                            &"absent upstream".to_owned(),
                        );
                    }
                }
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if actual.len() != expected.len() {
                comparison.exact(&format!("{path}.len"), &actual.len(), &expected.len());
                return;
            }
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                compare_values(
                    comparison,
                    source_corrections,
                    &format!("{path}[{index}]"),
                    actual,
                    expected,
                );
            }
        }
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}

/// The take-off and landing flap deflections a preset documents: native fields the frozen configuration has no
/// counterpart for, pinned to the sourced literals of
/// `product_corrections::declared_flap_setting_deg` (A320 15 / 35, A340 22 /
/// 32, B787 15 / 30, DC-10 15 / 50 deg), absent otherwise.
fn declared_flap_deflection(path: &str) -> Option<Value> {
    let (name, field) = path.split_once(".performance.")?;
    let (takeoff_deg, landing_deg) = product_corrections::declared_flap_setting_deg(name)?;
    match field {
        "takeoff_flap_deflection_deg" => Some(Value::from(takeoff_deg)),
        "landing_flap_deflection_deg" => Some(Value::from(landing_deg)),
        _ => None,
    }
}

fn compare_recorded_value(
    comparison: &mut Comparison,
    path: &str,
    actual: &Value,
    expected: &Value,
) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}
