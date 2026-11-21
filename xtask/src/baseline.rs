// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Expected-failure baseline: compares the outcome of a test run with the
//! tracked list of tests that are known to fail.
//!
//! A failing test that is not in the list is a regression; a listed test that
//! passes is a stale entry. Either makes the command fail, so the list can
//! only shrink as tests are fixed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Baseline location, relative to the repository root.
const BASELINE_PATH: &str = ".config/expected-failures.tsv";

/// `(crate or binary id, test name)`.
type TestKey = (String, String);

#[derive(Debug, Default, PartialEq)]
struct Outcomes {
    failed: BTreeSet<TestKey>,
    passed: BTreeSet<TestKey>,
}

/// Parse the baseline: `crate<TAB>test<TAB>reason`, `#` comments, and a
/// header row whose first columns are `crate` and `test`.
fn parse_baseline(text: &str) -> Result<BTreeMap<TestKey, String>, String> {
    let mut entries = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut columns = line.splitn(3, '\t');
        let (Some(krate), Some(test)) = (columns.next(), columns.next()) else {
            return Err(format!(
                "{BASELINE_PATH}:{}: expected crate<TAB>test<TAB>reason",
                index + 1
            ));
        };
        if krate == "crate" && test == "test" {
            continue;
        }
        let reason = columns.next().unwrap_or("").trim();
        if reason.is_empty() {
            return Err(format!(
                "{BASELINE_PATH}:{}: the reason is empty",
                index + 1
            ));
        }
        let key = (krate.trim().to_owned(), test.trim().to_owned());
        if entries.insert(key.clone(), reason.to_owned()).is_some() {
            return Err(format!(
                "{BASELINE_PATH}:{}: duplicate entry {} {}",
                index + 1,
                key.0,
                key.1
            ));
        }
    }
    Ok(entries)
}

/// Parse a result list. Accepted line shapes:
/// - nextest: `FAIL [ 0.1s] <binary-id> <test>` or `PASS [ ...] ...`
///   (retry attempts, `FLAKY` and summary prose are ignored);
/// - tab separated: `<ok|fail><TAB><crate><TAB><test>`;
/// - libtest: `test <name> ... ok|FAILED`, attributed to `default_crate`.
fn parse_results(text: &str, default_crate: Option<&str>) -> Outcomes {
    let mut outcomes = Outcomes::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("test ") {
            if let (Some(krate), Some((name, verdict))) = (default_crate, rest.rsplit_once(" ... "))
            {
                let key = (krate.to_owned(), name.trim().to_owned());
                match verdict.trim() {
                    "ok" => {
                        outcomes.passed.insert(key);
                    }
                    "FAILED" => {
                        outcomes.failed.insert(key);
                    }
                    _ => {}
                }
            }
            continue;
        }
        let columns: Vec<&str> = trimmed.split('\t').collect();
        if let [status, krate, test] = columns[..] {
            let key = (krate.trim().to_owned(), test.trim().to_owned());
            match status.trim().to_ascii_lowercase().as_str() {
                "ok" | "pass" => {
                    outcomes.passed.insert(key);
                }
                "fail" | "failed" => {
                    outcomes.failed.insert(key);
                }
                _ => {}
            }
            continue;
        }
        let Some(status) = trimmed.split_whitespace().next() else {
            continue;
        };
        let failing = matches!(
            status,
            "FAIL" | "SIGABRT" | "SIGSEGV" | "SIGKILL" | "TIMEOUT" | "LEAK-FAIL" | "EXEC-FAIL"
        );
        if !failing && !matches!(status, "PASS" | "LEAK") {
            continue;
        }
        // Skip the bracketed duration, which may contain spaces.
        let Some(after) = trimmed[status.len()..].trim_start().strip_prefix('[') else {
            continue;
        };
        let Some((_, tail)) = after.split_once(']') else {
            continue;
        };
        // Recent nextest versions print a progress counter after the duration,
        // `( 489/6865)`, padded inside the parentheses: skip it.
        let tail = tail.trim_start();
        let tail = match tail.strip_prefix('(').and_then(|rest| rest.split_once(')')) {
            Some((counter, rest))
                if counter.trim().split_once('/').is_some_and(|(done, total)| {
                    !done.is_empty()
                        && !total.is_empty()
                        && done.chars().all(|c| c.is_ascii_digit())
                        && total.chars().all(|c| c.is_ascii_digit())
                }) =>
            {
                rest
            }
            _ => tail,
        };
        let mut names = tail.split_whitespace();
        if let (Some(krate), Some(test)) = (names.next(), names.next()) {
            let key = (krate.to_owned(), test.to_owned());
            if failing {
                outcomes.failed.insert(key);
            } else {
                outcomes.passed.insert(key);
            }
        }
    }
    // A test that failed at least once in the list counts as failed.
    let failed = outcomes.failed.clone();
    outcomes.passed.retain(|key| !failed.contains(key));
    outcomes
}

