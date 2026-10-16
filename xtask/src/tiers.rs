// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `cargo xtask tiers <junit.xml> [--also <older.xml>]... [--write]`: regenerate the test tiers.
//!
//! Three tiers partition every test by its measured time, so the dev loop can
//! stay fast without a test ever being skipped by the full gate:
//!
//! * **integration**: every integration-test binary whose tests of 20 s or
//!   more add up to 60 s or more, plus every unit test of 40 s or more;
//! * **medium**: every other test of 5 s or more;
//! * **fast**: the rest (`default` profile).
//!
//! A fourth set, the nightly tier (tests named `*_nightly`), is outside all three.
//!
//! `--also` names older reports. A test takes its slowest time over all of
//! them, and only tests present in the first report are classified. Run with
//! the shared result store on (`cargo xtask test`), a test's time depends on
//! whether another test computed its input first; the slowest time is the cold
//! cost, so a warm store cannot move a test to a faster tier.
//!
//! The input is the JUnit report of a `--profile full` run
//! (`target/nextest/full/junit.xml`). Without `--write` the command prints the
//! tier sizes and the summed time of each; with it, the `default-filter` lines
//! of `.config/nextest.toml` are rewritten in place.

use std::path::Path;

/// A binary whose tests of at least [`INTEGRATION_TEST_S`] sum to this joins
/// the integration tier whole, in seconds.
const INTEGRATION_BINARY_S: f64 = 60.0;
/// A test this slow counts toward its binary's sum, in seconds.
const INTEGRATION_TEST_S: f64 = 20.0;
/// A unit test this slow joins the integration tier alone, in seconds.
const INTEGRATION_UNIT_S: f64 = 40.0;
/// A test this slow leaves the fast tier, in seconds.
const MEDIUM_S: f64 = 5.0;

/// The filter term of the nightly tier: tests named `*_nightly`. They belong to
/// no other tier and the `full` profile leaves them out; the integration
/// profile still runs them with their binary.
pub const NIGHTLY: &str = "test(/_nightly$/)";

/// One test from the JUnit report.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// Nextest binary id (`pkg` for a library's unit tests).
    pub class: String,
    /// Test name within the binary.
    pub name: String,
    /// Wall time, seconds.
    pub secs: f64,
}

/// The tier selectors, each a nextest filterset term, sorted.
#[derive(Debug, Default, PartialEq)]
pub struct Tiers {
    /// Terms of the integration tier.
    pub integration: Vec<String>,
    /// Terms of the medium tier.
    pub medium: Vec<String>,
    /// Summed seconds of the integration, medium and fast tiers.
    pub seconds: [f64; 3],
    /// Test counts of the integration, medium and fast tiers.
    pub counts: [usize; 3],
}

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let write = args.iter().any(|a| a == "--write");
    let usage = "usage: cargo xtask tiers <junit.xml> [--also <older.xml>]... [--write]";
    let (mut first, mut older_paths) = (None, Vec::new());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--write" => {}
            "--also" => older_paths.push(iter.next().ok_or(usage)?),
            other if other.starts_with("--") => {
                return Err(format!("unknown tiers argument: {other}"))
            }
            _ => first = Some(arg),
        }
    }
    let first = first.ok_or(usage)?;
    let read = |path: &str| {
        std::fs::read_to_string(path)
            .map(|text| parse_junit(&text))
            .map_err(|e| format!("cannot read {path}: {e}"))
    };
    let mut cases = read(first)?;
    if cases.is_empty() {
        return Err(format!("{first} lists no test cases"));
    }
    for older in older_paths {
        cases = slowest(cases, &read(older)?);
    }
    let tiers = classify(&cases);
    let [i, m, f] = tiers.counts;
    let [is, ms, fs] = tiers.seconds;
    println!(
        "{} tests: integration {i}, medium {m}, fast {f}",
        cases.len()
    );
    println!(
        "summed seconds: integration {is:.0}, medium {ms:.0}, fast {fs:.0}; \
         ideal wall at 16 threads: {:.0} s, {:.0} s, {:.0} s",
        is / 16.0,
        ms / 16.0,
        fs / 16.0
    );
    if !write {
        return Ok(());
    }
    let path = root.join(".config").join("nextest.toml");
    let config = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // The checkout may be CRLF; the rewrite works on LF and restores the style.
    let crlf = config.contains("\r\n");
    let mut updated = rewrite_config(&config.replace("\r\n", "\n"), &tiers)?;
    if crlf {
        updated = updated.replace('\n', "\r\n");
    }
    if updated != config {
        std::fs::write(&path, updated)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!("rewrote {}", path.display());
    } else {
        println!("{} is already current", path.display());
    }
    Ok(())
}

