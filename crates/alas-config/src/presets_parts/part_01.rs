// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../presets/cg_envelope.rs"]
mod cg_envelope;
#[path = "../presets/mission_evidence.rs"]
mod mission_evidence;
#[path = "../presets/narrowbody.rs"]
mod narrowbody;
#[path = "../presets/reference.rs"]
mod reference;
#[path = "../presets/regional.rs"]
mod regional;
#[path = "../presets/widebody.rs"]
mod widebody;

pub use cg_envelope::{
    CgEnvelopeCondition, CgEnvelopeSource, CgEnvelopeVertex, CgLimits, PlanningCgEnvelope,
    PlanningMacReference,
};
pub use mission_evidence::{
    applicability_label, datum_label, DesignMissionProvenanceSet, MissionDatumProvenance,
    MissionEvidenceTier, MissionPromotionRefusal,
};

use std::sync::OnceLock;

use crate::{
    DesignRequirements, DesignVector, GeometryConfig, LandingGearConfig, MassModelConfig,
    PerformanceConfig,
};

/// Provenance category of the CG limits available for a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CgEnvelopeEvidence {
    /// The public source is only a planning envelope; the aircraft WBM controls.
    PublicPlanning,
    /// The type-certificate source explicitly delegates limits to the AFM/WBM.
    AfmRequired,
    /// The aircraft is notional and its envelope is a design requirement.
    DesignRequirement,
    /// No defensible envelope source has yet been registered.
    #[default]
    Unknown,
}

/// Source-defined design mission against which a preset can be validated.
///
/// The airports stored in [`crate::AlasConfig`] describe the route selected
/// for one run. They are not aircraft-preset data and therefore cannot stand
/// in for a published payload/range mission.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignMissionReference {
    /// Source-defined still-air mission range, in metres.
    pub range_m: f64,
    /// Payload carried at the source-defined range, in kilograms.
    pub payload_kg: f64,
    /// Speed, altitude, climb, and descent assumptions for the mission.
    pub profile: crate::MissionProfileConfig,
    /// Fuel that must remain after the modeled trip, in kilograms.
    pub required_reserve_fuel_kg: f64,
    /// Optional departure field when the source defines one.
    pub departure_airport: Option<&'static str>,
    /// Optional arrival field when the source defines one.
    pub arrival_airport: Option<&'static str>,
    /// Exact document, revision, and location defining the mission.
    pub source: &'static str,
}

/// Evidence that a registered preset has a defensible design mission.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DesignMissionEvidence {
    /// A revision-locked source defines range, payload, profile, and reserve.
    SourceBacked(Box<DesignMissionReference>),
    /// No complete source-backed design mission has been registered.
    #[default]
    Unverified,
}

/// Kind of public range or mission evidence without promoting it to a mission.
///
/// A marketing range, a point read from a planning chart, a certification
/// demonstration, and the mission used to size an aircraft answer different
/// questions. Keeping the category typed prevents a numeric range from being
/// accepted merely because all four are commonly called a "mission".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialMissionEvidenceKind {
    /// Manufacturer-advertised maximum range, with its stated configuration.
    AdvertisedRange,
    /// A manufacturer payload/range capability chart or a point on that chart.
    PayloadRangeChart,
    /// A flight or analysis explicitly identified as a certification demonstration.
    CertificationDemonstration,
    /// An explicitly identified aircraft design or sizing mission.
    ActualDesignMission,
}

/// A range exactly as its source publishes it.
///
/// Planning documents often print rounded nautical-mile and kilometre values
/// side by side. Preserving the published unit avoids manufacturing precision
/// by converting one rounded value into the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PublishedRange {
    /// Range in nautical miles.
    NauticalMiles(f64),
    /// Range in kilometres.
    Kilometres(f64),
}

/// A published mass condition that bounds a capability chart without selecting
/// a payload/range design point.
///
/// These cases remain separate from [`PartialDesignMissionEvidence::payload_kg`]:
/// maximum takeoff mass and zero-fuel-weight envelopes are not payloads, and
/// treating either as one would manufacture a mission load.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishedMissionLoadCase {
    /// Curves published for the listed maximum takeoff masses.
    TakeoffMassesKg(Vec<f64>),
    /// A zero-fuel-weight/range envelope with no selected zero-fuel weight.
    ZeroFuelWeightRangeEnvelope,
}

