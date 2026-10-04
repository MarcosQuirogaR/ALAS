// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Failure type for the component mass buildup.

use std::fmt;

use alas_config::{
    MassArchitecture, PropulsionMassMethod, StructuralMassMethod, SystemsMassMethod,
};

use crate::flops_transport::{FlopsTransportUnverifiedReason, PartialFlopsTransportBreakdown};
use crate::wing_centroid::WingCentroidError;

/// Failure returned when a selected physical mass method cannot be verified.
///
/// The Torenbeek/fraction method never returns this error. FLOPS does when
/// a required range, cabin, or installed-architecture datum is absent, so a
/// caller cannot mistake a missing physical input for a valid mass buildup.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentMassError {
    /// The selected structural coordinate model could not be resolved.
    Geometry(WingCentroidError),
    /// NASA FLOPS inputs are incomplete or internally inconsistent.
    FlopsUnverified {
        /// Stable blockers that must be resolved before using the mass.
        reasons: Vec<FlopsTransportUnverifiedReason>,
        /// Independently available component projections, never a replacement
        /// for the complete verified buildup.
        partial: Box<PartialFlopsTransportBreakdown>,
    },
    /// The airframe evaluator reported success for a selection it did not
    /// honour, leaving the structural or propulsion group empty.
    ///
    /// The pure buildup asks for both groups and cannot publish a breakdown
    /// with a silently empty slot, so this is an error rather than a zero.
    FlopsIncompleteAirframe,
    /// The compatibility selectors disagree with the one architecture field.
    ///
    /// This can only be produced by a caller constructing a configuration
    /// directly; saved files are normalized on load.  Rejecting it here keeps
    /// an in-memory hybrid from reaching either the mass buildup or the
    /// item-level ledger.
    IncoherentMassArchitecture {
        /// Authoritative architecture field.
        architecture: MassArchitecture,
        /// Systems selector supplied by the caller.
        systems: SystemsMassMethod,
        /// Structural selector supplied by the caller.
        structure: StructuralMassMethod,
        /// Propulsion selector supplied by the caller.
        propulsion: PropulsionMassMethod,
    },
}

impl fmt::Display for ComponentMassError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Geometry(error) => {
                write!(formatter, "mass-coordinate geometry could not be resolved: {error}")
            }
            Self::FlopsUnverified { reasons, .. } => {
                let details = reasons
                    .iter()
                    .map(|reason| format!("{} ({})", reason.as_str(), reason.description()))
                    .collect::<Vec<_>>()
                    .join("; ");
                if details.is_empty() {
                    write!(formatter, "FLOPS transport mass method is unverified")
                } else {
                    write!(
                        formatter,
                        "FLOPS transport mass method is unverified: {details}"
                    )
                }
            }
            Self::FlopsIncompleteAirframe => write!(
                formatter,
                "the FLOPS airframe evaluation returned no structural or propulsion group"
            ),
            Self::IncoherentMassArchitecture {
                architecture,
                systems,
                structure,
                propulsion,
            } => write!(
                formatter,
                "mass architecture {architecture:?} disagrees with derived selectors (systems: {systems:?}, structure: {structure:?}, propulsion: {propulsion:?})"
            ),
        }
    }
}

impl std::error::Error for ComponentMassError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            Self::FlopsUnverified { .. }
            | Self::FlopsIncompleteAirframe
            | Self::IncoherentMassArchitecture { .. } => None,
        }
    }
}
