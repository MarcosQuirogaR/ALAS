// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`build_payload_layout`,
// `simulate_passenger_counts`)
// Reference: alas @ rust-port-baseline.

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

mod presets;

pub use presets::{
    apply_cabin_preset, apply_cabin_preset_reference_compatibility, CabinPresetError,
};

use alas_config::{AlasConfig, PassengerCabinConfig, SeatClassConfig};
use alas_geom::aircraft::airplane::Airplane;

use crate::cabin::{
    abreast, build_passenger_layout_reference_compatibility,
    build_passenger_layout_with_aircraft_cg_target, cabin_deck_segments, ceil_div,
    max_certifiable_capacity, min_exit_pairs, resolve_aisle_width, select_exit_type,
    service_reserve_len, MIN_PITCH, MONUMENT_LEN,
};
use crate::cargo::{build_cargo_layout, build_cargo_layout_reference_compatibility};
use crate::geometry::{CabinGeometry, CabinGeometryError};
use crate::layout::PayloadLayout;
use crate::numeric::round_half_even;

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

/// Rounding slack on the row-length budget, so a class whose rows divide its
/// share exactly is not denied its last row by a floating-point remainder.
const BUDGET_EPSILON: f64 = 1e-9;

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
        CabinGeometry::new(
            plane,
            &config.geometry,
            config.cabin.passenger.wall_thickness_m,
        )?
    };
    let requested_passengers =
        (!reference_compatibility).then_some(config.requirements.num_passengers);
    let mut effective = config.clone();
    if !reference_compatibility {
        presets::apply_cabin_preset_to_geometry(&mut effective, &g);
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
        // The selected cabin geometry determines the class proportions and
        // seat geometry, but the requirements' passenger count is the load
        // case authority. Materializing a Custom cabin used to overwrite it
        // with the shell's geometric capacity; that made an A320 preset turn
        // into a 106-passenger aircraft and a 787 into a 303-passenger one,
        // so the reported payload no longer matched the preset or its MZFW.
        let layout = if reference_compatibility {
            build_passenger_layout_reference_compatibility(
                &g,
                &config.cabin.passenger,
                &config.requirements,
            )
        } else {
            let mut product_config = config.clone();
            let requested = requested_passengers.unwrap_or_default();
            product_config
                .cabin
                .passenger
                .set_fixed_passenger_count(requested);
            product_config.requirements.num_passengers = requested;
            build_passenger_layout_with_aircraft_cg_target(
                &g,
                &product_config.cabin.passenger,
                &product_config.requirements,
                oew,
                x_oew,
                &product_config.cabin.cargo,
            )
        };
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
/// would report a count limited only by floor space -- 1,400 seats on an
/// A380-sized shell against the 853 the real aircraft is certified for.
pub fn simulate_passenger_counts(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    mix: &[(&str, f64)],
) -> PassengerCounts {
    simulate_passenger_counts_with_exit_semantics(g, pax, mix, false)
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
) -> PassengerCounts {
    simulate_passenger_counts_with_exit_semantics(g, pax, mix, true)
}

fn simulate_passenger_counts_with_exit_semantics(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    mix: &[(&str, f64)],
    product_exit_capacity: bool,
) -> PassengerCounts {
    let mut counts = PassengerCounts::default();
    let aisle_w = resolve_aisle_width(pax, TRANSPORT_CATEGORY_PAX);
    let deck_caps = max_certifiable_capacity(g, pax);
    let exit_spec = select_exit_type(g.diameter_m);
    let exit_cap = if product_exit_capacity {
        exit_spec.capacity_per_side * 2
    } else {
        exit_spec.capacity_per_side
    };

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
    let length_mix = length_mix_for_seat_targets(g, pax, target_mix);
    simulate_passenger_counts_product(g, pax, &length_mix)
}

fn length_mix_for_seat_targets<'a>(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    target_mix: &[(&'a str, f64)],
) -> Vec<(&'a str, f64)> {
    let mut weights = target_mix
        .iter()
        .filter(|(_, share)| share.is_finite() && *share > 0.0)
        .map(|&(name, share)| {
            let class = class_config(pax, name);
            let deck = &g.passenger_decks[0];
            let x = 0.5 * (g.cabin_start_x + g.cabin_end_x);
            let seats_abreast = abreast(class, deck, g, resolve_aisle_width(pax, 20), x).max(1);
            (
                name,
                share * class.pitch_m.max(MIN_PITCH) / seats_abreast as f64,
            )
        })
        .collect::<Vec<_>>();
    normalize_mix(&mut weights);

    // Row rounding makes the inverse discontinuous, so a damped multiplicative
    // correction is both more stable and more honest than pretending there is
    // a closed form. Twenty passes is tiny beside one geometry build.
    for _ in 0..20 {
        let counts = simulate_passenger_counts_product(g, pax, &weights);
        let total = counts.total().max(1) as f64;
        for (name, weight) in &mut weights {
            let target = target_mix
                .iter()
                .find(|(other, _)| other == name)
                .map_or(0.0, |(_, share)| *share);
            let achieved = counts.for_class(name) as f64 / total;
            let correction = if achieved > 0.0 {
                (target / achieved).clamp(0.25, 4.0).powf(0.65)
            } else {
                2.0
            };
            *weight *= correction;
        }
        normalize_mix(&mut weights);
    }
    weights
}

