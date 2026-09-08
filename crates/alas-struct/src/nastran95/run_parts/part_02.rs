// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


/// The displacement vector for each subcase, keyed by grid.
///
/// A subcase's table is printed as a contiguous run of grid rows in ascending
/// grid order, paginated by form feeds that repeat the title; a new subcase
/// restarts the grid order. So the parser collects rows while inside a
/// displacement section and opens a fresh table whenever a grid identifier drops
/// below the last one seen -- which separates a paginated continuation from a
/// new subcase without needing the page's subcase banner, and which the many
/// intervening `SPCFORCE`/`STRESS` tables of a modern deck cannot confuse,
/// because those are not displacement sections.
pub fn read_displacement_tables(print: &str) -> Vec<Vec<(i64, [f64; 6])>> {
    let mut tables: Vec<Vec<(i64, [f64; 6])>> = Vec::new();
    let mut in_displacement = false;
    let mut last_grid = i64::MAX;
    for line in text::splitlines(print) {
        if line.contains("D I S P L A C E M E N T   V E C T O R") {
            in_displacement = true;
            continue;
        }
        // Any other tabular section header ends the displacement span.
        if is_section_header(line) {
            in_displacement = false;
            continue;
        }
        if !in_displacement {
            continue;
        }
        if let Some((grid, row)) = displacement_row(line) {
            if grid <= last_grid || tables.is_empty() {
                tables.push(Vec::new());
            }
            last_grid = grid;
            if let Some(table) = tables.last_mut() {
                table.push((grid, row));
            }
        }
    }
    tables
}

/// The real eigenvector table for one extracted SOL 103 mode, keyed by grid.
///
/// NASTRAN-95 prints modal shapes under `REAL EIGENVECTOR`, not under the
/// modern solver's `DISPLACEMENT VECTOR` heading.  The same mode heading is
/// repeated at each printed page, so its `NO.` field, rather than the heading
/// count, keys the table. The row layout is otherwise the same six
/// translations/rotations.
pub fn read_eigenvector_tables(print: &str) -> Vec<Vec<(i64, [f64; 6])>> {
    let mut tables: Vec<Vec<(i64, [f64; 6])>> = Vec::new();
    let mut in_eigenvector = false;
    let mut current_table = None;
    for line in text::splitlines(print) {
        if line.contains("R E A L   E I G E N V E C T O R") {
            current_table = line
                .split_whitespace()
                .last()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|&mode| mode > 0)
                .map(|mode| mode - 1);
            if let Some(index) = current_table {
                while tables.len() <= index {
                    tables.push(Vec::new());
                }
                in_eigenvector = true;
            } else {
                in_eigenvector = false;
            }
            continue;
        }
        if line.contains("R E A L   E I G E N V A L U E S") {
            in_eigenvector = false;
            continue;
        }
        if in_eigenvector {
            if let Some((grid, row)) = displacement_row(line) {
                if let Some(table) = current_table.and_then(|index| tables.get_mut(index)) {
                    table.push((grid, row));
                }
            }
        }
    }
    tables
}

/// The displacement of one grid in one subcase, or `None` if it is not reported.
pub fn displacement_of(
    tables: &[Vec<(i64, [f64; 6])>],
    subcase: usize,
    grid: i64,
) -> Option<[f64; 6]> {
    tables
        .get(subcase)?
        .iter()
        .find(|&&(id, _)| id == grid)
        .map(|&(_, row)| row)
}

/// The real eigenvalues, one per extracted mode, in the order the table lists
/// them (ascending eigenvalue).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    /// The eigenvalue (radians-per-second squared).
    pub eigenvalue: f64,
    /// The cyclic frequency, Hz -- the fifth column.
    pub cyclic_hz: f64,
}

/// Read the `R E A L   E I G E N V A L U E S` table.
pub fn read_eigenvalues(print: &str) -> Vec<Mode> {
    let mut modes = Vec::new();
    let mut in_table = false;
    let mut started = false;
    for line in text::splitlines(print) {
        if line.contains("R E A L   E I G E N V A L U E S") {
            in_table = true;
            started = false;
            continue;
        }
        if !in_table {
            continue;
        }
        if let Some(mode) = eigenvalue_row(line) {
            started = true;
            modes.push(mode);
        } else if started && !line.trim().is_empty() && !is_page_furniture(line) {
            in_table = false;
        }
    }
    modes
}

/// A line's tokens, if it is `<grid> G <six reals>`.
fn displacement_row(line: &str) -> Option<(i64, [f64; 6])> {
    let mut tokens = line.split_whitespace();
    let grid: i64 = tokens.next()?.parse().ok()?;
    if tokens.next()? != "G" {
        return None;
    }
    let mut row = [0.0; 6];
    for slot in &mut row {
        *slot = tokens.next()?.parse().ok()?;
    }
    Some((grid, row))
}

/// A line's tokens, if it is an eigenvalue row `<mode> <order> <eigenvalue>
/// <radian> <cyclic> ...`.
fn eigenvalue_row(line: &str) -> Option<Mode> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }
    let _mode_no: i64 = tokens[0].parse().ok()?;
    let _order: i64 = tokens[1].parse().ok()?;
    let eigenvalue: f64 = tokens[2].parse().ok()?;
    let cyclic_hz: f64 = tokens[4].parse().ok()?;
    Some(Mode {
        eigenvalue,
        cyclic_hz,
    })
}

/// A line naming a different tabular section, which ends a displacement span.
fn is_section_header(line: &str) -> bool {
    const SECTIONS: [&str; 6] = [
        "F O R C E S",
        "S T R E S S E S",
        "R E A L   E I G E N V A L U E S",
        "E I G E N V A L U E",
        "O L O A D",
        "S O R T E D",
    ];
    SECTIONS.iter().any(|section| line.contains(section))
}

/// A line that is part of a table's paginated furniture rather than a data row.
fn is_page_furniture(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('+')
        || trimmed.starts_with('*')
        || line.contains("MESSAGE")
        || line.contains("MODE")
        || line.contains("NO.")
        || line.contains("EIGENVALUE")
}

fn drain<R: Read + Send + 'static>(mut stream: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stream.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    })
}

fn join(handle: thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rigid_format_directory_that_would_truncate_a_member_is_rejected() {
        let path = Path::new("C:/a-deliberately-long-nastran95-rigid-format-stage");
        let error =
            validate_rigid_format_stage(path).expect_err("the legacy buffer is only 44 bytes");
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
    fn a_configured_rf_stage_moves_transient_work_out_of_a_long_artifact_tree() {
        let solver = Nastran95Solver {
            exe: PathBuf::from("unused-nastran95.exe"),
            rf_source: PathBuf::from("unused-rf"),
            runtime: None,
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
            rf_stage: None,
            open_core_words: None,
            max_open_core_words: LEGACY_MAX_OPEN_CORE_WORDS,
        };
        for timeout in [f64::NAN, f64::MAX] {
            let outcome = run_nastran95(&solver, "", Path::new("not-created"), timeout);
            let RunOutcome::Failed(error) = outcome else {
                panic!("an invalid timeout must not launch a solve");
            };
            assert!(error.contains("representable"), "{error}");
        }
    }

    #[test]
    fn an_open_core_allocation_beyond_the_local_build_cap_is_rejected_before_staging() {
        let solver = Nastran95Solver {
            exe: PathBuf::from("unused-nastran95.exe"),
            rf_source: PathBuf::from("unused-rf"),
            runtime: None,
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
}

