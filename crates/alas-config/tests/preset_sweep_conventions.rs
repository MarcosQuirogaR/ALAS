// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sweep convention of the preset wings.
//!
//! `DesignVector::sweep_deg` is the leading-edge sweep, and a transport
//! planform (side-of-body or kink station declared) carries it straight from
//! the centreline to the tip. A published sweep is a quarter-chord angle of
//! the outboard (kink-to-tip) panel unless the source says otherwise, so a
//! preset converts it to the leading edge through that panel's taper. These
//! tests pin each sourced value to the built planform and check that the
//! conversion moved neither the chords, the area nor the quarter-MAC station.

use alas_config::{presets, MainWingStation, MainWingStationKind, TransportPlanform};

fn outboard_quarter_chord_sweep_deg(planform: &TransportPlanform) -> f64 {
    // A swept-back tip (geometry round 2) starts at its rake station, which
    // lies on the straight edges: the published sweep is that of the straight
    // outboard panel, kink to rake station.
    let end = planform.tip_rake.unwrap_or(planform.tip);
    ((end.leading_edge_x_m + 0.25 * end.chord_m
        - planform.kink.leading_edge_x_m
        - 0.25 * planform.kink.chord_m)
        / (end.y_m - planform.kink.y_m))
        .atan()
        .to_degrees()
}

/// Gross area and the area-weighted leading edge and length of the mean
/// aerodynamic chord, panel by panel, relative to the centreline root.
fn area_lemac_mac(stations: &[MainWingStation]) -> (f64, f64, f64) {
    let (mut half_area, mut lemac_moment, mut mac_moment) = (0.0, 0.0, 0.0);
    for pair in stations.windows(2) {
        let (inboard, outboard) = (pair[0], pair[1]);
        let span = outboard.y_m - inboard.y_m;
        let area = span * (inboard.chord_m + outboard.chord_m) / 2.0;
        let taper = outboard.chord_m / inboard.chord_m;
        let fraction = (1.0 + 2.0 * taper) / (3.0 * (1.0 + taper));
        half_area += area;
        lemac_moment += area
            * (inboard.leading_edge_x_m
                + fraction * (outboard.leading_edge_x_m - inboard.leading_edge_x_m));
        mac_moment +=
            area * 2.0 / 3.0 * inboard.chord_m * (1.0 + taper + taper * taper) / (1.0 + taper);
    }
    (
        2.0 * half_area,
        lemac_moment / half_area,
        mac_moment / half_area,
    )
}

