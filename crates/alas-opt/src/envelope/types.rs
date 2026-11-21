// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result and error types of the model-derived CG envelope assessment.

use super::{ModelCgConstraint, ModelCgConstraintAssessment, PhysicalCgLimits};
use alas_mass::stations::StationError;

/// Outcome of the frozen Python-compatible envelope check.
///
/// This type exists for reference parity. Product code uses
/// [`ModelCgEnvelopeAssessment`], whose typed constraints do not conflate the
/// preferred static margin with the hard stability floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgEnvelopeResult {
    /// True if any loading state falls outside its dynamic [fwd, aft] limits.
    pub violation: bool,
    /// Maximum fraction of MAC by which a CG limit was violated (0.0 if compliant).
    pub worst_exceedance: f64,
}

/// Named loading state evaluated by the model-derived assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCgLoadingState {
    /// Operating empty weight.
    OperatingEmpty,
    /// Zero-fuel load actually analyzed from OEW plus modeled payload.
    AnalyzedZeroFuel,
    /// Mid-mission load with half of the analyzed usable fuel remaining.
    OperationalMidMission,
    /// Reserve/arrival loading case with ten percent of analyzed fuel remaining.
    OperationalReserve,
    /// Takeoff load actually analyzed after any usable-fuel cap is applied.
    AnalyzedTakeoff,
    /// Landing load of the flown mission (the mass ledger's landing state),
    /// present only when the caller supplies one.
    AnalyzedLanding,
}

impl ModelCgLoadingState {
    /// Stable report label for this loading state.
    pub const fn label(self) -> &'static str {
        match self {
            Self::OperatingEmpty => "OEW",
            Self::AnalyzedZeroFuel => "analyzed ZFW",
            Self::OperationalMidMission => "operational mid-mission",
            Self::OperationalReserve => "operational reserve",
            Self::AnalyzedTakeoff => "analyzed TOW",
            Self::AnalyzedLanding => "analyzed landing",
        }
    }
}

/// Which physical CG-limit mechanisms apply to one loading state.
///
/// The ground mechanisms (maximum nose load forward; minimum nose load and
/// tip-back aft) apply to every state, because every state can stand on its
/// gear. The phase-specific mechanisms are scoped to the flight phase the
/// state represents:
/// - OEW and loading steps: ground only ([`Self::GROUND`]);
/// - zero-fuel, mid-mission and reserve: landing trim forward, static-margin
///   floor aft ([`Self::FLIGHT`]); heavier than the design maximum landing
///   mass, the static-margin floor aft alone ([`Self::EN_ROUTE_ABOVE_MLW`],
///   see [`Self::for_state_at_mass`]);
/// - takeoff: rotation forward, static-margin floor aft ([`Self::TAKEOFF`]);
/// - landing: landing trim forward, static-margin floor aft ([`Self::LANDING`]).
///
/// Rotation (nose-wheel lift-off) is a ground take-off event: it bounds the
/// take-off state alone, at that state's own mass and thrust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseLimits {
    /// The nose-wheel liftoff (rotation) forward boundary applies.
    pub rotation: bool,
    /// The landing trim/flare forward boundary applies.
    pub landing_trim: bool,
    /// The static-margin floor (aerodynamic aft boundary) applies.
    pub static_margin: bool,
    /// The clean climb-trim forward boundary (CS 25.161(c)(1), flaps
    /// retracted) applies: every airborne state.
    pub clean_trim: bool,
}

