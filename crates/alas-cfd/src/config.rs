// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User-facing, SI-valued OpenFOAM study configuration.

use serde::{Deserialize, Serialize};

use super::boundary::BoundarySettings;
use super::conventions::{
    COMPRESSIBLE_SOLVER_THRESHOLD_MACH, MAX_SUPPORTED_MACH, TRANSONIC_LOWER_MACH,
    TRANSONIC_UPPER_MACH,
};
use super::turbulence::{TurbulenceSpecification, TurbulenceState};

fn default_turbulence_viscosity_ratio() -> f64 {
    0.009
}

/// Ratio of specific heats used for the dry-air speed-of-sound calculation and
/// the perfect-gas thermodynamic state selected on the compressible path.
pub const DRY_AIR_GAMMA: f64 = 1.4;

/// Specific gas constant for dry air in J/(kg K), used with
/// [`DRY_AIR_GAMMA`] for Mach and perfect-gas pressure/density relations.
pub const DRY_AIR_GAS_CONSTANT_J_KG_K: f64 = 287.052_87;

fn default_freestream_temperature_k() -> f64 {
    288.15
}

fn default_derive_first_layer() -> bool {
    true
}

/// Source used to determine the dimensional velocity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingInput {
    /// The user enters speed; Reynolds number is derived.
    #[default]
    Speed,
    /// The user enters Reynolds number; speed is derived.
    Reynolds,
}

/// Aerodynamic regime selected from the effective freestream Mach number.
///
/// The thresholds are freestream values derived from the entered speed (or
/// the speed implied by Reynolds number), density and declared static
/// temperature.  They are not a user supplied solver switch: changing any of
/// those inputs can change the regime and therefore the generated equations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowRegime {
    /// Constant-density steady RANS is appropriate when compressibility is
    /// negligible at the imposed freestream Mach number.
    LowSubsonic,
    /// A perfect-gas steady RANS equation set is used below the transonic
    /// shock regime once density variation is no longer negligible.
    CompressibleSubsonic,
    /// The steady perfect-gas solver is retained with shock-safe bounded
    /// schemes through the transonic band.
    Transonic,
    /// Supersonic flow is outside the requested transonic validation domain,
    /// but the same density-based solver remains physically appropriate for
    /// the supported upper Mach limit.
    Supersonic,
}

impl FlowRegime {
    /// Select a regime from freestream Mach number using the constants in
    /// `conventions.rs`.
    pub fn from_mach(mach: f64) -> Self {
        if mach < COMPRESSIBLE_SOLVER_THRESHOLD_MACH {
            Self::LowSubsonic
        } else if mach < TRANSONIC_LOWER_MACH {
            Self::CompressibleSubsonic
        } else if mach < TRANSONIC_UPPER_MACH {
            Self::Transonic
        } else {
            Self::Supersonic
        }
    }

    /// Whether the density and energy equations must be solved.
    pub fn is_compressible(self) -> bool {
        !matches!(self, Self::LowSubsonic)
    }

    /// Stable report/UI label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LowSubsonic => "low subsonic",
            Self::CompressibleSubsonic => "compressible subsonic",
            Self::Transonic => "transonic",
            Self::Supersonic => "supersonic",
        }
    }
}

/// OpenFOAM executable selected by [`CfdStudyConfig::effective_simulation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CfdSolverKind {
    /// Constant-density steady RANS solver.
    SimpleFoam,
    /// Perfect-gas, density-based steady RANS solver.
    RhoSimpleFoam,
}

impl CfdSolverKind {
    /// Executable name in an OpenFOAM installation.
    pub fn executable(self) -> &'static str {
        match self {
            Self::SimpleFoam => "simpleFoam",
            Self::RhoSimpleFoam => "rhoSimpleFoam",
        }
    }

    /// Stable human-readable label.
    pub fn as_str(self) -> &'static str {
        self.executable()
    }
}

/// Solver and discretisation controls after Mach-regime policy is applied.
///
/// `SolverSettings` remains the persisted editor contract.  This type is the
/// exact execution contract used by case generation and is recorded through
/// the resolved configuration in `study.json`; it prevents a low-Mach setting
/// such as a constant-density solver or unbounded gradient from silently
/// leaking into a shock-containing case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveSimulationSettings {
    /// Regime inferred from the input state.
    pub regime: FlowRegime,
    /// Solver executable selected for this regime.
    pub solver: CfdSolverKind,
    /// Whether the case contains density and energy equations.
    pub compressible: bool,
    /// Maximum outer iterations actually emitted.
    pub max_iterations: u32,
    /// Bounded-upwind startup iterations actually emitted.
    pub startup_iterations: u32,
    /// Final momentum convection scheme label.
    pub convection_scheme: ConvectionScheme,
    /// Final turbulence convection scheme label.
    pub turbulence_convection_scheme: TurbulenceConvectionScheme,
    /// Cell-gradient limiter coefficient actually emitted.
    pub gradient_limiter: f64,
    /// Non-orthogonal correction limiter actually emitted.
    pub non_orthogonal_limiter: f64,
    /// Number of pressure correctors actually emitted.
    pub non_orthogonal_correctors: u32,
    /// Pressure relaxation actually emitted.
    pub pressure_relaxation: f64,
    /// Momentum relaxation actually emitted.
    pub equation_relaxation: f64,
    /// Turbulence relaxation actually emitted.
    pub turbulence_relaxation: f64,
    /// Inner pressure relative tolerance actually emitted.
    pub pressure_relative_tolerance: f64,
    /// Top-level field write interval actually emitted.
    pub write_interval: u32,
}

/// Repeatable mesh density preset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshPreset {
    /// Fast preflight with a coarse outer block and surface refinement.
    Coarse,
    /// Default engineering setup.
    #[default]
    Medium,
    /// Higher resolution for sensitivity checks.
    Fine,
}

impl MeshPreset {
    /// Base cells in `(x, y, z)` for the background block.
    pub fn base_cells(self) -> (u32, u32, u32) {
        match self {
            Self::Coarse => (120, 80, 1),
            Self::Medium => (180, 120, 1),
            Self::Fine => (280, 180, 1),
        }
    }

