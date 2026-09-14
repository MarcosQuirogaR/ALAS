// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one authoritative operating-empty-mass (OEW) reference registry.
//!
//! Every report, harness and example that compares a modelled operating empty
//! mass with a published one reads this registry; nothing else carries an OEW
//! anchor. Each registered aircraft has exactly one record, including the
//! ones for which no comparable value exists, so a missing number is a stated
//! fact rather than an empty cell.
//!
//! A record separates three things a published "empty weight" conflates:
//!
//! * **Which aircraft the number describes** (`reference_configuration`) and
//!   how that differs from the registered preset (`differences_from_preset`).
//!   A manufacturer empty weight, a basic empty weight, an operator dry
//!   operating weight and a planning OEW are different quantities, and a
//!   figure for another weight variant, engine, modification state or cabin
//!   is a different aircraft.
//! * **What the number includes** (`inclusion`): crew, catering, potable
//!   water, unusable fuel, oil, containers. FLOPS equation 141's operating
//!   empty mass is compared only after that list is known; unknown items are
//!   recorded as unknown, never assumed.
//! * **How it may be used** (`applicability`): only a configuration-matched
//!   record with a stated inclusion list enters a validation metric. Every
//!   other value is visible for a conditional comparison and is excluded from
//!   accuracy statistics.
//!
//! The registry never authorises a calibration: an anchor is something the
//! model is compared with, not fitted to.

#[path = "oew_reference/records.rs"]
mod records;
#[path = "oew_reference/sources.rs"]
mod sources;

use serde::Serialize;

/// How a published OEW relates to the registered preset configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OewApplicability {
    /// Same model, weight variant, engine, modification state and cabin as
    /// the preset, with a stated inclusion list. The only status that enters
    /// a validation metric.
    ConfigurationMatched,
    /// Same model and weight variant; a stated cabin, equipment or
    /// definition difference remains. Visible conditional comparison only.
    ConditionalMismatch,
    /// No published OEW for the preset's exact configuration. Any anchor the
    /// record carries belongs to a different configuration and is compared
    /// only through the named reconstructed case.
    SourceGap,
    /// A published value exists but the production mass model cannot
    /// evaluate the aircraft, so there is nothing to compare it with.
    UnsupportedModel,
    /// A notional design; no aircraft OEW exists.
    NotApplicable,
}

impl OewApplicability {
    /// Machine-readable label used in CSV and JSON artifacts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConfigurationMatched => "configuration_matched",
            Self::ConditionalMismatch => "conditional_mismatch",
            Self::SourceGap => "source_gap",
            Self::UnsupportedModel => "unsupported_model",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Where a number comes from, in decreasing order of authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OewSourceTier {
    /// A manufacturer airport-planning, recovery or weight document.
    ManufacturerPlanningDocument,
    /// A certification authority document (type-certificate data sheet).
    CertificationDocument,
    /// An operator or aircraft-specific record with an unstated inclusion
    /// list (specification sheet, weight-and-balance extract).
    OperatorRecord,
    /// A compilation or aggregator with no primary document behind it.
    Aggregator,
    /// A value the preset registry carried without a retained source.
    PresetLegacyValue,
}

impl OewSourceTier {
    /// Machine-readable label used in CSV and JSON artifacts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManufacturerPlanningDocument => "manufacturer_planning_document",
            Self::CertificationDocument => "certification_document",
            Self::OperatorRecord => "operator_record",
            Self::Aggregator => "aggregator",
            Self::PresetLegacyValue => "preset_legacy_value",
        }
    }
}

/// Whether one operating item is inside a published empty mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InclusionStatus {
    /// The source states the item is included.
    Included,
    /// The source states the item is excluded.
    Excluded,
    /// The source does not say.
    Unknown,
}

