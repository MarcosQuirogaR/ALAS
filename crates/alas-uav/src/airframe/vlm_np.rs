// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Neutral point from the production vortex lattice (UAV-PLAN M4, item 1).
//!
//! The airframe planform (main wing plus a horizontal tail or V pair) is
//! mapped onto `alas_geom` wings at the panel boundaries (chord is linear
//! inside a panel, so the boundary sections are exact for the lattice), and
//! the inviscid lattice of `alas_aero` gives lift and pitching moment at two
//! angles of attack.  The neutral point is where the lattice pitching moment
//! slope vanishes:
//!
//! `x_np = x_ref - (dM/dalpha) / (dL/dalpha)`, with `M` nose-up positive about
//! `x_ref` (the wing aerodynamic centre) and `L` the wind-axis lift.
//!
//! A fuselage pitching term (`FuselageContribution`) is added as a shift
//! `-c_ref Cm_alpha,fus / CLalpha` (same algebra as the closed form).
//!
//! Limits: inviscid, incompressible, attached flow; the fins (vertical
//! surfaces) do not enter the pitch derivative and are ignored here; the
//! lattice has no separate downwash gradient, so `downwash_gradient` is
//! reported as `0.0` (not applicable) for this method.  Finite-difference step
//! `+/- 0.5 deg`; the lattice is linear, so the step only removes round-off.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::{naca_coordinates, Airfoil};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::geometry::{AirframeGeometry, Planform, TailRole, TailSurfaceGeom};
use super::stability::{LongitudinalOptions, NeutralPoint, NeutralPointMethod};
use super::AirframeError;

/// Method identifier recorded in results.
pub const METHOD: &str = "vlm-inviscid-lattice";

/// Finite-difference half-step in degrees.
const HALF_STEP_DEG: f64 = 0.5;

/// Airspeed used only to scale dimensional forces (ratios are independent).
const REFERENCE_SPEED_M_S: f64 = 20.0;

/// Lattice neutral-point estimator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VlmNeutralPoint {
    /// Spanwise panels per semispan lattice section (see `alas_aero`).
    pub spanwise: usize,
    /// Chordwise panels.
    pub chordwise: usize,
}

impl Default for VlmNeutralPoint {
    fn default() -> Self {
        Self {
            spanwise: 24,
            chordwise: 4,
        }
    }
}

/// Wing-section stations of a planform, one `WingXSec` per panel boundary.
fn planform_sections(p: &Planform, airfoil: &Airfoil) -> Result<Vec<WingXSec>, AirframeError> {
    if p.vertical {
        return Err(AirframeError::Invalid(
            "vertical surfaces are not part of the pitch lattice".into(),
        ));
    }
    if p.panels.is_empty() {
        return Err(AirframeError::Invalid("planform has no panels".into()));
    }
    let mut out = Vec::with_capacity(p.panels.len() + 1);
    for panel in &p.panels {
        let [x, y, z] = panel.root_le_m;
        out.push(WingXSec::new(
            [x, y, z],
            panel.chord_root_m,
            0.0,
            airfoil.clone(),
        ));
    }
    if let Some(last) = p.panels.last() {
        let [x, y, z] = last.root_le_m;
        let s = last.span_m;
        let (sin_g, cos_g) = last.dihedral_rad.sin_cos();
        out.push(WingXSec::new(
            [
                x + s * last.sweep_le_rad.tan(),
                y + s * cos_g,
                z + s * sin_g,
            ],
            last.chord_tip_m,
            0.0,
            airfoil.clone(),
        ));
    }
    Ok(out)
}

fn lattice_error(e: impl std::fmt::Display) -> AirframeError {
    AirframeError::Invalid(format!("vortex lattice: {e}"))
}

impl VlmNeutralPoint {
    /// The airplane for the wing alone, or wing plus horizontal tail.
    fn airplane(
        g: &AirframeGeometry,
        with_tail: bool,
        airfoil: &Airfoil,
        x_ref: f64,
    ) -> Result<Airplane, AirframeError> {
        let mut wings = vec![Wing::new(
            "wing",
            planform_sections(&g.wing, airfoil)?,
            g.wing.mirror,
        )];
        if with_tail {
            if let Some(h) = g
                .horizontal
                .as_ref()
                .filter(|h| h.role != TailRole::Vertical)
            {
                wings.push(tail_wing(h, airfoil)?);
            }
        }
        Ok(Airplane {
            name: "airframe-np".into(),
            xyz_ref: [x_ref, 0.0, 0.0],
            s_ref: g.wing.projected_area_m2(),
            c_ref: g.wing.mac_m(),
            b_ref: g.wing.span_m(),
            wings,
            fuselages: Vec::new(),
        })
    }

