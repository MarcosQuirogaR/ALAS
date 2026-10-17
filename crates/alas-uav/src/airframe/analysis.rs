// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One result struct for the optimiser and the GUI (UAV-PLAN M4, item 3):
//! neutral point and static margin at the loaded CG, tail volume coefficients,
//! cruise trim (elevator, tail lift, trim drag) and the spar (bending, shear,
//! tip deflection, mass, margins).  Every field carries its unit in the name.
//!
//! Frame and sign conventions as in [`super`]: body axes, x aft, z up, SI,
//! `SM = (x_np - x_cg)/c` (positive stable).

use super::geometry::{AirframeGeometry, TailRole};
use super::materials::{spar_design, SparMaterials};
use super::spar::{analyse_spar, SparAnalysis, SparDesign, SparGeometry};
use super::stability::{
    LongitudinalOptions, NeutralPoint, NeutralPointMethod, SimpleLiftingSurface,
};
use super::tail::volume_coefficient;
use super::trim::{trim_tailed, TrimCondition, TrimResult, TrimSetup};
use super::AirframeError;

/// Standard gravity, m/s^2.
pub const G_M_S2: f64 = 9.806_65;

/// Static-margin screening band, fraction of MAC: 5 to 15 %.  A judgement for
/// screening, not a cited requirement.  Sources checked (bibliografia PDFs):
/// Raymer 6th ed. p. 626 (PDF page): transport aircraft 5-10 % at the aft CG,
/// general aviation more stable (Cessna 172 about 19 %), fighters about 5 %.
/// Sadraey Table 6.3 (printed p. 283, PDF p. 308) lists 0.1-0.3 for `h_np - h_cg`
/// in a requirements table of unstated scope.  The two sources disagree above
/// 10 %; the upper limit 15 % is kept as the screening compromise.
pub const STATIC_MARGIN_BAND: (f64, f64) = (0.05, 0.15);

/// Oswald-type efficiency of a tail surface for the trim drag (ESTIMATE).
pub const TAIL_OSWALD_EFFICIENCY: f64 = 0.8;

/// Where a static margin sits against [`STATIC_MARGIN_BAND`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginVerdict {
    /// Below the band (marginal or unstable in practice).
    Below,
    /// Inside the band.
    Within,
    /// Above the band (sluggish pitch response).
    Above,
}

/// Cruise state for the trim solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseState {
    /// True airspeed, m/s.
    pub speed_m_s: f64,
    /// Air density, kg/m^3.
    pub density_kg_m3: f64,
    /// Aircraft mass, kg.
    pub mass_kg: f64,
}

/// Cruise trim outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct CruiseTrim {
    /// Solver output (elevator, tail lift coefficient, control power, stops).
    pub trim: TrimResult,
    /// Aircraft lift coefficient `W / (q S)` at cruise.
    pub cl_required: f64,
    /// Tail lift at cruise, N (positive up).
    pub tail_lift_n: f64,
    /// Added tail-induced drag at cruise, N.
    pub trim_drag_n: f64,
}

/// Aggregated longitudinal, tail and spar result.
#[derive(Debug, Clone, PartialEq)]
pub struct AirframeAnalysis {
    /// Neutral-point method identifier.
    pub np_method: &'static str,
    /// Neutral point x, m.
    pub neutral_point_x_m: f64,
    /// Mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Loaded CG x, m.
    pub cg_x_m: f64,
    /// Static margin at the loaded CG, fraction of MAC (positive stable).
    pub static_margin: f64,
    /// Band verdict against [`STATIC_MARGIN_BAND`].
    pub static_margin_verdict: MarginVerdict,
    /// Total lift-curve slope on the reference area, 1/rad.
    pub cl_alpha_per_rad: f64,
    /// Pitching-moment slope about the CG, 1/rad (negative is stable).
    pub cm_alpha_per_rad: f64,
    /// Horizontal tail volume `S_h l_h / (S c)` (effective area), dimensionless.
    pub tail_volume_horizontal: Option<f64>,
    /// Vertical tail volume `S_v l_v / (S b)`, dimensionless.
    pub tail_volume_vertical: Option<f64>,
    /// Cruise trim, when a horizontal tail or V pair and a trim setup exist.
    pub cruise_trim: Option<CruiseTrim>,
    /// Spar analysis, when a spar design and a limit lift are supplied.
    pub spar: Option<SparAnalysis>,
    /// True for estimates that are not calibrated (trim drag, spar).
    pub estimate: bool,
}

