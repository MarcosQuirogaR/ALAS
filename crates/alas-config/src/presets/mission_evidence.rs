// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Credibility, conditions and variant match for design-mission evidence.
//!
//! [`crate::PartialDesignMissionEvidence`] already records *what* a source
//! documents and *which* of the four contract data it leaves missing. What it
//! cannot say is how much the source is worth: a type-certificate data sheet,
//! an aircraft-characteristics manual, a peer-reviewed re-derivation, a trade
//! journal and an encyclopedia entry all carry numbers, and only the first two
//! are admissible evidence that a *particular certified variant* flies a
//! *particular* payload/range mission.
//!
//! This module supplies that missing axis and the refusal rules that go with
//! it. It deliberately does **not** provide a way to build a
//! [`crate::DesignMissionEvidence::SourceBacked`] value: promotion is a
//! decision about four independently sourced data at once, so what is offered
//! here is [`DesignMissionProvenanceSet::refusals`], which enumerates every
//! reason a mission may not be promoted, and returns an empty list only when
//! none remain.
//!
//! # Units and conventions
//!
//! Range is metres, payload and fuel kilograms, angles degrees, consistent
//! with the primary aircraft body frame and unit convention declared in
//! [`crate::geometry`]. Uncertainty is a dimensionless relative fraction of
//! the stated value and is recorded only when the source states or clearly
//! implies one; an absent uncertainty is [`None`] and is never defaulted to
//! zero, because "the source did not say" and "the source said it is exact"
//! are different claims.

use crate::{MissingDesignMissionDatum, MissionEvidenceApplicability};

/// How much weight a mission datum's source can carry.
///
/// Ordered strongest first. The ordering is a statement about *provenance*,
/// not about numerical accuracy: a trade-press figure may well be closer to
/// the truth than a conservative certified one, and it still may not be used
/// to assert that a certified aeroplane flies a mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MissionEvidenceTier {
    /// Type-certificate data sheet, approved flight manual, or weight and
    /// balance manual: authoritative for the named variant and normally
    /// condition-complete.
    Certified,
    /// Manufacturer publication that is not an approved document: aircraft
    /// characteristics/airport planning manuals, product factsheets. Direct
    /// from the designer, but free to omit the conditions a figure holds at.
    Manufacturer,
    /// A peer-reviewed third party that independently re-derives a figure
    /// rather than reprinting the manufacturer's.
    PeerReviewedThirdParty,
    /// Trade press and commercial data services: cite them, do not weight
    /// them as primary.
    SecondaryTradePress,
    /// Community-maintained reference. Never sufficient alone; record it only
    /// as a labelled conflict against something stronger.
    Community,
}

impl MissionEvidenceTier {
    /// Stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Certified => "certified",
            Self::Manufacturer => "manufacturer",
            Self::PeerReviewedThirdParty => "peer-reviewed third party",
            Self::SecondaryTradePress => "secondary trade press",
            Self::Community => "community",
        }
    }

    /// Whether a datum at this tier may contribute to promoting a mission to
    /// [`crate::DesignMissionEvidence::SourceBacked`].
    ///
    /// Everything below [`Self::Manufacturer`] may only ever populate the
    /// existing [`crate::PartialDesignMissionEvidence`] path, tagged with its
    /// tier.
    #[must_use]
    pub const fn admits_promotion(self) -> bool {
        matches!(self, Self::Certified | Self::Manufacturer)
    }
}

/// One mission datum, with everything needed to decide whether it counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissionDatumProvenance {
    /// How much the source can carry.
    pub tier: MissionEvidenceTier,
    /// The operating conditions the source states the figure at, verbatim.
    ///
    /// [`None`] means the source did not state them. A range with no stated
    /// passenger mass, reserve policy, atmosphere and cruise schedule is not
    /// a mission; it is a marketing maximum.
    pub stated_conditions: Option<&'static str>,
    /// Relative uncertainty the source states or implies, dimensionless.
    pub relative_uncertainty: Option<f64>,
    /// Typed relation between the source configuration and the registered
    /// preset variant.
    pub variant_match: MissionEvidenceApplicability,
    /// Exact document, revision, page and figure.
    pub source: &'static str,
}

