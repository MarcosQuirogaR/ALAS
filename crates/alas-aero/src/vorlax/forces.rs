// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/VLM.py,
// steps 11 through 13 (VORLAX subroutines PRESS and AERO).
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Turning solved circulations into forces: VORLAX's `PRESS` and `AERO`.
//!
//! Two stages. [`pressure`] gives every panel a load coefficient: the
//! circulation's own contribution, plus the lateral term a sideslipping flow
//! adds by running along a swept strip rather than across it. [`integrate`]
//! reduces those loads strip by strip into a body-axis force and moment, adds
//! the leading-edge suction each strip's leading panel carries, and divides
//! by the reference area.
//!
//! # Where the single precision is
//!
//! The panelization is stored in `f32` and several of the quantities here are
//! derived from it *within* single precision before being used in a double
//! expression: `1/CHORD`, the leading and trailing edge sweep tangents, the
//! camber slope less the strip incidence and its `1 + t^2` denominator, the
//! sine and cosine of the strip incidence, and the strip's own half-span and
//! area. Each is marked at the point it happens. They are not stylistic: a
//! strip area computed in double and one computed in single differ in the
//! seventh digit, and the strip area multiplies every force this module
//! reports.
//!
//! # Two things that look like bugs and are not
//!
//! `GAF` is `0.5 + 0.5 * RJTS**2` with `RJTS` fixed at zero, so it is always
//! a half. `RJTS` is VORLAX's strip-exposure flag, and mission analysis model's own comment
//! says why it is nailed down: the sections here have no thickness, so no
//! strip is ever partly shielded. It is written as the derivation rather than
//! as `0.5`, so a thickness model can find it.
//!
//! And the leading-edge suction is computed for linear chordwise spacing,
//! which VORLAX itself skips: VORLAX evaluates `CLE` only for cosine
//! chordwise spacing. mission analysis model forces the calculation anyway, on the recorded
//! grounds that the trend is right even though the magnitude is understated.
//! That is a deliberate deviation of mission analysis model's from VORLAX, and this port
//! reproduces mission analysis model.

use super::induced::InducedVelocity;
use super::rhs::RhsTerms;
use super::types::{VlmCaseResult, VlmCondition, VlmGeometry, VortexDistribution};

/// VORLAX's `RJTS`, the spanwise strip-exposure flag. Always zero here: it
/// marks a strip partly shielded by a thick section, and every section in
/// this model is a sheet.
const STRIP_EXPOSURE: f64 = 0.0;

/// NumPy's `sign`, which answers zero at zero where Rust's `signum` answers
/// one. Only the unreachable Lan leading-edge correction reads it, and the
/// difference would be invisible until something reached that branch.
fn numpy_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Everything about the vehicle and the condition the integration needs.
pub struct LoadCase<'a> {
    /// The panelization.
    pub vd: &'a VortexDistribution,
    /// The vehicle's reference dimensions.
    pub geometry: &'a VlmGeometry,
    /// The flight condition.
    pub condition: &'a VlmCondition,
    /// Panel dihedral angles, from [`super::rhs::TangencyAngles`].
    pub phi: &'a [f64],
    /// The solved circulation per panel.
    pub gamma: &'a [f64],
    /// The influence matrix, for the leading-edge suction term.
    pub induced: &'a InducedVelocity,
    /// Half the bound vortex's projected length, per panel.
    pub semispan: &'a [f32],
    /// The boundary condition and the direction cosines it produced.
    pub rhs: &'a RhsTerms,
    /// VORLAX's `K_SPC`.
    pub leading_edge_suction_multiplier: f64,
}

