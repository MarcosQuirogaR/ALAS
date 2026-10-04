// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`build_payload_layout`,
// `simulate_passenger_counts`)

//! The entry point every consumer uses, and the fast auto-sizer that runs
//! before any interior is built.
//!
//! [`build_payload_layout`] is what the mass analysis, the deck plans and the
//! report all call. It is *not* a final-design-only step: the optimizer's
//! centre-of-gravity envelope check runs it on every candidate, because the
//! lumped cabin-density model that sizes the aircraft can be several percent
//! MAC away from where the real seating and loading put the payload, and
//! several percent MAC is the difference between a design inside its envelope
//! and one outside it.
//!
//! [`simulate_passenger_counts`] is the other half of that arrangement. It
//! answers "how many seats does this shell hold" without laying anything out,
//! which is what turns a class mix given as shares of cabin length into the
//! seat counts the requirements carry. It has to agree with the detailed
//! engine or a preset would advertise a capacity the layout cannot place, so it
//! reserves the same monument bays, applies the same exit-derived cap, and
//! reads the same [`crate::cabin::cabin_deck_segments`].

mod brief_cabin;
mod count;
mod mix_solve;
mod presets;

pub use presets::{
    apply_cabin_preset, apply_cabin_preset_reference_compatibility, CabinPresetError,
};

use alas_config::{AlasConfig, CertifiedExitLayout, PassengerCabinConfig};
use alas_geom::aircraft::airplane::Airplane;

use self::count::{class_config, count_deck, Deck};
use self::mix_solve::length_mix_for_seat_targets;
use crate::cabin::{
    build_passenger_layout_reference_compatibility, build_passenger_layout_with_aircraft_cg_target,
    cabin_deck_segments, ceil_div, count_declared_deck, effective_pair_capacity,
    largest_pair_rating, max_certifiable_capacity_reference_compatibility,
    max_certifiable_capacity_with_source_layout, min_exit_pairs, resolve_aisle_width,
    select_exit_type,
};
use crate::cargo::{build_cargo_layout, build_cargo_layout_reference_compatibility};
use crate::geometry::{CabinGeometry, CabinGeometryError};
use crate::layout::{LayoutSummary, PayloadLayout};

/// Resolve the passenger capacity ceiling for a registered aircraft.
///
/// The certified maximum (the exit-derived evacuation limit) always applies.
/// In a fixed-aircraft design basis
/// ([`alas_config::preset_policy::preset_mode_active`]: baseline sandbox or
/// reference adaptation) the percent-mode layout is also capped at the
/// preset's `planning_seats`, the manufacturer's typical cabin, so the analyzed
/// aircraft carries the seats its reference operating-empty mass belongs to
/// instead of a floor-filling layout. The cap is a ceiling: a count-mode cabin
/// declared below it is honored unchanged. A clean-sheet or unknown-preset
/// configuration has no source limit to apply, and the frozen compatibility
/// path deliberately preserves its historical geometry-only result.
pub(crate) fn registered_source_capacity_cap(
    config: &AlasConfig,
    reference_compatibility: bool,
) -> Option<i64> {
    if reference_compatibility || config.requirements.aircraft_type == "cargo" {
        return None;
    }
    let reference = &alas_config::presets::get(&config.preset).ok()?.reference;
    let (certified, planning) = seat_caps(config, reference);
    match (certified, planning) {
        (Some(certified), Some(planning)) => Some(certified.min(planning)),
        (certified, planning) => certified.or(planning),
    }
}

/// The certified maximum and the fixed-aircraft planning seat count of a
/// registered aircraft, each when it applies.
fn seat_caps(
    config: &AlasConfig,
    reference: &alas_config::AircraftReferenceData,
) -> (Option<i64>, Option<i64>) {
    let certified = reference.certified_max_seats.filter(|cap| *cap > 0);
    let planning = reference
        .planning_seats
        .filter(|cap| *cap > 0)
        .filter(|_| alas_config::preset_policy::preset_mode_active(config));
    (certified, planning)
}

/// Whether the planning seat count, not the certified maximum, is the lower
/// of the two source caps of [`registered_source_capacity_cap`].
fn planning_cap_binds(config: &AlasConfig, reference_compatibility: bool) -> bool {
    if reference_compatibility || config.requirements.aircraft_type == "cargo" {
        return false;
    }
    let Some(preset) = alas_config::presets::get(&config.preset).ok() else {
        return false;
    };
    match seat_caps(config, &preset.reference) {
        (Some(certified), Some(planning)) => planning < certified,
        (None, Some(_)) => true,
        _ => false,
    }
}

