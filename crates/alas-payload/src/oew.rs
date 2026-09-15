// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`oew_and_cg`)
// Reference: alas @ rust-port-baseline.

//! The operating-empty mass and where it balances.
//!
//! This is the input the cargo loader trims against: told what the aeroplane
//! weighs empty and where that weight sits, it can solve backwards for the
//! payload centre of gravity that puts the *loaded* aircraft on its target
//! trim point. Without it the loader can only centre the payload on itself,
//! which is a different and less useful question: a hold trimmed to its own
//! centre still flies out of the envelope if the empty aircraft is nose-heavy.
//!
//! It lives here rather than in `alas-mass` because it is the payload
//! module that needs it and upstream puts it here for the same reason; the
//! component list it sums, [`OEW_KEYS`], stays the single canonical definition
//! in `alas-mass::breakdown` and is imported rather than restated.
//!
//! # A signature narrowed on purpose
//!
//! Upstream takes two `Dict`s and skips any component that has a mass and no
//! coordinate. Every one of its three call sites: `pipeline.py`,
//! `analysis/full_analysis.py` and `optimization/objective.py`: passes the
//! output of `calculate_component_masses` and `define_mass_coordinates`
//! together, and those two always populate the same ten names, so the missing
//! -coordinate branch is unreachable from this program's inputs. Taking the
//! typed pair instead of two maps makes it unreachable by construction as
//! well, which is the trade `alas-mass::breakdown` already made for the same
//! two dictionaries. The negative-mass guard is kept: it is reachable, because
//! an empirical weight correlation evaluated on a degenerate candidate can go
//! below zero, and the optimizer evaluates those.

use alas_mass::breakdown::{MassBreakdown, MassCoordinates, OEW_KEYS};

/// The operating empty weight and its longitudinal centre of gravity.
///
/// A component whose estimated mass came out negative contributes nothing
/// rather than subtracting moment, and an aircraft with no mass at all
/// balances at the origin rather than dividing by zero: the optimizer's
/// first evaluations reach both.
pub fn oew_and_cg(masses: &MassBreakdown, coords: &MassCoordinates) -> (f64, f64) {
    let positions = coords.as_pairs();
    let mut total = 0.0;
    let mut moment = 0.0;

    for key in OEW_KEYS {
        let mass = masses.get(key).unwrap_or(0.0).max(0.0);
        if mass <= 0.0 {
            continue;
        }
        if let Some((_, xyz)) = positions.iter().find(|&&(name, _)| name == key) {
            total += mass;
            moment += mass * xyz[0];
        }
    }

    (total, if total > 0.0 { moment / total } else { 0.0 })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn masses(wing: f64, fuselage: f64) -> MassBreakdown {
        MassBreakdown {
            wing,
            h_stab: 0.0,
            v_stab: 0.0,
            fuselage,
            gear: 0.0,
            propulsion: 0.0,
            systems: 0.0,
            furnishings: 0.0,
            payload: 40_000.0,
            fuel: 60_000.0,
        }
    }

    fn coords(wing_x: f64, fuselage_x: f64) -> MassCoordinates {
        let at = |x: f64| [x, 0.0, 0.0];
        MassCoordinates {
            wing: at(wing_x),
            h_stab: at(50.0),
            v_stab: at(52.0),
            fuselage: at(fuselage_x),
            gear: at(25.0),
            propulsion: at(20.0),
            systems: at(15.0),
            furnishings: at(30.0),
            payload: at(30.0),
            fuel: at(28.0),
        }
    }

    #[test]
    fn payload_and_fuel_are_not_part_of_the_operating_empty_weight() {
        // The whole point of the OEW component list is that it excludes the
        // two things a load plan is free to change; including them would make
        // the loader trim against a target that moves as it loads.
        let (total, cg) = oew_and_cg(&masses(10_000.0, 15_000.0), &coords(15.0, 20.0));
        assert_eq!(total, 25_000.0);
        assert_eq!(cg, (10_000.0 * 15.0 + 15_000.0 * 20.0) / 25_000.0);
    }

    #[test]
    fn an_aircraft_with_no_mass_balances_at_the_origin_rather_than_at_a_nan() {
        // A NaN here would propagate into the cargo solver's target and fail
        // an envelope check for a reason that has nothing to do with loading.
        let (total, cg) = oew_and_cg(&masses(0.0, 0.0), &coords(15.0, 20.0));
        assert_eq!(total, 0.0);
        assert_eq!(cg, 0.0);
    }

    #[test]
    fn a_negative_component_mass_is_dropped_and_not_subtracted() {
        // An empirical weight correlation on a degenerate candidate can go
        // below zero; subtracting its moment would drag the CG the wrong way
        // instead of simply ignoring an impossible component.
        let (total, cg) = oew_and_cg(&masses(-5_000.0, 15_000.0), &coords(15.0, 20.0));
        assert_eq!(total, 15_000.0);
        assert_eq!(cg, 20.0);
    }
}
