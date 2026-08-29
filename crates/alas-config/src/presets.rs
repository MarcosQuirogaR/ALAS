// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py
// Reference: alas @ rust-port-baseline.

//! Real aircraft, as complete starting points.
//!
//! The other three registries in this crate bundle a handful of overrides on
//! one configuration struct. An entry here is a whole aeroplane: a design
//! vector, a geometry scaffold, a set of requirements, an engine, and -- for
//! the types the global assumptions do not fit -- its own mass-model and
//! field-performance calibration. Selecting one is how a user starts from
//! something that flies rather than from a blank form, and it is how this
//! program is checked against reality: an A320 that comes out sixteen tonnes
//! heavy is a visible failure in a way that a notional design never is.
//!
//! Dimensions come from published specification sheets. Where a manufacturer
//! does not publish root and tip chords, they are estimated from wing area,
//! aspect ratio, taper and sweep by standard planform relations, which is why
//! the two chord fields of a real type are the least certain numbers here.
//!
//! A registered type also carries its exact variant identity, revision-locked
//! reference data, and landing-gear topology. Those records distinguish a
//! documented aircraft configuration from an optimizer bound or a project
//! assumption; a missing public AFM/WBM value remains missing rather than
//! being inferred from unrelated planning data.
//!
//! # What a preset does not settle
//!
//! The first is the engine cycle. A preset names its engine and does not copy
//! the table entry in, so every one of them carries [`crate::EngineConfig`]'s
//! fallback cycle -- the GE9X's -- until something calls
//! [`crate::EngineConfig::apply_engine_spec`]. Upstream's geometry builder does
//! that as its first step, so nothing that goes through it ever sees the
//! stale numbers; anything that reads an unbuilt preset's thrust does. The
//! port reproduces that rather than resolving the spec at registration, since
//! resolving it here would make a preset disagree with the same preset loaded
//! from a saved file.
//!
//! Nor does a preset fit the design space it is offered in. The bounds in
//! [`crate::design_variables`] are one global set describing AVE's family, so
//! every published type here starts outside at least one of them -- an A320's
//! fuselage is twenty-eight metres shorter than the shortest the search will
//! consider. That is upstream's arrangement and not an oversight: whoever runs
//! a search narrows the bounds around the design it starts from, and the
//! optimizer reports an initial design that falls outside whatever bounds it
//! was handed.
//!
//! The remaining boundary is the two per-aircraft calibrations. They are
//! [`Option`]s, and
//! `None` means "use the global default" rather than "no calibration" --
//! [`crate::AlasConfig::from_value`] is where they are applied, because a
//! headless run that skipped them would silently revert an A220 to the
//! widebody-calibrated mass fractions its entry exists to correct.

mod cg_envelope;
mod narrowbody;
mod reference;
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
    /// Aircraft-appropriate mission schedule for the representative route.
    pub profile: crate::MissionProfileConfig,
    /// Why this city pair is representative and where that claim came from.
    pub provenance: &'static str,
}

impl AircraftPreset {
    /// Where each engine hangs along the span, in metres from the centerline.
    pub fn engine_spanwise_positions(&self) -> &[f64] {
        &self.geometry.engine.spanwise_positions_m
    }

