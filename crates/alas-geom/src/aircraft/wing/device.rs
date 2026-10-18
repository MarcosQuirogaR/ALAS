// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Names that mark the wingtip-device surface the builder appends to an
//! airplane, and whether the vortex-lattice model carries it.

use super::Wing;

/// Wingtip device that takes part in the aerodynamic (vortex-lattice) model.
pub const WINGTIP_DEVICE_NAME: &str = "Wingtip Device";

/// Wingtip device kept for geometry output only (render, export, outlines);
/// the vortex-lattice mesher skips it.
pub const WINGTIP_DEVICE_GEOMETRY_ONLY_NAME: &str = "Wingtip Device (geometry only)";

impl Wing {
    /// Whether this surface is a wingtip device excluded from the aerodynamic model.
    #[must_use]
    pub fn is_geometry_only(&self) -> bool {
        self.name == WINGTIP_DEVICE_GEOMETRY_ONLY_NAME
    }
}
