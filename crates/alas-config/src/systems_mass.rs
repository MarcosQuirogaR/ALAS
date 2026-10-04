// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product selection and declared architecture for transport subsystem mass.
//!
//! The Python-compatible buildup assigns systems and furnishings as fractions
//! of maximum takeoff mass. A component method needs inputs that those two
//! fractions cannot represent: range, crew, cabin class mix, hydraulic
//! pressure, engine installation, and fuel-system topology. Keeping them in a
//! separate, versioned contract prevents a missing physical input from being
//! replaced silently by the compatibility fractions.

use serde::{Deserialize, Serialize};

use crate::{CabinEquipmentMethod, ConfigNode, Kind, Leaf};

/// Versioned method used for systems, equipment, and operating-item mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemsMassMethod {
    /// Frozen `alas/physics/mass.py` fractions of maximum takeoff mass.
    #[default]
    ReferenceCompatibleFractions,
    /// NASA FLOPS transport correlations documented in TM-2017-219627 Vol. I.
    FlopsTransportV1,
}

impl SystemsMassMethod {
    /// Whether this selection preserves the frozen Python mass behavior.
    pub fn is_reference_compatible(&self) -> bool {
        matches!(self, Self::ReferenceCompatibleFractions)
    }
}

impl Leaf for SystemsMassMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// How an aircraft's cargo compartments are loaded.
///
/// FLOPS equations 125-126 charge one 175 lb container for every 950 lb of
/// containerised mass, and NASA Aviary's decks feed that equation the checked
/// baggage as well as the revenue cargo. The tare is real hardware - a unit
/// load device - so it exists only on an aircraft whose holds take one. A
/// regional turboprop or a narrowbody with loose-loaded ("bulk") holds carries
/// no ULD at all, and charging it one would put roughly a quarter of a tonne
/// of equipment into an operating empty mass that never contains it.
///
/// This is an aircraft architecture statement, not a study assumption, and it
/// is declared per registered aircraft from the manufacturer's airport
/// planning document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CargoHoldLoading {
    /// Loose-loaded holds. Baggage and cargo are netted directly to the hold
    /// floor, so there is no container tare to report at all.
    Bulk,
    /// Unit-load-device holds. Every checked bag and every kilogram of
    /// declared cargo rides in a container, which is the case FLOPS
    /// equations 125-126 were written for.
    #[default]
    Containerized,
    /// Containerised main holds plus a loose-loaded bulk compartment, which is
    /// the usual widebody arrangement. The containerised share of the checked
    /// baggage must then be declared in
    /// [`FlopsTransportConfig::containerized_baggage_fraction`]; declared
    /// cargo stays containerised by definition.
    Mixed,
}

impl CargoHoldLoading {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bulk => "bulk",
            Self::Containerized => "containerized",
            Self::Mixed => "mixed",
        }
    }
}

impl Leaf for CargoHoldLoading {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// What kind of knowledge a family of declared FLOPS inputs rests on.
///
/// Provenance strings say *where* a number came from; this says *what it is*.
/// The distinction matters because a run can be complete and still be built
/// largely on declared study values, and a coverage report that cannot tell
/// a certification datum from an engineering estimate is not a coverage
/// report. Nothing here ranks accuracy: a source-backed input can still be
/// the wrong quantity for the variant, which is what the applicability
/// statement is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlopsInputEvidence {
    /// Read from a manufacturer, certification-authority or NASA document,
    /// for this variant, at the locator the provenance names.
    SourceBacked,
    /// Chosen by whoever configured this run. Legitimate for a notional
    /// design and for a study scenario; it is not an aircraft fact.
    UserDeclared,
    /// A default printed in NASA/TM-2017-219627 itself, used as the source
    /// intends rather than as a substitute for a missing measurement.
    PublishedFlopsDefault,
    /// An engineering estimate with no document behind it. The weakest
    /// category, and the default, so an undeclared evidence kind can never
    /// read as stronger than it is.
    #[default]
    UncertainEngineeringEstimate,
}

