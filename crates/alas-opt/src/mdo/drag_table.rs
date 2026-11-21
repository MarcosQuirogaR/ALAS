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
//!   friction ([`AeroAnalysis::component_skin_friction`]: Torenbeek's
//!   `0.044 / Re^(1/6)` for a lifting surface, the Prandtl-Schlichting
//!   [`AeroAnalysis::turbulent_cf`] for every other component) is evaluated
//!   exactly at the component's own Reynolds number `rho(h) M a(h) L / mu(h)`. Moving the
//!   build-up between altitudes is therefore the skin-friction ratio alone,
//!   with no interpolation in altitude.
//! - **Induced and trim** `CDi_trim(CL)`: the vortex lattice is linear in
//!   circulation, so the trimmed induced drag (wing plus tail load) is a
//!   quadratic `c0 + c1 CL + c2 CL^2` in linear theory. Three trimmed solves
//!   at [`INDUCED_NODE_FRACTIONS`] seed this fit. On the product path the
//!   constant the declared twist leaves at every lift (the wake energy of the
//!   zero-lift span loading, the curve's minimum) is replaced by the
//!   non-optimum-twist allowance `0.0004 CL^2`
//!   (`alas_aero::analysis::NON_OPTIMUM_TWIST_DRAG_FACTOR`: Jenkinson, *Civil
//!   Jet Aircraft Design*, 1999, pp. 179-180), and the result is held at or
//!   above the elliptic loading's `CL^2 / (pi A)`. The cruise trim Jacobian
//!   places and converges each additional solve. Independent fourth-point
//!   and full-range checks retain this parabola when it meets
//!   [`INDUCED_CHECK_RELATIVE_TOLERANCE`]; otherwise checked quadratic cells
//!   are subdivided over zero to clean CLmax. Independent quarter points
//!   bound their error, and the cruise point remains an interior fit node.
//!   This resolves finite-angle geometry without relaxing the bound. When
//!   the cruise trim ran compressible (the default) the nodes are solved on
//!   the Goethert-stretched lattice at the compressible attitude (A1G), so the
//!   term carries the cruise Mach number; otherwise the lattice is
//!   incompressible and it has none.
//! - **Wave** `CDw(CL, M)`: the Lock/Korn law of [`AeroAnalysis::wave_drag`]
//!   (quarter-chord sweep, area-weighted t/c), tabulated on the (CL, M) grid,
//!   cubic Hermite in CL with the law's own analytic CL slope and linear in
//!   Mach. It is exactly zero below the configured onset Mach and below the
//!   Korn critical Mach at the queried CL.
//!
//! # Grid and error bound
//!
//! CL runs from [`CL_STEP`] to the clean `CLmax` in [`CL_STEP`] steps, with
//! the divergence law's curvature break
//! ([`DivergenceLaw::lift_slope_break`]) as a node. Mach
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

use alas_aero::analysis::{AeroAnalysis, DivergenceLaw};

mod grid;
mod induced;
mod kept_nodes;
mod mach_blend;
mod parasite;
mod queries;
mod upsweep;
pub use upsweep::high_lift_lift_shift;

use grid::{cell, cl_grid, hermite, lerp, mach_grid};
pub use induced::InducedCheck;
use induced::{induced_curve, InducedCurve};
pub(crate) use induced::{DesignTrim, StretchedBasis};
use kept_nodes::KeptNodes;
use mach_blend::MachBlend;
use parasite::{DragAreaColumn, ParasiteComponent};
pub use queries::{DragTableOutOfRange, DragTableRange, RangeSide};
use upsweep::UpsweepAttitude;

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

/// Lift-coefficient grid spacing. Only the Mach cells are refined, so the
/// CL error must sit below [`CD_ERROR_BOUND`] on this spacing alone. The
/// cubic Hermite midpoint error with exact end slopes is
/// `h^4/384 * d4CD/dCL4`. With the Korn law that is
/// `h^4/384 * 24 C (dM_crit/dCL)^4`, about 1e-8 in CD at C = 20 and 30
/// degrees of sweep. With the Shevell-Kroo law the curvature break of its
/// crest chart is a node ([`DivergenceLaw::lift_slope_break`]): a cell across
/// it has an `h^2` error instead, up to 2.7e-4 over the envelope below. On
/// the noded grid the largest midpoint error found over section increments
/// -0.04 to 0.06, quarter-chord sweep 15 to 40 degrees, mean t/c 0.08 to
/// 0.16, clean CLmax 1.2 to 1.8 and Mach 0.70 to 0.92 is 7.8e-6 (a
/// conventional section at Mach 0.92 and CL 1.8), so the Mach halvings can
/// close the bound.
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

