// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// generate_vortex_distribution.py and VLM.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The inputs and outputs of one VORLAX solve.
//!
//! Upstream these are untyped `Data()` bags with about ninety keys between
//! them, half of which are `np.empty` placeholders for branches this program
//! never reaches. What survives here is what the reached path reads, named
//! the same so that a reader can hold the two side by side -- including the
//! fully-capitalized names, which upstream's own docstring says mark a
//! variable it took from VORLAX itself.
//!
//! # Why the coordinate arrays are `f32`
//!
//! `generate_vortex_distribution` computes every panel corner in double
//! precision and then stores it through `np.array(x, dtype=precision)`, where
//! `precision` is `settings.floating_point_precision` and `Vortex_Lattice`
//! sets that to `np.float32`. So the panelization is double-precision
//! arithmetic rounded once, and every later stage reads the rounded value.
//! Reproducing that is not optional: the influence kernel divides differences
//! of these coordinates by other differences of them, and a panel corner that
//! is right to seventeen digits instead of seven moves the answer by more
//! than this row's tier allows. The rounding points are exactly where
//! upstream's `dtype=` appears, and nowhere else.

/// The `Vortex_Lattice` settings the reached path reads.
///
/// Five of these are copied down from `Fidelity_Zero.settings` by its
/// `initialize`, and the rest come from `Vortex_Lattice.__defaults__`. The
/// three that are `false` here are what make this row's scope what it is, so
/// they are fields rather than assumptions: a caller that sets one gets a
/// documented error rather than a silently wrong answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VlmSettings {
    /// Spanwise strips per surface. `Fidelity_Zero` leaves its own override
    /// unset, so this stays at `Vortex_Lattice`'s 15.
    pub number_spanwise_vortices: usize,
    /// Chordwise panels per strip. `Vortex_Lattice`'s default 5.
    pub number_chordwise_vortices: usize,
    /// Cosine rather than linear spanwise spacing. Default `true`.
    pub spanwise_cosine_spacing: bool,
    /// VORLAX's `K_SPC`. Default 1.0; a negative value selects Lan's
    /// leading-edge correction, which nothing here reaches.
    pub leading_edge_suction_multiplier: f64,
}

impl Default for VlmSettings {
    fn default() -> Self {
        Self {
            number_spanwise_vortices: 15,
            number_chordwise_vortices: 5,
            spanwise_cosine_spacing: true,
            leading_edge_suction_multiplier: 1.0,
        }
    }
}

/// One lifting surface, as `make_VLM_wings` leaves it.
///
/// Upstream copies eighteen attribute paths off a `Wing` into a `Data()` bag
/// and then rewrites four of them. This is that bag, minus the control
/// surface fields: with `discretize_control_surfaces` false, a wing's control
/// surfaces never become wings of their own and nothing downstream reads
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmWing {
    /// The wing's name. Two surfaces are emitted per symmetric wing and both
    /// carry this tag, which is how `calculate_VLM` regroups them.
    pub tag: String,
    /// Mirrored about the plane the wing is not in. Doubles the surface count.
    pub symmetric: bool,
    /// Laid out in the x-z plane rather than x-y, and reflected accordingly.
    pub vertical: bool,
    /// Enables VORLAX's vortex-lift correction below Mach 1. False on every
    /// surface the runner builds.
    pub vortex_lift: bool,
    /// Projected span. Halved for a symmetric wing before panelization.
    pub span_projected_m: f64,
    /// Chord at the root section.
    pub chord_root_m: f64,
    /// Chord at the tip section.
    pub chord_tip_m: f64,
    /// Tip over root chord.
    pub taper: f64,
    /// Span squared over reference area, used by the sweep conversion.
    pub aspect_ratio: f64,
    /// Sweep of the quarter-chord line.
    pub sweep_quarter_chord_rad: f64,
    /// Sweep of the leading edge, when the caller set one. `None` is the
    /// normal case and makes the root segment derive it from the quarter
    /// chord.
    pub sweep_leading_edge_rad: Option<f64>,
    /// Incidence at the root section.
    pub twist_root_rad: f64,
    /// Incidence at the tip section.
    pub twist_tip_rad: f64,
    /// Dihedral, applied outboard of the root segment.
    pub dihedral_rad: f64,
    /// Reference area, which the per-wing coefficients are divided by.
    pub area_reference_m2: f64,
    /// Position of the root leading edge in aircraft coordinates.
    pub origin_m: [f64; 3],
}