/// Source applicability of partial mission evidence to the registered preset.
///
/// A record can identify the right model yet still fail to select its exact
/// weight variant. Keeping that distinction typed stops a nearby product claim
/// from being mistaken for evidence about the configured aircraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionEvidenceApplicability {
    /// The source identifies the exact registered model, variant, and planning configuration.
    ExactPreset,
    /// The source identifies the model and engine family, but not the registered weight variant.
    ModelAndEngineFamily,
    /// The source identifies the model but not its exact engine/weight configuration.
    ModelOnly,
    /// The source identifies a related product at a different weight variant.
    DifferentWeightVariant,
}

/// Published reserve-policy terms that do not state a reserve fuel mass.
///
/// A diversion distance, a trip-fuel percentage, and a holding duration are
/// operational assumptions. They cannot become a reserve mass until an
/// aircraft-specific trip and holding fuel calculation is sourced or modeled
/// under an explicitly accepted method, so their presence does not remove
/// [`MissingDesignMissionDatum::ReserveFuel`].
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedReserveContract {
    /// Diversion distance required by the source, when it states one.
    pub diversion_range: Option<PublishedRange>,
    /// Contingency fuel expressed as a share of trip fuel, when published.
    pub trip_fuel_allowance_fraction: Option<f64>,
    /// Required holding duration in minutes, when published.
    pub holding_time_minutes: Option<f64>,
}

/// Required datum still absent from a partial design-mission record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingDesignMissionDatum {
    /// A source-selected range rather than a chart axis or marketing maximum.
    Range,
    /// Payload carried at that range.
    Payload,
    /// Explicit climb, cruise, and descent assumptions.
    Profile,
    /// Required reserve fuel mass after the modeled trip.
    ReserveFuel,
}

/// Useful public evidence that is insufficient for [`DesignMissionEvidence`].
///
/// Text assumptions are retained as evidence only. They are deliberately not
/// converted into [`crate::MissionProfileConfig`] or a reserve mass because a
/// chart caption such as "long-range cruise" does not define either one.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialDesignMissionEvidence {
    /// What the source is actually documenting.
    pub kind: PartialMissionEvidenceKind,
    /// Published range, only when the source states one without digitization.
    pub range: Option<PublishedRange>,
    /// Published payload mass, only when the source states one without inference.
    pub payload_kg: Option<f64>,
    /// Published mass condition or envelope, distinct from a payload mass.
    pub load_case: Option<PublishedMissionLoadCase>,
    /// Profile words present in the source, without filling omitted phases.
    pub profile_assumptions: Option<&'static str>,
    /// Reserve words present in the source, without deriving a fuel mass.
    pub reserve_assumptions: Option<&'static str>,
    /// Structured reserve-policy terms, when the source publishes them.
    pub reserve_contract: Option<PublishedReserveContract>,
    /// Why this evidence does or does not apply to the exact preset variant.
    pub applicability: &'static str,
    /// Typed relation between the source configuration and the registered preset.
    pub configuration_applicability: MissionEvidenceApplicability,
    /// The four contract fields that remain unavailable.
    pub missing: Vec<MissingDesignMissionDatum>,
    /// Exact document, revision, page, and figure when one is numbered.
    pub source: &'static str,
}

/// Exact certified/configuration identity represented by a real-aircraft preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircraftVariantIdentity {
    /// Certified aircraft model or versioned notional design.
    pub model: &'static str,
    /// Manufacturer weight-variant identifier.
    pub weight_variant: &'static str,
    /// Installed engine model, not merely its family.
    pub engine_model: &'static str,
    /// Modification state needed to make the weight and geometry data coherent.
    pub modification_state: &'static str,
    /// Fuel-tank configuration to which the usable capacity applies.
    pub tank_configuration: &'static str,
}

/// One source-defined emergency-exit pair and its CS-25 evacuation rating.
///
/// The capacities in the regulation are ratings for the complete pair of
/// exits.  Keeping that unit in the field name prevents a consumer from
/// multiplying a pair rating by two when it emits the two physical door
/// cut-outs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CertifiedExitPair {
    /// Exit class printed in the source cabin configuration.
    pub exit_type: &'static str,
    /// Passengers assigned to this complete exit pair.
    pub capacity_per_pair: i64,
}

/// A revision-locked exit arrangement for a registered aircraft variant.
///
/// This is source metadata used to keep a product preset's cabin topology
/// separate from the generic diameter heuristic.  It is not a declaration
/// that the layout engine has demonstrated the aircraft's certified
/// evacuation performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CertifiedExitLayout {
    /// Human-readable pair sequence, for example `C-III-C`.
    pub label: &'static str,
    /// Exit pairs in source order, including each pair's rating.
    pub pairs: &'static [CertifiedExitPair],
    /// Exact source, revision and location for the arrangement.
    pub source: &'static str,
}

