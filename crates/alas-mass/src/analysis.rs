// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared finalization for mass and balance analyses.
//!
//! Keeping payload replacement and fuel recomputation in one private helper
//! prevents the legacy and checked product seams from drifting.
//!
//! # The two arms are one loading definition priced on two passenger counts
//!
//! [`complete_mass_analysis`] takes `payload_layout: Option<&PayloadLayoutSummary>`
//! and substitutes the layout's mass, station and fuel remainder when one is
//! supplied. Both arms describe the **same** loading definition, every seat
//! occupied at `requirements.passenger_mass_kg`, and they disagree only about
//! how many seats there are:
//!
//! * `None`, `requirements.payload_kg()`, which is
//!   `num_passengers * passenger_mass_kg`, at the cabin centre.
//! * `Some(layout)`, `seated_pax * passenger_mass_kg`, where `seated_pax` is
//!   the capacity the cabin engine resolves from the candidate's own geometry,
//!   at that cabin's mass-weighted centroid.
//!
//! Measured on four registered aircraft
//! (`alas-payload/examples/payload_placement_divergence.rs`), the layout total
//! is `seated_pax * passenger_mass_kg` **exactly**, `unseated_pax` is zero
//! throughout, and the non-occupant share is `0.0 kg` on three of the four:
//!
//! ```text
//! preset      num_passengers  seated_pax   lumped kg   layout kg   d %MAC
//! A220-300              130         145      13 000      14 500     16.82
//! A320-200              150         180      15 000      18 000     14.91
//! ATR72-600              72          70       7 200       7 000     21.73
//! A380-800              525         663      52 500      66 956      1.15
//! ```
//!
//! So this is **not** the max-structural-payload point that
//! `DesignRequirements::max_structural_payload_kg` describes (no belly freight
//! is involved), and the two arms are **not** two design load cases that should
//! be allowed to differ. `DesignRequirements` states the contract itself:
//! "Passenger capacity is always recomputed for each candidate shell", and
//! `resolves_payload_from_candidate_geometry` returns `true` unconditionally.
//! The geometry-resolved count is therefore the authoritative one and
//! `num_passengers` is a seed, so the `None` arm prices a count the product has
//! already declared superseded.
//!
//! What this function cannot do is say afterwards which arm it applied: it
//! overwrites `masses.payload`, `masses.fuel` and `coordinates.payload` in
//! place, so the returned triple is indistinguishable between the two while
//! differing by up to 14 456 kg of payload, 2.9 m of station and 21.7 points of
//! %MAC. `alas-report`'s `quick_preview_report` passes `None` (and it is what
//! the interface's centre-of-gravity envelope, landing-gear and control-surface
//! previews read) while every residual and export path passes `Some`.
//!
//! `the_supplied_layout_replaces_the_planning_payload_mass_station_and_fuel`
//! pins the substitution so the two arms cannot silently converge or drift.

use alas_config::DesignRequirements;

use crate::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, PayloadLayoutSummary, OEW_KEYS,
};

pub(crate) fn complete_mass_analysis(
    mut masses: MassBreakdown,
    mut coordinates: MassCoordinates,
    requirements: &DesignRequirements,
    payload_layout: Option<&PayloadLayoutSummary>,
) -> (MassBreakdown, MassCoordinates, [f64; 3]) {
    if let Some(layout) = payload_layout {
        if layout.total_mass > 0.0 {
            let m_oew: f64 = OEW_KEYS
                .iter()
                .map(|&key| masses.get(key).unwrap_or(0.0))
                .sum();
            masses.payload = layout.total_mass;
            masses.fuel = requirements.mtow_kg - (m_oew + masses.payload);
            let z_payload = coordinates.payload[2];
            coordinates.payload = [layout.cg_x, layout.cg_y, z_payload];
        }
    }

    let cg = calculate_physical_cg(&masses, &coordinates);
    (masses, coordinates, cg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breakdown::MassCoordinates;

    /// A breakdown whose operating-empty keys sum to a round 20 000 kg, so the
    /// fuel closure can be checked by hand.
    fn breakdown() -> MassBreakdown {
        MassBreakdown {
            wing: 10_000.0,
            h_stab: 0.0,
            v_stab: 0.0,
            fuselage: 10_000.0,
            gear: 0.0,
            propulsion: 0.0,
            systems: 0.0,
            furnishings: 0.0,
            payload: 5_000.0,
            fuel: 25_000.0,
        }
    }

    fn coordinates() -> MassCoordinates {
        MassCoordinates {
            wing: [0.0; 3],
            h_stab: [0.0; 3],
            v_stab: [0.0; 3],
            fuselage: [0.0; 3],
            gear: [0.0; 3],
            propulsion: [0.0; 3],
            systems: [0.0; 3],
            furnishings: [0.0; 3],
            payload: [20.0, 0.0, 1.5],
            fuel: [0.0; 3],
        }
    }

    #[test]
    fn the_supplied_layout_replaces_the_planning_payload_mass_station_and_fuel() {
        let requirements = DesignRequirements {
            mtow_kg: 50_000.0,
            ..Default::default()
        };
        let oew: f64 = OEW_KEYS
            .iter()
            .map(|&key| breakdown().get(key).unwrap_or(0.0))
            .sum();

        // No layout: the planning payload and its station stand untouched.
        let (lumped, lumped_coords, _) =
            complete_mass_analysis(breakdown(), coordinates(), &requirements, None);
        assert_eq!(lumped.payload, 5_000.0);
        assert_eq!(lumped_coords.payload, [20.0, 0.0, 1.5]);
        assert_eq!(lumped.fuel, 25_000.0, "the incoming fuel is not recomputed");

        // A layout: the same loading definition on the geometry-resolved seat
        // count. Mass, both horizontal coordinates and the fuel remainder all
        // move; the vertical station is kept because the layout does not
        // resolve one.
        let layout = PayloadLayoutSummary {
            total_mass: 9_000.0,
            cg_x: 23.5,
            cg_y: 0.25,
        };
        let (detailed, detailed_coords, _) =
            complete_mass_analysis(breakdown(), coordinates(), &requirements, Some(&layout));
        assert_eq!(detailed.payload, 9_000.0);
        assert_eq!(detailed_coords.payload, [23.5, 0.25, 1.5]);
        assert!(
            (detailed.fuel - (50_000.0 - (oew + 9_000.0))).abs() < 1e-9,
            "fuel {} kg is not the remainder of the substituted payload",
            detailed.fuel
        );
        // The substitution is what makes the two returned triples describe
        // different aircraft under one type, with nothing on the result saying
        // which. 4 000 kg of payload moved out of the fuel, and the station
        // moved 3.5 m aft.
        assert_eq!(detailed.payload - lumped.payload, 4_000.0);
        assert_eq!(lumped.fuel - detailed.fuel, 4_000.0);
        assert_eq!(detailed_coords.payload[0] - lumped_coords.payload[0], 3.5);
    }

    #[test]
    fn a_layout_carrying_no_mass_is_not_allowed_to_erase_the_planning_payload() {
        // A cabin that resolved nothing must leave the lumped case alone rather
        // than publish a zero-payload aircraft.
        let requirements = DesignRequirements::default();
        let empty = PayloadLayoutSummary {
            total_mass: 0.0,
            cg_x: 99.0,
            cg_y: 9.0,
        };
        let (masses, coords, _) =
            complete_mass_analysis(breakdown(), coordinates(), &requirements, Some(&empty));
        assert_eq!(masses.payload, 5_000.0);
        assert_eq!(coords.payload, [20.0, 0.0, 1.5]);
    }
}
