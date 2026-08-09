// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the cross-field validation rules against the reference
//! implementation's.
//!
//! All three parts of an issue are compared, because all three are load
//! bearing and they fail independently. The path is what the interface scrolls
//! to, so a rule that fires correctly and names the wrong field sends the user
//! to edit something that was never the problem. The severity decides whether
//! the run is blocked at all, so a rule that quietly downgraded itself would
//! let a design that cruises outside its own structural envelope through to
//! the solver. And the message is the only place the two disagreeing values
//! appear, so a rule with the right verdict and a wrong number is a rule
//! nobody can act on.
//!
//! Compared at `exact`, including the rendered numbers inside the messages.
//! That is the strongest available check on the arithmetic behind them: the
//! rule computes a cruise equivalent airspeed and prints it, and nothing else
//! it produces would show the value being wrong.
//!
//! The cases are stated as saved files rather than as constructed objects, so
//! each one is built here through the same loading path `parity_settings.rs`
//! covers. A case whose configuration was built differently on the two sides
//! would be comparing the builders and not the rules.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{validate, AlasConfig, Severity, ValidationIssue};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    name: String,
    input: Value,
    issues: Vec<ValidationIssue>,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "validation")
}

#[test]
fn every_configuration_produces_the_issues_the_reference_reports() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::validation", Tier::Exact);

    for case in &fixture.cases {
        let Ok(config) = AlasConfig::from_value(&case.input) else {
            comparison.exact(&format!("{}: loads", case.name), &false, &true);
            continue;
        };

        let issues = validate(&config);
        comparison.exact(
            &format!("{}: issue count", case.name),
            &issues.len(),
            &case.issues.len(),
        );
        if issues.len() != case.issues.len() {
            continue;
        }

        // Compared pairwise in order: the order the rules are registered in
        // is the order the interface lists the problems in, and a rule that
        // ran out of turn would be a different program even where the set of
        // issues happened to match.
        for (index, (issue, expected)) in issues.iter().zip(&case.issues).enumerate() {
            let path = format!("{}[{index}]", case.name);
            comparison.exact(
                &format!("{path}.field_path"),
                &issue.field_path,
                &expected.field_path,
            );
            comparison.exact(
                &format!("{path}.severity"),
                &issue.severity,
                &expected.severity,
            );
            comparison.exact(
                &format!("{path}.message"),
                &issue.message,
                &expected.message,
            );
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_exercises_both_severities_and_both_rules() {
    // A fixture where every case passed would agree with any implementation
    // at all, including one whose rules never fire.
    let fixture = fixture();
    let issues: Vec<&ValidationIssue> = fixture
        .cases
        .iter()
        .flat_map(|case| case.issues.iter())
        .collect();

    assert!(issues.iter().any(|issue| issue.severity == Severity::Error));
    assert!(issues
        .iter()
        .any(|issue| issue.severity == Severity::Warning));
    assert!(issues
        .iter()
        .any(|issue| issue.field_path.starts_with("requirements.")));
    assert!(issues
        .iter()
        .any(|issue| issue.field_path.starts_with("geometry.empennage.")));
    assert!(fixture.cases.iter().any(|case| case.issues.is_empty()));
}
