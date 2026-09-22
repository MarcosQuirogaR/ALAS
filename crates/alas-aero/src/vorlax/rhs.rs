// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// compute_RHS_matrix.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The boundary condition: what the flow does at each control point before
//! any vortex is switched on.
//!
//! The circulation solve asks for the strengths that cancel the onset flow's
//! component through every panel, so this module produces that component:
//! the freestream resolved by angle of attack and sideslip, plus the local
//! velocity a rigid-body rotation adds at a control point away from the
//! rotation centre.
//!
//! # Two boundary conditions, and which one this is
//!
//! `build_RHS` computes both and picks between them on
//! `settings.use_VORLAX_matrix_calculation`. The default, and the only one
//! reached here, is mission analysis model's: the unit velocity vector dotted with the panel's
//! own unit normal. VORLAX's own: `ALOC`, built from direction cosines that
//! use the *strip leading edge's* camber, twist and dihedral for every panel
//! in the strip: exists so a developer can compare against VORLAX one
//! number at a time, and is not translated.
//!
//! Four of its byproducts *are*, because the leading-edge suction term in
//! [`super::forces`] reads them: `SCNTL`, `CCNTL`, `COD` and `SID` are the
//! direction cosines, and `EFFINC` is assembled from them there. They are
//! computed here rather than there because upstream computes them here, and
//! `EFFINC` is defined as differing from `ALOC` in exactly one term.
//!
//! # Scope
//!
//! The propeller-wake branch is absent. `compute_RHS_matrix` loops over the
//! vehicle's networks and, if `propeller_wake_model` is set, adds each
//! propeller's and lift rotor's slipstream to the onset flow.
//! `Fidelity_Zero.__defaults__` sets it false, the mission runner overrides
//! nothing, and the vehicle's one network is a turbofan with neither
//! `propellers` nor `lift_rotors`, so the induced-velocity totals are three
//! arrays of zeros that are added to the freestream and change nothing.

use super::types::{VlmCondition, VortexDistribution};

/// The boundary condition and the direction cosines that outlive it.
pub struct RhsTerms {
    /// The onset flow through each panel, normalized by the freestream
    /// speed. This is what the circulation solve is set equal to.
    pub rhs: Vec<f64>,
    /// VORLAX's `ONSET`: the axial velocity the rigid-body rotation adds at
    /// each control point, which the pressure coefficient adds to the
    /// freestream's own axial component.
    pub onset: Vec<f64>,
    /// Control-point offset from the rotation centre, lateral.
    pub ygiro: Vec<f64>,
    /// Control-point offset from the rotation centre, vertical.
    pub zgiro: Vec<f64>,
    /// The axial onset velocity component, reused by the suction term.
    pub vx: Vec<f64>,
    /// Sine of the panel's camber-slope angle.
    pub scntl: Vec<f32>,
    /// Cosine of the panel's camber-slope angle.
    pub ccntl: Vec<f32>,
    /// Cosine of the strip's dihedral angle, written on every panel.
    pub cod: Vec<f64>,
    /// Sine of the strip's dihedral angle, written on every panel.
    pub sid: Vec<f64>,
}

/// The two flow-tangency angles every stage reads.
///
/// `phi` is the panel's dihedral, taken between its two control-point
/// corners; `delta` is the mean camber surface's angle, taken between the
/// bound vortex and the control point.
///
/// Both are computed once for the whole vehicle and are the same for every
/// flight condition, which is why they are their own type rather than part
/// of [`RhsTerms`]. Their precision is upstream's and is not uniform: `phi`
/// is an `arctan` of single-precision coordinates evaluated in single
/// precision and then widened, because the multiplication by a row of ones
/// that widens it happens *after* the `arctan`; `delta`'s denominator is
/// widened *before* the division, so its `arctan` runs in double. Reproducing
/// that asymmetry is not pedantry: `phi` feeds the sine and cosine of the
/// dihedral, which multiply every side force in the integration.
pub struct TangencyAngles {
    /// Panel dihedral angle.
    pub phi: Vec<f64>,
    /// Mean camber surface angle.
    pub delta: Vec<f64>,
}

impl TangencyAngles {
    /// Compute both from the panelization.
    pub fn compute(vd: &VortexDistribution) -> Self {
        let p = &vd.panels;
        let phi = (0..vd.n_cp)
            .map(|i| f64::from(((p.zbc[i] - p.zac[i]) / (p.ybc[i] - p.yac[i])).atan()))
            .collect();
        let delta = (0..vd.n_cp)
            .map(|i| (f64::from(p.zc[i] - p.zch[i]) / f64::from(p.xc[i] - p.xch[i])).atan())
            .collect();
        Self { phi, delta }
    }
}