impl PhaseLimits {
    /// Ground mechanisms only.
    pub const GROUND: Self = Self {
        rotation: false,
        landing_trim: false,
        static_margin: false,
        clean_trim: false,
    };
    /// Takeoff: rotation forward and the static-margin floor aft.
    pub const TAKEOFF: Self = Self {
        rotation: true,
        landing_trim: false,
        static_margin: true,
        clean_trim: true,
    };
    /// En-route flight: landing trim forward and the static-margin floor aft.
    pub const FLIGHT: Self = Self {
        rotation: false,
        landing_trim: true,
        static_margin: true,
        clean_trim: true,
    };
    /// Landing: landing trim forward and the static-margin floor aft, since
    /// the approach is flown.
    pub const LANDING: Self = Self {
        rotation: false,
        landing_trim: true,
        static_margin: true,
        clean_trim: true,
    };
    /// En-route flight heavier than the maximum landing mass: the
    /// static-margin floor aft and no landing or take-off forward mechanism.
    /// The state cannot land (no landing trim) and is not on its take-off run
    /// (no rotation); it is airborne, so the clean climb trim applies.
    pub const EN_ROUTE_ABOVE_MLW: Self = Self {
        rotation: false,
        landing_trim: false,
        static_margin: true,
        clean_trim: true,
    };

    /// The mechanisms that apply to `state`.
    #[must_use]
    pub const fn for_state(state: ModelCgLoadingState) -> Self {
        match state {
            ModelCgLoadingState::OperatingEmpty => Self::GROUND,
            ModelCgLoadingState::AnalyzedZeroFuel
            | ModelCgLoadingState::OperationalMidMission
            | ModelCgLoadingState::OperationalReserve => Self::FLIGHT,
            ModelCgLoadingState::AnalyzedTakeoff => Self::TAKEOFF,
            ModelCgLoadingState::AnalyzedLanding => Self::LANDING,
        }
    }

    /// The limits that bound `state` at `mass_kg`, against the maximum landing
    /// mass `design_landing_mass_kg` (the declared MLW where there is one, see
    /// [`ModelCgEnvelopeAssessment::design_landing_mass_kg`], so an analysed
    /// landing state's presence moves no other state's scoping).
    ///
    /// The landing-trim boundary (Torenbeek eq. 6.2) is a landing condition.
    /// An en-route state (zero fuel, mid-mission, reserve) at or below the MLW
    /// may land and keeps it ([`Self::FLIGHT`]); one heavier than the MLW
    /// cannot land, so it drops landing trim and stays bounded by its own
    /// in-flight static-margin floor and the ground mechanisms at its own mass
    /// ([`Self::EN_ROUTE_ABOVE_MLW`]). It gains no rotation boundary: rotation
    /// is a ground take-off event and bounds the take-off state alone. The
    /// OEW, take-off and landing states keep [`Self::for_state`] at any mass;
    /// the analysed landing state is the landing flown.
    ///
    /// This scopes the limits only; which constraints a state carries is
    /// still [`Self::for_state`] (`constraints::assess_loading_constraints`),
    /// so an en-route state above the MLW keeps its forward-limit and
    /// usable-range rows, gated against this scope's forward boundary.
    #[must_use]
    pub fn for_state_at_mass(
        state: ModelCgLoadingState,
        mass_kg: f64,
        design_landing_mass_kg: f64,
    ) -> Self {
        let en_route = matches!(
            state,
            ModelCgLoadingState::AnalyzedZeroFuel
                | ModelCgLoadingState::OperationalMidMission
                | ModelCgLoadingState::OperationalReserve
        );
        if en_route && mass_kg > design_landing_mass_kg * (1.0 + 1e-9) {
            Self::EN_ROUTE_ABOVE_MLW
        } else {
            Self::for_state(state)
        }
    }

    /// Whether `constraint` is gated in a state with these mechanisms.
    ///
    /// The static-stability floor needs [`Self::static_margin`]. The
    /// physical forward limit and the minimum usable range need a flight
    /// forward mechanism: in a ground-only state the forward limit is the
    /// maximum nose load alone, which
    /// [`ModelCgConstraint::MaximumNoseGearLoadFraction`] already gates.
    /// Every ground-reaction constraint applies in every state.
    #[must_use]
    pub const fn admits(self, constraint: ModelCgConstraint) -> bool {
        match constraint {
            ModelCgConstraint::StaticStabilityFloor => self.static_margin,
            ModelCgConstraint::PhysicalForwardCgLimit | ModelCgConstraint::MinimumUsableCgRange => {
                self.rotation || self.landing_trim
            }
            ModelCgConstraint::NoseGearStrength
            | ModelCgConstraint::MainGearStrength
            | ModelCgConstraint::MinimumNoseGearLoad
            | ModelCgConstraint::MaximumNoseGearLoadFraction
            | ModelCgConstraint::TipBack
            | ModelCgConstraint::TailScrape => true,
        }
    }
}

