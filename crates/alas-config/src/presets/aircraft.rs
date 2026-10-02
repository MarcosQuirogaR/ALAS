// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`AircraftPreset`] and the reference data a registered aircraft carries.

use super::speed_schedules::{apply_a320_200_speed_schedule, apply_atr72_600_speed_schedule};
use super::{
    CgEnvelopeEvidence, DesignMissionEvidence, PartialDesignMissionEvidence,
    PayloadRangeDesignPoint, PlanningCgEnvelope, PublishedAftCgNoseLoad,
};
use crate::{
    DesignRequirements, DesignVector, GeometryConfig, LandingGearConfig, MassModelConfig,
    PerformanceConfig,
};

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

/// One source-defined emergency-exit pair.
///
/// The pair carries its CS 25.807(a) type letter and nothing about how many
/// passengers it may evacuate: the rating of a complete pair is a property
/// of the type (CS 25.807(g)), and the payload crate derives it from the
/// letter, so a preset cannot declare a rating that disagrees with the
/// regulation it cites.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CertifiedExitPair {
    /// Exit class printed in the source cabin configuration.
    pub exit_type: &'static str,
    /// Station of the door centre, m aft of the nose tip on the body the
    /// source draws, when the source prints one.
    pub station_m: Option<f64>,
}

/// A revision-locked exit arrangement for a registered aircraft variant.
///
/// This is source metadata used to keep a product preset's cabin topology
/// separate from the generic diameter heuristic.  It is not a declaration
/// that the layout engine has demonstrated the aircraft's certified
/// evacuation performance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CertifiedExitLayout {
    /// Human-readable pair sequence, for example `C-III-C`.
    pub label: &'static str,
    /// Exit pairs in source order, forward to aft.
    pub pairs: &'static [CertifiedExitPair],
    /// Overall body length of the drawing the pair stations are measured
    /// on, m. Stations are only usable together with it, because a built
    /// body of another length moves the aft doors with the tail.
    pub station_body_length_m: Option<f64>,
    /// Exact source, revision and location for the arrangement.
    pub source: &'static str,
}

impl CertifiedExitLayout {
    /// Whether every pair carries a station on a declared body length, which
    /// is what lets the cabin be bounded by the doors rather than by the
    /// generic nose and tail-cone lengths.
    pub fn has_stations(&self) -> bool {
        self.station_body_length_m
            .is_some_and(|length| length > 0.0)
            && self.pairs.len() >= 2
            && self
                .pairs
                .iter()
                .all(|pair| pair.station_m.is_some_and(f64::is_finite))
    }
}

/// One class of a source's typical cabin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourcedSeatClass {
    /// `"First"`, `"Business"` or `"Economy"`.
    pub class: &'static str,
    /// Seats of this class in the source arrangement.
    pub seats: i64,
    /// Seat pitch, m, when the source prints one.
    pub pitch_m: Option<f64>,
    /// Seats abreast, when the source prints them.
    pub abreast: Option<i64>,
    /// Seat width including armrests, m, when the source prints one.
    pub width_m: Option<f64>,
}

/// The typical cabin a manufacturer publishes for a registered aircraft:
/// seats per class and, where printed, their pitch, abreast and width.
///
/// Selecting the preset seeds the cabin with these seat shares and this seat
/// geometry; every value the source does not print keeps the generic class
/// default. It is a planning cabin, not an operator's approved LOPA.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourcedPlanningCabin {
    /// The classes, forward to aft.
    pub classes: &'static [SourcedSeatClass],
    /// Exact source, revision and location.
    pub source: &'static str,
}

impl SourcedPlanningCabin {
    /// Seats across every class.
    pub fn total_seats(&self) -> i64 {
        self.classes.iter().map(|class| class.seats.max(0)).sum()
    }
}

