// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result types of the viscous VLM.

use crate::aero::validity::Validity;

/// Converged state of one strip.
#[derive(Debug, Clone, PartialEq)]
pub struct StripResult {
    /// Strip centre, m (negative on the left half).
    pub y_m: f64,
    /// Local chord, m.
    pub chord_m: f64,
    /// Strip width, m.
    pub width_m: f64,
    /// Local chord Reynolds number (with the slipstream speed ratio).
    pub re: f64,
    /// Physical effective angle of attack, rad, chord-relative.
    pub alpha_eff_rad: f64,
    /// Lattice section lift on the freestream dynamic pressure.
    pub cl_vlm: f64,
    /// Section model lift on the local dynamic pressure.
    pub cl_section: f64,
    /// Section profile drag on the local chord and local dynamic pressure.
    pub cd_section: f64,
    /// Section quarter-chord moment coefficient (not coupled into `CM`).
    pub cm_section: f64,
    /// Local dynamic-pressure ratio (slipstream).
    pub q_ratio: f64,
    /// Slipstream coverage of the strip.
    pub coverage: f64,
    /// Domain findings of this strip.
    pub validity: Validity,
}

/// One converged (or best-effort) solution.
#[derive(Debug, Clone, PartialEq)]
pub struct ViscousSolution {
    /// Freestream angle of attack, rad (body axis to freestream).
    pub alpha_rad: f64,
    /// Lift coefficient on `S_ref`.
    pub cl: f64,
    /// Trefftz-plane induced-drag coefficient on `S_ref`.
    pub cd_induced: f64,
    /// Wing profile-drag coefficient on `S_ref`.
    pub cd_profile: f64,
    /// Lattice pitching-moment coefficient about `airplane.xyz_ref`, nose-up
    /// positive, on `S_ref` and `c_ref`.
    pub cm: f64,
    /// Span efficiency `CL^2/(pi A CDi)` (`None` when `CL` or `CDi` is too
    /// small to carry information).
    pub span_efficiency: Option<f64>,
    /// Lift for the supplied air state, N.
    pub lift_n: f64,
    /// Strips of the main wing in lattice order.
    pub strips: Vec<StripResult>,
    /// Incidence offsets per strip (warm start of the next solve), rad.
    pub offsets_rad: Vec<f64>,
    /// Iterations used.
    pub iterations: usize,
    /// Final `max |cl* - cl_vlm|`.
    pub residual: f64,
    /// Whether `residual < tolerance`.
    pub converged: bool,
    /// Aggregated domain findings.
    pub validity: Validity,
}

impl ViscousSolution {
    /// Wing drag coefficient `CDi + CDp` (non-wing parasite drag excluded).
    pub fn cd_wing(&self) -> f64 {
        self.cd_induced + self.cd_profile
    }
}

/// How the first stalled strip was identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallCriterion {
    /// Section lift reached 97 % of the section `cl_max` at the strip Re.
    Utilisation,
    /// Effective angle reached the angle of the section `cl_max`.
    AngleOfClMax,
    /// Neither happened before the coupled solution folded (stall onset
    /// between two solved angles): the strip closest to its `cl_max` at the
    /// last solved angle.
    Fold,
}

/// Where the wing first stalls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FirstStall {
    /// Strip centre, m.
    pub y_m: f64,
    /// Freestream angle of attack, rad.
    pub alpha_rad: f64,
    /// Whether the stall starts outboard of 60 % semispan (tip stall).
    pub tip_stall: bool,
    /// How it was identified.
    pub criterion: StallCriterion,
}

/// Maximum-lift result of an angle sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct ClMax {
    /// Maximum wing lift coefficient of the sweep.
    pub cl_max: f64,
    /// Angle of attack of the maximum, rad.
    pub alpha_rad: f64,
    /// Plan 3.1 uncertainty band until calibrated (ESTIMATE): +-0.1.
    pub band: f64,
    /// First strip to reach its 2-D `cl_max`, if any strip did.
    pub first_stall: Option<FirstStall>,
    /// Whether the maximum point was converged.
    pub converged_at_max: bool,
    /// All evaluated points `(alpha_rad, CL, converged)`.
    pub curve: Vec<(f64, f64, bool)>,
    /// Why the sweep ended before `stop_rad` or the post-peak drop, if a
    /// strip left the section-model domain (deep stall: the fixed point puts
    /// stalled strips at 25 deg or more, where `CL` is not trusted).
    pub terminated_by: Option<String>,
}
