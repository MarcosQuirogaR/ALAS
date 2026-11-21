// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The launch contract exercised end to end against a stub solver script, so
//! the Linux path (`build/bin/nastran`, `LD_LIBRARY_PATH`, environment names,
//! stdin/stdout plumbing) is tested without a Fortran build. Unix only: the
//! stub is a shell script.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// A solver tree whose `nastran` echoes its launch environment after a
/// minimal displacement table.
fn stub_tree(tag: &str, body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("an95-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bin = root.join("build/bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    std::fs::create_dir_all(root.join("rf")).expect("rf dir");
    std::fs::create_dir_all(root.join("runtime")).expect("runtime dir");
    std::fs::write(root.join("rf/NASINFO"), "TIM =    -99 \n").expect("NASINFO");
    std::fs::write(bin.join("nastran95-open-core.txt"), "64000000\n").expect("limit");
    let exe = root.join(Nastran95Solver::EXECUTABLE_RELATIVE);
    std::fs::write(&exe, format!("#!/bin/sh\n{body}\n")).expect("stub");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    root
}

#[test]
fn the_linux_launch_contract_reaches_the_solver() {
    let root = stub_tree(
        "ok",
        "deck=$(cat)\n\
         printf ' D I S P L A C E M E N T   V E C T O R\n'\n\
         printf '         1      G      0.0  0.0  4.0  0.0  0.0  0.0\n'\n\
         printf 'DECK=%s|OC=%s|DB=%s|LD=%s|RF=%s|FTN11=%s\n' \"$deck\" \"$OCMEM\" \"$DBMEM\" \"$LD_LIBRARY_PATH\" \"$RFDIR\" \"$FTN11\"",
    );
    let solver = Nastran95Solver::from_paths(&root, None, None, None).expect("stub is discovered");
    assert!(solver.exe.ends_with("build/bin/nastran"));
    assert_eq!(solver.max_open_core_words, 64_000_000);
    let work = root.join("w");
    let RunOutcome::Print(print) = run_nastran95(&solver, "ID DECK\r\n", &work, 30.0) else {
        panic!("the stub solver must produce a print file");
    };
    let tables = read_displacement_tables(&print);
    assert!((displacement_of(&tables, 0, 1).expect("grid 1")[2] - 4.0).abs() < 1e-12);
    let runtime = absolute_path(&root.join("runtime")).expect("absolute");
    assert!(print.contains("DECK=ID DECK|"), "{print}");
    assert!(print.contains("|OC=64000000|DB=1|"), "{print}");
    assert!(
        print.contains(&format!("|LD={}", runtime.display())),
        "adjacent runtime leads LD_LIBRARY_PATH: {print}"
    );
    assert!(
        print.contains(&format!("|RF={}", work.join("rf").display())),
        "{print}"
    );
    assert!(print.contains("|FTN11=none"), "{print}");
    assert!(work.join("rf/NASINFO").is_file());
    let patched = std::fs::read_to_string(work.join("rf/NASINFO")).expect("patched");
    assert!(patched.contains("TIM =     16"), "{patched}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_windows_executable_name_is_not_a_linux_solver() {
    let root = stub_tree("exe", "exit 0");
    std::fs::rename(
        root.join("build/bin/nastran"),
        root.join("build/bin/nastran.exe"),
    )
    .expect("rename");
    assert!(Nastran95Solver::from_paths(&root, None, None, None).is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_nonzero_exit_and_a_timeout_are_reported_with_the_linux_name() {
    let root = stub_tree("fail", "cat >/dev/null; echo boom >&2; exit 7");
    let solver = Nastran95Solver::from_paths(&root, None, None, None).expect("stub");
    let RunOutcome::Failed(error) = run_nastran95(&solver, "", &root.join("w"), 30.0) else {
        panic!("a nonzero exit must fail");
    };
    assert!(error.starts_with("nastran exited with code 7"), "{error}");
    assert!(error.contains("boom"), "{error}");

    let slow = stub_tree("slow", "sleep 30");
    let solver = Nastran95Solver::from_paths(&slow, None, None, None).expect("stub");
    let RunOutcome::Failed(error) = run_nastran95(&solver, "", &slow.join("w"), 0.5) else {
        panic!("a hung solver must time out");
    };
    assert!(error.starts_with("nastran timed out"), "{error}");
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(slow);
}