/// Name the planning seat count as the binding ceiling when it, rather than
/// the certified maximum, capped the row allocation.
fn label_planning_cap(layout: &mut PayloadLayout, planning_binds: bool) {
    if let LayoutSummary::Passenger(summary) = &mut layout.summary {
        if planning_binds && summary.capacity_binding == "source_certified_cap" {
            summary.capacity_binding = "planning_seat_cap";
        }
    }
}

/// The product cabin frame of `plane` under `config`: the sampled fuselage,
/// with the main deck bounded by the registered aircraft's declared door
/// stations where its source prints them.
///
/// # Errors
///
/// [`CabinGeometryError`], for an airplane the cabin frame cannot be sampled
/// from.
pub fn product_cabin_geometry(
    plane: &Airplane,
    config: &AlasConfig,
) -> Result<CabinGeometry, CabinGeometryError> {
    let layout = registered_source_exit_layout(config, false);
    Ok(CabinGeometry::new(
        plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )?
    .with_declared_doors(layout.as_ref()))
}

/// Resolve the exit arrangement of a passenger cabin: the source-defined one
/// of a registered preset, or the CS 25.807(g) derived one of a clean-sheet
/// brief. Frozen compatibility callers, registered aircraft without a
/// declared arrangement and the shipped reference brief retain the generic
/// diameter-based exit proxy.
pub(crate) fn registered_source_exit_layout(
    config: &AlasConfig,
    reference_compatibility: bool,
) -> Option<CertifiedExitLayout> {
    if reference_compatibility || config.requirements.aircraft_type == "cargo" {
        return None;
    }
    // A clean-sheet brief has no declared arrangement; it gets the smallest
    // CS 25.807(g) compliant one for its seat target. A registered aircraft
    // without a declared arrangement, and the shipped reference brief, keep
    // the generic proxy.
    if config.derives_clean_sheet_start() {
        let pax = &config.cabin.passenger;
        let seats = if pax.class_mix_mode == "count" && pax.total_seats() > 0 {
            pax.total_seats()
        } else {
            config.requirements.num_passengers
        };
        return crate::cabin::derived_exit_layout(config.geometry.fuselage.diameter_m, seats);
    }
    alas_config::presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.certified_exit_layout)
}

/// The class slots, forward to aft. The auto-sizer walks them in this order
/// whatever order the mix was written in, so two mixes naming the same shares
/// lay out the same cabin.
const CLASS_ORDER: [&str; 4] = ["First", "Business", "Premium", "Economy"];

/// The passenger count the auto-sizer resolves its aisle width against.
///
/// Transport-category sizing always targets at least twenty seats, so the
/// FAR/CS-25.815 rule resolves to the twenty-inch upper-body clearance rather
/// than to the narrow-cabin minimum, and pinning it here keeps the width from
/// depending on a seat count this function has not worked out yet.
const TRANSPORT_CATEGORY_PAX: i64 = 20;

/// Build the detailed payload layout for `plane` under `config`.
///
/// Dispatches on the requested aircraft type. `oew` and `x_oew` are the
/// operating-empty mass and its centre of gravity, which let the freighter
/// loader place the load so the *aircraft* balances on target; both may be zero
/// when only the payload's own balance is wanted, and the passenger cabin
/// ignores them entirely.
///
/// # Errors
///
/// [`CabinGeometryError`], for an airplane the cabin frame cannot be sampled
/// from.
pub fn build_payload_layout(
    plane: &Airplane,
    config: &AlasConfig,
    oew: f64,
    x_oew: f64,
) -> Result<PayloadLayout, CabinGeometryError> {
    build_payload_layout_with_mass_semantics(plane, config, oew, x_oew, false)
}

/// Build a payload layout using the frozen cargo gross-target correction used
/// by the Python parity fixture. Product analyses should call
/// [`build_payload_layout`].
pub fn build_payload_layout_reference_compatibility(
    plane: &Airplane,
    config: &AlasConfig,
    oew: f64,
    x_oew: f64,
) -> Result<PayloadLayout, CabinGeometryError> {
    build_payload_layout_with_mass_semantics(plane, config, oew, x_oew, true)
}

