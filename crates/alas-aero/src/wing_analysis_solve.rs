// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Surface selection and the wing-only drive of the vortex-lattice solver.
//!
//! The solver internals are untouched: this file selects surfaces, states the
//! reference quantities, assembles one lattice and reuses its factorization
//! for every evaluated angle. Frames, signs and units are stated in
//! [`super`].

use alas_config::{DesignVector, GeometryConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;

use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::vlm::{run_with_stability_derivatives, VlmResult, VlmSystem};

use super::types::{
    AlphaPoint, ResolvedCondition, SpanStation, StabilityOutcome, SurfaceShare, WingAnalysisError,
    WingAnalysisOutcome, WingReference,
};
use super::{
    diagnostics, operating_point, AttitudeInput, SurfaceSet, WingAnalysisInputs, ALPHA_LIMIT_DEG,
};

/// The surface the geometry builder names as the main lifting surface.
const MAIN_WING: &str = "Main Wing";

/// The empennage surfaces added by the include-empennage option, in the order
/// the builder lofts them.
const EMPENNAGE: [&str; 2] = ["Horizontal Stabilizer", "Vertical Stabilizer"];

/// Lift-target iteration limit and its closure tolerance in lift coefficient.
const LIFT_TARGET_ITERATIONS: usize = 8;
const LIFT_TARGET_TOLERANCE: f64 = 1.0e-6;

/// One analysable configuration: the selected surfaces with their reference
/// quantities and the moment reference the lattice resolves moments about.
///
/// The contained airplane carries no fuselage and no nacelle: it exists only
/// so the existing solver can be driven over the selected surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct WingModel {
    airplane: Airplane,
    reference: WingReference,
}

impl WingModel {
    /// Wrap already-lofted surfaces, taking moments about
    /// `moment_reference_m` in geometry axes.
    ///
    /// The reference area, span and chord are read from the first surface,
    /// which is the main wing by construction, so an included empennage never
    /// changes the quantities the coefficients are formed with.
    ///
    /// # Errors
    ///
    /// [`WingAnalysisError::MissingSurface`] when no surface is present, and
    /// [`WingAnalysisError::Geometry`] when the main wing has no usable
    /// reference area, span or chord.
    pub fn from_surfaces(
        name: &str,
        wings: Vec<Wing>,
        moment_reference_m: [f64; 3],
    ) -> Result<Self, WingAnalysisError> {
        let main = wings
            .first()
            .ok_or_else(|| WingAnalysisError::MissingSurface(MAIN_WING.to_owned()))?;
        let reference = WingReference {
            area_m2: main.reference_area(),
            span_m: main.reference_span(),
            chord_m: main.mean_aerodynamic_chord(),
            moment_reference_m,
        };
        if !(reference.area_m2.is_finite() && reference.area_m2 > 0.0)
            || !(reference.span_m.is_finite() && reference.span_m > 0.0)
            || !(reference.chord_m.is_finite() && reference.chord_m > 0.0)
        {
            return Err(WingAnalysisError::Geometry(
                "the main wing has no usable reference area, span or chord".to_owned(),
            ));
        }
        if !moment_reference_m.iter().all(|value| value.is_finite()) {
            return Err(WingAnalysisError::Geometry(
                "the moment reference point must be finite".to_owned(),
            ));
        }
        Ok(Self {
            airplane: Airplane {
                name: name.to_owned(),
                xyz_ref: moment_reference_m,
                s_ref: reference.area_m2,
                c_ref: reference.chord_m,
                b_ref: reference.span_m,
                wings,
                fuselages: Vec::new(),
            },
            reference,
        })
    }

    /// The selected surfaces as the solver sees them.
    pub fn airplane(&self) -> &Airplane {
        &self.airplane
    }

    /// The reference quantities and moment reference of this model.
    pub fn reference(&self) -> WingReference {
        self.reference
    }

    /// The modelled surface names, in lattice order.
    pub fn surface_names(&self) -> Vec<String> {
        self.airplane
            .wings
            .iter()
            .map(|wing| wing.name.clone())
            .collect()
    }

    /// The same model with a different moment reference, leaving the
    /// geometry and the reference quantities untouched.
    pub fn with_moment_reference(&self, moment_reference_m: [f64; 3]) -> Self {
        let mut model = self.clone();
        model.airplane.xyz_ref = moment_reference_m;
        model.reference.moment_reference_m = moment_reference_m;
        model
    }
}