/// Integrate one solved case into coefficients.
pub fn integrate(case: &LoadCase<'_>) -> VlmCaseResult {
    let vd = case.vd;
    let leading = vd.leading_edge_panels();
    let n_strips = leading.len();

    let alfa = case.condition.angle_of_attack_rad;
    let psi = case.condition.side_slip_angle_rad;
    let (sinalf, cosalf) = alfa.sin_cos();
    let (sinpsi, copsi) = psi.sin_cos();
    // VORLAX's `COSIN` carries a factor of two that `COSINP` does not; the
    // pressure stage's lateral term uses the first and the suction term the
    // second.
    let cosin = cosalf * sinpsi * 2.0;
    let cosinp = cosalf * sinpsi;
    let coscos = cosalf * copsi;

    let pitch = case.condition.pitch_rate_rad_s / case.condition.velocity_m_s;
    let roll = case.condition.roll_rate_rad_s / case.condition.velocity_m_s;
    let yaw = case.condition.yaw_rate_rad_s / case.condition.velocity_m_s;

    let b2 = case.condition.mach * case.condition.mach - 1.0;
    let dcp = pressure(vd, case.gamma, &case.rhs.onset, coscos, cosin);

    let strips = reduce_onto_strips(vd, &leading, &dcp);

    // ---- Leading-edge geometry ------------------------------------------
    let tan_le: Vec<f32> = leading.iter().map(|&i| sweep_tangent_le(vd, i)).collect();
    let t2: Vec<f32> = tan_le.iter().map(|&t| t * t).collect();
    // `STB` is the cotangent of the Mach angle relative to the leading edge.
    // It vanishes on an edge swept behind the Mach cone, which is
    // unreachable subsonically: `B2` is negative there and `T2` is not.
    let stb: Vec<f64> = t2
        .iter()
        .map(|&t| {
            if b2 < f64::from(t) {
                (f64::from(t) - b2).sqrt()
            } else {
                0.0
            }
        })
        .collect();

    let cod: Vec<f64> = leading.iter().map(|&i| case.phi[i].cos()).collect();
    let sid: Vec<f64> = leading.iter().map(|&i| case.phi[i].sin()).collect();
    let dcp_le: Vec<f64> = leading.iter().map(|&i| dcp[i]).collect();

    let mut cle = rotation_effects(
        case,
        &leading,
        &strips.xle,
        &stb,
        Rotation {
            cosinp,
            sinalf,
            pitch,
            roll,
            yaw,
        },
    );
    for (s, value) in cle.iter_mut().enumerate() {
        *value += 0.5 * dcp_le[s] * strips.xle[leading[s]].sqrt();
    }

    // The suction multiplier leaves its default only where a surface has
    // vortex lift enabled below Mach 1, and the exposure flag drops to zero
    // only behind a discretized non-slat control surface. Neither happens on
    // a vehicle the mission runner builds; both are read rather than assumed.
    let surface_of_strip = surfaces_of_strips(vd, n_strips);
    let spc: Vec<f64> = (0..n_strips)
        .map(|s| {
            let base = if vd.vortex_lift[surface_of_strip[s]] && case.condition.mach < 1.0 {
                -1.0
            } else {
                case.leading_edge_suction_multiplier
            };
            base * f64::from(vd.exposed_leading_edge_flag[s])
        })
        .collect();

    let csuc: Vec<f64> = (0..n_strips)
        .map(|s| 0.5 * std::f64::consts::PI * spc[s].abs() * (cle[s] * cle[s]) * stb[s])
        .collect();

    // ---- Strip forces and moments in body axes ---------------------------
    let mut lift = vec![0.0; n_strips];
    let mut drag = vec![0.0; n_strips];
    let mut moment = vec![0.0; n_strips];
    let mut side = vec![0.0; n_strips];
    let mut rolling = vec![0.0; n_strips];
    let mut yawing = vec![0.0; n_strips];
    let mut cl_y = vec![0.0; n_strips];
    let mut cdi_y = vec![0.0; n_strips];

    let (x_bar, z_bar) = (
        case.geometry.moment_reference_m[0],
        case.geometry.moment_reference_m[1],
    );
    let gaf = 0.5 + 0.5 * STRIP_EXPOSURE * STRIP_EXPOSURE;

    for s in 0..n_strips {
        let i = leading[s];
        let zeta = vd.tangent_incidence_angle[i];
        // Single precision: the strip incidence is stored as `f32` and its
        // sine, cosine and the leading-edge force's own angle are all taken
        // there before the body-axis sums widen them.
        let angle = vd.sle[s] - zeta;
        let (xcos, xsin) = (angle.cos(), angle.sin());
        let (tfx, tfz) = if spc[s] < 0.0 {
            let sign = numpy_sign(dcp_le[s]);
            (f64::from(xsin) * sign, f64::from(xcos.abs()) * sign)
        } else {
            (f64::from(xcos), -f64::from(xsin))
        };
        let caxl = strips.caxl[s] - tfx * csuc[s];
        let cnc = strips.cnc[s] + csuc[s] * f64::from((1.0 + t2[s]).sqrt()) * tfz;

        let fcos = f64::from(zeta.cos());
        let fsin = f64::from(zeta.sin());
        let bfx = -cnc * fsin + caxl * fcos;
        let bfy = -(cnc * fcos + caxl * fsin) * sid[s];
        let bfz = (cnc * fcos + caxl * fsin) * cod[s];

        let chord_strip = f64::from(vd.chord_lengths_m[i]);
        let bmle = strips.bmle[s] * chord_strip;
        let sicple = -strips.sicple[s] * cosin * cod[s] * gaf;

        let x = f64::from(vd.panels.xch[i]);
        let y = f64::from(vd.panels.ych[i]);
        let z = f64::from(vd.panels.zch[i]);

        let bmx = (bfz * y - bfy * (z - z_bar)) + sicple;
        let bmy = bmle * cod[s] + bfx * (z - z_bar) - bfz * (x - x_bar);
        let bmz = bmle * sid[s] - bfx * y + bfy * (x - x_bar);
        let cdc = (bfz * sinalf + (bfx * copsi + bfy * sinpsi) * cosalf) * chord_strip;

        // Single precision: the strip's span and area, as upstream forms
        // them from the `f32` bound-vortex half-span and the `f32` chord.
        let es = 2.0f32 * case.semispan[i];
        let strip_area = f64::from(es * vd.chord_lengths_m[i]);

        lift[s] = (bfz * cosalf - (bfx * copsi + bfy * sinpsi) * sinalf) * strip_area;
        drag[s] = cdc * f64::from(es);
        moment[s] = strip_area * (bmy * copsi - bmx * sinpsi);
        side[s] = (bfy * copsi - bfx * sinpsi) * strip_area;
        rolling[s] = strip_area * (bmx * cosalf * copsi + bmy * cosalf * sinpsi + bmz * sinalf);
        yawing[s] = strip_area * (bmz * cosalf - (bmx * copsi + bmy * sinpsi) * sinalf);

        cl_y[s] = lift[s] / chord_strip / f64::from(es);
        cdi_y[s] = drag[s] / chord_strip / f64::from(es);
    }

    // ---- Reduce onto surfaces and the whole vehicle ----------------------
    let mut cl_wing = Vec::with_capacity(vd.n_w);
    let mut cdi_wing = Vec::with_capacity(vd.n_w);
    for w in 0..vd.n_w {
        let first = vd.spanwise_breaks[w];
        let last = vd.spanwise_breaks.get(w + 1).copied().unwrap_or(n_strips);
        let area = f64::from(vd.wing_areas_m2[w]);
        cl_wing.push(lift[first..last].iter().sum::<f64>() / area);
        cdi_wing.push(drag[first..last].iter().sum::<f64>() / area);
    }

    let sref = case.geometry.reference_area_m2;
    let crtot = rolling.iter().sum::<f64>() / sref;
    let cntot = yawing.iter().sum::<f64>() / sref;
    let span = case.geometry.reference_span_m;

    VlmCaseResult {
        cl: lift.iter().sum::<f64>() / sref,
        cdi: drag.iter().sum::<f64>() / sref,
        cm: moment.iter().sum::<f64>() / sref / case.geometry.mean_aerodynamic_chord_m,
        cytot: side.iter().sum::<f64>() / sref,
        crtot,
        // The span-scaled pair carry upstream's sign flip: VORLAX reports a
        // rolling and yawing moment whose positive sense is the opposite of
        // the body-axis one the strip sum produced.
        crmtot: -(crtot / span),
        cntot,
        cymtot: -(cntot / span),
        cl_wing,
        cdi_wing,
        cl_y,
        cdi_y,
        // Reported in single precision, as upstream reports them: the two
        // field quantities go through `np.array(x, dtype=precision)` on the
        // way out while the coefficients do not.
        cp: dcp.iter().map(|&v| v as f32).collect(),
        gamma: case.gamma.iter().map(|&v| v as f32).collect(),
    }
}