    /// Surface refinement level retained for preset summaries and exports.
    pub fn refinement_level(self) -> u32 {
        match self {
            Self::Coarse => 2,
            Self::Medium => 3,
            Self::Fine => 4,
        }
    }
}

/// How the far-field patch receives the freestream state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FarFieldCondition {
    /// A fixed velocity and zero-gradient pressure condition.
    #[default]
    FixedValue,
    /// OpenFOAM's `freestream` mixed condition.
    Freestream,
}

/// Background and near-wall mesh controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeshSettings {
    /// Preset used for repeatable initial studies.
    pub preset: MeshPreset,
    /// Outer-domain upstream extent in chords.
    pub upstream_chords: f64,
    /// Outer-domain downstream extent in chords.
    pub downstream_chords: f64,
    /// Half-height of the outer domain in chords.
    pub half_height_chords: f64,
    /// First-cell-centre wall distance in metres, used when
    /// [`Self::derive_first_layer_from_target_y_plus`] is `false`.
    ///
    /// A raw length is not tied to any flow state, so the same value gives a
    /// different y+ at every Reynolds number.  It is kept as an explicit
    /// override for a user who has a reason to pin the first cell.
    pub first_layer_height_m: f64,
    /// Desired y+ for the selected wall treatment.
    pub target_y_plus: f64,
    /// Size the first cell from [`Self::target_y_plus`] and the flow state
    /// instead of taking [`Self::first_layer_height_m`] literally.
    ///
    /// With the literal 1e-5 m default at Re = 6e6 the measured wall y+ is
    /// 0.67 .. 4.99, straddling the viscous/log switch of the blended
    /// `omegaWallFunction`; deriving the distance instead puts it at
    /// 0.28 .. 2.04 and drops the limiting omega residual by a factor of four
    /// on an otherwise identical case (dispatch evidence
    /// `.agent/opus-cfd-20260916`, cases `E10` and `G10`).  Both the requested
    /// and the derived distance stay in `BoundaryLayerSizing`, so which one was
    /// used is always visible.
    #[serde(default = "default_derive_first_layer")]
    pub derive_first_layer_from_target_y_plus: bool,
    /// Whether layer controls are emitted for a later validated layer study.
    pub boundary_layers: bool,
    /// Number of prism layers when boundary layers are enabled.
    pub n_layers: u32,
    /// Surface refinement multiplier for the wake region.
    pub wake_refinement: u32,
    /// Leading-edge refinement level: each level halves the surface size
    /// around the leading edge.  Zero keeps the qualified template output.
    pub leading_edge_refinement: u32,
}

impl Default for MeshSettings {
    fn default() -> Self {
        Self {
            preset: MeshPreset::Medium,
            upstream_chords: 10.0,
            downstream_chords: 20.0,
            half_height_chords: 10.0,
            first_layer_height_m: 1.0e-5,
            target_y_plus: 1.0,
            derive_first_layer_from_target_y_plus: default_derive_first_layer(),
            boundary_layers: true,
            n_layers: 25,
            wake_refinement: 2,
            leading_edge_refinement: 0,
        }
    }
}

