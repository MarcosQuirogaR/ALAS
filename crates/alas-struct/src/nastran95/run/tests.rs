// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn a_rigid_format_directory_that_would_truncate_a_member_is_rejected() {
    let path = Path::new("C:/a-deliberately-long-nastran95-rigid-format-stage");
    let error = validate_rigid_format_stage(path).expect_err("the legacy buffer is only 44 bytes");
    assert!(error.contains("RFOPEN permits at most"), "{error}");
    assert!(error.contains("ALAS_NASTRAN95_RF_STAGE"), "{error}");
}

#[test]
fn a_relative_work_directory_is_resolved_before_it_reaches_the_child() {
    let work = absolute_path(Path::new("relative-nastran95-work"))
        .expect("the current directory is available to a test");
    assert!(work.is_absolute(), "{}", work.display());
    assert!(
        work.ends_with("relative-nastran95-work"),
        "{}",
        work.display()
    );
}

#[test]
fn a_stale_runtime_preference_uses_the_adjacent_solver_bundle() {
    let root = std::env::temp_dir().join(format!(
        "alas-nastran95-adjacent-runtime-{}",
        std::process::id()
    ));
    let executable = root.join("build/bin/nastran.exe");
    let rf = root.join("rf");
    let runtime = root.join("runtime");
    let stale = root.join("old-msys2/mingw64/bin");
    std::fs::create_dir_all(executable.parent().expect("fixture executable parent"))
        .expect("fixture executable directory creates");
    std::fs::create_dir_all(&rf).expect("fixture RF directory creates");
    std::fs::create_dir_all(&runtime).expect("fixture runtime directory creates");
    std::fs::write(&executable, b"solver fixture").expect("fixture executable writes");
    std::fs::write(rf.join("NASINFO"), b"NASINFO").expect("fixture RF writes");

    let solver = Nastran95Solver::from_paths(&root, Some(&stale), None, None)
        .expect("solver fixture resolves");
    assert_eq!(solver.runtime, Some(runtime.clone()));
    assert_eq!(
        solver.runtime_source,
        Nastran95RuntimeSource::Adjacent { root: root.clone() }
    );
    let warning = solver
        .runtime_warning()
        .expect("a stale runtime must be observable");
    assert!(warning.contains("using adjacent runtime"), "{warning}");
    assert!(warning.contains("runtime"), "{warning}");
    assert!(
        warning.contains(
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
        ),
        "{warning}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_existing_runtime_preference_remains_authoritative() {
    let root = std::env::temp_dir().join(format!(
        "alas-nastran95-configured-runtime-{}",
        std::process::id()
    ));
    let executable = root.join("build/bin/nastran.exe");
    let rf = root.join("rf");
    let configured = root.join("configured-runtime");
    let adjacent = root.join("runtime");
    std::fs::create_dir_all(executable.parent().expect("fixture executable parent"))
        .expect("fixture executable directory creates");
    std::fs::create_dir_all(&rf).expect("fixture RF directory creates");
    std::fs::create_dir_all(&configured).expect("configured runtime creates");
    std::fs::create_dir_all(&adjacent).expect("adjacent runtime creates");
    std::fs::write(&executable, b"solver fixture").expect("fixture executable writes");
    std::fs::write(rf.join("NASINFO"), b"NASINFO").expect("fixture RF writes");

    let solver = Nastran95Solver::from_paths(&root, Some(&configured), None, None)
        .expect("solver fixture resolves");
    assert_eq!(solver.runtime, Some(configured.clone()));
    assert_eq!(solver.runtime_source, Nastran95RuntimeSource::Configured);
    assert!(solver.runtime_warning().is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_configured_rf_stage_moves_transient_work_out_of_a_long_artifact_tree() {
    let solver = Nastran95Solver {
        exe: PathBuf::from("unused-nastran95.exe"),
        rf_source: PathBuf::from("unused-rf"),
        runtime: None,
        runtime_source: Nastran95RuntimeSource::Unspecified,
        rf_stage: Some(PathBuf::from("C:/nas-rf")),
        open_core_words: None,
        max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
    };
    let work = solver
        .workspace_for(Path::new("C:/a/deep/artifact/tree/nastran95/sol101"))
        .expect("a rooted RF stage has a parent");
    assert!(
        work.ancestors()
            .any(|path| path.file_name().is_some_and(|name| name == "nas-run")),
        "{}",
        work.display()
    );
    assert!(work.ends_with("sol101"), "{}", work.display());
    assert!(!work.starts_with("C:/a/deep"), "{}", work.display());
}

#[test]
fn an_overlong_dosnam_path_is_rejected_before_staging() {
    let work =
        Path::new("C:/this-is-an-intentionally-long-nastran95-working-directory-name/sol101");
    let error = validate_dosnam_paths(work).expect_err("DOSNAM has a 72-byte limit");
    assert!(error.contains("DOSNAM permits at most"), "{error}");
}

#[test]
fn an_invalid_timeout_is_reported_before_the_runner_touches_the_filesystem() {
    let solver = Nastran95Solver {
        exe: PathBuf::from("unused-nastran95.exe"),
        rf_source: PathBuf::from("unused-rf"),
        runtime: None,
        runtime_source: Nastran95RuntimeSource::Unspecified,
        rf_stage: None,
        open_core_words: None,
        max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
    };
    for timeout in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        let outcome = run_nastran95(&solver, "", Path::new("not-created"), timeout);
        let RunOutcome::Failed(error) = outcome else {
            panic!("an invalid timeout must not launch a solve");
        };
        assert!(error.starts_with("nastran.exe timeout must be"), "{error}");
    }
    assert!(!Path::new("not-created").exists());
}

#[test]
fn an_open_core_allocation_beyond_the_local_build_cap_is_rejected_before_staging() {
    let solver = Nastran95Solver {
        exe: PathBuf::from("unused-nastran95.exe"),
        rf_source: PathBuf::from("unused-rf"),
        runtime: None,
        runtime_source: Nastran95RuntimeSource::Unspecified,
        rf_stage: None,
        open_core_words: Some("32000000".to_owned()),
        max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
    };
    let outcome = run_nastran95(&solver, "", Path::new("not-created"), 30.0);
    let RunOutcome::Failed(error) = outcome else {
        panic!("an invalid open-core allocation must not launch a solve");
    };
    assert!(error.contains("14000000"), "{error}");
    assert!(!Path::new("not-created").exists());
}

#[test]
fn an_unset_open_core_uses_the_entire_compiled_allocation() {
    let words = open_core_words(None, 64_000_000).expect("a positive compiled limit");
    assert_eq!(words, "64000000");
}

#[test]
fn a_nonzero_exit_is_not_accepted_even_if_stdout_looks_like_a_result_table() {
    let outcome = supervise(
        nonzero_command_with_a_displacement_table(),
        "ignored",
        Duration::from_secs(5),
    );
    let RunOutcome::Failed(error) = outcome else {
        panic!("a non-zero solver exit must never be a usable solve");
    };
    assert!(error.contains("exited with code 7"), "{error}");
}

#[test]
fn a_displacement_table_is_read_by_grid() {
    let print = "\
                                         D I S P L A C E M E N T   V E C T O R
  POINT ID.   TYPE          T1             T2             T3
         1      G      0.0            0.0            0.0            0.0            0.0            0.0
         2      G      1.0E-3         0.0            5.0E-2         0.0           -1.0E-1         0.0
";
    let tables = read_displacement_tables(print);
    assert_eq!(tables.len(), 1);
    let row = displacement_of(&tables, 0, 2).unwrap();
    assert!((row[2] - 5.0e-2).abs() < 1e-12);
    assert!(displacement_of(&tables, 0, 9).is_none());
}

#[test]
fn eigenvector_page_headers_append_to_the_mode_they_name() {
    let print = "\
 R E A L   E I G E N V E C T O R   N O .          1
         1      G      0.0  0.0  1.0  0.0  0.0  0.0
 R E A L   E I G E N V E C T O R   N O .          1
         2      G      0.0  0.0  1.5  0.0  0.0  0.0
 R E A L   E I G E N V E C T O R   N O .          2
         1      G      0.0  0.0  2.0  0.0  0.0  0.0
";
    let tables = read_eigenvector_tables(print);
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0][0].1[2], 1.0);
    assert_eq!(tables[0][1].1[2], 1.5);
    assert_eq!(tables[1][0].1[2], 2.0);
}

