// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-level propulsion ratings.
//!
//! A mission segment asks for an operating rating, while the selected
//! propulsion technology owns the equations and the actual force envelope.
//! Keeping this small enum in the mission crate prevents segment code from
//! depending on a particular engine family.

/// Named propulsion rating requested by a mission phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustRating {
    /// Takeoff or go-around rating.
    TakeoffGoAround,
    /// Maximum climb rating.
    MaximumClimb,
    /// Maximum continuous rating.
    MaximumContinuous,
    /// Flight idle rating.
    FlightIdle,
    /// Normal cruise rating.
    Cruise,
}
