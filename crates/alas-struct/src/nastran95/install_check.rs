// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The closed-form check the one-click installer runs on a NASTRAN-95 it has just
//! built from source: the reference cantilever box against beam theory, the same
//! numerical validation the CI uses for a compiler's build.

use std::path::{Path, PathBuf};

use super::{run_cantilever_benchmark, CantileverBox, Nastran95Solver};

/// The name the installer manifest gives this check.
pub const INSTALL_CHECK_NAME: &str = "nastran95-closed-form";

/// Relative tolerance on mid-length deflection and flange stress against beam
/// theory (the CI gate of `tests/nastran95_closed_form.rs`).
const TOLERANCE: f64 = 0.05;

/// Deck length units per metre of the installer's solve (SI).
const LENGTH_PER_M: f64 = 1.0;

fn short_work_root() -> Result<PathBuf, String> {
    super::private_dir::create_private_dir("n95c-")
}

/// Solve the reference cantilever box with the NASTRAN-95 installed in
/// `install_dir` (holding `build/bin/nastran[.exe]` and `rf/`) and require deflection
/// and flange stress within 5 % of beam theory in all three load cases.
///
/// # Errors
///
/// The solver is incomplete, a solve fails, or a result is outside the tolerance.
pub fn installation_self_check(install_dir: &Path) -> Result<String, String> {
    let work = short_work_root()?;
    let outcome = run_check(install_dir, &work);
    let _ = std::fs::remove_dir_all(&work);
    outcome
}

fn run_check(install_dir: &Path, work: &Path) -> Result<String, String> {
    let mut solver =
        Nastran95Solver::from_paths(install_dir, None, None, None).ok_or_else(|| {
            format!(
                "{} does not hold a complete NASTRAN-95 (build/bin/{} and rf/NASINFO)",
                install_dir.display(),
                Nastran95Solver::EXECUTABLE_NAME
            )
        })?;
    // The same relative-path mode as a production solve of the installed tool.
    solver.private_work = true;
    let case = CantileverBox::reference();
    let outcome = run_cantilever_benchmark(&solver, &case, LENGTH_PER_M, &work.join("run"), 600.0)?;
    let worst = outcome
        .cases
        .iter()
        .map(|c| c.deflection_error().abs().max(c.stress_error().abs()))
        .fold(0.0_f64, f64::max);
    if outcome.within(TOLERANCE, TOLERANCE) {
        Ok(format!(
            "cantilever box matches beam theory (worst deflection/stress error {:.2} %, limit {:.0} %)",
            100.0 * worst,
            100.0 * TOLERANCE
        ))
    } else {
        Err(format!(
            "cantilever box is outside beam theory (worst deflection/stress error {:.2} %, limit {:.0} %)",
            100.0 * worst,
            100.0 * TOLERANCE
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs ALAS_NASTRAN95_DIR (+ ALAS_NASTRAN95_RUNTIME): a live NASTRAN-95 install"]
    fn a_live_install_passes_the_check_through_a_private_relative_work_tree() {
        let dir = std::env::var("ALAS_NASTRAN95_DIR").expect("ALAS_NASTRAN95_DIR");
        let detail = installation_self_check(Path::new(&dir)).expect("the check passes");
        assert!(detail.contains("matches beam theory"), "{detail}");
    }

    #[test]
    fn an_empty_directory_is_reported_as_an_incomplete_install() {
        let dir = std::env::temp_dir().join(format!("alas-n95-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let message = installation_self_check(&dir).expect_err("no solver in an empty directory");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            message.contains("does not hold a complete NASTRAN-95"),
            "{message}"
        );
    }
}
