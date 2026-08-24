// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// The in-process vortex-lattice implementation is retained as ALAS's primary
// aerodynamic model. Numerical provenance is recorded in docs/PORTING.md and
// the repository's third-party notice; this module exposes only ALAS types.

//! The in-process vortex-lattice model, scoped to what the aerodynamic,
//! stability, and dynamics stages
//! reach: constructing one with `airplane`, `op_point`, `spanwise_resolution`,
//! `chordwise_resolution` and `verbose=False`, then calling [`run`] or
//! [`run_with_stability_derivatives`]. Confirmed by grepping every
//! call in those stages.
//!
//! # Every constructor argument no call site ever overrides
//!
//! `xyz_ref` defaults to `airplane.xyz_ref`, which every call site leaves
//! unset, so [`run`] always reads it off the airplane rather than taking it
//! as a separate parameter. `run_symmetric_if_possible` defaults `false`, and
//! when a caller sets it upstream unconditionally raises
//! `NotImplementedError` before reaching the (also dead, commented-out)
//! symmetric-solve branch -- no call site in this program's inputs sets it,
//! so `run_symmetric` is always `false` and there is no symmetric-solve
//! branch to translate at all; this module has no parameter for it.
//! `vortex_core_radius` defaults `1e-8` and is never overridden, so
//! [`crate::singularities::calculate_induced_velocity_horseshoe`]'s smoothed
//! branch is the only one this module's callers ever reach.
//! `align_trailing_vortices_with_wind` defaults `false` and is never
//! overridden, so `trailing_vortex_direction` is always the constant
//! `[1, 0, 0]`, never the freestream direction.
//! `spanwise_spacing_function`/`chordwise_spacing_function` both default to
//! `np.cosspace`, matching this crate's [`SpacingFunction::Cosspace`] (for the
//! spanwise subdivision) and [`Wing::mesh_thin_surface`]'s own hardcoded
//! cosine spacing (for the chordwise stations) respectively; nothing here
//! takes either as a parameter for the same reason.
//!
//! # `run`'s panel mesh
//!
//! Every wing is optionally [`Wing::subdivide_sections`]'d (only when
//! `spanwise_resolution > 1`, upstream's own guard -- at the default
//! resolution of `1` this branch is skipped entirely, but
//! `AnalysisConfig.fine_spanwise_resolution` defaults to `2` and is used by
//! the full-analysis path through this same code, so the branch is real
//! production behavior and not hypothetical; see `docs/PORTING.md`), then
//! meshed with [`Wing::mesh_thin_surface`] at `chordwise_resolution`,
//! `add_camber=true`. `is_trailing_edge` and `areas`, upstream's other two
//! per-panel byproducts of this step, are not computed here: neither is read
//! by anything [`run`] itself does with the mesh -- `is_trailing_edge` only
//! feeds `calculate_streamlines`'s seed-point heuristic and `areas` is never
//! read at all in `run` -- and both are P11-only (`calculate_streamlines`) or
//! entirely unused, confirmed against the upstream source read in full for
//! this row.
//!
//! # `run_with_stability_derivatives`
//!
//! Reached only from `alas/physics/dynamics.py`'s `compute_dynamic_modes`
//! (P7), which needs the full derivative set (`alpha, beta, p, q, r` all
//! `true`, per that module's own doc comment). It is straightforward
//! finite-differencing on top of [`run`] -- re-running it once per perturbed
//! state variable and taking a forward difference -- with nothing numerically
//! new, and lives in the [`stability_derivatives`] submodule. See
//! `docs/PORTING.md` for why its five per-axis boolean flags are not
//! translated as parameters.
//!
//! # Left untranslated
//!
//! `get_induced_velocity_at_points`/
//! `get_velocity_at_points` as *standalone* public entry points, `draw` and
//! `calculate_streamlines` are P11 (Figures) concerns; the induced-velocity
//! computation itself is translated as a private helper [`run`] calls
//! internally for the near-field force, the same way upstream's
//! `get_velocity_at_points` does.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{SpacingFunction, SubdivideSectionsError, Wing};
use alas_math::linalg;

use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::singularities::calculate_induced_velocity_horseshoe;
use crate::vector3::{add3, cross3, dot3, norm3, scale3, sub3};

pub mod stability_derivatives;
pub mod streamlines;

pub use stability_derivatives::{
    run_with_stability_derivatives, CoefficientDerivatives, VlmStabilityResult,
};
pub use streamlines::{calculate_streamlines, PanelSample};

/// The Kaufmann vortex core smoothing radius `VortexLatticeMethod`'s
/// constructor defaults to and every call site in this program's inputs
/// leaves unset -- see the module doc.
const VORTEX_CORE_RADIUS: f64 = 1e-8;