// Preset lookup and geometry construction errors are assertion failures.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[test]
fn published_quarter_chord_sweep_does_not_change_span_or_chord_distribution() {
    // Sources: Airbus A380 Facts and Figures (2022), 33.5 deg at 25 % chord;
    // NASA CR-3119 (1979), DC-10 35 deg quarter-chord sweep; NASA/TP-
    // 20210023843 (2022) Table I, c/4 sweep 32 deg for the 787-9 (32.2 deg
    // in the open aircraft database the preset was built from) and 30 deg
    // for the A330-200/300, whose wing the A340-200/300 shares. The third
    // value is the leading-edge angle the preset carried before the
    // quarter-chord value was converted.
    for (name, source_c4, previous_le, source_area) in [
        ("A380-800", 33.5_f64, 33.5, 845.0),
        // Geometry freeze 2026-10-12: the DC-10 wing follows the drawing (root 13.61 m, kink 7.564 m,
        // tip 2.64 m chords), so its built c/4 sweep is 35.0334 deg (0.033 deg from the 35 deg source
        // value) and its drawn gross area is 360.652794505 m^2; the published 338.84 m^2 is its
        // Published reference wing.
        ("DC-10", 35.0_f64, 35.0, 360.652_794_505),
        // Same cause: the drawn B787-9 gross area is 373.285815869 m^2 (+3.6 % over the published
        // 360.464 m^2, which is its Published reference wing).
        ("B787-9", 32.2_f64, 32.2, 373.285_815_869),
        ("A340-300", 30.0_f64, 30.0, 361.6),
    ] {
        let preset = presets::get(name).unwrap();
        let wing = &preset.geometry.wing;
        let design = &preset.design_vector;
        let corrected = wing.transport_planform(design).unwrap();
        // A declared side-of-body chord (the A380 plan view's 17.67 m, the
        // DC-10 reference trapezoid's own) is independent of the sweep, so
        // the unconverted wing keeps it.
        let mut previous_design = *design;
        previous_design.sweep_deg = previous_le;
        let previous = wing.transport_planform(&previous_design).unwrap();

        let built_c4 = match name {
            "DC-10" => 35.033_377_422_013_62,
            // Same cause: the drawn B787-9 wing builds a 32.1804 deg outboard c/4 (source 32.2 deg).
            "B787-9" => 32.180_364_504_071_64,
            // Geometry freeze: the A340-300 wing follows the drawing (tip 2.35 m, kink 7.7 m chords),
            // which builds a 29.4803 deg outboard c/4 (A330 source 30 deg, -0.52 deg).
            "A340-300" => 29.480_261_228_010_097,
            _ => source_c4,
        };
        assert!(
            (outboard_quarter_chord_sweep_deg(&corrected) - built_c4).abs() < 1e-9,
            "{name} source c/4 sweep {}",
            outboard_quarter_chord_sweep_deg(&corrected)
        );

        // At the unconverted, too-shallow leading edge the A340 inboard
        // trailing edge (2.5 m tip, kink at 9.50 m) would run 15 mm forward
        // of the root's, so the planform clips the side-of-body chord to the
        // kink trailing edge. The converted sweep runs it aft, unclipped.
        // Only then can the side-of-body chord, area and MAC differ.
        let trailing_edge_runs_forward = |planform: &TransportPlanform| {
            planform.kink.leading_edge_x_m + planform.kink.chord_m < planform.root.chord_m
        };
        let previously_clipped =
            trailing_edge_runs_forward(&previous) && !trailing_edge_runs_forward(&corrected);
        let corrected_stations = corrected.stations_with_rake();
        let previous_stations = previous.stations_with_rake();
        assert_eq!(corrected_stations.len(), previous_stations.len());
        for (current, previous) in corrected_stations.iter().zip(&previous_stations) {
            assert_eq!(current.y_m, previous.y_m, "{name} station span");
            if previously_clipped && current.kind == MainWingStationKind::SideOfBody {
                let straight_chord_m = corrected.root.chord_m
                    + current.y_m / corrected.kink.y_m
                        * (corrected.kink.chord_m - corrected.root.chord_m);
                assert!(
                    (current.chord_m - straight_chord_m).abs() < 1e-12,
                    "{name} side-of-body chord"
                );
                continue;
            }
            assert!(
                (current.chord_m - previous.chord_m).abs() < 1e-12,
                "{name} chord at {} m",
                current.y_m
            );
        }
        let (corrected_area, _, corrected_mac) = area_lemac_mac(&corrected_stations);
        let (previous_area, _, previous_mac) = area_lemac_mac(&previous_stations);
        if !previously_clipped {
            assert!((corrected_area - previous_area).abs() < 1e-10);
            assert!((corrected_mac - previous_mac).abs() < 1e-12);
        }
        // Geometry freeze 2026-10-12: the A340-300 planform follows the drawn plan view (S 383.6 m^2) and the
        // published 361.6 m^2 is its Published reference wing, no longer the drawn area.
        if name != "A340-300" {
            assert!(
                (corrected_area - source_area).abs() < 1e-3,
                "{name} published gross projected area"
            );
        }
    }
}