impl MissionDatumProvenance {
    /// Whether this single datum clears every promotion bar.
    ///
    /// All three conditions are independent and all three are required: a
    /// certified document about the wrong weight variant, and a manufacturer
    /// figure with no stated conditions, are both refused.
    #[must_use]
    pub fn admits_promotion(&self) -> bool {
        self.tier.admits_promotion()
            && self.variant_match == MissionEvidenceApplicability::ExactPreset
            && self.stated_conditions.is_some()
    }
}

/// Why one datum may not contribute to a source-backed design mission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MissionPromotionRefusal {
    /// No source supplies this datum at all.
    DatumAbsent {
        /// Which of the four contract data is missing.
        datum: MissingDesignMissionDatum,
    },
    /// A source exists but its tier may not complete a mission.
    TierTooWeak {
        /// Which datum.
        datum: MissingDesignMissionDatum,
        /// The tier that was offered.
        tier: MissionEvidenceTier,
    },
    /// The source documents a different variant from the registered preset.
    VariantMismatch {
        /// Which datum.
        datum: MissingDesignMissionDatum,
        /// What the source actually applies to.
        applicability: MissionEvidenceApplicability,
    },
    /// The source states the figure without the conditions it holds at.
    ConditionsNotStated {
        /// Which datum.
        datum: MissingDesignMissionDatum,
    },
}

impl MissionPromotionRefusal {
    /// Which of the four contract data this refusal is about.
    #[must_use]
    pub const fn datum(self) -> MissingDesignMissionDatum {
        match self {
            Self::DatumAbsent { datum }
            | Self::TierTooWeak { datum, .. }
            | Self::VariantMismatch { datum, .. }
            | Self::ConditionsNotStated { datum } => datum,
        }
    }

    /// One sentence a report can print without further formatting.
    #[must_use]
    pub fn message(self) -> String {
        let datum = datum_label(self.datum());
        match self {
            Self::DatumAbsent { .. } => {
                format!("{datum}: no source registered")
            }
            Self::TierTooWeak { tier, .. } => format!(
                "{datum}: {} evidence may populate the partial record but may not complete a \
                 design mission",
                tier.label()
            ),
            Self::VariantMismatch { applicability, .. } => format!(
                "{datum}: the source applies to {}, not the exact registered variant",
                applicability_label(applicability)
            ),
            Self::ConditionsNotStated { .. } => {
                format!("{datum}: the source states no operating conditions for the figure")
            }
        }
    }
}

/// Stable label for one contract datum.
#[must_use]
pub const fn datum_label(datum: MissingDesignMissionDatum) -> &'static str {
    match datum {
        MissingDesignMissionDatum::Range => "range",
        MissingDesignMissionDatum::Payload => "payload",
        MissingDesignMissionDatum::Profile => "profile",
        MissingDesignMissionDatum::ReserveFuel => "reserve fuel",
    }
}

/// Stable label for a source-to-preset applicability.
#[must_use]
pub const fn applicability_label(applicability: MissionEvidenceApplicability) -> &'static str {
    match applicability {
        MissionEvidenceApplicability::ExactPreset => "the exact registered variant",
        MissionEvidenceApplicability::ModelAndEngineFamily => "the model and engine family",
        MissionEvidenceApplicability::ModelOnly => "the model only",
        MissionEvidenceApplicability::DifferentWeightVariant => "a different weight variant",
    }
}

/// The four independently sourced data a design mission is made of.
///
/// Defaults to all four absent, which is every registered preset's state
/// today and is the honest one.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DesignMissionProvenanceSet {
    /// Source-selected still-air range.
    pub range: Option<MissionDatumProvenance>,
    /// Payload carried at that range.
    pub payload: Option<MissionDatumProvenance>,
    /// Climb, cruise and descent assumptions.
    pub profile: Option<MissionDatumProvenance>,
    /// Fuel required to remain after the modeled trip.
    pub reserve_fuel: Option<MissionDatumProvenance>,
}