/// `cases` with each time raised to the slowest `others` recorded for the same
/// test.
pub fn slowest(mut cases: Vec<Case>, others: &[Case]) -> Vec<Case> {
    use std::collections::HashMap;
    let mut worst: HashMap<(&str, &str), f64> = HashMap::new();
    for case in others {
        let at = worst.entry((&case.class, &case.name)).or_default();
        *at = at.max(case.secs);
    }
    for case in &mut cases {
        if let Some(&secs) = worst.get(&(case.class.as_str(), case.name.as_str())) {
            case.secs = case.secs.max(secs);
        }
    }
    cases
}

/// Every `<testcase>` of a JUnit report.
pub fn parse_junit(text: &str) -> Vec<Case> {
    text.split("<testcase ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = chunk.split('>').next()?;
            Some(Case {
                class: attribute(tag, "classname")?,
                name: attribute(tag, "name")?,
                secs: attribute(tag, "time")?.parse().ok()?,
            })
        })
        .collect()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let marker = format!(" {name}=\"");
    let tag = format!(" {tag}");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(
        tag[start..end]
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

/// A library's unit tests have the package name as their binary id; every
/// other binary id (`pkg::test`, `pkg::bin/x`) names a whole binary.
fn is_unit(class: &str) -> bool {
    !class.contains("::")
}

fn unit_term(case: &Case) -> String {
    format!(
        "(package({}) & kind(lib) & test(={}))",
        case.class, case.name
    )
}

/// Partition `cases` by the rules in the module documentation.
pub fn classify(cases: &[Case]) -> Tiers {
    use std::collections::{BTreeMap, BTreeSet};

    let cases: Vec<&Case> = cases
        .iter()
        .filter(|case| !case.name.ends_with("_nightly"))
        .collect();
    let mut slow_sum: BTreeMap<&str, f64> = BTreeMap::new();
    for case in cases.iter().filter(|c| !is_unit(&c.class)) {
        if case.secs >= INTEGRATION_TEST_S {
            *slow_sum.entry(&case.class).or_default() += case.secs;
        }
    }
    let binaries: BTreeSet<&str> = slow_sum
        .into_iter()
        .filter(|&(_, sum)| sum >= INTEGRATION_BINARY_S)
        .map(|(class, _)| class)
        .collect();

    let mut tiers = Tiers::default();
    let mut integration_units = BTreeSet::new();
    let mut medium = BTreeSet::new();
    for case in cases.iter().copied() {
        let integration = if is_unit(&case.class) {
            case.secs >= INTEGRATION_UNIT_S
        } else {
            binaries.contains(case.class.as_str())
        };
        let slot = if integration {
            if is_unit(&case.class) {
                integration_units.insert((case.class.clone(), case.name.clone(), unit_term(case)));
            }
            0
        } else if case.secs >= MEDIUM_S {
            let term = if is_unit(&case.class) {
                unit_term(case)
            } else {
                format!("(binary_id(={}) & test(={}))", case.class, case.name)
            };
            medium.insert((case.class.clone(), case.name.clone(), term));
            1
        } else {
            2
        };
        tiers.seconds[slot] += case.secs;
        tiers.counts[slot] += 1;
    }
    tiers.integration = binaries
        .iter()
        .map(|class| format!("binary_id({class})"))
        .chain(integration_units.into_iter().map(|(_, _, term)| term))
        .collect();
    tiers.medium = medium.into_iter().map(|(_, _, term)| term).collect();
    tiers
}

/// The `default-filter` value of each profile for `tiers`.
fn filters(tiers: &Tiers) -> [(&'static str, String); 4] {
    let integration = tiers.integration.join(" | ");
    let medium = tiers.medium.join(" | ");
    [
        (
            "default",
            format!("not ({integration} | {medium} | {NIGHTLY})"),
        ),
        ("integration", integration.clone()),
        ("slow", integration),
        ("medium", medium),
    ]
}

/// `config` with each tier profile's `default-filter` line replaced.
pub fn rewrite_config(config: &str, tiers: &Tiers) -> Result<String, String> {
    let mut out = config.to_owned();
    for (profile, filter) in filters(tiers) {
        let header = format!("[profile.{profile}]\n");
        let start = out
            .find(&header)
            .ok_or_else(|| format!(".config/nextest.toml has no [profile.{profile}]"))?
            + header.len();
        let line_start = out[start..]
            .find("default-filter = ")
            .map(|at| at + start)
            .ok_or_else(|| format!("[profile.{profile}] has no default-filter"))?;
        let line_end = out[line_start..]
            .find('\n')
            .map_or(out.len(), |at| at + line_start);
        out.replace_range(
            line_start..line_end,
            &format!("default-filter = \"{filter}\""),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(class: &str, name: &str, secs: f64) -> Case {
        Case {
            class: class.to_owned(),
            name: name.to_owned(),
            secs,
        }
    }

    const JUNIT: &str = r#"<testsuites><testsuite name="a">
        <testcase name="x::y" classname="a" timestamp="t" time="0.5"/>
        <testcase name="quote &quot;q&quot;" classname="a::big" timestamp="t" time="70.25"/>
        </testsuite></testsuites>"#;

    #[test]
    fn junit_cases_are_read_with_their_attributes_unescaped() {
        let cases = parse_junit(JUNIT);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0], case("a", "x::y", 0.5));
        assert_eq!(cases[1].name, "quote \"q\"");
        assert_eq!(cases[1].secs, 70.25);
    }

    #[test]
    fn the_tiers_partition_by_the_documented_thresholds() {
        let cases = vec![
            // a binary whose 20 s tests sum to 60 s joins whole
            case("p::heavy", "a", 30.0),
            case("p::heavy", "b", 31.0),
            case("p::heavy", "tiny", 0.1),
            // a binary below the sum keeps its 5 s+ tests in the medium tier
            case("p::mid", "slowish", 25.0),
            case("p::mid", "quick", 1.0),
            // unit tests: 40 s alone is integration, 5 s is medium
            case("p", "unit::long", 41.0),
            case("p", "unit::some", 5.0),
            case("p", "unit::fast", 4.9),
        ];
        let tiers = classify(&cases);
        assert_eq!(tiers.counts, [4, 2, 2]);
        assert!(tiers
            .integration
            .contains(&"binary_id(p::heavy)".to_owned()));
        assert!(tiers
            .integration
            .contains(&"(package(p) & kind(lib) & test(=unit::long))".to_owned()));
        assert_eq!(
            tiers.medium,
            [
                "(package(p) & kind(lib) & test(=unit::some))",
                "(binary_id(=p::mid) & test(=slowish))"
            ]
        );
    }

    #[test]
    fn the_slowest_time_over_several_reports_decides() {
        let current = vec![case("a", "t", 1.0), case("a", "u", 9.0)];
        let older = vec![
            case("a", "t", 30.0),
            case("a", "gone", 99.0),
            case("a", "u", 2.0),
        ];
        let merged = slowest(current, &older);
        assert_eq!(merged, vec![case("a", "t", 30.0), case("a", "u", 9.0)]);
    }

    #[test]
    fn nightly_tests_belong_to_no_tier() {
        let tiers = classify(&[
            case("p::heavy", "x_nightly", 500.0),
            case("p::heavy", "a", 61.0),
            case("p", "m_nightly", 50.0),
        ]);
        assert_eq!(tiers.counts, [1, 0, 0]);
        assert_eq!(tiers.integration, ["binary_id(p::heavy)"]);
    }

    #[test]
    fn rewriting_replaces_each_profile_line_and_is_idempotent() {
        let config = "[profile.default]\ndefault-filter = \"old\"\nx = 1\n\
                      [profile.integration]\ndefault-filter = \"old\"\n\
                      [profile.slow]\ndefault-filter = \"old\"\n\
                      [profile.medium]\ndefault-filter = \"old\"\n";
        let tiers = classify(&[case("p::heavy", "a", 61.0), case("p", "m", 6.0)]);
        let once = rewrite_config(config, &tiers).expect("rewrites");
        assert!(once.contains(
            "[profile.default]\ndefault-filter = \"not (binary_id(p::heavy) | \
             (package(p) & kind(lib) & test(=m)) | test(/_nightly$/))\"\nx = 1\n"
        ));
        assert!(once.contains(
            "[profile.medium]\ndefault-filter = \"(package(p) & kind(lib) & test(=m))\"\n"
        ));
        assert_eq!(rewrite_config(&once, &tiers).expect("rewrites"), once);
    }
}
