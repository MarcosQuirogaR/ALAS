// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Names that mark the surfaces the builder appends to an airplane after the
//! main wing and the stabilizers, and whether the vortex-lattice model carries
//! them.

use super::Wing;

/// Wingtip device that takes part in the aerodynamic (vortex-lattice) model.
pub const WINGTIP_DEVICE_NAME: &str = "Wingtip Device";

/// Wingtip device kept for geometry output only (render, export, outlines);
/// the vortex-lattice mesher skips it.
pub const WINGTIP_DEVICE_GEOMETRY_ONLY_NAME: &str = "Wingtip Device (geometry only)";

/// Dorsal fin carried as a lifting surface by the vortex lattice
/// (`vstab_dorsal_in_aero`).
pub const DORSAL_FIN_NAME: &str = "Dorsal Fin";

/// Dorsal fin outside the vortex lattice: part of the built aircraft, its
/// exports, renders, wetted area and drag build-up, but no lifting surface
/// (Obert, Aerodynamic Design of Transport Aircraft, ch. 32: wind tunnel data
/// show no effect on the fin lift curve up to 15 deg of sideslip).
pub const DORSAL_FIN_OUTSIDE_LATTICE_NAME: &str = "Dorsal Fin (outside the lattice)";

/// Name of a strut under a centre engine pod, followed by its number: a product
/// surface outside the vortex lattice.
pub const CENTRE_POD_STRUT_NAME: &str = "Centre Pod Strut";

/// The ventral fin pair under the tail: a product surface outside the vortex
/// lattice. Its flow is a vortex off a sharp edge, as a dorsal's is.
pub const VENTRAL_FIN_NAME: &str = "Ventral Fin";

/// The fixed fairing behind the fin over a centre engine pod: a product surface
/// outside the vortex lattice.
pub const FIN_AFT_FAIRING_NAME: &str = "Fin Aft Fairing";

impl Wing {
    /// Whether this surface is excluded from the lifting-surface models (the
    /// vortex lattice and the decks that replay it). It remains part of the
    /// geometry and of every wetted-area integral.
    #[must_use]
    pub fn is_geometry_only(&self) -> bool {
        self.name == WINGTIP_DEVICE_GEOMETRY_ONLY_NAME
            || self.name == DORSAL_FIN_OUTSIDE_LATTICE_NAME
            || self.is_ventral_fin()
            || self.is_centre_pod_surface()
    }

    /// Whether this surface is a strut or the fin fairing of a centre engine pod.
    #[must_use]
    pub fn is_centre_pod_surface(&self) -> bool {
        self.name.starts_with(CENTRE_POD_STRUT_NAME) || self.name == FIN_AFT_FAIRING_NAME
    }

    /// The plan whose area is this surface's own skin. A dorsal's built sections run on
    /// inside the fin, whose skin the fin counts, so it answers on the part ahead of the
    /// fin edge, its reference planform; a ventral fin's root is held inside the body, so
    /// it answers on the part outside it, its reference planform; any other surface on
    /// its carried box.
    #[must_use]
    pub fn skin_planform(&self) -> std::borrow::Cow<'_, Self> {
        if self.skin_planform_is_exposed() {
            self.reference_planform()
        } else {
            self.carried_box()
        }
    }

    /// Whether [`Self::skin_planform`] is already the plan outside the body, so no
    /// buried share is to be taken off it again.
    #[must_use]
    pub fn skin_planform_is_exposed(&self) -> bool {
        self.is_dorsal_fin() || self.is_ventral_fin()
    }

    /// Whether this surface is the ventral fin pair.
    #[must_use]
    pub fn is_ventral_fin(&self) -> bool {
        self.name == VENTRAL_FIN_NAME
    }

    /// Whether this surface is a dorsal fin, in the lattice or outside it.
    #[must_use]
    pub fn is_dorsal_fin(&self) -> bool {
        self.name == DORSAL_FIN_NAME || self.name == DORSAL_FIN_OUTSIDE_LATTICE_NAME
    }
}
