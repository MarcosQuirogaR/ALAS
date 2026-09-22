// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The integral fuel the structural box encloses, as a load-relieving mass.
//!
//! A transport wing's fuel lives in the same box this crate sizes: bounded
//! chordwise by the front and rear spars, spanwise by two rib stations, and
//! vertically by the wing skins. Its volume is therefore a property of the box
//! that [`crate::sizing`] already builds, not an independent number, and it is
//! derived here from the same [`WingStructureGeometry`] the sizing integrals
//! use. That matters for an optimised wing: a candidate with a deeper or wider
//! box carries more fuel and is relieved by more of it, which a fixed declared
//! mass could not represent.
//!
//! # What the constants are, and why they are not fitted
//!
//! Three numbers bound the tank inside the box, and each is the generic
//! transport value this product already declares elsewhere rather than a value
//! chosen here:
//!
//! * The span band. Every registered aircraft's outermost integral wing cell
//!   ends at `0.85` of the semi-span and the innermost begins between `0.09`
//!   and `0.12` (`alas_config::preset_fuel_tanks::layout_for`); a bare
//!   [`alas_config::WingTankConfig`] declares `0.10` as its own inboard
//!   default. `0.10 .. 0.85` is used here for every wing, and
//!   `the_declared_tank_band_matches_the_configuration_defaults` pins it to
//!   those declarations so the two cannot drift apart.
//! * The usable fraction. `alas_config::WingTankConfig::default` declares
//!   `0.92` as the share of the geometric spar-box volume that is usable tank
//!   volume "after ribs, stringers, systems and the expansion space", with
//!   ninety to ninety-five percent stated as typical. `0.92` is used here.
//! * The density. Jet A-1 at `0.80 kg/L` is the density every registered
//!   aircraft's reference data states for its published capacities.
//!
//! # Deliberate conservatisms
//!
//! The volume element is the streamwise section area times `dy`, so the
//! `1/cos` stretch a swept, dihedralled box gains along its own axis is not
//! credited: the tank comes out smaller than it is, the relief smaller, and
//! the sized box heavier. Aircraft that carry part of their fuel in a centre
//! tank extending into the inboard wing (A220-300, B787-9, AVE) have that
//! volume credited to the wing band here; it sits at a short moment arm, so
//! its effect on the root moment is second order. Both are recorded rather
//! than tuned.

use alas_geom::wing_structure::WingStructureGeometry;

/// Inboard boundary of the modelled integral tank, fraction of semi-span.
pub const INTEGRAL_TANK_SPAN_START_FRACTION: f64 = 0.10;

/// Outboard boundary of the modelled integral tank, fraction of semi-span.
///
/// Transport wing tanks stop short of the tip, leaving the outer bays dry for
/// the surge tank and the aileron; every registered aircraft declares `0.85`.
pub const INTEGRAL_TANK_SPAN_END_FRACTION: f64 = 0.85;

/// Share of the geometric spar-box volume that is usable tank volume.
pub const INTEGRAL_TANK_USABLE_FRACTION: f64 = 0.92;

/// Jet A-1 density, kg/m^3, at the value the registered reference data states.
pub const JET_FUEL_DENSITY_KG_M3: f64 = 800.0;

/// Chordwise samples used to integrate the enclosed box section.
const SECTION_SAMPLES: usize = 60;

/// Enclosed cross-section area of the structural box at `eta`, m^2.
///
/// The box is bounded by the outermost two spars and by the wing surfaces, so
/// this is `c^2` times the integral of the normalised section height across the
/// spar band.
pub fn box_section_area_m2(wsg: &WingStructureGeometry, eta: f64, front: f64, rear: f64) -> f64 {
    // The negated comparison is deliberate and is not `rear <= front`: a NaN
    // spar station compares false against everything, so the `<=` form would
    // let it through and integrate a NaN section. Only the negation rejects it.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(rear > front) {
        return 0.0;
    }
    let chord = wsg.local_chord(eta);
    if !chord.is_finite() || chord <= 0.0 {
        return 0.0;
    }
    let step = (rear - front) / (SECTION_SAMPLES - 1) as f64;
    let mut acc = 0.0;
    let mut previous = {
        let (zu, zl) = wsg.airfoil_zu_zl(eta, front);
        zu - zl
    };
    for index in 1..SECTION_SAMPLES {
        let xc = front + step * index as f64;
        let (zu, zl) = wsg.airfoil_zu_zl(eta, xc);
        let height = zu - zl;
        acc += 0.5 * (height + previous) * step;
        previous = height;
    }
    acc * chord * chord
}

