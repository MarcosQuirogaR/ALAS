// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plausibility tier: every registered preset in every user MTOW mode is run
//! through the product pipeline and judged by the rules of
//! `alas_acceptance::plausibility`.
//!
//! The cases are run once and shared by the tests of this file.
//!
//! - `plausibility_gate` is the statement that the product is plausible: it
//!   fails while any HIGH finding or any difference between the two mission
//!   models' common legs exists.
//! - `registered_findings_do_not_worsen` is the ratchet under it: the open
//!   findings are exactly those of `data/plausibility_known.tsv`, and none
//!   lies further past its rule's limit than the bound registered there.
//! - `no_rule_group_is_skipped` fails when a case could not be judged by a
//!   whole group of rules, which would otherwise read as a clean case.
//!
//! `ALAS_PLAUS_OUT=<file>` writes every finding as JSON lines.
//! `ALAS_PLAUS_THREADS=<n>` sets the number of cases run at once (default 4).
//! `ALAS_PLAUS_ONLY=<substring>` restricts the cases.

#![allow(clippy::expect_used, clippy::print_stdout)]

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use alas_acceptance::plausibility::case::{registered_cases, run_preset_case, CaseOutcome};
use alas_acceptance::plausibility::{Finding, Severity};

type CaseResults = Vec<(String, Result<CaseOutcome, String>)>;

fn run_all() -> CaseResults {
    let only = std::env::var("ALAS_PLAUS_ONLY").ok();
    let cases: Vec<_> = registered_cases()
        .into_iter()
        .filter(|(name, mode)| {
            only.as_deref()
                .is_none_or(|s| format!("{name}@{}", mode.as_str()).contains(s))
        })
        .collect();
    let threads = std::env::var("ALAS_PLAUS_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1);
    let queue = Mutex::new(cases.into_iter().collect::<std::collections::VecDeque<_>>());
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let next = queue.lock().expect("queue").pop_front();
                let Some((name, mode)) = next else { break };
                let outcome = run_preset_case(&name, mode);
                results
                    .lock()
                    .expect("results")
                    .push((format!("{name}@{}", mode.as_str()), outcome));
            });
        }
    });
    let mut results = results.into_inner().expect("results");
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// The tier's cases, run once per test binary. Every finding and every
/// skipped rule group is printed, and written to `ALAS_PLAUS_OUT` when set.
fn results() -> &'static CaseResults {
    static RESULTS: OnceLock<CaseResults> = OnceLock::new();
    RESULTS.get_or_init(|| {
        let results = run_all();
        let mut lines = Vec::new();
        for (label, outcome) in &results {
            match outcome {
                Ok(case) => {
                    for f in &case.findings {
                        lines.push(
                            serde_json::json!({
                                "case": f.case, "rule": f.rule, "severity": f.severity.label(),
                                "kind": f.kind, "magnitude": f.magnitude,
                                "observation": f.observation,
                            })
                            .to_string(),
                        );
                        println!("{f}");
                    }
                    for s in &case.skipped {
                        println!("SKIPPED {label}: {s}");
                    }
                }
                Err(e) => println!("ERROR {label}: {e}"),
            }
        }
        if let Ok(path) = std::env::var("ALAS_PLAUS_OUT") {
            std::fs::write(path, lines.join("\n")).expect("write findings");
        }
        results
    })
}

fn judged(results: &CaseResults) -> impl Iterator<Item = (&String, &CaseOutcome)> {
    results
        .iter()
        .filter_map(|(label, outcome)| outcome.as_ref().ok().map(|case| (label, case)))
}

fn is_leg_rule(rule: &str) -> bool {
    rule == alas_acceptance::plausibility::leg_rules::P_TAKEOFF.id
        || rule == alas_acceptance::plausibility::leg_rules::P_RESTRICTION.id
}

/// The registry key of a finding: its aircraft and `[PRICED:]<rule>[/<kind>]`.
/// The sizing mode is not part of the key: the bound holds in every mode.
fn key_of(f: &Finding) -> (String, String) {
    let aircraft = f.case.split('@').next().unwrap_or(&f.case).to_owned();
    // The priced profile is judged under `<case>#priced`.
    let priced = f
        .case
        .ends_with(alas_acceptance::plausibility::mission_priced::PRICED_SUFFIX);
    let mut rule = if priced {
        format!("PRICED:{}", f.rule)
    } else {
        f.rule.to_owned()
    };
    if !f.kind.is_empty() {
        rule.push('/');
        rule.push_str(&f.kind);
    }
    (aircraft, rule)
}

