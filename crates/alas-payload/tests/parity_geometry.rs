// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-payload::geometry` and `::oew` against
//! `alas.physics.payload`, via `golden/generators/gen_payload.py`.
//!
//! The fixture builds each case's frozen-reference aircraft through
//! `AircraftBuilder::new_reference_compatibility(config.geometry).build` on a
//! preset's own design vector -- so what is compared here is the cabin frame
//! of a real fuselage rather than of a synthetic probe. Product geometry is a
//! separate path. Four bodies are covered: the shipped default, a narrowbody,
//! a twin-aisle widebody, and the one preset whose declared height clears 1.15
//! diameters and therefore reaches the double-deck branch and its second
//! passenger deck.
//!
//! Every sampler is probed two metres ahead of the nose and three metres past
//! the tail as well as across the body, because `np.interp` clamps rather than
//! extrapolates there and a translation that extrapolated instead would agree
//! everywhere inside the aircraft and put a hold outside it. One case carries
//! a wall thicker than the nose section, which is what drives
//! `internal_half_height` onto its 0.1 m floor and `usable_width` onto zero.
//!
//! `Tier::Closed`, matching `docs/PORTING.md`: every quantity is closed-form
//! `f64` arithmetic over the already-`green` built geometry -- a linear
//! interpolation, a fraction of a half-height, a percentage of a chord. The
//! deck fractions and the double-deck verdict are compared at `Tier::Exact`,
//! since those are values copied from the source rather than computed, and a
//! deck fraction that had drifted at all would be a transcription error.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{presets, AlasConfig, DesignVector};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_payload::{oew_and_cg, CabinGeometry, DeckSpec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct DeckRecord {
    name: String,
    floor_frac: f64,
    ceil_frac: f64,
    width_factor: f64,
    is_passenger: bool,
}

