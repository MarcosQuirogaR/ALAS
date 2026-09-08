// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An enumerated, sourced non-box wing inventory for a clean-sheet transport
//! wing.
//!
//! [`crate::wingbox_feedback`] reconciles an analytically sized wingbox with
//! the rest of the wing. In reference adaptation the rest of the wing is the
//! frozen empirical remainder of a baseline aircraft. A clean-sheet design has
//! no baseline, so the remainder has to be modelled item by item. This module
//! builds that item list from correlations that are functions of geometry,
//! design gross mass and load factor only -- never of the sized box -- so that
//! a stiffer or heavier sized box always produces a heavier aircraft rather
//! than being cancelled by a complementary remainder. By construction
//! `d(total wing)/d(sized box) = 1`.
//!
//! # Decomposition and sources
//!
//! | item | correlation | source |
//! |---|---|---|
//! | trailing-edge high-lift devices | Torenbeek Eq. C-10 with the 1.2 installation multiplier | Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, App. C, through [`crate::torenbeek`] |
//! | spoilers and speedbrakes | Torenbeek App. C spoiler allowance with the same multiplier | as above |
//! | leading-edge high-lift devices | movable-area increment of FLOPS Eq. 35 (`W2`) | NASA/TM-2017-219627 Vol. I, through [`crate::flops_transport::structure`] |
//! | ailerons | movable-area increment of FLOPS Eq. 35 (`W2`) | as above |
//! | fixed non-box structure, fairings, joints, tips, non-optimum | FLOPS Eq. 37 (`W3`) prorated to the planform outside the structural box | as above |
//!
//! The analytically sized box carries spar caps, spar webs, skin and ribs
//! between the spars. Its empirical counterpart is Torenbeek's *basic
//! structure* ("the cantilever spar box, skin and ribs, without any movable
//! surfaces"), which is why the plausibility diagnostics compare the box
//! against that quantity and not against a whole wing group.
//!
//! No item below carries fuel-system, hydraulic, actuation or surface-control
//! mass: those are the FLOPS propulsion and systems groups and are already
//! counted in [`crate::breakdown::MassBreakdown::propulsion`] and
//! [`crate::breakdown::MassBreakdown::systems`].
//!
//! # Units, frames and extent
//!
//! Masses are kilograms, areas square metres, centroids metres `[x, y, z]` in
//! the aircraft geometry frame, and first moments kilogram-metres. Every mass,
//! area and centroid here describes the complete wing (both halves), matching
//! [`crate::wingbox_feedback::WingExtent::FullWing`].
//!
//! # Validity domain
//!
//! Conventional cantilever metallic or lightly composite transport wings with
//! trailing-edge flaps, ailerons and a two-or-more-spar box. Strut-braced,
//! variable-sweep, all-moving and blended-wing-body configurations are outside
//! the domain of both source correlations and are not covered.

include!("wing_inventory_parts/part_01.rs");
include!("wing_inventory_parts/part_02.rs");