/// The vehicle a solve runs on.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmGeometry {
    /// The vehicle reference area every total coefficient is divided by.
    pub reference_area_m2: f64,
    /// Where the moments are taken, when it is not the origin. VORLAX falls
    /// back to the main wing's aerodynamic centre when the x component is
    /// exactly zero, which is the branch every vehicle this program builds
    /// takes.
    pub center_of_gravity_m: [f64; 3],
    /// The moment reference length: the main wing's mean aerodynamic chord.
    pub mean_aerodynamic_chord_m: f64,
    /// The roll and yaw scaling length: the main wing's projected span.
    pub reference_span_m: f64,
    /// The point the moments are taken about, x and z. Supplied rather than
    /// derived because the fallback reads a wing's `aerodynamic_center`,
    /// which is `alas-geom::aircraft::wing`'s concern and not this row's.
    pub moment_reference_m: [f64; 2],
    /// The lifting surfaces, in the order the vehicle holds them.
    pub wings: Vec<VlmWing>,
}

/// The flight conditions of one row of a solve.
///
/// A solve takes many of these at once, because the influence matrix depends
/// only on the Mach number and upstream builds one per *unique* Mach and
/// reuses it. Here that reuse is explicit rather than an array-indexing
/// trick, but the arithmetic is the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VlmCondition {
    /// Angle of attack.
    pub angle_of_attack_rad: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Sideslip angle, VORLAX's `PSI`.
    pub side_slip_angle_rad: f64,
    /// Body-axis pitch rate, VORLAX's `PITCHQ`.
    pub pitch_rate_rad_s: f64,
    /// Body-axis roll rate, VORLAX's `ROLLQ`.
    pub roll_rate_rad_s: f64,
    /// Body-axis yaw rate, VORLAX's `YAWQ`.
    pub yaw_rate_rad_s: f64,
    /// True airspeed. The rotation rates are divided by it, so a zero here is
    /// not usable; [`crate::vorlax::run`] substitutes 1e-6, as upstream does
    /// under `use_surrogate`.
    pub velocity_m_s: f64,
}

/// A panel's four corners, its bound-vortex leg and its control point.
///
/// Upstream carries these as thirty-nine parallel arrays on one `Data()`.
/// Keeping them parallel here rather than making a struct per panel is
/// deliberate: every consumer is a whole-array operation, several of them
/// index by a boolean mask over all panels at once, and the influence kernel
/// wants each coordinate contiguous.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelCoordinates {
    /// Left end of the bound vortex.
    pub xah: Vec<f32>,
    /// Left end of the bound vortex.
    pub yah: Vec<f32>,
    /// Left end of the bound vortex.
    pub zah: Vec<f32>,
    /// Right end of the bound vortex.
    pub xbh: Vec<f32>,
    /// Right end of the bound vortex.
    pub ybh: Vec<f32>,
    /// Right end of the bound vortex.
    pub zbh: Vec<f32>,
    /// Midpoint of the bound vortex.
    pub xch: Vec<f32>,
    /// Midpoint of the bound vortex.
    pub ych: Vec<f32>,
    /// Midpoint of the bound vortex.
    pub zch: Vec<f32>,
    /// Top left corner of the panel.
    pub xa1: Vec<f32>,
    /// Top left corner of the panel.
    pub ya1: Vec<f32>,
    /// Top left corner of the panel.
    pub za1: Vec<f32>,
    /// Bottom left corner of the panel.
    pub xa2: Vec<f32>,
    /// Bottom left corner of the panel.
    pub ya2: Vec<f32>,
    /// Bottom left corner of the panel.
    pub za2: Vec<f32>,
    /// Top right corner of the panel.
    pub xb1: Vec<f32>,
    /// Top right corner of the panel.
    pub yb1: Vec<f32>,
    /// Top right corner of the panel.
    pub zb1: Vec<f32>,
    /// Bottom right corner of the panel.
    pub xb2: Vec<f32>,
    /// Bottom right corner of the panel.
    pub yb2: Vec<f32>,
    /// Bottom right corner of the panel.
    pub zb2: Vec<f32>,
    /// Left control-point corner, three quarters back along the panel.
    pub xac: Vec<f32>,
    /// Left control-point corner.
    pub yac: Vec<f32>,
    /// Left control-point corner.
    pub zac: Vec<f32>,
    /// Right control-point corner.
    pub xbc: Vec<f32>,
    /// Right control-point corner.
    pub ybc: Vec<f32>,
    /// Right control-point corner.
    pub zbc: Vec<f32>,
    /// The control point itself.
    pub xc: Vec<f32>,
    /// The control point itself.
    pub yc: Vec<f32>,
    /// The control point itself.
    pub zc: Vec<f32>,
    /// The strip's trailing edge, left corner, repeated onto every panel of
    /// the strip.
    pub xa_te: Vec<f32>,
    /// The strip's trailing edge, left corner.
    pub ya_te: Vec<f32>,
    /// The strip's trailing edge, left corner.
    pub za_te: Vec<f32>,
    /// The strip's trailing edge, right corner.
    pub xb_te: Vec<f32>,
    /// The strip's trailing edge, right corner.
    pub yb_te: Vec<f32>,
    /// The strip's trailing edge, right corner.
    pub zb_te: Vec<f32>,
}

