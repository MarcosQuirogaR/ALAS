// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py
// Reference: alas @ rust-port-baseline.

//! What the aircraft carries, and where in the fuselage it sits.
//!
//! This drives the detailed payload layout, which is built afresh for every
//! candidate the optimizer evaluates (not only for the final design) so
//! that the centre of gravity checked against the envelope is the one the
//! real seating and loading produce rather than a lumped estimate.
//!
//! # Three product classes
//!
//! First, business and economy are the three supported product classes. The
//! former premium-economy slot remains in serialized files for compatibility,
//! but is hidden and excluded from all product allocation calculations.

mod allocation;
mod cargo;
mod seat_class;

use allocation::proportional_integer_allocation;
pub use cargo::CargoDeckConfig;
pub use seat_class::SeatClassConfig;

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, DesignRequirements, FlopsInputEvidence, MassModelConfig};

/// The three product class slots, forward to aft. Naming them once keeps the layout
/// order and the share mix from disagreeing about which cabin comes first.
const CLASS_NAMES: [&str; 3] = ["First", "Business", "Economy"];

/// The three class counts the FLOPS transport contract can consume.
///
/// `PassengerCabinConfig` still carries the former premium-economy slot for
/// saved-file compatibility, but FLOPS has no fourth class. A positive legacy
/// premium count is therefore folded into `tourist` by the resolver below.
/// Keeping this value type in the configuration crate lets payload, pipeline,
/// optimizer and validation code use the same authority without depending on
/// the mass-equation crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPassengerCounts {
    /// First-class seats.
    pub first: i64,
    /// Business-class seats.
    pub business: i64,
    /// Tourist/economy seats, including a positive legacy premium count.
    pub tourist: i64,
}

impl ResolvedPassengerCounts {
    /// Total installed passenger seats represented by this FLOPS split.
    pub fn total(self) -> i64 {
        self.first
            .saturating_add(self.business)
            .saturating_add(self.tourist)
    }

    /// Whether at least one seat was resolved.
    pub fn is_nonempty(self) -> bool {
        self.total() > 0
    }
}

/// Mark a FLOPS transport clone when its class-count mirror has been resolved
/// from the canonical cabin rather than copied from the serialized seed.
///
/// The transport node retains the source document and revision for the seed,
/// but a changed numeric split must not continue to look source-backed in an
/// evaluated report. There is no fourth Derived evidence enum in the saved
/// schema, so the existing auditable UserDeclared category is used together
/// with an explicit applicability/uncertainty note. The comparison is made
/// before the caller writes the new counts; calling this helper twice is
/// idempotent.
pub fn annotate_flops_cabin_resolution(
    mass_model: &mut MassModelConfig,
    counts: ResolvedPassengerCounts,
) {
    let current = [
        mass_model
            .flops_transport
            .first_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
        mass_model
            .flops_transport
            .business_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
        mass_model
            .flops_transport
            .tourist_class_passenger_count
            .and_then(|value| i64::try_from(value).ok()),
    ];
    let resolved = [
        Some(counts.first),
        Some(counts.business),
        Some(counts.tourist),
    ];
    if current == resolved {
        return;
    }

    let note = format!(
        "canonical cabin resolution for this evaluation uses FLOPS class counts First={}, Business={}, Tourist={}",
        counts.first, counts.business, counts.tourist
    );
    let provenance = &mut mass_model.flops_transport.provenance.cabin;
    provenance.applicability = replace_provenance_note(
        &provenance.applicability,
        "canonical cabin resolution for this evaluation uses FLOPS class counts",
        &note,
    );
    if !provenance
        .uncertainty
        .contains("serialized FLOPS class-count seed was superseded")
    {
        provenance.uncertainty = append_provenance_note(
            &provenance.uncertainty,
            "the serialized FLOPS class-count seed was superseded by the canonical cabin for this load case",
        );
    }
    provenance.evidence = FlopsInputEvidence::UserDeclared;
}

