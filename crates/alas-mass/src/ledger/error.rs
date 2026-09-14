// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why a mass ledger cannot be used.
//!
//! [`crate::ledger::MassLedger::validate`] raises every variant below, and
//! the FLOPS placement path raises the allocation mismatch. They live here
//! rather than in `ledger.rs` so that file stays the ledger data model and
//! its combination arithmetic.

use std::fmt;

/// Why a ledger cannot be used.
#[derive(Debug, Clone, PartialEq)]
pub enum LedgerError {
    /// An item has a negative, NaN or infinite mass.
    InvalidMass {
        /// The offending item.
        id: String,
        /// Its mass.
        mass_kg: f64,
    },
    /// An item has a non-finite position.
    InvalidPosition {
        /// The offending item.
        id: String,
    },
    /// An item's centroidal tensor is not a physical tensor.
    InvalidInertia {
        /// The offending item.
        id: String,
    },
    /// Two items share an identifier.
    DuplicateId {
        /// The repeated identifier.
        id: String,
    },
    /// Caller-supplied unusable-fuel rows disagree with the unusable-fuel
    /// mass the selected mass method already allocated inside a lumped
    /// group, so placing both would count that fuel twice.
    UnusableFuelAllocationMismatch {
        /// Sum of the supplied rows, kg.
        supplied_kg: f64,
        /// The mass method's own allocation, kg.
        allocated_kg: f64,
    },
}

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMass { id, mass_kg } => {
                write!(formatter, "ledger item {id} has invalid mass {mass_kg} kg")
            }
            Self::InvalidPosition { id } => {
                write!(formatter, "ledger item {id} has a non-finite position")
            }
            Self::InvalidInertia { id } => {
                write!(
                    formatter,
                    "ledger item {id} has a non-physical inertia tensor"
                )
            }
            Self::UnusableFuelAllocationMismatch {
                supplied_kg,
                allocated_kg,
            } => write!(
                formatter,
                "supplied unusable-fuel rows total {supplied_kg} kg against the mass method's \
                 {allocated_kg} kg allocation; placing both would count that fuel twice"
            ),
            Self::DuplicateId { id } => write!(formatter, "ledger item {id} is listed twice"),
        }
    }
}

impl std::error::Error for LedgerError {}
