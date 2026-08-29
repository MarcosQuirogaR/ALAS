// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Aircraft geometry visualization: planform overlays, 3-view/wireframe
//! projections, per-station airfoil cross-sections, and cabin/payload
//! layouts.
//!
//! Split into topical submodules, following the pattern
//! `crates/alas-geom/src/aircraft/airfoil/` already set, so no single file grows
//! past this crate's 700-line limit: [`planform`] for the top-view overlays
//! (`figure_geometry`, `figure_planform_comparison`, `figure_design_evolution`),
//! [`threeview`] for the four-panel orthographic/isometric projection,
//! [`wireframe`] for the isolated-component 3D wireframes and the exterior
//! live-preview scene, [`airfoil`] for the per-station cross-section figure,
//! and [`cabin`] for the interior layout (2D deck plans and the 3D preview).
//! [`shared`] holds what more than one of them needs: the top-view planform
//! drawing routine and an equal-aspect axis-range fitter, since
//! [`crate::scene::Axes2D`] has no `ax.set_aspect("equal")` of its own.

mod airfoil;
mod cabin;
mod cabin_3d;
mod cabin_seat_map;
mod planform;
mod shared;
mod threeview;
mod wireframe;

pub(crate) use wireframe::{draw_fuselage_wireframe, draw_wing_wireframe};

pub use airfoil::figure_airfoil_evolution;
pub use cabin::{figure_cabin_payload, figure_main_deck_seat_map};
pub use cabin_3d::figure_cabin_payload_3d;
pub use planform::{figure_design_evolution, figure_geometry, figure_planform_comparison};
#[doc(hidden)]
pub use threeview::figure_asb_threeview;
pub use threeview::figure_threeview;
pub use wireframe::{
    figure_exterior_3d, figure_wireframe_empennage, figure_wireframe_fuselage,
    figure_wireframe_wing,
};
