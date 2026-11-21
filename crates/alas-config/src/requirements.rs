// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/requirements.py

//! The stable wire group for mission targets and aircraft-level limits.
//!
//! The original configuration grouped mission requirements, discipline inputs
//! and a few study bounds here. The optimizer still reads this group as the
//! compatibility authority, but new code should classify each field explicitly
//! as a requirement, model input, preference or numerical setting. The classification
//! ledger lives in `docs/design-constraints.md`; moving fields
//! requires a saved-file migration rather than silently changing their meaning.
//!
//! The defaults describe a long-range transport, so the program produces a real aircraft
//! before anything has been configured.
//!
//! Passenger capacity is always recomputed for each candidate shell. A saved
//! `optimize_passenger_capacity` value is accepted for compatibility,
//! but does not change product behavior.
//!
//! Passenger and cargo targets are editable while the cabin preset is set to
//! `Custom`. A named preset computes its payload from geometry, and letting
//! both be edited would leave the two disagreeing with no indication of which
//! one the run used.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Cabin presets valid for a passenger aircraft.
const PASSENGER_PRESETS: &[&str] = &["Ryanair", "Iberia", "Emirates", "Custom"];

/// Cabin presets valid for a freighter.
const CARGO_PRESETS: &[&str] = &["Max payload", "Dense payload", "Custom"];

/// Why a set of requirements could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequirementsError {
    /// The aircraft type was neither `passenger` nor `cargo`.
    ///
    /// Upstream raises for the same input. It is a `Result` here because
    /// library code in this workspace does not panic on bad input; the
    /// condition, and when it is checked, are the same.
    #[error("aircraft_type must be 'passenger' or 'cargo', got '{0}'")]
    UnknownAircraftType(String),
}

/// One leg of a declared departure track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureTrackLeg {
    /// True heading of the leg, degrees.
    pub heading_deg: f64,
    /// Length of the leg, m.
    pub distance_m: f64,
}

