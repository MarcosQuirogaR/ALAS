// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tiered gate.
//!
//! * full (`cargo xtask gate`): checks, formatting, clippy and every test. CI
//!   and phase integration.
//! * quick (`--quick [--base <ref>]`): checks, formatting, then clippy and the
//!   fast test tier for the packages a change affects. Every change.
//!   `--part lint` runs only checks, formatting and clippy; `--part tests` only
//!   the nextest run; CI runs the two parts as parallel jobs.
//! * integration (`--integration`, alias `--slow`): only the heavy test tier
//!   (nextest profile `integration`).
//! * medium (`--medium`): the tests of 5 s or more outside the integration
//!   tier; nightly (`--nightly`): the `*_nightly` tests the full tier leaves out.
//!
//! The test steps run with the shared result store switched on
//! (`crate::memo`), so tests that need the same expensive nominal computation
//! share it across processes.
//!
//! Tests run under cargo-nextest when it is installed, which is what makes the
//! tiers possible (profiles live in `.config/nextest.toml`). Without it the
//! full tier falls back to `cargo test`, without doctests: the repository
//! checks reject runnable doc examples, and rustdoc startup alone cost minutes.

use crate::affected::{self, Affected};
use crate::{cargo, cargo_env, memo, run_checks};
use std::path::Path;
use std::process::{Command, Stdio};

/// Which steps of the quick gate to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    All,
    Lint,
    Tests,
}

enum Tier {
    Full,
    Quick { base: Option<String>, part: Part },
    Slow,
    Medium,
    Nightly,
}

fn parse_args(args: &[String]) -> Result<Tier, String> {
    let (mut quick, mut slow, mut medium, mut nightly, mut base) =
        (false, false, false, false, None);
    let mut part: Option<Part> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--quick" => quick = true,
            "--slow" | "--integration" => slow = true,
            "--medium" => medium = true,
            "--nightly" => nightly = true,
            "--base" => base = Some(iter.next().ok_or("--base needs a git ref")?.clone()),
            "--part" => {
                part = Some(match iter.next().map(String::as_str) {
                    Some("lint") => Part::Lint,
                    Some("tests") => Part::Tests,
                    _ => return Err("--part needs `lint` or `tests`".to_owned()),
                });
            }
            other => return Err(format!("unknown gate argument: {other}")),
        }
    }
    if part.is_some() && !quick {
        return Err("--part only applies to --quick".to_owned());
    }
    let chosen = [quick, slow, medium, nightly]
        .iter()
        .filter(|&&on| on)
        .count();
    if chosen > 1 {
        return Err("--quick, --slow, --medium and --nightly are mutually exclusive".to_owned());
    }
    match (quick, slow, medium, nightly, base) {
        (false, _, _, _, Some(_)) => Err("--base only applies to --quick".to_owned()),
        (true, _, _, _, base) => Ok(Tier::Quick {
            base,
            part: part.unwrap_or(Part::All),
        }),
        (_, true, _, _, None) => Ok(Tier::Slow),
        (_, _, true, _, None) => Ok(Tier::Medium),
        (_, _, _, true, None) => Ok(Tier::Nightly),
        _ => Ok(Tier::Full),
    }
}

/// `cargo xtask test [nextest args]`: `cargo nextest run` with the shared
/// result store enabled. Without arguments it runs the fast tier.
pub fn test(root: &Path, args: &[String]) -> Result<(), String> {
    if !nextest_available(root) {
        return Err("xtask test needs cargo-nextest: cargo install cargo-nextest --locked".into());
    }
    let mut command: Vec<&str> = vec!["nextest", "run", "--no-fail-fast"];
    command.extend(args.iter().map(String::as_str));
    cargo_env(root, &command, &memo_env(root))
}

/// The result-store environment, or nothing (with a notice) when the tree
/// cannot be hashed: the tests then just compute everything themselves.
fn memo_env(root: &Path) -> Vec<(String, String)> {
    memo::memo_env(root).unwrap_or_else(|error| {
        println!(
            "
result store disabled: {error}"
        );
        Vec::new()
    })
}