fn replace_provenance_note(existing: &str, marker: &str, note: &str) -> String {
    let existing = existing.trim().trim_end_matches('.');
    if let Some(index) = existing.find(marker) {
        let prefix = existing[..index].trim().trim_end_matches(';').trim();
        if prefix.is_empty() {
            return note.to_owned();
        }
        return format!("{prefix}; {note}");
    }
    append_provenance_note(existing, note)
}

fn append_provenance_note(existing: &str, note: &str) -> String {
    let existing = existing.trim().trim_end_matches('.');
    if existing.is_empty() {
        note.to_owned()
    } else {
        format!("{existing}; {note}")
    }
}

/// Passenger cabin: the class mix, the monuments, and the baggage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct PassengerCabinConfig {
    /// Whether the class mix is given as shares of length or as seat counts.
    #[config(skip)]
    pub class_mix_mode: String,

    /// The first-class cabin.
    #[config(nested, help = "First class: suites, typically one-two-one abreast.")]
    pub first: SeatClassConfig,

    /// The business cabin.
    #[config(
        nested,
        help = "Business class: lie-flat seats, typically one-two-one abreast."
    )]
    pub business: SeatClassConfig,

    /// The premium-economy cabin.
    #[config(skip)]
    pub premium: SeatClassConfig,

    /// The economy cabin.
    #[config(
        nested,
        help = "Economy class, which is what the layout falls back to filling the whole cabin with when no class has been given a share."
    )]
    pub economy: SeatClassConfig,

    /// Aisle width, or zero to take it from the regulation.
    #[config(skip)]
    pub aisle_width_m: f64,

    /// Galleys, or zero to derive from the passenger count.
    #[config(skip)]
    pub galley_count: i64,

    /// Lavatories, or zero to derive from the passenger count.
    #[config(skip)]
    pub lavatory_count: i64,

    /// Checked baggage per passenger.
    #[config(skip)]
    pub checked_bag_mass_kg: f64,

    /// Revenue freight in whatever hold capacity the bags leave.
    #[config(skip)]
    pub belly_cargo_kg: f64,

    /// How far in from the skin the usable cabin starts.
    #[config(skip)]
    pub wall_thickness_m: f64,

    /// Closest two emergency-exit pairs can realistically be installed.
    #[config(skip)]
    pub min_exit_pair_spacing_m: f64,

    /// How much of a large exit's rated capacity is realistically achievable.
    #[config(skip)]
    pub exit_capacity_realism_factor: f64,
}

impl Default for PassengerCabinConfig {
    fn default() -> Self {
        // A conventional two-class short and medium-haul cabin: a small
        // business section up front and economy behind it. The geometry per
        // class is the mid-range of current in-service transport cabins.
        Self {
            class_mix_mode: "percent".to_owned(),
            first: SeatClassConfig::new(0.0, 1.93, 0.95, 96.0),
            business: SeatClassConfig::new(15.0, 1.55, 0.70, 90.0),
            premium: SeatClassConfig::new(0.0, 0.97, 0.52, 86.0),
            economy: SeatClassConfig::new(85.0, 0.79, 0.46, 84.0),
            aisle_width_m: 0.0,
            galley_count: 0,
            lavatory_count: 0,
            checked_bag_mass_kg: 16.0,
            belly_cargo_kg: 0.0,
            wall_thickness_m: 0.15,
            min_exit_pair_spacing_m: 11.0,
            exit_capacity_realism_factor: 0.478,
        }
    }
}