/// Model quantities and constraint verdicts for one loading state.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCgLoadingAssessment {
    /// Loading state identifier.
    pub state: ModelCgLoadingState,
    /// The mechanisms [`Self::physical_limits`] is scoped to: this state's
    /// phase at its mass ([`PhaseLimits::for_state_at_mass`]).
    pub scope: PhaseLimits,
    /// Total mass in this state, in kilograms.
    pub mass_kg: f64,
    /// Longitudinal center of gravity from the model aircraft nose, in meters.
    pub cg_x_m: f64,
    /// CG height above the ground plane at this state, m: the height the
    /// state's tip-back limit station and angle are evaluated with.
    pub h_cg_m: f64,
    /// Center of gravity in the built aerodynamic model's MAC frame.
    pub cg_pct_mac: f64,
    /// Static margin from the common modeled neutral point.
    pub static_margin: f64,
    /// Static margin from the critical (most-forward) neutral point across the
    /// clean, high-lift and elastic conditions: the quantity
    /// [`ModelCgConstraint::StaticStabilityFloor`] gates on.
    pub critical_static_margin: f64,
    /// Nose-gear reaction divided by this loading state's total weight.
    pub nose_gear_load_fraction: f64,
    /// Whether this state's ground reactions are inside the validity domain
    /// of the two-group static split that produced them.
    ///
    /// `N_nose = m (x_mlg - x_cg) / (x_mlg - x_nlg)` and
    /// `N_main = m - N_nose` describe an aeroplane resting on both gear
    /// groups **in compression**. With the centre of gravity aft of the
    /// effective main-gear station the nose reaction comes out negative and
    /// the main reaction exceeds the whole weight: the aeroplane sits on its
    /// tail, the nose leg carries nothing, and neither number is a load the
    /// gear actually sees. When this is `false` the two strength assessments
    /// are omitted from [`Self::constraints`] rather than reported, because a
    /// negative reaction trivially satisfies a rated-capacity upper bound and
    /// would otherwise print as a passing strength margin.
    /// [`ModelCgConstraint::MinimumNoseGearLoad`] is the constraint that
    /// describes this state and is always retained.
    pub ground_reactions_admissible: bool,
    /// This state's physical aft/forward CG boundaries and which mechanism
    /// governs each: weight/`h_cg` dependent, so reported per state rather
    /// than once for the design. The governing limits are scoped to this
    /// state's phase at its mass ([`PhaseLimits::for_state_at_mass`], see
    /// [`PhysicalCgLimits::scoped`]); every mechanism's own boundary is still
    /// reported in the diagnostic fields.
    pub physical_limits: PhysicalCgLimits,
    /// Independently evaluated hard constraints.
    pub constraints: Vec<ModelCgConstraintAssessment>,
}

/// Non-governing optimizer preference evaluated at the analyzed takeoff point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticMarginPreferenceAssessment {
    /// Analyzed takeoff model static margin, as a fraction of MAC.
    pub actual: f64,
    /// Configured optimizer target, as a fraction of MAC.
    pub target: f64,
    /// Signed difference `actual - target`.
    pub deviation: f64,
}

impl StaticMarginPreferenceAssessment {
    /// Whether the analyzed takeoff point is at or above the preferred buffer.
    pub fn met_or_exceeded(self) -> bool {
        self.actual >= self.target
    }
}