/// Loft `geometry` at `design` and keep only the surfaces `surfaces` selects.
///
/// `moment_reference_m` defaults to the main wing's quarter mean-aerodynamic
/// chord when `None`, which is a geometric reference, not a mass property: a
/// wing analysis never reads a mass breakdown.
///
/// # Errors
///
/// [`WingAnalysisError::Geometry`] when the builder cannot loft the geometry,
/// and [`WingAnalysisError::MissingSurface`] when a selected surface is
/// absent from the lofted aircraft.
pub fn build_wing_model(
    geometry: &GeometryConfig,
    design: &DesignVector,
    surfaces: SurfaceSet,
    moment_reference_m: Option<[f64; 3]>,
) -> Result<WingModel, WingAnalysisError> {
    let airplane = AircraftBuilder::new(Some(geometry.clone())).build(Some(design), false)?;
    let main = airplane
        .wings
        .iter()
        .find(|wing| wing.name == MAIN_WING)
        .or_else(|| airplane.wings.first())
        .ok_or_else(|| WingAnalysisError::MissingSurface(MAIN_WING.to_owned()))?
        .clone();
    let mut wings = vec![main];
    if surfaces.includes_empennage() {
        for name in EMPENNAGE {
            let surface = airplane
                .wings
                .iter()
                .find(|wing| wing.name == name)
                .ok_or_else(|| WingAnalysisError::MissingSurface(name.to_owned()))?;
            wings.push(surface.clone());
        }
    }
    let reference = moment_reference_m.unwrap_or_else(|| wings[0].aerodynamic_center(0.25));
    let name = match surfaces {
        SurfaceSet::WingOnly => "ALAS Wing",
        SurfaceSet::WingAndEmpennage => "ALAS Wing and Empennage",
    };
    WingModel::from_surfaces(name, wings, reference)
}

/// Run one wing analysis over `model`.
///
/// One lattice is assembled and its factorization is reused for the reported
/// point and for every sweep angle. `cancelled` is polled between solves, so
/// a stop request is honoured at the next point rather than mid-solve, and a
/// cancelled run returns [`WingAnalysisError::Cancelled`] instead of a
/// partial outcome presented as a result.
///
/// # Errors
///
/// See [`WingAnalysisError`].
pub fn analyse(
    model: &WingModel,
    inputs: &WingAnalysisInputs,
    cancelled: &dyn Fn() -> bool,
) -> Result<WingAnalysisOutcome, WingAnalysisError> {
    let findings = inputs.validate();
    if !findings.is_empty() {
        return Err(WingAnalysisError::InvalidInputs(findings.join(" ")));
    }
    if cancelled() {
        return Err(WingAnalysisError::Cancelled);
    }
    let model = model.with_moment_reference(inputs.moment_reference_m);
    let reference = model.reference();
    let system = VlmSystem::assemble(
        model.airplane(),
        inputs.spanwise_resolution,
        inputs.chordwise_resolution,
    )?;
    let solve_at = |alpha_deg: f64| -> Result<
        (VlmResult, OperatingPoint, ResolvedCondition),
        WingAnalysisError,
    > {
        let (point, resolved) = operating_point(&inputs.condition, alpha_deg, reference.chord_m);
        let result = system.solve(&point)?;
        Ok((result, point, resolved))
    };

    let (alpha_deg, from_target) = match inputs.condition.attitude {
        AttitudeInput::AngleOfAttack(value) => (value, false),
        AttitudeInput::LiftCoefficient(target) => {
            (solve_for_lift(target, &solve_at, cancelled)?, true)
        }
    };
    if cancelled() {
        return Err(WingAnalysisError::Cancelled);
    }
    let (result, point, condition) = solve_at(alpha_deg)?;
    let dynamic_force = condition.dynamic_pressure_pa * reference.area_m2;
    let span_load = span_load(&result, &point, &condition, &reference);
    let modelled_surfaces = surface_shares(&result, &point, &model.surface_names());

    let mut sweep = Vec::with_capacity(inputs.sweep.points);
    for angle in inputs.sweep.values() {
        if cancelled() {
            return Err(WingAnalysisError::Cancelled);
        }
        let (swept, _, _) = solve_at(angle)?;
        sweep.push(AlphaPoint {
            alpha_deg: angle,
            cl: swept.cl_lift,
            cd_induced: swept.cd_drag,
            cm_pitch: swept.cm_pitch,
        });
    }

    let stability = if inputs.surfaces.includes_empennage() {
        if cancelled() {
            return Err(WingAnalysisError::Cancelled);
        }
        stability(&model, inputs, &point)?
    } else {
        None
    };

    Ok(WingAnalysisOutcome {
        inputs: *inputs,
        reference,
        condition,
        modelled_surfaces,
        cl: result.cl_lift,
        cd_induced: result.cd_drag,
        cm_pitch: result.cm_pitch,
        lift_n: result.lift,
        induced_drag_n: result.drag,
        pitch_moment_n_m: result.cm_pitch * dynamic_force * reference.chord_m,
        span_efficiency: span_efficiency(result.cl_lift, result.cd_drag, reference.aspect_ratio()),
        span_load,
        sweep,
        stability,
        diagnostics: diagnostics(&result.solve_diagnostics, system.panel_count(), inputs),
        alpha_from_lift_target: from_target,
    })
}

