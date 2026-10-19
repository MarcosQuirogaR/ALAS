// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! MSC launchers may return zero after an allocation failure with empty F06.
use std::path::Path;

pub(super) fn runtime_failure(deck: &Path) -> Option<String> {
    let log = deck.with_extension("log");
    let bytes = std::fs::read(&log).ok()?;
    let content = String::from_utf8_lossy(&bytes);
    let failures = failure_lines(&content);
    (!failures.is_empty()).then(|| {
        format!(
            "{} reports a runtime failure:\n{}",
            log.display(),
            failures.join("\n")
        )
    })
}

fn failure_lines(content: &str) -> Vec<&str> {
    content
        .lines()
        .filter(|line| {
            let upper = line.to_ascii_uppercase();
            upper.contains("OPEN CORE MEMORY ALLOCATION FAILED")
                || upper.contains("OPEN CORE ALLOCATION FAILED")
                || upper.contains("ERROR (MM_CORE)")
                || upper.contains("USER FATAL MESSAGE")
                || upper.contains("SYSTEM FATAL MESSAGE")
        })
        .take(5)
        .collect()
}

/// Mode frequencies (Hz, the `CYCLES` column) of the first `R E A L   E I G E N
/// V A L U E S` table of an SOL 103 `.f06`, in mode order.
///
/// The `TRIAL EIGENVALUE = ..., CYCLES = ...` lines printed by the Lanczos
/// shifts above that table carry the shift frequencies, not the modes, and their
/// Fortran `D` exponents lose the exponent under a naive float read. A table row
/// is `MODE EXTRACTION_ORDER EIGENVALUE RADIANS CYCLES GEN_MASS GEN_STIFFNESS`;
/// only rows with two integers and five floats are accepted. The table printed
/// after residual-vector augmentation is ignored (it repeats the same modes).
pub fn real_eigenvalue_cycles(content: &str) -> Vec<f64> {
    let mut modes = Vec::new();
    let mut in_table = false;
    for line in super::text::splitlines(content) {
        let spaced: String = line.split_whitespace().collect::<Vec<_>>().join("");
        if spaced.contains("REALEIGENVALUES") {
            if in_table || !modes.is_empty() {
                break;
            }
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let is_row = fields.len() == 7
            && fields[..2].iter().all(|f| f.parse::<u32>().is_ok())
            && fields[2..].iter().all(|f| f.parse::<f64>().is_ok());
        if is_row {
            if let Ok(cycles) = fields[4].parse::<f64>() {
                modes.push(cycles);
            }
        } else if !modes.is_empty() && !line.trim().is_empty() {
            break;
        }
    }
    modes
}

#[cfg(test)]
// Temporary-file setup failures should fail these diagnostic tests immediately.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn allocation_failure_is_retained_even_when_launcher_reports_analysis_complete() {
        let log = "Error (MM_CORE):Exception: malloc failed\nMAINAL: *** OPEN CORE MEMORY ALLOCATION FAILED *** ERROR = 1\nAnalysis complete 8\nMSC Nastran finished";
        assert_eq!(failure_lines(log).len(), 2);
        assert!(failure_lines("Analysis complete 0\nMSC Nastran finished").is_empty());
    }

    #[test]
    fn non_utf8_solver_banner_does_not_hide_runtime_failure() {
        let directory =
            std::env::temp_dir().join(format!("alas_log_encoding_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let deck = directory.join("case.bdf");
        std::fs::write(
            deck.with_extension("log"),
            b"MSC banner \xff\nOPEN CORE ALLOCATION Failed\n",
        )
        .unwrap();
        assert!(runtime_failure(&deck)
            .unwrap()
            .contains("OPEN CORE ALLOCATION Failed"));
        std::fs::remove_file(deck.with_extension("log")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn eigenvalue_table_cycles_are_read_not_the_lanczos_shift_lines() {
        let f06 = "     TRIAL EIGENVALUE = 1.612131D+00, CYCLES = 2.020786D-01 THE STURM COUNT = 0
                                         R E A L   E I G E N V A L U E S
                   (BEFORE AUGMENTATION OF RESIDUAL VECTORS)
                   MODE    EXTRACTION      EIGENVALUE            RADIANS             CYCLES            GENERALIZED         GENERALIZED
                   NO.       ORDER                                                                       MASS              STIFFNESS
                   1         1        7.058296E+01        8.401366E+00        1.337119E+00        1.000000E+00        7.058296E+01
                   2         2        1.468092E+02        1.211648E+01        1.928398E+00        1.000000E+00        1.468092E+02
                   
                   R E A L   E I G E N V A L U E S
                   1         1        7.058296E+01        8.401366E+00        9.999999E+00        1.000000E+00        7.058296E+01
";
        assert_eq!(real_eigenvalue_cycles(f06), vec![1.337119, 1.928398]);
        assert!(real_eigenvalue_cycles("no table").is_empty());
    }
}
