// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Published cabin arrangements: exit pairs with their door stations, and the
//! manufacturer's typical seating, for the presets whose sources print them.
//!
//! Frame: stations are metres aft of the nose tip, as the airport-planning
//! documents print them. An exit pair carries only its CS 25.807(a) type
//! letter; its passenger rating follows from the letter (CS 25.807(g)) in the
//! payload crate, so no rating is declared here.

use crate::{CertifiedExitLayout, CertifiedExitPair, SourcedPlanningCabin, SourcedSeatClass};

/// One inch, m.
const INCH_M: f64 = 0.0254;

/// Boeing 787-9 main-deck doors.
///
/// Stations are the 787-9 column of the door-location table, read as the
/// door centre (the table does not say which door edge it dimensions, so the
/// station carries half a door width, 0.53 m, of uncertainty). All four
/// pairs are Type A: the section 2.1.2 FAA exit limit of 420 seats needs
/// four pairs rated at least 105 each, and among the CS 25.807(g) types only
/// Type A (110 per pair) reaches that; four Type B pairs would rate 300.
pub const B787_9_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "A-A-A-A",
    pairs: &[
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(6.30),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(18.36),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(35.43),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(49.66),
        },
    ],
    // EASA TCDS body length, the preset's own (`american.rs`).
    station_body_length_m: Some(62.00),
    source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2.7.1 (door locations, 787-9: doors 1-4 at 20-8/6.30, 60-3/18.36, 116-3/35.43, 162-11/49.66 ft-in/m) and section 2.1.2 (FAA exit limit 420 seats)",
};

/// Boeing's typical two-class 787-9 cabin. The text of the document prints
/// the class counts and not the pitches, so the seat geometry stays at the
/// generic class defaults.
pub const B787_9_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[
        SourcedSeatClass {
            class: "Business",
            seats: 28,
            pitch_m: None,
            abreast: None,
            width_m: None,
        },
        SourcedSeatClass {
            class: "Economy",
            seats: 262,
            pitch_m: None,
            abreast: None,
            width_m: None,
        },
    ],
    source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2.1.2 (290 dual-class: 28 business, 262 economy)",
};

/// The standard Boeing 777-9 main-deck doors, flown on AVE as its 777X-class
/// stand-in (AVE's 76.72 m body is the 777-9's 76.73 m).
///
/// The four main entry/service doors print a 42 x 74 in clear opening, which
/// is at least the Type A minimum of CS 25.807(a) (42 x 72 in). The optional
/// emergency-exit/service door at 50.06 m (34 x 72 in) is not installed in
/// the standard arrangement and is left out.
pub const B777_9_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "A-A-A-A",
    pairs: &[
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(6.76),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(23.47),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(42.72),
        },
        CertifiedExitPair {
            exit_type: "A",
            station_m: Some(61.80),
        },
    ],
    station_body_length_m: Some(76.73),
    source: "Boeing 777X ACAP D6-86073 Rev G, September 2025, Table 2-3 (door locations and clear openings, 777-9: main entry/service doors 1-4 at 22-2/6.76, 77-0/23.47, 140-2/42.72, 202-9/61.80 ft-in/m, 42 x 74 in) and Figure 2-1 (length 251 ft 9 in, 76.73 m)",
};

/// Boeing's standard two-class 777-9 cabin, with the pitches the figure
/// prints and the 10-abreast, 18 in economy seat of the cross-section. The
/// business abreast is not printed and is left to the width rule.
pub const B777_9_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[
        SourcedSeatClass {
            class: "Business",
            seats: 42,
            pitch_m: Some(85.0 * INCH_M),
            abreast: None,
            width_m: None,
        },
        SourcedSeatClass {
            class: "Economy",
            seats: 384,
            pitch_m: Some(32.0 * INCH_M),
            abreast: Some(10),
            width_m: Some(18.0 * INCH_M),
        },
    ],
    source: "Boeing 777X ACAP D6-86073 Rev G, September 2025, Table 2-1 note 1 (426 two-class: 42 business, 384 economy), Figure 2-3 (85 in business and 32 in economy pitch) and Figure 2-5 (economy 10 abreast, 18 in seats)",
};