/// Findings for `outcomes` against `baseline`, plus the number of baseline
/// tests the run did not report (not an error: the run may be partial).
fn compare(baseline: &BTreeMap<TestKey, String>, outcomes: &Outcomes) -> (Vec<String>, usize) {
    let mut findings = Vec::new();
    for key in &outcomes.failed {
        if !baseline.contains_key(key) {
            findings.push(format!("unexpected failure: {} {}", key.0, key.1));
        }
    }
    for (key, reason) in baseline {
        if outcomes.passed.contains(key) {
            findings.push(format!(
                "listed test now passes, remove it from {BASELINE_PATH}: {} {} ({reason})",
                key.0, key.1
            ));
        }
    }
    let unreported = baseline
        .keys()
        .filter(|key| !outcomes.failed.contains(*key) && !outcomes.passed.contains(*key))
        .count();
    (findings, unreported)
}

/// `cargo xtask expected-failures <results-file> [--crate NAME] [--baseline FILE]`
pub(crate) fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let mut results_file = None;
    let mut default_crate = None;
    let mut baseline_file = root.join(BASELINE_PATH);
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--crate" => {
                default_crate = Some(args.get(index + 1).ok_or("--crate needs a name")?.clone());
                index += 2;
            }
            "--baseline" => {
                baseline_file = args.get(index + 1).ok_or("--baseline needs a file")?.into();
                index += 2;
            }
            other if !other.starts_with("--") && results_file.is_none() => {
                results_file = Some(other.to_owned());
                index += 1;
            }
            other => return Err(format!("expected-failures: unknown argument {other}")),
        }
    }
    let results_file = results_file.ok_or("expected-failures needs a results file")?;
    let baseline_text = fs::read_to_string(&baseline_file)
        .map_err(|e| format!("cannot read {}: {e}", baseline_file.display()))?;
    let baseline = parse_baseline(&baseline_text)?;
    let bytes = fs::read(&results_file).map_err(|e| format!("{results_file}: {e}"))?;
    let outcomes = parse_results(&String::from_utf8_lossy(&bytes), default_crate.as_deref());

    if outcomes.failed.is_empty() && outcomes.passed.is_empty() {
        return Err(format!(
            "expected-failures: no test results recognised in {results_file}"
        ));
    }
    let (findings, unreported) = compare(&baseline, &outcomes);
    println!(
        "expected-failures: {} failed, {} passed, {} listed ({unreported} not reported by this run)",
        outcomes.failed.len(),
        outcomes.passed.len(),
        baseline.len()
    );
    if findings.is_empty() {
        println!("expected-failures: pass");
        return Ok(());
    }
    for finding in &findings {
        println!("{finding}");
    }
    Err(format!("expected-failures: {} finding(s)", findings.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESULTS: &str = "\
        PASS [   0.010s] alas-a alpha::ok_test
        FAIL [   0.020s] alas-a alpha::listed_red
        FAIL [   0.030s] alas-b new_red
 TRY 1 FAIL [   0.030s] alas-b flaky_one
       FLAKY 2/3 [   0.030s] alas-b flaky_one
        PASS [   0.010s] alas-b listed_green
";

    #[test]
    fn regressions_and_stale_entries_are_reported() {
        let baseline = parse_baseline(
            "# c\ncrate\ttest\treason\nalas-a\talpha::listed_red\tpin\nalas-b\tlisted_green\tpin\n",
        )
        .unwrap();
        let outcomes = parse_results(RESULTS, None);
        let (findings, unreported) = compare(&baseline, &outcomes);
        assert_eq!(unreported, 0);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].contains("unexpected failure: alas-b new_red"));
        assert!(findings[1].contains("now passes"));
    }

    #[test]
    fn matching_run_passes_and_partial_run_is_not_an_error() {
        let baseline =
            parse_baseline("alas-a\talpha::listed_red\tpin\nalas-z\tgone\tpin\n").unwrap();
        let outcomes = parse_results("fail\talas-a\talpha::listed_red\nok\talas-a\tother\n", None);
        let (findings, unreported) = compare(&baseline, &outcomes);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(unreported, 1);
    }

    #[test]
    fn libtest_lines_use_the_default_crate() {
        let outcomes = parse_results("test a::b ... FAILED\ntest a::c ... ok\n", Some("k"));
        assert!(outcomes
            .failed
            .contains(&("k".to_owned(), "a::b".to_owned())));
        assert!(outcomes
            .passed
            .contains(&("k".to_owned(), "a::c".to_owned())));
    }

    #[test]
    fn a_nextest_progress_counter_is_not_read_as_the_binary_id() {
        let outcomes = parse_results(
            "        FAIL [   0.182s] ( 489/6865) alas-app cli::tests::t\n\
             \x20       PASS [   0.040s] (4564/6865) alas-pipeline a::b\n\
             \x20    TIMEOUT [ 600.371s] (1515/6865) alas-acceptance::m c\n",
            None,
        );
        assert!(outcomes
            .failed
            .contains(&("alas-app".to_owned(), "cli::tests::t".to_owned())));
        assert!(outcomes
            .passed
            .contains(&("alas-pipeline".to_owned(), "a::b".to_owned())));
        assert!(outcomes
            .failed
            .contains(&("alas-acceptance::m".to_owned(), "c".to_owned())));
    }

    #[test]
    fn malformed_baseline_rows_are_rejected() {
        assert!(parse_baseline("alas-a\tonly_two\n").is_err());
        assert!(parse_baseline("alas-a\tt\t\n").is_err());
        assert!(parse_baseline("a\tt\tr\na\tt\tr\n").is_err());
    }
}