/// Running mass of the usable integral fuel carried by one semi-wing, kg/m,
/// sampled at `y`.
///
/// Zero outside the declared tank band. The band is expressed in fractions of
/// the semi-span, so it follows the candidate's own wing rather than a fixed
/// station.
pub fn integral_fuel_running_mass_kg_m(
    wsg: &WingStructureGeometry,
    y: &[f64],
    front: f64,
    rear: f64,
) -> Vec<f64> {
    let semi_span = wsg.semi_span;
    if !semi_span.is_finite() || semi_span <= 0.0 {
        return vec![0.0; y.len()];
    }
    let density = JET_FUEL_DENSITY_KG_M3 * INTEGRAL_TANK_USABLE_FRACTION;
    y.iter()
        .map(|&station| {
            let eta = station / semi_span;
            if !(INTEGRAL_TANK_SPAN_START_FRACTION..=INTEGRAL_TANK_SPAN_END_FRACTION).contains(&eta)
            {
                return 0.0;
            }
            density * box_section_area_m2(wsg, eta, front, rear)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::preset_fuel_tanks;
    use alas_config::WingTankConfig;

    #[test]
    fn the_declared_tank_band_matches_the_configuration_defaults() {
        // The inboard station and the usable fraction are the product's own
        // generic integral-tank declaration, not values chosen in this crate.
        let generic = WingTankConfig::default();
        assert!((INTEGRAL_TANK_SPAN_START_FRACTION - generic.span_start_fraction).abs() < 1e-12);
        assert!((INTEGRAL_TANK_USABLE_FRACTION - generic.usable_fraction).abs() < 1e-12);

        // Every registered aircraft ends its outermost integral wing cell at
        // the same station, and the declared inboard edge sits inside the
        // spread of where their innermost cells begin.
        let mut innermost_declared = f64::INFINITY;
        let mut outermost_declared = f64::NEG_INFINITY;
        for preset in alas_config::presets::registry() {
            let Some(layout) = preset_fuel_tanks::layout_for(preset.name) else {
                continue;
            };
            let cells = [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing];
            let outermost = cells
                .iter()
                .filter(|cell| cell.enabled)
                .map(|cell| cell.span_end_fraction)
                .fold(f64::NEG_INFINITY, f64::max);
            let innermost = cells
                .iter()
                .filter(|cell| cell.enabled)
                .map(|cell| cell.span_start_fraction)
                .fold(f64::INFINITY, f64::min);
            if !outermost.is_finite() {
                continue;
            }
            assert!(
                (outermost - INTEGRAL_TANK_SPAN_END_FRACTION).abs() < 1e-9,
                "{} ends its outermost wing cell at {outermost}",
                preset.name
            );
            // The inboard station varies with where the centre tank stops,
            // between the A380-800's 0.09 and the A220-300's 0.32. The
            // declared band's inboard edge must sit inside that spread rather
            // than outside every aircraft in it.
            innermost_declared = innermost_declared.min(innermost);
            outermost_declared = outermost_declared.max(innermost);
        }
        assert!(
            innermost_declared.is_finite() && outermost_declared.is_finite(),
            "no registered aircraft declares an integral wing cell"
        );
        assert!(
            (innermost_declared..=outermost_declared).contains(&INTEGRAL_TANK_SPAN_START_FRACTION),
            "the declared inboard edge {INTEGRAL_TANK_SPAN_START_FRACTION} is outside the \
             registered spread {innermost_declared}..={outermost_declared}"
        );
    }

    #[test]
    fn a_degenerate_spar_band_encloses_no_fuel() {
        // A rear spar at or ahead of the front spar is not a box, and must
        // report no volume rather than a negative one.
        let wsg = probe_geometry();
        assert_eq!(box_section_area_m2(&wsg, 0.3, 0.70, 0.25), 0.0);
        assert_eq!(box_section_area_m2(&wsg, 0.3, 0.25, 0.25), 0.0);
    }

    #[test]
    fn the_fuel_band_is_dry_at_the_root_and_at_the_tip() {
        let wsg = probe_geometry();
        let y: Vec<f64> = (0..=100)
            .map(|i| i as f64 * wsg.semi_span / 100.0)
            .collect();
        let fuel = integral_fuel_running_mass_kg_m(&wsg, &y, 0.25, 0.70);
        assert_eq!(fuel[0], 0.0, "the centreline is inboard of the tank");
        assert_eq!(fuel[100], 0.0, "the tip is outboard of the tank");
        let wet: f64 = fuel.iter().sum();
        assert!(wet > 0.0, "the band between them must carry fuel");
        // Fuel density falls outboard with the box section.
        let inboard = fuel[15];
        let outboard = fuel[80];
        assert!(inboard > outboard && outboard > 0.0);
    }

    fn probe_geometry() -> WingStructureGeometry {
        use alas_config::{DesignVector, WingConfig};
        use alas_geom::airfoil_library::AirfoilLibrary;
        let section = AirfoilLibrary::get("naca2412").expect("the reference section resolves");
        WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &section,
            &section,
            &[0.25, 0.70],
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }
}
