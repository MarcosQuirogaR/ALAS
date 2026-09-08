// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// Whether a body of this declared height and diameter is a double-decker.
fn is_double_deck(height_m: Option<f64>, diameter_m: f64) -> bool {
    height_m.is_some_and(|h| h >= diameter_m * 1.15)
}

/// The passenger decks and the lower hold for a body of this shape.
///
/// A double-decker's main deck sits low and its upper deck high, each taking
/// roughly half the section; a single-deck body's main deck takes nearly all
/// of it and the hold takes the bottom fifth.
fn decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    // Keep a full passenger cabin above the cargo ceiling;
                    // the small upward shift also leaves the lower hold a
                    // realistic ULD bay instead of a one-container-wide slit.
                    floor_frac: -0.20,
                    // A380-class upper floors leave roughly two metres of
                    // clear cabin height for the seat block and overhead bins.
                    ceil_frac: 0.32,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.38,
                    // The upper deck follows the crown; the remaining top
                    // shell is the structural/insulation margin, not cabin
                    // floor that can be sold as seats.
                    ceil_frac: 0.95,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.70,
                ceil_frac: -0.26,
                // Passenger widebody lower holds are arranged as two
                // half-width LD-family positions across the bay in the
                // published A380 loading plans. Keep a structural/rail
                // margin instead of filling the raw fuselage chord; tapered
                // nose/tail stations still reduce this to one position.
                width_factor: 0.72,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: 0.0,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.71,
                ceil_frac: -0.02 - MIN_DECK_SEPARATION_FRAC,
                // A narrowbody remains one ULD across, while a 5.6--6.0 m
                // widebody gets the two-across lower-hold arrangement seen
                // in aircraft cargo plans.
                width_factor: 0.90,
                is_passenger: false,
            },
        )
    }
}

/// Original Python deck table, retained only for explicit parity evidence.
fn reference_decks(height_m: Option<f64>, diameter_m: f64) -> (Vec<DeckSpec>, DeckSpec) {
    if is_double_deck(height_m, diameter_m) {
        (
            vec![
                DeckSpec {
                    name: crate::layout::MAIN,
                    floor_frac: -0.48,
                    ceil_frac: -0.02,
                    width_factor: 0.95,
                    is_passenger: true,
                },
                DeckSpec {
                    name: crate::layout::UPPER,
                    floor_frac: 0.04,
                    ceil_frac: 0.55,
                    width_factor: 0.80,
                    is_passenger: true,
                },
            ],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.50,
                width_factor: 0.55,
                is_passenger: false,
            },
        )
    } else {
        (
            vec![DeckSpec {
                name: crate::layout::MAIN,
                floor_frac: -0.18,
                ceil_frac: 0.95,
                width_factor: 0.97,
                is_passenger: true,
            }],
            DeckSpec {
                name: crate::layout::LOWER,
                floor_frac: -0.95,
                ceil_frac: -0.20,
                width_factor: 0.60,
                is_passenger: false,
            },
        )
    }
}

