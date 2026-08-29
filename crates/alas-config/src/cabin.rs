// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/cabin_config.py
// Reference: alas @ rust-port-baseline.

//! What the aircraft carries, and where in the fuselage it sits.
//!
//! This drives the detailed payload layout, which is built afresh for every
//! candidate the optimizer evaluates -- not only for the final design -- so
//! that the centre of gravity checked against the envelope is the one the
//! real seating and loading produce rather than a lumped estimate.
//!
//! # Four fixed classes rather than a list
//!
//! The passenger cabin has exactly four class slots: first, business,
//! premium and economy. A class with no seats is simply absent. A
//! variable-length list would be more general and would need per-field
//! interface code to render, which is the whole thing the generated settings
//! form exists to avoid; four slots cover every transport cabin anyone
//! configures here. If every class is empty the layout falls back to a single
//! economy cabin sized to the requested passenger count.

mod cargo;
mod seat_class;

pub use cargo::CargoDeckConfig;
pub use seat_class::SeatClassConfig;

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// The four class slots, forward to aft. Naming them once keeps the layout
/// order and the share mix from disagreeing about which cabin comes first.
const CLASS_NAMES: [&str; 4] = ["First", "Business", "Premium", "Economy"];

/// Passenger cabin: the class mix, the monuments, and the baggage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct PassengerCabinConfig {
    /// Whether the class mix is given as shares of length or as seat counts.
    #[config(
        label = "Class mix mode",
        help = "'percent': give each class a share of cabin length and let the layout solve the seat counts (recommended -- counts depend on pitch, abreast and fuselage shape). 'count': type exact per-class seat numbers instead."
    )]
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
    #[config(
        nested,
        help = "Premium economy: a wider seat at a longer pitch than economy, typically two-four-two abreast."
    )]
    pub premium: SeatClassConfig,

    /// The economy cabin.
    #[config(
        nested,
        help = "Economy class, which is what the layout falls back to filling the whole cabin with when no class has been given a share."
    )]
    pub economy: SeatClassConfig,

    /// Aisle width, or zero to take it from the regulation.
    #[config(
        help = "Width of the cabin aisle, or 0 to take the certified minimum for the passenger count: 0.30 m up to 19 passengers, and 0.51 m above that, which is the upper-body clearance that governs at seat and armrest level. Set it explicitly for a wider premium aisle."
    )]
    pub aisle_width_m: f64,

    /// Galleys, or zero to derive from the passenger count.
    #[config(
        help = "Number of galleys, or 0 to derive it from the passenger count at the standard provisioning ratio of roughly one per hundred passengers plus one."
    )]
    pub galley_count: i64,

    /// Lavatories, or zero to derive from the passenger count.
    #[config(
        help = "Number of lavatories, or 0 to derive it from the passenger count at the standard provisioning ratio of roughly one per forty-five passengers."
    )]
    pub lavatory_count: i64,

    /// Checked baggage per passenger.
    #[config(
        help = "Checked baggage mass per passenger, containerised into real lower-deck positions and trimmed toward the seating centre of gravity, as airlines trim bags. Separate from the seated mass on each class, which covers the occupant and their carry-on."
    )]
    pub checked_bag_mass_kg: f64,

    /// Revenue freight in whatever hold capacity the bags leave.
    #[config(
        help = "Revenue freight loaded into the lower-deck capacity remaining after checked bags (0 = none). Real passenger aircraft rarely fly with empty bellies. Capped at what the holds can still take, and it counts toward payload, so it trades against fuel within the takeoff weight."
    )]
    pub belly_cargo_kg: f64,

    /// How far in from the skin the usable cabin starts.
    #[config(
        help = "Inset per side from the outer skin to the usable cabin wall: frames and stringers, at least an inch of insulation, the standoff drain gap, and trim panels. 0.15 m is typical for a narrowbody."
    )]
    pub wall_thickness_m: f64,

    /// Closest two emergency-exit pairs can realistically be installed.
    #[config(
        help = "Minimum longitudinal spacing between adjacent emergency-exit pairs on a deck. This is what caps how many legally evacuable passengers a deck can hold: without it, the layout would fill the whole floor with seats regardless of whether enough exits could physically be fitted to evacuate them."
    )]
    pub min_exit_pair_spacing_m: f64,

    /// How much of a large exit's rated capacity is realistically achievable.
    #[config(
        help = "Derates the theoretical rating of Type-A exits alone when computing the aircraft's realistic capacity ceiling. A real certified capacity comes from a full evacuation demonstration, which for a widebody with several large doors lands well below the sum of each door's individual rating, because aisle throughput rather than door count becomes the limit. Smaller exit types track their nominal rating in practice and are left undiscounted, and this does not change how many exits are actually installed."
    )]
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
    /// Every class slot with its name, forward to aft, present or not.
    pub fn all_classes(&self) -> [(&'static str, &SeatClassConfig); 4] {
        [
            (CLASS_NAMES[0], &self.first),
            (CLASS_NAMES[1], &self.business),
            (CLASS_NAMES[2], &self.premium),
            (CLASS_NAMES[3], &self.economy),
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

    /// Set the same per-passenger mass on every class slot.
    ///
    /// A requirements-first passenger mass is a single load-case authority,
    /// even when a named cabin preset supplies the class geometry. Keeping
    /// this operation on the cabin type lets callers restore that authority
    /// after materialising a preset without duplicating field-by-field writes.
    pub fn set_passenger_mass_kg(&mut self, mass_per_passenger_kg: f64) {
        self.first.mass_per_pax_kg = mass_per_passenger_kg;
        self.business.mass_per_pax_kg = mass_per_passenger_kg;
        self.premium.mass_per_pax_kg = mass_per_passenger_kg;
        self.economy.mass_per_pax_kg = mass_per_passenger_kg;
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
            self.premium.count.max(0) as f64,
            self.economy.count.max(0) as f64,
        ];
        let shares = [
            self.first.share_pct,
            self.business.share_pct,
            self.premium.share_pct,
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
            [0.0, 0.0, 0.0, 1.0]
        };
        let allocated = proportional_integer_allocation(target, weights);
        self.first.count = allocated[0];
        self.business.count = allocated[1];
        self.premium.count = allocated[2];
        self.economy.count = allocated[3];
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
        self.premium.share_pct = share_of(CLASS_NAMES[2]);
        self.economy.share_pct = share_of(CLASS_NAMES[3]);
    }
}

fn proportional_integer_allocation(target: i64, weights: [f64; 4]) -> [i64; 4] {
    if target <= 0 {
        return [0; 4];
    }
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return [0, 0, 0, target];
    }

    let mut allocation = [0_i64; 4];
    let mut fractional = [0.0_f64; 4];
    let mut assigned = 0_i64;
    for (index, weight) in weights.into_iter().enumerate() {
        let raw = target as f64 * weight / total;
        let whole = raw.floor() as i64;
        allocation[index] = whole;
        fractional[index] = raw - whole as f64;
        assigned += whole;
    }

    // At most three seats remain after flooring four class allocations. The
    // stable index tie-break keeps saved runs reproducible.
    let mut remaining = (target - assigned).max(0);
    while remaining > 0 {
        let index = fractional
            .iter()
            .enumerate()
            .max_by(|(left_index, left), (right_index, right)| {
                left.total_cmp(right)
                    .then_with(|| right_index.cmp(left_index))
            })
            .map_or(3, |(index, _)| index);
        allocation[index] += 1;
        fractional[index] = f64::NEG_INFINITY;
        remaining -= 1;
    }
    allocation
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
        cabin.first.share_pct = 25.0;
        cabin.business.share_pct = 25.0;
        cabin.premium.share_pct = 25.0;
        cabin.economy.share_pct = 25.0;

        cabin.set_fixed_passenger_count(2);

        assert_eq!(
            cabin.all_classes().map(|(_, class)| class.count),
            [1, 1, 0, 0]
        );
    }

    #[test]
    fn the_classes_reach_the_form_as_four_groups_in_cabin_order() {
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
        assert_eq!(names, vec!["first", "business", "premium", "economy"]);
    }
}
