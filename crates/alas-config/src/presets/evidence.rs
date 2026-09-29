// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed provenance for a preset's CG envelope and design mission.

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