/// The per-strip sums the force stage reads.
struct StripLoads {
    /// The normal-force coefficient, summed over the strip's panels.
    cnc: Vec<f64>,
    /// The sideslip couple about the strip centreline.
    sicple: Vec<f64>,
    /// The axial force from the panels' camber slopes.
    caxl: Vec<f64>,
    /// The pitching moment about the strip's leading edge.
    bmle: Vec<f64>,
    /// The first vortex midpoint as a fraction of chord, per *panel*.
    xle: Vec<f64>,
}

fn reduce_onto_strips(vd: &VortexDistribution, leading: &[usize], dcp: &[f64]) -> StripLoads {
    let n = vd.n_cp;
    let xle: Vec<f64> = (0..n)
        .map(|i| 0.125 * (2.0 / vd.panels_per_strip[i] as f64))
        .collect();

    let mut loads = StripLoads {
        cnc: vec![0.0; leading.len()],
        sicple: vec![0.0; leading.len()],
        caxl: vec![0.0; leading.len()],
        bmle: vec![0.0; leading.len()],
        xle,
    };

    for (strip, &start) in leading.iter().enumerate() {
        for k in 0..vd.panels_per_strip[start] {
            let i = start + k;
            let pion = 2.0 / vd.panels_per_strip[i] as f64;
            // The load contribution of this panel to the strip's nominal
            // area. The horseshoe spans cancel: every panel in a strip has
            // the same one.
            let sinf = 0.5 * pion * dcp[i];

            // The moment arm of the sideslip couple: the strip centreline
            // between the load point and the trailing edge, in `f32` as the
            // coordinates it is a difference of.
            let cormed = (vd.panels.xa_te[i] + vd.panels.xb_te[i]) / 2.0 - vd.panels.xch[i];

            // Single precision: the mean-surface slope less the strip's own
            // incidence, and the `1 + t^2` it is divided by.
            let tx = vd.slope[i] - vd.tangent_incidence_angle[i];
            let xx = (vd.chordwise_panel_number[i] as f64 - 0.75) * pion / 2.0;

            loads.cnc[strip] += sinf;
            loads.sicple[strip] += sinf * f64::from(cormed);
            loads.caxl[strip] += -sinf * f64::from(tx) / f64::from(1.0 + tx * tx);
            loads.bmle[strip] += (loads.xle[i] - xx) * sinf;
        }
    }
    loads
}