/// The panelization of a whole vehicle: upstream's `VD`.
///
/// A "surface" here is one side of a wing -- a symmetric wing contributes
/// two, which is why [`Self::n_w`] is five for a three-wing aircraft with two
/// symmetric wings. A "strip" is one chordwise column of panels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VortexDistribution {
    /// Number of surfaces.
    pub n_w: usize,
    /// Number of panels, and the order of the influence matrix.
    pub n_cp: usize,
    /// Strips per surface.
    pub n_sw: Vec<usize>,
    /// Panels per strip, per surface.
    pub n_cw: Vec<usize>,
    /// Index of the first panel of each strip.
    pub chordwise_breaks: Vec<usize>,
    /// Index into [`Self::chordwise_breaks`] of the first strip of each
    /// surface.
    pub spanwise_breaks: Vec<usize>,
    /// One flag per wing (not per surface): was it mirrored.
    pub symmetric_wings: Vec<bool>,
    /// True on the first panel of each strip.
    pub leading_edge_indices: Vec<bool>,
    /// True on the last panel of each strip.
    pub trailing_edge_indices: Vec<bool>,
    /// VORLAX's `RNMAX`: the strip's panel count, written on every panel of
    /// the strip.
    pub panels_per_strip: Vec<usize>,
    /// VORLAX's `RK`: the panel's one-based position within its strip.
    pub chordwise_panel_number: Vec<usize>,
    /// 0 on a strip whose leading edge is behind a non-slat control surface,
    /// which turns its leading-edge suction off. Always 1 here, since no
    /// control surface is discretized.
    pub exposed_leading_edge_flag: Vec<i16>,
    /// Per surface: whether vortex lift is enabled on it.
    pub vortex_lift: Vec<bool>,
    /// Per surface: the planform area the surface's coefficients are formed
    /// on before being rescaled to the wing's reference area.
    pub wing_areas_m2: Vec<f32>,
    /// VORLAX's `CHORD`: the strip's streamwise length, on every panel.
    pub chord_lengths_m: Vec<f32>,
    /// VORLAX's `ZETA`: the tangent of the strip's incidence, on every panel.
    pub tangent_incidence_angle: Vec<f32>,
    /// Panel areas. Computed and stored upstream; nothing on the reached path
    /// reads them, and they are carried so the fixture can check the
    /// panelization through a quantity no coefficient depends on.
    pub panel_areas_m2: Vec<f32>,
    /// Panel unit normals, flipped so none points downward. The boundary
    /// condition is a dot product against these.
    pub normals: Vec<[f32; 3]>,
    /// VORLAX's `SLOPE`: the panel's streamwise camber slope.
    pub slope: Vec<f32>,
    /// [`Self::slope`] at the leading-edge panels only.
    pub sle: Vec<f32>,
    /// The bound vortex's projected length, per strip.
    pub d: Vec<f32>,
    /// The panel geometry itself.
    pub panels: PanelCoordinates,
}

impl VortexDistribution {
    /// Indices of the leading-edge panels, which is to say one per strip.
    pub fn leading_edge_panels(&self) -> Vec<usize> {
        (0..self.n_cp)
            .filter(|&i| self.leading_edge_indices[i])
            .collect()
    }