impl FlopsInputEvidence {
    /// Stable machine-readable name for exports and evidence artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceBacked => "source_backed",
            Self::UserDeclared => "user_declared",
            Self::PublishedFlopsDefault => "published_flops_default",
            Self::UncertainEngineeringEstimate => "uncertain_engineering_estimate",
        }
    }
}

impl Leaf for FlopsInputEvidence {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Revision-locked evidence for one family of FLOPS inputs.
///
/// A document title alone cannot show that its numbers apply to the configured
/// variant. The revision, locator, and applicability statement keep declared
/// values auditable without promoting a marketing value to a design datum.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FlopsInputProvenance {
    /// Title or identifier of the controlling source document.
    #[config(
        advanced,
        label = "Source document",
        help = "Controlling document for this FLOPS input family."
    )]
    pub document: String,
    /// Published revision, issue, or release date of the source.
    #[config(
        advanced,
        label = "Source revision",
        help = "Revision, issue, or release date that fixes the source value."
    )]
    pub revision: String,
    /// Page, table, figure, or data-module locator within the source.
    #[config(
        advanced,
        label = "Source location",
        help = "Page, table, figure, or data-module locator for the declared values."
    )]
    pub location: String,
    /// Why this source applies to the configured variant and installation.
    #[config(
        advanced,
        label = "Source applicability",
        help = "Variant, installation, or mission applicability of the cited source."
    )]
    pub applicability: String,
    /// What kind of knowledge this family rests on.
    #[config(
        advanced,
        options = FlopsInputEvidence,
        label = "Evidence kind",
        help = "Whether these values are read from a manufacturer, certification or NASA document for this variant, declared by whoever configured the run, a published FLOPS default, or an uncertain engineering estimate. The weakest kind is assumed when none is stated."
    )]
    pub evidence: FlopsInputEvidence,
    /// Stated uncertainty, or why none can be stated.
    ///
    /// Free text because the families mix units: a Mach number, a seat count
    /// and a hydraulic pressure do not share a band. An empty string is
    /// itself a finding; it means the run cannot say how wrong these
    /// numbers might be.
    #[config(
        advanced,
        label = "Stated uncertainty",
        help = "Uncertainty band for this input family, in its own units, or an explicit statement that none could be established. Left blank, the coverage report records that this family carries no uncertainty statement."
    )]
    pub uncertainty: String,
}

impl FlopsInputProvenance {
    /// Whether every field needed to audit an input family is present.
    ///
    /// The evidence kind is deliberately *not* part of this test. A run built
    /// on declared study values is auditable and may proceed; what it may not
    /// do is present those values as aircraft data, which is what
    /// [`Self::evidence`] and the coverage report are for.
    pub fn is_declared(&self) -> bool {
        !self.document.trim().is_empty()
            && !self.revision.trim().is_empty()
            && !self.location.trim().is_empty()
            && !self.applicability.trim().is_empty()
    }
}

/// Evidence carried with a completed FLOPS transport evaluation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FlopsTransportProvenance {
    /// Evidence for maximum Mach and source-defined design range.
    #[config(
        nested,
        advanced,
        help = "Revision-locked evidence for the FLOPS maximum-Mach and design-range inputs."
    )]
    pub mission: FlopsInputProvenance,
    /// Evidence for installed crew and FLOPS passenger-class counts.
    #[config(
        nested,
        advanced,
        help = "Revision-locked evidence for the installed cabin, crew, and class mapping."
    )]
    pub cabin: FlopsInputProvenance,
    /// Evidence for hydraulics, sweep, engine mounting, fuel system, and cargo.
    #[config(
        nested,
        advanced,
        help = "Revision-locked evidence for the installed systems, fuel topology, engine mounting, and cargo inputs."
    )]
    pub architecture: FlopsInputProvenance,
}

