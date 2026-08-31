// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`DeckSpec`, `CabinGeometry`)
// Reference: alas @ rust-port-baseline.

//! Where the cabin is, how wide the floor is at each station, and where each
//! deck sits inside the fuselage.
//!
//! Both layout engines, the deck-plan drawings and the cargo loader all read
//! from this one object, which is the point of it: seats packed against a
//! floor width the hold loader disagreed about would produce a cabin and a
//! belly that cannot both be in the same aeroplane. It samples the *built*
//! fuselage rather than the configuration, so a body the design vector
//! stretched is the body the interior is laid out in.
//!
//! # Two things worth knowing before reading the numbers
//!
//! The deck fractions are of the internal half-height `b`, measured from the
//! section centre with +z up, so a deck occupies the band
//! `[zc + floor_frac*b, zc + ceil_frac*b]`. Items rest *on* the floor and have
//! their height clamped to the band, which is what stops a double-deck body's
//! two cabins from overlapping vertically.
//!
//! And the double-deck test is a shape test, not a name test: a body whose
//! declared height clears 1.15 diameters is treated as an A380-style ovoid and
//! gets two passenger decks. A circular fuselage leaves `height_m` unset and
//! can never reach it, which is why the test reads the raw `Option` rather
//! than [`alas_config::FuselageConfig::effective_height_m`] -- the latter
//! falls back to the diameter, and `d >= 1.15 d` would be a different
//! question that happens to have the same answer today.
//!
//! `_xsec_width` and `_xsec_height` do not survive the translation. Upstream
//! keeps them in `stability.py` to read a cross-section that may carry either
//! a `radius` or a `width`/`height` pair; `alas-geom::aircraft::fuselage`'s
//! `FuselageXSec` resolves that in its constructor, so both accessors are the
//! fields themselves here.

include!("geometry_parts/part_01.rs");
include!("geometry_parts/part_02.rs");