    /// `(dL/dalpha, dM/dalpha)` per radian for `airplane` (dimensional).
    fn slopes(airplane: &Airplane) -> Result<(f64, f64), AirframeError> {
        let sys = VlmSystem::assemble(airplane, 24, 4).map_err(lattice_error)?;
        let atm = Atmosphere::new(0.0);
        let run = |alpha_deg: f64| {
            let op = OperatingPoint::new(atm, REFERENCE_SPEED_M_S, alpha_deg, 0.0, 0.0, 0.0, 0.0);
            sys.solve(&op).map_err(lattice_error)
        };
        let plus = run(HALF_STEP_DEG)?;
        let minus = run(-HALF_STEP_DEG)?;
        let d_rad = 2.0 * HALF_STEP_DEG.to_radians();
        Ok((
            (plus.lift - minus.lift) / d_rad,
            (plus.pitch_moment - minus.pitch_moment) / d_rad,
        ))
    }
}

/// Tail as a lattice wing: horizontal planform or V pair, incidence as twist.
fn tail_wing(h: &TailSurfaceGeom, airfoil: &Airfoil) -> Result<Wing, AirframeError> {
    let mut xsecs = planform_sections(&h.planform, airfoil)?;
    for x in &mut xsecs {
        x.twist = h.incidence_rad.to_degrees();
    }
    Ok(Wing::new("tail", xsecs, h.planform.mirror))
}

impl NeutralPointMethod for VlmNeutralPoint {
    fn name(&self) -> &'static str {
        METHOD
    }

    fn neutral_point(
        &self,
        g: &AirframeGeometry,
        o: &LongitudinalOptions,
    ) -> Result<NeutralPoint, AirframeError> {
        let airfoil = naca_airfoil()?;
        let wing = &g.wing;
        let s = wing.projected_area_m2();
        let c = wing.mac_m();
        let x_acw = wing.aerodynamic_centre_m()[0];
        let x_ref = x_acw;

        let full = Self::airplane(g, true, &airfoil, x_ref)?;
        let (dl, dm) = Self::slopes(&full)?;
        let wing_only = Self::airplane(g, false, &airfoil, x_ref)?;
        let (dl_w, _) = Self::slopes(&wing_only)?;

        let q_s = 0.5 * Atmosphere::new(0.0).density() * REFERENCE_SPEED_M_S.powi(2) * s;
        let cl_total = dl / q_s;
        let cl_wing = dl_w / q_s;
        let cl_tail_total = cl_total - cl_wing;
        let x_np_vlm = x_ref - dm / dl;
        let x_np = x_np_vlm - c * o.fuselage.cm_alpha_per_rad / cl_total;
        if !(x_np.is_finite() && cl_total > 0.0) {
            return Err(AirframeError::Invalid(
                "lattice neutral point is not finite or the wing is not lifting".into(),
            ));
        }
        let (x_ac_tail, cl_alpha_tail) = match g.horizontal.as_ref() {
            Some(h) if h.role != TailRole::Vertical => {
                let s_eff = h.effective_horizontal_area_m2();
                let slope = if s_eff > 0.0 {
                    cl_tail_total / (o.tail_dynamic_pressure_ratio * s_eff / s)
                } else {
                    0.0
                };
                (h.planform.aerodynamic_centre_m()[0], slope)
            }
            _ => (x_acw, 0.0),
        };
        Ok(NeutralPoint {
            x_np_m: x_np,
            mac_m: c,
            s_ref_m2: s,
            cl_alpha_wing: cl_wing,
            cl_alpha_tail,
            tail_term: cl_tail_total,
            downwash_gradient: 0.0,
            x_ac_wing_m: x_acw,
            x_ac_tail_m: x_ac_tail,
            method: METHOD,
        })
    }
}

/// A symmetric NACA 0012 for the lattice (thickness does not enter the
/// inviscid lattice; camber is zero, so the zero-lift moment is zero).
fn naca_airfoil() -> Result<Airfoil, AirframeError> {
    let coords = naca_coordinates("NACA0012", 60)
        .ok_or_else(|| AirframeError::Invalid("NACA 0012 coordinates unavailable".into()))?;
    Ok(Airfoil::from_coordinates("NACA0012", coords))
}