fn build_payload_layout_with_mass_semantics(
    plane: &Airplane,
    config: &AlasConfig,
    oew: f64,
    x_oew: f64,
    reference_compatibility: bool,
) -> Result<PayloadLayout, CabinGeometryError> {
    let g = if reference_compatibility {
        CabinGeometry::new_reference_compatibility(
            plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )?
    } else {
        product_cabin_geometry(plane, config)?
    };
    let source_capacity_cap = registered_source_capacity_cap(config, reference_compatibility);
    let source_exit_layout = registered_source_exit_layout(config, reference_compatibility);
    let mut effective = config.clone();
    let mut explicit_count_cabin = false;
    if !reference_compatibility {
        // Premium is retained in saved files but the product layout has the
        // same three-class authority as FLOPS.  Canonicalize before deciding
        // whether an installed count cabin is present, so a legacy Premium
        // count cannot disappear from the payload total.
        effective.cabin.passenger = effective.cabin.passenger.canonicalized_for_product();
        explicit_count_cabin = effective.requirements.aircraft_type == "passenger"
            && effective.cabin.passenger.class_mix_mode == "count"
            && effective.cabin.passenger.total_seats() > 0;
        if explicit_count_cabin {
            // Count mode is an installed-cabin declaration.  A named preset
            // may not replace it with its geometry-derived capacity.
            effective.requirements.num_passengers = effective.cabin.passenger.total_seats();
        } else {
            presets::apply_cabin_preset_to_geometry(
                &mut effective,
                &g,
                source_capacity_cap,
                source_exit_layout,
            );
            // A clean-sheet brief's seat target becomes a count cabin there,
            // and a count is reported short, never replaced by the geometry.
            explicit_count_cabin = effective.requirements.aircraft_type == "passenger"
                && effective.cabin.passenger.class_mix_mode == "count"
                && effective.cabin.passenger.total_seats() > 0;
        }
        // `requirements.passenger_mass_kg` is the single product load-case
        // authority (occupant plus checked bag, the same for every class):
        // reprice each class slot after the preset wrote its geometry seed,
        // so every product path agrees with the optimizer's
        // `apply_candidate_payload_load_case`; the reference-compatibility
        // branch keeps the frozen per-class masses.
        let requirements = effective.requirements.clone();
        effective
            .cabin
            .passenger
            .apply_passenger_mass_authority(&requirements);
    }
    let config = if reference_compatibility {
        config
    } else {
        &effective
    };
    if config.requirements.aircraft_type == "cargo" {
        let layout = if reference_compatibility {
            build_cargo_layout_reference_compatibility(
                &g,
                &config.cabin.cargo,
                &config.requirements,
                oew,
                x_oew,
            )
        } else {
            build_cargo_layout(&g, &config.cabin.cargo, &config.requirements, oew, x_oew)
        };
        Ok(layout)
    } else {
        // Percent-mode cabin geometry determines the class proportions and
        // fills the usable floor for every registered or clean-sheet study.
        // A nonempty count-mode cabin is the exception: its installed count
        // is explicit and the layout reports any shortfall instead of
        // replacing it with computed capacity.
        let layout = if reference_compatibility {
            build_passenger_layout_reference_compatibility(
                &g,
                &config.cabin.passenger,
                &config.requirements,
            )
        } else {
            let mut product_config = config.clone();
            let mut layout = build_passenger_layout_with_aircraft_cg_target(
                &g,
                &product_config.cabin.passenger,
                &product_config.requirements,
                oew,
                x_oew,
                &product_config.cabin.cargo,
                source_capacity_cap,
                source_exit_layout,
            );

            // The fast capacity solver and the detailed row packer share the
            // same geometry rules, but row placement still has a few discrete
            // edge cases (most visibly at a class boundary on a 787). No
            // study carries an explicit passenger target to fall back on, so
            // every candidate must never expose an unseated passenger: close
            // that last-row gap against the actual product layout.
            if !explicit_count_cabin {
                for _ in 0..4 {
                    let LayoutSummary::Passenger(summary) = &layout.summary else {
                        break;
                    };
                    if summary.unseated_pax == 0 {
                        break;
                    }
                    let target = summary.seated_pax;
                    product_config
                        .cabin
                        .passenger
                        .set_fixed_passenger_count(target);
                    product_config.requirements.num_passengers = target;
                    layout = build_passenger_layout_with_aircraft_cg_target(
                        &g,
                        &product_config.cabin.passenger,
                        &product_config.requirements,
                        oew,
                        x_oew,
                        &product_config.cabin.cargo,
                        source_capacity_cap,
                        source_exit_layout,
                    );
                }
            }
            layout
        };
        let mut layout = layout;
        label_planning_cap(
            &mut layout,
            planning_cap_binds(config, reference_compatibility),
        );
        Ok(layout)
    }
}