/// Primary-source values against which one preset is validated.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AircraftReferenceData {
    /// Maximum ramp weight.
    pub mrw_kg: Option<f64>,
    /// Maximum takeoff weight.
    pub mtow_kg: Option<f64>,
    /// Maximum landing weight.
    pub mlw_kg: Option<f64>,
    /// Maximum zero-fuel weight.
    pub mzfw_kg: Option<f64>,
    /// Same-aircraft OEW for conditional comparison, from [`crate::oew_reference`].
    pub oew_kg: Option<f64>,
    /// Usable fuel volume before applying the declared density.
    pub usable_fuel_volume_l: Option<f64>,
    /// Published usable fuel mass for the declared density.
    pub usable_fuel_mass_kg: Option<f64>,
    /// Density used by the source's volume-to-mass conversion.
    pub fuel_density_kg_l: Option<f64>,
    /// Manufacturer/reference-plane wing area.
    pub reference_wing_area_m2: Option<f64>,
    /// Manufacturer planning cabin, not a certification limit.
    pub planning_seats: Option<i64>,
    /// Certified evacuation maximum for the applicable exit arrangement.
    pub certified_max_seats: Option<i64>,
    /// Source-defined exit-pair arrangement for the registered variant.
    pub certified_exit_layout: Option<CertifiedExitLayout>,
    /// Whether a complete design-mission definition has source provenance.
    pub design_mission_evidence: DesignMissionEvidence,
    /// Relevant public range/mission material that is not a complete mission.
    pub partial_design_mission_evidence: Vec<PartialDesignMissionEvidence>,
    /// What kind of CG evidence is publicly available.
    pub cg_evidence: CgEnvelopeEvidence,
    /// Published planning curve, when the source provides one.
    ///
    /// This is deliberately absent for presets whose type-certificate source
    /// delegates the limits to the AFM/WBM.
    pub planning_cg_envelope: Option<PlanningCgEnvelope>,
    /// Revision-locked primary documents supporting this record.
    pub sources: Vec<&'static str>,
}

/// An aircraft preset that was asked for and is not registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown preset '{name}'; available: {}", available.join(", "))]
pub struct UnknownAircraftPreset {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// One complete aircraft configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftPreset {
    /// The key the configuration selects it by.
    pub name: &'static str,
    /// What the interface calls it.
    pub display_name: &'static str,
    /// What kind of aircraft it is, in one sentence.
    pub description: &'static str,
    /// Exact model, weight variant, engine and modification identity.
    pub identity: AircraftVariantIdentity,
    /// Revision-locked primary-source validation values.
    pub reference: AircraftReferenceData,
    /// Its position in the design space the optimizer searches.
    pub design_vector: DesignVector,
    /// Everything about its shape the design vector does not own.
    pub geometry: GeometryConfig,
    /// The mission it is sized for.
    pub requirements: DesignRequirements,
    /// Which engine it is fitted with.
    pub engine_name: &'static str,
    /// How many of them.
    pub n_engines: usize,
    /// Existing-aircraft landing-gear topology and track.
    pub landing_gear: LandingGearConfig,
    /// Legacy comparison inputs calibrated for this type, where the global
    /// compatibility model misses.
    ///
    /// These Torenbeek/fraction values are retained for the explicit
    /// reference-compatible comparison path. Pure production runs use the
    /// preset's source-backed FLOPS transport and structure inputs instead.
    /// `None` means the global compatibility values already land close enough.
    pub mass_model: Option<MassModelConfig>,
    /// Field-performance assumptions calibrated for this type.
    ///
    /// The high-lift system is what decides the takeoff and landing speeds,
    /// and the vortex-lattice analysis cannot see one. A widebody scored with
    /// a regional jet's flaps comes out ten to twenty knots fast on every
    /// V-speed. `None` means the global default fits.
    pub performance: Option<PerformanceConfig>,
}

