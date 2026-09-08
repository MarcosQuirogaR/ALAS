// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py (the twin-aisle entries)
// Reference: alas @ rust-port-baseline.

//! Four published twin-aisle types, from a 1970s trijet to a double-decker.
//!
//! The span of this group is the point of it. A design method calibrated on
//! one modern widebody will reproduce that widebody; whether it also
//! reproduces a DC-10 with an engine in its fin, and an A380 with two decks
//! and four engines, is what says whether the method generalizes. So the group
//! deliberately covers two, three and four engines, thirty years of structural
//! technology, and a factor of two in maximum takeoff weight.
//!
//! Every dimension here comes from the manufacturer's published specification
//! sheet, except the root and break chords, which those sheets do not give and
//! which are estimated from wing area, aspect ratio, taper and sweep.
//!
//! All four share the advanced high-lift assumptions: triple-slotted flaps
//! with leading-edge slats, which is what a twin-aisle transport has and what
//! makes its takeoff and landing speeds come out right.

include!("widebody_parts/part_01.rs");
include!("widebody_parts/part_02.rs");