/// Seats per class, as the auto-sizer works them out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassengerCounts {
    /// First-class seats.
    pub first: i64,
    /// Business seats.
    pub business: i64,
    /// Premium-economy seats.
    pub premium: i64,
    /// Economy seats.
    pub economy: i64,
}

impl PassengerCounts {
    /// Seats across every class.
    pub fn total(self) -> i64 {
        self.first + self.business + self.premium + self.economy
    }

    /// One class's seats, by the name the mix uses.
    pub fn for_class(self, name: &str) -> i64 {
        match name {
            "First" => self.first,
            "Business" => self.business,
            "Premium" => self.premium,
            _ => self.economy,
        }
    }

    fn add(&mut self, name: &str, seats: i64) {
        match name {
            "First" => self.first += seats,
            "Business" => self.business += seats,
            "Premium" => self.premium += seats,
            _ => self.economy += seats,
        }
    }
}

/// How many seats of each class this shell holds under a length mix.
///
/// This drives `requirements.num_passengers`, and through it the first-pass
/// lumped payload mass every caller sees before the real cabin is built. It is
/// capped per deck at [`max_certifiable_capacity`] for the same reason the
/// detailed engine is: without that cap a high-density preset on a large body
/// would report a count limited only by floor space: 1,400 seats on an
/// A380-sized shell against the 853 the real aircraft is certified for.
pub fn simulate_passenger_counts(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    mix: &[(&str, f64)],
) -> PassengerCounts {
    simulate_passenger_counts_with_exit_semantics(g, pax, mix, false, None, None)
}