/// Top-level mission targets and constraints supplied by the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct DesignRequirements {
    /// The speed the aircraft is sized around.
    #[config(
        label = "Cruise Mach number",
        help = "Design cruise Mach number: the primary speed target the optimizer sizes the aircraft around."
    )]
    pub cruise_mach: f64,

    /// The altitude the cruise point is evaluated at.
    #[config(
        label = "Cruise altitude",
        unit = "m",
        help = "Design cruise altitude, used to compute air density/speed of sound for the cruise design point."
    )]
    pub cruise_altitude_m: f64,

    /// A declared initial cruise altitude (ICA) requirement, m: the level a
    /// jet must reach from brake release at [`Self::initial_cruise_mass_kg`]
    /// and still hold the planner's 300 ft/min residual climb at
    /// maximum-climb thrust, at the top-of-climb mass the mission's own
    /// integrated climb leaves. Unset (the default), nothing is required and
    /// the achievable ICA is only reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        advanced,
        label = "Required initial cruise altitude (ICA)",
        unit = "m",
        help = "Optional requirement: the initial cruise level a jet must reach with a 300 ft/min residual climb at maximum-climb thrust. Empty: the achievable ICA is only reported.",
        long_help = "Optional initial cruise altitude (ICA) requirement for a turbofan: climbing from brake release at the ICA mass basis (empty: the maximum take-off weight), the aircraft must still have a 300 ft/min residual rate of climb at maximum-climb thrust on reaching this level, at the top-of-climb mass the mission's own integrated climb leaves (the 300 ft/min criterion of Airbus, Getting to Grips with Aircraft Performance, 2002, Maximum altitude, which the cruise-level planner also uses). When set, a jet whose engine is sized by the requirements is sized to meet it, and a held engine that misses it fails the check. Empty (default): nothing is required and the report shows the achievable ICA as information."
    )]
    pub initial_cruise_altitude_m: Option<f64>,

    /// The brake-release mass the [`Self::initial_cruise_altitude_m`]
    /// requirement is stated at, kg; unset, the maximum take-off weight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        advanced,
        label = "ICA mass basis",
        unit = "kg",
        help = "Brake-release mass the required initial cruise altitude is stated at. Empty: the maximum take-off weight.",
        long_help = "Brake-release mass the required initial cruise altitude is stated at, as published ICA figures are (for example at MTOW, or at a typical take-off weight). The top-of-climb mass is this mass less the take-off roll and climb fuel the mission integrates. Empty (default): the maximum take-off weight. Ignored when no initial cruise altitude is required."
    )]
    pub initial_cruise_mass_kg: Option<f64>,

    /// The weight the whole sizing pipeline is anchored to.
    #[config(
        label = "Max take-off weight (MTOW)",
        unit = "kg",
        help = "Target maximum take-off weight: anchors the whole weight & balance / sizing pipeline."
    )]
    pub mtow_kg: f64,

    /// Whether the aircraft carries passengers or freight.
    #[config(
        options = AircraftType,
        label = "Aircraft type",
        help = "'passenger' or 'cargo': switches which cabin-preset list and payload model apply."
    )]
    pub aircraft_type: String,

    /// The named layout the cabin is filled from.
    ///
    /// The serialized values are historical identifiers (originally named
    /// after the airline whose published layout each one reproduces, see
    /// `alas-payload/src/build/presets.rs`) and must not change; only the
    /// descriptive label the GUI shows for each one does
    /// (`alas-gui/src/views/form_options.rs::display_option`).
    #[config(
        options = CabinPreset,
        label = "Cabin preset",
        help = "Named seating/payload layout preset (passenger: high-density, two-class, three-class; cargo: max or dense payload).",
        long_help = "Named seating/payload layout preset ('High-density single-class', 'Two-class (Business/Economy)', 'Three-class (First/Business/Economy)' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab."
    )]
    pub cabin_preset: String,

    /// Legacy passenger load-case switch, retained for saved-file compatibility.
    #[serde(default = "default_optimize_passenger_capacity", skip_serializing)]
    #[config(skip)]
    pub optimize_passenger_capacity: bool,

    /// The operator's planned departure track after the runway, for the en-route obstacle
    /// rule (CAT.POL.A.215 applies along the planned route): legs of a true heading and a
    /// length, flown in order from the end of the take-off path. `None` (the default; no
    /// preset declares one) means no route is declared: the en-route check and the take-off
    /// obstacle check beyond the first allowed turn (CAT.POL.A.210 (b)(2)) then assume the
    /// straight-out track, as a diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub departure_track: Option<Vec<DepartureTrackLeg>>,

    /// Derived passenger capacity, retained in the serialized model.
    #[config(
        hidden,
        help = "Target passenger count (if aircraft_type is 'passenger'). Auto-recomputed when a cabin preset is active, only editable with cabin_preset set to 'Custom'."
    )]
    pub num_passengers: i64,

    /// How much freight the aircraft is sized for.
    #[config(
        label = "Cargo payload capacity",
        unit = "kg",
        readonly_unless(field = "cabin_preset", value = "Custom"),
        help = "Target cargo payload capacity (if aircraft_type is 'cargo'). Auto-recomputed when a cabin preset is active, only editable with cabin_preset set to 'Custom'."
    )]
    pub cargo_payload_kg: f64,

    /// The freight mass the design is asked to match, kg.
    ///
    /// The user's requested cargo objective: deliberately a different
    /// quantity from `cargo_payload_kg` (the capacity a cabin preset
    /// determines and the load case requests) and from the payload a
    /// candidate actually carries. A cabin preset writes the capacity and
    /// never this field, because a request that preset application
    /// overwrote would not be a requirement at all. Read it through
    /// [`Self::cargo_target_kg`], the mass the objective scores a
    /// candidate's deviation from.
    #[serde(default)]
    #[config(
        advanced,
        label = "Cargo capacity objective",
        unit = "kg",
        help = "Cargo payload mass requested of a freighter, which the search is rewarded for approaching [kg].",
        long_help = "Cargo capacity objective: the cargo payload mass requested of a freighter (aircraft_type 'cargo'), which the search is rewarded for getting closer to. It stays separate from the cargo payload capacity above and from the payload a candidate actually carries, so it redefines neither, and it is not a floor a candidate must clear: mass, volume, structural-loading and mission feasibility are judged exactly as before. 0 = disabled (the default), and the objective then follows the configured cargo payload capacity."
    )]
    pub cargo_objective_kg: f64,

    /// The most the airframe may carry, whatever the hold could hold.
    #[config(
        advanced,
        label = "Max structural payload",
        unit = "kg",
        help = "Maximum structural payload (MZFW - OEW) [kg].",
        long_help = "Maximum structural payload (= MZFW - OEW), i.e. the most the airframe may carry regardless of how much the belly could physically hold. In passenger mode the detailed layout fills the lower-deck belly with revenue freight (on top of passengers + checked bags) up to this structural limit, so the payload, and therefore the residual fuel (MTOW - OEW - payload), matches the real aircraft's max-payload point. A widebody belly can volumetrically hold far more than this structural cap, so without it 'fill the belly' overshoots. 0 = disabled (use the explicit Cabin & Payload belly_cargo_kg instead)."
    )]
    pub max_structural_payload_kg: f64,

    /// The certified maximum landing mass of the variant flown, kg: an explicit
    /// requirement that governs the landing-mass limit in every design mode,
    /// ahead of the preset's declared MLW and the mass-model fraction
    /// ([`crate::AlasConfig::landing_mass_limit_kg`]). Unset (the default),
    /// those apply. A freighter flown on its passenger donor's preset needs
    /// it when the two are certified to different landing weights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        advanced,
        label = "Max landing weight (MLW)",
        unit = "kg",
        help = "Optional requirement: the certified maximum landing weight of the variant flown. Empty: the preset's declared MLW, or the mass-model fraction of MTOW.",
        long_help = "Optional maximum landing weight (MLW) requirement: the certified landing-weight limit of the variant being flown, for example a freighter or an optional weight column whose MLW differs from the registered preset's. When set it is the landing-mass limit in every design mode (never above the candidate's take-off mass) and the mass model's landing-mass ratio follows it. Empty (default): reference-anchored modes use the preset's declared MLW and a clean-sheet design the mass-model fraction of MTOW. Must be finite, positive and not above the MTOW."
    )]
    pub mlw_kg: Option<f64>,

    /// The certified maximum zero-fuel weight of the variant flown, kg: an
    /// explicit requirement that governs the zero-fuel check and the payload
    /// corner of the range chart ([`crate::presets::published_mzfw_kg`]),
    /// ahead of the registered preset's declared MZFW and whatever the
    /// geometry. Unset (the default), the preset's value applies to the
    /// unchanged registered variant only, and the feasibility report says so
    /// when none applies. A freighter flown on its passenger donor's preset
    /// needs it: the donor's MZFW belongs to another aircraft type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        advanced,
        label = "Max zero-fuel weight (MZFW)",
        unit = "kg",
        help = "Optional requirement: the certified maximum zero-fuel weight of the variant flown. Empty: the registered preset's declared MZFW for its own variant; otherwise none is defined.",
        long_help = "Optional maximum zero-fuel weight (MZFW) requirement: the certified zero-fuel limit of the variant being flown, for example a freighter or an optional weight column whose MZFW differs from the registered preset's. When set it is the limit of the zero-fuel check and of the payload corner of the payload-range chart, whatever the geometry. Empty (default): the registered preset's declared MZFW applies to its own variant only (same aircraft type, no more structural payload than registered, unchanged geometry); where none applies the feasibility report carries a 'maximum zero-fuel weight undefined' warning instead of skipping the check. Must be finite, positive and not above the MTOW."
    )]
    pub mzfw_kg: Option<f64>,

    /// The fewest passengers a candidate's geometry-resolved cabin may carry.
    #[config(
        advanced,
        label = "Minimum passenger capacity",
        help = "Hard floor on the geometry-resolved passenger capacity: a candidate with fewer seats is infeasible. 0 disables it (default).",
        long_help = "Hard floor on the geometry-resolved passenger capacity: a candidate whose class-mix and geometry produce fewer than this many seats is infeasible. 0 = disabled (the default): capacity is otherwise always dynamic, whatever the configured cabin class-mix percentages and the candidate's actual fuselage/cabin geometry produce, with no minimum."
    )]
    pub min_passenger_capacity: i64,

    /// Limit load factor with the certification safety margin applied.
    ///
    /// Read it through [`Self::design_ultimate_load_factor`], which follows the
    /// design MTOW when [`Self::ultimate_load_factor_from_mtow`] is set.
    #[config(
        advanced,
        label = "Ultimate load factor (n_ult)",
        help = "Structural screening input for the Torenbeek mass formulas (ultimate load factor, dimensionless).",
        long_help = "Structural screening input fed into the Torenbeek mass formulas. The shipped 3.75 is 1.5 x 2.5; verify the selected certification basis, amendment, aircraft category and load case before treating it as an airworthiness value."
    )]
    pub ultimate_load_factor: f64,

    /// Whether the ultimate load factor follows the design MTOW by CS 25.337(b).
    #[serde(default)]
    #[config(
        advanced,
        label = "Load factor from MTOW (CS 25.337(b))",
        help = "Derive the ultimate load factor from the design MTOW by CS 25.337(b) and CS 25.303. Off uses the declared ultimate load factor.",
        long_help = "Derive the ultimate load factor from the design MTOW: 1.5 x (2.1 + 24,000 / (W + 10,000)), W in lb, the limit held between 2.5 and 3.8 (CS 25.337(b), CS 25.303). Off uses the declared ultimate load factor."
    )]
    pub ultimate_load_factor_from_mtow: bool,

    /// The structural design dive speed.
    #[config(
        advanced,
        label = "Design dive speed (V_dive)",
        unit = "m/s",
        help = "Structural screening dive speed for the Torenbeek mass formulas and the V-n diagram.",
        long_help = "Structural screening dive speed, fed into the Torenbeek mass formulas and the V-n diagram. VC is the least value CS 25.335(b) admits for it: the larger of 0.8 VD and the VC whose upset-criterion margin gives VD; presets that declare a VMO derive VD from it by that rule. Verify speed type, altitude/Mach envelope, certification basis and amendment before treating it as an airworthiness result."
    )]
    pub dive_speed_m_s: f64,

    /// MMO, the maximum operating Mach number, taken as the design cruise Mach
    /// number MC (CS 25.1505); MD is `MC + 0.07` (CS 25.335(b)(2)). Unset, the
    /// cruise Mach number stands for MC. Read it through
    /// [`Self::design_cruise_mach_limit`] and [`Self::design_dive_mach`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        advanced,
        label = "Max operating Mach (MMO)",
        help = "Optional: the maximum operating Mach number, taken as the design cruise Mach MC of the gust and dive cases; MD = MMO + 0.07. Empty: the cruise Mach number.",
        long_help = "Optional maximum operating Mach number (MMO). CS 25.1505 requires MMO not above the design cruise Mach MC, so the least MC is MMO; the dive Mach MD is MC + 0.07, the CS 25.335(b)(2) margin that AMC 25.335(b)(2) accepts without further analysis. The gust loads at VC and VD and the wing stiffness increment are limited by these Mach numbers at altitude. Presets fill it from the declared operating limits. Empty (default): the cruise Mach number stands for MC. Must be finite, positive and below 1 when set."
    )]
    pub max_operating_mach: Option<f64>,

    /// Whether the certification basis carries a high speed protection function, which puts the
    /// VD margin on CS 25.335(b)(1)(ii) instead of (b)(1)(i) (see [`crate::design_speeds`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[config(
        advanced,
        label = "High speed protection (CS 25.335(b)(1)(ii))",
        help = "The flight controls carry a high speed protection function: the dive speed margin follows CS 25.335(b)(1)(ii). Off: CS 25.335(b)(1)(i).",
        long_help = "Set when the certification basis carries a high speed protection function (for a type, the special condition or CRI on design dive speed; for a new design, a function the design declares and must certify). The VD margin is then the greater of the CS 25.335(b)(1)(ii) manoeuvres, (A) the 7.5 deg upset with the path held and (B) the 15 deg upset recovered 3 s after the speed warning, instead of the (b)(1)(i) 20 s upset. Presets set it from their certification basis. Off (default): CS 25.335(b)(1)(i)."
    )]
    pub high_speed_protection: bool,

    /// The negative limit load factor.
    #[config(
        advanced,
        label = "Limit load factor, negative (n_lim,neg)",
        help = "Negative V-n screening load factor (dimensionless). The shipped -1.0 follows the large-aeroplane CS-25 case up to VC; the positive limit is ultimate_load_factor / 1.5.",
        long_help = "Negative V-n screening input. The shipped -1.0 follows the large-aeroplane CS-25 reference case up to VC; verify the selected certification basis, amendment, speed range and category before using it for qualification. The positive limit value is derived as ultimate_load_factor / 1.5."
    )]
    pub limit_load_factor_neg: f64,

    /// Upper bound on projected wing reference/planform area.
    #[config(
        advanced,
        label = "Maximum wing area",
        unit = "m^2",
        help = "Upper bound on the wing planform projected onto the aircraft XY reference plane; the optimizer is penalised for exceeding it."
    )]
    pub max_wing_area_m2: f64,

    /// Lower bound on wing loading.
    #[config(
        advanced,
        label = "Minimum wing loading (MTOW/S)",
        unit = "kg/m^2",
        help = "Lower bound on wing loading (MTOW / wing area): keeps the wing from being sized too large for the mass it carries."
    )]
    pub min_wing_loading_kg_m2: f64,

    /// How close to stall a cruise point may sit before it is rejected.
    #[config(
        advanced,
        label = "Maximum cruise CL (stall guard)",
        help = "Candidate designs whose required cruise CL exceeds this are rejected as infeasible (too close to stall)."
    )]
    pub max_cruise_cl: f64,

    /// A preferred static margin, used by the frozen legacy/reference CG
    /// gate and by report figures; the product gate enforces
    /// `min_physical_static_margin` instead.
    #[config(
        advanced,
        label = "Target static margin",
        unit = "fraction of MAC",
        help = "Preferred static margin at the aerodynamic aft CG limit (fraction of MAC): aft limit [%MAC] = neutral point - target*100.",
        long_help = "Preferred static margin at the aerodynamic aft CG limit: Aft CG Limit (%MAC) = Neutral Point (%MAC) - target_static_margin*100. This is a design preference used by the frozen reference CG-envelope path and report figures; the product feasibility gate enforces the (typically smaller) min_physical_static_margin below, measured at the actual mass-model CG, not this aerodynamic-reference target. The two boundaries can differ by (target_static_margin - min_physical_static_margin)*100 %MAC."
    )]
    pub target_static_margin: f64,

    /// How wide the centre-of-gravity envelope is.
    #[config(
        advanced,
        label = "CG envelope width",
        unit = "% MAC",
        help = "Width of the CG envelope. Forward CG Limit (%MAC) = Aft CG Limit (%MAC) - cg_range_pct_mac."
    )]
    pub cg_range_pct_mac: f64,

    /// The stability floor a design is hard-rejected below: the boundary
    /// the product feasibility gate actually enforces (see
    /// `target_static_margin` for the separate, larger design preference).
    ///
    /// Basis of the 0.05 default: a typical value, not a requirement.
    /// Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., sec. 16.3,
    /// printed p. 593: at the most aft CG "a typical transport aircraft has a
    /// positive static margin of 5-10%"; 0.05 is the low end of that range.
    /// The requirement itself is only a positive margin, the CG ahead of the
    /// neutral point: Torenbeek, *Advanced Aircraft Design*, 2013,
    /// sec. 6.3.1, printed p. 167.
    #[config(
        advanced,
        label = "Minimum physical static margin",
        unit = "fraction of MAC",
        help = "Minimum static margin at the actual mass-model CG (fraction of MAC).",
        long_help = "Minimum static margin measured using the actual mass-model (physical) CG, not the aerodynamic reference point. Designs below this are hard-rejected as inherently unstable. 0.0 = bare stability; 0.05 = 5% MAC buffer (recommended). This is the boundary the product feasibility gate enforces at every loading state; target_static_margin above is a separate, larger design preference used by the frozen reference path and report figures."
    )]
    pub min_physical_static_margin: f64,

    /// Average mass of an occupant and their baggage.
    #[config(
        min = 50.0,
        max = 200.0,
        decimals = 1,
        label = "Mass per passenger",
        unit = "kg",
        help = "Combined average mass per occupant, body plus baggage [kg]. The shipped 100 kg is a project load-case default, not an FAA AC 120-27F figure.",
        long_help = "Combined average mass per occupant (body + baggage). The shipped 100 kg is a transparent project load-case default; FAA AC 120-27F is operator weight-and-balance guidance and does not establish a universal passenger mass. Record the operator, population, baggage method and date before using another value operationally. This remains the single load-case authority for report, GUI preview, pipeline, export and optimizer paths: cabin.passenger.checked_bag_mass_kg supplies the baggage share and the occupant slot the remainder."
    )]
    pub passenger_mass_kg: f64,

    /// Gravitational acceleration.
    #[config(
        advanced,
        label = "Gravitational acceleration",
        unit = "m/s^2",
        help = "Standard gravity, used to turn every mass in the sizing into a weight. Configuration rather than a constant so a run's assumptions are all in one place and all visible."
    )]
    pub gravity_m_s2: f64,
}

