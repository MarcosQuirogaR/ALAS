// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cabin per case: percent-mode FLOPS cabin terms follow the seated
//! layout, while count-mode terms retain the declared installed cabin.
//!
//! The first mass pass has to run before a cabin exists, so it evaluates the
//! registered or declared passenger counts. Once the detailed layout has
//! seated a percent-mode cabin, the furnishings, passenger-service, cabin-
//! crew and air-conditioning terms must describe *that* cabin: the one whose
//! occupants are in the zero-fuel mass, and not a seed count the dynamic
//! capacity solve has since replaced. A count-mode cabin is an installed
//! equipment declaration, so a layout shortfall is reported by payload while
//! its declared furnishings and service terms remain intact. Before this seam
//! the product report could carry a 150-seat operating empty mass under a
//! 180-seat payload.
//!
//! The optimizer path already keeps the two in step
//! (`alas_opt::objective_model::apply_candidate_payload_load_case`); this is
//! the same rule at the report boundary.

use alas_config::{
    AlasConfig, DesignRequirements, MassModelConfig, MassSizingBasis, PassengerCabinConfig,
};

pub use alas_opt::mdo::cabin_sync::cabin_synchronized_for_cabin;
use alas_opt::mdo::cabin_sync::with_flops_counts;

/// Resolve the first-pass requirements and FLOPS mirrors from one cabin
/// authority. Count-mode seats are an installed-cabin declaration, so the
/// first evaluation cannot reject a declared 140-seat cabin against a
/// registered 130-seat seed. Percent-mode shares allocate the requested
/// passenger total unless counts are already materialized for that same
/// total; the detailed layout can then refine the result through
/// [`cabin_synchronized_for_cabin`].
pub fn declared_cabin(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    cabin: &PassengerCabinConfig,
) -> (DesignRequirements, MassModelConfig) {
    if requirements.aircraft_type != "passenger" {
        return (requirements.clone(), mass_model.clone());
    }
    let canonical = cabin.canonicalized_for_product();
    let counts = canonical.resolved_flops_counts(requirements.num_passengers);
    let synchronized_model = with_flops_counts(mass_model, counts);
    let mut synchronized_requirements = requirements.clone();
    if cabin.class_mix_mode == "count" && counts.is_nonempty() {
        synchronized_requirements.num_passengers = counts.total();
    }
    (synchronized_requirements, synchronized_model)
}

/// The FLOPS design gross mass a report at `takeoff_mass_kg` is sized on: a
/// registered aircraft keeps its declared design weight, a clean-sheet design
/// couples to the closed mass (`AlasConfig::at_closure_mass`).
pub(super) fn sized_design_gross_mass_kg(config: &AlasConfig, takeoff_mass_kg: f64) -> f64 {
    match config.mass_sizing_basis() {
        MassSizingBasis::FixedAircraft {
            design_gross_mass_kg,
            ..
        } => design_gross_mass_kg,
        MassSizingBasis::Coupled => takeoff_mass_kg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_mode_replaces_stale_seed_counts_before_the_first_mass_pass() {
        let requirements = DesignRequirements {
            num_passengers: 200,
            ..Default::default()
        };
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.share_pct = 10.0;
        cabin.business.share_pct = 20.0;
        cabin.economy.share_pct = 70.0;
        cabin.first.count = 2;
        cabin.business.count = 18;
        cabin.economy.count = 80;

        let (resolved_requirements, resolved_model) =
            declared_cabin(&requirements, &MassModelConfig::default(), &cabin);

        assert_eq!(resolved_requirements.num_passengers, 200);
        assert_eq!(
            resolved_model.flops_transport.first_class_passenger_count,
            Some(20)
        );
        assert_eq!(
            resolved_model
                .flops_transport
                .business_class_passenger_count,
            Some(40)
        );
        assert_eq!(
            resolved_model.flops_transport.tourist_class_passenger_count,
            Some(140)
        );
    }

    #[test]
    fn count_mode_preserves_the_installed_cabin_and_folds_premium_seats() {
        let requirements = DesignRequirements {
            num_passengers: 999,
            ..Default::default()
        };
        let mut cabin = PassengerCabinConfig {
            class_mix_mode: "count".to_owned(),
            ..Default::default()
        };
        cabin.first.count = 4;
        cabin.business.count = 16;
        cabin.premium.count = 10;
        cabin.economy.count = 70;

        let (resolved_requirements, resolved_model) =
            declared_cabin(&requirements, &MassModelConfig::default(), &cabin);

        assert_eq!(resolved_requirements.num_passengers, 100);
        assert_eq!(
            resolved_model.flops_transport.first_class_passenger_count,
            Some(4)
        );
        assert_eq!(
            resolved_model
                .flops_transport
                .business_class_passenger_count,
            Some(16)
        );
        assert_eq!(
            resolved_model.flops_transport.tourist_class_passenger_count,
            Some(80)
        );
    }
}