/// Build the boundary condition for one flight condition.
///
/// `moment_reference` is VORLAX's `(XBAR, ZBAR)`, the point the rigid-body
/// rotation is taken about. Upstream writes it onto the distribution as two
/// arrays of one repeated value; it is a pair here, because that is what it
/// is.
pub fn build(
    vd: &VortexDistribution,
    angles: &TangencyAngles,
    condition: &VlmCondition,
    moment_reference: [f64; 2],
) -> RhsTerms {
    let n = vd.n_cp;
    let p = &vd.panels;
    let (x_bar, z_bar) = (moment_reference[0], moment_reference[1]);

    let alfa = condition.angle_of_attack_rad;
    let psi = condition.side_slip_angle_rad;
    // VORLAX's `COSCOS`. Its `SINALF` and `COSIN` siblings are computed here
    // upstream too, and feed only `ALOC` (the untranslated boundary
    // condition) so they are absent.
    let coscos = alfa.cos() * psi.cos();

    // The rates made dimensionless by the freestream speed. Upstream forms
    // the roll one here too, and reads it only in `ALOC`.
    let velocity = condition.velocity_m_s;
    let pitch = condition.pitch_rate_rad_s / velocity;
    let yaw = condition.yaw_rate_rad_s / velocity;

    // The control point relative to the rotation centre. The half-panel
    // offset is VORLAX's: the onset flow is evaluated at the strip midpoint
    // of the panel's *leading* half, not at the three-quarter-chord control
    // point the tangency condition is imposed at.
    let mut xgiro = vec![0.0; n];
    let mut ygiro = vec![0.0; n];
    let mut zgiro = vec![0.0; n];
    for i in 0..n {
        let deltax = 0.5 / vd.panels_per_strip[i] as f64;
        xgiro[i] = f64::from(p.xch[i]) + f64::from(vd.chord_lengths_m[i]) * deltax - x_bar;
        ygiro[i] = f64::from(p.ych[i]);
        zgiro[i] = f64::from(p.zch[i]) - z_bar;
    }

    let vx: Vec<f64> = (0..n)
        .map(|i| coscos - pitch * zgiro[i] + yaw * ygiro[i])
        .collect();
    let onset: Vec<f64> = (0..n).map(|i| -pitch * zgiro[i] + yaw * ygiro[i]).collect();

    let scntl: Vec<f32> = vd.slope.iter().map(|&s| s / (1.0 + s * s).sqrt()).collect();
    let ccntl: Vec<f32> = scntl.iter().map(|&s| 1.0 / (1.0 + s * s).sqrt()).collect();

    // The dihedral angle is read at the strip's leading-edge panel and
    // written onto every panel of the strip: VORLAX treats a strip as one
    // rigid chordwise line, so a cambered strip's panels do not each get
    // their own dihedral.
    let mut cod = vec![0.0; n];
    let mut sid = vec![0.0; n];
    for &le in &vd.leading_edge_panels() {
        let strip_phi = angles.phi[le];
        for k in 0..vd.panels_per_strip[le] {
            cod[le + k] = strip_phi.cos();
            sid[le + k] = strip_phi.sin();
        }
    }

    // The onset flow in aircraft axes, made dimensionless by the freestream
    // speed and dotted with the panel normal.
    let rhs = (0..n)
        .map(|i| {
            let vx_total = velocity * alfa.cos() * psi.cos()
                - condition.pitch_rate_rad_s * zgiro[i]
                + condition.yaw_rate_rad_s * ygiro[i];
            let vy_total = velocity * alfa.cos() * psi.sin() - condition.yaw_rate_rad_s * xgiro[i]
                + condition.roll_rate_rad_s * zgiro[i];
            let vz_total = velocity * alfa.sin() - condition.roll_rate_rad_s * ygiro[i]
                + condition.pitch_rate_rad_s * xgiro[i];
            let normal = vd.normals[i];
            (vx_total / velocity) * f64::from(normal[0])
                + (vy_total / velocity) * f64::from(normal[1])
                + (vz_total / velocity) * f64::from(normal[2])
        })
        .collect();

    RhsTerms {
        rhs,
        onset,
        ygiro,
        zgiro,
        vx,
        scntl,
        ccntl,
        cod,
        sid,
    }
}