    /// Representative route and speed schedule loaded by interactive clients.
    pub fn operational_mission_defaults(&self) -> OperationalMissionDefaults {
        let (departure_airport, arrival_airport, provenance) = match self.name {
            "A220-300" => (
                "Riga (EVRA)",
                "Stockholm Arlanda (ESSA)",
                "airBaltic 30-year route history: Stockholm is one of its most popular Riga routes; airBaltic operates an all-A220-300 fleet (accessed 2026-08-29)",
            ),
            "A320-200" => (
                "Madrid Barajas (LEMD)",
                "Palma de Mallorca (LEPA)",
                "Aena 2025 traffic reporting identifies Madrid among Palma's principal connections; representative A320-family short-haul pairing (accessed 2026-08-29)",
            ),
            "A340-300" => (
                "Frankfurt (EDDF)",
                "Boston Logan (KBOS)",
                "Lufthansa 2026 timetable publishes ten weekly Frankfurt-Boston flights and 5,889 km route distance; representative remaining A340-300 operation (accessed 2026-08-29)",
            ),
            "A380-800" => (
                "Dubai (OMDB)",
                "London Heathrow (EGLL)",
                "Emirates identifies Dubai-London Heathrow as a high-frequency A380 market (accessed 2026-08-29)",
            ),
            "B787-9" => (
                "Tokyo Haneda (RJTT)",
                "Sydney (YSSY)",
                "ANA lists Sydney among the principal Haneda routes for its Boeing 787-9 (accessed 2026-08-29)",
            ),
            "DC-10" => (
                "Osaka Kansai (RJBB)",
                "Honolulu (PHNL)",
                "Northwest Airlines 1996-10-27 timetable explicitly assigns DC-10 equipment to Osaka-Honolulu; historical because scheduled passenger DC-10 service has ended",
            ),
            // AVE is a synthetic reference aircraft and has no real demand history.
            _ => (
                "London Heathrow (EGLL)",
                "Dubai (OMDB)",
                "Synthetic AVE reference route; no real-world subtype demand claim",
            ),
        };

        let mut profile = crate::MissionProfileConfig::default();
        let atmosphere = alas_atmo::Atmosphere::new(self.requirements.cruise_altitude_m);
        let cruise_tas_m_s = self.requirements.cruise_mach * atmosphere.speed_of_sound();
        profile.cruise_1_air_speed_m_s = cruise_tas_m_s;
        profile.cruise_2_air_speed_m_s = cruise_tas_m_s;
        profile.cruise_3_air_speed_m_s = cruise_tas_m_s;

        OperationalMissionDefaults {
            departure_airport,
            arrival_airport,
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

/// The named high-lift technology level a preset is scored with.
///
/// Every preset states one, so a `None` here is a name that does not exist
/// rather than a type happy with the generic default; the registry's own
/// tests are what catch that, since falling back silently would revert a
/// widebody to a narrowbody's flaps and its V-speeds with them.
fn high_lift(name: &'static str) -> Option<PerformanceConfig> {
    match crate::performance_presets::get(name) {
        Ok(preset) => Some(preset.settings.clone()),
        Err(error) => {
            tracing::error!(%error, "an aircraft preset names an unregistered high-lift level");
            None
        }
    }
}

fn sorted_names() -> Vec<String> {
    let mut names: Vec<String> = registry()
        .iter()
        .map(|preset| preset.name.to_owned())
        .collect();
    names.sort();
    names
}

fn build() -> Vec<AircraftPreset> {
    let mut presets = vec![
        reference::ave(),
        widebody::a340_300(),
        widebody::a380_800(),
        widebody::b787_9(),
        narrowbody::a320_200(),
        narrowbody::a220_300(),
        widebody::dc_10(),
    ];

    // The engine name is stated on the preset and read off the geometry, and
    // two of the seven state it in both places. Copying it down here is what
    // makes the two agree for the other five, and is upstream's `__post_init__`.
    for preset in &mut presets {
        preset.geometry.engine.engine_name = preset.engine_name.to_owned();
    }
    presets
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dropdown_lists_the_aircraft_in_registration_order() {
        assert_eq!(
            available(),
            vec!["AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",]
        );
    }

    #[test]
    fn an_unknown_aircraft_is_an_error_that_says_what_there_is() {
        let error = get("Concorde").unwrap_err();
        assert!(error.available.contains(&"A320-200".to_owned()));
        assert!(error.to_string().contains("A320-200"));
    }

    #[test]
    fn every_preset_carries_its_engine_name_into_its_geometry() {
        // Five of the seven state it only on the preset, and everything
        // downstream reads it off the geometry.
        for preset in registry() {
            assert_eq!(
                preset.geometry.engine.engine_name, preset.engine_name,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn every_operational_default_uses_two_registered_airports() {
        for preset in registry() {
            let defaults = preset.operational_mission_defaults();
            assert_ne!(
                defaults.departure_airport, defaults.arrival_airport,
                "{}",
                preset.name
            );
            assert!(
                crate::airports::get(defaults.departure_airport).is_ok(),
                "{}: unknown departure {}",
                preset.name,
                defaults.departure_airport
            );
            assert!(
                crate::airports::get(defaults.arrival_airport).is_ok(),
                "{}: unknown arrival {}",
                preset.name,
                defaults.arrival_airport
            );
            assert!(defaults.profile.cruise_1_air_speed_m_s.is_finite());
            assert!(!defaults.provenance.is_empty());
        }
    }

    #[test]
    fn a_preset_mounts_exactly_as_many_engines_as_it_claims_to_have() {
        for preset in registry() {
            assert_eq!(
                preset.engine_spanwise_positions().len(),
                preset.n_engines,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn multi_bogie_aircraft_do_not_fall_back_to_the_two_leg_weight_threshold() {
        assert_eq!(get("A340-300").unwrap().landing_gear.n_mlg_struts, 3);
        assert_eq!(get("A380-800").unwrap().landing_gear.n_mlg_struts, 4);
        assert_eq!(get("DC-10").unwrap().landing_gear.n_mlg_struts, 3);
    }

    #[test]
    fn a_preset_still_carries_the_fallback_cycle_until_the_builder_resolves_it() {
        // Nothing applies the engine table at registration, so an A320's
        // thrust reads as a GE9X's until the geometry builder runs. Stated as
        // a test because it is surprising and load-bearing: a consumer that
        // reads an unbuilt preset's thrust gets a widebody's.
        let a320 = get("A320-200").unwrap();
        assert_eq!(a320.geometry.engine.thrust_kn, 467.0);

        let mut resolved = a320.geometry.engine.clone();
        resolved.apply_engine_spec();
        assert!(resolved.thrust_kn < 200.0, "{}", resolved.thrust_kn);
    }

    #[test]
    fn every_engine_the_presets_name_is_one_the_table_carries() {
        // The name is a selector, and one the table does not carry leaves the
        // GE9X fallback in place for good -- a silent widebody engine on
        // whatever type mistyped it.
        for preset in registry() {
            assert!(
                crate::engines::get(preset.engine_name).is_ok(),
                "{}: no engine called {}",
                preset.name,
                preset.engine_name
            );
        }
    }

    #[test]
    fn every_real_preset_names_one_coherent_weight_variant() {
        for preset in registry().iter().filter(|preset| preset.name != "AVE") {
            assert!(!preset.identity.model.is_empty(), "{}", preset.name);
            assert!(
                !preset.identity.weight_variant.is_empty(),
                "{}",
                preset.name
            );
            assert!(!preset.identity.engine_model.is_empty(), "{}", preset.name);
            assert!(!preset.reference.sources.is_empty(), "{}", preset.name);
            assert_eq!(
                preset.reference.mtow_kg,
                Some(preset.requirements.mtow_kg),
                "{} mixes its public run weight with another weight variant",
                preset.name
            );
        }
    }

    #[test]
    fn every_passenger_preset_uses_candidate_geometry_for_capacity() {
        for preset in registry().iter().filter(|preset| preset.name != "AVE") {
            assert_eq!(
                preset.requirements.cabin_preset, "Custom",
                "{} has no sourced operator class layout",
                preset.name
            );
            assert!(
                preset.requirements.optimize_passenger_capacity,
                "{}",
                preset.name
            );
        }

        let ave = get("AVE").unwrap();
        assert!(ave.requirements.optimize_passenger_capacity);
    }

    #[test]
    fn every_preset_leaves_design_mission_compliance_unverified_without_a_source() {
        for preset in registry() {
            assert_eq!(
                preset.reference.design_mission_evidence,
                DesignMissionEvidence::Unverified,
                "{} must not inherit the application's default route as evidence",
                preset.name
            );
        }
    }

    #[test]
    fn only_public_planning_evidence_carries_a_planning_curve() {
        for preset in registry() {
            match preset.reference.cg_evidence {
                CgEnvelopeEvidence::PublicPlanning => assert!(
                    preset.reference.planning_cg_envelope.is_some(),
                    "{}",
                    preset.name
                ),
                CgEnvelopeEvidence::AfmRequired => assert!(
                    preset.reference.planning_cg_envelope.is_none(),
                    "{} must not invent limits the AFM/WBM owns",
                    preset.name
                ),
                CgEnvelopeEvidence::DesignRequirement | CgEnvelopeEvidence::Unknown => {}
            }
        }
    }

    #[test]
    fn only_the_reference_twin_fits_inside_the_unnarrowed_design_space() {
        // The design space's bounds describe AVE's family and nothing else, so
        // a real type of any other size starts outside them. Stated as a test
        // because it looks like a defect and is not: whoever runs a search
        // narrows the bounds around the design it starts from.
        let inside = |preset: &AircraftPreset| {
            preset
                .design_vector
                .to_array()
                .iter()
                .zip(crate::DESIGN_VARIABLE_SPECS)
                .all(|(value, spec)| *value >= spec.lower && *value <= spec.upper)
        };
        let fitting: Vec<&str> = registry()
            .iter()
            .filter(|preset| inside(preset))
            .map(|preset| preset.name)
            .collect();
        assert_eq!(fitting, vec!["AVE"]);
    }

    #[test]
    fn only_the_types_the_global_assumptions_miss_carry_their_own_calibration() {
        // A calibration on every preset would mean the defaults describe
        // nothing; one on none of them would mean the small narrowbody comes
        // out several tonnes light.
        let calibrated: Vec<&str> = registry()
            .iter()
            .filter(|preset| preset.mass_model.is_some())
            .map(|preset| preset.name)
            .collect();
        assert_eq!(calibrated, vec!["A220-300"]);
    }

    #[test]
    fn every_preset_states_a_high_lift_system_rather_than_inheriting_one() {
        // The default performance configuration is a generic narrowbody, and
        // it fits none of these seven well enough to leave unstated.
        for preset in registry() {
            assert!(preset.performance.is_some(), "{}", preset.name);
        }
    }
}
