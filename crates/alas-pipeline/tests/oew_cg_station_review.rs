// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset-level checks of the empty-aircraft component stations.
//!
//! Frame: x in metres aft of the model nose; percent MAC from the model
//! LEMAC. Every registered aircraft is analyzed once in the baseline sandbox
//! at its own design vector, and the item ledger of the pipeline's mass
//! statement is read back.
//!
//! Two properties are asserted:
//!
//! - The systems-and-equipment group of every turbofan preset has its centre
//!   of gravity inside 40 % to 50 % of the fuselage length, the band Scholz
//!   gives after Marckwardt with Torenbeek's data (M. F. Nita, *Aircraft
//!   Design Studies Based on the ATR 72*, project report, HAW Hamburg,
//!   examiner D. Scholz, 13 June 2008, section 8.3 and Fig. 8.2, p. 108).
//!   This is a secondary source. The wing-mounted-propeller band (38 % to
//!   40 %) is not asserted: the ATR 72-600 sits at 37.0 %, just forward of
//!   it, and that residual is reported rather than tuned away.
//! - The operating-empty centre of gravity clears the minimum nose-gear load
//!   and the tip-back angle on every aircraft except the ATR 72-600, whose
//!   empty aircraft is a known ground-limit failure of this model (no
//!   sourced station rule moves it inside). The ATR is asserted to fail so
//!   that fixing it is a visible change to this file, not a silent one.

// A test asserts on registered presets it loads, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode, PropulsionTechnology};
use alas_opt::{ModelCgConstraint, ModelCgLoadingState};
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

/// The registered aircraft whose empty aircraft fails its ground limits.
const KNOWN_OEW_GROUND_LIMIT_FAILURE: &str = "ATR72-600";

#[test]
fn empty_aircraft_stations_respect_the_systems_band_and_the_ground_limits() {
    for name in presets::available() {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        config.mission.enabled = false;
        config.mses.enabled = false;
        config.structures.enabled = false;
        let design = presets::get(name).unwrap().design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let physical = assess_physical_feasibility(&config, &design, &report, None);
        let statement = physical
            .mass_balance
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: mass statement"));

        let (systems_kg, systems_moment) = statement
            .ledger_items
            .iter()
            .filter(|item| item.group == "systems")
            .fold((0.0, 0.0), |(mass, moment), item| {
                (
                    mass + item.mass_kg,
                    moment + item.mass_kg * item.position_m[0],
                )
            });
        assert!(systems_kg > 0.0, "{name}: no systems rows");
        let systems_fraction = systems_moment / systems_kg / design.fuselage_length_m;
        let turboprop = config.geometry.engine.propulsion_technology
            == PropulsionTechnology::Turboprop
            || config.geometry.engine.turboprop.is_some();
        if !turboprop {
            assert!(
                (0.40..=0.50).contains(&systems_fraction),
                "{name}: systems group CG at {systems_fraction:.3} of the fuselage length"
            );
        }

        let envelope = physical
            .model_cg
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: model CG envelope"));
        let empty = envelope
            .loading_states
            .iter()
            .find(|state| state.state == ModelCgLoadingState::OperatingEmpty)
            .unwrap_or_else(|| panic!("{name}: OEW state"));
        let violated = |kind: ModelCgConstraint| {
            empty
                .constraints
                .iter()
                .find(|constraint| constraint.constraint == kind)
                .unwrap_or_else(|| panic!("{name}: {kind:?} not assessed"))
                .violated
        };
        let fails = violated(ModelCgConstraint::MinimumNoseGearLoad)
            || violated(ModelCgConstraint::TipBack);
        assert_eq!(
            fails,
            name == KNOWN_OEW_GROUND_LIMIT_FAILURE,
            "{name}: OEW CG {:.2} %MAC, nose-load fraction {:.3}",
            empty.cg_pct_mac,
            empty.nose_gear_load_fraction
        );
    }
}