impl Default for DesignRequirements {
    fn default() -> Self {
        Self {
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            initial_cruise_altitude_m: None,
            initial_cruise_mass_kg: None,
            mtow_kg: 358_670.0,
            aircraft_type: "passenger".to_owned(),
            cabin_preset: "Ryanair".to_owned(),
            optimize_passenger_capacity: true,
            departure_track: None,
            num_passengers: 350,
            cargo_payload_kg: 102_100.0,
            cargo_objective_kg: 0.0,
            max_structural_payload_kg: 0.0,
            mlw_kg: None,
            mzfw_kg: None,
            min_passenger_capacity: 0,
            ultimate_load_factor: 3.75,
            ultimate_load_factor_from_mtow: false,
            dive_speed_m_s: 220.0,
            max_operating_mach: None,
            high_speed_protection: false,
            limit_load_factor_neg: -1.0,
            max_wing_area_m2: 535.0,
            min_wing_loading_kg_m2: 485.0,
            max_cruise_cl: 0.95,
            target_static_margin: 0.10,
            cg_range_pct_mac: 30.0,
            min_physical_static_margin: 0.05,
            passenger_mass_kg: 100.0,
            // The single standard-gravity constant, matching
            // `alas_units::STANDARD_GRAVITY` (the CODATA/exact definitional
            // value that already drives every lbf conversion and the ISA
            // elsewhere in the program), rather than the frozen two-decimal
            // 9.81. `alas-config` has no dependency on `alas-units`
            // so the value is repeated here as a literal rather than
            // importing the constant across a new crate edge.
            gravity_m_s2: alas_atmo::STANDARD_GRAVITY,
        }
    }
}