impl DesignMissionProvenanceSet {
    /// The four slots paired with the datum they answer, in contract order.
    fn slots(&self) -> [(MissingDesignMissionDatum, Option<MissionDatumProvenance>); 4] {
        [
            (MissingDesignMissionDatum::Range, self.range),
            (MissingDesignMissionDatum::Payload, self.payload),
            (MissingDesignMissionDatum::Profile, self.profile),
            (MissingDesignMissionDatum::ReserveFuel, self.reserve_fuel),
        ]
    }

    /// Every reason this set may not become a source-backed design mission,
    /// in contract order, with **one refusal per datum**: the first bar it
    /// fails, so a reader closes them one at a time rather than re-reading a
    /// list that changes length for the same datum.
    ///
    /// An empty list is the only thing that may promote a mission. It is not
    /// itself a promotion: the evaluator that flies the mission through the
    /// model still has to agree.
    #[must_use]
    pub fn refusals(&self) -> Vec<MissionPromotionRefusal> {
        self.slots()
            .into_iter()
            .filter_map(|(datum, provenance)| {
                let Some(provenance) = provenance else {
                    return Some(MissionPromotionRefusal::DatumAbsent { datum });
                };
                if !provenance.tier.admits_promotion() {
                    return Some(MissionPromotionRefusal::TierTooWeak {
                        datum,
                        tier: provenance.tier,
                    });
                }
                if provenance.variant_match != MissionEvidenceApplicability::ExactPreset {
                    return Some(MissionPromotionRefusal::VariantMismatch {
                        datum,
                        applicability: provenance.variant_match,
                    });
                }
                (provenance.stated_conditions.is_none())
                    .then_some(MissionPromotionRefusal::ConditionsNotStated { datum })
            })
            .collect()
    }

    /// Which of the four contract data have no source at all.
    #[must_use]
    pub fn missing(&self) -> Vec<MissingDesignMissionDatum> {
        self.slots()
            .into_iter()
            .filter_map(|(datum, provenance)| provenance.is_none().then_some(datum))
            .collect()
    }

    /// Whether every datum clears every bar.
    ///
    /// A metadata row can never make this true on its own: each of the four
    /// slots is checked independently, so no count of weak rows substitutes
    /// for one missing strong one.
    #[must_use]
    pub fn admits_source_backed_promotion(&self) -> bool {
        self.refusals().is_empty()
    }