/// Representative operating defaults used when an aircraft is selected.
///
/// These are high-demand or historically representative city pairs, not
/// source-backed aircraft design missions.  Keeping them outside
/// [`AircraftReferenceData`] prevents an interactive example from being
/// mistaken for payload/range validation evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationalMissionDefaults {
    /// Departure airport display name, resolvable through the airport registry.
    pub departure_airport: &'static str,
    /// Arrival airport display name, resolvable through the airport registry.
    pub arrival_airport: &'static str,
    /// Route-appropriate requested final cruise altitude, in metres MSL.
    pub cruise_altitude_m: f64,
    /// Route-appropriate cruise Mach command.
    pub cruise_mach: f64,
    /// Representative gross route payload when the route requires a payload/fuel trade.
    pub route_payload_kg: Option<f64>,
    /// Aircraft-appropriate mission schedule for the representative route.
    pub profile: crate::MissionProfileConfig,
    /// Why this city pair is representative and where that claim came from.
    pub provenance: &'static str,
}

impl AircraftPreset {
    /// Cabin seed used for the registered aircraft's generic planning load
    /// case.
    ///
    /// The registry does not claim an operator-specific LOPA: the real
    /// aircraft may be delivered with several cabin mixes, and the AFM/WBM
    /// remains the authority for an actual dispatch load sheet. These seeds
    /// only make the published passenger target representable by the common
    /// geometry engine. A single-class economy seed is deliberately used for
    /// the narrowbody, regional, and A340 targets because the generic
    /// widebody lie-flat business block is not a valid default for those
    /// bodies. Users can still edit the target shares while the cabin preset
    /// is `Custom`.
    pub fn planning_cabin_config(&self) -> crate::CabinConfig {
        let mut cabin = crate::CabinConfig::default();
        // One declaration of the hold architecture, read here and by the FLOPS
        // container tare alike. Writing the two out separately lets them
        // disagree: only the A220-300 carried the bulk cabin declaration,
        // while `declared_cargo_loading` also declares the ATR 72-600 and the
        // A320-200 bulk. The ATR 72-600 has no lower hold at all (ATR 72-600
        // factsheet p. 22) and was still being offered LD3 positions in one.
        if crate::preset_flops::declared_cargo_loading(self.name) == crate::CargoHoldLoading::Bulk {
            cabin.cargo.lower_deck_uld = "BLK".to_owned();
        }
        match self.name {
            "A220-300" | "A320-200" | "A340-300" | "ATR72-600" => {
                cabin.passenger.set_length_share_mix(&[("Economy", 1.0)]);
            }
            _ => {}
        }
        if self.name == "A320-200" {
            // Airbus A320 ACAP Figure 2-4-1: 28/29 in pitch for the 180-seat
            // single-class arrangement; the 28 in lower bound is used.
            cabin.passenger.economy.pitch_m = 0.7112;
        }
        if self.name == "ATR72-600" {
            // The official ATR 72-600 72-seat layout uses two Type-III exit
            // pairs. The generic spacing proxy otherwise floors the 19.166 m
            // passenger stretch to one pair (70 seats). 9.5 m is the smallest
            // transparent spacing that represents two pairs in this
            // preliminary geometry model; it is not a certification value.
            cabin.passenger.min_exit_pair_spacing_m = 9.5;
        }
        cabin
    }

    /// Where each engine hangs along the span, in metres from the centerline.
    pub fn engine_spanwise_positions(&self) -> &[f64] {
        &self.geometry.engine.spanwise_positions_m
    }