/// Every finding the gate counts (HIGH, or a common-leg difference of any
/// severity), by registry key, with the largest magnitude over the modes and
/// states it occurs in (`None` for a finding that is a plain yes or no).
fn open_findings(results: &CaseResults) -> BTreeMap<(String, String), Option<f64>> {
    let mut open: BTreeMap<(String, String), Option<f64>> = BTreeMap::new();
    for (_, case) in judged(results) {
        for f in case
            .findings
            .iter()
            .filter(|f| f.severity == Severity::High || is_leg_rule(f.rule))
        {
            let worst = open.entry(key_of(f)).or_insert(None);
            *worst = match (*worst, f.magnitude) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
    }
    open
}

/// The open findings registered in `data/plausibility_known.tsv`: key, and
/// the largest magnitude the finding may have (`None` where the file gives
/// `-`, for a finding that carries no magnitude).
fn registered_findings() -> BTreeMap<(String, String), Option<f64>> {
    include_str!("../data/plausibility_known.tsv")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut parts = l.split('\t');
            let aircraft = parts.next().expect("aircraft column").to_owned();
            let rule = parts
                .next()
                .unwrap_or_else(|| panic!("row without a rule column: {l}"))
                .to_owned();
            let bound = parts
                .next()
                .unwrap_or_else(|| panic!("row without a bound column: {l}"));
            let bound = (bound != "-").then(|| {
                bound
                    .parse::<f64>()
                    .unwrap_or_else(|e| panic!("bound of row `{l}`: {e}"))
            });
            ((aircraft, rule), bound)
        })
        .collect()
}

fn assert_all_cases_ran(results: &CaseResults) {
    let errors: Vec<_> = results
        .iter()
        .filter_map(|(label, r)| r.as_ref().err().map(|e| format!("{label}: {e}")))
        .collect();
    assert!(errors.is_empty(), "cases failed to run: {errors:?}");
}

/// The plausibility statement itself: every case runs, no finding is HIGH
/// ("physically impossible or clearly absurd"), and the take-off climb and the
/// speed changes at the 10,000 ft limit are the same legs in the two mission
/// models (entry and exit state, duration, distance and fuel within the
/// stated tolerances of the rules). A registered finding fails this test
/// until it is fixed; the registry only keeps it from growing
/// (`registered_findings_do_not_worsen`).
#[test]
fn plausibility_gate() {
    let results = results();
    assert_all_cases_ran(results);
    let open = open_findings(results);
    assert!(
        open.is_empty(),
        "{} HIGH or common-leg plausibility findings are open: {open:#?}",
        open.len()
    );
}

/// The ratchet: the open findings are exactly the registered ones, and each
/// lies no further past its rule's limit than its registered bound. A fix
/// removes its row; an improvement may lower its bound; nothing here can
/// raise one. The registry is edited by hand, with the evidence in its
/// comments: a run never rewrites it.
#[test]
fn registered_findings_do_not_worsen() {
    assert!(
        std::env::var("ALAS_PLAUS_WRITE_KNOWN").is_err(),
        "ALAS_PLAUS_WRITE_KNOWN is not supported: data/plausibility_known.tsv is edited by hand"
    );
    let results = results();
    assert_all_cases_ran(results);
    let open = open_findings(results);
    let registered = registered_findings();

    let new: Vec<_> = open
        .iter()
        .filter(|(key, _)| !registered.contains_key(*key))
        .collect();
    assert!(new.is_empty(), "new plausibility findings: {new:#?}");

    let worse: Vec<String> = open
        .iter()
        .filter_map(|(key, magnitude)| {
            let bound = registered.get(key)?;
            match (magnitude, bound) {
                (Some(magnitude), Some(bound)) if magnitude <= bound => None,
                (None, None) => None,
                (Some(magnitude), Some(bound)) => Some(format!(
                    "{} {}: {magnitude} is past the registered bound {bound}",
                    key.0, key.1
                )),
                (Some(magnitude), None) => Some(format!(
                    "{} {}: magnitude {magnitude} has no registered bound",
                    key.0, key.1
                )),
                (None, Some(bound)) => Some(format!(
                    "{} {}: bound {bound} registered for a finding that carries no magnitude",
                    key.0, key.1
                )),
            }
        })
        .collect();
    assert!(
        worse.is_empty(),
        "registered findings got worse: {worse:#?}"
    );

    if std::env::var("ALAS_PLAUS_ONLY").is_err() {
        let stale: Vec<_> = registered
            .keys()
            .filter(|key| !open.contains_key(*key))
            .collect();
        assert!(
            stale.is_empty(),
            "registered findings no longer reproduced (remove their rows from \
             data/plausibility_known.tsv): {stale:?}"
        );
    }
}

