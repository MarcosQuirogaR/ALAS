// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component mass estimation.
//!
//! [`torenbeek`] is the empirical wing and fuselage weight methods
//! [`breakdown`] (`alas/physics/mass.py`) calls into: the component mass
//! buildup, the mass-coordinate scaffold, and the mass-weighted centre of
//! gravity every other physics module reads.
//!
//! [`transport_weight`] is the transport-category empty-weight buildup --
//! `Weights_Transport.evaluate()` and the `empty_weight` correlation family
//! it composes. Unlike [`torenbeek`], nothing in this program's own package
//! reaches it as a general-purpose geometry model: it is an analysis attached
//! to the mission network, so it takes a narrow `TransportVehicle` carrying
//! only what those correlations read rather than a built geometry.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod analysis;
pub mod breakdown;
pub mod flops_transport;
pub mod torenbeek;
pub mod transport_weight;
pub mod wing_centroid;
