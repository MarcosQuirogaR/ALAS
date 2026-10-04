// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parsing the print file the solver writes on stdout.

use super::*;

/// The displacement vector for each subcase, keyed by grid.
///
/// A subcase's table is printed as a contiguous run of grid rows in ascending
/// grid order, paginated by form feeds that repeat the title; a new subcase
/// restarts the grid order. So the parser collects rows while inside a
/// displacement section and opens a fresh table whenever a grid identifier drops
/// below the last one seen, which separates a paginated continuation from a
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
    /// The cyclic frequency, Hz: the fifth column.
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