/// Steady RANS solver and convergence controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SolverSettings {
    /// Maximum SIMPLE iterations.
    pub max_iterations: u32,
    /// Outer SIMPLE residual threshold applied to all primary equations. The
    /// convergence gate uses the last outer `Initial residual`; the inner
    /// linear-solver final residual remains audit evidence only.
    pub residual_tolerance: f64,
    /// Relative force-history spread over the tail window.
    pub force_tolerance: f64,
    /// Absolute continuity error tolerance.
    pub mass_balance_tolerance: f64,
    /// Number of final force samples used for stabilization.
    pub force_window: usize,
    /// Time limit for each external utility, in seconds.
    pub timeout_seconds: u64,
    /// Optional write interval for restart and provenance files.
    pub write_interval: u32,
    /// Convection discretization used for the final SIMPLE stage.
    pub convection_scheme: ConvectionScheme,
    /// Number of bounded-upwind startup iterations before switching to the
    /// selected final scheme.  Zero disables the warm-up stage.
    pub startup_iterations: u32,
    /// Pressure-field under-relaxation factor.
    ///
    /// The generated case runs the SIMPLEC (`consistent yes`) pressure-velocity
    /// loop, whose correction is already scaled for the neighbour coefficients.
    /// A factor of `1.0` therefore applies the correction as derived and is the
    /// fastest setting, while smaller values damp it.  Relaxation selects the
    /// path to the fixed point; it does not change the discrete equations the
    /// residual gate measures, so a converged solution is the same solution at
    /// any stable factor.  The default is measured, not assumed: see
    /// `.agent/opus-cfd-20260916` for the relaxation probe.
    #[serde(default = "default_pressure_relaxation")]
    pub pressure_relaxation: f64,
    /// Under-relaxation factor applied to the momentum equations.
    /// See [`Self::pressure_relaxation`].
    #[serde(default = "default_equation_relaxation")]
    pub equation_relaxation: f64,
    /// Optional separate under-relaxation factor for `k` and `omega`.
    ///
    /// `None` — the default — gives the turbulence pair
    /// [`Self::equation_relaxation`], which is exactly the shipped behaviour,
    /// so no existing configuration changes meaning.  It is separable because
    /// the turbulence pair and the momentum pair do not share a stability
    /// limit: `omega` spans many orders of magnitude between the wall and the
    /// free stream, and its outer residual is the one observed to limit-cycle
    /// while the momentum residuals are already four orders below tolerance.
    #[serde(default)]
    pub turbulence_relaxation: Option<f64>,
    /// Number of non-orthogonal pressure correctors inside one outer SIMPLE
    /// iteration.
    ///
    /// The non-orthogonal part of the Laplacian is an explicit deferred
    /// correction, so on a mesh with appreciable non-orthogonality a single
    /// corrector leaves a defect that the next outer iteration has to
    /// regenerate.  This is a solution-path control: the converged fields
    /// satisfy the same discrete equations for any corrector count, and the
    /// residual gate is unchanged.
    #[serde(default = "default_non_orthogonal_correctors")]
    pub non_orthogonal_correctors: u32,
    /// Blending coefficient of the non-orthogonal correction in
    /// `laplacianSchemes` and `snGradSchemes`, emitted as `limited <psi>`.
    ///
    /// `1.0` is the fully corrected (second-order) surface-normal gradient and
    /// `0.0` discards the correction entirely.  Values below one bound the
    /// explicit correction against the orthogonal part, which is a stability
    /// device that also introduces a consistent truncation error.
    #[serde(default = "default_non_orthogonal_limiter")]
    pub non_orthogonal_limiter: f64,
    /// Relative tolerance of the inner pressure solve, or `None` to take the
    /// mesh-preset default from
    /// [`Self::effective_pressure_relative_tolerance`].
    ///
    /// The inner solve stops when the residual has fallen by this factor within
    /// one outer iteration.  It bounds how deeply the pressure equation is
    /// solved per outer iteration; the outer residual gate is unaffected, and
    /// the converged fields satisfy the same discrete equations either way.
    #[serde(default)]
    pub pressure_relative_tolerance: Option<f64>,
    /// Linear solver used for the momentum and turbulence transport equations.
    #[serde(default)]
    pub momentum_linear_solver: MomentumLinearSolver,
    /// Cell-limiter coefficient of the gradient scheme, emitted as
    /// `cellLimited Gauss linear <coefficient>`.
    ///
    /// `1.0` fully limits reconstructed cell gradients so no face value exceeds
    /// the neighbouring cell range; `0.0` emits the unlimited `Gauss linear`
    /// gradient.  The limiter is solution dependent, so a limiter that switches
    /// state between outer iterations makes the fixed-point map non-smooth,
    /// which is one candidate mechanism for a residual plateau.
    #[serde(default = "default_gradient_limiter")]
    pub gradient_limiter: f64,
    /// Convection discretization used for `k` and `omega` in the final stage.
    #[serde(default)]
    pub turbulence_convection_scheme: TurbulenceConvectionScheme,
    /// Convergence tolerance of the implicit `nutUSpaldingWallFunction` solve.
    ///
    /// The Spalding wall function inverts a transcendental law-of-the-wall
    /// relation by Newton iteration at every wall face, every outer iteration.
    /// OpenFOAM's shipped default stops at a **relative error of 1e-2**, so the
    /// wall eddy viscosity carries roughly one per cent of iteration-to-iteration
    /// noise, which propagates into `k` and `omega` through the near-wall
    /// production and diffusion terms.  Tightening it solves the same wall
    /// relation more accurately; it changes no model coefficient and no
    /// equation.
    #[serde(default = "default_wall_function_tolerance")]
    pub wall_function_tolerance: f64,
    /// Iteration cap of the same wall-function solve.  OpenFOAM's default is
    /// 10, which is not enough to reach a tight tolerance.
    #[serde(default = "default_wall_function_max_iterations")]
    pub wall_function_max_iterations: u32,
}

fn default_pressure_relaxation() -> f64 {
    0.3
}

fn default_equation_relaxation() -> f64 {
    0.7
}

fn default_non_orthogonal_correctors() -> u32 {
    1
}

fn default_non_orthogonal_limiter() -> f64 {
    0.5
}

/// Inner pressure relative tolerance for a mesh preset, when the study does not
/// set one.
///
/// **Measured at all three shipped presets, not interpolated.** The same case
/// (`n0012`, `alpha = 4.04 deg`, `M = 0.15`, `Re = 6e6`, shipped defaults
/// otherwise) was solved at `0.05` and at `0.01` on each preset.  The figure of
/// merit is the solver's own total inner GAMG iteration count on `p`, which
/// machine contention cannot distort, alongside whether the outer loop reached
/// `residualControl` at all:
///
/// | preset | cells | outer it `0.05` | outer it `0.01` | inner `p` `0.05` | inner `p` `0.01` |
/// |---|---:|---:|---:|---:|---:|
/// | coarse | 82 973 | 878, self-stopped | 874, self-stopped | 5 922 | 9 144 (+54 %) |
/// | medium | 182 931 | 1130, self-stopped | 1133, self-stopped | 6 432 | 12 028 (+87 %) |
/// | fine | 436 389 | **6000, never self-stopped** | **1781, self-stopped** | 24 842 | **14 703 (-41 %)** |
///
/// Coarse and medium already reach the stopping rule at `0.05`, so a tighter
/// inner solve buys the same outer count for half again to twice the inner
/// work.  The fine preset *cannot* reach it at `0.05` — its outer pressure
/// residual floors just under the acceptance gate because each outer iteration
/// leaves the pressure field further from its own solution as the cell count
/// grows — and at `0.01` it converges in 30 % of the iterations, ending at
/// `4.994e-6` instead of `9.971e-6` against the unchanged `1e-5` gate.
///
/// This is a linear-solver stopping rule and cannot move the answer; measured,
/// it does not.  Coarse Cd/Cl agree to `0.04 %` / `0.005 %` between the two
/// settings, medium to `0.044 %` / `0.027 %`, fine to `0.019 %` / `0.002 %`,
/// the last across two different mesh instances.
///
/// The value is per preset because the measurement is per preset.  There are
/// exactly three shipped presets and all three were run; nothing here is
/// extrapolated, and a study that sets
/// [`SolverSettings::pressure_relative_tolerance`] explicitly is never
/// overridden.
pub fn default_pressure_relative_tolerance(preset: MeshPreset) -> f64 {
    match preset {
        MeshPreset::Coarse | MeshPreset::Medium => 0.05,
        MeshPreset::Fine => 0.01,
    }
}

