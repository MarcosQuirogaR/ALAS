// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Selection of the independent external-analysis stages in a full run.
//!
//! These switches deliberately live beside the aircraft configuration rather
//! than in an installation preference. A configured tool path says which
//! executable ALAS may use; this group records whether the corresponding
//! downstream evidence was requested for this particular design.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Optional external stages that consume, but do not alter, the analysed
/// aircraft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct DownstreamConfig {
    /// Whether to export the optimized geometry to OpenVSP.
    #[config(
        label = "OpenVSP geometry export",
        help = "Write the optimized aircraft as an inspectable OpenVSP script and, when OpenVSP is configured, materialize its .vsp3 project and CAD preview. This export is required before VSPAERO can run."
    )]
    pub openvsp: bool,

    /// Whether to run VSPAERO from the OpenVSP geometry.
    #[config(
        label = "VSPAERO analysis",
        help = "Run OpenVSP's VSPAERO vortex-lattice comparison on the exported optimized geometry. Requires OpenVSP geometry export and a configured VSPAERO executable; a missing installation is reported without changing the native aerodynamic result."
    )]
    pub vspaero: bool,

    /// Whether to run the independent AVL comparison.
    #[config(
        label = "AVL comparison",
        help = "Run Athena Vortex Lattice's independent take-off comparison and retain its inspectable SI deck. The Aerodynamic results selector must include AVL for the external solver to be requested."
    )]
    pub avl: bool,

    /// Whether to run the FLOWUnsteady adapter.
    #[config(
        label = "FLOWUnsteady analysis",
        help = "Run the FLOWUnsteady adapter on the optimized aircraft and retain its request, result and solver logs. Set ALAS_FLOWUNSTEADY_EXE to a compatible executable before enabling a real solve."
    )]
    pub flowunsteady: bool,
}

impl Default for DownstreamConfig {
    fn default() -> Self {
        Self {
            openvsp: true,
            vspaero: true,
            avl: true,
            flowunsteady: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DownstreamConfig;

    #[test]
    fn all_optional_external_stages_are_enabled_by_default() {
        assert_eq!(
            DownstreamConfig::default(),
            DownstreamConfig {
                openvsp: true,
                vspaero: true,
                avl: true,
                flowunsteady: true,
            }
        );
    }
}