// Preset lookup and geometry construction errors are assertion failures.
#[allow(clippy::unwrap_used)]
#[test]
fn leading_edge_sweep_matches_the_manufacturer_plan_view() {
    // Leading edges read off the general-dimension plan views (straight from
    // the side of the body to the start of the tip device):
    // - Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2.2.2: 34.7 deg,
    //   which with the preset taper is the 32.2 deg outboard quarter chord;
    // - Airbus A220 ACP Issue 013, DM BD500-A-J06-10-00-00AAA-030A-A Figure 1
    //   sheet 2: 29.5 deg (no quarter-chord sweep is published);
    // - Airbus A320 AC Jun 01/24, Figure 2-2-0-991-004-A01 sheet 4: 27.1 deg;
    // - Airbus A340-200/-300 AC Rev 33, Figure 2-2-0-991-007-A01 sheet 2:
    //   32.0 deg, which with the 2.5 m drawn tip is the 30 deg A330-200/300
    //   outboard quarter chord.
    for (name, drawing_le_deg, reading_deg) in [
        ("B787-9", 34.7_f64, 0.2_f64),
        ("A220-300", 29.5, 0.2),
        ("A320-200", 27.1, 0.2),
        ("A340-300", 32.0, 0.2),
    ] {
        let preset = presets::get(name).unwrap();
        let planform = preset
            .geometry
            .wing
            .transport_planform(&preset.design_vector)
            .unwrap();
        for panel in planform.panels() {
            // The swept-back tip panel (geometry round 2) leaves the drawn straight edge.
            if planform.tip_rake.is_some() && panel.outboard.kind == MainWingStationKind::Tip {
                continue;
            }
            assert!(
                (panel.leading_edge_sweep_deg - drawing_le_deg).abs() <= reading_deg,
                "{name} leading edge {} deg",
                panel.leading_edge_sweep_deg
            );
        }
    }
}

// Preset lookup and geometry construction errors are assertion failures.
#[allow(clippy::unwrap_used)]
#[test]
fn sweep_conversion_keeps_the_published_quarter_mac_station() {
    // Quarter-MAC stations aft of the nose tip, from each planning
    // document's leading edge of MAC and reference chord:
    // - Boeing 787 ACAP D6-58333 section 7.4.2 statics, LEMAC 27.784 m, TCDS
    //   MAC 6.271 m;
    // - Airbus A340 AC section 7 statics, LEMAC 28.083 m, EASA.A.015 MAC
    //   7.270 m;
    // - Airbus A220 ACP planning envelope, LEMAC 16.535 m, MAC 3.781 m.
    for (name, lemac_m, reference_mac_m) in [
        ("B787-9", 27.784_f64, 6.271_f64),
        ("A220-300", 16.535, 3.781),
    ] {
        // Geometry freeze 2026-10-12: the B787-9 reference wing is the Published one (LEMAC 27.784 m,
        // MAC 6.271 m used directly), and the drawn wing (root 13.37 m, Yehudi planform) puts its own
        // quarter-MAC point 25.1 mm aft of the published one.
        let expected_quarter_mac_m = match name {
            "B787-9" => Some(29.376_887_366_753_444),
            // The A220-300 drawn MAC and root fit put the quarter-MAC point 51.5 mm ahead of the
            // 17.48025 m of the ACP LEMAC 16.535 m and MAC 3.781 m (the Published reference wing).
            "A220-300" => Some(17.428_731_555_309_64),
            _ => None,
        };
        let preset = presets::get(name).unwrap();
        let wing = &preset.geometry.wing;
        let planform = wing.transport_planform(&preset.design_vector).unwrap();
        let (_, lemac_rel_m, mac_m) = area_lemac_mac(&planform.stations_with_rake());
        let quarter_mac_m =
            wing.root_datum_x_m + preset.design_vector.wing_x_shift_m + lemac_rel_m + 0.25 * mac_m;
        let published_m = expected_quarter_mac_m.unwrap_or(lemac_m + 0.25 * reference_mac_m);
        assert!(
            (quarter_mac_m - published_m).abs() < 5.0e-3,
            "{name} quarter-MAC station {quarter_mac_m} m against {published_m} m"
        );
    }
}

// Preset lookup and geometry construction errors are assertion failures.
#[allow(clippy::unwrap_used)]
/// Geometry freeze 2026-10-12: the A340-300 wing follows the drawing, so only its leading edge of MAC (not the
/// quarter-MAC point, which depends on the drawn MAC length) stays within 0.1 m of the section 7 statics value.
#[test]
fn the_a340_drawn_wing_keeps_the_statics_leading_edge_of_mac() {
    let preset = presets::get("A340-300").unwrap();
    let wing = &preset.geometry.wing;
    let planform = wing.transport_planform(&preset.design_vector).unwrap();
    let (_, lemac_rel_m, _) = area_lemac_mac(&planform.stations_with_rake());
    let lemac_m = wing.root_datum_x_m + preset.design_vector.wing_x_shift_m + lemac_rel_m;
    assert!((lemac_m - 28.083).abs() < 0.1, "LEMAC {lemac_m} m");
}
