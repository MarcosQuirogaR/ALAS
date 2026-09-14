// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User-facing, SI-valued OpenFOAM study configuration.

use serde::{Deserialize, Serialize};

use super::boundary::BoundarySettings;
use super::turbulence::{TurbulenceSpecification, TurbulenceState};

fn default_turbulence_viscosity_ratio() -> f64 {
    0.009
}

/// Source used to determine the dimensional velocity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingInput {
    /// The user enters speed; Reynolds number is derived.
    Speed,
    /// The user enters Reynolds number; speed is derived.
    Reynolds,
}

impl Default for OperatingInput {
    fn default() -> Self {
        Self::Speed
    }
}

/// Repeatable mesh density preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshPreset {
    /// Fast preflight with a coarse outer block and surface refinement.
    Coarse,
    /// Default engineering setup.
    Medium,
    /// Higher resolution for sensitivity checks.
    Fine,
}

impl Default for MeshPreset {
    fn default() -> Self {
        Self::Medium
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FarFieldCondition {
    /// A fixed velocity and zero-gradient pressure condition.
    FixedValue,
    /// OpenFOAM's `freestream` mixed condition.
    Freestream,
}

impl Default for FarFieldCondition {
    fn default() -> Self {
        Self::FixedValue
    }
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
    /// Target first-cell wall distance in metres. The generated template
    /// records this target; a quality gate reports achieved y+ when available.
    pub first_layer_height_m: f64,
    /// Desired y+ for the selected wall treatment.
    pub target_y_plus: f64,
    /// Whether layer controls are emitted for a later validated layer study.
    pub boundary_layers: bool,
    /// Number of prism layers when boundary layers are enabled.
    pub n_layers: u32,
    /// Surface refinement multiplier for the wake region.
    pub wake_refinement: u32,
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
            boundary_layers: true,
            n_layers: 25,
            wake_refinement: 2,
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

    /// Freestream Mach number using a fixed sea-level sound speed for the
    /// incompressible validity warning; no compressible thermodynamics are
    /// silently inferred by the template.
    pub fn approximate_mach(&self) -> f64 {
        self.effective_speed_m_s() / 340.294
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
                "Only kOmegaSST steady RANS is supported by the initial template; transition, laminar, compressible and unsteady modes are not enabled.".to_owned(),
            );
        }
        if !(1.0e3..=1.0e9).contains(&self.effective_reynolds()) {
            errors.push("The effective Reynolds number must be between 1e3 and 1e9.".to_owned());
        }
        if self.approximate_mach() > 0.3 {
            errors.push(
                "The incompressible template is limited to approximately Mach 0.3; reduce speed or use a validated compressible study.".to_owned(),
            );
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
        ok_if_empty(errors)
    }
}

impl SolverSettings {
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