    /// Representative route and speed schedule loaded by interactive clients.
    pub fn operational_mission_defaults(&self) -> OperationalMissionDefaults {
        let (departure_airport, arrival_airport, cruise_altitude_m, cruise_mach, provenance) = match self.name {
            "A220-300" => (
                "Riga (EVRA)",
                "Stockholm Arlanda (ESSA)",
                25_000.0 * 0.3048,
                0.74,
                "airBaltic 30-year route history: Stockholm is one of its most popular Riga routes; airBaltic operates an all-A220-300 fleet (accessed 2026-08-29)",
            ),
            "ATR72-600" => (
                "Madrid Barajas (LEMD)",
                "Palma de Mallorca (LEPA)",
                17_000.0 * 0.3048,
                // Internal-consistency correction, not a calibration to the
                // factsheet: this preset's `requirements.cruise_mach` (the
                // design/sizing cruise condition) is 0.44; this operational
                // route default previously used a separately chosen 0.40
                // with no stated reason. Using the same 0.44 here removes
                // that unexplained mismatch. It happens to land close to the
                // ATR 72-600 factsheet's 275 KTAS at 95% MTOW/ISA/optimum FL
                // (~273.5 KTAS at 17,000 ft ISA), but the factsheet does not
                // state which FL is "optimum," so that agreement is not
                // evidence of validation, see an internal ATR performance
                // study (2026-09-07).
                0.44,
                "Representative European regional-sector default; operational example only, not an ATR design-mission claim",
            ),
            "A320-200" => (
                "Madrid Barajas (LEMD)",
                "Palma de Mallorca (LEPA)",
                28_000.0 * 0.3048,
                0.74,
                "Aena 2025 traffic reporting identifies Madrid among Palma's principal connections; representative A320-family short-haul pairing (accessed 2026-08-29)",
            ),
            "A340-300" => (
                "Frankfurt (EDDF)",
                "Boston Logan (KBOS)",
                39_000.0 * 0.3048,
                0.82,
                "Lufthansa 2026 timetable publishes ten weekly Frankfurt-Boston flights and 5,889 km route distance; representative remaining A340-300 operation (accessed 2026-08-29)",
            ),
            "A380-800" => (
                "Dubai (OMDB)",
                "London Heathrow (EGLL)",
                39_000.0 * 0.3048,
                0.83,
                "Emirates identifies Dubai-London Heathrow as a high-frequency A380 market (accessed 2026-08-29)",
            ),
            "B787-9" => (
                "Tokyo Haneda (RJTT)",
                "Sydney (YSSY)",
                41_000.0 * 0.3048,
                0.85,
                "ANA lists Sydney among the principal Haneda routes for its Boeing 787-9 (accessed 2026-08-29)",
            ),
            "DC-10" => (
                "Osaka Kansai (RJBB)",
                "Honolulu (PHNL)",
                37_000.0 * 0.3048,
                0.82,
                "Northwest Airlines 1996-10-27 timetable explicitly assigns DC-10 equipment to Osaka-Honolulu; historical because scheduled passenger DC-10 service has ended",
            ),
            // AVE is a synthetic reference aircraft and has no real demand history.
            _ => (
                "London Heathrow (EGLL)",
                "Dubai (OMDB)",
                39_000.0 * 0.3048,
                0.84,
                "Synthetic AVE reference route; no real-world subtype demand claim",
            ),
        };

        let mut profile = crate::MissionProfileConfig::default();
        let atmosphere = alas_atmo::Atmosphere::new(cruise_altitude_m);
        let cruise_tas_m_s = cruise_mach * atmosphere.speed_of_sound();
        profile.cruise_1_air_speed_m_s = cruise_tas_m_s;
        profile.cruise_2_air_speed_m_s = cruise_tas_m_s;
        profile.cruise_3_air_speed_m_s = cruise_tas_m_s;

        if self.name == "A320-200" {
            apply_a320_200_speed_schedule(
                &mut profile,
                cruise_altitude_m,
                self.requirements.mtow_kg,
                self.reference
                    .reference_wing_area_m2
                    .unwrap_or(self.requirements.max_wing_area_m2),
                self.performance.as_ref().map_or_else(
                    || crate::PerformanceConfig::default().cl_max_to,
                    |p| p.cl_max_to,
                ),
            );
        }
        if self.name == "ATR72-600" {
            apply_atr72_600_speed_schedule(
                &mut profile,
                self.requirements.mtow_kg,
                self.reference
                    .reference_wing_area_m2
                    .unwrap_or(self.requirements.max_wing_area_m2),
                self.performance.as_ref().map_or_else(
                    || crate::PerformanceConfig::default().cl_max_to,
                    |p| p.cl_max_to,
                ),
            );
        }

        OperationalMissionDefaults {
            departure_airport,
            arrival_airport,
            cruise_altitude_m,
            cruise_mach,
            // 250 occupied seats at a transparent preliminary 100 kg per
            // passenger including baggage. This is a representative dispatch
            // load, not a claim about the historical flight's actual load sheet.
            route_payload_kg: (self.name == "DC-10").then_some(25_000.0),
            profile,
            provenance,
        }
    }
}

/// International knot, m/s.
const KNOT_M_S: f64 = 1852.0 / 3600.0;
/// Feet per minute, m/s.
const FT_MIN_M_S: f64 = 0.3048 / 60.0;
/// ISA sea-level density, kg/m^3, and standard gravity, m/s^2, for the
/// takeoff-speed derivation below (a sea-level, standard-day reference
/// stall speed, at which CAS, EAS and TAS coincide).
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;
/// CS-25.107(b) / FAR 25.107(b): V2 may not be less than 1.13 V_SR for a
/// two-engine turbopropeller aeroplane. The regulatory minimum ratio, used
/// here as the scheduled ratio (an assumption: an operator's actual V2 is
/// tabulated per weight, flap setting and atmosphere from AFM data this
/// preset does not carry).
const ATR72_600_V2_OVER_VSR: f64 = 1.13;