/// The operating items whose inclusion decides what an "empty weight" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OewInclusionList {
    /// Flight-deck crew.
    pub flight_crew: InclusionStatus,
    /// Cabin attendants.
    pub cabin_crew: InclusionStatus,
    /// Crew baggage.
    pub crew_baggage: InclusionStatus,
    /// Unusable fuel.
    pub unusable_fuel: InclusionStatus,
    /// Engine and system oil.
    pub engine_oil: InclusionStatus,
    /// Galley equipment and catering stores.
    pub galley_equipment_and_catering: InclusionStatus,
    /// Potable water and waste pre-charge.
    pub potable_water: InclusionStatus,
    /// Seats and cabin furnishings.
    pub seats_and_furnishings: InclusionStatus,
    /// Cargo containers and pallets.
    pub cargo_containers: InclusionStatus,
    /// Manuals and other operational items.
    pub manuals_and_operational_items: InclusionStatus,
    /// Usable fuel: excluded by every operating-empty definition.
    pub usable_fuel: InclusionStatus,
    /// Payload: excluded by every operating-empty definition.
    pub payload: InclusionStatus,
}

impl OewInclusionList {
    /// A list whose every operating item is unknown; fuel and payload are
    /// excluded by definition.
    pub const fn unknown() -> Self {
        Self {
            flight_crew: InclusionStatus::Unknown,
            cabin_crew: InclusionStatus::Unknown,
            crew_baggage: InclusionStatus::Unknown,
            unusable_fuel: InclusionStatus::Unknown,
            engine_oil: InclusionStatus::Unknown,
            galley_equipment_and_catering: InclusionStatus::Unknown,
            potable_water: InclusionStatus::Unknown,
            seats_and_furnishings: InclusionStatus::Unknown,
            cargo_containers: InclusionStatus::Unknown,
            manuals_and_operational_items: InclusionStatus::Unknown,
            usable_fuel: InclusionStatus::Excluded,
            payload: InclusionStatus::Excluded,
        }
    }

    /// Whether every operating item is stated one way or the other.
    pub fn is_complete(&self) -> bool {
        [
            self.flight_crew,
            self.cabin_crew,
            self.crew_baggage,
            self.unusable_fuel,
            self.engine_oil,
            self.galley_equipment_and_catering,
            self.potable_water,
            self.seats_and_furnishings,
            self.cargo_containers,
            self.manuals_and_operational_items,
        ]
        .iter()
        .all(|status| *status != InclusionStatus::Unknown)
    }
}

/// One revision-locked document locator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OewSource {
    /// Document title.
    pub document: &'static str,
    /// Publisher.
    pub publisher: &'static str,
    /// Revision or issue.
    pub revision: &'static str,
    /// Revision date, ISO 8601 where the document states one.
    pub date: &'static str,
    /// Section, page, figure or table.
    pub locator: &'static str,
    /// Public URL, or the retained local path when the document is not
    /// publicly hosted.
    pub url: &'static str,
    /// Retained local copy, when one exists.
    pub local_path: &'static str,
    /// Date the value was retrieved or verified, ISO 8601.
    pub retrieved: &'static str,
    /// The exact wording the value was read from.
    pub quote: &'static str,
    /// Authority tier.
    pub tier: OewSourceTier,
}

/// The aircraft a published value describes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OewReferenceConfiguration {
    /// Model designation.
    pub model: &'static str,
    /// Weight variant or maximum takeoff mass basis.
    pub weight_variant: &'static str,
    /// Maximum takeoff mass the value belongs to, when stated.
    pub mtow_kg: Option<f64>,
    /// Engine model.
    pub engine: &'static str,
    /// Modification state (wingtip devices, engine standard).
    pub modification_state: &'static str,
    /// Cabin the value belongs to.
    pub cabin: &'static str,
}

/// A published figure that is not the comparable OEW but is easily
/// mistaken for one, or a same-source alternative value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PublishedOewValue {
    /// What the source calls it.
    pub label: &'static str,
    /// Value, kg.
    pub value_kg: f64,
    /// Whether the label is an operating-empty definition at all.
    pub is_operating_empty: bool,
    /// Where it comes from.
    pub source: OewSource,
    /// Why it is or is not usable.
    pub note: &'static str,
}

/// An anchor that belongs to a different configuration than the preset and
/// is compared only through the named reconstructed case.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OewCaseAnchor {
    /// Label of the reconstructed case in the experiment matrix.
    pub case_label: &'static str,
    /// Value, kg.
    pub value_kg: f64,
    /// The configuration the value describes.
    pub configuration: OewReferenceConfiguration,
    /// Where it comes from.
    pub source: OewSource,
    /// Comparison uncertainty, kg, when one can be stated.
    pub uncertainty_kg: Option<f64>,
    /// What still separates the reconstructed case from the anchor.
    pub residual_mismatch: &'static [&'static str],
}