const fn default_optimize_passenger_capacity() -> bool {
    true
}

/// The CS 25.337(b) positive limit manoeuvring load factor at `mtow_kg`, times
/// the 1.5 factor of safety of CS 25.303: `n = 2.1 + 24,000 / (W + 10,000)`
/// with W in lb, not less than 2.5 and not more than 3.8. NaN for a mass that
/// is not finite and positive.
pub fn cs25_ultimate_load_factor(mtow_kg: f64) -> f64 {
    const POUND_KG: f64 = 0.453_592_37;
    if !mtow_kg.is_finite() || mtow_kg <= 0.0 {
        return f64::NAN;
    }
    1.5 * (2.1 + 24_000.0 / (mtow_kg / POUND_KG + 10_000.0)).clamp(2.5, 3.8)
}

impl DesignRequirements {
    /// MC, the design cruise Mach number of the gust and dive cases: the
    /// declared [`Self::max_operating_mach`] (CS 25.1505), else the cruise Mach.
    #[must_use]
    pub fn design_cruise_mach_limit(&self) -> f64 {
        self.max_operating_mach
            .filter(|mach| mach.is_finite() && *mach > 0.0)
            .unwrap_or(self.cruise_mach)
    }

    /// MD, the design dive Mach number: [`Self::design_cruise_mach_limit`]
    /// plus the CS 25.335(b)(2) margin of 0.07.
    #[must_use]
    pub fn design_dive_mach(&self) -> f64 {
        crate::design_speeds::dive_mach(self.design_cruise_mach_limit())
    }

