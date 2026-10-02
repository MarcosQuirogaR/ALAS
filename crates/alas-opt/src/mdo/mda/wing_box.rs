// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The primary wing box a mass pass may share with the next one.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

/// The primary box the latest mass pass sized, with the inputs it was sized
/// from: the pass configuration and the main wing. The design vector is the
/// context's and fixed for the whole loop.
///
/// A design mode that keeps its declared structural design masses (a
/// registered aircraft in reference adaptation or the baseline sandbox)
/// presents every pass with the same structural inputs, so re-sizing an
/// identical box each pass is pure repetition. Reuse requires the complete
/// configuration and the main wing to compare equal, which is stronger than
/// the inputs the sizing reads; anything else, including any non-finite
/// field, sizes afresh.
#[derive(Default)]
pub(crate) struct PassWingBox(
    std::cell::RefCell<
        Option<(
            AlasConfig,
            Wing,
            alas_mass::wing_reconciliation::DesignWingBox,
        )>,
    >,
);

impl PassWingBox {
    /// The stored box when `config` and `plane`'s main wing are the inputs
    /// it was sized from.
    pub(super) fn reusable(
        &self,
        config: &AlasConfig,
        plane: &Airplane,
    ) -> Option<alas_mass::wing_reconciliation::DesignWingBox> {
        let wing = alas_mass::wing_reconciliation::main_wing(plane)?;
        self.0
            .borrow()
            .as_ref()
            .filter(|(stored_config, stored_wing, _)| {
                stored_config == config && stored_wing == wing
            })
            .map(|(_, _, sized)| *sized)
    }

    /// Record the box a pass sized from `config` and `plane`.
    pub(super) fn store(
        &self,
        config: &AlasConfig,
        plane: &Airplane,
        sized: alas_mass::wing_reconciliation::DesignWingBox,
    ) {
        *self.0.borrow_mut() = alas_mass::wing_reconciliation::main_wing(plane)
            .map(|wing| (config.clone(), wing.clone(), sized));
    }
}

#[cfg(test)]
mod tests {
    use super::PassWingBox;

    #[test]
    fn a_pass_box_is_reused_only_for_identical_structural_inputs() {
        use alas_config::{AlasConfig, DesignVector};
        use alas_geom::builder::AircraftBuilder;

        let config = AlasConfig::default();
        let design = DesignVector::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap_or_else(|error| panic!("{error}"));
        let sized = alas_mass::wing_reconciliation::size_design_wing_box(&config, &design, &plane)
            .unwrap_or_else(|error| panic!("{error}"));
        let cache = PassWingBox::default();
        assert!(cache.reusable(&config, &plane).is_none());
        cache.store(&config, &plane, sized);
        assert_eq!(cache.reusable(&config, &plane), Some(sized));

        // Any configuration change, structural or not, sizes afresh.
        let mut heavier = config.clone();
        heavier.requirements.mtow_kg *= 1.01;
        assert!(cache.reusable(&heavier, &plane).is_none());
        let wider = DesignVector {
            span_m: design.span_m * 1.01,
            ..design
        };
        let other_wing = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&wider), true)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(cache.reusable(&config, &other_wing).is_none());
    }
}