// A test asserts on geometry it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::build_payload_layout;
    use alas_config::AlasConfig;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn xsec(x: f64, radius: f64) -> FuselageXSec {
        FuselageXSec::new([x, 0.0, 0.0], Some(radius), None, None, DEFAULT_SHAPE)
            .expect("a radius alone is a valid section")
    }

    fn wing() -> Wing {
        Wing {
            name: MAIN_WING.to_owned(),
            xsecs: vec![
                WingXSec {
                    xyz_le: [10.0, 0.0, 0.0],
                    chord: 6.0,
                    twist: 0.0,
                    airfoil: Airfoil::from_coordinates("probe", Vec::new()),
                },
                WingXSec {
                    xyz_le: [14.0, 12.0, 0.0],
                    chord: 2.0,
                    twist: 0.0,
                    airfoil: Airfoil::from_coordinates("probe", Vec::new()),
                },
            ],
            symmetric: true,
        }
    }

    fn plane(sections: Vec<FuselageXSec>) -> Airplane {
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![wing()],
            fuselages: vec![Fuselage::new("Fuselage", sections)],
            s_ref: 100.0,
            c_ref: 4.0,
            b_ref: 24.0,
        }
    }

    fn geometry(sections: Vec<FuselageXSec>) -> CabinGeometry {
        CabinGeometry::new(&plane(sections), &GeometryConfig::default(), 0.15)
            .expect("a probe with a fuselage and a wing builds")
    }

    #[test]
    fn sections_are_sorted_so_a_body_given_tail_first_samples_the_same() {
        // The station order is an input, not a guarantee: `argsort` exists
        // upstream because a builder is free to emit the tail first.
        let forward = geometry(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        let reversed = geometry(vec![xsec(10.0, 2.0), xsec(0.0, 1.0)]);
        assert_eq!(forward.width_at(5.0), reversed.width_at(5.0));
        assert_eq!(forward.x_min, reversed.x_min);
        assert_eq!(forward.x_max, reversed.x_max);
    }

    #[test]
    fn an_airplane_with_no_fuselage_is_an_error_and_not_a_panic() {
        let mut bare = plane(vec![xsec(0.0, 1.0)]);
        bare.fuselages.clear();
        assert_eq!(
            CabinGeometry::new(&bare, &GeometryConfig::default(), 0.15),
            Err(CabinGeometryError::NoFuselage)
        );
    }

    #[test]
    fn the_public_layout_entry_point_preserves_geometry_errors() {
        let mut bare = plane(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        bare.fuselages.clear();

        assert_eq!(
            build_payload_layout(&bare, &AlasConfig::default(), 0.0, 0.0),
            Err(CabinGeometryError::NoFuselage),
            "layout construction must not turn a missing cabin into an empty payload"
        );
    }

    #[test]
    fn an_airplane_with_no_wing_is_an_error_and_not_a_panic() {
        // There is no mean aerodynamic chord to report a payload CG against,
        // and the optimizer does evaluate geometry this degenerate.
        let mut wingless = plane(vec![xsec(0.0, 1.0), xsec(10.0, 2.0)]);
        wingless.wings.clear();
        assert_eq!(
            CabinGeometry::new(&wingless, &GeometryConfig::default(), 0.15),
            Err(CabinGeometryError::NoWings)
        );
    }

    #[test]
    fn a_circular_body_gets_one_passenger_deck_and_an_ovoid_one_gets_two() {
        assert!(!is_double_deck(None, 7.14));
        assert!(!is_double_deck(Some(8.0), 7.14));
        assert!(is_double_deck(Some(8.41), 7.14));

        let (single, _) = decks(None, 6.2);
        assert_eq!(single.len(), 1);
        let (double, hold) = decks(Some(8.41), 7.14);
        assert_eq!(double.len(), 2);
        assert_eq!(double[1].name, crate::layout::UPPER);
        // The upper deck's floor has to clear the main deck's ceiling, or the
        // two cabins would be drawn through each other.
        assert!(double[1].floor_frac - double[0].ceil_frac >= MIN_DECK_SEPARATION_FRAC);
        assert!(double[0].floor_frac - hold.ceil_frac >= MIN_DECK_SEPARATION_FRAC);
        let (single, hold) = decks(None, 6.2);
        assert!(single[0].floor_frac - hold.ceil_frac >= MIN_DECK_SEPARATION_FRAC);
    }

    #[test]
    fn deck_width_is_the_ellipse_chord_at_its_actual_floor() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        let deck = &cabin.lower_deck;
        let floor_z = cabin.floor_z(deck, 5.0);
        assert_eq!(
            cabin.usable_width(deck, 5.0),
            cabin.usable_width_at_z(5.0, floor_z) * deck.width_factor
        );
        assert!(cabin.usable_width(deck, 5.0) < (4.0 - 2.0 * cabin.wall) * deck.width_factor);
    }

    #[test]
    fn rectangular_containment_checks_top_corners_not_only_the_centre() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        assert!(cabin
            .check_rectangular_prism(5.0, 1.0, 0.0, 1.0, -0.5, 1.0)
            .is_ok());
        let error = cabin
            .check_rectangular_prism(5.0, 1.0, 0.0, 3.0, 0.0, 1.5)
            .expect_err("the upper outer corners protrude through the ellipse");
        assert!(matches!(
            error,
            InteriorEnvelopeError::OutsideEnvelope { .. }
        ));
    }

    #[test]
    fn longitudinal_containment_checks_the_tapered_item_end() {
        let cabin = geometry(vec![xsec(0.0, 0.5), xsec(5.0, 2.0), xsec(10.0, 2.0)]);
        let error = cabin
            .check_rectangular_prism(3.0, 4.0, 0.0, 1.3, -0.25, 0.5)
            .expect_err("an item fitting at its centre still protrudes at its nose end");
        assert!(matches!(
            error,
            InteriorEnvelopeError::OutsideEnvelope { x, .. } if x == 1.0
        ));
    }

    #[test]
    fn invalid_item_extents_are_typed_instead_of_becoming_geometry() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        assert_eq!(
            cabin.check_rectangular_prism(5.0, -1.0, 0.0, 1.0, 0.0, 1.0),
            Err(InteriorEnvelopeError::InvalidExtent)
        );
    }

    #[test]
    fn crown_width_is_narrower_than_the_section_center() {
        let cabin = geometry(vec![xsec(0.0, 2.0), xsec(10.0, 2.0)]);
        let center = cabin.usable_width_at_z(5.0, 0.0);
        let crown = cabin.usable_width_at_z(5.0, 1.4);
        assert!(center > crown);
        assert!(crown > 0.0);
    }

    #[test]
    fn a_wall_thicker_than_the_body_leaves_a_floor_rather_than_inverting() {
        // A nose section is narrower than twice the wall, and a negative
        // usable width would put seats outside the aeroplane.
        let g = geometry(vec![xsec(0.0, 0.05), xsec(10.0, 2.0)]);
        assert_eq!(g.usable_width(&g.lower_deck, 0.0), 0.0);
        assert_eq!(g.internal_half_height(0.0), 0.1);
        assert!(g.deck_height(&g.lower_deck, 0.0) >= 0.3);
    }

    #[test]
    fn an_item_rests_on_the_floor_and_is_capped_by_the_ceiling() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        let deck = &g.passenger_decks[0];
        let tall = 99.0;
        assert_eq!(g.clamp_height(deck, 10.0, tall), g.deck_height(deck, 10.0));
        assert_eq!(
            g.item_z(deck, 10.0, tall),
            g.floor_z(deck, 10.0) + g.deck_height(deck, 10.0) / 2.0
        );
        // A short item's centre is half its own height above the floor.
        assert_eq!(g.item_z(deck, 10.0, 1.0), g.floor_z(deck, 10.0) + 0.5);
    }

    #[test]
    fn the_percent_mac_frame_round_trips() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        for pct in [-20.0, 0.0, 25.0, 140.0] {
            let x = g.pct_mac_to_x(pct);
            assert!((g.x_to_pct_mac(x) - pct).abs() < 1e-9);
        }
        // Zero percent MAC is the leading edge of the MAC, by definition.
        assert_eq!(g.pct_mac_to_x(0.0), g.x_lemac);
    }

    #[test]
    fn the_wing_box_spans_the_root_chord_from_its_leading_edge() {
        let g = geometry(vec![xsec(0.0, 2.0), xsec(20.0, 2.0)]);
        let (start, end) = g.wing_box_x_range();
        assert_eq!(start, 10.0);
        assert_eq!(end, 16.0);
    }
}
