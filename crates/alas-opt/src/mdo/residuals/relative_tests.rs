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
        if name == "A320-200" {
            assert_eq!(
                outcome.sized.takeoff_loading.unwrap().status,
                alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited,
                "{name}"
            );
        }
        // DC-10: it seats its registered 255 passengers and its zero-fuel
        // mass exceeds MTOW less the usable capacity by 770.8 kg, so it is mass-limited (see
        // `balance_ledger::tests::hard_mtow_preset_ledgers_conserve_mass_and_report_volume_limits`).
        if name == "DC-10" {
            assert_eq!(
                outcome.sized.takeoff_loading.unwrap().status,
                alas_mass::loading::MtowFuelLoadingStatus::MassLimited,
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
    // rotation authority), and the A380-800's tail-scrape clearance (11.23 deg with the
    // aft loft) now exceeds the required 10 deg. The AVE is short of both:
    // 6.94 deg of scrape clearance and a usable range under 30 %MAC.
    let config = reference_config("AVE");
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
        mac_m: own.mac_m,
    };
    // The travel is held in metres: a registered aircraft with a chord a
    // tenth longer asks a tenth more of this chord, and the tail scrape, an
    // angle, is unchanged.
    let longer = NominalBalance {
        mac_m: 1.1 * own.mac_m,
        ..own
    };
    let scaled = relative_residuals(&assessment, longer, ResidualRole::Constraint);
    let unscaled = relative_residuals(&assessment, own, ResidualRole::Constraint);
    assert!(
        (find(&scaled, "usable_cg_range_vs_nominal").limit
            / find(&unscaled, "usable_cg_range_vs_nominal").limit
            - 1.1)
            .abs()
            < 1e-12
    );
    assert_eq!(
        find(&scaled, "tail_scrape_vs_nominal").limit,
        find(&unscaled, "tail_scrape_vs_nominal").limit
    );
    let failing = relative_residuals(&assessment, better, ResidualRole::Constraint);
    for id in ["usable_cg_range_vs_nominal", "tail_scrape_vs_nominal"] {
        let r = find(&failing, id);
        assert!(r.raw_residual > 0.0 && r.normalized_violation > 0.0, "{id}");
    }
    // A nominal above the requirement is capped at the requirement.
    let above = NominalBalance {
        usable_cg_range_pct_mac: 1.0e3,
        tail_scrape_deg: 1.0e3,
        mac_m: own.mac_m,
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

/// A clean sheet from a registered preset is
/// held to the registered aircraft on the rows the search cannot move, on its
/// own mission and on a new one: relative guards hard, absolute tail scrape
/// and usable CG range diagnostic.
#[test]
fn a_clean_sheet_is_held_to_the_registered_aircraft_on_any_mission() {
    let own = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200", "optimizer": {"design_space": {"mode": "clean_sheet"}}}))
        .unwrap_or_else(|error| panic!("{error}"));
    let mut new_mission = own.clone();
    new_mission.optimizer.objective.design_range_nmi = 3000.0;
    assert!(own.flies_registered_mission() && !new_mission.flies_registered_mission());
    let outcome = nominal_outcome(&own);
    for config in [&own, &new_mission] {
        assert!(nominal_balance(config).is_some());
        let rows = balance_residuals(&outcome, config, ResidualRole::Constraint);
        for id in ["usable_cg_range_vs_nominal", "tail_scrape_vs_nominal"] {
            let row = rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(row.role, ResidualRole::Constraint, "{id}");
        }
        for id in ["minimum_usable_cg_range", "tail_scrape"] {
            let row = rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(row.role, ResidualRole::Diagnostic, "{id}");
        }
    }
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
/// section, so the tank volume) get their own tank reference, each resolved
/// once however often it is read.
#[test]
fn tank_references_follow_the_complete_configuration() {
    let base = reference_config("A320-200");
    let mut thicker = base.clone();
    thicker.geometry.wing.tip_airfoil = "sc20610".to_owned();
    assert_ne!(
        base.geometry.wing.tip_airfoil,
        thicker.geometry.wing.tip_airfoil
    );
    let mut no_center = base.clone();
    no_center.fuel_tanks.center.enabled = false;
    let tank = super::nominal_tank_capacity_kg;
    for config in [&base, &thicker, &no_center, &base, &thicker, &no_center] {
        assert!(tank(config).is_some());
    }
    let (full, reduced) = (tank(&base), tank(&no_center));
    assert!(
        full.zip(reduced)
            .is_some_and(|(full, reduced)| reduced < full),
        "{full:?} vs {reduced:?}"
    );
    for config in [&base, &thicker, &no_center] {
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

/// Lever A12: the nominals are the registered aircraft's, so candidates with
/// different geometry-scaled CLmax share the one nominal resolved under the
/// declared CLmax, and the relative residuals are unchanged by the scaling.
#[test]
fn candidates_with_scaled_clmax_share_the_registered_nominal() {
    use alas_perf::performance::clmax::ClmaxSet;
    let config = reference_config("A320-200");
    let declared = super::super::sizing::clmax_geometry::declared_set(&config);
    let mut outcome = nominal_outcome(&config);
    outcome.declared_clmax = Some(declared);
    let mut rows = Vec::new();
    for scale in [0.9, 1.05, 1.1, 1.2] {
        let set = ClmaxSet {
            clean: declared.clean * scale,
            takeoff: declared.takeoff * scale,
            landing: declared.landing * scale,
        };
        outcome.clmax = Some(set);
        let scaled = outcome.config_with_clmax(&config);
        assert_ne!(config_key(&scaled), config_key(&config), "scale {scale}");
        let balance = balance_residuals(&outcome, &scaled, ResidualRole::Constraint);
        rows.push(find(&balance, "usable_cg_range_vs_nominal").limit);
    }
    assert!(rows
        .windows(2)
        .all(|pair| pair[0].to_bits() == pair[1].to_bits()));
    assert_eq!(NOMINAL.resolutions(&config), 1);
}

/// The A400M design mission needs more fuel than its tanks hold in the
/// mission-closed modes; the registered aircraft is loaded tank-limited, so
/// its balance reference still resolves (an unavailable reference makes every
/// candidate violate `relative_balance_nominal_unavailable`).
#[test]
fn a_registered_aircraft_whose_mission_overflows_its_tanks_still_has_a_nominal() {
    {
        let mode = "payload_adjusted";
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A400M",
            "optimizer": {
                "design_space": {"mode": "reference_adaptation"},
                "objective": {"mtow_sizing": mode}
            }
        }))
        .unwrap();
        let nominal =
            nominal_balance(&config).unwrap_or_else(|| panic!("A400M {mode}: no nominal"));
        assert!(nominal.usable_cg_range_pct_mac.is_finite() && nominal.tail_scrape_deg > 0.0);
        // The candidate path keeps the mission load: the overflow is an error
        // there (the capacity residuals reject it), not silently clamped.
        let design = alas_config::presets::get("A400M").unwrap().design_vector;
        let outcome = super::super::sizing::run_candidate(&config, &design.to_array()).unwrap();
        let error = assess(&outcome, &config).expect_err("mission fuel exceeds the tanks");
        assert!(error.contains("exceeds usable capacity"), "{error}");
    }
}

/// The A320 search under `payload_adjusted` once rejected 30 of its 32
/// candidates as `relative_balance_nominal_unavailable`: the registered
/// aircraft's nominal must resolve under the configuration a search runs in
/// every MTOW mode, and the reason must be stated when it does not.
#[test]
fn the_nominal_resolves_under_a_search_configuration_in_every_mtow_mode() {
    for mode in ["fixed_requirement", "mtow_band", "payload_adjusted"] {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "mses": {"enabled": false},
            "structures": {"run_nastran": false, "run_patran_export": false},
            "optimizer": {
                "solver": {
                    "workers": 1,
                    "screening": {"max_evaluations": 24},
                    "refinement": {"max_evaluations": 36}
                },
                "objective": {"mtow_sizing": mode}
            }
        }))
        .unwrap_or_else(|error| panic!("{mode}: {error}"));
        assert_eq!(
            config.optimizer.design_space.mode,
            alas_config::DesignMode::ReferenceAdaptation,
            "{mode}"
        );
        if let Err(reason) = super::relative_balance::resolve(&config) {
            panic!("A320-200 {mode}: nominal unavailable: {reason}");
        }
    }
    let config = reference_config("A320-200");
    let outcome = nominal_outcome(&config);
    let assessment = assess(&outcome, &config).unwrap_or_else(|error| panic!("{error}"));
    let rows = super::relative_balance::residuals_against(
        &assessment,
        Err("sizing did not close"),
        ResidualRole::Constraint,
    );
    let row = find(&rows, "relative_balance_nominal_unavailable");
    assert!(row.violated());
    assert!(
        row.detail
            .as_deref()
            .is_some_and(|d| d.ends_with("sizing did not close")),
        "{:?}",
        row.detail
    );
}