#[test]
fn pagination_keeps_one_subcase_together_and_a_grid_reset_starts_the_next() {
    // Two pages of subcase 1 (grids ascending across a repeated title), then
    // an SPCFORCE section, then subcase 2 restarting from grid 1.
    let print = "\
 D I S P L A C E M E N T   V E C T O R
         1      G      0.0  0.0  1.0  0.0  0.0  0.0
         2      G      0.0  0.0  2.0  0.0  0.0  0.0
 D I S P L A C E M E N T   V E C T O R
         3      G      0.0  0.0  3.0  0.0  0.0  0.0
 F O R C E S   O F   S I N G L E   P O I N T   C O N S T R A I N T
         1      G      9.0  0.0  0.0  0.0  0.0  0.0
 D I S P L A C E M E N T   V E C T O R
         1      G      0.0  0.0  7.0  0.0  0.0  0.0
         3      G      0.0  0.0  9.0  0.0  0.0  0.0
";
    let tables = read_displacement_tables(print);
    assert_eq!(tables.len(), 2, "{tables:?}");
    assert_eq!(tables[0].len(), 3);
    assert!((displacement_of(&tables, 0, 3).unwrap()[2] - 3.0).abs() < 1e-12);
    // The SPCFORCE row for grid 1 must not have polluted the displacements.
    assert!((displacement_of(&tables, 1, 1).unwrap()[2] - 7.0).abs() < 1e-12);
    assert!((displacement_of(&tables, 1, 3).unwrap()[2] - 9.0).abs() < 1e-12);
}

