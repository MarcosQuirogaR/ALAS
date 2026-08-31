// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`ObjectiveWeights`)
// Reference: alas @ rust-port-baseline.

//! The weights and thresholds shaping the cost the optimizer minimises.
//!
//! The objective is a lift-to-drag reward plus a stack of soft, continuous
//! penalties: an angle-of-attack window, a span cost, parasite drag, wing
//! loading bounds, the static margin, the centre-of-gravity envelope, the
//! fuel budget and tank volume, tail area and volume coefficients, taper
//! realism, wingbox/high-lift accommodation, wing position, fineness ratio
//! and payload shortfall.
//!
//! # Why almost everything here is soft
//!
//! Exactly one condition short-circuits the assembly and returns a flat cost:
//! a candidate that cannot be built or evaluated at all. Everything else --
//! including a design that is physically invalid, with a static margin below
//! the floor or a centre of gravity outside the envelope -- still has its
//! lift-to-drag computed, with a large but graduated penalty added on top.
//!
//! That distinction is the whole design of this module. An early return
//! throws away the gradient: a population of invalid candidates all scoring
//! the same flat cost gives the search nothing to climb, so it cannot find
//! its way back to compliance. A graduated penalty leaves the signal intact
//! and lets it.
//!
//! Fields whose names end in a weight suffix are offered as sliders rather
//! than as numbers, because only their ratio to each other means anything.
//! That rule is upstream's and catches a few thresholds that are not weights
//! at all -- a thickness floor, a fuselage length floor, the failure costs --
//! which is reproduced rather than corrected.

include!("weights_parts/part_01.rs");
include!("weights_parts/part_02.rs");