impl FlopsTransportProvenance {
    /// Whether every non-geometric FLOPS input family carries its evidence.
    pub fn is_complete(&self) -> bool {
        self.mission.is_declared() && self.cabin.is_declared() && self.architecture.is_declared()
    }
}

/// Which of the two LTH operating-item relations an aircraft takes.
///
/// The source publishes one relation for short/medium-haul and a second for
/// long-haul rather than a range threshold, so the class is declared per
/// aircraft instead of being inferred from a cut-off this project would have
/// invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingHaulClass {
    /// `m_opp = 32.907 n_pax^1.021`.
    #[default]
    ShortMediumHaul,
    /// `m_opp = 35.782 n_pax^1.1141`.
    LongHaul,
}

impl OperatingHaulClass {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShortMediumHaul => "short_medium_haul",
            Self::LongHaul => "long_haul",
        }
    }
}

impl Leaf for OperatingHaulClass {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Physical inputs that the FLOPS transport equations cannot derive from the
/// built outer geometry.
///
/// Every field is optional because an old configuration has none of them.
/// The evaluator reports each absent datum as unverified; these options are
/// not invitations to substitute a default correlation or preset constant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FlopsTransportConfig {
    /// Maximum operating or design Mach number used by the FLOPS fits.
    #[config(
        advanced,
        label = "Maximum Mach number",
        help = "FLOPS VMAX: the maximum operating or design Mach number. Cruise Mach is not substituted because the two are different certification quantities."
    )]
    pub maximum_mach: Option<f64>,
    /// Aircraft design range in nautical miles, as required by FLOPS.
    #[config(
        advanced,
        label = "Design range",
        unit = "nmi",
        help = "FLOPS DESRNG from a revision-locked aircraft design source. A marketing maximum or the selected airport route is not silently substituted."
    )]
    pub design_range_nmi: Option<f64>,
    /// Number of pilots and other flight-deck flight crew.
    #[config(
        advanced,
        label = "Flight crew count",
        help = "Installed flight-deck crew used by the FLOPS instruments, electrical, avionics, furnishings, and operating-item equations."
    )]
    pub flight_crew_count: Option<usize>,
    /// Number of cabin flight attendants.
    #[config(
        advanced,
        label = "Flight attendant count",
        help = "Installed cabin flight attendants. No passenger-ratio default is applied by the physical product method."
    )]
    pub flight_attendant_count: Option<usize>,
    /// Number of crew dedicated to galley service.
    #[config(
        advanced,
        label = "Galley crew count",
        help = "Installed galley-service crew. Enter zero explicitly when the aircraft carries none."
    )]
    pub galley_crew_count: Option<usize>,
    /// Number of first-class passengers in the installed cabin.
    #[config(
        advanced,
        label = "First-class passenger count",
        help = "Seed/mirror for direct FLOPS calls. Passenger product cases derive the installed first-class count from Cabin class configuration; edit the Cabin class layout to change seats."
    )]
    pub first_class_passenger_count: Option<usize>,
    /// Number of business-class passengers in the installed cabin.
    #[config(
        advanced,
        label = "Business-class passenger count",
        help = "Seed/mirror for direct FLOPS calls. Passenger product cases derive the installed business-class count from Cabin class configuration; edit the Cabin class layout to change seats."
    )]
    pub business_class_passenger_count: Option<usize>,
    /// Number of tourist/economy passengers in the installed cabin.
    #[config(
        advanced,
        label = "Tourist-class passenger count",
        help = "Seed/mirror for direct FLOPS calls. Passenger product cases derive the installed economy/tourist count from Cabin class configuration; edit the Cabin class layout to change seats."
    )]
    pub tourist_class_passenger_count: Option<usize>,
    /// Hydraulic system working pressure, in pascals.
    #[config(
        advanced,
        label = "Hydraulic system pressure",
        unit = "Pa",
        help = "Declared hydraulic working pressure. The FLOPS 3,000 psi reference appears inside its equation; it is not a missing-input default."
    )]
    pub hydraulic_pressure_pa: Option<f64>,
    /// FLOPS variable-sweep penalty: 0 fixed wing, 1 full variable sweep.
    #[config(
        advanced,
        label = "Variable-sweep penalty",
        unit = "0-1",
        help = "FLOPS VARSWP: enter 0 for a declared fixed wing and 1 for full variable sweep. Absence is not treated as fixed geometry."
    )]
    pub variable_sweep_penalty: Option<f64>,
    /// Number of engines installed on the wing.
    #[config(
        advanced,
        label = "Wing-mounted engine count",
        help = "FLOPS FNEW. This is declared because a centerline engine cannot be classified safely from coordinates alone."
    )]
    pub wing_mounted_engine_count: Option<usize>,
    /// Number of engines installed on the fuselage or empennage.
    #[config(
        advanced,
        label = "Fuselage-mounted engine count",
        help = "FLOPS FNEF, including empennage-mounted installations. Wing and fuselage counts must sum to the built engine count."
    )]
    pub fuselage_mounted_engine_count: Option<usize>,
    /// Total number of fuel tanks represented by the declared fuel system.
    #[config(
        advanced,
        label = "Fuel tank count",
        help = "FLOPS NTANK: physical tanks represented by the declared fuel-system topology."
    )]
    pub fuel_tank_count: Option<usize>,
    /// Maximum usable aircraft fuel capacity, in kilograms.
    #[config(
        advanced,
        label = "Maximum usable fuel capacity",
        unit = "kg",
        help = "FLOPS FMXTOT: source-backed maximum usable fuel capacity across wing, fuselage, and auxiliary tanks."
    )]
    pub maximum_fuel_capacity_kg: Option<f64>,
    /// Whether an auxiliary power unit is installed and should be priced by
    /// NASA FLOPS equation 101.  The FLOPS default is present; installations
    /// such as the ATR 72 that use an engine in hotel mode declare `false`.
    #[serde(default = "default_apu_installed")]
    #[config(
        advanced,
        label = "Auxiliary power unit installed",
        help = "Whether the aircraft carries an APU. NASA FLOPS equation 101 is evaluated when true; an architecture that supplies hotel power from an installed engine declares false rather than carrying an invented APU mass."
    )]
    pub apu_installed: bool,
    /// Cargo mass carried in standardized containers, in kilograms.
    #[config(
        advanced,
        label = "Containerized cargo mass",
        unit = "kg",
        help = "Cargo loaded into standardized containers for FLOPS WCON. Enter zero explicitly when no containerized cargo is carried."
    )]
    pub containerized_cargo_kg: Option<f64>,
    /// How the cargo compartments are loaded, which decides whether the
    /// checked baggage carries a container tare at all.
    #[config(
        advanced,
        options = CargoHoldLoading,
        label = "Cargo hold loading",
        help = "Whether the holds are loose-loaded (bulk), unit-load-device (containerized) or mixed. FLOPS equations 125-126 charge 175 lb of container for every 950 lb of containerized mass; a bulk-loaded aircraft carries no such hardware and is charged none. Blank keeps the containerized convention the FLOPS source itself assumes."
    )]
    pub cargo_loading: Option<CargoHoldLoading>,
    /// Share of the checked baggage that rides in containers, for a mixed
    /// hold arrangement.
    #[config(
        advanced,
        label = "Containerized baggage fraction",
        unit = "0-1",
        help = "Only read when the cargo hold loading is mixed: the share of checked baggage loaded into unit load devices rather than into the loose bulk compartment, normally the containerized share of the usable hold volume. Declared cargo is containerized by definition and is not scaled by it."
    )]
    pub containerized_baggage_fraction: Option<f64>,
    /// Which method prices the cabin equipment and the occupant-driven
    /// operating items.
    #[config(
        advanced,
        options = CabinEquipmentMethod,
        label = "Cabin equipment method",
        help = "FLOPS equation 110 and operating-item equations 119-126 as published, or the LTH civil-transport furnishings and operating-item relations. The two agree within four percent on a single-aisle; on a long-haul three-class widebody FLOPS is about 40 percent below both the LTH relations and the manufacturer's own accounting. FLOPS remains the default and the auditable baseline. A shaft-power aircraft below 40 t takes the regional turboprop systems relation instead."
    )]
    pub cabin_equipment_method: CabinEquipmentMethod,
    /// Which LTH operating-item relation this aircraft takes.
    #[config(
        advanced,
        options = OperatingHaulClass,
        label = "Operating haul class",
        help = "Read only by the LTH cabin-equipment method, which publishes one operating-item relation for short/medium-haul aircraft and a different one for long-haul aircraft. The source gives no range threshold, so the class is declared rather than inferred."
    )]
    pub haul_class: Option<OperatingHaulClass>,
    /// Revision-locked source families for every declared FLOPS input.
    #[config(
        nested,
        advanced,
        help = "Mission, cabin, and installed-architecture provenance required before FLOPS can report a verified component buildup."
    )]
    pub provenance: FlopsTransportProvenance,
}