/// Runs the gate for the tier named by `args`. Every step runs even if an
/// earlier one failed, so one failing step can never hide the others.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let nextest = nextest_available(root);
    let env = memo_env(root);
    let results = match parse_args(args)? {
        Tier::Full => {
            let tests = if nextest {
                cargo_env(
                    root,
                    &[
                        "nextest",
                        "run",
                        "--workspace",
                        "--profile",
                        "full",
                        "--no-fail-fast",
                    ],
                    &env,
                )
            } else {
                cargo(
                    root,
                    &[
                        "test",
                        "--workspace",
                        "--lib",
                        "--bins",
                        "--tests",
                        "--no-fail-fast",
                    ],
                )
            };
            vec![
                run_checks(root),
                cargo(root, &["fmt", "--all", "--check"]),
                cargo(
                    root,
                    &[
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--",
                        "-D",
                        "warnings",
                    ],
                ),
                tests,
            ]
        }
        Tier::Slow => {
            if !nextest {
                return Err(
                    "gate --integration needs cargo-nextest: cargo install cargo-nextest --locked"
                        .to_owned(),
                );
            }
            vec![cargo_env(
                root,
                &[
                    "nextest",
                    "run",
                    "--workspace",
                    "--profile",
                    "integration",
                    "--no-fail-fast",
                ],
                &env,
            )]
        }
        tier @ (Tier::Medium | Tier::Nightly) => {
            if !nextest {
                return Err("gate --medium and --nightly need cargo-nextest".to_owned());
            }
            let profile = if matches!(tier, Tier::Medium) {
                "medium"
            } else {
                "nightly"
            };
            vec![cargo_env(
                root,
                &[
                    "nextest",
                    "run",
                    "--workspace",
                    "--profile",
                    profile,
                    "--no-fail-fast",
                ],
                &env,
            )]
        }
        Tier::Quick { base, part } => quick(root, base.as_deref(), part, nextest, &env)?,
    };
    report_gate_results(results)
}

fn quick(
    root: &Path,
    base: Option<&str>,
    part: Part,
    nextest: bool,
    env: &[(String, String)],
) -> Result<Vec<Result<(), String>>, String> {
    let (packages, changed) = (workspace_packages(root)?, changed_files(root, base)?);
    let mut results = Vec::new();
    if part != Part::Tests {
        results.push(run_checks(root));
        results.push(cargo(root, &["fmt", "--all", "--check"]));
    }

    let selection: Vec<String> = match affected::affected(&packages, &changed) {
        Affected::All(why) => {
            println!("\ngate --quick: all packages affected ({why})");
            packages.iter().map(|p| p.name.clone()).collect()
        }
        Affected::Some(map) => {
            for (name, why) in &map {
                println!("gate --quick: affected {name} ({why})");
            }
            map.into_keys().collect()
        }
    };
    if selection.is_empty() {
        println!("\ngate --quick: no package affected; clippy and tests skipped");
        return Ok(results);
    }

    let mut selectors: Vec<&str> = Vec::new();
    for name in &selection {
        selectors.extend(["-p", name]);
    }
    let with = |head: &[&'static str], tail: &[&'static str]| -> Vec<&str> {
        [head, &selectors, tail].concat()
    };
    if part != Part::Tests {
        results.push(cargo(
            root,
            &with(&["clippy"], &["--all-targets", "--", "-D", "warnings"]),
        ));
    }
    if part == Part::Lint {
        return Ok(results);
    }
    results.push(if nextest {
        cargo_env(root, &with(&["nextest", "run"], &["--no-fail-fast"]), env)
    } else {
        println!(
            "\ngate --quick: cargo-nextest is not installed, so the slow-tier split is off and \
             the affected packages' whole suites run; install it with \
             `cargo install cargo-nextest --locked`"
        );
        cargo(
            root,
            &with(&["test"], &["--lib", "--bins", "--tests", "--no-fail-fast"]),
        )
    });
    Ok(results)
}

/// Reports every failed gate step instead of stopping at the first one.
pub fn report_gate_results(results: Vec<Result<(), String>>) -> Result<(), String> {
    let total = results.len();
    let failures: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    if failures.is_empty() {
        println!("\ngate: pass");
        return Ok(());
    }
    for failure in &failures {
        println!("\ngate: {failure}");
    }
    Err(format!(
        "gate: {} of {total} check(s) failed",
        failures.len()
    ))
}

fn nextest_available(root: &Path) -> bool {
    Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn workspace_packages(root: &Path) -> Result<Vec<affected::Package>, String> {
    let json = git_or_cargo(Command::new(env!("CARGO")).current_dir(root).args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
    ]))?;
    affected::parse_metadata(&json)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    git_or_cargo(
        Command::new("git")
            .current_dir(root)
            .args(["-c", "core.quotepath=off"])
            .args(args),
    )
}