/// The ATR 72-600's exits: the forward pair and the aft entry/service doors.
///
/// Stations are read off the ATR 72-600 Factsheet (PW127M/N edition,
/// 2020-07) p.22 cabin plan "72 seats at 29'' pitch", measured from the nose
/// tip of the plan and scaled by its 18 rows drawn at the printed 29 in
/// pitch (17 pitches, 12.52 m, from the first to the last row front). The
/// reading resolution is about 0.02 m; the plan is a schematic, so each
/// station carries an engineering-estimate uncertainty of about 0.15 m. They
/// are on the 27.166 m overall length the same page prints.
///
/// * Forward pair, Type III, centre 6.11 m: the emergency-exit marker just
///   aft of the forward baggage compartment. EASA TCDS A.084 Issue 14 names
///   the Type III exits of the ATR 72-212A (p.30, JAR 25.785(h): "Flight
///   attendant seat installed between the type III exits"; p.32, the Mod 7900
///   freighter is "without type III doors nor the aft RH service door"), and
///   the plan draws the attendant seat at that pair.
/// * Aft pair, centre 20.35 m: the passenger door and the aft RH service
///   door (TCDS p.32). The TCDS prints no type letter for it. Type I is the
///   smallest CS 25.807(g) floor-level rating that, with the Type III pair
///   (35 + 45 = 80), covers the 78-seat certified maximum of the same TCDS
///   (p.38); two Type III pairs rate 70, below the 72-seat standard cabin.
pub const ATR72_600_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "III-I",
    pairs: &[
        CertifiedExitPair {
            exit_type: "III",
            station_m: Some(6.11),
        },
        CertifiedExitPair {
            exit_type: "I",
            station_m: Some(20.35),
        },
    ],
    station_body_length_m: Some(27.166),
    source: "ATR 72-600 Factsheet (PW127M/N edition, 2020-07) p.22 cabin plan scaled by its printed 29 in seat pitch (exit centres 6.11 m and 20.35 m aft of the nose, +/-0.15 m) on the 27.166 m overall length; Type III forward exits from EASA TCDS A.084 Issue 14 pp.30 and 32; aft Type I inferred from the 78-seat maximum (TCDS p.38) against the CS 25.807(g) pair ratings",
};

/// The ATR 72-600's main-deck baggage compartments, forward to aft, as
/// `(name, forward station, aft station)` in metres aft of the nose tip on
/// the 27.166 m body.
///
/// Read off the same factsheet p.22 cabin plan, at the same scale and
/// uncertainty, as [`ATR72_600_EXIT_LAYOUT`]: the forward compartment
/// between the flight deck and the forward exits (3.47-5.51 m, the LH and RH
/// blocks either side of the entry vestibule), and the aft compartment from
/// the aft doors to the aft pressure bulkhead (20.72-22.95 m). The aft
/// compartment also houses the lavatory, so the enclosed volume over its
/// extent is an upper bound. ATR prints no volume or net-mass limit for
/// either, so both come from the built geometry and stay unlimited.
pub const ATR72_600_BAGGAGE_COMPARTMENTS: [(&str, f64, f64); 2] = [
    ("Forward baggage", 3.47, 5.51),
    ("Aft baggage", 20.72, 22.95),
];

/// ATR's standard 72-seat cabin.
pub const ATR72_600_PLANNING_CABIN: SourcedPlanningCabin = SourcedPlanningCabin {
    classes: &[SourcedSeatClass {
        class: "Economy",
        seats: 72,
        pitch_m: Some(29.0 * INCH_M),
        abreast: None,
        width_m: None,
    }],
    source: "ATR 72-600 Factsheet (PW127M/N edition, 2020-07) p.22 (standard configuration, 72 seats at 29 in pitch)",
};