/// Span-efficiency gain of the take-off flap setting: `delta_e` at the lift
/// `reference_lift_shift` the take-off flaps add at constant angle of attack.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FlapSpanLoading {
    /// Rise of the apparent Oswald factor at the take-off setting.
    pub delta_e: f64,
    /// Lift the take-off setting adds at constant angle of attack ([`high_lift_lift_shift`]).
    pub reference_lift_shift: f64,
}

/// `CD(CL, M, h)` for one trimmed candidate; see the module doc.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct TrimmedDragTable {
    aspect_ratio: f64,
    /// Aspect ratio of the elliptic bound on the induced drag: the overall
    /// projected span of the main wing, wingtip devices included, squared over
    /// the reference area ([`Self::span_bounded`]).
    #[serde(default)]
    span_bound_aspect_ratio: f64,
    /// Flap span-loading effect on the induced drag of high-lift configurations.
    flap_span_loading: Option<FlapSpanLoading>,
    /// Lift-dependent viscous drag fraction `kappa`: `CDv = kappa CD0 CL^2`.
    viscous_lift_fraction: f64,
    design_cl: f64,
    design_mach: f64,
    reference_altitude_m: f64,
    /// Checked quadratic cells of the trimmed wake energy, the basic loading
    /// replaced by the non-optimum-twist allowance when one is in effect.
    induced: InducedCurve,
    /// Non-optimum-twist allowance per unit `CL^2` in `induced` and `blend`,
    /// zero when none is in effect (the frozen reference paths).
    #[serde(default)]
    twist_allowance: f64,
    /// Wake energy of the basic loading removed from the cruise curve.
    #[serde(default)]
    basic_loading_cd: f64,
    /// The incompressible-lattice curve the cruise curve blends against off
    /// the cruise Mach; `None` when the table is incompressible throughout.
    blend: Option<MachBlend>,
    induced_check: InducedCheck,
    mach: Vec<f64>,
    cl: Vec<f64>,
    /// Wave drag and its CL slope, row-major `[mach][cl]`.
    wave: Vec<f64>,
    wave_slope: Vec<f64>,
    wave_onset_mach: f64,
    /// The analysis' drag-divergence law, whose critical Mach bounds the
    /// wave drag; not linear in CL for Torenbeek's Eq. 10.48.
    divergence: DivergenceLaw,
    parasite: Vec<ParasiteComponent>,
    drag_areas: Vec<DragAreaColumn>,
    /// The upsweep column re-evaluated at the trimmed body attitude.
    upsweep: Option<UpsweepAttitude>,
    /// `(alpha_trim deg, CL_trim, CL_alpha per deg)` of the design trim,
    /// the attitude model without trimmed nodes.
    design_attitude: (f64, f64, f64),
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
        // The wake a lifting system sheds spans its whole projected trace,
        // tip devices included: the reference span of a wing with winglets is
        // the span without them (LJ60 12.55 m against 13.35 m overall), and an
        // elliptic bound on it would sit above the minimum induced drag of the
        // span the wing actually has.
        let overall_span = plane
            .wings
            .first()
            .map_or(plane.b_ref, |wing| wing.projected_span())
            .max(plane.b_ref);
        let span_bound_aspect_ratio = overall_span * overall_span / plane.s_ref;
        // The curve on the solved lattice and, when the table blends toward
        // it, the same state's curve on the incompressible lattice are
        // independent sweeps, each in its own fixed order. They run one after
        // the other: both lock the analysis's one lattice cache, so side by
        // side they evict each other's factorization and contend for the
        // pool the lattice fill and LU already use (ATR 72-600 baseline,
        // 4 threads: full analysis 26.6 s side by side, 22.2 s in sequence).
        // The table is the same either way (the cache is order-independent).
        let reference = match (&design.stretched, &design.incompressible) {
            (Some(_), Some(reference)) => Some(reference.as_ref()),
            _ => None,
        };
        let (induced, induced_check) = induced_curve(aero, design, screening)?;
        let incompressible = reference
            .map(|reference| induced_curve(aero, reference, screening))
            .transpose()?;
        let twist = aero.non_optimum_twist_drag_factor();
        let designed = |curve: InducedCurve| match twist {
            Some(factor) => curve.with_twist_allowance(factor),
            None => (curve, 0.0),
        };
        let (induced, basic_loading_cd) = designed(induced);
        let blend = match (&design.stretched, incompressible) {
            (Some(basis), Some((incompressible, _))) => {
                let (incompressible, basic_loading_cd) = designed(incompressible);
                Some(MachBlend {
                    incompressible,
                    basic_loading_cd,
                    cruise_excess: 1.0 / basis.beta - 1.0,
                    cruise_mach: design.mach,
                })
            }
            _ => None,
        };
        let divergence = aero.divergence_law(None);
        let cl = cl_grid(design.cl_max_clean, divergence.lift_slope_break())?;
        let mach = mach_grid(design.mach, aero.drag.wave_drag_onset_mach);
        let breakdown = aero.parasite_breakdown(mach[0], design.altitude_m, None, None);
        let drag_areas = DragAreaColumn::columns(&breakdown.drag_areas);
        let upsweep = UpsweepAttitude::of(&breakdown, aero.afterbody_flow());
        let design_attitude = upsweep::design_attitude(design);
        let parasite = breakdown
            .components
            .iter()
            .map(|term| ParasiteComponent {
                label: term.name.to_owned(),
                reynolds_length_m: term.reynolds_length_m,
                friction: term.friction,
                factor: Vec::new(),
            })
            .collect();
        let mut table = Self {
            aspect_ratio,
            span_bound_aspect_ratio,
            flap_span_loading: None,
            viscous_lift_fraction: aero.lift_dependent_viscous_fraction(),
            design_cl: design.cl,
            design_mach: design.mach,
            reference_altitude_m: design.altitude_m,
            induced,
            twist_allowance: twist.unwrap_or(0.0),
            basic_loading_cd,
            blend,
            induced_check,
            mach: Vec::new(),
            cl,
            wave: Vec::new(),
            wave_slope: Vec::new(),
            wave_onset_mach: if divergence.takes_onset_gate() {
                aero.drag.wave_drag_onset_mach
            } else {
                0.0
            },
            divergence,
            parasite,
            drag_areas,
            upsweep,
            design_attitude,
            max_abs_error_cd: 0.0,
        };
        table.tabulate(aero, &mach)?;
        table.refine(aero)?;
        Ok(table)
    }

    /// The table with the flap span-loading effect of high-lift configurations
    /// ([`FlapSpanLoading`]); the clean polar is unchanged.
    #[must_use]
    pub fn with_flap_span_loading(mut self, loading: FlapSpanLoading) -> Self {
        self.flap_span_loading = Some(loading);
        self
    }

    /// Total drag coefficient at `cl`, `mach` and ISA altitude `altitude_m`,
    /// the upsweep at the trimmed body attitude.
    pub fn cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        let cd0 = self.cd0(mach, altitude_m);
        cd0 + self.viscous_lift_cd(cl, cd0)
            + self.upsweep_attitude_cd(cl, mach)
            + self.induced_at(cl, mach).0
            + self.wave_cd(cl, mach)
    }

    /// Trimmed induced drag, trim drag included, from its checked CL cell, at
    /// the cruise Mach the curve was solved at.
    pub fn induced_cd(&self, cl: f64) -> f64 {
        self.span_bounded(cl, self.induced.value_and_slope(cl)).0
    }

    /// Wake energy of the zero-lift (basic) span loading the declared twist
    /// gives the trimmed lattice at the cruise Mach, which the table does not
    /// carry: zero when no allowance is in effect.
    pub fn basic_loading_cd(&self) -> f64 {
        self.basic_loading_cd
    }

    /// The trimmed wake energy of the lattice as solved at the cruise Mach,
    /// declared twist included: [`Self::induced_cd`] with the basic loading
    /// back and the allowance out.
    pub fn lattice_induced_cd(&self, cl: f64) -> f64 {
        self.induced.value_and_slope(cl).0 + self.basic_loading_cd - self.twist_allowance * cl * cl
    }

    /// [`Self::lattice_induced_cd`] at flight `mach`: the lattice's cruise
    /// and incompressible curves as solved, blended as [`Self::induced_cd_at`]
    /// blends the table's.
    pub fn lattice_induced_cd_at(&self, cl: f64, mach: f64) -> f64 {
        let allowance = self.twist_allowance * cl * cl;
        let cruise = self.induced.value_and_slope(cl).0 + self.basic_loading_cd - allowance;
        match &self.blend {
            Some(blend) => {
                let low =
                    blend.incompressible.value_and_slope(cl).0 + blend.basic_loading_cd - allowance;
                low + blend.weight(mach) * (cruise - low)
            }
            None => cruise,
        }
    }

    /// Trimmed induced drag at `cl` and flight `mach`: [`Self::induced_cd`]
    /// blended toward the incompressible-lattice curve off the cruise Mach.
    pub fn induced_cd_at(&self, cl: f64, mach: f64) -> f64 {
        self.induced_at(cl, mach).0
    }

    /// Value and CL slope of the trimmed induced drag at `mach`: the cruise
    /// curve blended toward the incompressible-lattice curve as the Mach falls.
    fn induced_at(&self, cl: f64, mach: f64) -> (f64, f64) {
        let curve = match &self.blend {
            Some(blend) => blend.value_and_slope(&self.induced, cl, mach),
            None => self.induced.value_and_slope(cl),
        };
        self.span_bounded(cl, curve)
    }

    /// `curve` (value and CL slope of the induced drag at `cl`) held at or
    /// above the wake energy of the elliptic loading on the overall projected
    /// span `b`, `CL^2 / (pi b^2 / S)`: with the basic loading taken out, a
    /// curve whose minimum sits at a positive lift would otherwise pass below
    /// that bound at low lift. The span is the main wing's projected span
    /// with its wingtip devices, not the reference span, which leaves them
    /// out: the elliptic loading is the minimum induced drag of a planar wing
    /// of the span the trailing wake has (Munk; J. D. Anderson, *Fundamentals
    /// of Aerodynamics*, 5th ed., sec. 5.3.1), and the lattice the curve comes
    /// from carries the devices. The lattice's own curve (no allowance in
    /// effect) is returned as solved.
    fn span_bounded(&self, cl: f64, curve: (f64, f64)) -> (f64, f64) {
        if self.twist_allowance == 0.0 {
            return curve;
        }
        let aspect_ratio = if self.span_bound_aspect_ratio > 0.0 {
            self.span_bound_aspect_ratio
        } else {
            self.aspect_ratio
        };
        let elliptic = 1.0 / (std::f64::consts::PI * aspect_ratio);
        if curve.0 < elliptic * cl * cl {
            (elliptic * cl * cl, 2.0 * elliptic * cl)
        } else {
            curve
        }
    }

    /// Span efficiency the trimmed induced drag implies at `cl`:
    /// `CL^2 / (pi AR CDi_trim)`.
    pub fn effective_oswald(&self, cl: f64) -> f64 {
        cl * cl / (std::f64::consts::PI * self.aspect_ratio * self.induced_cd(cl))
    }

    /// The analysis' drag-divergence law the wave column is tabulated on.
    pub fn divergence_law(&self) -> &DivergenceLaw {
        &self.divergence
    }

    /// Critical Mach of the analysis' divergence law at `cl`.
    pub fn critical_mach(&self, cl: f64) -> f64 {
        self.divergence.critical_mach(cl)
    }

    /// Wave drag `CDw(CL, M)`: zero below the onset Mach and below
    /// [`Self::critical_mach`], otherwise interpolated and non-negative.
    pub fn wave_cd(&self, cl: f64, mach: f64) -> f64 {
        self.wave_value_and_slope(cl, mach).0
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
        if mach < self.wave_onset_mach || mach <= self.divergence.onset_mach(cl) {
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

    /// Fill every per-Mach column on `mach`, copying the nodes the table
    /// already holds ([`KeptNodes`]).
    fn tabulate(&mut self, aero: &AeroAnalysis<'_>, mach: &[f64]) -> Result<(), DragTableError> {
        let kept = KeptNodes::take(self, mach);
        let margin = aero.drag.margin();
        let s_ref = aero.plane.s_ref;
        for &m in mach {
            if kept.push(self, m) {
                continue;
            }
            let breakdown = aero.parasite_breakdown(m, self.reference_altitude_m, None, None);
            let terms = &breakdown.components;
            if terms.len() != self.parasite.len() {
                return Err(DragTableError::NonPhysical("parasite component count"));
            }
            DragAreaColumn::push_all(&mut self.drag_areas, &breakdown.drag_areas, margin)?;
            for (component, term) in self.parasite.iter_mut().zip(terms) {
                let factor = term.form_factor
                    * term.interference_factor
                    * (term.wetted_area_m2 / s_ref)
                    * breakdown.component_scale
                    * margin;
                if !factor.is_finite() || factor < 0.0 {
                    return Err(DragTableError::NonPhysical("parasite factor"));
                }
                component.factor.push(factor);
            }
            for &c in &self.cl {
                let value = aero.wave_drag(m, c, None);
                let slope = self.divergence.wave_cd_lift_slope(m, c, value);
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
                let viscous_error = self.viscous_lift_fraction * parasite_error * cl * cl;
                let error = (parasite_error + viscous_error + wave_error).abs();
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
