// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Certified exit layouts for the single-aisle presets.

use crate::{CertifiedExitLayout, CertifiedExitPair};

/// EASA's baseline A220-300 cabin arrangement for the legacy registered
/// variant.  Each number is the rating of the complete exit pair; the
/// physical layout has two Type-C pairs and one Type-III pair, for 145 seats
/// in the source table.  The 149-seat C-III*-C option requires a different
/// exit installation and is intentionally absent here.
pub const A220_300_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "C-III-C",
    pairs: &[
        CertifiedExitPair {
            exit_type: "C",
            capacity_per_pair: 55,
        },
        CertifiedExitPair {
            exit_type: "III",
            capacity_per_pair: 35,
        },
        CertifiedExitPair {
            exit_type: "C",
            capacity_per_pair: 55,
        },
    ],
    source: "EASA.IM.A.570 BD-500 TCDS Issue 24, 2026-02-20, Section 2 BD-500-1A11 III.19 p.23 (baseline C-III-C MPSC 145; Option C25631002 is required for C-III*-C 149)",
};

/// EASA's A320-200 maximum-seating arrangement.  The four stations on each
/// side are two Type-C door pairs and two Type-III overwing pairs; their
/// complete-pair ratings sum to the 180-seat certified maximum.  The source
/// permits a lower maximum when an overwing exit is deactivated, so this
/// metadata is tied to the fully active WV017 arrangement used by the preset.
pub const A320_200_CERTIFIED_EXIT_LAYOUT: CertifiedExitLayout = CertifiedExitLayout {
    label: "C-III-III-C",
    pairs: &[
        CertifiedExitPair {
            exit_type: "C",
            capacity_per_pair: 55,
        },
        CertifiedExitPair {
            exit_type: "III",
            capacity_per_pair: 35,
        },
        CertifiedExitPair {
            exit_type: "III",
            capacity_per_pair: 35,
        },
        CertifiedExitPair {
            exit_type: "C",
            capacity_per_pair: 55,
        },
    ],
    source: "EASA.A.064 A318/A319/A320/A321 TCDS Issue 12, 2013-09-12, Section 1 III.19 (maximum certified seating 180 with all four Type III overwing exits active)",
};