/// The ATR 72-600 takeoff-segment speed, m/s CAS: `1.13 * V_SR` with the
/// reference stall speed taken at `mtow_kg`, `wing_area_m2` and the preset's
/// configured takeoff lift limit `cl_max_takeoff` on a sea-level standard
/// day, floored at the factsheet's published V2 min of 116 KCAS.
///
/// The lift limit is the preset's `conservative_simple_flaps` modelling
/// value, not ATR high-lift data, so the result (about 135 KCAS at 23 t,
/// 61 m^2 and CL_max 1.6) is a model-consistent operational speed under a
/// declared assumption, and the mission deck's takeoff lift check then
/// holds with the 1.13^2 margin. It is not a validated all-mass V2
/// schedule; replacing the lift limit with source-specific high-lift data
/// moves this speed with it.
pub fn atr72_600_takeoff_speed_m_s(mtow_kg: f64, wing_area_m2: f64, cl_max_takeoff: f64) -> f64 {
    let published_v2_min_m_s = 116.0 * KNOT_M_S;
    let stall_reference_m_s = (2.0 * mtow_kg * STANDARD_GRAVITY_M_S2
        / (ISA_SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_max_takeoff))
        .sqrt();
    (ATR72_600_V2_OVER_VSR * stall_reference_m_s).max(published_v2_min_m_s)
}

/// The ATR 72-600's takeoff/climb/descent/landing schedule, stated in
/// calibrated airspeed and resolved to true airspeed against the real
/// ambient state at each leg's own altitude by both mission paths.
///
/// `MissionProfileConfig::default()` is the long-range-widebody worked
/// example (128.6-250 m/s *true* airspeeds with a 10,000 ft takeoff
/// segment), which a PW127M-powered turboprop cannot fly: the mission deck
/// rejected it with a typed climb energy deficit at 991 m and 128.6 m/s.
///
/// Sourced values (ATR 72-600 factsheet, 2020, page 2):
/// - optimum climb speed 170 KCAS (initial climb and both step climbs);
/// - V2 min 116 KCAS. This is a published *minimum* at an unspecified
///   weight, configuration and atmosphere, not a V2 valid at every mass: at
///   this preset's MTOW it implies a takeoff lift coefficient above the
///   preset's configured takeoff limit (1.615 against 1.60), and the
///   mission deck rightly refuses to fly it. The takeoff segment therefore
///   flies [`atr72_600_takeoff_speed_m_s`], a speed derived from the
///   preset's own MTOW, reference wing area and configured takeoff lift
///   limit, with the published minimum kept only as a floor;
/// - approach speed 113 KIAS: an *indicated* airspeed, not a published
///   CAS. The landing leg flies 113 KCAS as an operational approximation
///   that ASSUMES ZERO position and instrument error (unsourced; on a
///   transport aircraft the difference is of the order of one to a few
///   knots). It is therefore not an independent CAS reference and must not
///   be used to validate the CAS conversion;
/// - sea-level, MTOW rate of climb 1,355 ft/min (requested for the takeoff
///   segment only; the deck's rating limit caps whatever cannot be
///   delivered).
///
/// Everything else below is an UNSOURCED profile assumption, chosen to be
/// operationally plausible for a 23 t turboprop cruising at FL170 and
/// labelled as such: the 1,500 ft AGL takeoff-segment top (a typical
/// acceleration altitude), the en-route climb rates, the whole descent
/// ladder (altitudes, calibrated speeds and rates: the factsheet publishes
/// no altitude-resolved climb or descent table) and the 3-degree-like
/// approach rate. None of it is calibrated to the factsheet's block fuel or
/// time figures; see an internal speed-schedule-integration study
/// (2026-09-07) for the comparison that was actually run.
fn apply_atr72_600_speed_schedule(
    profile: &mut crate::MissionProfileConfig,
    mtow_kg: f64,
    wing_area_m2: f64,
    cl_max_takeoff: f64,
) {
    profile.climb_descent_speed_reference = crate::SpeedReference::CalibratedAirspeed;
    // Takeoff segment: the derived V2 held to a 1,500 ft AGL acceleration
    // altitude at the published sea-level MTOW rate of climb.
    profile.takeoff_altitude_gain_m = 1_500.0 * 0.3048; // unsourced
    profile.takeoff_air_speed_m_s =
        atr72_600_takeoff_speed_m_s(mtow_kg, wing_area_m2, cl_max_takeoff);
    profile.takeoff_climb_rate_m_s = 1_355.0 * FT_MIN_M_S; // factsheet SL/MTOW ROC
                                                           // En-route climb at the published optimum climb speed; the rates are
                                                           // unsourced and below the sea-level figure because the deck's available
                                                           // power falls with altitude (a request above the rating is capped, not
                                                           // silently met).
    profile.initial_climb_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.initial_climb_rate_m_s = 1_000.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_1_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.step_climb_1_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_2_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.step_climb_2_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
                                                        // Descent ladder: entirely unsourced. Speeds are kept below the 250 KIAS
                                                        // class VMO with margin and step down towards the approach speed.
    profile.descent_1_altitude_ft = 10_000.0;
    profile.descent_1_air_speed_m_s = 220.0 * KNOT_M_S;
    profile.descent_1_rate_m_s = 1_500.0 * FT_MIN_M_S;
    profile.descent_2_altitude_ft = 6_000.0;
    profile.descent_2_air_speed_m_s = 200.0 * KNOT_M_S;
    profile.descent_2_rate_m_s = 1_200.0 * FT_MIN_M_S;
    profile.descent_3_altitude_ft = 3_000.0;
    profile.descent_3_air_speed_m_s = 170.0 * KNOT_M_S;
    profile.descent_3_rate_m_s = 1_000.0 * FT_MIN_M_S;
    profile.descent_4_altitude_ft = 1_500.0;
    profile.descent_4_air_speed_m_s = 140.0 * KNOT_M_S;
    profile.descent_4_rate_m_s = 800.0 * FT_MIN_M_S;
    // Final approach at the published approach speed on a nominal 3-degree
    // path (600 ft/min at ~113 kt ground speed; unsourced rate).
    // Factsheet 113 KIAS taken as 113 KCAS: zero position/instrument error
    // assumed (unsourced approximation, see above).
    profile.landing_air_speed_m_s = 113.0 * KNOT_M_S;
    profile.landing_descent_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
}

