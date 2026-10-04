// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trimmed drag table one candidate's missions are flown on:
//! `CD(CL, M, h) = CD0(M, h) + CDi_trim(CL) + CDw(CL, M)`.
//!
//! Every term comes from the native build-up the cruise trim already runs,
//! evaluated on the same [`AeroAnalysis`] (quarter-chord sweep, candidate
//! geometry and drag model):
//!
//! - **Parasite** `CD0(M, h)`: [`AeroAnalysis::parasite_breakdown`]. Each
//!   component's skin-friction-free factor `FF Q Swet/S` (times the viscous
//!   margin) is tabulated on the Mach grid; the Prandtl-Schlichting skin
//!   friction [`AeroAnalysis::turbulent_cf`] is evaluated exactly at the
//!   component's own Reynolds number `rho(h) M a(h) L / mu(h)`. Moving the
//!   build-up between altitudes is therefore the skin-friction ratio alone,
//!   with no interpolation in altitude.
//! - **Induced and trim** `CDi_trim(CL)`: the vortex lattice is linear in
//!   circulation, so the trimmed induced drag (wing plus tail load) is a
//!   quadratic `c0 + c1 CL + c2 CL^2` in linear theory. Three trimmed solves
//!   at [`INDUCED_NODE_FRACTIONS`] seed this fit. The cruise trim Jacobian
//!   places and converges each additional solve. Independent fourth-point
//!   and full-range checks retain this parabola when it meets
//!   [`INDUCED_CHECK_RELATIVE_TOLERANCE`]; otherwise checked quadratic cells
//!   are subdivided over zero to clean CLmax. Independent quarter points
//!   bound their error, and the cruise point remains an interior fit node.
//!   This resolves finite-angle geometry without relaxing the bound. The
//!   lattice is incompressible, so this term has no Mach dependence.
//! - **Wave** `CDw(CL, M)`: the Lock/Korn law of [`AeroAnalysis::wave_drag`]
//!   (quarter-chord sweep, area-weighted t/c), tabulated on the (CL, M) grid,
//!   cubic Hermite in CL with the law's own analytic CL slope and linear in
//!   Mach. It is exactly zero below the configured onset Mach and below the
//!   Korn critical Mach at the queried CL.
//!
//! # Grid and error bound
//!
//! CL runs from [`CL_STEP`] to the clean `CLmax` in [`CL_STEP`] steps. Mach
//! runs from [`MACH_MIN`] in [`MACH_COARSE_STEP`] steps to
//! [`MACH_FINE_FROM`], then in [`MACH_FINE_STEP`] steps to
//! `M_cruise + `[`MACH_ABOVE_CRUISE`], with the wave-onset Mach as a node.
//! Every Mach cell whose total-CD error at the cell centres, against direct
//! evaluation of the native functions at the reference altitude and at sea
//! level, exceeds [`CD_ERROR_BOUND`] is halved, up to
//! [`MAX_REFINEMENT_PASSES`] times; a grid that still misses the bound is an
//! error. The largest remaining error is [`TrimmedDragTable::max_abs_error_cd`].
//!
//! Outside the grid, CL and Mach are clamped for the tabulated factors only
//! (form factor and wave drag); the nearest induced cell and the skin friction
//! are evaluated at the queried values.
//!
//! # Units
//!
//! Mach and coefficients dimensionless, referred to `Airplane::s_ref`;
//! altitudes m (ISA geometric, standard day); angles degrees.

use alas_aero::analysis::AeroAnalysis;

mod grid;
mod induced;
mod parasite;

use grid::{cell, cl_grid, hermite, lerp, mach_grid};
pub(crate) use induced::DesignTrim;
pub use induced::InducedCheck;
use induced::{induced_curve, InducedCurve};

/// Cruise-CL fractions of the three trimmed induced-drag nodes: from a
/// light, low-lift descent (0.35) to a heavy, near-buffet cruise (1.4), so
/// the flown lift range is interpolated rather than extrapolated.
pub const INDUCED_NODE_FRACTIONS: [f64; 3] = [0.35, 1.0, 1.4];

/// Cruise-CL fraction of the fourth, checking trimmed solve: inside the
/// lower node interval, where most climb, descent and light cruise is flown.
pub const INDUCED_CHECK_FRACTION: f64 = 0.7;