/// The trailing-leg direction every call site in this program's inputs gets,
/// since `align_trailing_vortices_with_wind` is never set `true` -- see the
/// module doc.
const TRAILING_VORTEX_DIRECTION: [f64; 3] = [1.0, 0.0, 0.0];

/// Why [`run`] could not solve.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum VlmError {
    /// Subdividing a wing's spanwise sections failed. Reachable only through
    /// [`SubdivideSectionsError::Blend`] in practice: `run` only calls
    /// [`Wing::subdivide_sections`] when `spanwise_resolution > 1`, which
    /// always satisfies that method's own `ratio >= 2` requirement, so
    /// [`SubdivideSectionsError::RatioTooSmall`] cannot occur from this call
    /// path.
    #[error("subdividing a wing's spanwise sections failed: {0}")]
    Subdivide(#[from] SubdivideSectionsError),
    /// The AIC matrix was numerically singular at the named elimination step.
    /// Not expected for a well-formed mesh -- a horseshoe's self-influence on
    /// its own collocation point is always well defined -- but library code
    /// reports a numerical surprise rather than panicking on it
    /// (`CONTRIBUTING.md`).
    #[error("the panel influence matrix was numerically singular at row {0}")]
    SingularAic(usize),
}

/// Every field `run`'s upstream docstring lists, plus the solved circulation
/// vector -- `VortexLatticeMethod.run`'s returned `dict`, restated as a typed
/// struct. `alas-aero::analysis` and, eventually, `alas-stab` (P7) each read
/// a different subset of this, so nothing here is trimmed to only what
/// today's callers use.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmResult {
    /// Net aerodynamic force in geometry axes, N -- `F_g`.
    pub force_geometry: [f64; 3],
    /// Net aerodynamic force in body axes, N -- `F_b`.
    pub force_body: [f64; 3],
    /// Net aerodynamic force in wind axes, N -- `F_w`.
    pub force_wind: [f64; 3],
    /// Net aerodynamic moment about geometry axes, Nm -- `M_g`.
    pub moment_geometry: [f64; 3],
    /// Net aerodynamic moment about body axes, Nm -- `M_b`.
    pub moment_body: [f64; 3],
    /// Net aerodynamic moment about wind axes, Nm -- `M_w`.
    pub moment_wind: [f64; 3],
    /// Lift, N, wind axes by definition -- `L`.
    pub lift: f64,
    /// Drag, N, wind axes by definition -- `D`.
    pub drag: f64,
    /// Side force, N, wind axes -- `Y`.
    pub side_force: f64,
    /// Rolling moment about the body X axis, Nm; positive is roll-right --
    /// `l_b`.
    pub roll_moment: f64,
    /// Pitching moment about the body Y axis, Nm; positive is pitch-up --
    /// `m_b`.
    pub pitch_moment: f64,
    /// Yawing moment about the body Z axis, Nm; positive is nose-right --
    /// `n_b`.
    pub yaw_moment: f64,
    /// Lift coefficient -- `CL`.
    pub cl_lift: f64,
    /// Drag coefficient -- `CD`.
    pub cd_drag: f64,
    /// Side-force coefficient -- `CY`.
    pub cy_side: f64,
    /// Rolling-moment coefficient, body axes -- `Cl`.
    pub cl_roll: f64,
    /// Pitching-moment coefficient, body axes -- `Cm`.
    pub cm_pitch: f64,
    /// Yawing-moment coefficient, body axes -- `Cn`.
    pub cn_yaw: f64,
    /// The solved circulation strength of every horseshoe vortex, in the
    /// panel order `run` built the mesh in. Not part of upstream's returned
    /// `dict` -- it lives on the solved `VortexLatticeMethod` instance as
    /// `self.vortex_strengths` instead -- but returned here since this port
    /// has no persistent instance to hang it on afterward, and it is the
    /// strongest available check that panel assembly and the solve are both
    /// right (a wrong AIC assembly can still integrate to a coincidentally
    /// close total force).
    pub vortex_strengths: Vec<f64>,
    /// Every panel's raw mesh geometry, aligned with [`Self::vortex_strengths`]
    /// -- upstream's per-panel instance arrays (`front_left_vertices`, ...,
    /// `is_trailing_edge`), exposed for figures that read more than the net
    /// totals above: the spanwise lift distribution and the wake streamlines.
    pub panels: Vec<PanelSample>,
    /// Each panel's own aerodynamic force in geometry axes, N, aligned with
    /// [`Self::panels`] -- the per-panel terms [`Self::force_geometry`] sums.
    /// Not part of upstream's returned `dict` for the same reason
    /// `vortex_strengths` is not: it lives on the solved instance instead
    /// (`self.forces_geometry`).
    pub panel_forces_geometry: Vec<[f64; 3]>,
}

