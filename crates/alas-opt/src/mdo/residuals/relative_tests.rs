// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::mdo::ResidualRole;

use super::super::nominal_cache::config_key;
use super::relative_balance::{
    assess, nominal_balance, relative_residuals, NominalBalance, NOMINAL,
};
use super::*;

const PRESETS: [&str; 8] = [
    "A320-200",
    "A220-300",
    "A340-300",
    "A380-800",
    "DC-10",
    "B787-9",
    "AVE",
    "ATR72-600",
];

fn reference_config(name: &str) -> AlasConfig {
    // Mission-closed comparisons use the same mission condition on the
    // candidate and reference; the hard-MTOW loading is tested separately.
    AlasConfig::from_value(&serde_json::json!({
        "preset": name,
        "optimizer": {
            "design_space": {"mode": "reference_adaptation"},
            "objective": {"mtow_sizing": "sized_by_mission"}
        }
    }))
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn nominal_outcome(config: &AlasConfig) -> SizingOutcome {
    let design = alas_config::presets::get(&config.preset)
        .unwrap_or_else(|error| panic!("{}: {error}", config.preset))
        .design_vector;
    super::super::sizing::run_candidate(config, &design.to_array())
        .unwrap_or_else(|failure| panic!("{}: {}", config.preset, failure.reason))
}

fn find<'a>(residuals: &'a [ConstraintResidual], id: &str) -> &'a ConstraintResidual {
    residuals
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("missing residual {id}"))
}

#[test]
fn hard_mtow_nominals_keep_relative_guards_with_volume_limited_loading() {
    for name in PRESETS {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        assert_eq!(
            config.optimizer.objective.mtow_sizing,
            alas_config::MtowSizing::FixedRequirement
        );
        let outcome = nominal_outcome(&config);
        let rows = balance_residuals(&outcome, &config, ResidualRole::Constraint);
        assert!(
            nominal_balance(&config).is_some(),
            "{name}: no nominal loading"
        );
        assert!(!rows.iter().any(|row| row.id == "cg_model_error"), "{name}");
        if matches!(name, "A320-200" | "DC-10") {
            assert_eq!(
                outcome.sized.takeoff_loading.unwrap().status,
                alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited,
                "{name}"
            );
        }
        for id in ["usable_cg_range_vs_nominal", "tail_scrape_vs_nominal"] {
            let row = find(&rows, id);
            assert_eq!(row.role, ResidualRole::Constraint);
            assert_eq!(row.normalized_violation, 0.0, "{name}: {id}");
        }
    }
}

#[test]
fn every_preset_nominal_passes_both_relative_constraints() {
    for name in PRESETS {
        let config = reference_config(name);
        let outcome = nominal_outcome(&config);
        let residuals = balance_residuals(&outcome, &config, ResidualRole::Constraint);
        let nominal = nominal_balance(&config).unwrap_or_else(|| panic!("{name}: no nominal"));
        for (id, value) in [
            (
                "usable_cg_range_vs_nominal",
                nominal.usable_cg_range_pct_mac,
            ),
            ("tail_scrape_vs_nominal", nominal.tail_scrape_deg),
        ] {
            let r = find(&residuals, id);
            assert_eq!(r.role, ResidualRole::Constraint, "{name} {id}");
            assert_eq!(r.normalized_violation, 0.0, "{name} {id}: nominal fails");
            assert!(r.limit <= value + 1e-12, "{name} {id}: limit above nominal");
            assert!(
                (r.actual - value).abs() < 1e-9,
                "{name} {id}: actual {} differs from nominal {value}",
                r.actual
            );
        }
        for id in ["minimum_usable_cg_range", "tail_scrape"] {
            assert_eq!(find(&residuals, id).role, ResidualRole::Diagnostic);
        }
    }
}

#[test]
fn a_candidate_below_both_nominal_and_requirement_fails() {
    // The fixture needs a registered aircraft short of both requirements;
    // the A320's usable takeoff range now exceeds 30 %MAC (the derived
    // rotation authority), the A380-800's does not.
    let config = reference_config("A380-800");
    let outcome = nominal_outcome(&config);
    let assessment = assess(&outcome, &config).unwrap_or_else(|error| panic!("{error}"));
    let own = nominal_balance(&config).unwrap_or_else(|| panic!("no nominal"));
    let absolute = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    // A better registered aircraft that is itself still short of the requirement.
    let headroom = |id: &str, actual: f64| {
        let limit = find(&absolute, id).limit;
        assert!(actual < limit, "{id}: preset already meets {limit}");
        actual + 0.5 * (limit - actual)
    };
    let better = NominalBalance {
        usable_cg_range_pct_mac: headroom("minimum_usable_cg_range", own.usable_cg_range_pct_mac),
        tail_scrape_deg: headroom("tail_scrape", own.tail_scrape_deg),
    };
    let failing = relative_residuals(&assessment, better, ResidualRole::Constraint);
    for id in ["usable_cg_range_vs_nominal", "tail_scrape_vs_nominal"] {
        let r = find(&failing, id);
        assert!(r.raw_residual > 0.0 && r.normalized_violation > 0.0, "{id}");
    }
    // A nominal above the requirement is capped at the requirement.
    let above = NominalBalance {
        usable_cg_range_pct_mac: 1.0e3,
        tail_scrape_deg: 1.0e3,
    };
    let capped = relative_residuals(&assessment, above, ResidualRole::Constraint);
    assert_eq!(
        find(&capped, "usable_cg_range_vs_nominal").limit,
        find(&absolute, "minimum_usable_cg_range").limit
    );
    assert_eq!(
        find(&capped, "tail_scrape_vs_nominal").limit,
        find(&absolute, "tail_scrape").limit
    );
}