    /// The weakest tier present, or [`None`] when nothing is registered.
    ///
    /// Reported so a partial record keeps its credibility visible rather than
    /// being summarized by its strongest source.
    #[must_use]
    pub fn weakest_registered_tier(&self) -> Option<MissionEvidenceTier> {
        self.slots()
            .into_iter()
            .filter_map(|(_, provenance)| provenance.map(|value| value.tier))
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certified(source: &'static str) -> MissionDatumProvenance {
        MissionDatumProvenance {
            tier: MissionEvidenceTier::Certified,
            stated_conditions: Some("ISA, 95 kg per passenger, EASA basic reserves"),
            relative_uncertainty: None,
            variant_match: MissionEvidenceApplicability::ExactPreset,
            source,
        }
    }

    fn complete_set() -> DesignMissionProvenanceSet {
        DesignMissionProvenanceSet {
            range: Some(certified("range source")),
            payload: Some(certified("payload source")),
            profile: Some(certified("profile source")),
            reserve_fuel: Some(certified("reserve source")),
        }
    }

    /// The registry's state today: nothing registered, four refusals, no
    /// promotion.
    #[test]
    fn an_empty_set_refuses_all_four_data() {
        let set = DesignMissionProvenanceSet::default();
        assert!(!set.admits_source_backed_promotion());
        assert_eq!(set.missing().len(), 4);
        assert_eq!(set.refusals().len(), 4);
        assert!(set
            .refusals()
            .iter()
            .all(|refusal| matches!(refusal, MissionPromotionRefusal::DatumAbsent { .. })));
        assert_eq!(set.weakest_registered_tier(), None);
    }

    /// The A220-300 corner: a manufacturer range that is genuinely published
    /// but for the up-to-70.9 t product rather than the registered 67,585 kg
    /// legacy variant. A conditioned, manufacturer-tier, *wrong-variant*
    /// datum must not count, and it must say so as a variant mismatch rather
    /// than as an absence.
    #[test]
    fn a_right_tier_wrong_variant_datum_is_refused_as_a_mismatch() {
        let set = DesignMissionProvenanceSet {
            range: Some(MissionDatumProvenance {
                tier: MissionEvidenceTier::Manufacturer,
                stated_conditions: Some("up to 70.9 t takeoff mass"),
                relative_uncertainty: None,
                variant_match: MissionEvidenceApplicability::DifferentWeightVariant,
                source: "Airbus A220 Digital Pamphlet FAI V5.2, July 2022, p.1",
            }),
            ..complete_set()
        };
        assert!(!set.admits_source_backed_promotion());
        assert!(
            set.missing().is_empty(),
            "the datum exists; it is the wrong one"
        );
        let refusals = set.refusals();
        assert_eq!(refusals.len(), 1);
        assert!(matches!(
            refusals[0],
            MissionPromotionRefusal::VariantMismatch {
                datum: MissingDesignMissionDatum::Range,
                applicability: MissionEvidenceApplicability::DifferentWeightVariant,
            }
        ));
    }

    /// No quantity of weak evidence substitutes for one strong datum: four
    /// exactly-matching, fully-conditioned community rows still refuse.
    #[test]
    fn metadata_alone_never_promotes_a_mission() {
        let community = MissionDatumProvenance {
            tier: MissionEvidenceTier::Community,
            stated_conditions: Some("stated in full"),
            relative_uncertainty: Some(0.02),
            variant_match: MissionEvidenceApplicability::ExactPreset,
            source: "community reference",
        };
        let set = DesignMissionProvenanceSet {
            range: Some(community),
            payload: Some(community),
            profile: Some(community),
            reserve_fuel: Some(community),
        };
        assert!(!set.admits_source_backed_promotion());
        assert_eq!(set.refusals().len(), 4);
        assert_eq!(
            set.weakest_registered_tier(),
            Some(MissionEvidenceTier::Community)
        );
    }

    /// A manufacturer figure with no stated conditions is refused for that
    /// reason specifically, not silently accepted at its tier.
    #[test]
    fn an_unconditioned_manufacturer_datum_is_refused_for_its_conditions() {
        let set = DesignMissionProvenanceSet {
            reserve_fuel: Some(MissionDatumProvenance {
                tier: MissionEvidenceTier::Manufacturer,
                stated_conditions: None,
                relative_uncertainty: None,
                variant_match: MissionEvidenceApplicability::ExactPreset,
                source: "factsheet",
            }),
            ..complete_set()
        };
        let refusals = set.refusals();
        assert_eq!(refusals.len(), 1);
        assert!(matches!(
            refusals[0],
            MissionPromotionRefusal::ConditionsNotStated {
                datum: MissingDesignMissionDatum::ReserveFuel
            }
        ));
        assert!(refusals[0].message().contains("reserve fuel"));
    }

    /// The one shape that clears every bar, so the rule is a gate rather than
    /// a blanket refusal.
    #[test]
    fn four_certified_exact_variant_conditioned_data_clear_every_bar() {
        let set = complete_set();
        assert!(set.admits_source_backed_promotion());
        assert!(set.refusals().is_empty());
        assert!(set.missing().is_empty());
        assert_eq!(
            set.weakest_registered_tier(),
            Some(MissionEvidenceTier::Certified)
        );
    }
}
