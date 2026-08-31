// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`WingConfig`)
// Reference: alas @ rust-port-baseline.

//! The parts of the main wing the optimizer is not allowed to move.
//!
//! The design vector owns projected span, derived projected area, sweep, the
//! chords and the section morphing factors. What is left here is everything that decides which
//! *family* of wing those numbers describe: where along the fuselage the root
//! sits, how the defining sections are stacked vertically, how they are
//! twisted, and where the planform cranks. Two runs with different values here
//! are not searching the same design space, so these are fixed for the length
//! of a run and configurable between runs -- which is the whole reason they
//! are named fields rather than the constants the original scripts buried
//! inside their geometry builders.

include!("wing_parts/part_01.rs");
include!("wing_parts/part_02.rs");