/// Span efficiency `CL^2 / (pi * AR * CDi)` of one solved point.
///
/// Below the guard the ratio is the quotient of two near-zero numbers: it is
/// reported as absent rather than as a number the solve does not support.
fn span_efficiency(cl: f64, cd_induced: f64, aspect_ratio: f64) -> Option<f64> {
    if cl.abs() < 1.0e-3 || cd_induced <= 1.0e-9 || !aspect_ratio.is_finite() {
        return None;
    }
    let value = cl * cl / (std::f64::consts::PI * aspect_ratio * cd_induced);
    value.is_finite().then_some(value)
}

/// The angle of attack that reaches `target` lift coefficient.
///
/// The lattice is close to linear in angle but not exactly so: the freestream
/// direction rotates with the angle, so this is a secant iteration seeded
/// from two probe angles rather than a single division by a slope.
fn solve_for_lift(
    target: f64,
    solve_at: &dyn Fn(
        f64,
    )
        -> Result<(VlmResult, OperatingPoint, ResolvedCondition), WingAnalysisError>,
    cancelled: &dyn Fn() -> bool,
) -> Result<f64, WingAnalysisError> {
    let mut low_angle = 0.0;
    let mut low_lift = solve_at(low_angle)?.0.cl_lift;
    let mut high_angle = 2.0;
    let mut high_lift = solve_at(high_angle)?.0.cl_lift;
    for _ in 0..LIFT_TARGET_ITERATIONS {
        if cancelled() {
            return Err(WingAnalysisError::Cancelled);
        }
        let slope = (high_lift - low_lift) / (high_angle - low_angle);
        if !slope.is_finite() || slope.abs() < 1.0e-9 {
            return Err(WingAnalysisError::UnreachableLift { target });
        }
        let next_angle = high_angle + (target - high_lift) / slope;
        if !next_angle.is_finite() || next_angle.abs() > ALPHA_LIMIT_DEG {
            return Err(WingAnalysisError::UnreachableLift { target });
        }
        let next_lift = solve_at(next_angle)?.0.cl_lift;
        low_angle = high_angle;
        low_lift = high_lift;
        high_angle = next_angle;
        high_lift = next_lift;
        if (high_lift - target).abs() <= LIFT_TARGET_TOLERANCE {
            return Ok(high_angle);
        }
    }
    if (high_lift - target).abs() <= 1.0e-3 {
        Ok(high_angle)
    } else {
        Err(WingAnalysisError::UnreachableLift { target })
    }
}

/// Lift carried by each modelled surface, in wind axes.
fn surface_shares(
    result: &VlmResult,
    point: &OperatingPoint,
    names: &[String],
) -> Vec<SurfaceShare> {
    let mut shares: Vec<SurfaceShare> = names
        .iter()
        .map(|name| SurfaceShare {
            name: name.clone(),
            lift_n: 0.0,
            panel_count: 0,
        })
        .collect();
    for (panel, force) in result.panels.iter().zip(&result.panel_forces_geometry) {
        let Some(share) = shares.get_mut(panel.wing_index) else {
            continue;
        };
        share.lift_n += panel_lift(*force, point);
        share.panel_count += 1;
    }
    shares
}

/// One panel's lift, N: the wind-axis `-z` component of its geometry-axis
/// force.
fn panel_lift(force: [f64; 3], point: &OperatingPoint) -> f64 {
    let (_, _, wind_z) = point.convert_axes(
        force[0],
        force[1],
        force[2],
        AxisFrame::Geometry,
        AxisFrame::Wind,
    );
    -wind_z
}