/// The OEW reference record of one registered aircraft.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OewReference {
    /// The preset name.
    pub preset: &'static str,
    /// How the record may be used.
    pub applicability: OewApplicability,
    /// The value comparable with the preset's fixed-design-weight OEW, kg.
    /// Present only for configuration-matched, conditional-mismatch and
    /// unsupported-model records.
    pub reference_oew_kg: Option<f64>,
    /// What the source calls the value.
    pub definition_label: &'static str,
    /// The aircraft the value describes.
    pub reference_configuration: OewReferenceConfiguration,
    /// Stated differences between that aircraft and the registered preset.
    pub differences_from_preset: &'static [&'static str],
    /// Inclusion list of the comparable value.
    pub inclusion: OewInclusionList,
    /// Source of the comparable value.
    pub source: Option<OewSource>,
    /// Comparison uncertainty of the comparable value, kg.
    pub uncertainty_kg: Option<f64>,
    /// An anchor for a different configuration, if one exists.
    pub case_anchor: Option<OewCaseAnchor>,
    /// Other published figures that must not be mixed in.
    pub other_published_values: &'static [PublishedOewValue],
    /// The empty-mass figure the preset's declared structural payload
    /// (`requirements.max_structural_payload_kg`) was historically derived
    /// from as maximum zero-fuel mass minus this value. It documents a
    /// declared input; it is not a reference.
    pub structural_payload_basis_oew_kg: Option<f64>,
    /// Free-text notes.
    pub notes: &'static str,
}

impl OewReference {
    /// Whether the record may enter a validation metric.
    pub fn counts_toward_validation(&self) -> bool {
        self.applicability == OewApplicability::ConfigurationMatched
            && self.reference_oew_kg.is_some()
            && self.inclusion.is_complete()
    }

    /// The value shown in a conditional comparison, kg: the comparable value
    /// when there is one, else the case anchor.
    pub fn visible_value_kg(&self) -> Option<f64> {
        self.reference_oew_kg
            .or_else(|| self.case_anchor.map(|anchor| anchor.value_kg))
    }

    /// The label under which the visible value is compared: the preset
    /// itself, or the reconstructed case that carries the anchor.
    pub fn visible_comparison_case(&self) -> Option<&'static str> {
        if self.reference_oew_kg.is_some() {
            Some("fixed_design_weight")
        } else {
            self.case_anchor.map(|anchor| anchor.case_label)
        }
    }

    /// The authority tier of the visible value.
    pub fn visible_tier(&self) -> Option<OewSourceTier> {
        if self.reference_oew_kg.is_some() {
            self.source.map(|source| source.tier)
        } else {
            self.case_anchor.map(|anchor| anchor.source.tier)
        }
    }

    /// The value the preset's `reference.oew_kg` carries: the comparable
    /// value for a same-aircraft record, nothing for a source gap.
    pub fn preset_reference_oew_kg(&self) -> Option<f64> {
        match self.applicability {
            OewApplicability::ConfigurationMatched
            | OewApplicability::ConditionalMismatch
            | OewApplicability::UnsupportedModel => self.reference_oew_kg,
            OewApplicability::SourceGap | OewApplicability::NotApplicable => None,
        }
    }

    /// The record as JSON, for artifacts that must carry the same registry.
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(map) = &mut value {
            map.insert(
                "counts_toward_validation".to_owned(),
                serde_json::Value::Bool(self.counts_toward_validation()),
            );
            map.insert(
                "visible_value_kg".to_owned(),
                serde_json::json!(self.visible_value_kg()),
            );
            map.insert(
                "visible_comparison_case".to_owned(),
                serde_json::json!(self.visible_comparison_case()),
            );
            map.insert(
                "visible_tier".to_owned(),
                serde_json::json!(self.visible_tier().map(OewSourceTier::as_str)),
            );
        }
        value
    }
}

/// Every registered aircraft's record, in preset registration order.
pub fn registry() -> &'static [OewReference] {
    records::RECORDS
}

/// The record of one preset.
pub fn get(preset: &str) -> Option<&'static OewReference> {
    registry().iter().find(|record| record.preset == preset)
}