/// Default cell-limiter coefficient of the gradient scheme.
///
/// Zero — the unlimited `Gauss linear` gradient.  The shipped `1.0` was
/// measured to be the dominant destabiliser of this template: with it, the
/// default case DIVERGED at outer iteration 621 on one mesh instance and left
/// the pressure residual pinned at 1.8e-4 .. 2.6e-4 on every other setting
/// tried, while removing it alone dropped that residual by a factor of twenty.
/// Evidence: `.agent/opus-cfd-convergence-20260916`, cases `S0`..`S6` and
/// `V4-inletoutlet-celllimited`.
fn default_gradient_limiter() -> f64 {
    0.0
}

fn default_wall_function_tolerance() -> f64 {
    1.0e-2
}

fn default_wall_function_max_iterations() -> u32 {
    10
}

/// Linear solver family for the momentum and turbulence equations.
///
/// Both solve the same discrete equations to the same tolerance; they differ
/// only in robustness and cost per outer iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MomentumLinearSolver {
    /// Symmetric Gauss-Seidel smoother, the qualified template default.
    #[default]
    SmoothSolver,
    /// Preconditioned bi-conjugate gradient stabilized with a DILU
    /// preconditioner, which tolerates stiffer systems than a smoother.
    #[serde(rename = "pbicgstab")]
    PBiCgStab,
}

impl MomentumLinearSolver {
    /// Stable label used in lifecycle messages and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SmoothSolver => "smoothSolver/symGaussSeidel",
            Self::PBiCgStab => "PBiCGStab/DILU",
        }
    }

    pub(crate) fn fv_solution_entry(self) -> &'static str {
        match self {
            Self::SmoothSolver => "solver smoothSolver; smoother symGaussSeidel;",
            Self::PBiCgStab => "solver PBiCGStab; preconditioner DILU;",
        }
    }
}

/// Convection scheme applied to the turbulence transport equations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurbulenceConvectionScheme {
    /// Bounded limited-linear. Second order where the solution is smooth, with
    /// a solution-dependent limiter.  This was the template default until it
    /// was measured to leave `k` at 2.0e-5, twice the gate, indefinitely
    /// (`V3-inletoutlet-limitedlinear`, flat from iteration 1000 to 1746), and
    /// to let the turbulence pair run away and stop being solved at all in
    /// three other configurations.
    LimitedLinear,
    /// Bounded first-order upwind, the default.  More diffusive on `k` and
    /// `omega`, and the only turbulence convection tested here that is stable
    /// in every configuration and reaches the residual gate.
    #[default]
    Upwind,
    /// Bounded linear upwind with the reconstructed scalar gradient. Second
    /// order like `limitedLinear`, but its correction comes from the gradient
    /// scheme rather than from a flux limiter, so it carries no switching
    /// function of its own.
    LinearUpwind,
}

impl TurbulenceConvectionScheme {
    /// Stable label used in lifecycle messages and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LimitedLinear => "bounded Gauss limitedLinear 1",
            Self::Upwind => "bounded Gauss upwind",
            Self::LinearUpwind => "bounded Gauss linearUpwind",
        }
    }

    /// Scheme text for one transported scalar, which `linearUpwind` needs
    /// because it names the gradient it reconstructs from.
    pub(crate) fn scheme_for(self, field: &str) -> String {
        match self {
            Self::LinearUpwind => format!("bounded Gauss linearUpwind grad({field})"),
            other => other.as_str().to_owned(),
        }
    }
}

/// Bounded convection choices supported by the versioned incompressible case.
///
/// The linear-upwind option is second order in smooth regions and is the
/// engineering default.  A short bounded-upwind warm-up is retained as a
/// separate, auditable stage because it is substantially more robust from a
/// potential-flow initial state on coarse or highly stretched meshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvectionScheme {
    /// First-order bounded Gauss upwind.
    BoundedUpwind,
    /// Bounded Gauss linear upwind with the cell gradient.
    BoundedLinearUpwind,
}

impl Default for ConvectionScheme {
    fn default() -> Self {
        Self::BoundedLinearUpwind
    }
}

impl ConvectionScheme {
    /// Stable label used in lifecycle messages and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BoundedUpwind => "bounded Gauss upwind",
            Self::BoundedLinearUpwind => "bounded Gauss linearUpwind grad(U)",
        }
    }
}

impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            max_iterations: 2_000,
            residual_tolerance: 1.0e-5,
            force_tolerance: 0.01,
            mass_balance_tolerance: 1.0e-5,
            force_window: 20,
            timeout_seconds: 1_800,
            write_interval: 100,
            convection_scheme: ConvectionScheme::default(),
            startup_iterations: 100,
            pressure_relaxation: default_pressure_relaxation(),
            equation_relaxation: default_equation_relaxation(),
            turbulence_relaxation: None,
            non_orthogonal_correctors: default_non_orthogonal_correctors(),
            non_orthogonal_limiter: default_non_orthogonal_limiter(),
            pressure_relative_tolerance: None,
            momentum_linear_solver: MomentumLinearSolver::default(),
            gradient_limiter: default_gradient_limiter(),
            turbulence_convection_scheme: TurbulenceConvectionScheme::default(),
            wall_function_tolerance: default_wall_function_tolerance(),
            wall_function_max_iterations: default_wall_function_max_iterations(),
        }
    }
}

