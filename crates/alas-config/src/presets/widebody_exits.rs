// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared exit-pair arrangements of the widebodies whose airport-planning
//! documents print their doors: the A340-300 and the DC-10-30.
//!
//! Only the CS 25.807(a) type letter of each pair is declared. Its rating
//! follows from CS 25.807(g) in the payload crate (A 110, B 75, C 55, I 45,
//! II 40, III 35 per complete pair). No door station is declared because the
//! planning documents print the stations only as drawing dimensions that are
//! not tied to one body length, so the cabin keeps the generic nose and
//! tail-cone bounds; the layout is the evacuation ceiling, nothing more.

use crate::{CertifiedExitLayout, CertifiedExitPair};

const fn pair(exit_type: &'static str) -> CertifiedExitPair {
    CertifiedExitPair {
        exit_type,
        station_m: None,
    }
}

/// A340-300: three passenger/crew door pairs and one emergency-exit door
/// pair. Airbus A340-200/-300 Aircraft Characteristics (airbus_2025_a340ac,
/// Dec 01/25): plan view p.55 (doors 1, 2 and 4 passenger/crew doors, door 3
/// emergency exit door); p.80 (a dual-lane slide-raft at each of the six
/// passenger/crew doors, a single-lane slide at each of the two emergency exit
/// doors); p.81 note (single-lane slide is for a Type I door, a dual-lane
/// slide-raft is for a Type A door); p.69 door opening 1.07 m x 1.93 m, the
/// Type A minimum of CS 25.807(a) (42 x 72 in). 3 x 110 + 45 = 375, equal to
/// the certified maximum the registry already carries.
pub const A340_300_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "A-A-I-A",
    pairs: &[pair("A"), pair("A"), pair("I"), pair("A")],
    station_body_length_m: None,
    source: "Airbus A340-200/-300 Aircraft Characteristics Dec 01/25 pp.55, 69, 80-83 (doors 1, 2, 4 passenger/crew Type A 1.07 x 1.93 m; door 3 emergency exit, Type I)",
};

/// DC-10-30: four door pairs, the forward one a Type B and the others Type A.
/// Boeing DC-10 ACAP (boeing_2023_dc10acap), PDF p.20 section 2.4.1 plan:
/// entry door 1L/1R 32 x 76 in (81 x 193 cm), doors 2, 3 and 4 42 x 76 in
/// (107 x 193 cm), and the note "all entry doors are emergency exits".
/// 32 x 72 in is the Type B minimum and 42 x 72 in the Type A minimum of
/// CS 25.807(a). 75 + 3 x 110 = 405, above the 399-seat maximum of the PDF p.11
/// general-characteristics table, which then governs.
pub const DC_10_30_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "B-A-A-A",
    pairs: &[pair("B"), pair("A"), pair("A"), pair("A")],
    station_body_length_m: None,
    source: "Boeing DC-10 ACAP 2023 PDF p.20 (entry doors 1: 32 x 76 in; 2, 3, 4: 42 x 76 in; all entry doors are emergency exits) and PDF p.11 (maximum seating capacity 399)",
};
