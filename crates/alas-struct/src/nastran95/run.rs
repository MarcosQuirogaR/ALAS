// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Driving the built NASTRAN-95 solver, and reading its print file back.
//!
//! The run contract is nothing like the modern one [`crate::nastran::run`]
//! drives. There is no deck argument and no result file keyword: the solver
//! reads the deck on **stdin**, writes the print file on **stdout**, and is
//! configured entirely through environment variables -- `DBMEM`, `OCMEM`,
//! `RFDIR`, `DIRCTY` and the rest of `mds/nastrn.f`'s `GETENV` list. Four of
//! those cost a run each to discover and are honoured here: `NASINFO` is read
//! from `$RFDIR` and not the working directory, so a copy with its timing
//! constants switched on is staged there; most `/DOSNAM/` paths are
//! `CHARACTER*72`, but the rigid-format loader has its own 44-byte `RFDIR` and
//! destination buffer. Its longest shipped member is `AERO10`, so the staged
//! directory itself can occupy at most 37 bytes. A long scratch directory still
//! comes back empty rather than truncated, and a long rigid-format directory
//! truncates a member name and fails in `RFOPEN`; this runner checks the latter
//! before it launches and accepts a separately configured short stage directory.
//! When that stage is configured, the solver's transient work directory is
//! placed beside it rather than beneath the retained artifact tree: the legacy
//! `DOSNAM` buffers are only 72 bytes, while a useful desktop output directory
//! commonly exceeds that once `run.log` or `scr` is appended.
//! `OCMEM` may select any allocation up to the executable's fixed open-core
//! array. Current local builds write that limit beside `nastran.exe`, letting
//! the runner use the compiled capacity by default and reject invalid requests
//! before the Fortran program exits successfully without results.
//! The deck is fed with its carriage returns stripped, since a bare `CR` lands
//! in column 81 and the scanner rejects the card; and the runtime directory
//! carrying `libgfortran` has to be on `PATH`.
//!
//! The parser is shared with the modern solver on purpose: NASTRAN-95 and MSC
//! print the same `D I S P L A C E M E N T   V E C T O R` and
//! `R E A L   E I G E N V A L U E S` tables, in the same columns, because one
//! is the other's ancestor. So the cross-solver comparison reads both outputs
//! through [`read_displacement_tables`] and [`read_eigenvalues`], and any
//! difference it finds is in the solve and not in two different parsers.

include!("run_parts/part_01.rs");
include!("run_parts/part_02.rs");