/// The value a preset's `reference.oew_kg` carries; see
/// [`OewReference::preset_reference_oew_kg`].
pub fn preset_reference_oew_kg(preset: &str) -> Option<f64> {
    get(preset).and_then(OewReference::preset_reference_oew_kg)
}

/// The whole registry as JSON, in registration order.
pub fn registry_json() -> serde_json::Value {
    serde_json::Value::Array(registry().iter().map(OewReference::to_json).collect())
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_preset_has_exactly_one_record_in_registration_order() {
        let presets: Vec<&str> = crate::presets::available();
        let records: Vec<&str> = registry().iter().map(|record| record.preset).collect();
        assert_eq!(records, presets);
    }

    #[test]
    fn no_record_currently_counts_toward_validation() {
        // No published OEW is configuration-matched with a stated inclusion
        // list; the registry says so rather than letting a conditional
        // anchor leak into an accuracy metric.
        for record in registry() {
            assert!(
                !record.counts_toward_validation(),
                "{} is not configuration matched",
                record.preset
            );
        }
    }

    #[test]
    fn preset_reference_values_follow_the_applicability_rule() {
        for record in registry() {
            let preset = crate::presets::get(record.preset).unwrap();
            assert_eq!(
                preset.reference.oew_kg,
                record.preset_reference_oew_kg(),
                "{} reference.oew_kg must be the registry value",
                record.preset
            );
            match record.applicability {
                OewApplicability::SourceGap | OewApplicability::NotApplicable => {
                    assert!(record.reference_oew_kg.is_none(), "{}", record.preset);
                }
                OewApplicability::ConfigurationMatched
                | OewApplicability::ConditionalMismatch
                | OewApplicability::UnsupportedModel => {
                    assert!(record.reference_oew_kg.is_some(), "{}", record.preset);
                    assert!(record.source.is_some(), "{}", record.preset);
                }
            }
        }
    }

    #[test]
    fn the_declared_structural_payload_basis_is_documented_where_it_exists() {
        for record in registry() {
            let preset = crate::presets::get(record.preset).unwrap();
            if let (Some(basis), Some(mzfw)) = (
                record.structural_payload_basis_oew_kg,
                preset.reference.mzfw_kg,
            ) {
                assert!(
                    (preset.requirements.max_structural_payload_kg - (mzfw - basis)).abs() < 1e-6,
                    "{}: declared structural payload is MZFW minus the documented basis",
                    record.preset
                );
            }
        }
    }

    #[test]
    fn the_a320_registered_case_has_no_comparable_value_but_a_case_anchor() {
        let a320 = get("A320-200").unwrap();
        assert_eq!(a320.applicability, OewApplicability::SourceGap);
        assert_eq!(a320.reference_oew_kg, None);
        let anchor = a320.case_anchor.unwrap();
        assert_eq!(anchor.value_kg, 41_052.0);
        assert_eq!(anchor.case_label, "A320_F-HDRF_77t_180Y");
        assert!(a320
            .other_published_values
            .iter()
            .any(|value| value.value_kg == 41_244.0 && !value.is_operating_empty));
        assert_eq!(a320.visible_value_kg(), Some(41_052.0));
        assert_eq!(a320.visible_comparison_case(), Some("A320_F-HDRF_77t_180Y"));
    }

    #[test]
    fn the_a220_record_is_primary_with_a_stated_inclusion_list() {
        let a220 = get("A220-300").unwrap();
        assert_eq!(a220.applicability, OewApplicability::ConditionalMismatch);
        assert_eq!(a220.reference_oew_kg, Some(37_149.0));
        assert_eq!(
            a220.source.unwrap().tier,
            OewSourceTier::ManufacturerPlanningDocument
        );
        assert_eq!(a220.inclusion.unusable_fuel, InclusionStatus::Included);
        assert_eq!(a220.inclusion.cargo_containers, InclusionStatus::Unknown);
        assert!(!a220.inclusion.is_complete());
    }

    #[test]
    fn registry_json_carries_the_derived_fields() {
        let json = registry_json();
        let rows = json.as_array().unwrap();
        assert_eq!(rows.len(), registry().len());
        for row in rows {
            assert!(row.get("counts_toward_validation").is_some());
            assert!(row.get("visible_value_kg").is_some());
            assert!(row.get("applicability").is_some());
        }
    }
}
