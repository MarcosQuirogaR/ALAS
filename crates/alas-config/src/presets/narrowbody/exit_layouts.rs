// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Certified exit layouts for the single-aisle presets.
//!
//! Neither source prints door stations, so these arrangements bound the seat
//! ceiling and not the cabin length.

use crate::{CertifiedExitLayout, CertifiedExitPair};

/// EASA's baseline A220-300 cabin arrangement for the legacy registered
/// variant: two Type-C pairs and one Type-III pair, whose CS 25.807(g)
/// ratings (55 + 35 + 55) are the 145-seat maximum of the source table.  The
/// 149-seat C-III*-C option requires a different exit installation and is
/// intentionally absent here.
pub const A220_300_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "C-III-C",
    pairs: &[
        CertifiedExitPair {
            exit_type: "C",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "C",
            station_m: None,
        },
    ],
    station_body_length_m: None,
    source: "EASA.IM.A.570 BD-500 TCDS Issue 24, 2026-02-20, Section 2 BD-500-1A11 III.19 p.23 (baseline C-III-C MPSC 145; Option C25631002 is required for C-III*-C 149)",
};

/// EASA's A320-200 maximum-seating arrangement.  The four stations on each
/// side are two Type-C door pairs and two Type-III overwing pairs; their
/// CS 25.807(g) ratings (55 + 35 + 35 + 55) sum to the 180-seat certified
/// maximum.  The source permits a lower maximum when an overwing exit is
/// deactivated, so this metadata is tied to the fully active WV017
/// arrangement used by the preset.
pub const A320_200_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "C-III-III-C",
    pairs: &[
        CertifiedExitPair {
            exit_type: "C",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "C",
            station_m: None,
        },
    ],
    station_body_length_m: None,
    source: "EASA.A.064 A318/A319/A320/A321 TCDS Issue 12, 2013-09-12, Section 1 III.19 (maximum certified seating 180 with all four Type III overwing exits active)",
};

/// COMAC C919 maximum-seating arrangement.  The ACAP names one forward and one
/// aft passenger cabin door and one over-wing cabin emergency exit per side
/// (02-04 Figures 1-4) and certifies a maximum of 174 seats (02-01 Table 1,
/// 174-seat high-intensity layout).  Exit classes are not printed; they are
/// read from the 02-07 door clearances: the passenger doors are 0.98 m x
/// 1.86 m (38.5 x 73.2 in, Figure 1, large enough for Type B, 32 x 72 in) and
/// the over-wing exit is 0.52 m x 1.12 m (20.5 x 44.1 in) with a 0.38 m sill
/// step (Figure 3 sheet 3, Type III).  Type B + III + B rates 75 + 35 + 75 =
/// 185 under CS 25.807(g), covering the certified 174, whereas Type C doors
/// (145) could not: the class of the doors is an inference from the size and
/// from that coverage, not a printed value.
pub const C919_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "B-III-B",
    pairs: &[
        CertifiedExitPair {
            exit_type: "B",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "III",
            station_m: None,
        },
        CertifiedExitPair {
            exit_type: "B",
            station_m: None,
        },
    ],
    station_body_length_m: None,
    source: "COMAC C919-SVV19-50009-00 ACAP: 02-01 Table 1 (maximum seating capacity 174), 02-04 Figures 1-4 (FWD and AFT cabin doors, one cabin emergency exit per side), 02-07 Figures 1 and 3 (door 0.98 x 1.86 m; over-wing exit 0.52 x 1.12 m); exit classes inferred from those dimensions",
};