/// Which surface each strip belongs to.
fn surfaces_of_strips(vd: &VortexDistribution, n_strips: usize) -> Vec<usize> {
    let mut out = vec![0usize; n_strips];
    for w in 0..vd.spanwise_breaks.len() {
        let first = vd.spanwise_breaks[w];
        let last = vd.spanwise_breaks.get(w + 1).copied().unwrap_or(n_strips);
        for entry in out.iter_mut().take(last).skip(first) {
            *entry = w;
        }
    }
    out
}

/// The tangent of a strip's leading-edge sweep, in single precision.
fn sweep_tangent_le(vd: &VortexDistribution, i: usize) -> f32 {
    let p = &vd.panels;
    let run = ((p.zb1[i] - p.za1[i]).powi(2) + (p.yb1[i] - p.ya1[i]).powi(2)).sqrt();
    (p.xb1[i] - p.xa1[i]) / run
}

/// The tangent of a strip's trailing-edge sweep, in single precision.
fn sweep_tangent_te(vd: &VortexDistribution, i: usize) -> f32 {
    let p = &vd.panels;
    let run = ((p.zb_te[i] - p.za_te[i]).powi(2) + (p.yb_te[i] - p.ya_te[i]).powi(2)).sqrt();
    (p.xb_te[i] - p.xa_te[i]) / run
}