/// Largest relative disagreement at an independent trimmed checking point.
/// Induced drag is about a third of cruise drag, so 0.3 % of CDi is 0.1 %
/// of total drag, below the build-up's resolution. Refine the representation
/// when finite-angle geometry exceeds this bound.
pub const INDUCED_CHECK_RELATIVE_TOLERANCE: f64 = 3.0e-3;

/// Largest total-CD interpolation error accepted at a cell centre: a tenth
/// of a drag count, 0.03 % of a transport's cruise CD of about 0.03.
pub const CD_ERROR_BOUND: f64 = 1.0e-5;

/// Lift-coefficient grid spacing. With the Korn law's analytic CL slope the
/// cubic Hermite error is `h^4/384 * 24 C (dM_crit/dCL)^4`, about 1e-8 in
/// CD at C = 20 and 30 degrees of sweep, so the Mach cells set the error.
pub const CL_STEP: f64 = 0.05;

/// Lowest Mach node, low enough that a turboprop's approach and lift-off
/// Mach (about 0.17) is interpolated rather than clamped: the wing form
/// factor carries `M^0.18`, which moves CD0 by 5 % between Mach 0.15 and 0.20.
pub const MACH_MIN: f64 = 0.10;

/// Mach grid spacing below [`MACH_FINE_FROM`], where only the `M^0.18`
/// form factor varies; refinement adds nodes where its curvature needs them.
pub const MACH_COARSE_STEP: f64 = 0.05;

/// Mach above which the grid is spaced [`MACH_FINE_STEP`]: below the Korn
/// critical Mach of every transport section at cruise lift.
pub const MACH_FINE_FROM: f64 = 0.70;

/// Mach grid spacing above [`MACH_FINE_FROM`], starting resolution for the
/// `C (M - M_crit)^4` rise before refinement.
pub const MACH_FINE_STEP: f64 = 0.01;

/// Mach margin the grid extends past the cruise Mach: the MMO margin of a
/// transport (A320 0.78 cruise / 0.82 MMO, EASA TCDS A.064; B787 0.85 /
/// 0.90, FAA TCDS T00021SE).
pub const MACH_ABOVE_CRUISE: f64 = 0.06;

/// Mach-cell halvings allowed before the error bound is declared unmet.
/// Eight halvings take a 0.01 cell to 4e-5, far below any smooth-law need.
pub const MAX_REFINEMENT_PASSES: usize = 8;

/// Why a trimmed drag table could not be built. None of these is repaired.
#[derive(Debug, Clone, PartialEq)]
pub enum DragTableError {
    /// A vortex-lattice solve failed.
    Solve(String),
    /// The trim Jacobian is singular or not finite.
    SingularJacobian,
    /// An induced node did not reach the moment tolerance.
    UntrimmedNode {
        /// Lift coefficient the node was placed at.
        cl_target: f64,
        /// Pitching moment left after the last correction.
        cm_residual: f64,
    },
    /// An independent trimmed point disagrees with the fitted quadratic cell.
    InducedCheck(InducedCheck),
    /// A term came out non-finite or with a non-physical sign.
    NonPhysical(&'static str),
    /// The Mach grid could not be refined to [`CD_ERROR_BOUND`].
    Refinement {
        /// Largest cell-centre error of the last grid.
        max_abs_error_cd: f64,
    },
}

impl std::fmt::Display for DragTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solve(error) => write!(f, "drag-table VLM solve failed: {error}"),
            Self::SingularJacobian => write!(f, "drag-table trim Jacobian is singular"),
            Self::UntrimmedNode {
                cl_target,
                cm_residual,
            } => write!(
                f,
                "induced node at CL {cl_target:.4} left Cm {cm_residual:.3e} untrimmed"
            ),
            Self::InducedCheck(check) => write!(
                f,
                "independent trimmed point at CL {:.4}: VLM CDi {:.6e}, quadratic {:.6e} ({:.3} %)",
                check.cl,
                check.vlm_cd_induced,
                check.quadratic_cd_induced,
                100.0 * check.relative_error()
            ),
            Self::NonPhysical(what) => write!(f, "drag-table term is non-physical: {what}"),
            Self::Refinement { max_abs_error_cd } => write!(
                f,
                "drag-table error {max_abs_error_cd:.3e} exceeds {CD_ERROR_BOUND:.1e}"
            ),
        }
    }
}