/// One panel's four quad-mesh corners and the vortex-lattice quantities
/// derived from them -- the per-panel arrays `run` builds and consumes,
/// grouped so the assembly loop below reads as one step per panel rather
/// than eight parallel index operations.
struct Panel {
    normal_direction: [f64; 3],
    left_vortex_vertex: [f64; 3],
    right_vortex_vertex: [f64; 3],
    vortex_center: [f64; 3],
    vortex_bound_leg: [f64; 3],
    collocation_point: [f64; 3],
    /// Kept alongside the derived quantities above so [`VlmResult::panels`]
    /// can report the raw mesh, not just what the AIC assembly needs -- see
    /// [`streamlines::PanelSample`].
    front_left: [f64; 3],
    back_left: [f64; 3],
    back_right: [f64; 3],
    front_right: [f64; 3],
    is_trailing_edge: bool,
    wing_index: usize,
}

impl Panel {
    /// Derive one panel's vortex-lattice quantities from its four quad-mesh
    /// corners, in `run`'s own front-left/back-left/back-right/front-right
    /// order.
    fn from_quad(
        front_left: [f64; 3],
        back_left: [f64; 3],
        back_right: [f64; 3],
        front_right: [f64; 3],
        is_trailing_edge: bool,
        wing_index: usize,
    ) -> Self {
        let diag1 = sub3(front_right, back_left);
        let diag2 = sub3(front_left, back_right);
        let cross = cross3(diag1, diag2);
        let normal_direction = scale3(cross, 1.0 / norm3(cross));

        let left_vortex_vertex = add3(scale3(front_left, 0.75), scale3(back_left, 0.25));
        let right_vortex_vertex = add3(scale3(front_right, 0.75), scale3(back_right, 0.25));
        let vortex_center = scale3(add3(left_vortex_vertex, right_vortex_vertex), 0.5);
        let vortex_bound_leg = sub3(right_vortex_vertex, left_vortex_vertex);

        let collocation_left = add3(scale3(front_left, 0.25), scale3(back_left, 0.75));
        let collocation_right = add3(scale3(front_right, 0.25), scale3(back_right, 0.75));
        let collocation_point = scale3(add3(collocation_left, collocation_right), 0.5);

        Self {
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
        }
    }
}

/// Mesh every wing on `airplane` into quad panels, exactly as `run`'s own
/// meshing step does: [`Wing::subdivide_sections`] with
/// [`SpacingFunction::Cosspace`] when `spanwise_resolution > 1`, then
/// [`Wing::mesh_thin_surface`] at `chordwise_resolution` with camber.
fn mesh_panels(
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
        panels.extend(faces.iter().enumerate().map(|(i, face)| {
            let is_trailing_edge = (i + 1) % chordwise_resolution == 0;
            Panel::from_quad(
                points[face[0]],
                points[face[1]],
                points[face[2]],
                points[face[3]],
                is_trailing_edge,
                wing_index,
            )
        }));
    }
    Ok(panels)
}

/// The velocity every horseshoe vortex (strength `vortex_strengths[j]`)
/// induces at `points[i]`, summed over every panel, plus the freestream and
/// rotation-induced velocity at that point -- `get_velocity_at_points`
/// (through `get_induced_velocity_at_points`), scoped to the internal use
/// [`run`] makes of it. See the module doc for why the two upstream methods
/// are collapsed into this one non-broadcast helper.
fn velocity_at_points(
    points: &[[f64; 3]],
    panels: &[Panel],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
    steady_freestream_velocity: [f64; 3],
) -> Vec<[f64; 3]> {
    let rotation_velocities = op_point.rotation_velocity_geometry_axes(points);
    points
        .iter()
        .zip(rotation_velocities)
        .map(|(&point, rotation_velocity)| {
            let induced = panels.iter().zip(vortex_strengths).fold(
                [0.0, 0.0, 0.0],
                |acc, (panel, &gamma)| {
                    let contribution = calculate_induced_velocity_horseshoe(
                        point,
                        panel.left_vortex_vertex,
                        panel.right_vortex_vertex,
                        TRAILING_VORTEX_DIRECTION,
                        gamma,
                        VORTEX_CORE_RADIUS,
                    );
                    add3(acc, contribution)
                },
            );
            add3(induced, add3(steady_freestream_velocity, rotation_velocity))
        })
        .collect()
}