    /// The CS 25.335(b) criterion of [`Self::high_speed_protection`].
    #[must_use]
    pub fn upset_criterion(&self) -> crate::design_speeds::UpsetCriterion {
        crate::design_speeds::UpsetCriterion::from_protection(self.high_speed_protection)
    }

    /// VC, m/s EAS, of [`Self::dive_speed_m_s`] under [`Self::upset_criterion`].
    #[must_use]
    pub fn design_cruise_speed_eas_m_s(&self) -> f64 {
        crate::design_speeds::design_cruise_speed_eas_m_s_for(
            self.dive_speed_m_s,
            self.upset_criterion(),
        )
    }

    /// The ultimate load factor the design is sized to: the CS 25.337(b) value
    /// at the running [`Self::mtow_kg`] when
    /// [`Self::ultimate_load_factor_from_mtow`] is set, else the declared
    /// [`Self::ultimate_load_factor`].
    pub fn design_ultimate_load_factor(&self) -> f64 {
        if self.ultimate_load_factor_from_mtow {
            cs25_ultimate_load_factor(self.mtow_kg)
        } else {
            self.ultimate_load_factor
        }
    }

    /// The declared initial cruise altitude requirement as `(altitude_m,
    /// brake_release_mass_kg)`, the mass defaulting to `mtow_kg` (the
    /// candidate's maximum take-off weight); `None` when no usable altitude
    /// is declared. A declared mass that is not finite and positive is no
    /// declaration, so the default applies.
    pub fn initial_cruise_requirement(&self, mtow_kg: f64) -> Option<(f64, f64)> {
        let usable = |value: f64| value.is_finite() && value > 0.0;
        let altitude_m = self.initial_cruise_altitude_m.filter(|h| usable(*h))?;
        let mass_kg = self
            .initial_cruise_mass_kg
            .filter(|m| usable(*m))
            .unwrap_or(mtow_kg);
        Some((altitude_m, mass_kg))
    }