/// Complete user-facing study configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CfdStudyConfig {
    /// Selected database key. The lookup is exact for the persisted snapshot.
    pub airfoil_name: String,
    /// Reference chord in metres.
    pub chord_m: f64,
    /// Geometric angle of attack in degrees. Positive velocity normal is +y.
    pub angle_of_attack_deg: f64,
    /// Whether speed or Reynolds number is the independent input.
    pub operating_input: OperatingInput,
    /// Freestream speed in m/s (used directly in `Speed` mode).
    pub speed_m_s: f64,
    /// Reynolds number based on chord (used directly in `Reynolds` mode).
    pub reynolds: f64,
    /// Freestream density in kg/m^3.
    pub density_kg_m3: f64,
    /// Dynamic viscosity in Pa s.
    pub dynamic_viscosity_pa_s: f64,
    /// Static freestream temperature in K.  This calculates Mach as
    /// `U / sqrt(gamma R T)` and initializes the compressible perfect-gas
    /// temperature field when the Mach policy selects `rhoSimpleFoam`.
    #[serde(default = "default_freestream_temperature_k")]
    pub freestream_temperature_k: f64,
    /// Turbulence intensity as a fraction, e.g. 0.01 for 1%.
    pub turbulence_intensity: f64,
    /// Input form used to derive the freestream omega field.
    pub turbulence_specification: TurbulenceSpecification,
    /// Turbulent length scale in metres.
    pub turbulence_length_m: f64,
    /// Turbulent-to-molecular viscosity ratio used when the viscosity-ratio
    /// specification is selected.
    #[serde(default = "default_turbulence_viscosity_ratio")]
    pub turbulence_viscosity_ratio: f64,
    /// Selected near-wall treatment. The initial template supports SST wall
    /// functions with an explicit y+ target; low-Re and transition models are
    /// rejected until a validated template is added.
    pub turbulence_model: String,
    /// Boundary condition choices.
    pub boundaries: BoundarySettings,
    /// Mesh controls.
    pub mesh: MeshSettings,
    /// Solver controls.
    pub solver: SolverSettings,
}

impl Default for CfdStudyConfig {
    fn default() -> Self {
        Self {
            airfoil_name: "SC2-0714".to_owned(),
            chord_m: 1.0,
            angle_of_attack_deg: 2.0,
            operating_input: OperatingInput::Speed,
            speed_m_s: 51.0,
            reynolds: 3.5e6,
            density_kg_m3: 1.225,
            dynamic_viscosity_pa_s: 1.81e-5,
            freestream_temperature_k: default_freestream_temperature_k(),
            // NASA/TMBWG turbulent NACA validation cases use 0.052% free
            // stream intensity and a 0.009 turbulent/molecular viscosity
            // ratio.  Keep these values explicit rather than implying that a
            // generic 1%/7%-chord inflow is universally valid.
            turbulence_intensity: 0.00052,
            turbulence_specification: TurbulenceSpecification::ViscosityRatio,
            turbulence_length_m: 0.07,
            turbulence_viscosity_ratio: 0.009,
            turbulence_model: "kOmegaSST".to_owned(),
            boundaries: BoundarySettings::default(),
            mesh: MeshSettings::default(),
            solver: SolverSettings::default(),
        }
    }
}

impl CfdStudyConfig {
    /// Derived freestream speed in m/s, respecting the selected independent
    /// input. This is the only speed used in generated dictionaries.
    pub fn effective_speed_m_s(&self) -> f64 {
        match self.operating_input {
            OperatingInput::Speed => self.speed_m_s,
            OperatingInput::Reynolds => {
                self.reynolds * self.dynamic_viscosity_pa_s / (self.density_kg_m3 * self.chord_m)
            }
        }
    }

    /// Derived Reynolds number based on chord.
    pub fn effective_reynolds(&self) -> f64 {
        match self.operating_input {
            OperatingInput::Speed => {
                self.density_kg_m3 * self.speed_m_s * self.chord_m / self.dynamic_viscosity_pa_s
            }
            OperatingInput::Reynolds => self.reynolds,
        }
    }

    /// Dry-air speed of sound in m/s for the explicitly declared static
    /// freestream temperature.  The same gamma and gas constant are written
    /// into the generated perfect-gas state when the Mach policy selects
    /// `rhoSimpleFoam`.
    pub fn speed_of_sound_m_s(&self) -> f64 {
        (DRY_AIR_GAMMA * DRY_AIR_GAS_CONSTANT_J_KG_K * self.freestream_temperature_k).sqrt()
    }

    /// Freestream Mach number `M = U/a`, where
    /// `a = sqrt(gamma R T)` for dry air.  It is deliberately not called
    /// "approximate": the stated thermodynamic convention makes this
    /// quantity reproducible for either the constant-density or perfect-gas
    /// solution path.
    pub fn mach_number(&self) -> f64 {
        self.effective_speed_m_s() / self.speed_of_sound_m_s()
    }

    /// Regime selected from the actual effective speed and declared static
    /// temperature.
    pub fn flow_regime(&self) -> FlowRegime {
        FlowRegime::from_mach(self.mach_number())
    }

    /// OpenFOAM executable selected from [`Self::flow_regime`].
    pub fn solver_kind(&self) -> CfdSolverKind {
        if self.flow_regime().is_compressible() {
            CfdSolverKind::RhoSimpleFoam
        } else {
            CfdSolverKind::SimpleFoam
        }
    }

    /// Static pressure used to initialise the compressible perfect-gas state.
    ///
    /// The legacy incompressible form allowed a zero gauge pressure reference.
    /// A perfect-gas case needs a positive absolute pressure, so zero means
    /// "derive atmospheric-level pressure from the entered density and
    /// temperature".  A positive explicit pressure remains authoritative.
    pub fn effective_static_pressure_pa(&self) -> f64 {
        if self.boundaries.pressure_reference_pa.is_finite()
            && self.boundaries.pressure_reference_pa > 0.0
        {
            self.boundaries.pressure_reference_pa
        } else {
            self.density_kg_m3 * DRY_AIR_GAS_CONSTANT_J_KG_K * self.freestream_temperature_k
        }
    }