#[derive(Debug, Deserialize)]
struct PerDeckRecord {
    name: String,
    floor_z: Vec<f64>,
    ceil_z: Vec<f64>,
    deck_height: Vec<f64>,
    clamp_height: Vec<f64>,
    item_z: Vec<f64>,
    usable_width: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct GeometryRecord {
    wall: f64,
    x_min: f64,
    x_max: f64,
    fus_len: f64,
    diameter_m: f64,
    cabin_start_x: f64,
    tailcone_len: f64,
    cabin_end_x: f64,
    is_double_deck: bool,
    mac: f64,
    x_wing_ac: f64,
    x_lemac: f64,
    x_wing_le: f64,
    wing_root_chord: f64,
    passenger_decks: Vec<DeckRecord>,
    lower_deck: DeckRecord,
    wing_box_x_range: [f64; 2],
    stations: Vec<f64>,
    width_at: Vec<f64>,
    height_at: Vec<f64>,
    zc_at: Vec<f64>,
    internal_half_height: Vec<f64>,
    x_to_pct_mac: Vec<f64>,
    pct_mac_to_x: Vec<f64>,
    per_deck: Vec<PerDeckRecord>,
}

#[derive(Debug, Deserialize)]
struct GeometryCase {
    name: String,
    input: Value,
    geometry: GeometryRecord,
}

#[derive(Debug, Deserialize)]
struct OewCase {
    name: String,
    masses: std::collections::BTreeMap<String, f64>,
    coords: std::collections::BTreeMap<String, [f64; 3]>,
    oew: f64,
    x_oew: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    geometry: Vec<GeometryCase>,
    oew: Vec<OewCase>,
}

/// The probe height the generator clamps items against: the drawn seat-row
/// height, tall enough that a lower hold cuts it down and a main deck does not.
const PROBE_HEIGHT: f64 = 1.25;

/// The percent-MAC stations the generator converts back to metres.
const PCT_MAC_PROBES: [f64; 6] = [-20.0, 0.0, 12.5, 25.0, 40.0, 100.0];

/// Build a case's configuration and aircraft the way the generator does: the
/// overlay through the shared loading path, and the preset's own design vector
/// where the overlay names one.
fn cabin_geometry(input: &Value) -> CabinGeometry {
    cabin_geometry_with_source_corrections(input, false)
}

fn cabin_geometry_with_source_corrections(
    input: &Value,
    use_source_corrected_geometry: bool,
) -> CabinGeometry {
    let config = AlasConfig::from_value(input).expect("the overlay loads");
    let preset_name = input
        .get("preset")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut design_vector: Option<DesignVector> = if preset_name.is_empty() {
        None
    } else {
        Some(
            presets::get(preset_name)
                .expect("the fixture names a registered preset")
                .design_vector,
        )
    };

    // Algorithm parity must compare the same input on both sides. The physical
    // audit deliberately corrected the A380 length and closed its estimated
    // taper distribution on Airbus S_ref; replay the frozen Python vector for
    // the parity fixture and test the corrected geometry separately below.
    if !use_source_corrected_geometry && preset_name == "A380-800" {
        if let Some(design) = &mut design_vector {
            design.fuselage_length_m = 72.72;
            design.root_chord_m = 23.0;
            design.break_chord_m = 11.3;
            design.tip_chord_m = 3.5;
        }
    }

    // This fixture replays historical Python geometry; product geometry is a
    // separate path and must not be mixed into the parity evidence.
    let builder = AircraftBuilder::new_reference_compatibility(Some(config.geometry.clone()));
    let plane = builder
        .build(design_vector.as_ref(), false)
        .expect("the case's aircraft builds");

    CabinGeometry::new(
        &plane,
        &builder.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("a built aircraft has a fuselage and a wing")
}

#[test]
fn the_source_corrected_a380_frame_uses_the_current_length_and_finite_mac() {
    let input = serde_json::json!({"preset": "A380-800"});
    let geometry = cabin_geometry_with_source_corrections(&input, true);

    assert_eq!(geometry.fus_len, 72.73);
    assert_eq!(geometry.x_max - geometry.x_min, 72.73);
    assert!(geometry.mac.is_finite() && geometry.mac > 0.0);
    assert!(geometry.x_lemac.is_finite());
    assert!(geometry.x_wing_ac.is_finite());
}

fn compare_deck(
    comparison: &mut Comparison,
    label: &str,
    actual: &DeckSpec,
    expected: &DeckRecord,
) {
    comparison
        .exact(
            &format!("{label}.name"),
            &actual.name.to_owned(),
            &expected.name,
        )
        .exact(
            &format!("{label}.floor_frac"),
            &actual.floor_frac,
            &expected.floor_frac,
        )
        .exact(
            &format!("{label}.ceil_frac"),
            &actual.ceil_frac,
            &expected.ceil_frac,
        )
        .exact(
            &format!("{label}.width_factor"),
            &actual.width_factor,
            &expected.width_factor,
        )
        .exact(
            &format!("{label}.is_passenger"),
            &actual.is_passenger,
            &expected.is_passenger,
        );
}

#[test]
fn the_cabin_frame_matches_python_on_every_shipped_fuselage() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");

    // The deck table and the double-deck verdict are transcribed constants and
    // a comparison, not computed quantities, so any difference at all is a
    // mistake rather than an accumulated ulp.
    let mut structure = Comparison::new("alas-payload::geometry (deck table)", Tier::Exact);
    let mut sampled = Comparison::new("alas-payload::geometry (samplers)", Tier::Closed);

    for case in &fixture.geometry {
        let g = cabin_geometry(&case.input);
        let record = &case.geometry;
        let at = |what: &str| format!("{}: {what}", case.name);

        structure.exact(
            &at("is_double_deck"),
            &g.is_double_deck,
            &record.is_double_deck,
        );
        structure.exact(
            &at("passenger_decks.len"),
            &g.passenger_decks.len(),
            &record.passenger_decks.len(),
        );
        for (index, (actual, expected)) in g
            .passenger_decks
            .iter()
            .zip(&record.passenger_decks)
            .enumerate()
        {
            compare_deck(
                &mut structure,
                &at(&format!("passenger_decks[{index}]")),
                actual,
                expected,
            );
        }
        compare_deck(
            &mut structure,
            &at("lower_deck"),
            &g.lower_deck,
            &record.lower_deck,
        );

        for (name, actual, expected) in [
            ("wall", g.wall, record.wall),
            ("x_min", g.x_min, record.x_min),
            ("x_max", g.x_max, record.x_max),
            ("fus_len", g.fus_len, record.fus_len),
            ("diameter_m", g.diameter_m, record.diameter_m),
            ("cabin_start_x", g.cabin_start_x, record.cabin_start_x),
            ("tailcone_len", g.tailcone_len, record.tailcone_len),
            ("cabin_end_x", g.cabin_end_x, record.cabin_end_x),
            ("mac", g.mac, record.mac),
            ("x_wing_ac", g.x_wing_ac, record.x_wing_ac),
            ("x_lemac", g.x_lemac, record.x_lemac),
            ("x_wing_le", g.x_wing_le, record.x_wing_le),
            ("wing_root_chord", g.wing_root_chord, record.wing_root_chord),
        ] {
            sampled.scalar(&at(name), actual, expected);
        }

        let (box_start, box_end) = g.wing_box_x_range();
        sampled
            .scalar(
                &at("wing_box_x_range[0]"),
                box_start,
                record.wing_box_x_range[0],
            )
            .scalar(
                &at("wing_box_x_range[1]"),
                box_end,
                record.wing_box_x_range[1],
            );

        let stations = &record.stations;
        sampled
            .slice(
                &at("width_at"),
                &stations.iter().map(|&x| g.width_at(x)).collect::<Vec<_>>(),
                &record.width_at,
            )
            .slice(
                &at("height_at"),
                &stations.iter().map(|&x| g.height_at(x)).collect::<Vec<_>>(),
                &record.height_at,
            )
            .slice(
                &at("zc_at"),
                &stations.iter().map(|&x| g.zc_at(x)).collect::<Vec<_>>(),
                &record.zc_at,
            )
            .slice(
                &at("internal_half_height"),
                &stations
                    .iter()
                    .map(|&x| g.internal_half_height(x))
                    .collect::<Vec<_>>(),
                &record.internal_half_height,
            )
            .slice(
                &at("x_to_pct_mac"),
                &stations
                    .iter()
                    .map(|&x| g.x_to_pct_mac(x))
                    .collect::<Vec<_>>(),
                &record.x_to_pct_mac,
            )
            .slice(
                &at("pct_mac_to_x"),
                &PCT_MAC_PROBES
                    .iter()
                    .map(|&p| g.pct_mac_to_x(p))
                    .collect::<Vec<_>>(),
                &record.pct_mac_to_x,
            );

        // The generator probes the passenger decks in order and then the
        // lower hold, so the same sequence is rebuilt here rather than
        // matched by name -- a deck that appeared in the wrong place would
        // otherwise compare against the right numbers.
        let decks: Vec<&DeckSpec> = g
            .passenger_decks
            .iter()
            .chain(std::iter::once(&g.lower_deck))
            .collect();
        structure.exact(&at("per_deck.len"), &decks.len(), &record.per_deck.len());

        for (deck, expected) in decks.iter().zip(&record.per_deck) {
            let label = |what: &str| at(&format!("{}.{what}", expected.name));
            structure.exact(&label("name"), &deck.name.to_owned(), &expected.name);
            sampled
                .slice(
                    &label("floor_z"),
                    &stations
                        .iter()
                        .map(|&x| g.floor_z(deck, x))
                        .collect::<Vec<_>>(),
                    &expected.floor_z,
                )
                .slice(
                    &label("ceil_z"),
                    &stations
                        .iter()
                        .map(|&x| g.ceil_z(deck, x))
                        .collect::<Vec<_>>(),
                    &expected.ceil_z,
                )
                .slice(
                    &label("deck_height"),
                    &stations
                        .iter()
                        .map(|&x| g.deck_height(deck, x))
                        .collect::<Vec<_>>(),
                    &expected.deck_height,
                )
                .slice(
                    &label("clamp_height"),
                    &stations
                        .iter()
                        .map(|&x| g.clamp_height(deck, x, PROBE_HEIGHT))
                        .collect::<Vec<_>>(),
                    &expected.clamp_height,
                )
                .slice(
                    &label("item_z"),
                    &stations
                        .iter()
                        .map(|&x| g.item_z(deck, x, PROBE_HEIGHT))
                        .collect::<Vec<_>>(),
                    &expected.item_z,
                )
                .slice(
                    &label("usable_width"),
                    &stations
                        .iter()
                        .map(|&x| g.usable_width(deck, x))
                        .collect::<Vec<_>>(),
                    &expected.usable_width,
                );
        }
    }

    structure.finish();
    sampled.finish();
}

#[test]
fn a_double_deck_body_is_actually_exercised_by_the_fixture() {
    // The two-deck `DeckSpec` pair is only reachable on an ovoid body, and a
    // fixture that had quietly stopped covering one would leave the whole
    // upper-deck branch unchecked while still passing.
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let double: Vec<&GeometryCase> = fixture
        .geometry
        .iter()
        .filter(|case| case.geometry.is_double_deck)
        .collect();
    assert!(
        !double.is_empty(),
        "no geometry case reaches the double-deck branch"
    );
    for case in double {
        assert_eq!(case.geometry.passenger_decks.len(), 2, "{}", case.name);
    }
}

#[test]
fn the_operating_empty_weight_and_its_balance_match_python() {
    let fixture: Fixture = alas_testkit::load("payload", "layout");
    let mut comparison = Comparison::new("alas-payload::oew::oew_and_cg", Tier::Closed);

    for case in &fixture.oew {
        let named = |name: &str| {
            *case
                .masses
                .get(name)
                .unwrap_or_else(|| panic!("case {} has no mass for {name}", case.name))
        };
        let at = |name: &str| {
            *case
                .coords
                .get(name)
                .unwrap_or_else(|| panic!("case {} has no coordinate for {name}", case.name))
        };

        let masses = MassBreakdown {
            wing: named("Wing"),
            h_stab: named("H-Stab"),
            v_stab: named("V-Stab"),
            fuselage: named("Fuselage"),
            gear: named("Gear"),
            propulsion: named("Propulsion"),
            systems: named("Systems"),
            furnishings: named("Furnishings"),
            payload: named("Payload"),
            fuel: named("Fuel"),
        };
        let coords = MassCoordinates {
            wing: at("Wing"),
            h_stab: at("H-Stab"),
            v_stab: at("V-Stab"),
            fuselage: at("Fuselage"),
            gear: at("Gear"),
            propulsion: at("Propulsion"),
            systems: at("Systems"),
            furnishings: at("Furnishings"),
            payload: at("Payload"),
            fuel: at("Fuel"),
        };

        let (oew, x_oew) = oew_and_cg(&masses, &coords);
        comparison
            .scalar(&format!("{}: oew", case.name), oew, case.oew)
            .scalar(&format!("{}: x_oew", case.name), x_oew, case.x_oew);
    }
    comparison.finish();
}