    /// Whether the payload target is recomputed from each candidate's cabin.
    ///
    /// This is unconditional in the product model. The legacy serialized
    /// switch is deliberately ignored so an old file cannot restore partial
    /// floor use.
    pub fn resolves_payload_from_candidate_geometry(&self) -> bool {
        true
    }

    /// Reject an unknown aircraft type, and bring the cabin preset into line
    /// with the type.
    ///
    /// A freighter carrying a passenger preset, or the reverse, is not a
    /// configuration anything downstream can act on: the payload model
    /// selected by the type would be reading a layout meant for the other
    /// one. Upstream does this when a requirements object is constructed,
    /// and reproducing where it happens matters, laying a partial
    /// configuration over an existing one does *not* re-run it there, so a
    /// file that sets only the aircraft type leaves a mismatched preset in
    /// place, and this port must not quietly fix that.
    ///
    /// # Errors
    ///
    /// [`RequirementsError::UnknownAircraftType`] when the type is neither
    /// `passenger` nor `cargo`.
    pub fn normalized(mut self) -> Result<Self, RequirementsError> {
        let presets = match self.aircraft_type.as_str() {
            "passenger" => PASSENGER_PRESETS,
            "cargo" => CARGO_PRESETS,
            other => return Err(RequirementsError::UnknownAircraftType(other.to_owned())),
        };
        if !presets.contains(&self.cabin_preset.as_str()) {
            // The two fall back differently upstream: a freighter with an
            // unrecognized preset gets the default layout, a passenger
            // aircraft gets `Custom`. Reproduced rather than unified.
            self.cabin_preset = if self.aircraft_type == "cargo" {
                "Max payload".to_owned()
            } else {
                "Custom".to_owned()
            };
        }
        Ok(self)
    }

    /// Total payload mass, from whichever of the two payload models the
    /// aircraft type selects.
    pub fn payload_kg(&self) -> f64 {
        if self.aircraft_type == "cargo" {
            return self.cargo_payload_kg;
        }
        self.num_passengers as f64 * self.passenger_mass_kg
    }

    /// The cargo payload mass the objective scores a candidate against, kg.
    ///
    /// The requested [`Self::cargo_objective_kg`] when the user entered one,
    /// otherwise the configured [`Self::cargo_payload_kg`] capacity, which is
    /// the mass the objective used before a request could be entered: a study
    /// that enters none is scored exactly as it was. A non-finite or
    /// non-positive request is no request, since a payload mass is positive
    /// by definition and zero or below is outside the target's valid domain
    /// rather than a request to carry nothing.
    ///
    /// This resolves the target only; it writes neither field, so the request
    /// can redefine neither the capacity a cabin preset determined nor the
    /// payload the load case actually carries.
    pub fn cargo_target_kg(&self) -> f64 {
        if self.cargo_objective_kg.is_finite() && self.cargo_objective_kg > 0.0 {
            return self.cargo_objective_kg;
        }
        self.cargo_payload_kg
    }