    /// Resolve all solver controls that the case writer will emit.
    ///
    /// The low-subsonic path preserves the editable settings.  Once density
    /// variation is relevant, the policy deliberately selects a compressible
    /// solver, bounded shock-safe convection, a limited gradient, a robust
    /// linear solver and damped SIMPLE relaxation.  These controls change the
    /// numerical path and model equations as required by the regime; they are
    /// not a cosmetic relaxation of the old Mach validation.
    pub fn effective_simulation(&self) -> EffectiveSimulationSettings {
        let regime = self.flow_regime();
        let compressible = regime.is_compressible();
        let solver = if compressible {
            CfdSolverKind::RhoSimpleFoam
        } else {
            CfdSolverKind::SimpleFoam
        };
        let mut settings = self.solver.clone();
        if compressible {
            let minimum_iterations = match regime {
                FlowRegime::CompressibleSubsonic => 2_500,
                FlowRegime::Transonic => 3_000,
                FlowRegime::Supersonic => 3_500,
                FlowRegime::LowSubsonic => 0,
            };
            settings.max_iterations = settings.max_iterations.max(minimum_iterations);
            let startup = match regime {
                FlowRegime::CompressibleSubsonic => 100,
                FlowRegime::Transonic => 150,
                FlowRegime::Supersonic => 200,
                FlowRegime::LowSubsonic => 0,
            };
            settings.startup_iterations = startup.min(settings.max_iterations.saturating_sub(1));
            settings.convection_scheme = ConvectionScheme::BoundedLinearUpwind;
            settings.turbulence_convection_scheme = TurbulenceConvectionScheme::Upwind;
            settings.gradient_limiter = 1.0;
            settings.non_orthogonal_limiter = 0.5;
            settings.non_orthogonal_correctors = 1;
            settings.pressure_relaxation = 0.3;
            settings.equation_relaxation = match regime {
                FlowRegime::CompressibleSubsonic => 0.5,
                FlowRegime::Transonic => 0.3,
                FlowRegime::Supersonic => 0.2,
                FlowRegime::LowSubsonic => settings.equation_relaxation,
            };
            settings.turbulence_relaxation = Some(0.7);
            settings.pressure_relative_tolerance = Some(0.01);
            settings.momentum_linear_solver = MomentumLinearSolver::PBiCgStab;
        }
        EffectiveSimulationSettings {
            regime,
            solver,
            compressible,
            max_iterations: settings.max_iterations,
            startup_iterations: settings.startup_iterations,
            convection_scheme: settings.convection_scheme,
            turbulence_convection_scheme: settings.turbulence_convection_scheme,
            gradient_limiter: settings.gradient_limiter,
            non_orthogonal_limiter: settings.non_orthogonal_limiter,
            non_orthogonal_correctors: settings.non_orthogonal_correctors,
            pressure_relaxation: settings.pressure_relaxation,
            equation_relaxation: settings.equation_relaxation,
            turbulence_relaxation: settings
                .turbulence_relaxation
                .unwrap_or(settings.equation_relaxation),
            pressure_relative_tolerance: settings
                .effective_pressure_relative_tolerance(self.mesh.preset),
            write_interval: settings.write_interval.max(1),
        }
    }

    /// Clone the persisted study with the effective regime settings applied.
    /// This is used for result classification and provenance so the settings
    /// shown after a run are the ones the solver actually received.
    pub fn with_effective_simulation(&self) -> Self {
        let effective = self.effective_simulation();
        let mut resolved = self.clone();
        resolved.solver.max_iterations = effective.max_iterations;
        resolved.solver.startup_iterations = effective.startup_iterations;
        resolved.solver.convection_scheme = effective.convection_scheme;
        resolved.solver.turbulence_convection_scheme = effective.turbulence_convection_scheme;
        resolved.solver.gradient_limiter = effective.gradient_limiter;
        resolved.solver.non_orthogonal_limiter = effective.non_orthogonal_limiter;
        resolved.solver.non_orthogonal_correctors = effective.non_orthogonal_correctors;
        resolved.solver.pressure_relaxation = effective.pressure_relaxation;
        resolved.solver.equation_relaxation = effective.equation_relaxation;
        resolved.solver.turbulence_relaxation = Some(effective.turbulence_relaxation);
        resolved.solver.pressure_relative_tolerance = Some(effective.pressure_relative_tolerance);
        resolved.solver.momentum_linear_solver = if effective.compressible {
            MomentumLinearSolver::PBiCgStab
        } else {
            resolved.solver.momentum_linear_solver
        };
        resolved
    }

    /// Derive the complete freestream turbulence state in SI units.
    pub fn effective_turbulence(&self) -> TurbulenceState {
        let speed = self.effective_speed_m_s();
        let k = 1.5 * (self.turbulence_intensity * speed).powi(2);
        let nu = self.dynamic_viscosity_pa_s / self.density_kg_m3;
        let omega = match self.turbulence_specification {
            TurbulenceSpecification::ViscosityRatio => k / (self.turbulence_viscosity_ratio * nu),
            TurbulenceSpecification::LengthScale => {
                k.sqrt() / (0.09_f64.powf(0.25) * self.turbulence_length_m)
            }
        };
        let nu_t = k / omega;
        let nu_t_over_nu = nu_t / nu;
        let effective_length_m = k.sqrt() / (0.09_f64.powf(0.25) * omega);
        TurbulenceState {
            specification: self.turbulence_specification,
            intensity_fraction: self.turbulence_intensity,
            configured_length_m: self.turbulence_length_m,
            configured_viscosity_ratio: self.turbulence_viscosity_ratio,
            k_m2_s2: k,
            omega_s_inv: omega,
            nu_t_m2_s: nu_t,
            nu_t_over_nu,
            effective_length_m,
        }
    }