impl PassengerCabinConfig {
    /// Every product class slot with its name, forward to aft, present or not.
    pub fn all_classes(&self) -> [(&'static str, &SeatClassConfig); 3] {
        [
            (CLASS_NAMES[0], &self.first),
            (CLASS_NAMES[1], &self.business),
            (CLASS_NAMES[2], &self.economy),
        ]
    }

    /// The classes that have seats, forward to aft.
    pub fn classes(&self) -> Vec<(&'static str, &SeatClassConfig)> {
        self.all_classes()
            .into_iter()
            .filter(|(_, class)| class.is_present())
            .collect()
    }

    /// Total seats across every present class.
    pub fn total_seats(&self) -> i64 {
        self.classes().iter().map(|(_, class)| class.count).sum()
    }

    /// Resolve the cabin's canonical three-class FLOPS split.
    ///
    /// Count-mode values are deliberate installed-cabin declarations and take
    /// precedence over the requirements seed. A positive serialized
    /// premium-economy count is legacy data, so it is retained by folding it
    /// into FLOPS tourist/economy. Percent-mode counts are only trusted when
    /// they are already materialized and agree with the requested total;
    /// otherwise the requested total is allocated from the class shares. This
    /// gives every pre-layout caller a complete, coherent split without
    /// changing the cabin geometry or silently replacing a user's count-mode
    /// declaration with payload occupancy.
    pub fn resolved_flops_counts(&self, requested_passengers: i64) -> ResolvedPassengerCounts {
        let requested_passengers = requested_passengers.max(0);
        let declared = self.declared_flops_counts();

        if self.class_mix_mode == "count" && declared.is_nonempty() {
            return declared;
        }

        if self.class_mix_mode != "count" {
            let materialized = self.materialized_flops_counts(requested_passengers);
            if let Some(counts) = materialized {
                return counts;
            }
        }

        let weights = [
            self.first.share_pct,
            self.business.share_pct,
            self.economy.share_pct,
        ];
        let weights = if weights
            .iter()
            .all(|share| share.is_finite() && *share >= 0.0)
            && weights.iter().sum::<f64>() > 0.0
        {
            weights
        } else {
            [0.0, 0.0, 1.0]
        };
        let allocation = proportional_integer_allocation(requested_passengers, weights);
        ResolvedPassengerCounts {
            first: allocation[0],
            business: allocation[1],
            tourist: allocation[2],
        }
    }

    /// Return a product cabin with the hidden legacy Premium slot mapped to
    /// the public Economy slot whenever a saved count seed carries it.
    ///
    /// This is intentionally a clone-producing boundary. Validation and saved
    /// configuration inspection must preserve the user's original document,
    /// while payload layout and mass callers need the same three-class cabin.
    pub fn canonicalized_for_product(&self) -> Self {
        let mut canonical = self.clone();
        if canonical.premium.count > 0 {
            canonical.economy.count = canonical
                .economy
                .count
                .max(0)
                .saturating_add(canonical.premium.count);
            canonical.premium.count = 0;
        }
        canonical
    }

    fn declared_flops_counts(&self) -> ResolvedPassengerCounts {
        ResolvedPassengerCounts {
            first: self.first.count.max(0),
            business: self.business.count.max(0),
            tourist: self
                .economy
                .count
                .max(0)
                .saturating_add(self.premium.count.max(0)),
        }
    }

    fn materialized_flops_counts(
        &self,
        requested_passengers: i64,
    ) -> Option<ResolvedPassengerCounts> {
        if requested_passengers <= 0 {
            return Some(ResolvedPassengerCounts {
                first: 0,
                business: 0,
                tourist: 0,
            });
        }
        let counts = self.declared_flops_counts();
        (counts.total() == requested_passengers).then_some(counts)
    }

    /// Set every class slot's occupant mass from one combined passenger mass.
    ///
    /// A requirements-first passenger mass is a single load-case authority,
    /// even when a named cabin preset supplies the class geometry. That
    /// authority is defined as body plus baggage (`passenger_mass_kg`'s 100 kg
    /// standard), while the layout charges checked baggage separately per
    /// seated passenger through `checked_bag_mass_kg`. The per-class slot is
    /// therefore the occupant remainder: writing the combined mass into it as
    /// well counted every checked bag twice on the optimizer path (116 kg per
    /// seat against the 100 kg the report path carried for the same cabin).
    pub fn set_passenger_mass_kg(&mut self, combined_mass_per_passenger_kg: f64) {
        let occupant_kg =
            (combined_mass_per_passenger_kg - self.checked_bag_mass_kg.max(0.0)).max(0.0);
        self.first.mass_per_pax_kg = occupant_kg;
        self.business.mass_per_pax_kg = occupant_kg;
        self.premium.mass_per_pax_kg = occupant_kg;
        self.economy.mass_per_pax_kg = occupant_kg;
    }

    /// The single load-case authority for what one seated passenger costs the
    /// zero-fuel mass, of any class.
    ///
    /// `requirements.passenger_mass_kg` is FAA AC 120-27E / EASA standard
    /// mass: occupant plus checked baggage combined, and combined masses do
    /// not vary by class. A named cabin preset (or a hand-edited class) may
    /// still declare its own `mass_per_pax_kg` for display and for the seat
    /// geometry it seeds, but every product path (report, GUI preview,
    /// pipeline, export, acceptance and the optimizer) must reprice each
    /// seated passenger, whatever class fills the seat, as this combined
    /// mass: occupant = `passenger_mass_kg` - `checked_bag_mass_kg` (floored
    /// at zero), plus the bag. Calling this after a preset or a hand edit has
    /// written its own class masses is what removes the per-class premium
    /// (a business seat priced richer than an economy one) from the payload
    /// ledger; the reference-compatibility paths intentionally never call it,
    /// so the frozen Python parity fixtures keep their historical per-class
    /// masses unchanged.
    pub fn apply_passenger_mass_authority(&mut self, requirements: &DesignRequirements) {
        self.set_passenger_mass_kg(requirements.passenger_mass_kg);
    }

    /// Occupant plus checked baggage for one seat of `class`, kg: the mass a
    /// seated passenger adds to the zero-fuel mass.
    pub fn combined_mass_per_pax_kg(&self, class: &SeatClassConfig) -> f64 {
        class.mass_per_pax_kg + self.checked_bag_mass_kg.max(0.0)
    }

    /// Fix the total number of seats for a transient requirements load case.
    ///
    /// The class geometry remains the selected cabin seed, while the
    /// canonical brief owns the total passenger count. Existing class counts
    /// are preserved proportionally when available; otherwise the declared
    /// length shares provide the deterministic allocation. This keeps a named
    /// preset from replacing a brief passenger target with its own geometric
    /// capacity during preview or solver projection.
    pub fn set_fixed_passenger_count(&mut self, target: i64) {
        let target = target.max(0);
        let counts = [
            self.first.count.max(0) as f64,
            self.business.count.max(0) as f64,
            self.economy.count.max(0) as f64,
        ];
        let shares = [
            self.first.share_pct,
            self.business.share_pct,
            self.economy.share_pct,
        ];
        let count_total: f64 = counts.iter().sum();
        let weights = if count_total.is_finite() && count_total > 0.0 {
            counts
        } else if shares
            .iter()
            .all(|share| share.is_finite() && *share >= 0.0)
            && shares.iter().sum::<f64>() > 0.0
        {
            shares
        } else {
            [0.0, 0.0, 1.0]
        };
        let allocated = proportional_integer_allocation(target, weights);
        self.first.count = allocated[0];
        self.business.count = allocated[1];
        self.premium.count = 0;
        self.economy.count = allocated[2];
    }

    /// The class mix as normalized fractions of cabin length, forward to aft.
    ///
    /// Classes with no positive share are omitted. The shares are normalized,
    /// so they need not have been given as anything summing to a hundred.
    /// An empty result means no class asked for any length, which callers
    /// treat as nothing to lay out rather than dividing by zero.
    pub fn length_share_mix(&self) -> Vec<(&'static str, f64)> {
        let positive: Vec<(&'static str, f64)> = self
            .all_classes()
            .into_iter()
            .filter(|(_, class)| class.share_pct > 0.0)
            .map(|(name, class)| (name, class.share_pct))
            .collect();

        let total: f64 = positive.iter().map(|&(_, share)| share).sum();
        if total <= 0.0 {
            return Vec::new();
        }
        positive
            .into_iter()
            .map(|(name, share)| (name, share / total))
            .collect()
    }

    /// Write a class mix back into the per-class shares, as percentages,
    /// zeroing any class the mix does not name.
    ///
    /// A named preset picks its mix internally; without writing it back, the
    /// cabin page would go on showing the previous shares while the layout
    /// used the preset's, and nothing would indicate which of the two the run
    /// had actually flown.
    pub fn set_length_share_mix(&mut self, mix: &[(&str, f64)]) {
        let share_of = |name: &str| {
            mix.iter()
                .find(|(other, _)| *other == name)
                .map_or(0.0, |&(_, fraction)| fraction * 100.0)
        };
        self.first.share_pct = share_of(CLASS_NAMES[0]);
        self.business.share_pct = share_of(CLASS_NAMES[1]);
        self.premium.share_pct = 0.0;
        self.economy.share_pct = share_of(CLASS_NAMES[2]);
    }
}

/// The composed cabin and payload configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct CabinConfig {
    /// The passenger cabin.
    #[config(nested, help = "Passenger seating, monuments and baggage.")]
    pub passenger: PassengerCabinConfig,

    /// The cargo decks.
    #[config(nested, help = "Which decks carry freight, and how it is loaded.")]
    pub cargo: CargoDeckConfig,
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_cabin_is_two_classes_sharing_the_whole_length() {
        let mix = PassengerCabinConfig::default().length_share_mix();
        assert_eq!(mix.len(), 2);
        assert_eq!(mix[0].0, "Business");
        assert_eq!(mix[1].0, "Economy");
        let total: f64 = mix.iter().map(|&(_, share)| share).sum();
        assert!((total - 1.0).abs() < 1e-12);
    }

    #[test]
    fn shares_are_normalized_so_they_need_not_add_up_to_a_hundred() {
        let cabin = PassengerCabinConfig {
            business: SeatClassConfig::new(1.0, 1.55, 0.70, 90.0),
            economy: SeatClassConfig::new(3.0, 0.79, 0.46, 84.0),
            ..Default::default()
        };
        let mix = cabin.length_share_mix();
        assert!((mix[0].1 - 0.25).abs() < 1e-12);
        assert!((mix[1].1 - 0.75).abs() < 1e-12);
    }

    #[test]
    fn a_cabin_with_no_shares_lays_nothing_out_rather_than_dividing_by_zero() {
        let cabin = PassengerCabinConfig {
            business: SeatClassConfig::new(0.0, 1.55, 0.70, 90.0),
            economy: SeatClassConfig::new(0.0, 0.79, 0.46, 84.0),
            ..Default::default()
        };
        assert!(cabin.length_share_mix().is_empty());
    }

    #[test]
    fn writing_a_mix_back_zeroes_the_classes_it_does_not_name() {
        // A preset that dropped business has to leave business at zero, or
        // the page would keep showing a cabin the run did not fly.
        let mut cabin = PassengerCabinConfig::default();
        cabin.set_length_share_mix(&[("First", 0.2), ("Economy", 0.8)]);
        assert_eq!(cabin.first.share_pct, 20.0);
        assert_eq!(cabin.business.share_pct, 0.0);
        assert_eq!(cabin.premium.share_pct, 0.0);
        assert_eq!(cabin.economy.share_pct, 80.0);
    }

    #[test]
    fn a_mix_survives_a_round_trip_through_the_shares() {
        let mut cabin = PassengerCabinConfig::default();
        let original = cabin.length_share_mix();
        cabin.set_length_share_mix(&original);
        assert_eq!(cabin.length_share_mix(), original);
    }

    #[test]
    fn only_classes_with_seats_are_laid_out_and_counted() {
        let mut cabin = PassengerCabinConfig::default();
        assert!(cabin.classes().is_empty());
        assert_eq!(cabin.total_seats(), 0);

        cabin.business.count = 20;
        cabin.economy.count = 150;
        assert_eq!(cabin.classes().len(), 2);
        assert_eq!(cabin.total_seats(), 170);
    }

    #[test]
    fn the_combined_passenger_mass_is_split_between_occupant_and_checked_bag() {
        let mut cabin = PassengerCabinConfig::default();
        assert_eq!(cabin.checked_bag_mass_kg, 16.0);
        cabin.set_passenger_mass_kg(100.0);
        for (_, class) in cabin.classes() {
            assert_eq!(class.mass_per_pax_kg, 84.0);
            assert_eq!(cabin.combined_mass_per_pax_kg(class), 100.0);
        }
        cabin.set_passenger_mass_kg(10.0);
        assert_eq!(cabin.economy.mass_per_pax_kg, 0.0);
    }

    #[test]
    fn fixed_passenger_count_preserves_existing_class_proportions() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.business.count = 15;
        cabin.economy.count = 85;

        cabin.set_fixed_passenger_count(80);

        assert_eq!(cabin.total_seats(), 80);
        assert_eq!(cabin.business.count, 12);
        assert_eq!(cabin.economy.count, 68);
    }