#[test]
fn the_eigenvalue_table_yields_cyclic_frequencies_in_order() {
    let print = "\
                                          R E A L   E I G E N V A L U E S
   MODE    EXTRACTION       EIGENVALUE            RADIAN              CYCLIC
NO.       ORDER
    1         2        3.237408E+01        5.689823E+00        9.055634E-01        1.0E+04        3.3E+05
    2         1        2.022407E+02        1.422113E+01        2.263364E+00        1.0E+04        2.0E+06
 SORTED BULK
";
    let modes = read_eigenvalues(print);
    assert_eq!(modes.len(), 2);
    assert!((modes[0].cyclic_hz - 9.055634e-1).abs() < 1e-9);
    assert!((modes[1].cyclic_hz - 2.263364).abs() < 1e-9);
    assert!((modes[0].eigenvalue - 3.237408e1).abs() < 1e-6);
}

#[test]
fn a_print_file_with_no_tables_reads_as_empty() {
    assert!(read_displacement_tables("nothing here").is_empty());
    assert!(read_eigenvalues("nothing here").is_empty());
}

#[cfg(windows)]
#[test]
fn an_imported_gnu_runtime_is_rejected_before_a_child_can_show_a_loader_dialog() {
    let root = std::env::temp_dir().join(format!(
        "alas-nastran95-runtime-missing-{}",
        std::process::id()
    ));
    let executable = root.join("nastran.exe");
    std::fs::create_dir_all(&root).expect("runtime fixture directory creates");
    std::fs::write(
        &executable,
        b"PE fixture libgcc_s_seh-1.dll libgfortran-5.dll",
    )
    .expect("runtime fixture writes");

    // The validator also searches the inherited PATH, and a host that
    // carries a GNU runtime there (Git's own mingw64/bin ships
    // libgcc_s_seh-1.dll) resolves that import legitimately. The
    // expectation therefore follows the host: an import is reported
    // missing exactly when no PATH entry provides it.
    let on_path = |name: &str| {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
        })
    };
    let fixture = ["libgcc_s_seh-1.dll", "libgfortran-5.dll"];
    if fixture.iter().all(|name| on_path(name)) {
        let _ = std::fs::remove_dir_all(root);
        return;
    }
    let error = validate_runtime_dependencies(&executable, Some(&root.join("missing")))
        .expect_err("missing imported DLLs must fail before spawn");
    for name in fixture {
        assert_eq!(error.contains(name), !on_path(name), "{name}: {error}");
    }
    assert!(error.contains("no solver process was spawned"), "{error}");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn imported_gnu_runtime_is_accepted_when_the_configured_directory_is_complete() {
    let root = std::env::temp_dir().join(format!(
        "alas-nastran95-runtime-present-{}",
        std::process::id()
    ));
    let executable = root.join("nastran.exe");
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).expect("runtime fixture directory creates");
    std::fs::write(
        &executable,
        b"PE fixture libgcc_s_seh-1.dll libgfortran-5.dll",
    )
    .expect("runtime fixture writes");
    for name in ["libgcc_s_seh-1.dll", "libgfortran-5.dll"] {
        std::fs::write(runtime.join(name), b"DLL fixture").expect("DLL fixture writes");
    }

    validate_runtime_dependencies(&executable, Some(&runtime))
        .expect("all imported runtime DLLs are present");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn a_static_or_differently_built_solver_does_not_require_gnu_runtime_files() {
    let root = std::env::temp_dir().join(format!(
        "alas-nastran95-runtime-static-{}",
        std::process::id()
    ));
    let executable = root.join("nastran.exe");
    std::fs::create_dir_all(&root).expect("runtime fixture directory creates");
    std::fs::write(&executable, b"PE fixture with no GNU imports").expect("runtime fixture writes");

    validate_runtime_dependencies(&executable, None)
        .expect("no imported GNU DLLs means no runtime preflight is needed");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_fatal_message_across_a_form_feed_is_found() {
    let print = "some output\u{c}   *** USER FATAL MESSAGE 9994 (IFP)   ";
    assert_eq!(fatal_lines(print), ["*** USER FATAL MESSAGE 9994 (IFP)"]);
}

#[cfg(windows)]
fn nonzero_command_with_a_displacement_table() -> Command {
    let mut command = Command::new("cmd.exe");
    command.args([
        "/d",
        "/c",
        "echo D I S P L A C E M E N T   V E C T O R & echo 1 G 0 0 0 0 0 0 & exit /b 7",
    ]);
    command
}

#[cfg(not(windows))]
fn nonzero_command_with_a_displacement_table() -> Command {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'D I S P L A C E M E N T   V E C T O R\\n1 G 0 0 0 0 0 0\\n'; exit 7",
    ]);
    command
}