const fn default_apu_installed() -> bool {
    true
}

impl Default for FlopsTransportConfig {
    fn default() -> Self {
        Self {
            maximum_mach: None,
            design_range_nmi: None,
            flight_crew_count: None,
            flight_attendant_count: None,
            galley_crew_count: None,
            first_class_passenger_count: None,
            business_class_passenger_count: None,
            tourist_class_passenger_count: None,
            hydraulic_pressure_pa: None,
            variable_sweep_penalty: None,
            wing_mounted_engine_count: None,
            fuselage_mounted_engine_count: None,
            fuel_tank_count: None,
            maximum_fuel_capacity_kg: None,
            apu_installed: default_apu_installed(),
            containerized_cargo_kg: None,
            cargo_loading: None,
            containerized_baggage_fraction: None,
            cabin_equipment_method: CabinEquipmentMethod::default(),
            haul_class: None,
            provenance: FlopsTransportProvenance::default(),
        }
    }
}

impl FlopsTransportConfig {
    /// A complete conventional transport scenario for an unconfigured run.
    ///
    /// [`Default::default`] intentionally remains the empty contract: callers
    /// that want to prove a configuration has declared every physical datum
    /// can still construct it and receive typed blockers.  The product-level
    /// [`crate::MassModelConfig`] uses this constructor for its default so a
    /// new run is executable end to end.  Every value here is marked as
    /// `UserDeclared` and carries an uncertainty statement; these are explicit
    /// study inputs, not aircraft facts and not a fallback to the legacy
    /// fraction method.
    pub fn working_default() -> Self {
        Self {
            maximum_mach: Some(0.90),
            design_range_nmi: Some(7_600.0),
            flight_crew_count: Some(2),
            flight_attendant_count: Some(7),
            galley_crew_count: Some(0),
            first_class_passenger_count: Some(0),
            business_class_passenger_count: Some(0),
            tourist_class_passenger_count: Some(350),
            hydraulic_pressure_pa: Some(3_000.0 * 6_894.757_293_168),
            variable_sweep_penalty: Some(0.0),
            wing_mounted_engine_count: Some(2),
            fuselage_mounted_engine_count: Some(0),
            fuel_tank_count: Some(3),
            maximum_fuel_capacity_kg: Some(200_000.0),
            apu_installed: true,
            containerized_cargo_kg: Some(0.0),
            // A 350-seat notional widebody is a containerised aircraft; the
            // study scenario says so explicitly rather than inheriting it.
            // Since the tare is reported outside operating empty mass, this
            // decides a separately reported quantity and the hold
            // architecture, not the empty mass.
            cargo_loading: Some(CargoHoldLoading::Containerized),
            containerized_baggage_fraction: None,
            // The same domain rule a registered preset gets. A configuration
            // built without a preset must not default to the published FLOPS
            // equations while every preset above 40 t takes the LTH relations: the
            // identical aircraft would be 16,515 kg (+10.0 %) heavier as a preset
            // than as a clean sheet, and any objective comparing the two would be
            // comparing two accounting systems. 350 seats admits this
            // scenario through the seat clause.
            cabin_equipment_method: CabinEquipmentMethod::for_civil_transport_size(
                None,
                Some(350),
                false,
            ),
            // The 7,600 nmi design range above is a long-haul scenario, and
            // the LTH relations publish a separate operating-item relation for
            // it. Leaving this blank silently priced a long-haul cabin with
            // the short/medium-haul relation.
            haul_class: Some(OperatingHaulClass::LongHaul),
            provenance: FlopsTransportProvenance {
                mission: conventional_default_provenance(
                    "mission speed and range are a notional conventional-transport scenario",
                ),
                cabin: conventional_default_provenance(
                    "the cabin is an all-economy 350-seat study scenario",
                ),
                architecture: conventional_default_provenance(
                    "twin wing-mounted engines, fixed wing, three tanks and 3,000 psi are study inputs",
                ),
            },
        }
    }

