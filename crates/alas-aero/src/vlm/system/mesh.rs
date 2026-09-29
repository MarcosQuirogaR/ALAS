// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The quad panel mesh of an airplane and the vortex-lattice quantities derived from each panel.

use super::*;

/// Radius of the Rankine core around a horseshoe's legs, as a fraction of the
/// smaller of the panel's bound-leg length and chord, seen by collocation
/// points of other lifting surfaces only.
///
/// Within one surface the lattice places every collocation point at least
/// half a panel from the legs that matter, so no core is needed and none is
/// applied. Between surfaces the separation is set by two independent
/// meshes: a fin crossing the plane of a wing root puts collocation points a
/// few millimetres from the wing's shared-edge trailing legs (ATR72-600 at
/// 1x24: 18 mm from legs 0.13 m panels), and the bare `1 / distance` entry
/// then dominates the influence matrix and drives its pivot ratio past
/// [`MAX_PIVOT_RATIO`] with no change in the lift. A core a quarter of the
/// panel size leaves every approach closer than that regularized and every
/// other one exact.
const INTER_SURFACE_CORE_FRACTION: f64 = 0.25;

/// The leg core radius `source` presents to a collocation point on wing
/// `field_wing_index`: its own core for another surface, none for its own.
pub(in crate::vlm) fn inter_surface_core(field_wing_index: usize, source: &Panel) -> f64 {
    if source.wing_index == field_wing_index {
        0.0
    } else {
        source.leg_core_radius
    }
}

/// One panel's four quad-mesh corners and the vortex-lattice quantities
/// derived from them: the per-panel arrays `run` builds and consumes,
/// grouped so the assembly loop reads as one step per panel rather than
/// eight parallel index operations.
pub(in crate::vlm) struct Panel {
    pub(super) normal_direction: [f64; 3],
    pub(super) left_vortex_vertex: [f64; 3],
    pub(super) right_vortex_vertex: [f64; 3],
    pub(super) vortex_center: [f64; 3],
    pub(super) vortex_bound_leg: [f64; 3],
    pub(super) collocation_point: [f64; 3],
    /// Kept alongside the derived quantities above so [`VlmResult::panels`]
    /// can report the raw mesh, not just what the AIC assembly needs, see
    /// [`PanelSample`].
    pub(super) front_left: [f64; 3],
    pub(super) back_left: [f64; 3],
    pub(super) back_right: [f64; 3],
    pub(super) front_right: [f64; 3],
    pub(super) is_trailing_edge: bool,
    pub(super) wing_index: usize,
    /// Radius of the Rankine core applied to this panel's horseshoe legs when
    /// a collocation point of a *different* surface passes near them, see
    /// [`INTER_SURFACE_CORE_FRACTION`].
    leg_core_radius: f64,
}

impl Panel {
    /// Derive one panel's vortex-lattice quantities from its four quad-mesh
    /// corners, in `run`'s own front-left/back-left/back-right/front-right
    /// order.
    pub(in crate::vlm) fn from_quad(
        front_left: [f64; 3],
        back_left: [f64; 3],
        back_right: [f64; 3],
        front_right: [f64; 3],
        is_trailing_edge: bool,
        wing_index: usize,
    ) -> Result<Self, VlmError> {
        let diag1 = sub3(front_right, back_left);
        let diag2 = sub3(front_left, back_right);
        let cross = cross3(diag1, diag2);
        let area_normal = norm3(cross);
        if !area_normal.is_finite() || area_normal <= f64::EPSILON {
            return Err(VlmError::DegeneratePanel { wing_index });
        }
        let normal_direction = scale3(cross, 1.0 / area_normal);

        let left_vortex_vertex = add3(scale3(front_left, 0.75), scale3(back_left, 0.25));
        let right_vortex_vertex = add3(scale3(front_right, 0.75), scale3(back_right, 0.25));
        let vortex_center = scale3(add3(left_vortex_vertex, right_vortex_vertex), 0.5);
        let vortex_bound_leg = sub3(right_vortex_vertex, left_vortex_vertex);

        let panel_chord =
            0.5 * (norm3(sub3(front_left, back_left)) + norm3(sub3(front_right, back_right)));
        let leg_core_radius =
            INTER_SURFACE_CORE_FRACTION * norm3(vortex_bound_leg).min(panel_chord);

        let collocation_left = add3(scale3(front_left, 0.25), scale3(back_left, 0.75));
        let collocation_right = add3(scale3(front_right, 0.25), scale3(back_right, 0.75));
        let collocation_point = scale3(add3(collocation_left, collocation_right), 0.5);

        Ok(Self {
            normal_direction,
            left_vortex_vertex,
            right_vortex_vertex,
            vortex_center,
            vortex_bound_leg,
            collocation_point,
            front_left,
            back_left,
            back_right,
            front_right,
            is_trailing_edge,
            wing_index,
            leg_core_radius,
        })
    }
}

/// Mesh every wing on `airplane` into quad panels, exactly as `run`'s own
/// meshing step does: [`Wing::subdivide_sections`] with
/// [`SpacingFunction::Cosspace`] when `spanwise_resolution > 1`, then
/// [`Wing::mesh_thin_surface`] at `chordwise_resolution` with camber.
pub(super) fn mesh_panels(
    airplane: &Airplane,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<Vec<Panel>, VlmError> {
    let mut panels = Vec::new();
    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        let subdivided;
        let wing_ref: &Wing = if spanwise_resolution > 1 {
            subdivided = wing.subdivide_sections(spanwise_resolution, SpacingFunction::Cosspace)?;
            &subdivided
        } else {
            wing
        };

        let (points, faces) = wing_ref.mesh_thin_surface(chordwise_resolution, true);
        // Upstream's `(arange(len(faces)) + 1) % chordwise_resolution == 0`,
        // evaluated per wing (including its mirrored half, already appended
        // to `faces` by `mesh_thin_surface` when the wing is symmetric)
        // before the per-wing arrays are concatenated.
        for (i, face) in faces.iter().enumerate() {
            let is_trailing_edge = (i + 1) % chordwise_resolution == 0;
            panels.push(Panel::from_quad(
                points[face[0]],
                points[face[1]],
                points[face[2]],
                points[face[3]],
                is_trailing_edge,
                wing_index,
            )?);
        }
    }
    Ok(panels)
}
