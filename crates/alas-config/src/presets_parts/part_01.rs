// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[path = "../presets/cg_envelope.rs"]
mod cg_envelope;
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
    /// Configuration-specific operating empty weight, when publicly available.
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
    /// A mass model calibrated for this type, where the global one misses.
    ///
    /// The Torenbeek fractions in [`MassModelConfig`] are calibrated around a
    /// modern widebody, and structural and furnishings mass does not scale
    /// linearly with weight: a small narrowbody carries a higher operating
    /// empty weight per unit of maximum takeoff weight than a widebody does.
    /// `None` means the global default already lands close enough.
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
        match self.name {
            "A220-300" | "A320-200" | "A340-300" | "ATR72-600" => {
                cabin.passenger.set_length_share_mix(&[("Economy", 1.0)]);
            }
            _ => {}
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
                0.40,
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
