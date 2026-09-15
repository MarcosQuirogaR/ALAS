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

use crate::{ConfigNode, Kind, Leaf};

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

    /// Whether this family may be described as aircraft data rather than as
    /// a choice made for the run.
    pub const fn is_aircraft_data(self) -> bool {
        matches!(self, Self::SourceBacked)
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

    /// Whether this family states how uncertain it is.
    pub fn states_uncertainty(&self) -> bool {
        !self.uncertainty.trim().is_empty()
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

    /// The three families, named, in the order the coverage report lists them.
    pub fn families(&self) -> [(&'static str, &FlopsInputProvenance); 3] {
        [
            ("mission", &self.mission),
            ("cabin", &self.cabin),
            ("architecture", &self.architecture),
        ]
    }

    /// Whether every family is read from a document for this variant.
    ///
    /// This is the only condition under which a run's non-geometric inputs
    /// may be described as aircraft data. Anything else is a scenario, and
    /// calling it otherwise is the mistake this method exists to prevent.
    pub fn is_entirely_source_backed(&self) -> bool {
        self.families()
            .iter()
            .all(|(_, family)| family.evidence.is_aircraft_data())
    }

    /// Names of the families that state no uncertainty.
    pub fn families_without_uncertainty(&self) -> Vec<&'static str> {
        self.families()
            .iter()
            .filter(|(_, family)| !family.states_uncertainty())
            .map(|(name, _)| *name)
            .collect()
    }
}

/// Physical inputs that the FLOPS transport equations cannot derive from the
/// built outer geometry.
///
/// Every field is optional because an old configuration has none of them.
/// The evaluator reports each absent datum as unverified; these options are
/// not invitations to substitute a default correlation or preset constant.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ConfigNode)]
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
        help = "Installed first-class seats for FLOPS furnishings and passenger-service mass."
    )]
    pub first_class_passenger_count: Option<usize>,
    /// Number of business-class passengers in the installed cabin.
    #[config(
        advanced,
        label = "Business-class passenger count",
        help = "Installed business-class seats for FLOPS furnishings and passenger-service mass."
    )]
    pub business_class_passenger_count: Option<usize>,
    /// Number of tourist/economy passengers in the installed cabin.
    #[config(
        advanced,
        label = "Tourist-class passenger count",
        help = "Installed economy/tourist seats for FLOPS furnishings and passenger-service mass."
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
    /// Cargo mass carried in standardized containers, in kilograms.
    #[config(
        advanced,
        label = "Containerized cargo mass",
        unit = "kg",
        help = "Cargo loaded into standardized containers for FLOPS WCON. Enter zero explicitly when no containerized cargo is carried."
    )]
    pub containerized_cargo_kg: Option<f64>,
    /// Revision-locked source families for every declared FLOPS input.
    #[config(
        nested,
        advanced,
        help = "Mission, cabin, and installed-architecture provenance required before FLOPS can report a verified component buildup."
    )]
    pub provenance: FlopsTransportProvenance,
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
            containerized_cargo_kg: Some(0.0),
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
        assert!(FlopsTransportConfig::default().is_unspecified());
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