fn git_or_cargo(command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("cannot run {:?}: {e}", command.get_program()))?;
    if !output.status.success() {
        return Err(format!(
            "{:?} failed: {}",
            command.get_program(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Files changed against the merge-base with the base ref, plus uncommitted
/// and untracked files. Without `--base` the default falls back through
/// `origin/dev`, `dev` and `HEAD~1` for checkouts that lack the first ones.
fn changed_files(root: &Path, base: Option<&str>) -> Result<Vec<String>, String> {
    let candidates: Vec<&str> = base.map_or(vec!["origin/dev", "dev", "HEAD~1"], |b| vec![b]);
    let base = candidates
        .iter()
        .find(|c| {
            git(
                root,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{c}^{{commit}}"),
                ],
            )
            .is_ok()
        })
        .ok_or_else(|| format!("none of the base refs exist: {}", candidates.join(", ")))?;
    println!("gate --quick: base {base}");
    let merge_base = git(root, &["merge-base", "HEAD", base])?.trim().to_owned();

    let mut files: Vec<String> = [
        vec!["diff", "--name-only", merge_base.as_str()],
        vec!["diff", "--name-only"],
        vec!["ls-files", "--others", "--exclude-standard"],
    ]
    .iter()
    .map(|args| git(root, args))
    .collect::<Result<Vec<_>, _>>()?
    .iter()
    .flat_map(|out| out.lines().map(str::to_owned))
    .filter(|l| !l.is_empty())
    .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn report_gate_results_passes_when_all_steps_pass() {
        assert!(report_gate_results(vec![Ok(()), Ok(()), Ok(()), Ok(())]).is_ok());
    }

    #[test]
    fn report_gate_results_reports_every_failure_not_just_the_first() {
        let results = vec![
            Err("checks: 1 finding(s)".to_owned()),
            Ok(()),
            Err("cargo clippy --workspace --all-targets -- -D warnings failed".to_owned()),
            Ok(()),
        ];
        let error = report_gate_results(results).expect_err("two steps failed");
        assert_eq!(error, "gate: 2 of 4 check(s) failed");
    }

    #[test]
    fn report_gate_results_counts_a_shorter_quick_run() {
        let error = report_gate_results(vec![Err("x".to_owned()), Ok(())]).expect_err("one failed");
        assert_eq!(error, "gate: 1 of 2 check(s) failed");
    }

    #[test]
    fn arguments_select_the_tier() {
        assert!(matches!(parse_args(&args(&[])), Ok(Tier::Full)));
        assert!(matches!(parse_args(&args(&["--slow"])), Ok(Tier::Slow)));
        assert!(matches!(
            parse_args(&args(&["--integration"])),
            Ok(Tier::Slow)
        ));
        let quick = parse_args(&args(&["--quick", "--base", "main"]));
        assert!(matches!(quick, Ok(Tier::Quick { base: Some(b), part: Part::All }) if b == "main"));
        let lint = parse_args(&args(&["--quick", "--part", "lint"]));
        assert!(matches!(
            lint,
            Ok(Tier::Quick {
                base: None,
                part: Part::Lint
            })
        ));
        let tests = parse_args(&args(&["--quick", "--part", "tests"]));
        assert!(matches!(
            tests,
            Ok(Tier::Quick {
                base: None,
                part: Part::Tests
            })
        ));
    }

    #[test]
    fn invalid_argument_combinations_are_rejected() {
        for bad in [
            &["--quick", "--slow"][..],
            &["--base", "main"],
            &["--slow", "--base", "main"],
            &["--quick", "--base"],
            &["--fast"],
            &["--part", "lint"],
            &["--quick", "--part"],
            &["--quick", "--part", "both"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }

    /// The quoted `default-filter` expression of a nextest profile.
    fn default_filter(config: &str, profile: &str) -> String {
        let header = format!("[profile.{profile}]");
        let section = config.split(&header).nth(1).expect("profile exists");
        let line = section
            .lines()
            .find(|l| l.starts_with("default-filter"))
            .expect("profile has a default-filter");
        let quoted = line.split_once('=').expect("key = value").1.trim();
        quoted.trim_matches('"').to_owned()
    }

    #[test]
    fn the_fast_profile_excludes_exactly_what_the_other_tiers_select() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.config/nextest.toml");
        let config = std::fs::read_to_string(path).expect("nextest config exists");
        let slow = default_filter(&config, "integration");
        let medium = default_filter(&config, "medium");
        assert!(slow.contains("binary_id("));
        assert_eq!(default_filter(&config, "slow"), slow, "slow is an alias");
        assert_eq!(
            default_filter(&config, "default"),
            format!("not ({slow} | {medium} | {})", crate::tiers::NIGHTLY)
        );
        assert_eq!(default_filter(&config, "nightly"), crate::tiers::NIGHTLY);
        assert_eq!(
            default_filter(&config, "full"),
            format!("not {}", crate::tiers::NIGHTLY)
        );
    }

    #[test]
    fn no_test_is_in_both_the_integration_and_the_medium_tier() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.config/nextest.toml");
        let config = std::fs::read_to_string(path).expect("nextest config exists");
        let slow = default_filter(&config, "integration");
        let medium = default_filter(&config, "medium");
        for term in slow.split(" | ") {
            if let Some(binary) = term
                .strip_prefix("binary_id(")
                .and_then(|t| t.strip_suffix(')'))
            {
                assert!(
                    !medium.contains(&format!("binary_id(={binary})")),
                    "{binary} is in both the medium and the integration tier"
                );
            }
        }
    }
}