/// The main wing's spanwise load distribution at one solved point.
///
/// A station is one chordwise strip of the first surface, including the
/// mirrored half, so the distribution spans `-b/2` to `+b/2`. The local chord
/// is the distance from the strip's leading-edge midpoint to its
/// trailing-edge midpoint on the meshed camber surface.
fn span_load(
    result: &VlmResult,
    point: &OperatingPoint,
    condition: &ResolvedCondition,
    reference: &WingReference,
) -> Vec<SpanStation> {
    let semispan = 0.5 * reference.span_m;
    let dynamic_pressure = condition.dynamic_pressure_pa;
    let mut stations = Vec::new();
    let mut leading_edge: Option<[f64; 3]> = None;
    let mut strip_lift = 0.0;
    let mut strip_width = 0.0;
    let mut strip_y = 0.0;
    for (panel, force) in result.panels.iter().zip(&result.panel_forces_geometry) {
        if panel.wing_index != 0 {
            continue;
        }
        if leading_edge.is_none() {
            leading_edge = Some(midpoint(panel.front_left, panel.front_right));
            strip_width = (panel.front_right[1] - panel.front_left[1]).abs();
            strip_y = 0.5 * (panel.front_right[1] + panel.front_left[1]);
        }
        strip_lift += panel_lift(*force, point);
        if !panel.is_trailing_edge {
            continue;
        }
        let trailing_edge = midpoint(panel.back_left, panel.back_right);
        let chord = leading_edge.map_or(0.0, |edge| distance(edge, trailing_edge));
        leading_edge = None;
        // NaN in any of these means a diverged/invalid upstream state (e.g.
        // `dynamic_pressure_pa` is an explicit NaN sentinel from
        // `operating_point` for an unreachable atmosphere state, or the
        // lattice solve produced degenerate panel geometry). Such a station
        // must be rejected, not silently propagated into `section_cl`, so
        // this checks finiteness explicitly rather than relying on
        // `!(x > 0.0)`, which is false (i.e. "valid") for NaN operands on
        // this partially ordered type. Matches the reference-geometry guard
        // above (`is_finite() && x > 0.0`).
        if !(strip_width.is_finite() && strip_width > 0.0)
            || !(chord.is_finite() && chord > 0.0)
            || !(dynamic_pressure.is_finite() && dynamic_pressure > 0.0)
        {
            strip_lift = 0.0;
            continue;
        }
        let lift_per_span = strip_lift / strip_width;
        let section_cl = lift_per_span / (dynamic_pressure * chord);
        stations.push(SpanStation {
            y_m: strip_y,
            y_over_semispan: if semispan > 0.0 {
                strip_y / semispan
            } else {
                f64::NAN
            },
            chord_m: chord,
            lift_per_span_n_m: lift_per_span,
            section_cl,
            loading: if reference.chord_m > 0.0 {
                section_cl * chord / reference.chord_m
            } else {
                f64::NAN
            },
        });
        strip_lift = 0.0;
    }
    stations.sort_by(|left, right| left.y_m.total_cmp(&right.y_m));
    stations
}

/// Midpoint of two mesh corners.
fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        0.5 * (left[0] + right[0]),
        0.5 * (left[1] + right[1]),
        0.5 * (left[2] + right[2]),
    ]
}

/// Euclidean distance between two mesh points, m.
fn distance(from: [f64; 3], to: [f64; 3]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Static stability of the modelled configuration about its stated moment
/// reference.
///
/// The derivatives are the solver's own central finite differences. The
/// static margin is `(x_np - x_ref) / c_ref` in geometry axes, which equals
/// `-Cm_alpha / CL_alpha` and is positive when the neutral point lies aft of
/// the moment reference.
fn stability(
    model: &WingModel,
    inputs: &WingAnalysisInputs,
    point: &OperatingPoint,
) -> Result<Option<StabilityOutcome>, WingAnalysisError> {
    let result = run_with_stability_derivatives(
        model.airplane(),
        point,
        inputs.spanwise_resolution,
        inputs.chordwise_resolution,
    )?;
    let cl_alpha = result.d_alpha.cl_lift;
    if !cl_alpha.is_finite() || cl_alpha.abs() < 1.0e-9 {
        return Ok(None);
    }
    Ok(Some(StabilityOutcome {
        cl_alpha_per_rad: cl_alpha,
        cm_alpha_per_rad: result.d_alpha.cm_pitch,
        cy_beta_per_rad: result.d_beta.cy_side,
        cn_beta_per_rad: result.d_beta.cn_yaw,
        cl_beta_per_rad: result.d_beta.cl_roll,
        cm_q_per_rad: result.d_q.cm_pitch,
        neutral_point_x_m: result.x_np,
        static_margin: -result.d_alpha.cm_pitch / cl_alpha,
    }))
}