    /// Validate values before a case is created or a process is launched.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.airfoil_name.trim().is_empty() {
            errors.push("Select an airfoil from the database.".to_owned());
        }
        for (name, value) in [
            ("chord", self.chord_m),
            ("speed", self.speed_m_s),
            ("Reynolds number", self.reynolds),
            ("density", self.density_kg_m3),
            ("dynamic viscosity", self.dynamic_viscosity_pa_s),
            ("freestream temperature", self.freestream_temperature_k),
        ] {
            if !value.is_finite() || value <= 0.0 {
                errors.push(format!("{name} must be finite and greater than zero."));
            }
        }
        if !self.angle_of_attack_deg.is_finite() || self.angle_of_attack_deg.abs() > 30.0 {
            errors.push("Angle of attack must be between -30 and +30 degrees.".to_owned());
        }
        if !self.turbulence_intensity.is_finite()
            || !(1.0e-8..=0.3).contains(&self.turbulence_intensity)
        {
            errors.push("Turbulence intensity must be between 1e-8 and 30%.".to_owned());
        }
        if self.turbulence_specification == TurbulenceSpecification::ViscosityRatio
            && (!self.turbulence_viscosity_ratio.is_finite()
                || !(1.0e-6..=1.0e4).contains(&self.turbulence_viscosity_ratio))
        {
            errors.push(
                "Turbulent-to-molecular viscosity ratio must be between 1e-6 and 1e4.".to_owned(),
            );
        }
        if self.turbulence_specification == TurbulenceSpecification::LengthScale
            && (!self.turbulence_length_m.is_finite() || self.turbulence_length_m <= 0.0)
        {
            errors.push("Turbulence length must be finite and greater than zero.".to_owned());
        }
        if self.turbulence_model != "kOmegaSST" {
            errors.push(
                "Only kOmegaSST steady RANS is supported by the initial template; transition, laminar and unsteady modes are not enabled.".to_owned(),
            );
        }
        if !(1.0e3..=1.0e9).contains(&self.effective_reynolds()) {
            errors.push("The effective Reynolds number must be between 1e3 and 1e9.".to_owned());
        }
        let speed_of_sound = self.speed_of_sound_m_s();
        if !speed_of_sound.is_finite() || speed_of_sound <= 0.0 {
            errors.push(
                "Freestream temperature must produce a finite positive speed of sound.".to_owned(),
            );
        } else if self.mach_number() > MAX_SUPPORTED_MACH {
            errors.push(format!(
                "Mach {:.3} exceeds the supported airfoil CFD limit of {:.1}.",
                self.mach_number(),
                MAX_SUPPORTED_MACH
            ));
        }
        if let Err(mesh_errors) = self.mesh.validate() {
            errors.extend(mesh_errors);
        }
        if let Err(solver_errors) = self.solver.validate() {
            errors.extend(solver_errors);
        }
        if let Err(boundary_errors) = self.boundaries.validate() {
            errors.extend(boundary_errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl MeshSettings {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if !(2.0..=100.0).contains(&self.upstream_chords)
            || !(5.0..=200.0).contains(&self.downstream_chords)
            || !(2.0..=100.0).contains(&self.half_height_chords)
        {
            errors.push("Domain extents are outside the supported chord-scaled range.".to_owned());
        }
        if !self.first_layer_height_m.is_finite() || self.first_layer_height_m <= 0.0 {
            errors.push("First-layer height must be greater than zero.".to_owned());
        }
        if !(1.0..=300.0).contains(&self.target_y_plus) {
            errors.push(
                "The initial SST wall treatment accepts a target y+ from 1 to 300.".to_owned(),
            );
        }
        if self.boundary_layers && !(1..=30).contains(&self.n_layers) {
            errors.push("Boundary-layer count must be between 1 and 30.".to_owned());
        }
        if self.wake_refinement > 6 {
            errors.push("Wake refinement must be between 0 and 6.".to_owned());
        }
        if self.leading_edge_refinement > 6 {
            errors.push("Leading-edge refinement must be between 0 and 6.".to_owned());
        }
        ok_if_empty(errors)
    }
}

/// Wall-clock seconds one outer iteration costs per cell, single core.
///
/// Measured on this host (AMD Ryzen 7 5800X, OpenFOAM v2606 native Windows,
/// serial) from two certified cases in `.agent/opus-cfd-convergence-20260916`:
/// `V2-inletoutlet-medium` took `1219.443 s` of OpenFOAM `ExecutionTime` for
/// 1030 outer iterations on 183 071 cells, i.e. `6.47e-6 s` per cell-iteration,
/// while several other solves shared the machine; `V1-inletoutlet-coarse` took
/// `152.695 s` for 807 iterations on 82 975 cells, i.e. `2.28e-6 s`, with a
/// lighter load.  The larger, contended figure is the right basis for a guard,
/// rounded up to `1e-5`.
///
/// Four later cases solved **alone on the machine** put the uncontended rate
/// between `1.65e-6` and `3.34e-6 s` per cell-iteration:
///
/// | case | cells | outer iterations | `ExecutionTime` | s per cell-iteration |
/// |---|---:|---:|---:|---:|
/// | `Q1b-coarse-preltol001` | 82 945 | 874 | 119.339 s | 1.65e-6 |
/// | `Z1-shipped-defaults-coarse` | 82 973 | 878 | 190.456 s | 2.61e-6 |
/// | `P1-fine-preltol001` | 436 931 | 1781 | 1639.752 s | 2.11e-6 |
/// | `G3-fine-p404` | 436 389 | 6000 | 8756.027 s | 3.34e-6 |
///
/// So `1e-5` is roughly 3x to 6x the clean rate, and with
/// [`SOLVER_TIMEOUT_SAFETY_FACTOR`] the derived timeout is an order of
/// magnitude above the expected runtime.  That is deliberate and is kept: this
/// is a guard against a hung solve, not a schedule, the batch workflow really
/// does run contended, and the derived value can only ever *raise*
/// [`SolverSettings::timeout_seconds`], never lower it.
const SOLVER_SECONDS_PER_CELL_ITERATION: f64 = 1.0e-5;

/// Multiplier applied to the estimate above before it becomes a timeout.
///
/// A timeout exists to stop a run that will never finish, not to bound one that
/// is doing what was asked, so it must sit well clear of the legitimate cost on
/// a machine slower than the one the constant was measured on.
const SOLVER_TIMEOUT_SAFETY_FACTOR: f64 = 3.0;

impl SolverSettings {
    /// Wall-clock budget for one **solver** invocation, in seconds.
    ///
    /// [`Self::timeout_seconds`] is a reasonable guard for the short utilities —
    /// `gmsh`, `gmshToFoam`, `checkMesh` and `potentialFoam` all finish in
    /// seconds to a minute on the shipped presets — but it is not a sane guard
    /// for the solver itself.  At the shipped `1800 s` and the shipped default
    /// `max_iterations`, a **fine**-preset case (≈434 000 cells) legitimately
    /// needs of order `2.6e4 s` on one core and is killed less than a quarter of
    /// the way in; the study is then reported as failed with "the solver
    /// exceeded its configured timeout" for a run that was doing exactly what it
    /// was asked to do.  The medium default has under 30 % margin and loses it
    /// on any slower machine or under load.
    ///
    /// The budget therefore scales with the work requested — cells times the
    /// iteration budget — and the configured value is kept as a **floor**, so a
    /// user who asks for a longer guard still gets it and nothing shrinks.
    /// Returns the configured value unchanged when the cell count is not known
    /// yet, which is the case for every stage before `checkMesh`.
    /// Inner pressure relative tolerance actually emitted for `preset`.
    ///
    /// An explicit [`Self::pressure_relative_tolerance`] always wins; `None`
    /// takes the measured per-preset value from
    /// [`default_pressure_relative_tolerance`], which documents the three runs
    /// it comes from.
    pub fn effective_pressure_relative_tolerance(&self, preset: MeshPreset) -> f64 {
        self.pressure_relative_tolerance
            .unwrap_or_else(|| default_pressure_relative_tolerance(preset))
    }

    /// The solver timeout actually applied, in seconds.
    ///
    /// With no usable cell count the configured [`Self::timeout_seconds`] is
    /// returned unchanged. With one, the estimate is
    /// `cells * max_iterations * SOLVER_SECONDS_PER_CELL_ITERATION *
    /// SOLVER_TIMEOUT_SAFETY_FACTOR`, clamped to 24 h because the adapter
    /// clamps there too, and the configured value is a floor rather than a
    /// ceiling: a bigger mesh may raise the timeout, never lower it.
    pub fn solver_timeout_seconds(&self, cells: Option<u64>) -> u64 {
        let Some(cells) = cells.filter(|cells| *cells > 0) else {
            return self.timeout_seconds;
        };
        let estimate = cells as f64
            * f64::from(self.max_iterations)
            * SOLVER_SECONDS_PER_CELL_ITERATION
            * SOLVER_TIMEOUT_SAFETY_FACTOR;
        // The adapter clamps to 24 h in any case; clamp here too so the number
        // this function reports is the number that is actually applied.
        let derived = estimate.clamp(0.0, 86_400.0) as u64;
        derived.max(self.timeout_seconds)
    }

    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if !(10..=100_000).contains(&self.max_iterations) {
            errors.push("Maximum iterations must be between 10 and 100000.".to_owned());
        }
        if !(1.0e-12..=1.0e-1).contains(&self.residual_tolerance) {
            errors.push("Residual tolerance must be between 1e-12 and 1e-1.".to_owned());
        }
        if !(1.0e-5..=1.0).contains(&self.force_tolerance) {
            errors.push("Force stabilization tolerance must be between 1e-5 and 1.".to_owned());
        }
        if !(1.0e-10..=1.0).contains(&self.mass_balance_tolerance) {
            errors.push("Mass-balance tolerance must be between 1e-10 and 1.".to_owned());
        }
        if !(3..=500).contains(&self.force_window) {
            errors.push("Force-history window must be between 3 and 500 samples.".to_owned());
        }
        if !(1..=86_400).contains(&self.timeout_seconds) {
            errors.push("Solver timeout must be between 1 second and 24 hours.".to_owned());
        }
        for (name, value) in [
            ("Pressure relaxation", self.pressure_relaxation),
            ("Equation relaxation", self.equation_relaxation),
        ] {
            if !value.is_finite() || !(0.01..=1.0).contains(&value) {
                errors.push(format!("{name} must be between 0.01 and 1."));
            }
        }
        if let Some(value) = self.turbulence_relaxation {
            if !value.is_finite() || !(0.01..=1.0).contains(&value) {
                errors.push("Turbulence relaxation must be between 0.01 and 1.".to_owned());
            }
        }
        if self.non_orthogonal_correctors > 10 {
            errors.push("Non-orthogonal correctors must be between 0 and 10.".to_owned());
        }
        for (name, value) in [
            ("Non-orthogonal limiter", self.non_orthogonal_limiter),
            ("Gradient limiter", self.gradient_limiter),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                errors.push(format!("{name} must be between 0 and 1."));
            }
        }
        if self
            .pressure_relative_tolerance
            .is_some_and(|value| !value.is_finite() || !(0.0..=0.5).contains(&value))
        {
            errors.push("Pressure relative tolerance must be between 0 and 0.5.".to_owned());
        }
        if !self.wall_function_tolerance.is_finite()
            || !(1.0e-12..=1.0e-1).contains(&self.wall_function_tolerance)
        {
            errors.push("Wall-function tolerance must be between 1e-12 and 1e-1.".to_owned());
        }
        if !(1..=1_000).contains(&self.wall_function_max_iterations) {
            errors.push("Wall-function iteration cap must be between 1 and 1000.".to_owned());
        }
        if self.startup_iterations > self.max_iterations {
            errors.push("Upwind startup iterations cannot exceed maximum iterations.".to_owned());
        }
        if self.convection_scheme != ConvectionScheme::BoundedUpwind
            && self.startup_iterations == self.max_iterations
            && self.startup_iterations > 0
        {
            errors.push(
                "A non-upwind final scheme requires at least one final-stage iteration after upwind startup."
                    .to_owned(),
            );
        }
        ok_if_empty(errors)
    }
}

impl BoundarySettings {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.inlet_patch != "inlet"
            || self.outlet_patch != "outlet"
            || self.airfoil_patch != "airfoil"
        {
            errors.push(
                "The initial versioned template requires inlet, outlet and airfoil patch names."
                    .to_owned(),
            );
        }
        if !self.pressure_reference_pa.is_finite() {
            errors.push("Pressure reference must be finite.".to_owned());
        }
        ok_if_empty(errors)
    }
}

fn ok_if_empty(errors: Vec<String>) -> Result<(), Vec<String>> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