/// Which physical mechanism actually bounds the aft end of the CG envelope.
///
/// A transport's aft centre-of-gravity limit is the **more forward** of two
/// independent boundaries: the aerodynamic one, where the static margin falls
/// to its floor, and the ground one, where the nose-gear reaction falls to the
/// steering-load minimum and the aeroplane approaches tipping back onto its
/// tail. [`ModelCgEnvelopeAssessment`] has only ever derived the first.
///
/// When the aerodynamic boundary lies **aft** of the ground boundary, the
/// envelope admits loading states that tip the aeroplane, and
/// [`ModelCgConstraint::MinimumNoseGearLoad`] fires as a symptom at whichever
/// state happens to land there, with nothing naming the missing boundary.
/// This enum is that name.
///
/// It is a diagnostic, not a new threshold: neither
/// [`ModelCgEnvelopeAssessment::aerodynamic_aft_limit_pct_mac`] nor
/// [`ModelCgEnvelopeAssessment::configured_forward_limit_pct_mac`] is
/// re-derived from it, so no constraint is loosened and no candidate that the
/// existing limits reject becomes feasible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AftCgLimitGovernance {
    /// No main-gear station or wheelbase was usable, so the ground boundary
    /// could not be placed and only the aerodynamic one exists.
    NotEvaluated,
    /// The aerodynamic boundary is at or forward of the ground boundary: the
    /// envelope's aft end is the one the assessment already reports.
    Aerodynamic,
    /// The ground boundary is forward of the aerodynamic one by
    /// `margin_pct_mac`, so the reported aft limit is **not** the governing
    /// one and the gap is the band of CG positions the envelope admits and
    /// the gear cannot carry.
    GroundMinimumNoseLoad {
        /// `aerodynamic_aft_limit_pct_mac - ground_aft_limit_pct_mac`, % MAC,
        /// strictly positive in this variant.
        margin_pct_mac: f64,
    },
    /// The longitudinal tip-back boundary is forward of both
    /// the aerodynamic and ground boundaries at the state this was formed
    /// from, by `margin_pct_mac` against the more forward of the other two.
    TipBack {
        /// Positive overhang, % MAC, against the more forward of the
        /// aerodynamic and ground boundaries.
        margin_pct_mac: f64,
    },
}

impl AftCgLimitGovernance {
    /// How far the reported aerodynamic aft limit overhangs the ground one,
    /// % MAC; zero when the aerodynamic boundary governs or none was placed.
    #[must_use]
    pub const fn overhang_pct_mac(self) -> f64 {
        match self {
            Self::GroundMinimumNoseLoad { margin_pct_mac } | Self::TipBack { margin_pct_mac } => {
                margin_pct_mac
            }
            Self::NotEvaluated | Self::Aerodynamic => 0.0,
        }
    }
}