fn normalize_mix(mix: &mut [(&str, f64)]) {
    let total = mix.iter().map(|(_, share)| *share).sum::<f64>();
    if total > 0.0 && total.is_finite() {
        for (_, share) in mix {
            *share /= total;
        }
    }
}

/// One deck stretch, as the counting pass reads it.
struct Deck<'a> {
    geometry: &'a CabinGeometry,
    spec: &'a crate::geometry::DeckSpec,
    x0: f64,
    total_length: f64,
    cap: i64,
}

/// Seat one deck with `n_mid_bays` extra monument bays charged against it.
///
/// The row-length budget is shared across the whole deck rather than split
/// into per-class sections. Splitting it and truncating each class
/// independently throws away up to one row's worth of floor in *every* class,
/// which compounds to several rows across a four-class cabin. Earlier classes
/// take their share rounded to the nearest whole row, so the rounding averages
/// out instead of always undershooting, and the last class absorbs whatever is
/// left -- which is how an airline actually sets an exact business row count
/// and lets economy fill the rest.
fn count_deck(
    deck: &Deck<'_>,
    pax: &PassengerCabinConfig,
    classes: &[(&str, f64)],
    mix: &[(&str, f64)],
    aisle_w: f64,
    n_mid_bays: i64,
) -> (PassengerCounts, i64) {
    let mut local = PassengerCounts::default();
    let mut seated = 0i64;

    let bays = classes.len() as i64 + 1 + n_mid_bays;
    let l_seating = deck.total_length
        - bays as f64 * MONUMENT_LEN
        - service_reserve_len(deck.total_length, mix);
    if l_seating <= 0.0 {
        return (local, seated);
    }

    let mut x = deck.x0 + MONUMENT_LEN;
    let mut remaining = l_seating;
    for (i, &(name, share)) in classes.iter().enumerate() {
        let class = class_config(pax, name);
        let pitch = class.pitch_m.max(MIN_PITCH);
        let is_last = i == classes.len() - 1;
        let budget = if is_last {
            remaining
        } else {
            let rows = (round_half_even(share * l_seating / pitch) as i64).max(0);
            remaining.min(rows as f64 * pitch)
        };

        let mut n_rows = 0i64;
        while n_rows as f64 * pitch + pitch <= budget + BUDGET_EPSILON && seated < deck.cap {
            let row = abreast(class, deck.spec, deck.geometry, aisle_w, x).min(deck.cap - seated);
            local.add(name, row);
            seated += row;
            x += pitch;
            n_rows += 1;
        }
        remaining -= n_rows as f64 * pitch;
        if !is_last {
            x += MONUMENT_LEN;
        }
    }
    (local, seated)
}

/// The class slot a mix name refers to.
fn class_config<'a>(pax: &'a PassengerCabinConfig, name: &str) -> &'a SeatClassConfig {
    match name {
        "First" => &pax.first,
        "Business" => &pax.business,
        "Premium" => &pax.premium,
        _ => &pax.economy,
    }
}

// These are assertions over fixtures constructed in the test itself; a failed
// unwrap or expect is the assertion failing, not a library invariant.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod product_tests {
    use super::*;
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
    fn product_layout_preserves_the_requested_passenger_load_case() {
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
                summary.total_pax, preset.requirements.num_passengers,
                "{name} layout changed the requested load case"
            );
        }
    }

    #[test]
    fn product_sizing_counts_a_complete_exit_pair() {
        let g = geometry();
        let pax = PassengerCabinConfig::default();
        let mix = pax.length_share_mix();
        let compatibility = simulate_passenger_counts(&g, &pax, &mix);
        let product = simulate_passenger_counts_product(&g, &pax, &mix);

        assert!(product.total() > compatibility.total());
    }
}