/// Inputs of [`analyse_airframe`] beyond the geometry.
pub struct AnalysisInputs<'a> {
    /// Stability options (fuselage term, efficiencies).
    pub options: &'a LongitudinalOptions,
    /// Loaded CG x, m (margin and trim).
    pub x_cg_m: f64,
    /// Cruise state (trim and trim drag); `None` skips the trim.
    pub cruise: Option<CruiseState>,
    /// Trim setup (Cm0, incidence, elevator effectiveness, stops).
    pub trim_setup: Option<&'a TrimSetup>,
    /// Spar design and the total limit lift it carries, N.
    pub spar: Option<(&'a SparDesign, f64)>,
}

fn margin_verdict(sm: f64) -> MarginVerdict {
    if sm < STATIC_MARGIN_BAND.0 {
        MarginVerdict::Below
    } else if sm > STATIC_MARGIN_BAND.1 {
        MarginVerdict::Above
    } else {
        MarginVerdict::Within
    }
}

/// Trim setup of a design evaluation.  ESTIMATES, not calibrated: zero
/// wing-plus-fuselage `Cm0`, tail incidence 0, elevator effectiveness 0.6
/// (Sadraey Fig. 12.12, digitised), stops at +/-25 deg.
#[must_use]
pub fn estimate_trim_setup() -> TrimSetup {
    TrimSetup {
        cm0: 0.0,
        tail_incidence_rad: 0.0,
        tau: 0.6,
        max_down_rad: 25.0_f64.to_radians(),
        max_up_rad: 25.0_f64.to_radians(),
    }
}

/// Everything the design evaluation needs beyond the mass and CG it computes.
#[derive(Debug, Clone, PartialEq)]
pub struct AirframeInputs {
    /// Lifting-surface planform and tails.
    pub geometry: AirframeGeometry,
    /// Cruise true airspeed from the mission objectives, m/s.
    pub cruise_speed_m_s: f64,
    /// Air density, kg/m^3.
    pub air_density_kg_m3: f64,
    /// Limit load factor of the design.
    pub limit_load_factor: f64,
    /// Ultimate safety factor on limit loads.
    pub safety_factor: f64,
    /// Trim setup (see [`estimate_trim_setup`]).
    pub trim_setup: TrimSetup,
    /// Spar geometry (uniform along the semispan in v1).
    pub spar_geometry: SparGeometry,
    /// Spar laminate source.
    pub spar_materials: SparMaterials,
    /// Spanwise stations of the spar beam.
    pub spar_stations: usize,
}

/// Evaluate the design-level airframe analysis at the design mass and CG.
///
/// Uses the closed-form neutral point (fast enough for the optimiser inner
/// loop); the lattice method is available through [`analyse_airframe`].
///
/// # Errors
///
/// Any failure of the neutral point, trim, or spar model.
pub fn analyse_design(
    inputs: &AirframeInputs,
    mass_kg: f64,
    x_cg_m: f64,
) -> Result<AirframeAnalysis, AirframeError> {
    let spar = spar_design(
        inputs.spar_geometry.clone(),
        &inputs.spar_materials,
        inputs.safety_factor,
        inputs.spar_stations,
    )?;
    let limit_lift_n = inputs.limit_load_factor * mass_kg * G_M_S2;
    let options = LongitudinalOptions::default();
    analyse_airframe(
        &inputs.geometry,
        &SimpleLiftingSurface,
        &AnalysisInputs {
            options: &options,
            x_cg_m,
            cruise: Some(CruiseState {
                speed_m_s: inputs.cruise_speed_m_s,
                density_kg_m3: inputs.air_density_kg_m3,
                mass_kg,
            }),
            trim_setup: Some(&inputs.trim_setup),
            spar: Some((&spar, limit_lift_n)),
        },
    )
}