/// One parasite component with its skin-friction-free factor per Mach node.
#[derive(Debug, Clone, PartialEq)]
struct ParasiteComponent {
    label: String,
    reynolds_length_m: f64,
    /// `FF Q Swet/S * viscous_margin` at each Mach node.
    factor: Vec<f64>,
}

/// `CD(CL, M, h)` for one trimmed candidate; see the module doc.
#[derive(Debug, Clone, PartialEq)]
pub struct TrimmedDragTable {
    aspect_ratio: f64,
    design_cl: f64,
    design_mach: f64,
    reference_altitude_m: f64,
    /// Checked quadratic cells of the trimmed wake energy.
    induced: InducedCurve,
    induced_check: InducedCheck,
    mach: Vec<f64>,
    cl: Vec<f64>,
    /// Wave drag and its CL slope, row-major `[mach][cl]`.
    wave: Vec<f64>,
    wave_slope: Vec<f64>,
    wave_onset_mach: f64,
    /// Korn critical Mach `M_crit(CL) = mcrit_at_zero_cl + mcrit_per_cl CL`.
    mcrit_at_zero_cl: f64,
    mcrit_per_cl: f64,
    parasite: Vec<ParasiteComponent>,
    max_abs_error_cd: f64,
}

impl TrimmedDragTable {
    /// Build the table from the analysis the cruise trim was evaluated on.
    ///
    /// # Errors
    ///
    /// See [`DragTableError`].
    #[cfg(test)]
    pub(crate) fn build(
        aero: &AeroAnalysis<'_>,
        design: &DesignTrim,
    ) -> Result<Self, DragTableError> {
        Self::build_with_check_set(aero, design, false)
    }