    /// Whether no FLOPS-specific physical datum has been declared.
    pub fn is_unspecified(&self) -> bool {
        self == &Self::default()
    }
}

/// Provenance attached to each family in [`FlopsTransportConfig::working_default`].
fn conventional_default_provenance(uncertainty: &str) -> FlopsInputProvenance {
    FlopsInputProvenance {
        document: "ALAS pure FLOPS conventional transport default".to_owned(),
        revision: "ALAS 2026-09-11".to_owned(),
        location: "FlopsTransportConfig::working_default".to_owned(),
        applicability: "generic notional transport; configured run must replace with aircraft evidence when available".to_owned(),
        evidence: FlopsInputEvidence::UserDeclared,
        uncertainty: uncertainty.to_owned(),
    }
}

#[cfg(test)]
// A test decodes a fixture it wrote inline here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_is_the_backward_compatible_default() {
        assert_eq!(
            SystemsMassMethod::default(),
            SystemsMassMethod::ReferenceCompatibleFractions
        );
        assert!(SystemsMassMethod::default().is_reference_compatible());
    }

    #[test]
    fn an_empty_flops_contract_is_explicitly_detectable() {
        let default = FlopsTransportConfig::default();
        assert!(default.is_unspecified());
        assert!(default.apu_installed);
        let legacy: FlopsTransportConfig = serde_json::from_value(serde_json::json!({}))
            .expect("legacy empty FLOPS configuration loads");
        assert!(legacy.apu_installed);
        let configured = FlopsTransportConfig {
            hydraulic_pressure_pa: Some(20_684_271.879_504),
            ..Default::default()
        };
        assert!(!configured.is_unspecified());
    }

    #[test]
    fn provenance_requires_a_revision_locator_and_applicability() {
        let incomplete = FlopsInputProvenance {
            document: "Aircraft characteristics".to_owned(),
            ..Default::default()
        };
        assert!(!incomplete.is_declared());
        let complete = FlopsInputProvenance {
            document: "Aircraft characteristics".to_owned(),
            revision: "Rev 1".to_owned(),
            location: "p. 2".to_owned(),
            applicability: "configured weight variant".to_owned(),
            evidence: FlopsInputEvidence::SourceBacked,
            uncertainty: "published variant value".to_owned(),
        };
        assert!(complete.is_declared());
    }

    #[test]
    fn the_method_names_are_stable_in_saved_configuration() {
        assert_eq!(
            serde_json::to_value(SystemsMassMethod::FlopsTransportV1).ok(),
            Some(serde_json::json!("flops_transport_v1"))
        );
    }
}