#[test]
fn clean_sheet_has_no_nominal_and_no_relative_residuals() {
    // The fixture keeps the legacy 3.5 m / -0.3 m A320 nose. With the measured
    // 4.78 m nose (v1.3.2) the clean-sheet nominal of the preset's own 37.57 m
    // design vector fails its item-level CG assessment: the mass ledger has a
    // systems residual of -22 kg (cg_model_error), a model sensitivity of the
    // clean-sheet mass path to the shorter compartment, recorded in the
    // release notes rather than pinned here.
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200", "geometry": {"fuselage": {"cabin_start_x_m": 3.5, "nose_z_m": -0.3}}, "optimizer": {"design_space": {"mode": "clean_sheet"}}}))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_ne!(
        config.optimizer.design_space.mode,
        alas_config::DesignMode::ReferenceAdaptation
    );
    assert!(nominal_balance(&config).is_none());
    let outcome = nominal_outcome(&config);
    let residuals = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    assert!(residuals.iter().all(|r| !r.id.ends_with("_vs_nominal")));
    assert!(residuals.iter().any(|r| r.id == "minimum_usable_cg_range"));
}

#[test]
fn cache_key_follows_preset_and_every_configuration_input() {
    let config = reference_config("A320-200");
    assert_eq!(config_key(&config), config_key(&config.clone()));
    let mut other = config.clone();
    other.mass_model.fuel_density_kg_m3 *= 1.01;
    assert_ne!(config_key(&config), config_key(&other));
    let mut other = config.clone();
    other.requirements.cruise_mach += 0.01;
    assert_ne!(config_key(&config), config_key(&other));
    assert_ne!(
        config_key(&config),
        config_key(&reference_config("A220-300"))
    );
}

#[test]
fn the_nominal_is_sized_once_across_many_candidate_evaluations() {
    let mut config = reference_config("A320-200");
    // A density no other test uses, so the key is private to this test.
    config.mass_model.fuel_density_kg_m3 = 801.7;
    let design = alas_config::presets::get("A320-200")
        .unwrap_or_else(|error| panic!("{error}"))
        .design_vector
        .to_array();
    for k in 0..5 {
        let mut x = design.clone();
        x[0] *= 1.0 + 0.004 * f64::from(k);
        let outcome = super::super::sizing::run_candidate(&config, &x)
            .unwrap_or_else(|failure| panic!("candidate {k}: {}", failure.reason));
        let residuals = balance_residuals(&outcome, &config, ResidualRole::Constraint);
        assert!(residuals
            .iter()
            .any(|r| r.id == "usable_cg_range_vs_nominal"));
    }
    assert_eq!(NOMINAL.resolutions(&config), 1);
}

/// Two configurations that differ only in the geometry scaffold (the tip
/// section, so the reference wing's thickness and tank volume) get their own
/// buffet and tank references, each resolved once however often it is read.
#[test]
fn buffet_and_tank_references_follow_the_complete_configuration() {
    let base = reference_config("A320-200");
    let mut thicker = base.clone();
    thicker.geometry.wing.tip_airfoil = "sc20610".to_owned();
    assert_ne!(
        base.geometry.wing.tip_airfoil,
        thicker.geometry.wing.tip_airfoil
    );
    let mut no_center = base.clone();
    no_center.fuel_tanks.center.enabled = false;
    let wing = super::super::residuals_buffet::reference_wing_basis;
    let tank = super::nominal_tank_capacity_kg;
    for config in [&base, &thicker, &no_center, &base, &thicker, &no_center] {
        assert!(wing(config).is_some() && tank(config).is_some());
    }
    let (thin_wing, thick_wing) = (wing(&base), wing(&thicker));
    assert!(
        thick_wing
            .zip(thin_wing)
            .is_some_and(|(thick, thin)| thick.1 > thin.1),
        "{thin_wing:?} vs {thick_wing:?}"
    );
    let (full, reduced) = (tank(&base), tank(&no_center));
    assert!(
        full.zip(reduced)
            .is_some_and(|(full, reduced)| reduced < full),
        "{full:?} vs {reduced:?}"
    );
    for config in [&base, &thicker, &no_center] {
        assert_eq!(
            super::super::residuals_buffet::REFERENCE_WING_BASIS.resolutions(config),
            1
        );
        assert_eq!(super::NOMINAL_TANK_CAPACITY_KG.resolutions(config), 1);
    }
}

#[test]
fn the_cache_key_costs_far_less_than_a_millisecond() {
    let config = reference_config("A320-200");
    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(config_key(std::hint::black_box(&config)));
    }
    let per_call = start.elapsed().as_secs_f64() / 100.0;
    assert!(per_call < 1.0e-3, "key took {per_call} s");
}

/// An unavailable nominal cannot silently bypass the balance requirement.
#[test]
fn an_unresolvable_nominal_is_a_hard_violation() {
    let good = reference_config("A320-200");
    let outcome = nominal_outcome(&good);
    let assessment = assess(&outcome, &good).unwrap_or_else(|error| panic!("{error}"));
    let mut broken = good.clone();
    broken.requirements.cruise_mach = f64::NAN;
    {
        let role = ResidualRole::Constraint;
        let residuals = super::relative_balance::residuals(&assessment, &broken, role);
        let flag = find(&residuals, "relative_balance_nominal_unavailable");
        assert_eq!(flag.role, role);
        assert!(flag.normalized_violation > 0.0);
        assert!(residuals.iter().all(|r| !r.id.ends_with("_vs_nominal")));
    }
}