/// Primary-source values against which one preset is validated.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AircraftReferenceData {
    /// ICAO Annex 14 aerodrome reference code letter of the type, from its
    /// wingspan against Table 1-1. Bounds the span of a reference
    /// adaptation.
    pub aerodrome_reference_code: Option<crate::AerodromeReferenceCode>,
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
    /// The manufacturer's typical cabin, seats and seat geometry per class.
    pub planning_cabin: Option<SourcedPlanningCabin>,
    /// Whether a complete design-mission definition has source provenance.
    pub design_mission_evidence: DesignMissionEvidence,
    /// Relevant public range/mission material that is not a complete mission.
    pub partial_design_mission_evidence: Vec<PartialDesignMissionEvidence>,
    /// Charted design range and payload the takeoff-mass design modes close on.
    pub design_point: Option<PayloadRangeDesignPoint>,
    /// What kind of CG evidence is publicly available.
    pub cg_evidence: CgEnvelopeEvidence,
    /// Published planning curve, when the source provides one.
    ///
    /// This is deliberately absent for presets whose type-certificate source
    /// delegates the limits to the AFM/WBM.
    pub planning_cg_envelope: Option<PlanningCgEnvelope>,
    /// Static gear-load split the airport-planning document tabulates at the
    /// most-aft CG; it sets the ground minimum nose-gear load in place of the
    /// class default (see [`PublishedAftCgNoseLoad`]).
    pub aft_cg_nose_load: Option<PublishedAftCgNoseLoad>,
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
    /// Why `geometry.wing.airfoil_class` is what it is: the source or
    /// design-era basis of the declared section technology, which fixes the
    /// Korn technology factor.
    pub airfoil_class_source: &'static str,
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
    /// Field-performance assumptions and source-backed inputs for this type.
    ///
    /// The high-lift system is what decides the takeoff and landing speeds,
    /// and the vortex-lattice analysis cannot see one. Published landing-speed
    /// inputs recover an effective CLmax on their stated mass and reference
    /// area; other field inputs remain conceptual assumptions. `None` uses
    /// the global defaults.
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

/// Seed a cabin with a source's typical arrangement: its seat shares as the
/// target passenger mix, and every seat dimension the source prints.
///
/// The shares are seat shares, which is what the product layout reads them
/// as; the layout solves the floor allocation that reproduces them.
fn apply_sourced_planning_cabin(
    passenger: &mut crate::PassengerCabinConfig,
    planning: &SourcedPlanningCabin,
) {
    let total = planning.total_seats();
    if total <= 0 {
        return;
    }
    let mix: Vec<(&str, f64)> = planning
        .classes
        .iter()
        .filter(|class| class.seats > 0)
        .map(|class| (class.class, class.seats as f64 / total as f64))
        .collect();
    passenger.set_length_share_mix(&mix);
    for class in planning.classes {
        let slot = match class.class {
            "First" => &mut passenger.first,
            "Business" => &mut passenger.business,
            _ => &mut passenger.economy,
        };
        if let Some(pitch_m) = class.pitch_m {
            slot.pitch_m = pitch_m;
        }
        if let Some(abreast) = class.abreast {
            slot.abreast = abreast;
        }
        if let Some(width_m) = class.width_m {
            slot.width_m = width_m;
        }
    }
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
            "A220-300" | "A320-200" | "A340-300" => {
                cabin.passenger.set_length_share_mix(&[("Economy", 1.0)]);
            }
            _ => {}
        }
        if self.name == "A320-200" {
            // Airbus A320 ACAP Figure 2-4-1: 28/29 in pitch for the 180-seat
            // single-class arrangement; the 28 in lower bound is used.
            cabin.passenger.economy.pitch_m = 0.7112;
        }
        if let Some(planning) = self.reference.planning_cabin {
            apply_sourced_planning_cabin(&mut cabin.passenger, &planning);
        }
        if self.name == "ATR72-600" {
            cabin.cargo.hold_compartments = super::ATR72_600_BAGGAGE_COMPARTMENTS
                .iter()
                .map(|&(name, x_start_m, x_end_m)| crate::HoldCompartmentConfig {
                    name: name.to_owned(),
                    x_start_m,
                    x_end_m,
                    volume_m3: None,
                    max_net_kg: None,
                    deck: crate::HoldDeck::Main,
                })
                .collect();
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
                // FL200, not the prior FL170: at FL170 the PW127M/568F deck
                // sits at 99% rated power at this design Mach, above the
                // factsheet's max-cruise fuel flow; FL200's thinner air
                // lowers the required power. Internal ATR study, 2026-09-07.
                20_000.0 * 0.3048,
                // Internal-consistency correction: `requirements.cruise_mach`
                // (design/sizing) is 0.44; the operational default used an
                // unexplained 0.40. 0.44 removes that mismatch (close to the
                // factsheet's 275 KTAS, but not itself validation evidence).
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