/// Typed preliminary assessment of model stability and ground reactions.
///
/// This is not public planning, AFM, or WBM evidence. Manufacturer-source
/// comparisons remain a separate pipeline contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCgEnvelopeAssessment {
    /// Loading states evaluated by the model.
    pub loading_states: Vec<ModelCgLoadingAssessment>,
    /// The maximum landing mass, kg, that scopes every state and sets the
    /// landing-trim speeds: `AlasConfig::landing_mass_limit_kg` at the
    /// candidate MTOW (`requirements.mtow_kg`), the same MLW the mass model uses.
    pub design_landing_mass_kg: f64,
    /// Mean aerodynamic chord every `% MAC` quantity here is referred to, m.
    pub mac_m: f64,
    /// Hard longitudinal-stability floor, as a fraction of MAC.
    pub minimum_physical_static_margin: f64,
    /// Aft aerodynamic boundary: the critical (most-forward) neutral point
    /// minus [`Self::minimum_physical_static_margin`]. Weight/state
    /// independent.
    pub aerodynamic_aft_limit_pct_mac: f64,
    /// Diagnostic only: the clean, single-condition neutral point,
    /// `%MAC`; never used as a limit.
    pub clean_np_pct_mac: f64,
    /// The envelope-wide physical forward CG boundary: the most aft of the
    /// rotation boundary (evaluated at the analyzed takeoff state), the
    /// landing-trim boundary and the maximum-nose-load boundary, whatever
    /// the phase. It is not scoped per state: each state's
    /// [`ModelCgConstraint::PhysicalForwardCgLimit`] is gated against its own
    /// scoped [`ModelCgLoadingAssessment::physical_limits`], which can lie
    /// forward of this value. Gear sizing places the forward design CG here.
    pub configured_forward_limit_pct_mac: f64,
    /// Diagnostic only: the maximum-nose-load-handling forward boundary.
    pub max_nose_load_fwd_limit_pct_mac: f64,
    /// The landing-trim forward boundary, % MAC: the approach trim at
    /// `1.23 V_SR0` with the stabiliser at its stop (and the flare) when the
    /// airplane has a landing authority, else Torenbeek's eq. 6.2. It is the
    /// landing law that governs; [`Self::configured_forward_limit_pct_mac`]
    /// is the most aft of it, the rotation and the maximum nose load.
    pub landing_trim_fwd_limit_pct_mac: f64,
    /// Diagnostic only, reported beside the governing landing law: Torenbeek's
    /// design rule, eq. 6.2 at `C_L,max,land` with `C_L,h = -0.80`
    /// (*Advanced Aircraft Design*, Wiley, 2013, sec. 6.3.3, pp. 166-168).
    /// Never a limit.
    pub scissor_plot_fwd_limit_pct_mac: f64,
    /// Effective main-gear longitudinal station in the built model's own MAC
    /// frame, % MAC; `NaN` when no station could be placed.
    pub main_gear_station_pct_mac: f64,
    /// Aft boundary at which the nose-gear reaction falls to
    /// `mass_model.pct_load_nlg_min`, % MAC; `NaN` when no station could be
    /// placed.
    ///
    /// Derived from the same two-point static split the loading states use:
    /// `x_cg = x_mlg - pct_load_nlg_min * wheelbase`.
    pub ground_aft_limit_pct_mac: f64,
    /// Which of the three aft boundaries governs at the tightest (most
    /// forward) loading state; per-state detail (tip-back is `h_cg`
    /// dependent) lives on [`ModelCgLoadingAssessment::physical_limits`].
    pub aft_limit_governance: AftCgLimitGovernance,
    /// Most forward (worst-case) physical aft limit across every state, % MAC.
    pub worst_aft_limit_pct_mac: f64,
    /// True when this design carries real gear-tire ratings declared by
    /// configuration (forced wheel counts), so [`ModelCgConstraint::NoseGearStrength`]/
    /// [`ModelCgConstraint::MainGearStrength`] are non-tautological.
    /// When false, tire capacity was auto-sized to exactly this envelope and
    /// those two constraints are omitted from every loading state.
    pub capacity_basis_declared: bool,
    /// True when the sized nose/main tire is under-rated,
    /// declared basis or not.
    pub tire_overloaded: bool,
    /// Weight the gear was sized at, `max(MRW, takeoff mass)`, kg.
    pub gear_design_weight_kg: f64,
    /// Total main-gear static reaction the gear was sized for, kg: the design
    /// weight less the nose reaction at the ground aft design CG, which is
    /// held at or above the minimum static nose share (so never above the
    /// design weight).
    pub gear_design_main_reaction_kg: f64,
    /// True when the design aft CG lay behind the minimum-nose-load station
    /// and the main reaction was held at the ground limit.
    pub gear_aero_aft_limit_pulled_back: bool,
    /// Nose reaction of the raw moment balance at the design aft CG, before
    /// the minimum share is applied, kg; negative when the aerodynamic aft
    /// limit lies behind the main gear.
    pub gear_raw_aft_nose_reaction_kg: f64,
    /// True when the raw design aft CG is the takeoff-state CG (it lies behind
    /// the aerodynamic aft limit), false when the aerodynamic limit governs.
    pub gear_raw_aft_cg_is_takeoff_state: bool,
    /// Required tip-back/rotation angle, degrees: `max(min_tip_back_deg,
    /// tail_scrape_angle_deg)`.
    pub required_tip_back_deg: f64,
    /// Fuselage lower-contour scrape angle at the main gear, degrees, on the
    /// static ground plane, when the built geometry could place one. The
    /// tail-scrape gate uses the gear-extended plane instead when
    /// `LandingGearConfig::required_rotation_basis` is gear-extended or
    /// state-unspecified.
    pub scrape_angle_deg: Option<f64>,
    /// Soft optimizer preference, reported without governing feasibility.
    pub target_static_margin: StaticMarginPreferenceAssessment,
    /// The take-off trim model the rotation boundary used (a trimmable
    /// stabiliser whose setting is derived), for validation against the
    /// production inputs.
    pub(crate) takeoff_trim: Option<super::ths_trim::TakeoffTrimModel>,
}