    /// Build with fewer independent checks only for screening. Any failed
    /// check uses the full adaptive representation and its original bounds.
    pub(crate) fn build_with_check_set(
        aero: &AeroAnalysis<'_>,
        design: &DesignTrim,
        screening: bool,
    ) -> Result<Self, DragTableError> {
        let plane = aero.plane;
        let aspect_ratio = plane.b_ref * plane.b_ref / plane.s_ref;
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
            return Err(DragTableError::NonPhysical("aspect ratio"));
        }
        let (induced, induced_check) = induced_curve(aero, design, screening)?;
        let mcrit_at_zero_cl = aero.korn_mach_numbers(0.0, None).1;
        let mcrit_per_cl = aero.korn_mach_numbers(1.0, None).1 - mcrit_at_zero_cl;
        let cl = cl_grid(design.cl_max_clean)?;
        let mach = mach_grid(design.mach, aero.drag.wave_drag_onset_mach);
        let parasite = aero
            .parasite_breakdown(mach[0], design.altitude_m, None, None)
            .components
            .iter()
            .map(|term| ParasiteComponent {
                label: term.name.to_owned(),
                reynolds_length_m: term.reynolds_length_m,
                factor: Vec::new(),
            })
            .collect();
        let mut table = Self {
            aspect_ratio,
            design_cl: design.cl,
            design_mach: design.mach,
            reference_altitude_m: design.altitude_m,
            induced,
            induced_check,
            mach: Vec::new(),
            cl,
            wave: Vec::new(),
            wave_slope: Vec::new(),
            wave_onset_mach: aero.drag.wave_drag_onset_mach,
            mcrit_at_zero_cl,
            mcrit_per_cl,
            parasite,
            max_abs_error_cd: 0.0,
        };
        table.tabulate(aero, &mach)?;
        table.refine(aero)?;
        Ok(table)
    }

    /// Total drag coefficient at `cl`, `mach` and ISA altitude `altitude_m`.
    pub fn cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        self.cd0(mach, altitude_m) + self.induced_cd(cl) + self.wave_cd(cl, mach)
    }

    /// Trimmed induced drag, trim drag included, from its checked CL cell.
    pub fn induced_cd(&self, cl: f64) -> f64 {
        self.induced.value_and_slope(cl).0
    }

    /// Span efficiency the trimmed induced drag implies at `cl`:
    /// `CL^2 / (pi AR CDi_trim)`.
    pub fn effective_oswald(&self, cl: f64) -> f64 {
        cl * cl / (std::f64::consts::PI * self.aspect_ratio * self.induced_cd(cl))
    }

    /// Korn critical Mach at `cl`.
    pub fn critical_mach(&self, cl: f64) -> f64 {
        self.mcrit_at_zero_cl + self.mcrit_per_cl * cl
    }

    /// Wave drag `CDw(CL, M)`: zero below the onset Mach and below
    /// [`Self::critical_mach`], otherwise interpolated and non-negative.
    pub fn wave_cd(&self, cl: f64, mach: f64) -> f64 {
        self.wave_value_and_slope(cl, mach).0
    }

    /// The parabola `cd0 + k CL^2` tangent to the table at the design lift
    /// coefficient, Mach `mach` and the reference altitude: it reproduces
    /// the drag and its CL slope there, wave drag included.
    pub fn parabolic_equivalent(&self, mach: f64) -> (f64, f64) {
        self.parabolic_equivalent_at(self.design_cl, mach, self.reference_altitude_m)
    }

    /// The parabola `cd0 + k CL^2` tangent to the shared table at positive
    /// `cl`, `mach` and `altitude_m`, retaining trim and wave drag's local slope.
    pub fn parabolic_equivalent_at(&self, cl: f64, mach: f64, altitude_m: f64) -> (f64, f64) {
        let cd = self.cd(cl, mach, altitude_m);
        let slope = self.induced.value_and_slope(cl).1 + self.wave_value_and_slope(cl, mach).1;
        let k = slope / (2.0 * cl);
        (cd - k * cl * cl, k)
    }

    /// Lift coefficient of maximum `CL/CD` (minimum drag force) at `mach`
    /// and `altitude_m`, by golden-section search over the CL grid.
    pub fn min_drag_cl(&self, mach: f64, altitude_m: f64) -> f64 {
        let ratio = |cl: f64| cl / self.cd(cl, mach, altitude_m);
        let golden = 0.5 * (5.0_f64.sqrt() - 1.0);
        let cl_min = self.cl[0];
        let cl_max = self.cl[self.cl.len() - 1];
        self.induced
            .ranges()
            .filter_map(|(low, high)| {
                let (mut low, mut high) = (low.max(cl_min), high.min(cl_max));
                if low >= high {
                    return None;
                }
                // 60 contractions by 0.618 shrink the bracket below 1e-12 of its size.
                for _ in 0..60 {
                    let a = high - golden * (high - low);
                    let b = low + golden * (high - low);
                    if ratio(a) < ratio(b) {
                        low = a;
                    } else {
                        high = b;
                    }
                }
                Some(0.5 * (low + high))
            })
            .max_by(|left, right| ratio(*left).total_cmp(&ratio(*right)))
            .unwrap_or(cl_min)
    }

    /// Largest total-CD error measured at the cell centres of the final grid.
    pub fn max_abs_error_cd(&self) -> f64 {
        self.max_abs_error_cd
    }

    /// Lift coefficient of the cruise trim the table was built around.
    pub fn design_cl(&self) -> f64 {
        self.design_cl
    }

    /// Cruise Mach of that trim.
    pub fn design_mach(&self) -> f64 {
        self.design_mach
    }

    /// ISA altitude of that trim, m.
    pub fn reference_altitude_m(&self) -> f64 {
        self.reference_altitude_m
    }

    /// The fourth-point check the table passed.
    pub fn induced_check(&self) -> InducedCheck {
        self.induced_check
    }

    /// Number of (CL, Mach) nodes.
    pub fn grid_size(&self) -> (usize, usize) {
        (self.cl.len(), self.mach.len())
    }

    /// Mach cell index and linear weight, clamped to the grid.
    fn mach_cell(&self, mach: f64) -> (usize, f64) {
        cell(&self.mach, mach)
    }

    fn wave_value_and_slope(&self, cl: f64, mach: f64) -> (f64, f64) {
        if mach < self.wave_onset_mach || mach <= self.critical_mach(cl) {
            return (0.0, 0.0);
        }
        let (i, wm) = self.mach_cell(mach);
        let (j, _) = cell(&self.cl, cl);
        let clc = cl.clamp(self.cl[0], self.cl[self.cl.len() - 1]);
        let row = |k: usize| {
            let at = |c: usize| {
                (
                    self.wave[k * self.cl.len() + c],
                    self.wave_slope[k * self.cl.len() + c],
                )
            };
            hermite(self.cl[j], self.cl[j + 1], at(j), at(j + 1), clc)
        };
        let (lo, hi) = (row(i), row(i + 1));
        let value = lerp(lo.0, hi.0, wm);
        if value <= 0.0 {
            return (0.0, 0.0);
        }
        (value, lerp(lo.1, hi.1, wm))
    }

    /// Fill every per-Mach column on `mach`.
    fn tabulate(&mut self, aero: &AeroAnalysis<'_>, mach: &[f64]) -> Result<(), DragTableError> {
        self.mach = mach.to_vec();
        self.wave.clear();
        self.wave_slope.clear();
        for component in &mut self.parasite {
            component.factor.clear();
        }
        let margin = aero.drag.viscous_margin;
        let s_ref = aero.plane.s_ref;
        for &m in mach {
            let terms = aero
                .parasite_breakdown(m, self.reference_altitude_m, None, None)
                .components;
            if terms.len() != self.parasite.len() {
                return Err(DragTableError::NonPhysical("parasite component count"));
            }
            for (component, term) in self.parasite.iter_mut().zip(&terms) {
                let factor = term.form_factor
                    * term.interference_factor
                    * (term.wetted_area_m2 / s_ref)
                    * margin;
                if !factor.is_finite() || factor < 0.0 {
                    return Err(DragTableError::NonPhysical("parasite factor"));
                }
                component.factor.push(factor);
            }
            for &c in &self.cl {
                let value = aero.wave_drag(m, c, None);
                let excess = m - self.critical_mach(c);
                let slope = if value > 0.0 && excess > 0.0 {
                    -4.0 * value / excess * self.mcrit_per_cl
                } else {
                    0.0
                };
                if !value.is_finite() || value < 0.0 || !slope.is_finite() {
                    return Err(DragTableError::NonPhysical("wave drag"));
                }
                self.wave.push(value);
                self.wave_slope.push(slope);
            }
        }
        Ok(())
    }

    /// Halve every Mach cell that misses [`CD_ERROR_BOUND`] until none does.
    fn refine(&mut self, aero: &AeroAnalysis<'_>) -> Result<(), DragTableError> {
        for _ in 0..=MAX_REFINEMENT_PASSES {
            let mut worst = 0.0_f64;
            let mut inserted = Vec::new();
            for pair in self.mach.windows(2) {
                let middle = 0.5 * (pair[0] + pair[1]);
                let error = self.cell_error(aero, middle);
                worst = worst.max(error);
                if error > CD_ERROR_BOUND {
                    inserted.push(middle);
                }
            }
            if inserted.is_empty() {
                self.max_abs_error_cd = worst;
                return Ok(());
            }
            let mut mach = self.mach.clone();
            mach.extend(inserted);
            mach.sort_by(f64::total_cmp);
            self.tabulate(aero, &mach)?;
            self.max_abs_error_cd = worst;
        }
        Err(DragTableError::Refinement {
            max_abs_error_cd: self.max_abs_error_cd,
        })
    }

    /// Largest `|table - direct|` total CD over the CL cell centres at
    /// `mach`, at the reference altitude and at sea level.
    fn cell_error(&self, aero: &AeroAnalysis<'_>, mach: f64) -> f64 {
        let mut worst = 0.0_f64;
        for altitude_m in [self.reference_altitude_m, 0.0] {
            let parasite_error =
                self.cd0(mach, altitude_m) - aero.parasite_drag(mach, altitude_m, 0.0, None, None);
            for pair in self.cl.windows(2) {
                let cl = 0.5 * (pair[0] + pair[1]);
                let wave_error = self.wave_cd(cl, mach) - aero.wave_drag(mach, cl, None);
                let error = (parasite_error + wave_error).abs();
                worst = worst.max(if error.is_finite() {
                    error
                } else {
                    f64::INFINITY
                });
            }
        }
        worst
    }
}

#[cfg(test)]
mod tests;