/// Analyse one airframe.
///
/// # Errors
///
/// Propagates geometry, neutral-point, trim, or spar failures.  A missing
/// trim setup with a cruise state skips the trim (`cruise_trim = None`).
pub fn analyse_airframe(
    geometry: &AirframeGeometry,
    method: &dyn NeutralPointMethod,
    inputs: &AnalysisInputs<'_>,
) -> Result<AirframeAnalysis, AirframeError> {
    let np: NeutralPoint = method.neutral_point(geometry, inputs.options)?;
    let cg_x = inputs.x_cg_m;
    let sm = np.static_margin(cg_x);
    let s_ref = np.s_ref_m2;
    let x_acw = np.x_ac_wing_m;

    let tail_volume_horizontal = match geometry.horizontal.as_ref() {
        Some(h) if h.role != TailRole::Vertical => {
            let arm = np.x_ac_tail_m - x_acw;
            (arm > 0.0)
                .then(|| volume_coefficient(h.effective_horizontal_area_m2(), arm, np.mac_m, s_ref))
        }
        _ => None,
    };
    let fin = geometry
        .vertical
        .as_ref()
        .map(|v| {
            (
                v.effective_vertical_area_m2(),
                v.planform.aerodynamic_centre_m()[0],
            )
        })
        .or_else(|| {
            geometry
                .horizontal
                .as_ref()
                .filter(|h| h.role == TailRole::VPair)
                .map(|h| {
                    (
                        h.effective_vertical_area_m2(),
                        h.planform.aerodynamic_centre_m()[0],
                    )
                })
        });
    let span = geometry.wing.span_m();
    let tail_volume_vertical = fin.and_then(|(area, x_ac)| {
        let arm = x_ac - x_acw;
        (area > 0.0 && arm > 0.0).then(|| volume_coefficient(area, arm, span, s_ref))
    });

    let cruise_trim = match (
        inputs.cruise,
        inputs.trim_setup,
        geometry.horizontal.as_ref(),
    ) {
        (Some(c), Some(setup), Some(h)) => {
            let q = 0.5 * c.density_kg_m3 * c.speed_m_s * c.speed_m_s;
            let cl_required = c.mass_kg * G_M_S2 / (q * s_ref);
            let trim = trim_tailed(
                &np,
                geometry,
                inputs.options,
                setup,
                &TrimCondition {
                    name: "cruise".into(),
                    cl: cl_required,
                    x_cg_m: inputs.x_cg_m,
                },
            )?;
            let tail_lift_n = q * h.effective_horizontal_area_m2() * trim.cl_tail;
            let b_tail = h.planform.span_m();
            let trim_drag_n = if b_tail > 0.0 {
                tail_lift_n * tail_lift_n
                    / (q * std::f64::consts::PI * TAIL_OSWALD_EFFICIENCY * b_tail * b_tail)
            } else {
                0.0
            };
            Some(CruiseTrim {
                trim,
                cl_required,
                tail_lift_n,
                trim_drag_n,
            })
        }
        _ => None,
    };

    let spar = match inputs.spar {
        Some((design, limit_lift_n)) => Some(analyse_spar(design, limit_lift_n)?),
        None => None,
    };
    let estimate = cruise_trim.is_some() || spar.is_some();
    Ok(AirframeAnalysis {
        np_method: np.method,
        neutral_point_x_m: np.x_np_m,
        mac_m: np.mac_m,
        cg_x_m: cg_x,
        static_margin: sm,
        static_margin_verdict: margin_verdict(sm),
        cl_alpha_per_rad: np.cl_alpha_total(),
        cm_alpha_per_rad: np.cm_alpha_per_rad(cg_x),
        tail_volume_horizontal,
        tail_volume_vertical,
        cruise_trim,
        spar,
        estimate,
    })
}