/// A closure that stops unsettled holds its last pass's state, not the
/// dispatch mass it reports; the balance ledger re-evaluates the state's
/// grouped decomposition at the state's own mass. Read at a heavier dispatch,
/// the systems items outgrew their slot and the ledger refused the statement
/// (the A320 payload-adjusted nominal: -0.35 kg).
#[test]
fn the_ledger_reads_the_mass_its_state_was_evaluated_at() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"objective": {"mtow_sizing": "payload_adjusted"}}
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    let mut outcome = nominal_outcome(&config);
    let at = outcome
        .evaluated_at
        .unwrap_or_else(|| panic!("a mission-sized closure records its state's mass"));
    // As a closure stopped 400 kg short of its dispatch reports it.
    outcome.sized.takeoff_mass_kg = at.takeoff_mass_kg + 400.0;
    super::balance_ledger::loading_bases_for(
        &outcome,
        &config,
        super::balance_ledger::FuelLoad::TankLimited,
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

/// The balance rows judge the loading the report's load plan places, not the
/// nominal placement: the registered C295, whose report passes every hard CG
/// constraint after a forward plan, is not rejected on tip-back by the search.
#[test]
fn the_search_judges_the_planned_loading_of_the_registered_c295() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "C295",
        "optimizer": {"design_space": {"mode": "baseline_sandbox"}}
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    let outcome = nominal_outcome(&config);
    let (ledger, flown) = super::balance_ledger::loading_bases(&outcome, &config)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(flown.is_some(), "the C295 route is dispatched");
    // The nominal placement alone leaves the design take-off aft of the
    // tip-back limit; the rows must not report that loading.
    let _ = ledger;
    let rows = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    let tip_back = find(&rows, "tip_back");
    assert!(
        !tip_back.violated(),
        "tip-back {} deg against {} deg",
        tip_back.actual,
        tip_back.limit
    );
    assert!(rows
        .iter()
        .all(|row| row.role != ResidualRole::Constraint || !row.violated()));
}

/// The plan reads both loadings the rows gate. The LJ60 route's states sit
/// inside their windows at the nominal placement while its maximum-fuel
/// design take-off is aft of the tip-back limit; the report plans that
/// loading and passes, and so does the search.
#[test]
fn the_plan_serves_the_design_loading_of_the_registered_lj60() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "LJ60",
        "optimizer": {"design_space": {"mode": "baseline_sandbox"}}
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    let outcome = nominal_outcome(&config);
    let rows = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    let tip_back = find(&rows, "tip_back");
    assert!(
        !tip_back.violated(),
        "tip-back {} deg against {} deg",
        tip_back.actual,
        tip_back.limit
    );
}