    /// The lift coefficient needed to hold level flight: `CL = W / (q * S)`.
    pub fn required_cruise_cl(&self, dynamic_pressure_pa: f64, wing_area_m2: f64) -> f64 {
        let weight_n = self.mtow_kg * self.gravity_m_s2;
        weight_n / (dynamic_pressure_pa * wing_area_m2)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_load_factor_follows_the_mtow_only_when_asked() {
        let mut req = DesignRequirements::default();
        assert_eq!(req.design_ultimate_load_factor(), 3.75);
        req.ultimate_load_factor_from_mtow = true;
        // Above 50,000 lb the rule floors at 2.5.
        assert!((req.design_ultimate_load_factor() - 3.75).abs() < 1e-12);
        // The LJ60 at 23,500 lb: 2.816 limit.
        req.mtow_kg = 10_659.0;
        let at_mtow = req.design_ultimate_load_factor();
        assert!((at_mtow - 4.224).abs() < 1e-3, "{at_mtow}");
        req.mtow_kg = 9_000.0;
        assert!(req.design_ultimate_load_factor() > at_mtow);
        assert!(cs25_ultimate_load_factor(0.0).is_nan());
    }

    #[test]
    fn an_initial_cruise_altitude_is_required_only_when_declared() {
        let mut req = DesignRequirements::default();
        assert_eq!(req.initial_cruise_requirement(70_000.0), None);
        req.initial_cruise_mass_kg = Some(60_000.0);
        assert_eq!(req.initial_cruise_requirement(70_000.0), None);
        req.initial_cruise_altitude_m = Some(10_058.4);
        assert_eq!(
            req.initial_cruise_requirement(70_000.0),
            Some((10_058.4, 60_000.0))
        );
        req.initial_cruise_mass_kg = None;
        assert_eq!(
            req.initial_cruise_requirement(70_000.0),
            Some((10_058.4, 70_000.0))
        );
        for unusable in [0.0, -1.0, f64::NAN] {
            req.initial_cruise_altitude_m = Some(unusable);
            assert_eq!(req.initial_cruise_requirement(70_000.0), None);
        }
        // Unset fields stay out of a saved file, so files written before
        // they existed load unchanged.
        let saved = serde_json::to_value(DesignRequirements::default()).unwrap();
        assert!(saved.get("initial_cruise_altitude_m").is_none());
        assert!(saved.get("initial_cruise_mass_kg").is_none());
    }

    #[test]
    fn the_shipped_defaults_need_no_normalizing() {
        // If they did, the defaults and what a run actually uses would differ,
        // and the parity fixture records the defaults.
        let defaults = DesignRequirements::default();
        assert_eq!(defaults.clone().normalized().unwrap(), defaults);
    }

    #[test]
    fn a_freighter_carrying_a_passenger_preset_is_brought_into_line() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cabin_preset: "Emirates".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            requirements.normalized().unwrap().cabin_preset,
            "Max payload"
        );
    }

    #[test]
    fn a_passenger_aircraft_with_an_unknown_preset_falls_back_to_custom() {
        // The two fall back to different things upstream, and the difference
        // is reproduced rather than tidied up.
        let requirements = DesignRequirements {
            cabin_preset: "Max payload".to_owned(),
            ..Default::default()
        };
        assert_eq!(requirements.normalized().unwrap().cabin_preset, "Custom");
    }

    #[test]
    fn an_unknown_aircraft_type_is_an_error_not_a_panic() {
        let requirements = DesignRequirements {
            aircraft_type: "seaplane".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            requirements.normalized().unwrap_err(),
            RequirementsError::UnknownAircraftType("seaplane".to_owned())
        );
    }

    #[test]
    fn the_payload_comes_from_whichever_model_the_type_selects() {
        let passenger = DesignRequirements::default();
        assert_eq!(passenger.payload_kg(), 350.0 * 100.0);

        let freighter = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            ..Default::default()
        };
        assert_eq!(freighter.payload_kg(), freighter.cargo_payload_kg);
    }

    #[test]
    fn the_cargo_objective_is_disabled_by_default_and_the_capacity_is_the_target() {
        // Nothing entered: the objective is scored against the configured
        // capacity, which is the mass it was scored against before a request
        // could be entered at all.
        let defaults = DesignRequirements::default();
        assert_eq!(defaults.cargo_objective_kg, 0.0);
        assert_eq!(defaults.cargo_target_kg(), defaults.cargo_payload_kg);
    }

    #[test]
    fn an_entered_cargo_objective_is_the_target_and_leaves_the_capacity_alone() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cargo_payload_kg: 45_000.0,
            cargo_objective_kg: 60_000.0,
            ..Default::default()
        };
        assert_eq!(requirements.cargo_target_kg(), 60_000.0);
        // The request is the target and nothing else: the capacity, and so
        // the payload the load case asks the hold for, are untouched.
        assert_eq!(requirements.cargo_payload_kg, 45_000.0);
        assert_eq!(requirements.payload_kg(), 45_000.0);
    }

    #[test]
    fn a_cargo_objective_outside_its_valid_domain_is_no_request() {
        // A payload mass is positive and finite. Anything else is not a
        // smaller target, it is no target, and the capacity keeps the role.
        for request in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let requirements = DesignRequirements {
                cargo_objective_kg: request,
                ..Default::default()
            };
            assert_eq!(
                requirements.cargo_target_kg(),
                requirements.cargo_payload_kg,
                "request {request}"
            );
        }
    }

    #[test]
    fn the_cargo_objective_survives_a_save_and_load_round_trip() {
        let requirements = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            cabin_preset: "Max payload".to_owned(),
            cargo_objective_kg: 62_500.0,
            ..Default::default()
        };
        let saved = serde_json::to_value(requirements.clone()).unwrap();
        assert_eq!(saved["cargo_objective_kg"], 62_500.0);
        let loaded: DesignRequirements = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded, requirements);
        assert_eq!(loaded.cargo_target_kg(), 62_500.0);
    }

    #[test]
    fn a_file_written_before_the_cargo_objective_existed_loads_with_it_disabled() {
        let mut saved = serde_json::to_value(DesignRequirements::default()).unwrap();
        saved.as_object_mut().unwrap().remove("cargo_objective_kg");
        let loaded: DesignRequirements = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.cargo_objective_kg, 0.0);
        assert_eq!(loaded, DesignRequirements::default());
    }

    #[test]
    fn the_cargo_objective_is_an_advanced_field_in_kilograms() {
        // Advanced, so the guided requirements card keeps exactly the fields
        // it had: the objective is rendered there by the Inputs view itself,
        // and only for a freighter. It is a free input at all times, unlike
        // the capacity a cabin preset owns.
        let schema = DesignRequirements::default().schema();
        let field = schema
            .field("cargo_objective_kg")
            .expect("the cargo objective is in the schema");
        assert_eq!(field.unit, "kg");
        assert!(field.advanced);
        match &field.entry {
            crate::Entry::Leaf(leaf) => assert!(leaf.readonly_unless.is_none()),
            crate::Entry::Node(_) => panic!("a payload mass is not a group"),
        }
    }

    #[test]
    fn candidate_geometry_always_resolves_payload_capacity() {
        let fixed = DesignRequirements {
            cabin_preset: "Custom".to_owned(),
            optimize_passenger_capacity: false,
            num_passengers: 130,
            ..Default::default()
        };
        let capacity = DesignRequirements {
            optimize_passenger_capacity: true,
            ..fixed.clone()
        };

        assert!(!fixed.optimize_passenger_capacity);
        assert_eq!(fixed.num_passengers, 130);
        assert!(fixed.resolves_payload_from_candidate_geometry());
        assert!(capacity.resolves_payload_from_candidate_geometry());

        let cargo = DesignRequirements {
            aircraft_type: "cargo".to_owned(),
            optimize_passenger_capacity: false,
            ..fixed
        };
        assert!(cargo.resolves_payload_from_candidate_geometry());
    }

    #[test]
    fn historical_files_without_the_load_case_flag_keep_the_reference_default() {
        let value = serde_json::to_value(DesignRequirements::default()).unwrap();
        assert!(value.get("optimize_passenger_capacity").is_none());
        let requirements: DesignRequirements = serde_json::from_value(value).unwrap();

        assert!(requirements.optimize_passenger_capacity);
    }

    #[test]
    fn a_legacy_false_capacity_flag_loads_but_is_ignored_and_not_saved_again() {
        let mut value = serde_json::to_value(DesignRequirements::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("optimize_passenger_capacity".to_owned(), false.into());

        let requirements: DesignRequirements = serde_json::from_value(value).unwrap();
        assert!(!requirements.optimize_passenger_capacity);
        assert!(requirements.resolves_payload_from_candidate_geometry());
        assert!(serde_json::to_value(requirements)
            .unwrap()
            .get("optimize_passenger_capacity")
            .is_none());
    }

    #[test]
    fn the_legacy_load_case_choice_is_absent_from_the_settings_schema() {
        let schema = DesignRequirements::default().schema();
        assert!(schema.field("optimize_passenger_capacity").is_none());
        assert!(schema.field("num_passengers").is_none());
    }

    #[test]
    fn the_required_lift_coefficient_falls_with_speed_and_with_wing_area() {
        let requirements = DesignRequirements::default();
        let base = requirements.required_cruise_cl(15_000.0, 500.0);
        assert!(requirements.required_cruise_cl(30_000.0, 500.0) < base);
        assert!(requirements.required_cruise_cl(15_000.0, 1000.0) < base);
        // W / (q S), checked against the arithmetic rather than a recorded
        // number, since this is the one formula in this module.
        let expected = 358_670.0 * 9.806_65 / (15_000.0 * 500.0);
        assert!((base - expected).abs() < 1e-12);
    }

    #[test]
    fn cargo_payload_remains_editable_only_for_a_custom_preset() {
        let schema = DesignRequirements::default().schema();
        match &schema.field("cargo_payload_kg").unwrap().entry {
            crate::Entry::Leaf(leaf) => {
                let condition = leaf.readonly_unless.unwrap();
                assert_eq!(condition.field, "cabin_preset");
                assert_eq!(condition.value, "Custom");
            }
            crate::Entry::Node(_) => panic!("a passenger count is not a group"),
        }
    }

    #[test]
    fn the_passenger_capacity_floor_defaults_to_disabled_and_is_advanced() {
        assert_eq!(DesignRequirements::default().min_passenger_capacity, 0);
        let schema = DesignRequirements::default().schema();
        let field = schema
            .field("min_passenger_capacity")
            .expect("min_passenger_capacity is in the schema");
        assert!(field.advanced);
    }
}