    #[test]
    fn fixed_passenger_count_uses_shares_when_no_counts_are_present() {
        let mut cabin = PassengerCabinConfig::default();

        cabin.set_fixed_passenger_count(80);

        assert_eq!(cabin.total_seats(), 80);
        assert_eq!(cabin.business.count, 12);
        assert_eq!(cabin.economy.count, 68);
    }

    #[test]
    fn fixed_passenger_count_has_a_stable_largest_remainder_tie_break() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.share_pct = 1.0;
        cabin.business.share_pct = 1.0;
        cabin.economy.share_pct = 1.0;

        cabin.set_fixed_passenger_count(2);

        assert_eq!(cabin.all_classes().map(|(_, class)| class.count), [1, 1, 0]);
    }

    #[test]
    fn percent_resolution_allocates_the_requested_total_from_shares() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.share_pct = 10.0;
        cabin.business.share_pct = 20.0;
        cabin.economy.share_pct = 70.0;

        assert_eq!(
            cabin.resolved_flops_counts(101),
            ResolvedPassengerCounts {
                first: 10,
                business: 20,
                tourist: 71,
            }
        );
        assert_eq!(cabin.total_seats(), 0);
    }

    #[test]
    fn percent_resolution_keeps_a_materialized_layout_when_it_matches_the_target() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.class_mix_mode = "percent".to_owned();
        cabin.first.count = 2;
        cabin.business.count = 18;
        cabin.economy.count = 80;

        assert_eq!(
            cabin.resolved_flops_counts(100),
            ResolvedPassengerCounts {
                first: 2,
                business: 18,
                tourist: 80,
            }
        );
    }

    #[test]
    fn percent_resolution_discards_stale_counts_when_the_target_differs() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.share_pct = 25.0;
        cabin.business.share_pct = 25.0;
        cabin.economy.share_pct = 50.0;
        cabin.first.count = 2;
        cabin.business.count = 18;
        cabin.economy.count = 80;

        assert_eq!(
            cabin.resolved_flops_counts(200),
            ResolvedPassengerCounts {
                first: 50,
                business: 50,
                tourist: 100,
            }
        );
    }

    #[test]
    fn count_resolution_preserves_declared_classes_and_folds_legacy_premium() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.class_mix_mode = "count".to_owned();
        cabin.first.count = 4;
        cabin.business.count = 16;
        cabin.premium.count = 10;
        cabin.economy.count = 70;

        assert_eq!(
            cabin.resolved_flops_counts(999),
            ResolvedPassengerCounts {
                first: 4,
                business: 16,
                tourist: 80,
            }
        );
        assert_eq!(cabin.canonicalized_for_product().premium.count, 0);
        assert_eq!(cabin.canonicalized_for_product().economy.count, 80);
    }

    #[test]
    fn percent_mode_legacy_premium_survives_serialization_but_is_folded_for_product() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.count = 4;
        cabin.business.count = 16;
        cabin.premium.count = 10;
        cabin.economy.count = 70;

        let encoded = serde_json::to_string(&cabin).unwrap();
        let decoded: PassengerCabinConfig = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, cabin);
        assert_eq!(decoded.resolved_flops_counts(100).total(), 100);
        let product = decoded.canonicalized_for_product();
        assert_eq!(product.premium.count, 0);
        assert_eq!(product.economy.count, 80);
        assert_eq!(cabin.premium.count, 10);
    }

    #[test]
    fn a_resolved_split_downgrades_source_backed_cabin_provenance() {
        let mut mass_model = MassModelConfig::default();
        mass_model.flops_transport.provenance.cabin.evidence = FlopsInputEvidence::SourceBacked;

        annotate_flops_cabin_resolution(
            &mut mass_model,
            ResolvedPassengerCounts {
                first: 10,
                business: 20,
                tourist: 70,
            },
        );

        let provenance = &mass_model.flops_transport.provenance.cabin;
        assert_eq!(provenance.evidence, FlopsInputEvidence::UserDeclared);
        assert!(provenance
            .applicability
            .contains("canonical cabin resolution"));
        assert!(provenance
            .uncertainty
            .contains("serialized FLOPS class-count seed was superseded"));
    }

    #[test]
    fn a_second_cabin_resolution_replaces_the_first_provenance_split() {
        let mut mass_model = MassModelConfig::default();
        annotate_flops_cabin_resolution(
            &mut mass_model,
            ResolvedPassengerCounts {
                first: 10,
                business: 20,
                tourist: 70,
            },
        );
        mass_model.flops_transport.first_class_passenger_count = Some(10);
        mass_model.flops_transport.business_class_passenger_count = Some(20);
        mass_model.flops_transport.tourist_class_passenger_count = Some(70);
        annotate_flops_cabin_resolution(
            &mut mass_model,
            ResolvedPassengerCounts {
                first: 8,
                business: 22,
                tourist: 70,
            },
        );

        let applicability = &mass_model.flops_transport.provenance.cabin.applicability;
        assert!(applicability.contains("First=8"));
        assert!(!applicability.contains("First=10"));
    }

    #[test]
    fn empty_count_mode_uses_the_requirements_seed_until_the_layout_is_materialized() {
        let mut cabin = PassengerCabinConfig::default();
        cabin.class_mix_mode = "count".to_owned();
        cabin.first.share_pct = 0.0;
        cabin.business.share_pct = 0.0;
        cabin.economy.share_pct = 100.0;

        assert_eq!(
            cabin.resolved_flops_counts(73),
            ResolvedPassengerCounts {
                first: 0,
                business: 0,
                tourist: 73,
            }
        );
    }

    #[test]
    fn the_form_exposes_only_the_three_product_classes_in_cabin_order() {
        let schema = CabinConfig::default().schema();
        let crate::Entry::Node(passenger) = &schema.field("passenger").unwrap().entry else {
            panic!("the passenger cabin is a group");
        };
        let names: Vec<&str> = passenger
            .fields
            .iter()
            .filter(|field| matches!(field.entry, crate::Entry::Node(_)))
            .map(|field| field.name)
            .collect();
        assert_eq!(names, vec!["first", "business", "economy"]);
    }
}