/// Product cabin sizing with the evacuation capacity of a complete exit pair.
///
/// The public [`simulate_passenger_counts`] entry point is retained for the
/// frozen Python fixture, whose historical implementation divided by the
/// capacity of one side of an exit. Product calculations use both exits in a
/// pair; otherwise the sizing pass installs unnecessary mid-cabin bays and
/// removes real seat rows.
fn simulate_passenger_counts_product(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    mix: &[(&str, f64)],
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> PassengerCounts {
    simulate_passenger_counts_with_exit_semantics(
        g,
        pax,
        mix,
        true,
        source_capacity_cap,
        source_exit_layout,
    )
}

fn simulate_passenger_counts_with_exit_semantics(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    mix: &[(&str, f64)],
    product_exit_capacity: bool,
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> PassengerCounts {
    let mut counts = PassengerCounts::default();
    let aisle_w = resolve_aisle_width(pax, TRANSPORT_CATEGORY_PAX);
    let deck_caps = if product_exit_capacity {
        max_certifiable_capacity_with_source_layout(g, pax, source_exit_layout, source_capacity_cap)
    } else {
        max_certifiable_capacity_reference_compatibility(g, pax)
    };
    let exit_spec = select_exit_type(g.diameter_m);
    let exit_cap = if let Some(source_exit_layout) = source_exit_layout {
        largest_pair_rating(&source_exit_layout)
    } else if product_exit_capacity {
        effective_pair_capacity(exit_spec, pax)
    } else {
        exit_spec.capacity_per_pair
    };

    let first_deck = g.passenger_decks.first().map(|deck| deck.name);
    for segment in cabin_deck_segments(g) {
        let total_length = segment.x1 - segment.x0;
        let deck_cap = deck_caps.for_deck(segment.deck.name);

        let classes: Vec<(&str, f64)> = CLASS_ORDER
            .iter()
            .filter_map(|&name| {
                let share = mix
                    .iter()
                    .find(|(other, _)| *other == name)
                    .map_or(0.0, |&(_, share)| share);
                (share > 0.0).then_some((name, share))
            })
            .collect();
        if classes.is_empty() {
            continue;
        }
        // A main deck bounded by declared doors is counted by walking its
        // seats between the doors, the walk the detailed engine places with.
        if product_exit_capacity
            && !g.door_stations.is_empty()
            && Some(segment.deck.name) == first_deck
        {
            let declared: Vec<_> = classes
                .iter()
                .map(|&(name, share)| (name, class_config(pax, name), share))
                .collect();
            let (x0, x1, deck) = (segment.x0, segment.x1, segment.deck);
            let seats = count_declared_deck(g, deck, x0, x1, pax, &declared, aisle_w, deck_cap);
            for (name, seats) in seats {
                counts.add(name, seats);
            }
            continue;
        }

        // Two passes, as the detailed engine makes: one with no mid-cabin bays
        // to find the door-pair count, then one charging a bay per door
        // interval against the seating length.
        let deck = Deck {
            geometry: g,
            spec: segment.deck,
            x0: segment.x0,
            total_length,
            cap: deck_cap,
        };
        let (_, seated) = count_deck(&deck, pax, &classes, mix, aisle_w, 0);
        let n_pairs = if seated == 0 {
            1
        } else {
            min_exit_pairs(seated).max(ceil_div(seated, exit_cap))
        };
        let (local, _) = count_deck(&deck, pax, &classes, mix, aisle_w, (n_pairs - 1).max(0));

        for &(name, _) in &classes {
            counts.add(name, local.for_class(name));
        }
    }
    counts
}

/// Convert a requested *seat-count* mix into the floor-length mix the row
/// packer needs, then return the capacity of this particular shell.
///
/// Airlines publish seats by class, while floor length is an internal packing
/// quantity.  Premium seats consume more pitch and fewer fit abreast, so using
/// the published percentages directly as length percentages would materially
/// overstate their passenger share.  This small deterministic inverse solve
/// closes that mismatch while whole rows and exit/service reserves remain in
/// the forward simulation.
pub fn simulate_passenger_counts_for_seat_mix(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    target_mix: &[(&str, f64)],
) -> PassengerCounts {
    simulate_passenger_counts_for_seat_mix_with_source_cap(g, pax, target_mix, None, None)
}

/// Product cabin sizing for a registered aircraft, with its immutable source
/// passenger cap applied during the inverse mix solve and the final row
/// packing.  The public three-argument helper retains the clean-sheet API.
pub(crate) fn simulate_passenger_counts_for_seat_mix_with_source_cap(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    target_mix: &[(&str, f64)],
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> PassengerCounts {
    let length_mix =
        length_mix_for_seat_targets(g, pax, target_mix, source_capacity_cap, source_exit_layout);
    simulate_passenger_counts_product(g, pax, &length_mix, source_capacity_cap, source_exit_layout)
}

// These are assertions over fixtures constructed in the test itself; a failed
// unwrap or expect is the assertion failing, not a library invariant.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod product_tests {
    use super::*;
    use crate::cabin::max_certifiable_capacity;
    use crate::layout::LayoutSummary;
    use alas_config::{presets, GeometryConfig};
    use alas_geom::builder::AircraftBuilder;

    fn geometry() -> CabinGeometry {
        let builder = AircraftBuilder::new(Some(GeometryConfig::default()));
        let plane = builder.build(None, false).expect("default aircraft builds");
        CabinGeometry::new(&plane, &builder.geometry, 0.15).expect("default cabin samples")
    }

    #[test]
    fn published_seat_shares_are_not_used_as_floor_length_shares() {
        let g = geometry();
        let mut pax = PassengerCabinConfig::default();
        pax.first.abreast = 4;
        pax.business.abreast = 4;
        let target = [
            ("First", 14.0 / 519.0),
            ("Business", 76.0 / 519.0),
            ("Economy", 429.0 / 519.0),
        ];
        let direct = simulate_passenger_counts(&g, &pax, &target);
        let solved = simulate_passenger_counts_for_seat_mix(&g, &pax, &target);

        assert_ne!(direct, solved);
        let total = solved.total() as f64;
        assert!((solved.first as f64 / total - target[0].1).abs() < 0.04);
        assert!((solved.business as f64 / total - target[1].1).abs() < 0.04);
    }

    #[test]
    fn product_layout_reports_registered_capacity_or_an_explicit_source_gap() {
        for name in [
            "AVE",
            "A220-300",
            "A320-200",
            "A340-300",
            "A380-800",
            "ATR72-600",
            "B787-9",
            "DC-10",
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("registered preset loads");
            let preset = presets::get(name).expect("registered preset resolves");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), true)
                .expect("preset geometry builds");
            let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
                .expect("product passenger layout builds");
            let LayoutSummary::Passenger(summary) = layout.summary else {
                panic!("passenger preset selected a cargo layout");
            };
            assert_eq!(
                summary.unseated_pax, 0,
                "{name} preset left passengers without seats"
            );
            assert_eq!(
                summary.total_pax, summary.seated_pax,
                "{name} preset reported a passenger shortfall"
            );
            if name == "DC-10" {
                // The DC-10 source record publishes a planning seat count
                // without a revision-locked exit-pair arrangement and LOPA
                // that this generic cabin can reproduce.  Keep the corrected
                // complete-pair proxy visible as a source/layout gap instead
                // of silently doubling a pair rating or treating a planning
                // count as a hard floor-fill target.  The common assertions
                // above still require a physically consistent row pack: no
                // passenger is reported seated without a mass-bearing row.
                assert_eq!(summary.source_exit_layout, None);
                // Its planning-seat ceiling is below the geometric exit sum,
                // which the summary reports as the cap even though this shell
                // cannot seat that many.
                assert_eq!(summary.capacity_binding, "planning_seat_cap");
                assert!(
                    summary.total_pax < preset.requirements.num_passengers,
                    "{name} source/layout gap was hidden by a planning-count pin"
                );
            } else {
                assert!(
                    summary.total_pax >= preset.requirements.num_passengers,
                    "{name} preset capacity regressed below its published planning load: {} of {}",
                    summary.total_pax,
                    preset.requirements.num_passengers
                );
            }
        }
    }

    #[test]
    fn product_layout_preserves_named_nonempty_count_cabins_and_reports_shortfall() {
        let preset = presets::get("AVE").expect("AVE preset");
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" }))
            .expect("AVE configuration");
        // A named preset must not replace this installed count with its own
        // geometry-derived capacity, and the count is intentionally larger
        // than the AVE cabin so the layout's shortfall remains observable.
        config.requirements.cabin_preset = "Emirates".to_owned();
        config.cabin.passenger.class_mix_mode = "count".to_owned();
        config.cabin.passenger.first.count = 100;
        config.cabin.passenger.business.count = 100;
        config.cabin.passenger.premium.count = 0;
        config.cabin.passenger.economy.count = 600;
        config.requirements.num_passengers = 800;

        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("AVE geometry builds");
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("explicit count cabin layout builds");
        let LayoutSummary::Passenger(summary) = layout.summary else {
            panic!("count cabin selected a cargo layout");
        };
        assert_eq!(summary.total_pax, 800);
        assert!(summary.seated_pax <= summary.total_pax);
        assert!(
            summary.unseated_pax > 0,
            "the explicit shortfall must remain visible"
        );
        assert_eq!(
            summary.seated_pax + summary.unseated_pax,
            summary.total_pax,
            "payload must account for every declared passenger"
        );
    }

    #[test]
    fn a_seat_ceiling_is_reached_when_the_floor_allows_it() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" })).unwrap();
        let preset = presets::get("A380-800").unwrap();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        let g = CabinGeometry::new(
            &plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )
        .unwrap();
        let pax = &config.cabin.passenger;
        let mix = pax.length_share_mix();
        let seats =
            |cap| simulate_passenger_counts_for_seat_mix_with_source_cap(&g, pax, &mix, cap, None);
        // The floor holds 663 seats; a ceiling below that must be filled to
        // the seat rather than left a whole business row short of it, and a
        // ceiling above the floor must not change the uncapped answer.
        assert_eq!(seats(None).total(), 663);
        assert_eq!(seats(Some(868)), seats(None));
        assert_eq!(seats(Some(555)).total(), 555);
        assert_eq!(seats(Some(600)).total(), 600);
        assert_eq!(seats(Some(555)).business, 85);
    }

    #[test]
    fn product_sizing_uses_complete_exit_pair_ratings() {
        let g = geometry();
        let pax = PassengerCabinConfig::default();
        let spec = select_exit_type(g.diameter_m);

        // The regulatory Type-A value is already the rating of a complete
        // pair.  The serialized 0.478 field is the legacy half-pair form;
        // the product conversion makes its effective pair utilization
        // explicit and bounded at 0.956.
        assert_eq!(spec.name, "A");
        assert_eq!(spec.capacity_per_pair, 110);
        assert_eq!(effective_pair_capacity(spec, &pax), 105);
        assert!((2.0 * pax.exit_capacity_realism_factor) <= 1.0);

        // Five geometry-derived pairs therefore produce 525 seats.  A
        // product pair must never be doubled again merely because two door
        // cut-outs are emitted for it.
        let caps = max_certifiable_capacity(&g, &pax);
        assert_eq!(caps.total, 5 * effective_pair_capacity(spec, &pax));
    }
}