/// VORLAX's `PRESS`: the load coefficient on every panel.
///
/// Two terms. The first is the circulation's own, scaled by the panel count
/// and the strip chord so that it is a pressure difference rather than a
/// strength, and multiplied by the axial onset flow: freestream plus
/// whatever the body rotation adds at that station. The second, `DCPSID`, is
/// what sideslip adds: a flow with a lateral component runs partly *along* a
/// swept strip, and the resulting spanwise load needs the circulation
/// accumulated ahead of the panel, which is a chordwise cumulative sum reset
/// at each strip's leading edge.
fn pressure(
    vd: &VortexDistribution,
    gamma: &[f64],
    onset: &[f64],
    coscos: f64,
    cosin: f64,
) -> Vec<f64> {
    let mut dcp = vec![0.0; vd.n_cp];

    for (strip, &start) in vd.leading_edge_panels().iter().enumerate() {
        let count = vd.panels_per_strip[start];
        let rnmax = count as f64;

        let tnl = f64::from(sweep_tangent_le(vd, start));
        let tnt = f64::from(sweep_tangent_te(vd, start));
        let cos_dl = f64::from((vd.panels.ybh[start] - vd.panels.yah[start]) / vd.d[strip]);

        // `GANT` is the circulation accumulated *ahead* of the panel, so it
        // is zero at the leading edge and lags the running sum by one panel.
        let mut running = 0.0;
        for k in 0..count {
            let i = start + k;
            // Single precision: the reciprocal chord, as upstream's
            // `1 / CHORD` over an `f32` array.
            let gfx = f64::from(1.0f32 / vd.chord_lengths_m[i]);
            let gant = running;
            running += gfx * gamma[i];

            let xia = (vd.chordwise_panel_number[i] as f64 - 1.0) / rnmax;
            let xib = vd.chordwise_panel_number[i] as f64 / rnmax;
            let tana = tnl * (1.0 - xia) + tnt * xia;
            let tanb = tnl * (1.0 - xib) + tnt * xib;

            let glat = gant * (tana - tanb) - gfx * gamma[i] * tanb;
            let dcpsid = cosin * cos_dl * glat / (xib - xia);

            let factor = coscos + onset[i];
            let gnet = gamma[i] * factor * rnmax / f64::from(vd.chord_lengths_m[i]);
            dcp[i] = 2.0 * gnet + dcpsid;
        }
    }
    dcp
}

/// The rate terms the suction calculation resolves the onset flow with.
struct Rotation {
    cosinp: f64,
    sinalf: f64,
    pitch: f64,
    roll: f64,
    yaw: f64,
}

/// The induced flow at each strip's leading edge, which sets the suction.
///
/// This is `compute_rotation_effects`. The influence matrix already carries
/// the circulation field's contribution, in the sending panels' dihedral
/// frame; what has to be subtracted is the onset flow's own component along
/// the camber-line normal, `EFFINC`. That differs from the boundary
/// condition `ALOC` in exactly one term (the streamwise offset from the
/// rotation centre is measured to the leading edge rather than to the
/// control point) which is why [`super::rhs`] hands over the pieces rather
/// than the result.
fn rotation_effects(
    case: &LoadCase<'_>,
    leading: &[usize],
    xle: &[f64],
    stb: &[f64],
    rotation: Rotation,
) -> Vec<f64> {
    let vd = case.vd;
    let n = vd.n_cp;
    let x_bar = case.geometry.moment_reference_m[0];

    leading
        .iter()
        .enumerate()
        .map(|(s, &i)| {
            let mut cle: f64 = 0.0;
            for k in 0..n {
                cle += f64::from(case.induced.ew[i * n + k]) * case.gamma[k];
            }

            let xgiro =
                f64::from(vd.panels.xch[i]) - f64::from(vd.chord_lengths_m[i]) * xle[i] - x_bar;
            let vy = rotation.cosinp - rotation.yaw * xgiro + rotation.roll * case.rhs.zgiro[i];
            let vz = rotation.sinalf - rotation.roll * case.rhs.ygiro[i] + rotation.pitch * xgiro;

            let effinc = case.rhs.vx[i] * f64::from(case.rhs.scntl[i])
                + vy * f64::from(case.rhs.ccntl[i]) * case.rhs.sid[i]
                - vz * f64::from(case.rhs.ccntl[i]) * case.rhs.cod[i];

            let cle = cle - effinc;
            if stb[s] > 0.0 {
                cle / vd.panels_per_strip[i] as f64 / stb[s]
            } else {
                cle
            }
        })
        .collect()
}
