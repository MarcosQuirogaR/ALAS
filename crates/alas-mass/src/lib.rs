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
//!
//! The item-level view sits beside the lumped one. [`ledger`] is the list of
//! mass items with positions, roles and centroidal tensors that every mass
//! property is computed from, [`inertia`] the closed-form tensors of the
//! solids those items are modelled as, [`tanks`] the fuel-tank arrangement
//! resolved on the built geometry, [`fuel_plan`] the typed fuel
//! decomposition under an operating rule and the burn-model seam it is
//! priced through, and [`fuel_policy`] and [`dispatch`] the rules and the
//! takeoff-mass closure that turn a design mission into a fuel load.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod analysis;
pub mod breakdown;
pub mod breguet;
pub mod dispatch;
pub mod flops_transport;
pub mod fuel_plan;
pub mod fuel_policy;
pub mod inertia;
pub mod ledger;
pub mod product_stations;
pub mod propulsion_mass;
pub mod statement;
pub mod stations;
pub mod tanks;
pub mod torenbeek;
pub mod transport_weight;
pub mod wing_centroid;
pub mod wing_inventory;
pub mod wing_reconciliation;
pub mod wingbox_feedback;