impl ModelCgEnvelopeAssessment {
    /// Whether every hard constraint passes in every loading state.
    ///
    /// Diagnostic constraints ([`ModelCgConstraint::is_diagnostic`]) are
    /// excluded: they are reported as `Warning` findings
    /// (`alas-pipeline/src/feasibility/model_cg.rs`) but never reject.
    pub fn hard_constraints_pass(&self) -> bool {
        self.loading_states
            .iter()
            .flat_map(|state| &state.constraints)
            .filter(|constraint| !constraint.is_diagnostic())
            .all(|constraint| !constraint.violated)
    }

    /// The most forward (worst-case) physical aft boundary across every
    /// loading state, % MAC: [`Self::worst_aft_limit_pct_mac`].
    pub fn governing_aft_limit_pct_mac(&self) -> f64 {
        self.worst_aft_limit_pct_mac
    }

    /// Whether any loading state left the compression domain of the two-point
    /// ground split, that is, sat back on its tail.
    ///
    /// This separates a layout whose aft boundary is merely mis-attributed
    /// from one where the mis-attribution has already produced a reaction the
    /// gear cannot see.
    pub fn any_ground_reaction_inadmissible(&self) -> bool {
        self.loading_states
            .iter()
            .any(|state| !state.ground_reactions_admissible)
    }
}

/// Why the model-derived assessment could not be formed.
///
/// `Eq` is deliberately not derived: [`Self::MainGearStationNotMeasured`]
/// carries a [`StationError`] whose evidence is floating point, and an
/// exact-equality trait on it would invite comparisons that are not
/// meaningful. Every variant remains `PartialEq`.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ModelCgEnvelopeError {
    /// The aircraft has no main aerodynamic surface.
    #[error("model CG assessment requires a main wing")]
    MissingMainWing,
    /// The aircraft has no fuselage from which gear stations can be derived.
    #[error("model CG assessment requires a fuselage")]
    MissingFuselage,
    /// An input required by the static-equilibrium calculation is invalid.
    #[error("model CG assessment received a non-finite or non-positive input")]
    InvalidInput,
    /// No main-gear longitudinal station is available for this aircraft, so
    /// no ground reaction in this assessment has a point to act about.
    ///
    /// Every quantity this assessment reports about the ground (both gear
    /// strength limits, the minimum nose-gear load, and the wheelbase that
    /// normalizes them) is a moment about the main-gear station. When
    /// [`alas_mass::stations`] refuses to supply one, the wing-mounted
    /// fallback this module rebuilds is outside its stated domain, and the
    /// reactions computed from it would be reported as if they were measured.
    /// The assessment is therefore refused whole rather than returned with
    /// the gear constraints evaluated at an invented station; the carried
    /// [`StationError`] keeps the two heights that decided it.
    #[error("model CG assessment has no measured main-gear station: {0}")]
    MainGearStationNotMeasured(#[source] StationError),
}