/// A case that a whole group of rules could not judge (no mission telemetry,
/// no CG assessment, no structural analysis, ...) has fewer findings than one
/// that was judged badly, so it must not pass for a clean one.
#[test]
fn no_rule_group_is_skipped() {
    let results = results();
    assert_all_cases_ran(results);
    let skipped: Vec<String> = judged(results)
        .flat_map(|(label, case)| case.skipped.iter().map(move |s| format!("{label}: {s}")))
        .collect();
    assert!(
        skipped.is_empty(),
        "rule groups were not judged: {skipped:#?}"
    );
}

/// ENGINE-OVERSIZE fires in the tier: the E195-E2 brief held on
/// its catalogue engine needs about 0.85 of the PW1900G rating, so a 2 percent
/// rating window flags it MEDIUM with the numbers; the default 20 percent
/// window does not. The run is the product pipeline, and the rule reads the
/// pipeline's own warning finding, the one the report and the GUI show.
/// The A320's brief needs 1.028 of the CFM56, so its held engine is not oversize.
#[test]
fn engine_oversize_fires_on_a_held_clean_sheet_engine() {
    use alas_acceptance::plausibility::case::{case_config, judge, run_pipeline};
    use alas_config::MtowSizing;
    use alas_pipeline::FindingCode;

    let case = |window: f64| {
        let brief = serde_json::json!({"optimizer": {"design_space": {
            "mode": "clean_sheet",
            "fuselage_sized_by_cabin": false,
            "engine_sized_by_requirements": false,
            "engine_rating_window": window,
        }}});
        let config = case_config("E195-E2", Some(&brief), MtowSizing::FixedRequirement)
            .expect("held E195-E2 configuration");
        let result = run_pipeline(&config).expect("pipeline run");
        let outcome = judge("E195-E2-held@fixed_requirement", &result);
        (result, outcome)
    };

    let (result, tight) = case(0.02);
    let finding = result
        .feasibility
        .findings
        .iter()
        .find(|f| f.code == FindingCode::EngineOversize)
        .expect("the pipeline warns about the oversize held engine");
    assert!(finding.actual > finding.limit, "{finding:?}");
    let hit = tight
        .findings
        .iter()
        .find(|f| f.rule == "ENGINE-OVERSIZE")
        .expect("the tier rule fires");
    assert_eq!(hit.severity, Severity::Medium);
    assert!(
        hit.observation.contains("held engine") && hit.observation.contains("kN"),
        "{}",
        hit.observation
    );
    println!("{hit}");

    let (result, inside) = case(0.20);
    assert!(result
        .feasibility
        .findings
        .iter()
        .all(|f| f.code != FindingCode::EngineOversize));
    assert!(inside.findings.iter().all(|f| f.rule != "ENGINE-OVERSIZE"));
}

/// A declared cruise level the aircraft cannot climb to at the dispatched mass is flown lower and
/// the user is told: the pipeline emits the warning finding
/// `declared_cruise_level_lowered`. The default configuration declares FL390 for a 6,500 km
/// mission it cannot climb to at its dispatched mass (a probe, `declared_level_probe`); the
/// same configuration under the optimum-step policy flies a level it can hold and emits nothing.
#[test]
fn a_declared_level_the_aircraft_cannot_reach_is_reported_to_the_user() {
    use alas_acceptance::plausibility::case::run_pipeline;
    use alas_config::mission::CruiseAltitudePolicy;
    use alas_config::AlasConfig;
    use alas_pipeline::FindingCode;

    let run = |policy: CruiseAltitudePolicy| {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_altitude_policy = policy;
        run_pipeline(&config).expect("pipeline run")
    };
    let declared = run(CruiseAltitudePolicy::Declared);
    let finding = declared
        .feasibility
        .findings
        .iter()
        .find(|f| f.code == FindingCode::DeclaredCruiseLevelLowered)
        .expect("the pipeline reports the lowered level");
    assert!(finding.limit.unwrap_or(0.0) > finding.actual.unwrap_or(f64::MAX));
    assert!(finding.message.contains("declared cruise level"));
    // The report text lists it (the GUI lists every finding message).
    assert!(alas_pipeline::format_feasibility(&declared.feasibility).contains(&finding.message));
    println!("{}", finding.message);
    let optimum = run(CruiseAltitudePolicy::OptimumStep);
    assert!(optimum
        .feasibility
        .findings
        .iter()
        .all(|f| f.code != FindingCode::DeclaredCruiseLevelLowered));
}