/// Run a vortex-lattice solve of `airplane` at `op_point` -- `VortexLatticeMethod(...).run()`,
/// with the constructor arguments folded in as documented above.
/// `spanwise_resolution`/`chordwise_resolution` are the only two constructor
/// arguments this program's call sites ever vary.
///
/// # Errors
///
/// See [`VlmError`].
pub fn run(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<VlmResult, VlmError> {
    let panels = mesh_panels(airplane, spanwise_resolution, chordwise_resolution)?;
    let n = panels.len();

    let steady_freestream_velocity = op_point.freestream_velocity_geometry_axes();

    let collocation_points: Vec<[f64; 3]> = panels.iter().map(|p| p.collocation_point).collect();
    let rotation_at_collocation = op_point.rotation_velocity_geometry_axes(&collocation_points);

    let freestream_influences: Vec<f64> = panels
        .iter()
        .zip(&rotation_at_collocation)
        .map(|(panel, &rotation_velocity)| {
            let freestream_velocity = add3(steady_freestream_velocity, rotation_velocity);
            dot3(freestream_velocity, panel.normal_direction)
        })
        .collect();

    // AIC[i][j]: the velocity horseshoe j (unit strength) induces at
    // collocation point i, dotted with that collocation panel's own normal.
    let mut aic = vec![vec![0.0; n]; n];
    for (i, collocation_panel) in panels.iter().enumerate() {
        for (j, source_panel) in panels.iter().enumerate() {
            let induced = calculate_induced_velocity_horseshoe(
                collocation_panel.collocation_point,
                source_panel.left_vortex_vertex,
                source_panel.right_vortex_vertex,
                TRAILING_VORTEX_DIRECTION,
                1.0,
                VORTEX_CORE_RADIUS,
            );
            aic[i][j] = dot3(induced, collocation_panel.normal_direction);
        }
    }

    let rhs: Vec<Vec<f64>> = freestream_influences
        .iter()
        .map(|&value| vec![-value])
        .collect();
    let solved = linalg::solve(&aic, &rhs).map_err(VlmError::SingularAic)?;
    let vortex_strengths: Vec<f64> = solved.into_iter().map(|row| row[0]).collect();

    let vortex_centers: Vec<[f64; 3]> = panels.iter().map(|p| p.vortex_center).collect();
    let v_centers = velocity_at_points(
        &vortex_centers,
        &panels,
        &vortex_strengths,
        op_point,
        steady_freestream_velocity,
    );

    let density = op_point.atmosphere.density();
    let mut force_geometry = [0.0; 3];
    let mut moment_geometry = [0.0; 3];
    // Recorded per panel as the loop goes, alongside the running totals it
    // has always computed -- same operations in the same order, so the totals
    // above are unaffected; this is only an additional read-out.
    let mut panel_forces_geometry = Vec::with_capacity(n);
    for ((panel, &gamma), &v_center) in panels.iter().zip(&vortex_strengths).zip(&v_centers) {
        let vi_cross_li = cross3(v_center, panel.vortex_bound_leg);
        let force_panel = scale3(vi_cross_li, density * gamma);
        let moment_panel = cross3(sub3(panel.vortex_center, airplane.xyz_ref), force_panel);
        force_geometry = add3(force_geometry, force_panel);
        moment_geometry = add3(moment_geometry, moment_panel);
        panel_forces_geometry.push(force_panel);
    }

    let (fbx, fby, fbz) = op_point.convert_axes(
        force_geometry[0],
        force_geometry[1],
        force_geometry[2],
        AxisFrame::Geometry,
        AxisFrame::Body,
    );
    let force_body = [fbx, fby, fbz];
    let (fwx, fwy, fwz) = op_point.convert_axes(
        force_body[0],
        force_body[1],
        force_body[2],
        AxisFrame::Body,
        AxisFrame::Wind,
    );
    let force_wind = [fwx, fwy, fwz];

    let (mbx, mby, mbz) = op_point.convert_axes(
        moment_geometry[0],
        moment_geometry[1],
        moment_geometry[2],
        AxisFrame::Geometry,
        AxisFrame::Body,
    );
    let moment_body = [mbx, mby, mbz];
    let (mwx, mwy, mwz) = op_point.convert_axes(
        moment_body[0],
        moment_body[1],
        moment_body[2],
        AxisFrame::Body,
        AxisFrame::Wind,
    );
    let moment_wind = [mwx, mwy, mwz];

    let lift = -force_wind[2];
    let drag = -force_wind[0];
    let side_force = force_wind[1];
    let roll_moment = moment_body[0];
    let pitch_moment = moment_body[1];
    let yaw_moment = moment_body[2];

    let q = op_point.dynamic_pressure();
    let s_ref = airplane.s_ref;
    let b_ref = airplane.b_ref;
    let c_ref = airplane.c_ref;

    Ok(VlmResult {
        force_geometry,
        force_body,
        force_wind,
        moment_geometry,
        moment_body,
        moment_wind,
        lift,
        drag,
        side_force,
        roll_moment,
        pitch_moment,
        yaw_moment,
        cl_lift: lift / q / s_ref,
        cd_drag: drag / q / s_ref,
        cy_side: side_force / q / s_ref,
        cl_roll: roll_moment / q / s_ref / b_ref,
        cm_pitch: pitch_moment / q / s_ref / c_ref,
        cn_yaw: yaw_moment / q / s_ref / b_ref,
        vortex_strengths,
        panels: panels
            .iter()
            .map(|p| streamlines::PanelSample {
                front_left: p.front_left,
                back_left: p.back_left,
                back_right: p.back_right,
                front_right: p.front_right,
                left_vortex_vertex: p.left_vortex_vertex,
                right_vortex_vertex: p.right_vortex_vertex,
                vortex_center: p.vortex_center,
                is_trailing_edge: p.is_trailing_edge,
                wing_index: p.wing_index,
            })
            .collect(),
        panel_forces_geometry,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_atmo::Atmosphere;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn flat_rectangular_wing(symmetric: bool) -> Wing {
        Wing::new(
            "Flat",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    fn single_wing_airplane(symmetric: bool) -> Airplane {
        let wing = flat_rectangular_wing(symmetric);
        let s_ref = wing.area();
        let b_ref = wing.span();
        let c_ref = wing.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.25, 0.0, 0.0],
            wings: vec![wing],
            fuselages: Vec::new(),
            s_ref,
            c_ref,
            b_ref,
        }
    }

    fn level_flight_point(alpha: f64) -> OperatingPoint {
        OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha, 0.0, 0.0, 0.0, 0.0)
    }

    #[test]
    fn a_symmetric_flat_plate_at_zero_alpha_produces_no_lift() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(0.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(
            result.lift.abs() < 1.0,
            "an uncambered symmetric wing at zero incidence should carry ~no lift, got {}",
            result.lift
        );
    }

    #[test]
    fn positive_angle_of_attack_produces_positive_lift() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(5.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(result.lift > 0.0, "lift={}", result.lift);
        assert!(result.cl_lift > 0.0);
    }

    #[test]
    fn a_symmetric_wing_at_zero_beta_produces_no_side_force_or_yaw_or_roll() {
        let airplane = single_wing_airplane(true);
        let op_point = level_flight_point(4.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        assert!(result.side_force.abs() < 1e-6, "Y={}", result.side_force);
        assert!(result.yaw_moment.abs() < 1e-6, "n_b={}", result.yaw_moment);
        assert!(
            result.roll_moment.abs() < 1e-6,
            "l_b={}",
            result.roll_moment
        );
    }

    #[test]
    fn vortex_strengths_has_one_entry_per_panel() {
        let airplane = single_wing_airplane(false);
        let op_point = level_flight_point(3.0);
        let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
        // 1 spanwise interval * 4 chordwise panels, unmirrored.
        assert_eq!(result.vortex_strengths.len(), 4);
    }

    #[test]
    fn spanwise_resolution_above_one_multiplies_the_panel_count() {
        let airplane = single_wing_airplane(false);
        let op_point = level_flight_point(3.0);
        let coarse = run(&airplane, &op_point, 1, 2).expect("well-posed solve");
        let fine = run(&airplane, &op_point, 3, 2).expect("well-posed solve");
        assert_eq!(coarse.vortex_strengths.len(), 2);
        assert_eq!(fine.vortex_strengths.len(), 3 * 2);
    }

    #[test]
    fn doubling_the_freestream_velocity_leaves_the_lift_coefficient_unchanged() {
        // CL depends on alpha, not on the airspeed itself, for an inviscid
        // linear solve -- doubling V quadruples both L and q, so CL should be
        // invariant.
        let airplane = single_wing_airplane(true);
        let slow = OperatingPoint::new(Atmosphere::new(0.0), 40.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        let fast = OperatingPoint::new(Atmosphere::new(0.0), 80.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        let slow_result = run(&airplane, &slow, 1, 4).expect("well-posed solve");
        let fast_result = run(&airplane, &fast, 1, 4).expect("well-posed solve");
        assert!(
            (slow_result.cl_lift - fast_result.cl_lift).abs() < 1e-9,
            "slow CL={} fast CL={}",
            slow_result.cl_lift,
            fast_result.cl_lift
        );
    }
}