/// CS-25.107(b)(1)/FAR 25.107(b)(1): V2 may not be less than 1.13 V_SR for a
/// two-engine turbojet without provisions for obtaining a significant
/// reduction in one-engine-inoperative stall speed. The same regulatory
/// minimum the ATR helper above uses, for the same reason and with the same
/// caveat: an operator's V2 is tabulated per weight, flap setting and
/// atmosphere from AFM data this preset does not carry.
const NARROWBODY_V2_OVER_VSR: f64 = 1.13;

/// The A320-200's takeoff/climb/descent/landing schedule, stated in
/// calibrated airspeed and resolved to true airspeed against the real ambient
/// state at each leg's own altitude by both mission paths.
///
/// **Why this preset needs its own ladder, measured rather than assumed.**
/// `MissionProfileConfig::default()` is a *literal true airspeed* ladder
/// written for the AVE reference aircraft's design point (FL390, M0.84):
/// 250 m/s true on the upper climb rungs. A true airspeed is not a flight
/// condition. Applied to a narrowbody whose operational cruise is FL280, the
/// same number lands at about 178 m/s equivalent - roughly 345 kt, past what
/// an A320 climbs at by a third - and the mission deck refuses it with a typed
/// climb energy deficit: measured at 6 894 m and 250.0 m/s, **57 046 N of drag
/// against 56 106 N of maximum-climb rating**, so the aircraft cannot hold that
/// speed level, let alone climb 3 m/s at it. That single defect rejected
/// **253 of 253** A320-200 candidates in the all-preset matrix as
/// `dispatch_model_failed`. The aeroplane was never the problem; the schedule
/// was.
///
/// **What is sourced and what is not.** The speeds are the standard
/// air-transport climb and descent profile, which is a published operating
/// convention rather than a measurement of this airframe: 250 kt below
/// 10 000 ft (the regulatory speed limit), 300 kt above it, with no Mach
/// crossover rung because 300 kt calibrated reaches M0.78 near FL290, above
/// this preset's declared FL280 cruise, so one calibrated speed covers the
/// whole upper climb without an invented break point. Every vertical **rate**
/// is unsourced and representative, exactly as in the ATR helper above; none
/// of it is calibrated to a block-fuel or block-time figure. The takeoff
/// segment flies a V2 derived from this preset's own MTOW, reference wing
/// area and configured takeoff lift limit, so no speed is asserted that the
/// preset's own geometry does not support.
///
/// **A latent hazard this does not fix, stated here because it is the same
/// defect:** the shared default ladder is still literal true airspeed, and it
/// is still written for AVE's flight level. Any other preset flown at a lower
/// cruise altitude inherits the same over-speed, and the four widebodies that
/// currently close their dispatch do so because their operational levels
/// happen to sit near the one the default was written for, not because the
/// contract is right.
fn apply_a320_200_speed_schedule(
    profile: &mut crate::MissionProfileConfig,
    cruise_altitude_m: f64,
    mtow_kg: f64,
    wing_area_m2: f64,
    cl_max_takeoff: f64,
) {
    profile.climb_descent_speed_reference = crate::SpeedReference::CalibratedAirspeed;
    let transition_m = 10_000.0 * 0.3048;
    let stall_reference_m_s = (2.0 * mtow_kg * STANDARD_GRAVITY_M_S2
        / (ISA_SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_max_takeoff))
        .sqrt();

    // Takeoff segment: V2 held to a 1,500 ft AGL acceleration altitude.
    profile.takeoff_altitude_gain_m = 1_500.0 * 0.3048; // unsourced
    profile.takeoff_air_speed_m_s = NARROWBODY_V2_OVER_VSR * stall_reference_m_s;
    profile.takeoff_climb_rate_m_s = 2_500.0 * FT_MIN_M_S; // unsourced

    // The 250 kt speed limit below 10,000 ft, then 300 kt to the cruise
    // level. The rung boundaries are fractions of the cruise altitude, which
    // is the schema's own convention: they are set here so the break lands on
    // 10,000 ft for this preset's *declared* cruise level, and they move with
    // an edited cruise altitude the way every other preset's do.
    let transition_fraction = (transition_m / cruise_altitude_m).clamp(0.05, 0.9);
    profile.initial_climb_altitude_fraction = transition_fraction;
    profile.initial_climb_air_speed_m_s = 250.0 * KNOT_M_S;
    profile.initial_climb_rate_m_s = 2_500.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_1_altitude_fraction = (0.5 * (1.0 + transition_fraction)).clamp(0.1, 0.95);
    profile.step_climb_1_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.step_climb_1_rate_m_s = 1_800.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_2_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.step_climb_2_rate_m_s = 1_000.0 * FT_MIN_M_S; // unsourced

    // Descent ladder: the same 300/250 kt convention read downwards, stepping
    // to the approach speed. Altitudes and rates unsourced.
    profile.descent_1_altitude_ft = 20_000.0;
    profile.descent_1_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.descent_1_rate_m_s = 2_000.0 * FT_MIN_M_S;
    profile.descent_2_altitude_ft = 10_000.0;
    profile.descent_2_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.descent_2_rate_m_s = 2_000.0 * FT_MIN_M_S;
    profile.descent_3_altitude_ft = 5_000.0;
    profile.descent_3_air_speed_m_s = 250.0 * KNOT_M_S;
    profile.descent_3_rate_m_s = 1_500.0 * FT_MIN_M_S;
    profile.descent_4_altitude_ft = 3_000.0;
    profile.descent_4_air_speed_m_s = 210.0 * KNOT_M_S;
    profile.descent_4_rate_m_s = 1_000.0 * FT_MIN_M_S;

    // Final approach at 1.23 V_SR, the CS-25.125 reference landing approach
    // speed ratio, on a nominal 3-degree path (unsourced rate).
    profile.landing_air_speed_m_s = 1.23 * stall_reference_m_s;
    profile.landing_descent_rate_m_s = 700.0 * FT_MIN_M_S; // unsourced
}

/// Every registered aircraft, in the order the interface lists them.
pub fn registry() -> &'static [AircraftPreset] {
    static REGISTRY: OnceLock<Vec<AircraftPreset>> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

/// Look one aircraft up by name.
///
/// # Errors
///
/// [`UnknownAircraftPreset`], carrying what is available. Upstream raises for
/// the same input.
pub fn get(name: &str) -> Result<&'static AircraftPreset, UnknownAircraftPreset> {
    registry()
        .iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| UnknownAircraftPreset {
            name: name.to_owned(),
            available: sorted_names(),
        })
}

/// Every preset's name, in registration order.
pub fn available() -> Vec<&'static str> {
    registry().iter().map(|preset| preset.name).collect()
}

/// Every preset's name paired with what to call it, in registration order.
pub fn display_names() -> Vec<(&'static str, &'static str)> {
    registry()
        .iter()
        .map(|preset| (preset.name, preset.display_name))
        .collect()
}
