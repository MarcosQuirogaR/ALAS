// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Centre-of-gravity and neutral-point markers.
//!
//! CG per load case from the bottom-up mass statement (database masses,
//! weighed overrides, structure ESTIMATES flagged by `MassBasis`); neutral
//! point from the closed-form lifting-surface estimate (power-off, no
//! fuselage term unless `options.fuselage` carries one; replace by the VLM
//! value through `NeutralPointMethod` when available). Static margin is
//! `(x_np - x_cg) / c_wing_MAC`, positive when stable. Body axes, m.

use alas_uav_db::Database;

use crate::airframe::geometry::{AirframeGeometry, TailShape};
use crate::airframe::inertia::Vec3;
use crate::airframe::mass::{LoadCase, MassStatement};
use crate::airframe::stability::{
    LongitudinalOptions, NeutralPoint, NeutralPointMethod, SimpleLiftingSurface, StabilityPoint,
};
use crate::airframe::structure::FuselageShell;
use crate::airframe::{build_mass_statement, MassInputs};
use crate::spec::UavSpec;

use super::StudioError;

/// What a marker marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Centre of gravity of a load case.
    Cg,
    /// Neutral point (stick fixed, power off).
    NeutralPoint,
    /// Wing aerodynamic centre.
    WingAc,
    /// Tail aerodynamic centre.
    TailAc,
}

/// A labelled point.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// What it marks.
    pub kind: MarkerKind,
    /// Text label.
    pub label: String,
    /// Position, m.
    pub position_m: Vec3,
}

/// Markers and the numbers behind them.
#[derive(Debug, Clone)]
pub struct Markers {
    /// The markers (CG per case first).
    pub markers: Vec<Marker>,
    /// Neutral point result.
    pub neutral_point: NeutralPoint,
    /// Static margin per load case.
    pub stability: Vec<StabilityPoint>,
    /// Mass statement of the first load case.
    pub mass: MassStatement,
}

/// Mass inputs with the fuselage shell taken from the layout: a thin shell of
/// the pod's perimeter-equivalent diameter `2 (w + h) / pi`.
pub fn mass_inputs_for(spec: &UavSpec) -> MassInputs {
    let mut inputs = MassInputs::generic();
    if let Some(f) = &spec.master.layout.fuselage {
        let o = spec.master.fuselage_xyz_m;
        inputs.structure.fuselage_shell = Some(FuselageShell {
            x_start_m: o[0],
            length_m: f.length_m,
            diameter_m: 2.0 * (f.width_m + f.height_m) / std::f64::consts::PI,
            centre_yz_m: [o[1], o[2]],
        });
    }
    inputs
}

/// Compute the markers for `cases` (at least one).
///
/// # Errors
///
/// Mass-statement or geometry failures.
pub fn markers(
    spec: &UavSpec,
    db: &Database,
    inputs: &MassInputs,
    cases: &[LoadCase],
) -> Result<Markers, StudioError> {
    if cases.is_empty() {
        return Err(StudioError::Invalid("no load case".into()));
    }
    let geometry =
        AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default())?;
    let np = SimpleLiftingSurface.neutral_point(&geometry, &LongitudinalOptions::default())?;
    let mut out = Vec::new();
    let mut triples = Vec::new();
    let mut first = None;
    for case in cases {
        let ms = build_mass_statement(spec, db, inputs, case)?;
        let cg = ms.cg_m()?;
        triples.push((case.name.clone(), ms.total_mass_kg(), cg[0]));
        out.push(Marker {
            kind: MarkerKind::Cg,
            label: format!("CG {}", case.name),
            position_m: cg,
        });
        first.get_or_insert(ms);
    }
    let z = out[0].position_m[2];
    out.push(Marker {
        kind: MarkerKind::NeutralPoint,
        label: "NP".into(),
        position_m: [np.x_np_m, 0.0, z],
    });
    out.push(Marker {
        kind: MarkerKind::WingAc,
        label: "AC wing".into(),
        position_m: [np.x_ac_wing_m, 0.0, z],
    });
    if geometry.horizontal.is_some() {
        out.push(Marker {
            kind: MarkerKind::TailAc,
            label: "AC tail".into(),
            position_m: geometry
                .horizontal
                .as_ref()
                .map_or([np.x_ac_tail_m, 0.0, z], |h| {
                    h.planform.aerodynamic_centre_m()
                }),
        });
    }
    let stability = StabilityPoint::evaluate(&np, &triples);
    let mass = first.ok_or_else(|| StudioError::Invalid("no mass statement".into()))?;
    Ok(Markers {
        markers: out,
        neutral_point: np,
        stability,
        mass,
    })
}

/// Move the wing along `x` until the smallest static margin over `cases`
/// equals `target` (fraction of the wing MAC), re-deriving the publications
/// each step so every part mounted on the wing moves with it. This is a design
/// action (a designer sliding the wing), not a calibration to a measured CG.
/// Returns the achieved smallest margin.
///
/// # Errors
///
/// Model failures; [`StudioError::Invalid`] when the iteration does not
/// converge within 40 steps (for example a tail that cannot give stability).
pub fn fit_wing_x_for_margin(
    spec: &mut UavSpec,
    db: &Database,
    cases: &[LoadCase],
    target: f64,
) -> Result<f64, StudioError> {
    // d(SM)/dx is close to (1 - share of the mass that moves with the wing)
    // over the MAC; the update below is a fixed-gain iteration on that slope.
    for _ in 0..40 {
        super::publish::refresh(spec)?;
        let m = markers(spec, db, &mass_inputs_for(spec), cases)?;
        let sm = m
            .stability
            .iter()
            .map(|s| s.static_margin)
            .fold(f64::INFINITY, f64::min);
        let err = sm - target;
        if err.abs() < 1e-5 {
            return Ok(sm);
        }
        spec.master.wing_xyz_m[0] -= err * m.neutral_point.mac_m / 0.8;
    }
    Err(StudioError::Invalid(
        "wing position did not converge to the requested static margin".into(),
    ))
}