    /// Indices of the trailing-edge panels.
    pub fn trailing_edge_panels(&self) -> Vec<usize> {
        (0..self.n_cp)
            .filter(|&i| self.trailing_edge_indices[i])
            .collect()
    }

    /// Total number of strips across every surface.
    pub fn n_strips(&self) -> usize {
        self.chordwise_breaks.len()
    }
}

/// What one solve reports, for one flight condition.
///
/// The eight totals are VORLAX's own `_TOT` outputs and carry its names; the
/// rest are mission analysis model's additions. `gamma` and `cp` are here because a wrong
/// influence matrix can still integrate to a plausible total, so the fixture
/// compares the field and not only its integral.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmCaseResult {
    /// Total lift coefficient, VORLAX's `CLTOT`.
    pub cl: f64,
    /// Total induced drag coefficient, VORLAX's `CDTOT`.
    pub cdi: f64,
    /// Pitching moment coefficient, VORLAX's `CMTOT`.
    pub cm: f64,
    /// Side force coefficient.
    pub cytot: f64,
    /// Rolling moment coefficient, unscaled by span.
    pub crtot: f64,
    /// Rolling moment coefficient, scaled by span and negated.
    pub crmtot: f64,
    /// Yawing moment coefficient, unscaled by span.
    pub cntot: f64,
    /// Yawing moment coefficient, scaled by span and negated.
    pub cymtot: f64,
    /// Lift coefficient of each *surface*, on the surface's own area.
    pub cl_wing: Vec<f64>,
    /// Induced drag coefficient of each surface.
    pub cdi_wing: Vec<f64>,
    /// Lift coefficient of each strip.
    pub cl_y: Vec<f64>,
    /// Induced drag coefficient of each strip.
    pub cdi_y: Vec<f64>,
    /// Pressure coefficient of each panel, VORLAX's `DCP`. Single precision
    /// because upstream reports it that way: the two field quantities go
    /// through `np.array(x, dtype=precision)` on the way out of `VLM` while
    /// none of the coefficients above do.
    pub cp: Vec<f32>,
    /// Solved circulation of each panel, in single precision for the same
    /// reason. The solve itself runs in double.
    pub gamma: Vec<f32>,
}

/// The result of a whole solve: the panelization, and one entry per
/// condition.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmResults {
    /// The panelization every case was solved on. Built once; it depends on
    /// the geometry alone.
    pub distribution: VortexDistribution,
    /// One entry per input condition, in the order they were given.
    pub cases: Vec<VlmCaseResult>,
    /// Residual and pivot-ratio evidence for each unique-Mach circulation
    /// solve, in the order those Mach groups were assembled by [`run`].
    ///
    /// A single factorization serves all conditions in one Mach group, so
    /// these diagnostics are intentionally group-level rather than repeated
    /// once per case.
    pub solve_diagnostics: Vec<alas_math::linalg::SolveDiagnostics>,
}

/// Why a solve could not run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VlmError {
    /// A wing needs at least as many spanwise stations as it has section
    /// breaks. With two breaks per wing and fifteen stations this cannot
    /// happen, but upstream raises here and so does this.
    #[error("wing '{tag}' has {breaks} section breaks and only {stations} spanwise stations")]
    NotEnoughStations {
        /// The wing that could not be discretized.
        tag: String,
        /// How many breaks it has.
        breaks: usize,
        /// How many stations were available.
        stations: usize,
    },
    /// The influence matrix was singular at the named elimination step.
    #[error("the influence matrix is singular at step {step}")]
    SingularInfluenceMatrix {
        /// Which elimination step found no usable pivot.
        step: usize,
    },
    /// The vehicle has no lifting surfaces, so there is nothing to solve.
    #[error("the vehicle has no wings")]
    NoWings,
    /// The dense solve produced non-finite residual or conditioning evidence.
    #[error("the VORLAX solve produced non-finite numerical diagnostics")]
    NonFiniteNumericalDiagnostics,
    /// The translated kernel is valid only for finite subsonic Mach numbers.
    #[error("Mach {mach} is outside the translated subsonic VORLAX domain [0, 1)")]
    MachOutsideSubsonicDomain {
        /// Requested Mach number.
        mach: f64,
    },
}
